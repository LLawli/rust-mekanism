// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::native_block_entity_draw::{BlockEntityDraw, CubeFace};
use crate::game::{Game, GameLevel, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::crafting::Ingredient;
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
use machinery_basis_core::machine::{self, Step, Supply};
use machinery_basis_core::recipe::{
    Conversion, ConversionFields, ItemResourceFields, ItemResourceRecipe, ResourceInput,
};
use machinery_basis_core::resource::{self, Registry, Tags};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::tank::{Action, FORMAT, Tank};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

const LISTENER: &str = "machinery_basis:resources";
const TAGS_LISTENER: &str = "machinery_basis:resource_tags";
const HOPPER: &str = "minecraft:hopper";
const SAVED: &str = "machinery_basis";
const ENERGY: &str = "energy";
const PROGRESS: &str = "progress";
const RECIPE: &str = "recipe";
const DEFAULT_STACK: u32 = 64;
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
// Os índices de receitas por tipo. Só valem até a próxima carga de dados.
static MACHINE_RECIPES: Mutex<BTreeMap<String, Arc<Vec<MachineRecipe>>>> = Mutex::new(BTreeMap::new());
static CONVERSIONS: Mutex<BTreeMap<String, Arc<Vec<ItemConversion>>>> = Mutex::new(BTreeMap::new());

// A parte da tela que toda máquina tem, nas posições do inventário vanilla.
const INVENTORY_LEFT: f32 = 7.0;
const INVENTORY_TOP: f32 = 83.0;
const HOTBAR_GAP: f32 = 4.0;
const INVENTORY_COLUMNS: usize = 9;
const INVENTORY_ROWS: usize = 3;
const SLOT_SIZE: f32 = 18.0;
const TITLE_TOP: f32 = 6.0;
const LABEL_COLOR: u32 = 0x40_40_40;

/// Os sprites que dão a cara de um mod às telas dele.
pub(in crate::game) struct PanelStyle {
    pub base: &'static str,
    pub base_size: f32,
    pub base_border: f32,
    pub slot: &'static str,
}

/// Uma receita de item e recurso, com o que o jogo resolveu.
pub(in crate::game) struct MachineRecipe {
    pub id: String,
    pub item: Ingredient,
    pub count: u32,
    pub resource: ResourceInput,
    pub output: ItemStack,
}

/// Um item que vira recurso, com o que o jogo resolveu.
pub(in crate::game) struct ItemConversion {
    pub item: Ingredient,
    pub count: u32,
    pub resource: String,
    pub amount: u64,
}

/// O que define uma máquina de item e recurso que dá um item.
pub(in crate::game) struct ItemResourceMachine {
    pub recipe_type: &'static str,
    pub fields: ItemResourceFields,
    pub ticks: u32,
    pub energy_per_tick: i32,
    pub input: usize,
    pub output: usize,
}

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
    listeners.push(NativeReloadListener::json(TAGS_LISTENER, resource::TAGS_FOLDER));
}

