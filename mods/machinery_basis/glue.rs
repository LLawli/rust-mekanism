// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::native_block_entity_draw::{BlockEntityDraw, CubeFace};
use crate::game::{Game, GameLevel, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::crafting::{Cooking, Ingredient, Shape};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;
use horizon_core::position::BlockPos;
use horizon_world::level::Ticks;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use horizon_client::item_data::{StackData, canon_tag, decode_components, encode_components};
use machinery_basis_core::components::Components;
use machinery_basis_core::components_ingredient::{self, ComponentsIngredient, Leaf, Tree};
use machinery_basis_core::containers::{ItemResource, SlotContents, room_for};
use machinery_basis_core::biome_temperature;
use machinery_basis_core::fluid_logged;
use machinery_basis_core::gui::{self, NineSlice, TilingDirection};
use machinery_basis_core::tooltip;
use machinery_basis_core::heat;
use machinery_basis_core::liquid_data;
use machinery_basis_core::hopper::{self, Cooldown, Ejector, Hopper, Transfer};
use machinery_basis_core::neighbors::{OUTPUT_SIGNAL_ORDER, PendingPower};
use machinery_basis_core::network::{self, EnergyBlocks, Link};
use machinery_basis_core::recipe::{
    Conversion, ConversionFields, EnergyConversionData, MachineRecipeData, RecipeFields, ResourceInput,
};
use machinery_basis_core::resource::{self, Registry, Tags};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::tank::{FORMAT, Tank};
use machinery_basis_core::temperature;
use machinery_basis_core::upgrade::{self, UpgradeSaved, UpgradeTags, UpgradeTypes};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

const LISTENER: &str = "machinery_basis:resources";
const TAGS_LISTENER: &str = "machinery_basis:resource_tags";
const ENERGY_LISTENER: &str = "machinery_basis:energy";
const UPGRADES_LISTENER: &str = "machinery_basis:upgrades";
const UPGRADE_TAGS_LISTENER: &str = "machinery_basis:upgrade_tags";
const LIQUIDS_LISTENER: &str = "machinery_basis:liquids";
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
static ENERGY_CONVERSIONS: Mutex<BTreeMap<String, Arc<Vec<ItemEnergyConversion>>>> = Mutex::new(BTreeMap::new());
/// As receitas que `report` achou incompletas (um ingrediente sem membro: `placementInfo().isImpossibleToPlace()`) nesta carga de dados.
/// Se esvazia a cada carga, junto dos índices.
static INCOMPLETE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
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
    /// Os componentes do ingrediente, quando ele é um `neoforge:components`.
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub resource: Option<ResourceInput>,
    pub output: ItemStack,
    /// `per_tick_usage`: a receita gasta o recurso a cada tick, e não de uma vez no fim.
    pub per_tick_usage: bool,
    /// `MekanismRecipe.experience()` ou `SmeltingRecipe.experience()`: a experiência de cada operação.
    pub experience: f32,
}

/// Um item que vira recurso, com o que o jogo resolveu.
pub(in crate::game) struct ItemConversion {
    pub id: String,
    pub item: Ingredient,
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub resource: String,
    pub amount: u64,
}

/// Um item que vira energia, com o que o jogo resolveu.
pub(in crate::game) struct ItemEnergyConversion {
    pub id: String,
    pub item: Ingredient,
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub amount: i32,
}

