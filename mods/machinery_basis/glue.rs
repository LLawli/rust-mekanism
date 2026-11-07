// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::native_block_entity_draw::{BlockEntityDraw, CubeFace};
use crate::game::{Game, GameLevel, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::crafting::{Cooking, Ingredient};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;
use horizon_core::position::BlockPos;
use horizon_world::level::Ticks;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::fluid_logged;
use machinery_basis_core::gui::{self, NineSlice, TilingDirection};
use machinery_basis_core::tooltip;
use machinery_basis_core::heat;
use machinery_basis_core::hopper::{self, Cooldown, Ejector, Hopper, Transfer};
use machinery_basis_core::network::{self, EnergyBlocks, Link};
use machinery_basis_core::recipe::{
    Conversion, ConversionFields, MachineRecipeData, RecipeFields, ResourceInput,
};
use machinery_basis_core::resource::{self, Registry, Tags};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::tank::{FORMAT, Tank};
use machinery_basis_core::upgrade::{self, UpgradeTags, UpgradeTypes};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

const LISTENER: &str = "machinery_basis:resources";
const TAGS_LISTENER: &str = "machinery_basis:resource_tags";
const ENERGY_LISTENER: &str = "machinery_basis:energy";
const UPGRADES_LISTENER: &str = "machinery_basis:upgrades";
const UPGRADE_TAGS_LISTENER: &str = "machinery_basis:upgrade_tags";
const HOPPER: &str = "minecraft:hopper";
const SAVED: &str = "machinery_basis";
const ENERGY: &str = "energy";
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
/// Sobe a cada carga de dados (`data_loaded`). Os índices de receitas de cada thread (`MekanismRecipeType.getInputCache`) guardam a
/// carga de onde saíram e se limpam (`clear`) quando ela muda.
static DATA_GENERATION: AtomicU64 = AtomicU64::new(0);
// A travessia de uma rede custa 1 ms a cada mil transmissores. Guardada por um segundo, uma mudança
// demora até isso para valer, e nada se perde: a energia só espera no transmissor.
const NETWORK_REFRESH_TICKS: i64 = 20;
type NetworkWalk = (i64, Arc<Vec<([i32; 3], Face)>>);
static NETWORK_WALKS: LazyLock<Mutex<HashMap<(&'static str, [i32; 3]), NetworkWalk>>> =
    LazyLock::new(Default::default);

// A parte da tela que toda máquina tem, nas posições do inventário vanilla.
const INVENTORY_LEFT: f32 = 7.0;
const INVENTORY_TOP: f32 = 83.0;
const HOTBAR_GAP: f32 = 4.0;
const INVENTORY_COLUMNS: usize = 9;
const INVENTORY_ROWS: usize = 3;

/// Os sprites que dão a cara de um mod às telas dele.
pub(in crate::game) struct PanelStyle {
    /// O painel de fundo (`GuiMekanism.BASE_BACKGROUND_SLICE`) e o `gui.scaling` do `.mcmeta` dele.
    pub base: &'static str,
    pub base_slice: NineSlice,
    /// A moldura dos slots do inventário do jogador (`SlotType.NORMAL`).
    pub slot: &'static str,
}

/// Uma receita de máquina, com o que o jogo resolveu.
pub(in crate::game) struct MachineRecipe {
    pub id: String,
    pub item: Ingredient,
    pub count: u32,
    pub resource: Option<ResourceInput>,
    pub output: ItemStack,
    /// `per_tick_usage`: a receita gasta o recurso a cada tick, e não de uma vez no fim.
    pub per_tick_usage: bool,
}

/// Um item que vira recurso, com o que o jogo resolveu.
pub(in crate::game) struct ItemConversion {
    pub item: Ingredient,
    pub count: u32,
    pub resource: String,
    pub amount: u64,
}

/// De onde vêm as receitas de uma máquina.
#[derive(Clone, Copy)]
pub(in crate::game) enum RecipeSource {
    /// Arquivos de um tipo próprio.
    Custom(&'static str, RecipeFields),
    /// As receitas de cozimento do jogo, de um tipo como `smelting`.
    Cooking(&'static str),
}

impl RecipeSource {
    /// O tipo de receita de onde as da máquina saem: um índice de receitas por tipo.
    pub(in crate::game) fn key(&self) -> &'static str {
        match self {
            Self::Custom(recipe_type, _) | Self::Cooking(recipe_type) => recipe_type,
        }
    }
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

fn face_of(direction: Direction) -> Face {
    match direction {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}

/// Se um transmissor se liga ao bloco `neighbour` pela face `face` dele.
fn joins(level: &GameLevel<'_>, energy: &EnergyBlocks, neighbour: BlockStateId, face: Face) -> bool {
    level.registry.state(neighbour).is_some_and(|state| {
        let facing = state.properties.get("facing");
        let front = facing.and_then(|facing| Face::from_property(facing));
        energy.joins(&state.name, front.unwrap_or(Face::North), face)
    })
}

fn with_side(level: &GameLevel<'_>, state: BlockStateId, side: Face, joined: bool) -> BlockStateId {
    let value = if joined { "true" } else { "false" };
    level.native_state_with_property(state, side.name(), value).unwrap_or(state)
}

fn is_transmitter_state(level: &GameLevel<'_>, energy: &EnergyBlocks, state: BlockStateId) -> bool {
    level.native_state_block(state).is_some_and(|block| energy.is_transmitter(block))
}

/// O transmissor sendo colocado: cada lado se liga só ao que troca energia por aquela face.
pub(in crate::game) fn state_for_placement(
    level: &GameLevel<'_>,
    state: BlockStateId,
    pos: BlockPos,
    answer: &mut Option<Option<BlockStateId>>,
) {
    let energy = network::current();
    if !is_transmitter_state(level, &energy, state) {
        return;
    }
    let placed = Face::ALL.iter().fold(state, |placed, &side| {
        let [x, y, z] = neighbour([pos.x, pos.y, pos.z], side);
        let joined = joins(level, &energy, level.world.block(x, y, z), side.opposite());
        with_side(level, placed, side, joined)
    });
    *answer = Some(Some(placed));
}

/// Um vizinho mudou: o transmissor refaz a ligação daquele lado, e a lava do bloco olha de novo
/// para onde pode correr.
pub(in crate::game) fn update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    direction: Direction,
    around: BlockStateId,
    new: &mut Option<BlockStateId>,
) {
    let energy = network::current();
    if is_transmitter_state(level, &energy, state) {
        let side = face_of(direction);
        let joined = joins(level, &energy, around, side.opposite());
        *new = Some(with_side(level, state, side, joined));
        return;
    }
    let logged = level.registry.state(state).is_some_and(|s| {
        s.properties.get(fluid_logged::PROPERTY).map(String::as_str) == Some(fluid_logged::LAVA)
    });
    if logged {
        level.native_schedule_fluid_tick(ticks, [pos.x, pos.y, pos.z], LAVA, LAVA_TICK_DELAY);
    }
}

/// O nome das pastas da própria lib, entre as fontes de dados.
const BASIS_SOURCE: &str = "machinery_basis";

pub(in crate::game) fn reload_listeners(listeners: &mut Vec<NativeReloadListener>) {
    listeners.push(NativeReloadListener::json(LISTENER, resource::DATA_FOLDER));
    listeners.push(NativeReloadListener::json(TAGS_LISTENER, resource::TAGS_FOLDER));
    listeners.push(NativeReloadListener::json(ENERGY_LISTENER, network::ENERGY_FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADES_LISTENER, upgrade::TYPES_FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADE_TAGS_LISTENER, upgrade::TAGS_FOLDER));
}

pub(in crate::game) fn reload_apply(
    listener: &NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    if listener.id == TAGS_LISTENER {
        resource::set_tags_source(BASIS_SOURCE, Tags::load(files));
        return;
    }
    if listener.id == ENERGY_LISTENER {
        network::set_current(EnergyBlocks::load(files));
        return;
    }
    if listener.id == UPGRADE_TAGS_LISTENER {
        upgrade::set_tags_source(BASIS_SOURCE, UpgradeTags::load(files));
        return;
    }
    if listener.id == UPGRADES_LISTENER {
        let (types, errors) = UpgradeTypes::load(files);
        for (id, error) in errors {
            eprintln!("machinery_basis: upgrade {id} recusado: {error}");
        }
        eprintln!("machinery_basis: {} upgrades", types.len());
        upgrade::set_types_source(BASIS_SOURCE, types);
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
    resource::set_source(BASIS_SOURCE, registry);
}

/// A carga de dados em uso: muda a cada `data_loaded`.
pub(in crate::game) fn data_generation() -> u64 {
    DATA_GENERATION.load(Ordering::Acquire)
}

/// Os dados mudaram (`MekanismRecipeType.clearCache`): os índices de receitas se limpam e se refazem no próximo uso.
pub(in crate::game) fn data_loaded() {
    DATA_GENERATION.fetch_add(1, Ordering::AcqRel);
    MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    NETWORK_WALKS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn custom_recipes(game: &Game<'_>, recipe_type: &str, fields: &RecipeFields) -> Vec<MachineRecipe> {
    let recipes = game.native_recipes_of_type(recipe_type).iter().filter_map(|recipe| {
        let parsed = MachineRecipeData::parse(&recipe.json, fields)?;
        Some(MachineRecipe {
            id: recipe.id.clone(),
            item: game.native_recipe_ingredient(&parsed.item.ingredient)?,
            count: parsed.item.count,
            resource: parsed.resource,
            output: game.native_recipe_result(&parsed.output)?,
            per_tick_usage: parsed.per_tick_usage,
        })
    });
    recipes.collect()
}

fn cooking_recipes(game: &Game<'_>, kind: &str) -> Vec<MachineRecipe> {
    let of_kind = |recipe: &&Cooking| recipe.kind.strip_prefix("minecraft:").unwrap_or(&recipe.kind) == kind;
    let cooking = game.recipes().cooking.iter().filter(of_kind);
    cooking
        .map(|recipe| MachineRecipe {
            id: recipe.id.clone(),
            item: recipe.ingredient.clone(),
            count: 1,
            resource: None,
            output: recipe.result.clone(),
            per_tick_usage: false,
        })
        .collect()
}

/// As receitas de uma máquina. Fica de fora a que cita um item que o jogo não conhece.
pub(in crate::game) fn machine_recipes(
    game: &Game<'_>,
    source: &RecipeSource,
) -> Arc<Vec<MachineRecipe>> {
    let mut index = MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(recipes) = index.get(source.key()) {
        return recipes.clone();
    }
    let recipes = Arc::new(match source {
        RecipeSource::Custom(recipe_type, fields) => custom_recipes(game, recipe_type, fields),
        RecipeSource::Cooking(kind) => cooking_recipes(game, kind),
    });
    index.insert(source.key().to_owned(), recipes.clone());
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

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.item == b.item && a.extra == b.extra
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

/// A capacidade de energia de cada máquina em jogo, por instância: o `ModEnergyBuffer` do bloco é um só para todos, e a
/// capacidade de uma máquina muda com os upgrades (`MachineEnergyContainer.setMaxEnergy`). O valor guarda o id do bloco, para
/// uma capacidade velha não valer para outro bloco que tome o lugar.
static ENERGY_CAPACITIES: Mutex<BTreeMap<(&'static str, [i32; 3]), (String, i32)>> =
    Mutex::new(BTreeMap::new());

/// Uma capacidade em `i64` como o `ModEnergyBuffer` a guarda, em `i32`. O que passa de `i32::MAX` fica em `i32::MAX`, e isso é
/// dito no log: o tipo do Horizon é o limite, e nada o corta sem aviso.
pub(in crate::game) fn energy_as_int(value: i64, what: &str) -> i32 {
    match i32::try_from(value) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("machinery_basis: {what} {value} passa do `i32` do Horizon e fica em {}", i32::MAX);
            i32::MAX
        }
    }
}

/// Dá à máquina em `pos` a capacidade que ela tem agora. Quem faz a máquina andar chama quando ela muda.
pub(in crate::game) fn set_energy_capacity(
    dimension: &'static str,
    pos: [i32; 3],
    block: &str,
    capacity: i32,
) {
    let mut capacities = ENERGY_CAPACITIES.lock().unwrap_or_else(|e| e.into_inner());
    capacities.insert((dimension, pos), (block.to_owned(), capacity));
}

/// Esquece as capacidades das máquinas que já não andam: a tabela não cresce com os blocos quebrados.
pub(in crate::game) fn retain_energy_capacities(keep: impl Fn(&'static str, [i32; 3]) -> bool) {
    let mut capacities = ENERGY_CAPACITIES.lock().unwrap_or_else(|e| e.into_inner());
    capacities.retain(|(dimension, pos), _| keep(*dimension, *pos));
}

/// Capacidade por instância: a máquina com uma capacidade guardada responde com ela no lugar da do bloco. Sem uma, ou com outro
/// bloco no lugar, segue o caminho normal do Horizon.
pub(in crate::game) fn energy_capacity(
    game: &Game<'_>,
    pos: [i32; 3],
) -> std::ops::ControlFlow<Option<(Arc<horizon_client::containers::ModEnergyBuffer>, i32)>> {
    use horizon_client::containers::{BlockEntity, ModEnergyBuffer};
    use std::ops::ControlFlow::{Break, Continue};
    let key = (pos[0], pos[1], pos[2]);
    let Some((block, capacity)) = ENERGY_CAPACITIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(game.native_dimension(), pos))
        .cloned()
    else {
        return Continue(());
    };
    let state = game.world.block(pos[0], pos[1], pos[2]);
    let Some(buffer) = game.world.block_entities.mod_energy(state.0) else {
        return Continue(());
    };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&key) else {
        return Continue(());
    };
    if entity.id.is_empty() || entity.id != block {
        return Continue(());
    }
    let resized = ModEnergyBuffer {
        capacity,
        max_insert: capacity,
        ..**buffer
    };
    Break(Some((Arc::new(resized), stored_energy(entity))))
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

/// Tira os tanques salvos da block entity, para um mod que guarda os seus em outro formato. Os outros dados da lib no mesmo
/// compound (os cooldowns dos hoppers) ficam.
pub(in crate::game) fn discard_tanks(entity: &mut ModBlockEntity) {
    let has_tanks = saved(entity).is_some_and(|compound| {
        compound.contains_key(&JavaString::from("v")) || compound.contains_key(&JavaString::from("tanks"))
    });
    if !has_tanks {
        return;
    }
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.remove(&JavaString::from("v"));
    compound.remove(&JavaString::from("tanks"));
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

/// Empurra a energia do bloco em tick pelas faces de saída que ele declarou, para os vizinhos que
/// aceitam por aquela face. O bloco em tick está fora do mundo, então lê e grava o próprio NBT em
/// vez de usar `native_move_energy`.
pub(in crate::game) fn distribute_energy(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
) -> i32 {
    let energy = network::current();
    let sides = energy.sides(&entity.id, front(game, pos));
    let start = stored_energy(entity);
    let mut stored = start;
    for face in Face::ALL {
        if stored == 0 {
            break;
        }
        let at = neighbour(pos, face);
        let block = game.native_block(at);
        if sides.gives_output(face) && energy.receives(&block, front(game, at), face.opposite()) {
            let reached = direction(face.opposite());
            stored -= game.native_energy_insert(at, reached, stored, false);
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
    can_give: &dyn Fn(&Game<'_>, usize, &ItemStack) -> bool,
) -> bool {
    for &slot in outputs {
        let Some(stack) = entity.items[slot].clone() else {
            continue;
        };
        // O `canExtract` do slot, com quem está de fora (`AutomationType.EXTERNAL`): o slot que recusa o item não o entrega.
        if !can_give(game, slot, &stack) {
            continue;
        }
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
    can_give: &dyn Fn(&Game<'_>, usize, &ItemStack) -> bool,
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
                    Some(Transfer::Give) => {
                        give_one(game, hopper_pos, entity, slots.outputs, can_give)
                    }
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

/// O que cada lado do transmissor em `pos` alcança. Uma saída de outro bloco liga o cabo, mas não
/// é consumidor: a rede só entrega por uma face de entrada.
fn links(game: &Game<'_>, pos: [i32; 3], energy: &EnergyBlocks) -> [Option<Link>; 6] {
    Face::ALL.map(|face| {
        if game.native_block_property(pos, face.name()).as_deref() != Some("true") {
            return None;
        }
        let at = neighbour(pos, face);
        let block = game.native_block(at);
        if energy.is_transmitter(&block) {
            Some(Link::Transmitter)
        } else {
            energy.receives(&block, front(game, at), face.opposite()).then_some(Link::Acceptor)
        }
    })
}

/// Quem a rede alcança a partir de `pos`, numa travessia guardada por `NETWORK_REFRESH_TICKS`.
fn network_acceptors(game: &Game<'_>, pos: [i32; 3]) -> Arc<Vec<([i32; 3], Face)>> {
    let now = game.world.game_time;
    let key = (game.native_dimension(), pos);
    let mut walks = NETWORK_WALKS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((expires, acceptors)) = walks.get(&key)
        && now < *expires
    {
        return acceptors.clone();
    }
    walks.retain(|_, (expires, _)| now < *expires);
    let energy = network::current();
    let acceptors = Arc::new(network::acceptors(pos, |at| links(game, at, &energy)));
    walks.insert(key, (now + NETWORK_REFRESH_TICKS, acceptors.clone()));
    acceptors
}

/// Um tick de um transmissor de energia: o que ele guarda vai para os consumidores que a rede
/// alcança, divididos por igual. As ligações são as propriedades de lado do bloco.
pub(in crate::game) fn transmit_energy(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
) {
    let stored = stored_energy(entity);
    if stored == 0 {
        return;
    }
    let acceptors = network_acceptors(game, pos);
    let wanted: Vec<i32> = acceptors
        .iter()
        .map(|&(at, face)| game.native_energy_insert(at, direction(face), stored, true))
        .collect();
    let shares = network::split_fairly(stored, &wanted);
    let mut sent = 0;
    for (&(at, face), share) in acceptors.iter().zip(shares) {
        if share > 0 {
            sent += game.native_energy_insert(at, direction(face), share, false);
        }
    }
    set_stored_energy(entity, stored - sent);
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

/// O `ARGB` de uma cor do Java na ordem dos vértices do HUD (`hud::rgba`).
fn hud_color(argb: u32) -> u32 {
    let [a, r, g, b] = argb.to_be_bytes();
    horizon_client::hud::rgba(r, g, b, a)
}

/// Avisa o renderer do Horizon de que uma tela de mod desenha este sprite, para o `animate_sprites` dele misturar os quadros
/// interpolados como o original (injeção `sprite_ties`).
fn register_animated(game: &Game<'_>, rect: &horizon_client::hud::Rect) {
    horizon_client::hud::machinery_sprite_ties::register(game.gui.atlas.revision, rect);
}

/// Parte `src` do sprite `name` (em pixels dele) desenhada em `dst` multiplicada por `argb`, como `native_sprite` mas com a cor
/// (`GuiGraphics.blitSprite(..., color)`). Falso se o sprite não existe.
pub(in crate::game) fn sprite_tinted(
    game: &Game<'_>,
    batch: &mut HudBatch,
    name: &str,
    src: [f32; 4],
    dst: [f32; 4],
    argb: u32,
) -> bool {
    let Some(mut r) = game.gui.atlas.sprites.get(name).copied() else {
        return false;
    };
    register_animated(game, &r);
    let (du, dv) = ((r.u1 - r.u0) / r.w as f32, (r.v1 - r.v0) / r.h as f32);
    r.u0 += src[0] * du;
    r.v0 += src[1] * dv;
    r.u1 = r.u0 + src[2] * du;
    r.v1 = r.v0 + src[3] * dv;
    batch.rect_uv(dst[0], dst[1], dst[2], dst[3], r, hud_color(argb));
    true
}

/// Um sprite de nove partes sobre `rect` (`GuiGraphicsExtractor.blitNineSlicedSprite`): os cantos no tamanho do sprite, as bordas
/// e o miolo repetidos, não esticados, a menos que o `.mcmeta` peça. O Horizon só lê o `gui.scaling` dos sprites do vanilla.
pub(in crate::game) fn nine_slice(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    slice: NineSlice,
    rect: [f32; 4],
) {
    for cell in gui::nine_slice_cells(slice, rect) {
        game.native_sprite(batch, sprite, cell.src, cell.dst);
    }
}

/// Os sprites de aviso de um elemento (`GuiElement.WARNING_BACKGROUND_TEXTURE` e `WARNING_TEXTURE`). Os dois têm `gui.scaling`
/// do tipo `tile`, que o Horizon não aplica: `tile` é o tamanho do mosaico, o `width` e o `height` do `.mcmeta`.
pub(in crate::game) struct WarningSprites {
    pub background: &'static str,
    pub overlay: &'static str,
    pub tile: [f32; 2],
}

/// Um sprite em mosaico: repete a partir do canto de cima e da esquerda de `dst`, corta o que passa da borda (`clip` é o scissor).
pub(in crate::game) fn tiled_sprite(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
    clip: Option<[f32; 4]>,
) {
    if let Some(rect) = game.gui.atlas.sprites.get(sprite) {
        register_animated(game, rect);
    }
    for cell in gui::tile_cells(dst, tile, clip) {
        game.native_sprite(batch, sprite, cell.src, cell.dst);
    }
}

/// O que `GuiBar` desenha em volta do conteúdo: a moldura (`bar/base`, nove partes) e o `gui.scaling` dela.
pub(in crate::game) struct BarStyle {
    pub frame: &'static str,
    pub frame_slice: NineSlice,
}

/// `GuiBar.drawBackground` de uma barra vertical: a moldura sobre todo o elemento, o fundo do aviso dentro dela, o conteúdo que
/// `contents` desenha (recebe o retângulo de baixo para cima que `level` pede, só se há nível) e, com a barra quase cheia, o aviso
/// de novo por cima, só na metade de trás. `rect` é o elemento inteiro, com a moldura.
pub(in crate::game) fn vertical_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &BarStyle,
    rect: [f32; 4],
    level: f64,
    warning: Option<&WarningSprites>,
    contents: &dyn Fn(&Game<'_>, &mut HudBatch, [f32; 4]),
) {
    nine_slice(game, batch, style.frame, style.frame_slice, rect);
    let warned = warning.map(|sprites| (sprites, gui::bar_warning(rect, level, false)));
    if let Some((sprites, bar)) = &warned {
        tiled_sprite(game, batch, sprites.background, sprites.tile, bar.background, None);
    }
    let Some(dst) = gui::bar_contents(rect, level) else {
        return;
    };
    contents(game, batch, dst);
    if let Some((sprites, bar)) = &warned
        && let Some(overlay) = bar.overlay
    {
        tiled_sprite(game, batch, sprites.overlay, sprites.tile, overlay.dst, Some(overlay.clip));
    }
}

/// `GuiVerticalPowerBar.renderBarContents`: o sprite em mosaico (`tile`, o `gui.scaling` do `.mcmeta`) sobre `dst`, a partir do canto
/// de cima e da esquerda.
pub(in crate::game) fn power_bar_contents(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
) {
    tiled_sprite(game, batch, sprite, tile, dst, None);
}

/// `GuiTankBar.renderBarContentsDirect` com o sprite de um químico: de cima para baixo e da esquerda para a direita
/// (`TilingDirection.DOWN_RIGHT`), cortado em `dst`, multiplicado pelo `tint` (`ARGB`). `tile` é o tamanho do quadro do sprite. O
/// sprite é o do atlas de blocos, que o HUD do Horizon anima como o jogo.
pub(in crate::game) fn tank_bar_contents(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
    tint: u32,
) {
    for cell in gui::tiled_fill(dst, tile, TilingDirection::DownRight) {
        sprite_tinted(game, batch, sprite, cell.src, cell.dst, tint);
    }
}

/// `GuiGauge`: o medidor com moldura de nove partes e um overlay do tamanho do miolo.
pub(in crate::game) struct GaugeStyle {
    pub frame: &'static str,
    pub frame_slice: NineSlice,
    pub overlay: &'static str,
}

/// `GuiGauge.drawBackground` e `renderContents`: a moldura, o conteúdo de `scaled` pixels (`GuiTankGauge.getScaledLevel`) em
/// mosaico a partir da base (`TilingDirection.UP_RIGHT`) multiplicado por `tint`, e o overlay por cima de tudo. `fill` é o sprite do
/// que o tanque guarda e o tamanho do quadro dele. `rect` é o elemento inteiro, com a moldura.
pub(in crate::game) fn gauge(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &GaugeStyle,
    rect: [f32; 4],
    scaled: i32,
    fill: Option<(&str, [f32; 2], u32)>,
) {
    let [x, y, w, h] = rect;
    nine_slice(game, batch, style.frame, style.frame_slice, rect);
    if let Some((sprite, tile, tint)) = fill
        && scaled > 0
    {
        let scaled = scaled as f32;
        let dst = [x + 1.0, y + 1.0 + (h - 2.0) - scaled, w - 2.0, scaled];
        for cell in gui::tiled_fill(dst, tile, TilingDirection::UpRight) {
            sprite_tinted(game, batch, sprite, cell.src, cell.dst, tint);
        }
    }
    game.native_sprite(batch, style.overlay, [0.0, 0.0, w - 2.0, h - 2.0], [x + 1.0, y + 1.0, w - 2.0, h - 2.0]);
}

/// `GuiSlot.draw`: a moldura do slot de item em `item` (a posição do item, a moldura começa um pixel antes) e o desenho por cima dela.
/// Quem chama escolhe `frame`: a do tipo do slot, ou a do aviso quando a verificação dele diz que sim. `None` num slot sem moldura.
pub(in crate::game) fn slot(
    game: &Game<'_>,
    batch: &mut HudBatch,
    origin: [f32; 2],
    item: [f32; 2],
    frame: Option<&str>,
    overlay: Option<&str>,
) {
    let [x, y, w, h] = gui::slot_frame(item);
    let dst = [origin[0] + x, origin[1] + y, w, h];
    for sprite in frame.into_iter().chain(overlay) {
        game.native_sprite(batch, sprite, [0.0, 0.0, w, h], dst);
    }
}

/// Os três sprites de um tipo de `ProgressType`: o vazio, o cheio e o do aviso, todos do tamanho do elemento.
pub(in crate::game) struct ProgressSprites {
    pub empty: &'static str,
    pub full: &'static str,
    pub warning: &'static str,
}

/// `GuiProgress.drawBackground` de um progresso horizontal que enche da esquerda para a direita: o sprite vazio, e por cima o
/// cheio até onde o progresso chegou (pelo menos um pixel). Com aviso é o sprite do aviso, com o progresso em 1.
/// `inner_offset` é a moldura que o sprite não preenche (`ProgressType.BAR` tem um pixel de cada lado).
pub(in crate::game) fn progress_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprites: &ProgressSprites,
    rect: [f32; 4],
    fraction: f32,
    inner_offset: f32,
    warning: bool,
) {
    let [x, y, w, h] = rect;
    game.native_sprite(batch, sprites.empty, [0.0, 0.0, w, h], rect);
    if let Some((offset, width)) = gui::progress_fill(w, inner_offset, fraction, warning) {
        let sprite = if warning { sprites.warning } else { sprites.full };
        game.native_sprite(batch, sprite, [offset, 0.0, width, h], [x + offset, y, width, h]);
    }
}

/// `GuiWarningTab`, à esquerda da tela: `sprite` na posição que o Java dá, a partir do canto `origin` da tela.
pub(in crate::game) fn warning_tab(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, origin: [f32; 2]) {
    let [x, y, w, h] = gui::WARNING_TAB_LEFT;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], [origin[0] + x, origin[1] + y, w, h]);
}

/// O tooltip de um widget (`WidgetTooltipHolder`): `lines`, cada uma com a cor própria (`None` é a do padrão), ao lado do cursor
/// mas por cima ou por baixo do `widget` (`MenuTooltipPositioner`), em vez de sobre o cursor como o tooltip de um item. O hook
/// `native_screen_tooltip` só entrega texto com os códigos `§` do vanilla, que não têm as cores de `EnumColor`, e posiciona sempre
/// no cursor. `mouse` e `widget` (`[x, y, largura, altura]`) estão em pixels de GUI da janela.
pub(in crate::game) fn menu_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
) {
    menu_tooltip_with(game, batch, lines, mouse, widget, true);
}

/// Como [`menu_tooltip`], para um texto que o jogo quebrou em linhas (`Tooltip.create`): sem o intervalo de dois pixels depois da
/// primeira linha, que só o tooltip com título tem.
pub(in crate::game) fn menu_tooltip_flat(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
) {
    menu_tooltip_with(game, batch, lines, mouse, widget, false);
}

fn menu_tooltip_with(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
    title_gap: bool,
) {
    let lines: Vec<(horizon_client::text::Text, u32)> = lines
        .iter()
        .map(|(text, color)| {
            let text = match color {
                Some(color) => horizon_client::text::Text::colored(text.as_str(), *color),
                None => horizon_client::text::Text::literal(text.as_str()),
            };
            (text, u32::MAX)
        })
        .collect();
    let atlas = &game.gui.atlas;
    // O tamanho não depende de onde o tooltip cai; a posição é a do `MenuTooltipPositioner`.
    let (_, _, mut w, mut h) = Game::tooltip_geometry(atlas, &lines, (0.0, 0.0), (batch.width, batch.height), [0.0; 2]);
    if !title_gap {
        // Cada linha medida sozinha: a largura é a maior, e a altura 10 por linha menos 2.
        w = 0.0;
        for line in &lines {
            let (_, _, line_w, _) = Game::tooltip_geometry(atlas, std::slice::from_ref(line), (0.0, 0.0), (batch.width, batch.height), [0.0; 2]);
            w = w.max(line_w);
        }
        h = 10.0 * lines.len() as f32 - 2.0;
    }
    let [x, y] = tooltip::menu_position(
        [batch.width as i32, batch.height as i32],
        [mouse.0 as i32, mouse.1 as i32],
        [w as i32, h as i32],
        widget.map(|edge| edge.round() as i32),
    );
    // O 26.x desenha o tooltip com os sprites `tooltip/background` e `tooltip/frame` em nove partes (`TooltipRenderUtil`), de 12 pixels
    // além do texto. `tooltip_box` do Horizon é o desenho de retângulos de versões antigas, de cantos quadrados.
    for sprite in ["gui/sprites/tooltip/background", "gui/sprites/tooltip/frame"] {
        horizon_client::ui::nine_slice(batch, atlas, sprite, x as f32 - 12.0, y as f32 - 12.0, w + 24.0, h + 24.0, u32::MAX);
    }
    if title_gap {
        Game::tooltip_text(batch, atlas, &lines, x as f32, y as f32);
    } else {
        for (n, line) in lines.iter().enumerate() {
            Game::tooltip_text(batch, atlas, std::slice::from_ref(line), x as f32, y as f32 + 10.0 * n as f32);
        }
    }
}

/// O fundo de uma tela de máquina: o painel (`GuiMekanism.extractBackground`), os slots do inventário do jogador
/// (`GuiMekanism.addSlots`, um `GuiSlot` normal para cada slot que não é do tile) e os dois rótulos (`renderTitleText` e
/// `renderInventoryText`), o título centrado e "Inventory" à esquerda, os dois em `SpecialColors.TEXT_TITLE` e sem sombra.
pub(in crate::game) fn machine_panel(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &PanelStyle,
    rect: [f32; 4],
    title: &str,
) {
    let [x, y, width, height] = rect;
    nine_slice(game, batch, style.base, style.base_slice, rect);
    for row in 0..=INVENTORY_ROWS {
        let gap = if row == INVENTORY_ROWS { HOTBAR_GAP } else { 0.0 };
        let slot_y = y + INVENTORY_TOP + row as f32 * gui::SLOT_SIZE + gap;
        for column in 0..INVENTORY_COLUMNS {
            let slot_x = x + INVENTORY_LEFT + column as f32 * gui::SLOT_SIZE;
            let dst = [slot_x, slot_y, gui::SLOT_SIZE, gui::SLOT_SIZE];
            game.native_sprite(batch, style.slot, [0.0, 0.0, gui::SLOT_SIZE, gui::SLOT_SIZE], dst);
        }
    }
    let title_x = x + gui::title_x(width, game.native_text_width(title));
    game.native_text(batch, title, [title_x, y + gui::TITLE_LABEL_Y], gui::TITLE_TEXT_COLOR, false);
    let inventory = horizon_client::lang::get("container.inventory");
    let label = [x + gui::INVENTORY_LABEL_X, y + gui::inventory_label_y(height)];
    game.native_text(batch, &inventory, label, gui::TITLE_TEXT_COLOR, false);
}

/// Liga a instrumentação de medida da comparação das opções B e C: `MEKTRACE` a cada tick de uma máquina e `MEKBENCH` a
/// cada janela. É o único lugar que a liga e desliga.
pub(in crate::game) const MEDIR: bool = false;
/// A janela do `MEKBENCH`, em ticks do jogo.
const BENCH_WINDOW_TICKS: i64 = 1200;
static BENCH_MACHINE_TICKS: AtomicU64 = AtomicU64::new(0);
static BENCH_NANOS: AtomicU64 = AtomicU64::new(0);
static BENCH_WINDOW: AtomicI64 = AtomicI64::new(i64::MIN);

/// Soma o tempo da lógica de um tick de uma máquina, e a cada [`BENCH_WINDOW_TICKS`] imprime
/// `MEKBENCH ticks=<ticks de máquina na janela> ns=<soma>` e zera.
pub(in crate::game) fn bench_record(game_time: i64, nanos: u64) {
    let window = game_time.div_euclid(BENCH_WINDOW_TICKS);
    let last = BENCH_WINDOW.swap(window, Ordering::Relaxed);
    if last != i64::MIN && last != window {
        let ticks = BENCH_MACHINE_TICKS.swap(0, Ordering::Relaxed);
        let nanos_sum = BENCH_NANOS.swap(0, Ordering::Relaxed);
        eprintln!("MEKBENCH ticks={ticks} ns={nanos_sum}");
    }
    BENCH_MACHINE_TICKS.fetch_add(1, Ordering::Relaxed);
    BENCH_NANOS.fetch_add(nanos, Ordering::Relaxed);
}

/// Uma linha de `MEKTRACE`: o JSON de um tick, com os campos dos rastros.
pub(in crate::game) fn trace(json: &str) {
    eprintln!("MEKTRACE {json}");
}