pub(in crate::game) fn reload_apply(
    listener: &NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    if listener.id == TAGS_LISTENER {
        resource::set_current_tags(Tags::load(files));
        return;
    }
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

/// Os dados mudaram: os índices de receitas se refazem no próximo uso.
pub(in crate::game) fn data_loaded() {
    MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// As receitas de um tipo. Fica de fora a que cita um item que o jogo não conhece.
pub(in crate::game) fn machine_recipes(
    game: &Game<'_>,
    recipe_type: &str,
    fields: &ItemResourceFields,
) -> Arc<Vec<MachineRecipe>> {
    let mut index = MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(recipes) = index.get(recipe_type) {
        return recipes.clone();
    }
    let recipes = game.native_recipes_of_type(recipe_type).iter().filter_map(|recipe| {
        let parsed = ItemResourceRecipe::parse(&recipe.json, fields)?;
        Some(MachineRecipe {
            id: recipe.id.clone(),
            item: game.native_recipe_ingredient(&parsed.item.ingredient)?,
            count: parsed.item.count,
            resource: parsed.resource,
            output: game.native_recipe_result(&parsed.output)?,
        })
    });
    let recipes = Arc::new(recipes.collect::<Vec<_>>());
    index.insert(recipe_type.to_owned(), recipes.clone());
    recipes
}

/// As conversões de um tipo. Fica de fora a que dá um recurso que ninguém declarou.
pub(in crate::game) fn conversions(
    game: &Game<'_>,
    recipe_type: &str,
    fields: &ConversionFields,
) -> Arc<Vec<ItemConversion>> {
    let mut index = CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(conversions) = index.get(recipe_type) {
        return conversions.clone();
    }
    let registry = resource::current();
    let conversions = game.native_recipes_of_type(recipe_type).iter().filter_map(|recipe| {
        let parsed = Conversion::parse(&recipe.json, fields)?;
        registry.get(&parsed.resource)?;
        Some(ItemConversion {
            item: game.native_recipe_ingredient(&parsed.item.ingredient)?,
            count: parsed.item.count,
            resource: parsed.resource,
            amount: parsed.amount,
        })
    });
    let conversions = Arc::new(conversions.collect::<Vec<_>>());
    index.insert(recipe_type.to_owned(), conversions.clone());
    conversions
}

/// A primeira receita que as entradas pagam por inteiro.
fn matching_recipe<'a>(
    recipes: &'a [MachineRecipe],
    stack: Option<&ItemStack>,
    tank: &Tank,
    tags: &Tags,
) -> Option<&'a MachineRecipe> {
    let stack = stack?;
    let held = tank.resource()?;
    recipes.iter().find(|recipe| {
        recipe.item.test(stack)
            && stack.count >= recipe.count
            && recipe.resource.accepts(held, tags)
            && tank.amount() >= recipe.resource.amount
    })
}

/// Se o item entra em alguma receita, junto do recurso que o tanque guarda, quando guarda.
pub(in crate::game) fn is_recipe_input(
    recipes: &[MachineRecipe],
    stack: &ItemStack,
    held: Option<&str>,
) -> bool {
    let tags = resource::current_tags();
    recipes.iter().any(|recipe| {
        recipe.item.test(stack) && held.is_none_or(|held| recipe.resource.accepts(held, &tags))
    })
}

/// Se o recurso entra em alguma receita, junto do item que está na entrada, quando há um.
pub(in crate::game) fn is_recipe_resource(
    recipes: &[MachineRecipe],
    resource: &str,
    stack: Option<&ItemStack>,
) -> bool {
    let tags = resource::current_tags();
    recipes.iter().any(|recipe| {
        recipe.resource.accepts(resource, &tags) && stack.is_none_or(|stack| recipe.item.test(stack))
    })
}

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.extra == b.extra
}

fn output_fits(game: &Game<'_>, slot: Option<&ItemStack>, output: &ItemStack) -> bool {
    let limit = game.native_max_stack_size(&output.item).unwrap_or(DEFAULT_STACK);
    slot.is_none_or(|held| same_item(held, output) && held.count + output.count <= limit)
}

fn take_from_slot(entity: &mut ModBlockEntity, slot: usize, count: u32) {
    if let Some(stack) = &mut entity.items[slot] {
        stack.count = stack.count.saturating_sub(count);
        if stack.count == 0 {
            entity.items[slot] = None;
        }
    }
}

fn saved_progress(entity: &ModBlockEntity, recipe: &str) -> u32 {
    let saved = saved(entity);
    let same = matches!(
        saved.and_then(|compound| compound.get(&JavaString::from(RECIPE))),
        Some(Tag::String(id)) if id.to_string_lossy() == recipe
    );
    match saved.and_then(|compound| compound.get(&JavaString::from(PROGRESS))) {
        Some(Tag::Int(progress)) if same => u32::try_from(*progress).unwrap_or(0),
        _ => 0,
    }
}

fn save_progress(entity: &mut ModBlockEntity, recipe: &str, progress: u32) {
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.insert(JavaString::from(RECIPE), Tag::String(JavaString::from(recipe)));
    compound.insert(JavaString::from(PROGRESS), Tag::Int(progress as i32));
    entity.nbt.insert(JavaString::from(SAVED), Tag::Compound(compound));
}