/// De onde vêm as receitas de uma máquina.
#[derive(Clone, Copy)]
pub(in crate::game) enum RecipeSource {
    /// Arquivos de um tipo próprio.
    Custom(&'static str, RecipeFields),
    /// As receitas de cozimento do jogo, de um tipo como `smelting`.
    Cooking(&'static str),
}

impl Game<'_> {
    /// Roda `act` em cada nível que o servidor ticou e que `wanted` aceita (pelo id da dimensão), com o nível no lugar de `self.world`.
    /// Serve ao `ServerTickEnd`, onde só o último nível está no lugar: entra em cada um como `close_idle_levels` e deixa no lugar o
    /// que estava, para o resto do fim do tick não ver outro nível.
    pub(in crate::game) fn mb_in_levels(
        &mut self,
        wanted: impl Fn(&str) -> bool,
        mut act: impl FnMut(&mut Self, &'static str),
    ) {
        let back = self.dim;
        let mut dims: Vec<crate::game::Dim> =
            self.levels_to_tick().into_iter().filter(|dim| wanted(dim.id())).collect();
        // O nível que já está no lugar fica por último: o caso comum, uma dimensão só, não troca nada.
        dims.sort_by_key(|dim| *dim == back);
        for dim in dims {
            if dim != self.dim && self.enter_level(None, dim).is_err() {
                continue;
            }
            act(self, dim.id());
        }
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
    }
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

/// Um balde coloca ou tira a lava de um bloco com `fluid_logged`, como água numa escada, e o item de upgrade com shift entra num bloco
/// com janela de upgrades. Um bloco com tela só chega aqui com o jogador agachado: em pé, o clique abre a tela.
pub(in crate::game) fn use_item_on(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    // `ItemUpgrade.useOn`: o upgrade na mão, com shift, entra no bloco que o aceita. Os itens não se cruzam com o balde.
    game.mb_window_use_item_on(stack, pos, claimed);
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
pub(in crate::game) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    // O balde de um fluido de mod esvazia, e o balde vazio pega a fonte de um (`machinery_liquid`).
    crate::game::machinery_liquid::use_item(game, stack, claimed);
    if *claimed {
        return;
    }
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
    crate::game::machinery_liquid::update_shape(level, ticks, pos, state, direction, around, new);
    if new.is_some() {
        return;
    }
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
    listeners.push(NativeReloadListener::json(LIQUIDS_LISTENER, liquid_data::FOLDER));
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
    if listener.id == LIQUIDS_LISTENER {
        let (liquids, errors) = liquid_data::Liquids::load(files);
        for (id, error) in errors {
            eprintln!("machinery_basis: fluido {id} recusado: {error}");
        }
        eprintln!("machinery_basis: {} fluidos", liquids.len());
        liquid_data::set_source(BASIS_SOURCE, liquids);
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
    ENERGY_CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).clear();
    NETWORK_WALKS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Se a entrada de chave de componentes `patch_key` passa no ingrediente `tree`: o patch dela é o que `PATCHES` guarda para a chave.
pub(in crate::game) fn tree_accepts(tree: &Tree, item: &str, patch_key: Option<&str>) -> bool {
    let patch = patch_key
        .and_then(|key| PATCHES.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned())
        .unwrap_or_default();
    tree.accepts(item, &patch)
}

/// A árvore de um ingrediente do Horizon, e se há componentes nela.
fn build_node(ingredient: &Ingredient, defaults: &horizon_world::item_defaults::Defaults) -> Result<(Tree, bool), Problem> {
    if let Some((kind, data)) = &ingredient.custom {
        if kind != components_ingredient::KIND {
            let other = ingredient.clone();
            let test = move |item: &str| {
                other.test(&ItemStack { item: Arc::from(item), count: 1, damage: 0, extra: None })
            };
            return Ok((Tree::Opaque(Box::new(test)), false));
        }
        let parsed = ComponentsIngredient::parse(data, |value| canon_tag(&json_tag(value)))
            .ok_or_else(|| Problem::Parse("`components` ou `strict` inválido".into()))?;
        let items = ingredient.order.clone();
        let table = items.iter().map(|item| (item.clone(), default_texts(defaults, item))).collect();
        return Ok((Tree::Components(Leaf { ingredient: parsed, items, defaults: table }), true));
    }
    let Some((shape, kids)) = &ingredient.children else {
        return Ok((Tree::Plain(ingredient.order.clone()), false));
    };
    let mut nodes = Vec::with_capacity(kids.len());
    let mut any = false;
    for kid in kids {
        let (node, has) = build_node(kid, defaults)?;
        any |= has;
        nodes.push(node);
    }
    if !any {
        return Ok((Tree::Plain(ingredient.order.clone()), false));
    }
    let node = match shape {
        Shape::Compound => Tree::Compound(nodes),
        Shape::Intersection => Tree::Intersection(nodes),
        Shape::Difference => {
            let mut nodes = nodes.into_iter();
            match (nodes.next(), nodes.next()) {
                (Some(base), Some(subtracted)) => Tree::Difference(Box::new(base), Box::new(subtracted)),
                _ => return Err(Problem::Parse("`difference` sem `base` e `subtracted`".into())),
            }
        }
    };
    Ok((node, true))
}

/// A árvore de um ingrediente com `neoforge:components` em algum ramo; `None` sem nenhum (o jogo o testa sozinho).
fn node_of(game: &Game<'_>, ingredient: &Ingredient) -> Result<Option<Arc<Tree>>, Problem> {
    let defaults = horizon_world::item_defaults::shared(&game.options.resources)
        .map_err(|error| Problem::Parse(format!("componentes padrão: {error}")))?;
    let (node, has) = build_node(ingredient, &defaults)?;
    Ok(has.then(|| Arc::new(node)))
}

/// O valor JSON de um componente como `Tag`, para o texto canônico ser o mesmo do patch de uma pilha do jogo.
fn json_tag(value: &serde_json::Value) -> Tag {
    match value {
        serde_json::Value::Null => Tag::Compound(Compound::default()),
        serde_json::Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(whole) => i32::try_from(whole).map_or(Tag::Long(whole), Tag::Int),
            None => Tag::Double(number.as_f64().unwrap_or(0.0).to_bits()),
        },
        serde_json::Value::String(text) => Tag::String(JavaString::from(text.as_str())),
        serde_json::Value::Array(list) => Tag::List(list.iter().map(json_tag).collect()),
        serde_json::Value::Object(map) => {
            Tag::Compound(map.iter().map(|(key, value)| (JavaString::from(key.as_str()), json_tag(value))).collect())
        }
    }
}

/// Os componentes padrão de um item em texto canônico.
fn default_texts(defaults: &horizon_world::item_defaults::Defaults, item: &str) -> BTreeMap<String, String> {
    defaults
        .get(item)
        .and_then(serde_json::Value::as_object)
        .map(|components| components.iter().map(|(kind, value)| (kind.clone(), canon_tag(&json_tag(value)))).collect())
        .unwrap_or_default()
}

/// Por que um ingrediente não virou nada: o arquivo está errado (o codec o recusaria: `Couldn't parse data file`), ou o ingrediente
/// não tem item nenhum (uma tag vazia: o Mekanism guarda a receita, que não casa com nada, e avisa em `Incomplete recipe detected`).
enum Problem {
    Parse(String),
    Empty(Vec<String>),
}

/// O texto de `Problem` para a receita `id`, no log, como o original: `Couldn't parse data file` do `SimpleJsonResourceReloadListener`, e
/// `Incomplete recipe detected` com `Empty tag` (`MekanismRecipeType.checkMyIncompleteRecipes`, `ItemStackIngredient.logMissingTags`).
fn report(id: &str, problem: &Problem) {
    match problem {
        Problem::Parse(why) => eprintln!("machinery_basis: Couldn't parse data file '{id}': {why}"),
        Problem::Empty(tags) => {
            INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_owned());
            eprintln!("machinery_basis: Incomplete recipe detected: {id}");
            for tag in tags {
                eprintln!("machinery_basis: Empty tag: {tag}");
            }
        }
    }
}

/// O que `native_recipe_ingredient` não achou: um item que o jogo não conhece é erro do arquivo, e uma tag sem membros deixa a receita
/// incompleta.
fn missing_items(json: &serde_json::Value) -> Problem {
    let names: Vec<&str> = match json {
        serde_json::Value::String(name) => vec![name.as_str()],
        serde_json::Value::Array(list) => list.iter().filter_map(serde_json::Value::as_str).collect(),
        other => return Problem::Parse(format!("ingrediente inválido: {other}")),
    };
    let tags: Vec<String> = names.iter().filter_map(|name| name.strip_prefix('#')).map(str::to_owned).collect();
    if !tags.is_empty() && tags.len() == names.len() {
        return Problem::Empty(tags);
    }
    Problem::Parse(format!("item desconhecido em {json}"))
}

/// O ingrediente de item de uma receita de máquina. Um item, uma tag ou uma lista saem pelo jogo (`native_recipe_ingredient`, que também
/// tira os itens que ele não conhece). Um objeto (`neoforge:components`, `compound`, `intersection`, `difference`) o jogo não entrega à
/// mod, então o arquivo passa pelo `Ingredient::parse` do Horizon (com a injeção `components-ingredient`, que lê o
/// `neoforge:components` com as tags resolvidas) e a árvore sai daqui.
fn machine_ingredient(game: &Game<'_>, json: &serde_json::Value) -> Result<(Ingredient, Option<Arc<Tree>>), Problem> {
    if !json.is_object() {
        return game.native_recipe_ingredient(json).map(|ingredient| (ingredient, None)).ok_or_else(|| missing_items(json));
    }
    let ingredient = Ingredient::parse(json, game.items())
        .ok_or_else(|| Problem::Parse(format!("o Horizon não leu o ingrediente {json}")))?;
    let node = node_of(game, &ingredient)?;
    Ok((ingredient, node))
}

fn custom_recipes(game: &Game<'_>, recipe_type: &str, fields: &RecipeFields) -> Vec<MachineRecipe> {
    let mut recipes = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = MachineRecipeData::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(MachineRecipeData::why_not(&recipe.json, fields)));
            continue;
        };
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        let Some(output) = game.native_recipe_result(&parsed.output) else {
            report(&recipe.id, &Problem::Parse(format!("resultado inválido ou com item desconhecido: {}", parsed.output)));
            continue;
        };
        // Uma tag de recurso sem membro (`ChemicalStackIngredient.hasNoMatchingInstances`) deixa a receita incompleta, como a de item. O
        // original a guarda, e ela não casa com nada: aqui a receita fica, com o mesmo efeito, e só o aviso sai.
        if let Some(tag) = parsed.resource.as_ref().and_then(|input| input.ingredient.strip_prefix(resource::TAG_PREFIX))
            && resource::current_tags().members(tag).next().is_none()
        {
            report(&recipe.id, &Problem::Empty(vec![tag.to_owned()]));
        }
        recipes.push(MachineRecipe {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            resource: parsed.resource,
            output,
            per_tick_usage: parsed.per_tick_usage,
            experience: recipe.json.get("experience").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
        });
    }
    recipes
}

/// A árvore do ingrediente de uma receita de cozimento do jogo, se tem `neoforge:components` nela. Se a árvore não se monta, a receita fica com
/// o teste do Horizon e o aviso no log.
fn cooking_node(game: &Game<'_>, recipe: &Cooking) -> Option<Arc<Tree>> {
    match node_of(game, &recipe.ingredient) {
        Ok(node) => node,
        Err(problem) => {
            report(&recipe.id, &problem);
            None
        }
    }
}

