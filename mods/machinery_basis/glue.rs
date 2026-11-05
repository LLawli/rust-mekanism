// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::native_block_entity_draw::{BlockEntityDraw, CubeFace};
use crate::game::{Game, GameLevel, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;
use horizon_core::position::BlockPos;
use horizon_world::level::Ticks;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::fluid_logged;
use machinery_basis_core::heat;
use machinery_basis_core::hopper::{self, Cooldown, Ejector, Hopper, Transfer};
use machinery_basis_core::resource::{self, Registry};
use machinery_basis_core::sides::{Face, Sides};
use machinery_basis_core::tank::{FORMAT, Tank};
use std::sync::atomic::{AtomicI64, Ordering};

const LISTENER: &str = "machinery_basis:resources";
const HOPPER: &str = "minecraft:hopper";
const SAVED: &str = "machinery_basis";
const ENERGY: &str = "energy";
// Um contador por face, mais um para o ejetor.
const COOLDOWNS: usize = 7;
const EJECT_COOLDOWN: usize = 6;
const LAVA: &str = "minecraft:lava";
const LAVA_SPRITE: &str = "block/lava_still";
// A lava dentro do bloco, um pouco para dentro das faces do modelo, na altura de uma fonte. Em
// unidades de modelo, de 0 a 16.
const LOGGED_LAVA_FROM: [f32; 3] = [0.03, 0.03, 0.03];
const LOGGED_LAVA_TO: [f32; 3] = [15.97, 14.2, 15.97];
const FULL_BRIGHT: u32 = 0xF0F0;
const WHITE: u32 = 0xFF_FF_FF;
/// `LavaFluid.getTickDelay` fora do Nether.
const LAVA_TICK_DELAY: i32 = 30;
const CREATIVE: i32 = 1;
// O tick da última troca de balde, e por quantos ticks o uso do item que vem junto é ignorado.
const BUCKET_SWAP_TICKS: i64 = 2;
static LAST_BUCKET_SWAP: AtomicI64 = AtomicI64::new(i64::MIN / 2);

/// Se o bloco em `pos` guarda lava no estado `fluid_logged`.
pub(in crate::game) fn has_logged_lava(game: &Game<'_>, pos: [i32; 3]) -> bool {
    game.native_block_property(pos, fluid_logged::PROPERTY).as_deref() == Some(fluid_logged::LAVA)
}

/// Põe ou tira a lava do bloco. Posta, ela começa a correr como a água de um bloco alagado.
fn set_logged_lava(game: &mut Game<'_>, pos: [i32; 3], logged: bool) {
    let value = if logged {
        fluid_logged::LAVA
    } else {
        fluid_logged::EMPTY
    };
    game.native_set_block_property(pos, fluid_logged::PROPERTY, value);
    if logged {
        game.with_level(|_, level, ticks| {
            level.native_schedule_fluid_tick(ticks, pos, LAVA, LAVA_TICK_DELAY)
        });
    }
}

/// Um balde coloca ou tira a lava de um bloco com `fluid_logged`, como água numa escada. Um bloco
/// com tela só chega aqui com o jogador agachado: em pé, o clique abre a tela.
pub(in crate::game) fn use_item_on(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    if *claimed || game.native_block_property(pos, fluid_logged::PROPERTY).is_none() {
        return;
    }
    let Some(swap) = fluid_logged::bucket_swap(&stack.item, has_logged_lava(game, pos)) else {
        return;
    };
    set_logged_lava(game, pos, swap.logged);
    LAST_BUCKET_SWAP.store(game.world.game_time, Ordering::Relaxed);
    let creative = game
        .native_acting_player()
        .is_some_and(|player| player.game_mode == CREATIVE);
    if !creative && game.native_take_items(swap.take, 1) == 1 {
        game.native_give_items(swap.give, 1);
    }
    *claimed = true;
}

/// O mesmo clique ainda chega como uso do item, já com o balde cheio na mão: sem isto, ele despeja
/// a lava que acabou de sair do bloco.
pub(in crate::game) fn use_item(game: &Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    let since_swap = game.world.game_time - LAST_BUCKET_SWAP.load(Ordering::Relaxed);
    if &*stack.item == fluid_logged::LAVA_BUCKET && (0..=BUCKET_SWAP_TICKS).contains(&since_swap) {
        *claimed = true;
    }
}

/// O mesher lê o fluido de `FluidState::from_block`, que nenhuma injeção alcança nesta versão. Quem
/// desenha é a block entity, e o bloco precisa mandar uma cópia dela ao cliente.
pub(in crate::game) fn submit(game: &Game<'_>, pos: [i32; 3], draw: &mut BlockEntityDraw) {
    if !has_logged_lava(game, pos) {
        return;
    }
    let face = Some(CubeFace {
        sprite: LAVA_SPRITE,
        uv: None,
    });
    draw.light = FULL_BRIGHT;
    game.native_draw_cube(draw, LOGGED_LAVA_FROM, LOGGED_LAVA_TO, [face; 6], WHITE);
}

/// Um vizinho mudou: a lava do bloco olha de novo para onde pode correr.
pub(in crate::game) fn update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
) {
    let logged = level.registry.state(state).is_some_and(|s| {
        s.properties.get(fluid_logged::PROPERTY).map(String::as_str) == Some(fluid_logged::LAVA)
    });
    if logged {
        level.native_schedule_fluid_tick(ticks, [pos.x, pos.y, pos.z], LAVA, LAVA_TICK_DELAY);
    }
}

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
    accepts: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
) -> bool {
    let reached = direction(face.opposite());
    let dimension = game.native_dimension();
    let Some(slots) = game.native_container_slots(dimension, hopper_pos, reached) else {
        return false;
    };
    // Como o hopper: o primeiro item que o destino aceita, na ordem dos slots.
    let found = slots.iter().find_map(|s| {
        let stack = s.stack.as_ref().filter(|stack| s.can_take && accepts(game, stack))?;
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
    accepts: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
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

/// A temperatura do ar numa posição. Usa a temperatura base do bioma, sem o ajuste por altura.
pub(in crate::game) fn ambient_temperature(game: &Game<'_>, pos: [i32; 3]) -> f64 {
    let [x, y, z] = pos;
    let biome = game.world.chunks.get(&(x >> 4, z >> 4)).map(|chunk| {
        world_biomes!(game).list[chunk.biome(x, y, z) as usize].temperature
    });
    biome.map_or(heat::AMBIENT_TEMP, |temperature| heat::ambient(f64::from(temperature)))
}

/// Um painel de nove partes: cantos fixos e bordas e miolo esticados, de um sprite `size` x `size`.
pub(in crate::game) fn nine_slice(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    size: f32,
    border: f32,
    rect: [f32; 4],
) {
    let [x, y, w, h] = rect;
    let inner = size - 2.0 * border;
    let columns = [
        (0.0, border, x, border),
        (border, inner, x + border, w - 2.0 * border),
        (size - border, border, x + w - border, border),
    ];
    let rows = [
        (0.0, border, y, border),
        (border, inner, y + border, h - 2.0 * border),
        (size - border, border, y + h - border, border),
    ];
    for (src_y, src_h, dst_y, dst_h) in rows {
        for (src_x, src_w, dst_x, dst_w) in columns {
            game.native_sprite(batch, sprite, [src_x, src_y, src_w, src_h], [dst_x, dst_y, dst_w, dst_h]);
        }
    }
}

/// Um medidor que enche de baixo para cima, sobre um fundo escuro.
pub(in crate::game) fn gauge(
    game: &Game<'_>,
    batch: &mut HudBatch,
    rect: [f32; 4],
    fraction: f32,
    argb: u32,
) {
    const BACKGROUND: u32 = 0xFF_1E_1E_1E;
    let [x, y, w, h] = rect;
    game.native_fill(batch, rect, BACKGROUND);
    let filled = h * fraction.clamp(0.0, 1.0);
    game.native_fill(batch, [x, y + h - filled, w, filled], argb);
}