/// Um tick de uma máquina de item e recurso. Diz o que houve e em que tick a operação está.
pub(in crate::game) fn run_machine(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    tank: &mut Tank,
    spec: &ItemResourceMachine,
) -> (Step, u32) {
    let recipes = machine_recipes(game, spec.recipe_type, &spec.fields);
    let tags = resource::current_tags();
    let found = matching_recipe(&recipes, entity.items[spec.input].as_ref(), tank, &tags);
    let id = found.map_or("", |recipe| recipe.id.as_str());
    // Outra receita recomeça do zero, como um CachedRecipe novo.
    let mut progress = saved_progress(entity, id);
    let supply = match found {
        None => Supply::Missing,
        Some(recipe) if output_fits(game, entity.items[spec.output].as_ref(), &recipe.output) => {
            Supply::Ready
        }
        Some(_) => Supply::OutputFull,
    };
    let stored = stored_energy(entity);
    let step = machine::step(&mut progress, spec.ticks, supply, stored, spec.energy_per_tick);
    set_stored_energy(entity, stored - step.energy_used);
    if let Some(recipe) = found.filter(|_| step.finished) {
        take_from_slot(entity, spec.input, recipe.count);
        tank.extract(recipe.resource.amount, Action::Execute);
        match &mut entity.items[spec.output] {
            Some(held) => held.count += recipe.output.count,
            empty => *empty = Some(recipe.output.clone()),
        }
    }
    save_progress(entity, id, progress);
    (step, progress)
}

/// `ChemicalInventorySlot.fillTankOrConvert`: um item do slot vira recurso, se todo ele couber.
pub(in crate::game) fn convert_into_tank(
    entity: &mut ModBlockEntity,
    slot: usize,
    tank: &mut Tank,
    conversions: &[ItemConversion],
    accepts: &dyn Fn(&str) -> bool,
) {
    let Some(stack) = &entity.items[slot] else {
        return;
    };
    let found = conversions
        .iter()
        .find(|conversion| conversion.item.test(stack) && stack.count >= conversion.count);
    let Some(conversion) = found else {
        return;
    };
    let room = tank.insert(&conversion.resource, conversion.amount, Action::Simulate);
    if room == conversion.amount && accepts(&conversion.resource) {
        tank.insert(&conversion.resource, conversion.amount, Action::Execute);
        take_from_slot(entity, slot, conversion.count);
    }
}

/// Leva o que couber de `stack` para o slot da máquina: o resto fica em `stack`.
pub(in crate::game) fn move_into_slot(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    slot: usize,
    stack: &mut ItemStack,
) {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(DEFAULT_STACK);
    match &mut entity.items[slot] {
        Some(held) if same_item(held, stack) => {
            let moved = limit.saturating_sub(held.count).min(stack.count);
            held.count += moved;
            stack.count -= moved;
        }
        Some(_) => {}
        empty => {
            let moved = stack.count.min(limit);
            let mut placed = stack.clone();
            placed.count = moved;
            *empty = Some(placed);
            stack.count -= moved;
        }
    }
}

/// A face da frente de uma máquina, pela propriedade `facing`.
pub(in crate::game) fn front(game: &Game<'_>, pos: [i32; 3]) -> Face {
    game.native_block_property(pos, "facing")
        .and_then(|facing| Face::from_property(&facing))
        .unwrap_or(Face::North)
}

pub(in crate::game) fn stored_energy(entity: &ModBlockEntity) -> i32 {
    match entity.nbt.get(&JavaString::from(ENERGY)) {
        Some(Tag::Int(stored)) => *stored,
        _ => 0,
    }
}