fn cooking_recipes(game: &Game<'_>, kind: &str) -> Vec<MachineRecipe> {
    let of_kind = |recipe: &&Cooking| recipe.kind.strip_prefix("minecraft:").unwrap_or(&recipe.kind) == kind;
    let cooking = game.recipes().cooking.iter().filter(of_kind);
    cooking
        .map(|recipe| MachineRecipe {
            id: recipe.id.clone(),
            item: recipe.ingredient.clone(),
            components: cooking_node(game, recipe),
            count: 1,
            resource: None,
            output: recipe.result.clone(),
            per_tick_usage: false,
            experience: recipe.experience,
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
    let mut loaded = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = Conversion::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(Conversion::why_not(&recipe.json, fields)));
            continue;
        };
        if registry.get(&parsed.resource).is_none() {
            report(&recipe.id, &Problem::Parse(format!("o recurso {} não existe", parsed.resource)));
            continue;
        }
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        loaded.push(ItemConversion {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            resource: parsed.resource,
            amount: parsed.amount,
        });
    }
    let conversions = loaded;
    let conversions = Arc::new(conversions);
    index.insert(recipe_type.to_owned(), conversions.clone());
    conversions
}

/// As conversões de item em energia de um tipo (`energy_conversion`), na ordem em que o jogo as entrega. Fica de fora a que cita
/// um item ou uma tag que o jogo não conhece (`native_recipe_ingredient` devolve `None`), e a que não tem uma energia inteira e
/// positiva.
pub(in crate::game) fn energy_conversions(
    game: &Game<'_>,
    recipe_type: &str,
    fields: &ConversionFields,
) -> Arc<Vec<ItemEnergyConversion>> {
    let mut index = ENERGY_CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(conversions) = index.get(recipe_type) {
        return conversions.clone();
    }
    let mut loaded = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = EnergyConversionData::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(EnergyConversionData::why_not(&recipe.json, fields)));
            continue;
        };
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        loaded.push(ItemEnergyConversion {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            amount: parsed.amount,
        });
    }
    let conversions = loaded;
    let conversions = Arc::new(conversions);
    index.insert(recipe_type.to_owned(), conversions.clone());
    conversions
}

