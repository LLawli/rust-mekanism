// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::{Game, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::hopper::{self, Cooldown, Ejector, Hopper, Transfer};
use machinery_basis_core::resource::{self, Registry};
use machinery_basis_core::sides::{Face, Sides};
use machinery_basis_core::tank::{FORMAT, Tank};

const LISTENER: &str = "machinery_basis:resources";
const HOPPER: &str = "minecraft:hopper";
const SAVED: &str = "machinery_basis";
const ENERGY: &str = "energy";
// Um contador por face, mais um para o ejetor.
const COOLDOWNS: usize = 7;
const EJECT_COOLDOWN: usize = 6;

pub(in crate::game) fn reload_listeners(listeners: &mut Vec<NativeReloadListener>) {
    listeners.push(NativeReloadListener::json(LISTENER, resource::DATA_FOLDER));
}

pub(in crate::game) fn reload_apply(
    listener: &NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    if listener.id != LISTENER {
        return;
    }
    let (registry, errors) = Registry::load(files);
    for (id, error) in errors {
        eprintln!("machinery_basis: recurso {id} recusado: {error}");
    }
    eprintln!("machinery_basis: {} recursos", registry.len());
    resource::set_current(registry);
}

pub(in crate::game) fn direction(face: Face) -> Direction {
    match face {
        Face::Down => Direction::Down,
        Face::Up => Direction::Up,
        Face::North => Direction::North,
        Face::South => Direction::South,
        Face::West => Direction::West,
        Face::East => Direction::East,
    }
}

fn neighbour(pos: [i32; 3], face: Face) -> [i32; 3] {
    let [x, y, z] = face.offset();
    [pos[0] + x, pos[1] + y, pos[2] + z]
}

fn saved(entity: &ModBlockEntity) -> Option<&Compound> {
    match entity.nbt.get(&JavaString::from(SAVED)) {
        Some(Tag::Compound(compound)) => Some(compound),
        _ => None,
    }
}

/// Os tanques salvos da block entity, um por capacidade. Sem dados, ou num formato que esta
/// versão não conhece, vêm vazios.
pub(in crate::game) fn load_tanks(entity: &ModBlockEntity, capacities: &[u64]) -> Vec<Tank> {
    let list = saved(entity).and_then(|compound| {
        let version = compound.get(&JavaString::from("v"));
        if !matches!(version, Some(Tag::Int(v)) if *v == FORMAT) {
            return None;
        }
        match compound.get(&JavaString::from("tanks")) {
            Some(Tag::List(list)) => Some(list),
            _ => None,
        }
    });
    capacities
        .iter()
        .enumerate()
        .map(|(i, &capacity)| {
            let Some(Tag::Compound(tank)) = list.and_then(|list| list.get(i)) else {
                return Tank::new(capacity);
            };
            let resource = match tank.get(&JavaString::from("resource")) {
                Some(Tag::String(id)) => Some(id.to_string_lossy()),
                _ => None,
            };
            let amount = match tank.get(&JavaString::from("amount")) {
                Some(Tag::Long(amount)) => u64::try_from(*amount).unwrap_or(0),
                _ => 0,
            };
            Tank::restored(resource, amount, capacity)
        })
        .collect()
}

pub(in crate::game) fn save_tanks(entity: &mut ModBlockEntity, tanks: &[Tank]) {
    let list = tanks
        .iter()
        .map(|tank| {
            let mut compound = Compound::default();
            if let Some(resource) = tank.resource() {
                compound.insert(
                    JavaString::from("resource"),
                    Tag::String(JavaString::from(resource)),
                );
            }
            let amount = i64::try_from(tank.amount()).unwrap_or(i64::MAX);
            compound.insert(JavaString::from("amount"), Tag::Long(amount));
            Tag::Compound(compound)
        })
        .collect();
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.insert(JavaString::from("v"), Tag::Int(FORMAT));
    compound.insert(JavaString::from("tanks"), Tag::List(list));
    entity.nbt.insert(JavaString::from(SAVED), Tag::Compound(compound));
}

fn cooldowns(entity: &ModBlockEntity) -> Vec<i32> {
    match saved(entity).and_then(|compound| compound.get(&JavaString::from("cooldowns"))) {
        Some(Tag::IntArray(values)) if values.len() == COOLDOWNS => values.clone(),
        _ => vec![0; COOLDOWNS],
    }
}

fn save_cooldowns(entity: &mut ModBlockEntity, values: Vec<i32>) {
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.insert(JavaString::from("cooldowns"), Tag::IntArray(values));
    entity.nbt.insert(JavaString::from(SAVED), Tag::Compound(compound));
}