pub(in crate::game) fn set_stored_energy(entity: &mut ModBlockEntity, stored: i32) {
    entity.nbt.insert(JavaString::from(ENERGY), Tag::Int(stored));
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
    let start = stored_energy(entity);
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
    set_stored_energy(entity, stored);
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

fn room_in(
    game: &Game<'_>,
    entity: &ModBlockEntity,
    inputs: &[usize],
    stack: &ItemStack,
    accepts: &dyn Fn(&Game<'_>, usize, &ItemStack) -> bool,
) -> Option<usize> {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(DEFAULT_STACK);
    inputs.iter().copied().find(|&slot| {
        accepts(game, slot, stack)
            && match &entity.items[slot] {
                None => true,
                Some(held) => held.item == stack.item && held.count < limit,
            }
    })
}

fn take_one(
    game: &mut Game<'_>,
    hopper_pos: [i32; 3],
    face: Face,
    entity: &mut ModBlockEntity,
    inputs: &[usize],
    accepts: &dyn Fn(&Game<'_>, usize, &ItemStack) -> bool,
) -> bool {
    let reached = direction(face.opposite());
    let dimension = game.native_dimension();
    let Some(slots) = game.native_container_slots(dimension, hopper_pos, reached) else {
        return false;
    };
    // Como o hopper: o primeiro item que o destino aceita, na ordem dos slots.
    let found = slots.iter().find_map(|s| {
        let stack = s.stack.as_ref().filter(|_| s.can_take)?;
        Some((s.slot, room_in(game, entity, inputs, stack, accepts)?))
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
    slots: &dyn Fn(Face) -> FaceSlots,
    accepts: &dyn Fn(&Game<'_>, usize, &ItemStack) -> bool,
) {
    let mut values = cooldowns(entity);
    for face in Face::ALL {
        let mut cooldown = Cooldown {
            ticks: values[face as usize] as u8,
        };
        if cooldown.tick() {
            let hopper_pos = neighbour(pos, face);
            if let Some(neighbour) = hopper_at(game, hopper_pos) {
                let slots = slots(face);
                let moved = match hopper::transfer(face, neighbour, sides) {
                    Some(Transfer::Give) => give_one(game, hopper_pos, entity, slots.outputs),
                    Some(Transfer::Take) => {
                        take_one(game, hopper_pos, face, entity, slots.inputs, accepts)
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
    const BACKGROUND: u32 = 0xFF_08_08_08;
    let [x, y, w, h] = rect;
    game.native_fill(batch, rect, BACKGROUND);
    let filled = h * fraction.clamp(0.0, 1.0);
    game.native_fill(batch, [x, y + h - filled, w, filled], argb);
}

/// Uma barra que enche da esquerda para a direita, com o sprite vazio e o cheio do mesmo tamanho.
pub(in crate::game) fn progress_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    empty: &str,
    full: &str,
    rect: [f32; 4],
    fraction: f32,
) {
    let [x, y, w, h] = rect;
    game.native_sprite(batch, empty, [0.0, 0.0, w, h], rect);
    let filled = w * fraction.clamp(0.0, 1.0);
    game.native_sprite(batch, full, [0.0, 0.0, filled, h], [x, y, filled, h]);
}

/// O fundo de uma tela de máquina: o painel, o título e os slots do inventário do jogador.
pub(in crate::game) fn machine_panel(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &PanelStyle,
    rect: [f32; 4],
    title: &str,
) {
    let [x, y, width, _] = rect;
    nine_slice(game, batch, style.base, style.base_size, style.base_border, rect);
    for row in 0..=INVENTORY_ROWS {
        let gap = if row == INVENTORY_ROWS { HOTBAR_GAP } else { 0.0 };
        let slot_y = y + INVENTORY_TOP + row as f32 * SLOT_SIZE + gap;
        for column in 0..INVENTORY_COLUMNS {
            let slot_x = x + INVENTORY_LEFT + column as f32 * SLOT_SIZE;
            let dst = [slot_x, slot_y, SLOT_SIZE, SLOT_SIZE];
            game.native_sprite(batch, style.slot, [0.0, 0.0, SLOT_SIZE, SLOT_SIZE], dst);
        }
    }
    let title_x = x + (width - game.native_text_width(title)) / 2.0;
    game.native_text(batch, title, [title_x, y + TITLE_TOP], LABEL_COLOR, false);
}