/// `MekanismRecipeType.checkIncompleteRecipes`: lê agora as receitas dos tipos que o mod usa (o que o primeiro uso faria depois da carga de
/// dados) e devolve os ids das que ficaram incompletas, em ordem. Cada uma já foi ao log (`Incomplete recipe detected: <id>`, com
/// `Empty tag: <tag>`), como `checkMyIncompleteRecipes` e `logMissingTags`.
pub(in crate::game) fn incomplete_recipes(
    game: &Game<'_>,
    machines: &[RecipeSource],
    conversions: &[(&str, &ConversionFields)],
    energy: &[(&str, &ConversionFields)],
) -> Vec<String> {
    for source in machines {
        machine_recipes(game, source);
    }
    for (recipe_type, fields) in conversions {
        self::conversions(game, recipe_type, fields);
    }
    for (recipe_type, fields) in energy {
        energy_conversions(game, recipe_type, fields);
    }
    INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

/// `Item.getMaxStackSize` de uma pilha: o componente `minecraft:max_stack_size` dela, se tem, e senão o do item
/// (`ItemResource.getMaxStackSize` lê os componentes do recurso).
pub(in crate::game) fn stack_limit(game: &Game<'_>, stack: &ItemStack) -> u32 {
    let patched = stack
        .data()
        .and_then(|data| data.other(MAX_STACK_SIZE_COMPONENT))
        .and_then(|tag| match tag {
            Tag::Int(limit) => u32::try_from(*limit).ok(),
            _ => None,
        });
    patched.unwrap_or_else(|| game.native_max_stack_size(&stack.item).unwrap_or(DEFAULT_STACK))
}

/// O que um `ItemStack` do jogo tem além do id e da quantidade, guardado por chave de componentes: o dano e os componentes que o
/// distinguem. A lib compara o `ItemResource` por esses componentes em texto canônico, e o jogo precisa do que o texto veio para
/// refazer a pilha quando o item sai de um slot da lib. Cresce com os conjuntos de componentes distintos que já passaram por uma
/// máquina.
type Extras = (u32, Option<Box<StackData>>);
static EXTRAS: Mutex<BTreeMap<String, Extras>> = Mutex::new(BTreeMap::new());
/// Os componentes em texto canônico de cada chave de `EXTRAS`: o teste de um ingrediente de componentes (`neoforge:components`) só recebe a
/// chave da entrada e refaz a pilha daqui.
static PATCHES: Mutex<BTreeMap<String, Vec<(String, String)>>> = Mutex::new(BTreeMap::new());
const MAX_STACK_SIZE_COMPONENT: &str = "minecraft:max_stack_size";

/// O `DataComponentPatch` de uma pilha como a lib o compara: cada componente com o valor em texto canônico, o dano entre eles
/// (`minecraft:damage`). Registra o que a pilha tem para refazê-la.
fn item_components(stack: &ItemStack) -> Components {
    let Some(Tag::Compound(entries)) = encode_components(stack) else {
        return Components::none();
    };
    let texts: Vec<(String, String)> = entries
        .iter()
        .map(|(id, value)| (id.to_string_lossy(), canon_tag(value)))
        .collect();
    let components = Components::new(texts.iter().map(|(id, value)| (id.as_str(), value.as_str())));
    let mut extras = EXTRAS.lock().unwrap_or_else(|e| e.into_inner());
    extras
        .entry(components.key().to_owned())
        .or_insert_with(|| (stack.damage, stack.extra.clone()));
    PATCHES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(components.key().to_owned())
        .or_insert_with(|| components.iter().map(|(kind, value)| (kind.to_owned(), value.to_owned())).collect());
    components
}

/// `ItemResource.of(stack)`: o item, a pilha máxima dele e os componentes que o distinguem.
pub(in crate::game) fn item_resource(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    ItemResource::with_components(&stack.item, stack_limit(game, stack), item_components(stack))
}

/// O que um slot da lib guarda, de uma pilha do jogo.
pub(in crate::game) fn slot_contents(game: &Game<'_>, stack: Option<&ItemStack>) -> SlotContents {
    match stack {
        Some(stack) => SlotContents::new(Some(item_resource(game, stack)), stack.count),
        None => SlotContents::default(),
    }
}

/// `ItemResource.toStack(amount)`: a pilha de um recurso, com o dano e os componentes que ele tem. `None` quando o recurso tem
/// componentes que nunca passaram por uma pilha do jogo (quem o fez na lib os monta à mão).
pub(in crate::game) fn stack_of_resource(resource: &ItemResource, count: u32) -> Option<ItemStack> {
    let (damage, extra) = if resource.components.is_empty() {
        (0, None)
    } else {
        EXTRAS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(resource.components.key())
            .cloned()?
    };
    Some(ItemStack {
        item: Arc::from(&*resource.id),
        count,
        damage,
        extra,
    })
}

/// Os componentes de um recurso como o `components` do `ItemResource.CODEC` os grava, para o NBT: o dano e cada componente na forma
/// que a pilha tem. `None` sem componentes ou quando o recurso não veio de uma pilha do jogo.
pub(in crate::game) fn components_tag(resource: &ItemResource) -> Option<Tag> {
    encode_components(&stack_of_resource(resource, 1)?)
}

/// O recurso do `ItemResource.CODEC` lido do NBT: o id e o `components` (dano e componentes), com a pilha máxima do item.
pub(in crate::game) fn resource_from_tag(game: &Game<'_>, id: &str, components: Option<&Tag>) -> ItemResource {
    let (damage, extra) = decode_components(components);
    let stack = ItemStack {
        item: Arc::from(id),
        count: 1,
        damage,
        extra,
    };
    item_resource(game, &stack)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O componente de upgrades (`TileComponentUpgrade`) no NBT do tile e nos slots do menu. Vale para quem tem o objeto vivo (as máquinas do
// Mekanism) e para quem se refaz do NBT a cada tick (o Heat Generator).

/// `SerializationConstants.COMPONENT_UPGRADE`: o `TileComponentUpgrade` grava num filho com este nome.
const COMPONENT_UPGRADE: &str = "component_upgrade";
/// `SerializationConstants.UPGRADES` e `ITEMS`: o mapa de upgrades instalados e a lista dos dois slots.
const UPGRADES_KEY: &str = "upgrades";
const ITEMS_KEY: &str = "items";
/// `ContainerType.ITEM.saveTo` e `LargeResourceStack.codec`: `{container: byte, stored: {type: {id, components}, amount}}`.
const CONTAINER_KEY: &str = "container";
const STORED_KEY: &str = "stored";
const TYPE_KEY: &str = "type";
const AMOUNT_KEY: &str = "amount";
/// O `ItemResource.CODEC`: o id do item e os componentes que o distinguem.
const ITEM_ID_KEY: &str = "id";
const COMPONENTS_KEY: &str = "components";

/// `LargeResourceStack.codec` de um `ItemResource`: `type` (`id` e `components`) e `amount`. Os componentes são o `DataComponentPatch`
/// inteiro (o dano e cada componente, o tipo do upgrade entre eles). Um slot que se lê mal vale vazio, como o `readOrEmpty`.
fn upgrade_slot_contents(game: &Game<'_>, stored: &Compound) -> SlotContents {
    let Some(Tag::Compound(kind)) = stored.get(&JavaString::from(TYPE_KEY)) else {
        return SlotContents::default();
    };
    let Some(Tag::String(id)) = kind.get(&JavaString::from(ITEM_ID_KEY)) else {
        return SlotContents::default();
    };
    let id = id.to_string_lossy();
    let amount = match stored.get(&JavaString::from(AMOUNT_KEY)) {
        Some(Tag::Long(amount)) => u32::try_from(*amount).ok(),
        Some(Tag::Int(amount)) => u32::try_from(*amount).ok(),
        _ => None,
    };
    let resource = resource_from_tag(game, &id, kind.get(&JavaString::from(COMPONENTS_KEY)));
    SlotContents::new(Some(resource), amount.unwrap_or(0))
}

/// `TileComponentUpgrade.deserialize`: `upgrades` (o mapa de tipo para total) e os dois slots, na lista `items`.
pub(in crate::game) fn read_upgrades(game: &Game<'_>, entity: &ModBlockEntity) -> UpgradeSaved {
    let mut saved = UpgradeSaved::default();
    let Some(Tag::Compound(component)) = entity.nbt.get(&JavaString::from(COMPONENT_UPGRADE)) else {
        return saved;
    };
    if let Some(Tag::Compound(upgrades)) = component.get(&JavaString::from(UPGRADES_KEY)) {
        for (id, total) in upgrades {
            if let Tag::Int(total) = total {
                saved.upgrades.insert(id.to_string_lossy(), *total);
            }
        }
    }
    if let Some(Tag::List(items)) = component.get(&JavaString::from(ITEMS_KEY)) {
        for entry in items {
            let Tag::Compound(entry) = entry else {
                continue;
            };
            let slot = match entry.get(&JavaString::from(CONTAINER_KEY)) {
                Some(Tag::Byte(slot)) => *slot,
                _ => -1,
            };
            let contents = match entry.get(&JavaString::from(STORED_KEY)) {
                Some(Tag::Compound(stored)) => upgrade_slot_contents(game, stored),
                _ => SlotContents::default(),
            };
            match slot {
                0 => saved.input = contents,
                1 => saved.output = contents,
                _ => {}
            }
        }
    }
    saved
}

/// O `components` do `ItemResource.CODEC` de um recurso, ou nada sem componentes. Um recurso que veio de uma pilha do jogo tem o
/// patch inteiro. O que a lib faz sozinha (`UpgradeData.resource_of`, o item com o tipo e mais nada) nunca passou por uma pilha:
/// o tipo é o único componente e é um texto.
fn resource_components(resource: &ItemResource) -> Option<Tag> {
    if resource.components.is_empty() {
        return None;
    }
    if let Some(components) = components_tag(resource) {
        return Some(components);
    }
    let mut components = Compound::default();
    for (id, value) in resource.components.iter() {
        let Some(text) = Components::parse_text_value(value) else {
            eprintln!("machinery_basis: o componente {id} do upgrade não é um texto e não se grava");
            continue;
        };
        components.insert(JavaString::from(id), Tag::String(JavaString::from(text.as_str())));
    }
    Some(Tag::Compound(components))
}

/// `TileComponentUpgrade.serialize`: o filho some quando não tem nada.
pub(in crate::game) fn write_upgrades(entity: &mut ModBlockEntity, saved: &UpgradeSaved) {
    let mut component = Compound::default();
    if !saved.upgrades.is_empty() {
        let mut upgrades = Compound::default();
        for (id, total) in &saved.upgrades {
            upgrades.insert(JavaString::from(id.as_str()), Tag::Int(*total));
        }
        component.insert(JavaString::from(UPGRADES_KEY), Tag::Compound(upgrades));
    }
    let mut items = Vec::new();
    for (index, contents) in [&saved.input, &saved.output].into_iter().enumerate() {
        let Some(resource) = &contents.resource else {
            continue;
        };
        let mut kind = Compound::default();
        kind.insert(JavaString::from(ITEM_ID_KEY), Tag::String(JavaString::from(&*resource.id)));
        if let Some(components) = resource_components(resource) {
            kind.insert(JavaString::from(COMPONENTS_KEY), components);
        }
        let mut stored = Compound::default();
        stored.insert(JavaString::from(TYPE_KEY), Tag::Compound(kind));
        stored.insert(JavaString::from(AMOUNT_KEY), Tag::Long(i64::from(contents.amount)));
        let mut entry = Compound::default();
        entry.insert(JavaString::from(CONTAINER_KEY), Tag::Byte(index as i8));
        entry.insert(JavaString::from(STORED_KEY), Tag::Compound(stored));
        items.push(Tag::Compound(entry));
    }
    if !items.is_empty() {
        component.insert(JavaString::from(ITEMS_KEY), Tag::List(items));
    }
    let key = JavaString::from(COMPONENT_UPGRADE);
    if component.is_empty() {
        entity.nbt.remove(&key);
    } else {
        entity.nbt.insert(key, Tag::Compound(component));
    }
}

/// A pilha de um recurso de upgrade. O que a lib faz (`UpgradeData.resource_of`, ao remover um upgrade) é o item com o tipo no
/// componente `component` e mais nada, e nunca passou por uma pilha do jogo: monta-se aqui, e a pilha registrada é a do recurso a
/// partir de então.
pub(in crate::game) fn upgrade_stack(game: &Game<'_>, component: &str, resource: &ItemResource, count: u32) -> ItemStack {
    if let Some(stack) = stack_of_resource(resource, count) {
        return stack;
    }
    let mut stack = ItemStack::new(&resource.id, count);
    if let Some(kind) = resource.components.get(component).and_then(Components::parse_text_value) {
        let _ = game.native_set_component(&mut stack, component, Tag::String(JavaString::from(kind.as_str())));
    }
    let registered = item_resource(game, &stack);
    if registered != *resource {
        eprintln!("machinery_basis: o upgrade {:?} saiu com outros componentes ({:?})", resource.components.key(), registered.components.key());
    }
    stack
}

/// O item que um slot de upgrade guarda, com o tipo no componente `component`.
pub(in crate::game) fn upgrade_slot_stack(game: &Game<'_>, component: &str, contents: &SlotContents) -> Option<ItemStack> {
    let resource = contents.resource.as_ref()?;
    Some(upgrade_stack(game, component, resource, contents.amount))
}

/// O que o componente deixou num slot de upgrade (instalar tira da entrada, remover enche a saída), se a célula do menu não é mais
/// isso: a célula nova (`None` por dentro quando o slot ficou vazio). `None` por fora é a célula igual, que não se mexe.
pub(in crate::game) fn upgrade_cell_after(
    game: &Game<'_>,
    component: &str,
    cell: Option<&ItemStack>,
    contents: &SlotContents,
) -> Option<Option<ItemStack>> {
    let same = match (&contents.resource, cell) {
        (None, None) => true,
        (Some(resource), Some(stack)) => item_resource(game, stack) == *resource && contents.amount == stack.count,
        _ => false,
    };
    (!same).then(|| upgrade_slot_stack(game, component, contents))
}

/// Leva para a célula do menu o que o componente deixou no slot de upgrade, só quando mudou.
pub(in crate::game) fn upgrade_to_cell(game: &Game<'_>, component: &str, cell: &mut Option<ItemStack>, contents: &SlotContents) {
    if let Some(new) = upgrade_cell_after(game, component, cell.as_ref(), contents) {
        *cell = new;
    }
}

/// Insere até `amount` de `item` numa célula de item do jogo (um slot do bloco ou do inventário): só junta o mesmo recurso, o item e os
/// componentes, até a capacidade (`BasicResourceContainer.insert`), e uma célula vazia recebe o item. Quantos entraram. A regra de
/// aceitar é de quem chama.
pub(in crate::game) fn insert_into_cell(
    game: &Game<'_>,
    cell: &mut Option<ItemStack>,
    item: &ItemStack,
    amount: u32,
) -> u32 {
    let resource = item_resource(game, item);
    let held = slot_contents(game, cell.as_ref());
    let moved = amount.min(room_for(&held, &resource, resource.slot_capacity()));
    if moved == 0 {
        return 0;
    }
    match cell {
        Some(held) => held.count += moved,
        empty => {
            let mut placed = item.clone();
            placed.count = moved;
            *empty = Some(placed);
        }
    }
    moved
}

/// Tira `amount` de uma célula de item (`Slot.remove`): ela fica vazia ao chegar a zero.
pub(in crate::game) fn remove_from_cell(cell: &mut Option<ItemStack>, amount: u32) {
    let emptied = match cell {
        Some(held) => {
            held.count = held.count.saturating_sub(amount);
            held.count == 0
        }
        None => false,
    };
    if emptied {
        *cell = None;
    }
}

/// Leva o que couber de `stack` para o slot da máquina: o resto fica em `stack`. O slot só junta o que é o mesmo recurso, o item e
/// os componentes (`ItemStack.isSameItemSameComponents`).
pub(in crate::game) fn move_into_slot(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    slot: usize,
    stack: &mut ItemStack,
) {
    let resource = item_resource(game, stack);
    let held = slot_contents(game, entity.items[slot].as_ref());
    let moved = stack.count.min(room_for(&held, &resource, resource.slot_capacity()));
    match &mut entity.items[slot] {
        Some(held) => held.count += moved,
        empty => {
            let mut placed = stack.clone();
            placed.count = moved;
            if moved > 0 {
                *empty = Some(placed);
            }
        }
    }
    stack.count -= moved;
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

/// O arquivo de configuração que o jogo leu, como o `ModConfigSpec` o lê (`acceptConfig`): um valor errado vai para o limite ou para o padrão, e o arquivo é
/// escrito de novo, no lugar de onde foi lido (o do mundo, se um do mundo trocou o de `config/`), com as chaves que a especificação não tem como estavam. O
/// Horizon avisa da mudança no segundo seguinte, e o texto novo já está certo. `previous` é o que a leitura anterior do mesmo arquivo deu: um validador que
/// olha outra chave a vê pelo valor em cache (`Spec::load_after`).
pub(in crate::game) fn read_config(
    config: &crate::game::NativeConfigFile,
    spec: &machinery_basis_core::client_config::Spec,
    previous: Option<&machinery_basis_core::client_config::Loaded>,
) -> machinery_basis_core::client_config::Loaded {
    let loaded = spec.load_after(&config.text, previous);
    if loaded.corrected {
        // `ConfigFileTypeHandler`: o arquivo de antes fica em `<nome>-1.toml.bak` (até cinco), e só então o corrigido o substitui.
        let saved = machinery_basis_core::client_config::back_up(&config.path, machinery_basis_core::client_config::MAX_BACKUPS)
            .and_then(|()| std::fs::write(&config.path, spec.rewrite(&config.text, &loaded)));
        if let Err(error) = saved {
            eprintln!("machinery_basis: não consegui corrigir {}: {error}", config.path.display());
        }
    }
    loaded
}

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
    // Como o hopper: o primeiro item que o destino aceita, na ordem dos slots, para o primeiro slot de entrada que o aceita e tem
    // lugar (vazio, ou o mesmo recurso abaixo da capacidade).
    let offers: Vec<hopper::Offer<ItemStack>> = slots
        .iter()
        .filter_map(|s| {
            let stack = s.stack.as_ref().filter(|_| s.can_take)?;
            Some(hopper::Offer {
                slot: s.slot,
                resource: item_resource(game, stack),
                payload: stack.clone(),
            })
        })
        .collect();
    let held: Vec<(usize, SlotContents)> = inputs
        .iter()
        .map(|&slot| (slot, slot_contents(game, entity.items[slot].as_ref())))
        .collect();
    let Some((offer, into)) =
        hopper::first_taker(&offers, &held, |slot, offer| accepts(game, slot, &offer.payload))
    else {
        return false;
    };
    let Some(one) = game.native_extract_item(dimension, hopper_pos, reached, offers[offer].slot, 1) else {
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

/// O clima do bioma que `Level.getBiome(pos)` dá em `pos`: o `BiomeManager` não lê o quadrado de 4 por 4 que contém o bloco, e sim o
/// mais próximo de (x - 2, y - 2, z - 2) com um sorteio por quadrado (`BiomeZoom::quart_at_block`, do Horizon), e o quadrado pode
/// estar no chunk vizinho. `None` se o chunk dele não está carregado.
fn biome_climate(game: &Game<'_>, pos: [i32; 3]) -> Option<biome_temperature::Climate> {
    let [qx, qy, qz] = game.streaming.biome_zoom.quart_at_block(BlockPos::new(pos[0], pos[1], pos[2]));
    let [x, y, z] = [qx << 2, qy << 2, qz << 2];
    let chunk = game.world.chunks.get(&(x >> 4, z >> 4))?;
    let biome = &world_biomes!(game).list[chunk.biome(x, y, z) as usize];
    Some(biome_temperature::Climate { temperature: biome.temperature, frozen: biome.frozen })
}

/// `Level.getSeaLevel()` do nível em que o jogo está: o do gerador da dimensão (63, 32 e 0), ou -63 no overworld de um mundo plano
/// (`FlatLevelSource.getSeaLevel`). O Horizon sabe só qual predefinição o mundo tem (`world_flat`).
pub(in crate::game) fn sea_level(game: &Game<'_>) -> i32 {
    biome_temperature::sea_level(game.native_dimension(), game.world_flat)
}

/// A temperatura do ar numa posição (`HeatAPI.getAmbientTemp(level, pos)`): a temperatura do bioma ali com o ajuste por altura de
/// `Biome.getTemperature(pos, seaLevel)`, levada para kelvin. `None` se o chunk do bioma não está carregado (o Java o carregaria): quem
/// guarda o valor (`heat::AmbientCache`) não guarda esse caso.
pub(in crate::game) fn ambient_temperature(game: &Game<'_>, pos: [i32; 3]) -> Option<f64> {
    biome_climate(game, pos).map(|climate| climate.ambient_at(pos, sea_level(game)))
}

/// As configurações locais do cliente (`MekanismConfig.local`), uma por namespace: a unidade de temperatura que as abas de calor
/// mostram. É estado do cliente e vive num `static`, porque o desenho e o clique só têm `&Game`.
static LOCAL_UNITS: Mutex<Vec<(&'static str, temperature::LocalUnit)>> = Mutex::new(Vec::new());

/// Roda `f` com a unidade local do namespace `ns` (`config/<ns>/local.toml`), criando-a na primeira vez. `None` se o jogo não dá a
/// pasta de configuração.
fn with_local_unit<T>(ns: &'static str, f: impl FnOnce(&mut temperature::LocalUnit) -> T) -> Option<T> {
    let mut units = LOCAL_UNITS.lock().unwrap_or_else(|e| e.into_inner());
    if !units.iter().any(|(known, _)| *known == ns) {
        let path = Game::native_config_dir().ok()?.join(ns).join(temperature::LOCAL_CONFIG_FILE);
        units.push((ns, temperature::LocalUnit::new(path)));
    }
    units.iter_mut().find(|(known, _)| *known == ns).map(|(_, local)| f(local))
}

/// `MekanismConfig.local.tempUnit.get()`: a unidade de temperatura do cliente, relida do arquivo quando ele muda. O desenho e o
/// tooltip da aba de calor a leem a cada quadro.
pub(in crate::game) fn temperature_unit(ns: &'static str) -> temperature::TemperatureUnit {
    with_local_unit(ns, |local| local.get(Instant::now())).unwrap_or_default()
}

/// `MekanismConfig.local.holidays.get()`: se os feriados valem (o som de Natal), relido do arquivo quando ele muda.
pub(in crate::game) fn holidays_enabled(ns: &'static str) -> bool {
    with_local_unit(ns, |local| local.holidays(Instant::now())).unwrap_or(true)
}

/// `tempUnit.set(unit)` seguido de `save()`: vale no próximo quadro e fica no arquivo.
pub(in crate::game) fn set_temperature_unit(ns: &'static str, unit: temperature::TemperatureUnit) {
    with_local_unit(ns, |local| local.set(unit, Instant::now()));
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

/// `GuiInsetElement.drawBackground`, a cada quadro: o fundo do botão, o suporte (`holder_left` ou `holder_right`) tingido por `tint` por cima
/// dele, o fundo outra vez e o ícone de 18 por 18 sobre o botão (`GuiSideHolder.drawBackground` e depois a própria). Os sprites do fundo e do
/// suporte são `<ns>:gui/sprites/button`, `button_focus`, `button_inactive` e `holder_*`; `icon` é o nome inteiro do sprite do ícone.
/// `origin` é o canto da tela.
#[allow(clippy::too_many_arguments)]
pub(in crate::game) fn inset_tab(
    game: &Game<'_>,
    batch: &mut HudBatch,
    ns: &str,
    tab: machinery_basis_core::side_tab::InsetTab,
    origin: [f32; 2],
    tint: u32,
    icon: &str,
    look: machinery_basis_core::side_tab::ButtonLook,
) {
    let shift = |rect: gui::Rect| [origin[0] + rect[0], origin[1] + rect[1], rect[2], rect[3]];
    let button = shift(tab.button());
    let holder = shift(tab.holder());
    let background = format!("{ns}:gui/sprites/{}", look.sprite());
    let holder_sprite = format!("{ns}:gui/sprites/holder_{}", if tab.left { "left" } else { "right" });
    let nine = |batch: &mut HudBatch, sprite: &str, slice: NineSlice, rect: gui::Rect, argb: u32| {
        for cell in gui::nine_slice_cells(slice, rect) {
            sprite_tinted(game, batch, sprite, cell.src, cell.dst, argb);
        }
    };
    use machinery_basis_core::upgrade_window::{BUTTON_SLICE, HOLDER_SLICE};
    nine(batch, &background, BUTTON_SLICE, button, 0xFF_FF_FF_FF);
    nine(batch, &holder_sprite, HOLDER_SLICE, holder, tint);
    nine(batch, &background, BUTTON_SLICE, button, 0xFF_FF_FF_FF);
    let [_, _, w, h] = button;
    sprite_tinted(game, batch, icon, [0.0, 0.0, w, h], button, 0xFF_FF_FF_FF);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O nome de um tile (`TileEntityMekanism.customName`): o componente inteiro, do item ao tile, ao NBT, ao título da tela e ao item solto.

/// `SerializationConstants.CUSTOM_NAME`: o NBT do tile.
const CUSTOM_NAME: &str = "CustomName";

thread_local! {
    /// O nome do item que acabou de ser colocado, para o tile que ainda não nasceu: o Horizon cria o tile no primeiro tick.
    static PENDING_NAMES: std::cell::RefCell<HashMap<[i32; 3], Tag>> = std::cell::RefCell::new(HashMap::new());
    /// O nome do tile que o jogador está quebrando, guardado por [`name_destroying`] antes de o Horizon tirá-lo do mundo.
    static BROKEN_NAME: std::cell::RefCell<Option<([i32; 3], Option<Tag>)>> = const { std::cell::RefCell::new(None) };
}

/// O componente de nome que um tag guarda (`ComponentSerialization`): o texto puro (`"Camara"`), um componente com estilo (compound) ou uma
/// lista. O tag é o componente inteiro e passa sem ser lido, com cor, negrito e filhos.
pub(in crate::game) fn name_of_tag(tag: Option<&Tag>) -> Option<Tag> {
    match tag? {
        tag @ Tag::String(_) => Some(tag.clone()),
        tag @ Tag::Compound(compound) if !compound.is_empty() => Some(tag.clone()),
        tag @ Tag::List(list) if !list.is_empty() => Some(tag.clone()),
        _ => None,
    }
}

/// O `CustomName` que o tile em `pos` guarda, se o bloco ali é de `block_ids` (`is_ours`): o componente inteiro.
pub(in crate::game) fn saved_name(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) -> Option<Tag> {
    use horizon_client::containers::BlockEntity;
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if is_ours(&entity.id) => name_of_tag(entity.nbt.get(&JavaString::from(CUSTOM_NAME))),
        _ => None,
    }
}

/// `getDisplayName` na tela: o nome que o servidor mandou ao abrir, ou a descrição do contêiner (`container_description` é a chave de
/// tradução). O componente chega com o estilo.
pub(in crate::game) fn screen_title(game: &Game<'_>, container_description: &str) -> horizon_client::text::Text {
    use horizon_client::text::Text;
    let name = game.mb_open_screen_pos().and_then(|pos| crate::game::machinery_tile_sound::tile_name(game.native_dimension(), pos));
    let tile = {
        let mut tile = machinery_basis_core::name::TileName::<Text>::new(true);
        tile.load(name.as_ref().map(horizon_client::component::name_text));
        tile
    };
    let description = Text::literal(horizon_client::lang::get(container_description));
    tile.display_name(&description, &description).clone()
}

/// `applyImplicitComponents`: o item colocado tinha `custom_name`, e o tile o guarda (`saveAdditional`), o componente inteiro.
pub(in crate::game) fn name_placed(game: &Game<'_>, stack: &ItemStack, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    if !is_ours(&game.native_block(pos)) {
        return;
    }
    // `minecraft:custom_name` não é um componente de mod: o componente está nos dados da pilha.
    let item_name = stack.extra.as_ref().and_then(|data| name_of_tag(data.custom_name.as_ref()));
    let mut tile = machinery_basis_core::name::TileName::<Tag>::new(true);
    tile.apply_implicit_components(item_name);
    if let Some(name) = tile.saved() {
        PENDING_NAMES.with_borrow_mut(|names| names.insert(pos, name.clone()));
    } else {
        PENDING_NAMES.with_borrow_mut(|names| names.remove(&pos));
    }
}

/// O tile nasceu: o nome do item que o colocou entra no NBT dele.
pub(in crate::game) fn name_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3]) {
    if let Some(name) = PENDING_NAMES.with_borrow_mut(|names| names.remove(&pos)) {
        entity.nbt.insert(JavaString::from(CUSTOM_NAME), name);
    }
}

/// O jogador (ou uma explosão que passa pelo hook) vai quebrar o tile em `pos`: o Horizon o tira do mundo antes de rolar a tabela de loot, e
/// o nome que o item solto leva (`copy_name`) é guardado aqui, enquanto o tile ainda existe.
pub(in crate::game) fn name_destroying(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    let name = saved_name(game, pos, is_ours);
    BROKEN_NAME.with_borrow_mut(|slot| *slot = Some((pos, name)));
}

/// `copy_name` da tabela de loot: o item solto leva o componente de nome do tile, inteiro. Um tile que ainda está no mundo dá o nome dele;
/// o que foi quebrado, o que [`name_destroying`] guardou.
pub(in crate::game) fn name_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    is_ours: &dyn Fn(&str) -> bool,
) {
    let Some((pos, _)) = context.block else {
        return;
    };
    // Cada mod com nome chama isto: o nome guardado só sai para quem tem o item entre os que caíram (os outros mods o veem depois).
    if !drops.iter().any(|stack| is_ours(&stack.item)) {
        return;
    }
    let kept = BROKEN_NAME.with_borrow_mut(|slot| match slot {
        Some((at, _)) if *at == pos => slot.take().and_then(|(_, name)| name),
        _ => None,
    });
    let Some(name) = saved_name(game, pos, is_ours).or(kept) else {
        return;
    };
    for stack in drops.iter_mut().filter(|stack| is_ours(&stack.item)) {
        stack.extra.get_or_insert_with(Default::default).custom_name = Some(name.clone());
    }
}

/// O jogador abriu o contêiner de um tile: o nome vai com ele (`getDisplayName` no pacote de abertura).
pub(in crate::game) fn name_container_opened(game: &mut Game<'_>, player: u128, pos: Option<[i32; 3]>, is_ours: &dyn Fn(&str) -> bool) {
    let Some(pos) = pos else {
        return;
    };
    if !is_ours(&game.native_block(pos)) {
        return;
    }
    let name = saved_name(game, pos, is_ours);
    let dimension = game.native_dimension();
    game.mb_name_open(player, dimension, pos, name.as_ref());
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
/// `renderInventoryText`), o título centrado e "Inventory" à esquerda, os dois em `SpecialColors.TEXT_TITLE` e sem sombra (o título, se
/// tem estilo, com o dele).
pub(in crate::game) fn machine_panel(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &PanelStyle,
    rect: [f32; 4],
    title: &horizon_client::text::Text,
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
    // O título é o componente inteiro (`drawScrollingString` com `text.getVisualOrderText()`): a cor e o negrito do próprio texto valem
    // sobre `TEXT_TITLE`, e a largura que o centra é a do texto com o estilo.
    let atlas = &game.gui.atlas;
    let title_x = x + gui::title_x(width, atlas.font.width_text(title));
    batch.text_styled(atlas, title, title_x, y + gui::TITLE_LABEL_Y, hud_color(0xFF_00_00_00 | gui::TITLE_TEXT_COLOR), false);
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

// ---------------------------------------------------------------------------------------------------------------------------
// O sinal de redstone dos vizinhos (`TileEntityMekanism.updatePower`). Só muda quando o jogo avisa o bloco; veja
// `machinery_basis_core::neighbors`.
// ---------------------------------------------------------------------------------------------------------------------------

type PowerKey = (&'static str, [i32; 3]);

/// Quem o jogo avisou desde a última leitura, na ordem em que avisou. Quem avisa e quem lê podem ser threads diferentes (o servidor de
/// um jogador só), por isso não é `thread_local!`.
static NOTIFIED: Mutex<Vec<PowerKey>> = Mutex::new(Vec::new());
/// O que `flush_neighbor_signals` leu e o dono do tile ainda não pegou (`take_power`).
static PENDING_POWER: LazyLock<Mutex<PendingPower<PowerKey>>> = LazyLock::new(Default::default);
/// Os blocos que têm `redstone` como o `TileEntityMekanism`, por dono (um mod): cada um diz os seus por nome.
static REDSTONE_OWNERS: Mutex<Vec<(&'static str, fn(&str) -> bool)>> = Mutex::new(Vec::new());

/// Um mod diz quais dos seus blocos são `TileEntityMekanism` com `supportsRedstone()`. Pode ser dito mais de uma vez.
pub(in crate::game) fn watch_redstone(owner: &'static str, is_ours: fn(&str) -> bool) {
    let mut owners = REDSTONE_OWNERS.lock().unwrap_or_else(|e| e.into_inner());
    if !owners.iter().any(|(known, _)| *known == owner) {
        owners.push((owner, is_ours));
    }
}

fn takes_redstone(block: &str) -> bool {
    REDSTONE_OWNERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|(_, is_ours)| is_ours(block))
}

/// `BlockTile.neighborChanged`: o jogo avisou o bloco em `pos`. Só anota quem; o sinal é lido no fim da operação em andamento
/// (`flush_neighbor_signals`), porque o gancho não tem como ler o mundo.
pub(in crate::game) fn neighbor_changed(
    level: &GameLevel<'_>,
    dimension: &'static str,
    pos: BlockPos,
    state: BlockStateId,
) {
    crate::game::machinery_liquid::note(level, dimension, pos, state);
    if level.native_state_block(state).is_some_and(takes_redstone) {
        NOTIFIED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((dimension, [pos.x, pos.y, pos.z]));
    }
}

/// Lê o sinal dos vizinhos de cada bloco avisado (`level.hasNeighborSignal(pos)`) e guarda para o dono. Roda no fim de cada operação no
/// mundo (`Game::flush_spill`, o último passo de `with_level`) e antes de o dono ler o `redstone`, então o valor é o do mundo no fim da
/// operação que avisou, e não o de um tick depois. Um aviso de outra dimensão espera a vez dela.
pub(in crate::game) fn flush_neighbor_signals(game: &mut Game<'_>) {
    crate::game::machinery_liquid::flush(game);
    let notified = std::mem::take(&mut *NOTIFIED.lock().unwrap_or_else(|e| e.into_inner()));
    if notified.is_empty() {
        return;
    }
    let here = game.native_dimension();
    let mut later = Vec::new();
    for key in notified {
        if key.0 != here {
            later.push(key);
            continue;
        }
        record_power(game, key);
    }
    if !later.is_empty() {
        NOTIFIED.lock().unwrap_or_else(|e| e.into_inner()).extend(later);
    }
}

/// `updatePower`: guarda o sinal de agora se há um tile do mod em `key`. Sem tile o Java não faz nada (`tile != null`).
fn record_power(game: &mut Game<'_>, key: PowerKey) {
    let (dimension, pos) = key;
    let ours = matches!(
        game.world.block_entities.get(&(pos[0], pos[1], pos[2])),
        Some(horizon_client::containers::BlockEntity::Mod(entity)) if takes_redstone(&entity.id)
    );
    if !ours {
        return;
    }
    let power = game.native_neighbor_signal(dimension, pos) > 0;
    PENDING_POWER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(key, power);
}

/// O que `updatePower` deixou para o tile de `pos`: o dono chama antes de ler o `redstone`, uma vez por tick.
pub(in crate::game) fn take_power(game: &mut Game<'_>, pos: [i32; 3]) -> Option<bool> {
    flush_neighbor_signals(game);
    let key = (game.native_dimension(), pos);
    PENDING_POWER.lock().unwrap_or_else(|e| e.into_inner()).take(&key)
}

/// O que `updatePower` deixou para o tile de `pos`, sem tirá-lo da fila: o menu de um tile que ainda não andou o mostra, e o primeiro tick dele o pega.
pub(in crate::game) fn peek_power(game: &mut Game<'_>, pos: [i32; 3]) -> Option<bool> {
    flush_neighbor_signals(game);
    let key = (game.native_dimension(), pos);
    let mut pending = PENDING_POWER.lock().unwrap_or_else(|e| e.into_inner());
    let power = pending.take(&key);
    if let Some(power) = power {
        pending.record(key, power);
    }
    power
}

/// `TileEntityMekanism.onAdded`, de `BlockMekanism.setPlacedBy`: o bloco posto com um item lê o sinal na hora.
pub(in crate::game) fn on_added(game: &mut Game<'_>, pos: [i32; 3]) {
    let key = (game.native_dimension(), pos);
    let power = game.native_neighbor_signal(key.0, pos) > 0;
    PENDING_POWER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(key, power);
}

/// `Level.updateNeighbourForOutputSignal` do NeoForge, para o que o mod mesmo faz (`notifyComparatorChange`): cada vizinho com tile do mod
/// recebe `onNeighborChange`, na ordem de `OUTPUT_SIGNAL_ORDER`, e lê o sinal na hora (`updatePower`). O Horizon só avisa os
/// comparadores do vanilla.
pub(in crate::game) fn on_neighbor_change(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    for face in OUTPUT_SIGNAL_ORDER {
        let [x, y, z] = face.offset();
        let neighbor = [pos[0] + x, pos[1] + y, pos[2] + z];
        record_power(game, (dimension, neighbor));
    }
}

/// `BlockEntity.setChanged` de um block entity do vanilla com contêiner (`Game::container_changed`): o NeoForge avisa os vizinhos pelo
/// `updateNeighbourForOutputSignal` (`BlockEntity.java:216-221`), e as máquinas do Mekanism releem o sinal (`onNeighborChange`). Um tile de mod fica de fora: o
/// `TileEntityMekanism.setChanged` não avisa ninguém (`TileEntityUpdateable.java:89-100`), só o nível do comparador avisa (`notify_comparators`). As duas metades de um
/// baú duplo mudam juntas (`CompoundContainer.setChanged`), cada uma com os vizinhos dela.
pub(in crate::game) fn container_changed(game: &mut Game<'_>, pos: (i32, i32, i32)) {
    let dimension = game.native_dimension();
    let halves: Vec<(i32, i32, i32)> = std::iter::once(pos).chain(game.chest_other_half(pos)).collect();
    for half in halves {
        let vanilla = game
            .world
            .block_entities
            .get(&half)
            .is_some_and(|entity| !matches!(entity, horizon_client::containers::BlockEntity::Mod(_)));
        if vanilla {
            on_neighbor_change(game, dimension, [half.0, half.1, half.2]);
        }
    }
}

/// `MekanismConfig.client.machineEffects` (`ClientConfig.java:92`, `rendering.particle.machineEffects`), como o último arquivo de cliente o deixou. Fica
/// aqui porque quem lê é cada mod com blocos que soltam partículas (as máquinas e o Heat Generator usam a mesma chave do Mekanism).
static MACHINE_EFFECTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// `ModConfigEvent.Loading` e `Reloading` do arquivo de cliente.
pub(in crate::game) fn set_machine_effects(enabled: bool) {
    MACHINE_EFFECTS.store(enabled, Ordering::Relaxed);
}

/// Escreve no stderr cada chamada que soltou partículas, com o tick do jogo. Só as cópias de teste ligam.
const FX_TRACE: bool = false;

/// `ParticleOptions` de uma partícula de `AttributeParticleFX`.
fn fx_options(particle: machinery_basis_core::active_fx::Particle) -> horizon_client::particles::ParticleOptions {
    use horizon_client::particles::{ParticleOptions, ParticleType};
    use machinery_basis_core::active_fx::Particle;
    match particle {
        Particle::Smoke => ParticleOptions::simple(ParticleType::Smoke),
        Particle::Flame => ParticleOptions::simple(ParticleType::Flame),
        // `DustParticleOptions(color, scale)`: `ARGB.vector3fFromRGB24`.
        Particle::Dust { rgb, scale } => ParticleOptions::dust(
            [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0],
            scale,
        ),
    }
}

/// O motor de partículas como o `Level` de `BlockTile.animateTick`: o gerador do sorteio das posições e o `addParticle`.
struct FxLevel<'a>(&'a mut horizon_client::particles::ParticleEngine);

impl machinery_basis_core::active_fx::ParticleLevel for FxLevel<'_> {
    fn next_double(&mut self) -> f64 {
        // `BitRandomSource.nextDouble`: o `next_double` do Horizon faz a conta em `double` (2^-53), e o jogo em `float` (`long * 1.110223E-16F`). Com um expoente de potência de dois,
        // `nextInt(1 << bits)` é `next(bits)` (`BitRandomSource.nextInt(int)`, `(bound * next(31)) >> 31`), um passo do gerador cada.
        use horizon_core::random::RandomSource;
        let random = self.0.random();
        let high = random.next_int_bound(1 << 26).unwrap_or(0);
        let low = random.next_int_bound(1 << 27).unwrap_or(0);
        machinery_basis_core::active_fx::bit_random_double(high, low)
    }

    fn add_particle(&mut self, spawn: machinery_basis_core::active_fx::Spawn) {
        self.0.add(&fx_options(spawn.particle), spawn.pos, [0.0; 3]);
    }
}

/// `BlockTile.animateTick` (`BlockTile.java:80-103`), para o bloco sorteado pelo `ClientLevel.animateTick`: `fx_of` devolve o `AttributeParticleFX` do bloco
/// (ou `None` se ele não tem). Com `machineEffects` ligado e o bloco ativo (`lit`), solta uma partícula por entrada, na posição que ela sorteia, girada pelo
/// `facing` e somada ao centro do bloco. O gerador é o do motor de partículas, o mesmo do sorteio das posições.
pub(in crate::game) fn animate_active_block(
    engine: &mut horizon_client::particles::ParticleEngine,
    ctx: &horizon_client::animate_tick::AnimateContext<'_>,
    state: BlockStateId,
    pos: [i32; 3],
    fx_of: &dyn Fn(&str) -> Option<&'static [machinery_basis_core::active_fx::FxParticle]>,
) {
    use machinery_basis_core::active_fx::{Facing, animate_tick, center_of};
    let Some(row) = ctx.registry.state(state) else {
        return;
    };
    let Some(fx) = fx_of(&row.name) else {
        return;
    };
    let active = row.properties.get("lit").map(String::as_str) == Some("true");
    let facing = Facing::from_property(row.properties.get("facing").map_or("", String::as_str));
    let effects = MACHINE_EFFECTS.load(Ordering::Relaxed);
    animate_tick(fx, facing, center_of(pos), active, effects, &mut FxLevel(engine));
    if FX_TRACE && active && effects {
        eprintln!("MEKFX gt={} block={} pos={pos:?} facing={facing:?} n={}", ctx.world.game_time, row.name, fx.len());
    }
}