/// Empurra a energia do bloco em tick para os vizinhos das faces de saída. O bloco em tick está
/// fora do mundo, então lê e grava o próprio NBT em vez de usar `native_move_energy`.
pub(in crate::game) fn distribute_energy(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    sides: &Sides,
) -> i32 {
    let key = JavaString::from(ENERGY);
    let start = match entity.nbt.get(&key) {
        Some(Tag::Int(stored)) => *stored,
        _ => 0,
    };
    let mut stored = start;
    for face in Face::ALL {
        if stored == 0 {
            break;
        }
        if sides.gives_output(face) {
            let reached = direction(face.opposite());
            stored -= game.native_energy_insert(neighbour(pos, face), reached, stored, false);
        }
    }
    entity.nbt.insert(key, Tag::Int(stored));
    start - stored
}

fn hopper_at(game: &Game<'_>, pos: [i32; 3]) -> Option<Hopper> {
    if game.native_block(pos) != HOPPER {
        return None;
    }
    let facing = game.native_block_property(pos, "facing")?;
    Some(Hopper {
        facing: Face::from_property(&facing)?,
        enabled: game.native_block_property(pos, "enabled").as_deref() != Some("false"),
    })
}

fn give_one(
    game: &mut Game<'_>,
    hopper_pos: [i32; 3],
    entity: &mut ModBlockEntity,
    outputs: &[usize],
) -> bool {
    for &slot in outputs {
        let Some(stack) = entity.items[slot].clone() else {
            continue;
        };
        let mut one = stack.clone();
        one.count = 1;
        if game.native_insert_item(hopper_pos, Direction::Up, one).is_some() {
            return false;
        }
        let mut left = stack;
        left.count -= 1;
        entity.items[slot] = (left.count > 0).then_some(left);
        return true;
    }
    false
}

fn room_in(game: &Game<'_>, entity: &ModBlockEntity, inputs: &[usize], item: &str) -> Option<usize> {
    let limit = game.native_max_stack_size(item).unwrap_or(64);
    inputs.iter().copied().find(|&slot| match &entity.items[slot] {
        None => true,
        Some(stack) => &*stack.item == item && stack.count < limit,
    })
}

fn take_one(
    game: &mut Game<'_>,
    hopper_pos: [i32; 3],
    face: Face,
    entity: &mut ModBlockEntity,
    inputs: &[usize],
    accepts: &dyn Fn(&ItemStack) -> bool,
) -> bool {
    let reached = direction(face.opposite());
    let dimension = game.native_dimension();
    let Some(slots) = game.native_container_slots(dimension, hopper_pos, reached) else {
        return false;
    };
    // Como o hopper: o primeiro item que o destino aceita, na ordem dos slots.
    let found = slots.iter().find_map(|s| {
        let stack = s.stack.as_ref().filter(|stack| s.can_take && accepts(stack))?;
        Some((s.slot, room_in(game, entity, inputs, &stack.item)?))
    });
    let Some((from, into)) = found else {
        return false;
    };
    let Some(one) = game.native_extract_item(dimension, hopper_pos, reached, from, 1) else {
        return false;
    };
    match &mut entity.items[into] {
        Some(stack) => stack.count += one.count,
        empty => *empty = Some(one),
    }
    true
}

/// A regra dos hoppers vizinhos, a cada tick: veja `machinery_basis_core::hopper`. A máquina
/// declara `inputs` e `outputs` vazios no seu `ModContainer`.
pub(in crate::game) fn tick_hoppers(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    sides: &Sides,
    inputs: &[usize],
    outputs: &[usize],
    accepts: &dyn Fn(&ItemStack) -> bool,
) {
    let mut values = cooldowns(entity);
    for face in Face::ALL {
        let mut cooldown = Cooldown {
            ticks: values[face as usize] as u8,
        };
        if cooldown.tick() {
            let hopper_pos = neighbour(pos, face);
            if let Some(neighbour) = hopper_at(game, hopper_pos) {
                let moved = match hopper::transfer(face, neighbour, sides) {
                    Some(Transfer::Give) => give_one(game, hopper_pos, entity, outputs),
                    Some(Transfer::Take) => {
                        take_one(game, hopper_pos, face, entity, inputs, accepts)
                    }
                    None => false,
                };
                if moved {
                    cooldown.moved();
                }
            }
        }
        values[face as usize] = i32::from(cooldown.ticks);
    }
    save_cooldowns(entity, values);
}

/// O push: entrega os itens de saída a qualquer inventário vizinho que os aceite, pelas faces de
/// saída, como um dropper atirando num hopper. O ritmo é o de `Ejector`.
pub(in crate::game) fn eject_items(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    sides: &Sides,
    outputs: &[usize],
) {
    let mut values = cooldowns(entity);
    let mut ejector = Ejector {
        delay: values[EJECT_COOLDOWN] as u8,
    };
    if ejector.tick() {
        for face in Face::ALL {
            if !sides.gives_output(face) {
                continue;
            }
            let reached = direction(face.opposite());
            for &slot in outputs {
                if let Some(stack) = entity.items[slot].take() {
                    entity.items[slot] = game.native_insert_item(neighbour(pos, face), reached, stack);
                }
            }
        }
    }
    values[EJECT_COOLDOWN] = i32::from(ejector.delay);
    save_cooldowns(entity, values);
}
