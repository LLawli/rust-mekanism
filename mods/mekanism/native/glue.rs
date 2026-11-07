// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::{Game, GameLevel, NativeReloadListener, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::crafting::{Ingredient, Shape};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemCache, ItemToItemRecipe};
use machinery_basis_core::chemical::{
    ChemicalConversion, ChemicalIngredient, ChemicalResource, ConversionCache, TankContents, can_fill_or_convert_with,
};
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource, SlotContents};
use machinery_basis_core::data_slots;
use machinery_basis_core::gui;
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::resource;
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::tile::{
    ProgressMachine, RECIPE_CHECK_FREQUENCY, RedstoneControl, SavedState, machine_energy_slot_accepts,
};
use machinery_basis_core::quick_move::{self, QuickMenu, Region};
use machinery_basis_core::recipe_cache::{IngredientShape, ShapedIngredient, TypedInstance};
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use machinery_basis_core::upgrade_window::{UpgradeSync, UpgradeWindowSpec};
use machinery_basis_core::window::SelectedWindow;
use machinery_basis_core::upgrade::{self, UpgradeSaved, UpgradeTags, UpgradeTypes};
use machinery_basis_core::slot_view::{SlotOverlay, SlotType};
use basis::RecipeSource::{Cooking, Custom};
use std::cell::{Cell, RefCell};
use horizon_core::geometry::Direction;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DUMP_BUTTON: i32 = 0;
/// `GameType.CREATIVE` como o Horizon o numera.
const CREATIVE: i32 = 1;
const INFUSING: basis::RecipeSource = Custom(rules::INFUSING, rules::INFUSING_FIELDS);

/// Onde cada slot fica no `ModContainer`, na ordem de `getInitialInventory` de cada máquina.
struct Slots {
    input: usize,
    output: usize,
    energy: usize,
    /// O slot que enche o tanque de químico, nas máquinas que têm tanque.
    infusion: Option<usize>,
    /// Os dois slots virtuais da janela de upgrades (`TileComponentUpgrade`): entrada e saída.
    upgrade: usize,
    upgrade_output: usize,
}

/// Uma máquina de progresso: as quatro só mudam no id, nas receitas, no consumo e nos slots.
struct Machine {
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
    slots: Slots,
    /// Os data slots da tela e os avisos dela.
    screen: &'static rules::ScreenSpec,
}

const ELECTRIC_SLOT_LAYOUT: Slots = Slots {
    input: rules::ELECTRIC_INPUT_SLOT,
    output: rules::ELECTRIC_OUTPUT_SLOT,
    energy: rules::ELECTRIC_ENERGY_SLOT,
    infusion: None,
    upgrade: rules::ELECTRIC_UPGRADE_SLOT,
    upgrade_output: rules::ELECTRIC_UPGRADE_OUTPUT_SLOT,
};

const fn electric(
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
) -> Machine {
    Machine {
        block,
        title,
        recipes,
        energy_per_tick,
        slots: ELECTRIC_SLOT_LAYOUT,
        screen: &rules::ELECTRIC_SCREEN,
    }
}

static MACHINES: [Machine; 4] = [
    electric(
        rules::ENRICHMENT_CHAMBER,
        "container.mekanism.enrichment_chamber",
        Custom(rules::ENRICHING, rules::ELECTRIC_FIELDS),
        rules::ENRICHMENT_USAGE,
    ),
    electric(
        rules::CRUSHER,
        "container.mekanism.crusher",
        Custom(rules::CRUSHING, rules::ELECTRIC_FIELDS),
        rules::CRUSHER_USAGE,
    ),
    electric(
        rules::ENERGIZED_SMELTER,
        "container.mekanism.energized_smelter",
        Cooking(rules::SMELTING),
        rules::SMELTER_USAGE,
    ),
    Machine {
        block: rules::INFUSER,
        title: "container.mekanism.metallurgic_infuser",
        recipes: INFUSING,
        energy_per_tick: rules::INFUSER_USAGE,
        slots: Slots {
            input: rules::INPUT_SLOT,
            output: rules::OUTPUT_SLOT,
            energy: rules::ENERGY_SLOT,
            infusion: Some(rules::INFUSION_SLOT),
            upgrade: rules::UPGRADE_SLOT,
            upgrade_output: rules::UPGRADE_OUTPUT_SLOT,
        },
        screen: &rules::INFUSER_SCREEN,
    },
];

// As telas, nas posições de GuiMetallurgicInfuser e GuiElectricMachine. Um slot guarda a posição do item, e a moldura fica um
// pixel antes (`GuiMekanism.addSlots`).
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const SLOT: &str = "mekanism:gui/sprites/slot/normal";
/// `ProgressType.RIGHT` e `ProgressType.BAR`: o sprite vazio, o cheio e o do aviso.
const PROGRESS_RIGHT: basis::ProgressSprites = basis::ProgressSprites {
    empty: "mekanism:gui/sprites/progress/right/empty",
    full: "mekanism:gui/sprites/progress/right/full",
    warning: "mekanism:gui/sprites/progress/right/warning",
};
const PROGRESS_BAR_TYPE: basis::ProgressSprites = basis::ProgressSprites {
    empty: "mekanism:gui/sprites/progress/bar/empty",
    full: "mekanism:gui/sprites/progress/bar/full",
    warning: "mekanism:gui/sprites/progress/bar/warning",
};
/// O pixel de moldura que `ProgressType.BAR` tem de cada lado e `RIGHT` não tem.
const PROGRESS_BAR_FRAME: f32 = 1.0;
const PROGRESS_RIGHT_FRAME: f32 = 0.0;
/// `GuiSlot.WARNING`, no lugar da moldura do tipo do slot.
const SLOT_WARNING: &str = "mekanism:gui/sprites/slot/warning";
/// `GuiElement.WARNING_BACKGROUND_TEXTURE` e `WARNING_TEXTURE`: `warning_background.png.mcmeta` e `warning.png.mcmeta` dizem
/// `gui.scaling` do tipo `tile`, 16 por 16.
const WARNING_SPRITES: basis::WarningSprites = basis::WarningSprites {
    background: "mekanism:gui/sprites/warning_background",
    overlay: "mekanism:gui/sprites/warning",
    tile: [16.0, 16.0],
};
/// `GuiWarningTab.WARNING_LEFT`.
const WARNING_TAB_SPRITE: &str = "mekanism:gui/sprites/tab/warning_info_left";
const UP_ARROW: &str = "mekanism:gui/sprites/arrow/up";
const DUMP_SPRITE: &str = "mekanism:gui/sprites/button/dump";
const PANEL: basis::PanelStyle = basis::PanelStyle {
    base: "mekanism:gui/sprites/base",
    base_slice: rules::PANEL_SLICE,
    slot: SLOT,
};
/// `GuiBar.BAR` (`bar/base`) e o sprite de `GuiVerticalPowerBar` (`bar/vertical_power`, em mosaico de `POWER_BAR_TILE`).
const BAR_STYLE: basis::BarStyle = basis::BarStyle {
    frame: "mekanism:gui/sprites/bar/base",
    frame_slice: rules::BAR_FRAME_SLICE,
};
const POWER_BAR_SPRITE: &str = "mekanism:gui/sprites/bar/vertical_power";
/// Os elementos, do tamanho do `GuiElement` (a barra soma dois à largura e à altura que o Java passa, para a moldura).
const INFUSION_BAR: [f32; 4] = [7.0, 15.0, 6.0, 54.0];
const ENERGY_BAR: [f32; 4] = [164.0, 15.0, 6.0, 54.0];
const ELECTRIC_ENERGY_BAR: [f32; 4] = [164.0, 16.0, 6.0, 54.0];
const INFUSER_PROGRESS: [f32; 4] = [72.0, 47.0, 32.0, 8.0];
const ELECTRIC_PROGRESS_BAR: [f32; 4] = [86.0, 38.0, 25.0, 9.0];
const ELECTRIC_ARROW: [f32; 4] = [68.0, 38.0, 8.0, 10.0];
const DUMP_RECT: [f32; 4] = [16.0, 59.0, 21.0, 10.0];
/// O lado do quadro de um sprite de químico: a tira tem quadros de 16 por 16.
const CHEMICAL_FRAME: f32 = 16.0;

/// O sprite da moldura de um slot (`SlotType.texture`). Só os tipos que estas telas usam têm arquivo.
fn slot_sprite(slot: SlotType) -> &'static str {
    match slot {
        SlotType::Power => "mekanism:gui/sprites/slot/power",
        SlotType::Extra => "mekanism:gui/sprites/slot/extra",
        SlotType::Input => "mekanism:gui/sprites/slot/input",
        SlotType::Output => "mekanism:gui/sprites/slot/output",
        _ => SLOT,
    }
}

/// O sprite que vai por cima da moldura (`SlotOverlay.texture`). Só o do slot de energia tem arquivo.
fn overlay_sprite(overlay: SlotOverlay) -> Option<&'static str> {
    (overlay == SlotOverlay::Power).then_some("mekanism:gui/sprites/slot/overlay/power")
}

fn is_infuser(block: &str) -> bool {
    block == rules::INFUSER
}

fn machine_of(block: &str) -> Option<&'static Machine> {
    MACHINES.iter().find(|machine| machine.block == block)
}

/// As máquinas de um item só, com a tela de `GuiElectricMachine`.
fn electric_machine(block: &str) -> Option<&'static Machine> {
    machine_of(block).filter(|machine| machine.slots.infusion.is_none())
}

fn is_machine(block: &str) -> bool {
    machine_of(block).is_some()
}

fn is_cable(block: &str) -> bool {
    rules::cable_capacity(block).is_some()
}

fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    gui::contains(rect, at)
}

fn sprite_rect(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, rect: [f32; 4]) {
    let [_, _, w, h] = rect;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], rect);
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if (is_machine(block) || is_cable(block)) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_machine(block) || is_cable(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if let Some(capacity) = rules::cable_capacity(block) {
        *energy = Some(ModEnergyBuffer {
            capacity,
            max_insert: capacity,
            insert_faces: [true; 6],
            ..Default::default()
        });
        return;
    }
    let Some(machine) = machine_of(block) else {
        return;
    };
    let usage = machine.energy_per_tick;
    // `LightConfigInfo.INPUT_ONLY`: energia entra por todas as faces.
    *energy = Some(ModEnergyBuffer {
        capacity: rules::energy_capacity(usage),
        max_insert: rules::energy_capacity(usage),
        insert_faces: [true; 6],
        ..Default::default()
    });
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    let slots = match machine_of(block) {
        Some(machine) if machine.slots.infusion.is_some() => rules::MENU_SLOTS,
        Some(_) => rules::ELECTRIC_MENU_SLOTS,
        None => return,
    };
    // Sem interface vanilla: hoppers passam pela regra da machinery_basis.
    *container = Some(ModContainer {
        slots,
        menu: Some(block.to_owned()),
        ..Default::default()
    });
}

fn native_screen(machine: &Machine) -> NativeScreen {
    // Os slots virtuais da janela de upgrades vêm depois dos da máquina e ficam escondidos até a janela abrir.
    let mut slots: Vec<Option<[f32; 2]>> = machine.screen.slots.iter().map(|slot| Some(slot.item)).collect();
    slots.extend([None, None]);
    NativeScreen {
        width: WIDTH,
        height: HEIGHT,
        slots,
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: true,
        own_background: true,
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if let Some(machine) = machine_of(block) {
        *screen = Some(native_screen(machine));
        if let Some(spec) = window_spec(block) {
            Game::mb_window_register(spec, remove_upgrade);
        }
    }
}

// A janela de upgrades de cada máquina: só dados. A mecânica (aba, janela, slots virtuais) é da machinery_basis.
const fn window_spec_of(block: &'static str, slots: &Slots, tracked_base: usize, layout: &rules::DataLayout) -> UpgradeWindowSpec {
    UpgradeWindowSpec {
        block,
        virtual_slots: [slots.upgrade, slots.upgrade_output],
        tracked_base,
        tracking_key: rules::UPGRADE_TRACKING_KEY,
        tracked_items: rules::UPGRADE_TRACKED_ITEMS,
        sync_offset: layout.upgrade,
        sync_len: rules::UPGRADE_SYNC_LEN,
        ns: "mekanism",
        window_type: rules::UPGRADE_WINDOW,
        window_types: &rules::WINDOW_TYPES,
        base_slice: rules::PANEL_SLICE,
        max_multiplier: rules::UPGRADE_MULTIPLIER as i32,
        data: window_upgrade_data,
        color: rules::enum_color,
        item: rules::UPGRADE_ITEM,
        item_component: rules::UPGRADE_TYPE_COMPONENT,
    }
}

fn window_upgrade_data() -> Rc<upgrade::UpgradeData> {
    rules::upgrade_data(&upgrade::current_types(), &upgrade::current_tags(), UNKNOWN_STACK)
}

static WINDOW_SPECS: [UpgradeWindowSpec; 4] = [
    window_spec_of(rules::ENRICHMENT_CHAMBER, &ELECTRIC_SLOT_LAYOUT, rules::ELECTRIC_TRACKED_BASE, &rules::ELECTRIC_DATA),
    window_spec_of(rules::CRUSHER, &ELECTRIC_SLOT_LAYOUT, rules::ELECTRIC_TRACKED_BASE, &rules::ELECTRIC_DATA),
    window_spec_of(rules::ENERGIZED_SMELTER, &ELECTRIC_SLOT_LAYOUT, rules::ELECTRIC_TRACKED_BASE, &rules::ELECTRIC_DATA),
    window_spec_of(
        rules::INFUSER,
        &Slots {
            input: rules::INPUT_SLOT,
            output: rules::OUTPUT_SLOT,
            energy: rules::ENERGY_SLOT,
            infusion: Some(rules::INFUSION_SLOT),
            upgrade: rules::UPGRADE_SLOT,
            upgrade_output: rules::UPGRADE_OUTPUT_SLOT,
        },
        rules::INFUSER_TRACKED_BASE,
        &rules::INFUSER_DATA,
    ),
];

fn window_spec(block: &str) -> Option<&'static UpgradeWindowSpec> {
    WINDOW_SPECS.iter().find(|spec| spec.block == block)
}

/// `PacketRemoveUpgrade.handle`: o servidor tira um ou todos os upgrades do tipo da máquina em `pos`.
fn remove_upgrade(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, all: bool) {
    let key = (game.native_dimension(), pos);
    let Some(mut live) = LIVE_MACHINES.with_borrow_mut(|machines| machines.remove(&key)) else {
        return;
    };
    live.machine.upgrades().remove_upgrade(upgrade, all);
    let version = live.machine.upgrades_version();
    if live.upgrades_version != Some(version) {
        // O que saiu vai também para os slots do menu: o próximo tick lê o menu primeiro, e um slot de saída velho apagaria o que entrou.
        let at = (pos[0], pos[1], pos[2]);
        let id = match game.world.block_entities.get(&at) {
            Some(BlockEntity::Mod(entity)) => entity.id.clone(),
            _ => String::new(),
        };
        let stacks = machine_of(&id).map(|machine| {
            let upgrades = live.machine.upgrades();
            (
                machine.slots.upgrade,
                upgrade_slot_stack(game, upgrades.upgrade_slot()),
                machine.slots.upgrade_output,
                upgrade_slot_stack(game, upgrades.upgrade_output_slot()),
            )
        });
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
            write_upgrades(entity, &live.machine.upgrades().saved());
            if let Some((input, input_stack, output, output_stack)) = stacks
                && entity.items.len() > output.max(input)
            {
                entity.items[input] = input_stack;
                entity.items[output] = output_stack;
            }
        }
        live.upgrades_version = Some(version);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
    LIVE_MACHINES.with_borrow_mut(|machines| machines.insert(key, live));
}

// Os objetos vivos das máquinas de progresso (o desenho do Java, com contêineres e receita em cache como objetos).
//
// Moram aqui, numa tabela por thread, uma por block entity, na chave (dimensão, posição). O Horizon entrega a block entity
// por `&mut` só durante o tick, e `ModBlockEntity` guarda `Items`, `nbt` e `data`, sem lugar para um objeto vivo: então o
// objeto é um cache, e a verdade que sobrevive é a `ModBlockEntity`.
//
// - Nasce no primeiro tick do bloco, ou no primeiro depois de uma lacuna (o chunk saiu, o mundo foi salvo e reaberto, o bloco
//   foi trocado): refeito dos itens, da energia e do progresso que a `ModBlockEntity` tem. É o `deserialize` do Java.
// - Vive enquanto o tick seguinte for o do tick seguinte do jogo. Todo tick começa lendo da `ModBlockEntity` o que mudou
//   fora dele (a tela, um hopper, um cabo), como um `setContents` do Java, que dispara os listeners; e termina escrevendo o
//   que mudou, em tags simples com os nomes que o Java grava. Salvar, abrir a tela e mandar energia funcionam sem o objeto saber.
// - Morre pela varredura: sem andar por `STALE_TICKS`, o que acontece com bloco quebrado ou chunk descarregado, o objeto sai
//   da tabela. Um bloco novo na mesma posição não herda nada: sem a chave `redstone` na `ModBlockEntity` (o Java a grava sempre), nasce outro.
thread_local! {
    static LIVE_MACHINES: RefCell<HashMap<(&'static str, [i32; 3]), Live>> = RefCell::new(HashMap::new());
    static COMPARATOR_LEVELS: RefCell<HashMap<(&'static str, [i32; 3]), i32>> = RefCell::new(HashMap::new());
    static COMPARATOR_QUEUE: RefCell<Vec<(&'static str, [i32; 3])>> = const { RefCell::new(Vec::new()) };
    // Quantos jogadores têm a tela aberta, por bloco, como o `mod_container_viewers` disse por último. Existe mesmo sem objeto
    // vivo: a abertura que chega antes do primeiro tick, ou depois de a varredura tirar o objeto, é contada quando ele nasce.
    static VIEWERS: RefCell<HashMap<(&'static str, [i32; 3]), u32>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// Os nomes de `SerializationConstants` que o Java grava, em tags simples no NBT da `ModBlockEntity`. `REDSTONE` é a
/// marca de que o tile já foi salvo: o Java a escreve sempre.
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const ACTIVE_STATE: &str = "active_state";
const UPDATE_DELAY: &str = "update_delay";
const CURRENT_REDSTONE: &str = "current_redstone";
const PROGRESS: &str = "progress";
const USED_SO_FAR: &str = "used_so_far";
const RECIPES_USED: &str = "recipes_used";
const LAST_RECIPE_USED: &str = "last_recipe_used";
const LAST_RECIPE_AMOUNT: &str = "last_recipe_amount";
/// `SerializationConstants.EJECT + TransmissionType.ITEM.ordinal()`: ENERGY, FLUID, CHEMICAL, ITEM.
const EJECT_ITEM: &str = "eject3";
/// A chave que liga o ejetor por comando: `/data merge block x y z {mekanism_eject:1}`.
const EJECT_SWITCH: &str = "mekanism_eject";
/// `ChemicalContainerType.getTag()` e o que `IListContainerType.saveTo` e `LargeResourceStack.codec` gravam: uma lista de
/// `{container: byte, stored: {type, amount}}`, sem o item da lista quando o tanque está vazio, e sem a lista inteira.
const CHEMICAL_TANKS: &str = "chemical_tanks";
const CONTAINER: &str = "container";
const STORED: &str = "stored";
const TYPE: &str = "type";
const AMOUNT: &str = "amount";
/// `SerializationConstants.COMPONENT_UPGRADE`: o `TileComponentUpgrade` grava num filho com este nome.
const COMPONENT_UPGRADE: &str = "component_upgrade";
/// `SerializationConstants.UPGRADES` e `ITEMS`: o mapa de upgrades instalados e a lista dos dois slots.
const UPGRADES: &str = "upgrades";
const ITEMS: &str = "items";
/// O `ItemResource.CODEC`: o id do item e os componentes que o distinguem.
const ITEM_ID: &str = "id";
const COMPONENTS: &str = "components";
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar um objeto vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;
/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;

struct Live {
    machine: ProgressMachine,
    /// A carga de dados (`basis::data_generation`) que a receita em cache da máquina viu: outra carga a invalida.
    generation: u64,
    last_seen: i64,
    /// O que já está no NBT, para só escrever quando muda.
    saved: Option<SavedState>,
    used_version: Option<u32>,
    /// O `lit` que o bloco tem, para só escrever quando muda.
    /// O nível que os comparadores já viram.
    reported_level: Option<i32>,
    lit: Option<bool>,
    /// O que a janela de upgrades viu no tick anterior. O Java sincroniza o `upgradeTicks` e o mapa depois do tick do bloco, na
    /// batida seguinte da tela: o cliente do original mostra o valor de um tick atrás (medido: passos 10 do original igual a
    /// passos 9 do port).
    upgrade_sync_seen: Option<UpgradeSync>,
    /// A face de saída do ejetor, a direita da frente.
    eject_side: Option<Face>,
    /// Os tipos e as tags de upgrade de onde os dados da máquina saíram: uma carga nova de dados os troca.
    upgrade_types: Arc<UpgradeTypes>,
    upgrade_tags: Arc<UpgradeTags>,
    /// Quando os upgrades e os slots mudam, o NBT é escrito de novo.
    upgrades_version: Option<u32>,
    /// A capacidade que o Horizon já sabe desta máquina.
    energy_capacity: Option<i32>,
}

fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK);
    ItemResource::new(&stack.item, limit)
}

fn recipe_ingredient(recipe: &basis::MachineRecipe) -> &Ingredient {
    &recipe.item
}

fn conversion_ingredient(conversion: &basis::ItemConversion) -> &Ingredient {
    &conversion.item
}

fn energy_conversion_ingredient(conversion: &basis::ItemEnergyConversion) -> &Ingredient {
    &conversion.item
}

/// Se o ingrediente do Horizon não guarda nada além dos itens: nem um tipo de ingrediente de mod, nem um filho que guarde
/// (`Ingredient.isCustom() && !isSimple()`).
fn is_plain(ingredient: &Ingredient) -> bool {
    ingredient.custom.is_none()
        && ingredient
            .children
            .as_ref()
            .is_none_or(|(_, children)| children.iter().all(is_plain))
}

/// O que `ItemInputCache.mapIngredient` faz com o ingrediente. O Horizon já resolveu as tags: `order` são os itens do
/// ingrediente na ordem do `HolderSet` (`getValues()`, e `items()` do ingrediente simples), e `children` guarda os filhos do
/// `CompoundIngredient`. Um ingrediente simples indexa os itens, um composto com filho que não é simples mapeia cada filho, e
/// qualquer outro é complexo (`Custom`). O Horizon não tem o `DataComponentIngredient` exato (`Exact`).
fn item_shape(ingredient: &Ingredient) -> IngredientShape {
    if is_plain(ingredient) {
        return IngredientShape::Types(ingredient.order.iter().map(|item| Rc::from(item.as_str())).collect());
    }
    match &ingredient.children {
        Some((Shape::Compound, children)) if ingredient.custom.is_none() => {
            IngredientShape::Compound(children.iter().map(item_shape).collect())
        }
        _ => IngredientShape::Custom,
    }
}

/// O ingrediente do Horizon (item ou tag) vira o da lib, com a classificação do índice e o teste de tipo (`testType`, sem a
/// quantidade), e guarda a última resposta, porque a receita em cache pergunta todo tick pelo mesmo item. `source` e `index` dizem
/// de qual entrada da lista ele sai.
fn item_ingredient<T: 'static>(source: &Arc<Vec<T>>, index: usize, ingredient: fn(&T) -> &Ingredient) -> ItemIngredient {
    let shape = item_shape(ingredient(&source[index]));
    let source = source.clone();
    let last: RefCell<Option<(Rc<str>, bool)>> = RefCell::new(None);
    ItemIngredient::shaped(ShapedIngredient::new(shape, move |input: &dyn TypedInstance| {
        let id = input.type_id();
        if let Some((held, answer)) = &*last.borrow()
            && **held == *id
        {
            return *answer;
        }
        let stack = ItemStack {
            item: Arc::from(id),
            count: 1,
            damage: 0,
            extra: None,
        };
        let answer = ingredient(&source[index]).test(&stack);
        *last.borrow_mut() = Some((Rc::from(id), answer));
        answer
    }))
}

/// `ItemToItemRecipe` das receitas do jogo.
fn core_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemToItemRecipe>> {
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        converted.push(Rc::new(ItemToItemRecipe {
            id: recipe.id.clone(),
            input: item_ingredient(recipes, index, recipe_ingredient),
            input_count: recipe.count,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
        }));
    }
    converted
}

/// `ItemChemicalToItemRecipe` das receitas do jogo. O químico de uma receita é um id ou uma tag, e a tag vale como estava na
/// carga de dados em que a receita foi lida: uma carga nova refaz as receitas. A receita que gasta o químico a cada tick
/// (`per_tick_usage`) segue com o campo, e a máquina escolhe `ItemChemicalConstantUsageRecipe` para ela.
fn core_chemical_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemChemicalToItemRecipe>> {
    let tags = resource::current_tags();
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        let Some(chemical) = recipe.resource.clone() else {
            continue;
        };
        let amount = u32::try_from(chemical.amount).unwrap_or(u32::MAX);
        converted.push(Rc::new(ItemChemicalToItemRecipe {
            id: recipe.id.clone(),
            item_input: item_ingredient(recipes, index, recipe_ingredient),
            item_count: recipe.count,
            chemical_input: ChemicalIngredient::shaped(chemical.shaped(tags.clone())),
            chemical_amount: amount,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
            per_tick_usage: recipe.per_tick_usage,
        }));
    }
    converted
}

/// `ChemicalConversion` das conversões de item em químico do jogo.
fn core_conversions(conversions: &Arc<Vec<basis::ItemConversion>>) -> Vec<Rc<ChemicalConversion>> {
    let mut converted = Vec::with_capacity(conversions.len());
    for (index, conversion) in conversions.iter().enumerate() {
        converted.push(Rc::new(ChemicalConversion {
            input: item_ingredient(conversions, index, conversion_ingredient),
            input_count: conversion.count,
            output: ChemicalResource::new(&conversion.resource),
            amount: i32::try_from(conversion.amount).unwrap_or(i32::MAX),
        }));
    }
    converted
}

/// `EnergyConversion` das conversões de item em energia do jogo.
fn core_energy_conversions(conversions: &Arc<Vec<basis::ItemEnergyConversion>>) -> Vec<Rc<EnergyConversion>> {
    let mut converted = Vec::with_capacity(conversions.len());
    for (index, conversion) in conversions.iter().enumerate() {
        converted.push(Rc::new(EnergyConversion {
            input: item_ingredient(conversions, index, energy_conversion_ingredient),
            input_count: conversion.count,
            output: conversion.amount,
        }));
    }
    converted
}

/// O `getInputCache` de um tipo de receita (`MekanismRecipeType`), com a lista que o `getRecipes(world)` dele devolve. O índice lê
/// a lista na primeira pergunta depois de um `clear`, e `loaded` é a carga de dados (`basis::data_generation`) de onde ela saiu:
/// outra carga (`MekanismRecipeType.clearCache`) limpa o índice e troca a lista.
struct Book<C, R> {
    cache: Rc<C>,
    list: Rc<RefCell<Vec<Rc<R>>>>,
    loaded: Option<u64>,
}

impl<C, R: 'static> Book<C, R> {
    fn new(make: impl FnOnce(Box<dyn Fn() -> Vec<Rc<R>>>) -> Rc<C>) -> Self {
        let list = Rc::new(RefCell::new(Vec::new()));
        let source = list.clone();
        Self {
            cache: make(Box::new(move || source.borrow().clone())),
            list,
            loaded: None,
        }
    }

    /// O índice com a lista desta carga de dados: `clear` e `load` só quando a carga mudou.
    fn current(&mut self, clear: impl FnOnce(&C), load: impl FnOnce() -> Vec<Rc<R>>) -> Rc<C> {
        let generation = basis::data_generation();
        if self.loaded != Some(generation) {
            clear(&*self.cache);
            *self.list.borrow_mut() = load();
            self.loaded = Some(generation);
        }
        self.cache.clone()
    }
}

/// Os índices de receitas desta thread: um por tipo de receita, que todas as máquinas do tipo e as regras de inserção dos slots
/// compartilham.
#[derive(Default)]
struct Books {
    electric: HashMap<&'static str, Book<ItemToItemCache, ItemToItemRecipe>>,
    infusing: Option<Book<ItemChemicalCache, ItemChemicalToItemRecipe>>,
    conversions: Option<Book<ConversionCache, ChemicalConversion>>,
    energy: Option<Book<EnergyConversionCache, EnergyConversion>>,
}

thread_local! {
    static BOOKS: RefCell<Books> = RefCell::new(Books::default());
}

/// Os índices que uma máquina usa.
#[derive(Clone)]
enum Indexes {
    Electric {
        recipes: Rc<ItemToItemCache>,
        energy: Rc<EnergyConversionCache>,
    },
    Infuser {
        recipes: Rc<ItemChemicalCache>,
        conversions: Rc<ConversionCache>,
        energy: Rc<EnergyConversionCache>,
    },
}

impl Indexes {
    /// `MekanismRecipeType.ENERGY_CONVERSION.getInputCache()`, que as quatro máquinas partilham.
    fn energy(&self) -> &Rc<EnergyConversionCache> {
        match self {
            Self::Electric { energy, .. } | Self::Infuser { energy, .. } => energy,
        }
    }
}

fn indexes_of(game: &Game<'_>, machine: &Machine) -> Indexes {
    BOOKS.with_borrow_mut(|books| {
        let energy = books
            .energy
            .get_or_insert_with(|| Book::new(|source| EnergyConversion::cache(source)))
            .current(
                |cache| cache.clear(),
                || {
                    let loaded = basis::energy_conversions(game, rules::ENERGY_CONVERSION, &rules::CONVERSION_FIELDS);
                    core_energy_conversions(&loaded)
                },
            );
        match machine.slots.infusion {
        None => {
            let book = books
                .electric
                .entry(machine.recipes.key())
                .or_insert_with(|| Book::new(|source| ItemToItemRecipe::cache(source)));
            let recipes = book.current(
                |cache| cache.clear(),
                || core_recipes(game, &basis::machine_recipes(game, &machine.recipes)),
            );
            Indexes::Electric { recipes, energy }
        }
        Some(_) => {
            let infusing = books.infusing.get_or_insert_with(|| Book::new(|source| rules::infusing_cache(source)));
            let recipes = infusing.current(
                |cache| cache.clear(),
                || core_chemical_recipes(game, &basis::machine_recipes(game, &INFUSING)),
            );
            let conversions = books
                .conversions
                .get_or_insert_with(|| Book::new(|source| ChemicalConversion::cache(source)));
            let conversions = conversions.current(
                |cache| cache.clear(),
                || {
                    let loaded = basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS);
                    core_conversions(&loaded)
                },
            );
            Indexes::Infuser { recipes, conversions, energy }
        }
        }
    })
}

/// O que o tanque e o slot de entrada guardam, para as regras de inserção que olham um pelo outro.
struct Held {
    chemical: Option<ChemicalResource>,
    input: Option<ItemResource>,
}

/// O item como o `TypedInstance` que o índice pergunta (`ItemResource`): só o id conta para o tipo.
fn typed_item(stack: &ItemStack) -> ItemResource {
    ItemResource::new(&stack.item, UNKNOWN_STACK)
}

/// Se o slot `slot` da máquina aceita o item: o slot de entrada (`InputInventorySlot`, com `containsRecipe` na Chamber e
/// `containsRecipeAB` mais `containsRecipeA` no Infuser), o de infusão (`ChemicalInventorySlot.canFillOrConvert`) e o de energia
/// (`EnergyInventorySlot.canFillOrConvert`). A regra do hopper vizinho e a do menu são as mesmas.
fn slot_accepts(indexes: &Indexes, slots: &Slots, slot: usize, item: &ItemResource, held: &Held) -> bool {
    if slot == slots.energy {
        return machine_energy_slot_accepts(indexes.energy(), item);
    }
    match indexes {
        Indexes::Electric { recipes, .. } => slot == slots.input && recipes.contains_input(item),
        Indexes::Infuser { recipes, conversions, .. } => {
            if slot == slots.input {
                recipes.contains_input_a(item) && recipes.contains_input_ab(item, &held.chemical)
            } else if Some(slot) == slots.infusion {
                // `simulateCanInsert(tank, conversion, MANUAL)`: o `isValidForInsertion` do tanque do Infuser é o `containsRecipeB`
                // e o `containsRecipeBA` com o item que está na entrada.
                can_fill_or_convert_with(conversions, item, held.chemical.as_ref(), |chemical| {
                    recipes.contains_input_b(chemical) && recipes.contains_input_ba(&held.input, chemical)
                })
            } else {
                false
            }
        }
    }
}

fn has_saved(entity: &ModBlockEntity) -> bool {
    entity.nbt.contains_key(&JavaString::from(REDSTONE))
}

fn nbt_int(entity: &ModBlockEntity, key: &str) -> i32 {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

fn nbt_bool(entity: &ModBlockEntity, key: &str) -> bool {
    matches!(entity.nbt.get(&JavaString::from(key)), Some(Tag::Byte(value)) if *value != 0)
}

fn nbt_string(entity: &ModBlockEntity, key: &str) -> Option<String> {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// `loadAdditional` do `TileEntityMekanism`, do monitor e do componente de upgrades, do que a `ModBlockEntity` guarda.
fn read_saved(game: &Game<'_>, entity: &ModBlockEntity) -> SavedState {
    let mut recipes_used = BTreeMap::new();
    if let Some(Tag::Compound(used)) = entity.nbt.get(&JavaString::from(RECIPES_USED)) {
        for (id, amount) in used {
            if let Tag::Int(amount) = amount {
                recipes_used.insert(id.to_string_lossy(), *amount);
            }
        }
    }
    SavedState {
        redstone: nbt_bool(entity, REDSTONE),
        control_type: RedstoneControl::from_serialized_name(
            &nbt_string(entity, CONTROL_TYPE).unwrap_or_default(),
        ),
        active: nbt_bool(entity, ACTIVE_STATE),
        update_delay: nbt_int(entity, UPDATE_DELAY),
        current_redstone: nbt_int(entity, CURRENT_REDSTONE),
        progress: nbt_int(entity, PROGRESS),
        used_so_far: nbt_int(entity, USED_SO_FAR),
        recipes_used,
        last_recipe_used: nbt_string(entity, LAST_RECIPE_USED),
        last_recipe_amount: nbt_int(entity, LAST_RECIPE_AMOUNT),
        ejecting: nbt_bool(entity, EJECT_ITEM),
        upgrades: read_upgrades(game, entity),
    }
}

/// `LargeResourceStack.codec` de um `ItemResource`: `type` (`id` e `components`) e `amount`. O único componente que a máquina lê é o
/// tipo do upgrade. Um slot que se lê mal vale vazio, como o `readOrEmpty`.
fn upgrade_slot_contents(game: &Game<'_>, stored: &Compound) -> SlotContents {
    let Some(Tag::Compound(kind)) = stored.get(&JavaString::from(TYPE)) else {
        return SlotContents::default();
    };
    let Some(Tag::String(id)) = kind.get(&JavaString::from(ITEM_ID)) else {
        return SlotContents::default();
    };
    let id = id.to_string_lossy();
    let variant = match kind.get(&JavaString::from(COMPONENTS)) {
        Some(Tag::Compound(components)) => {
            match components.get(&JavaString::from(rules::UPGRADE_TYPE_COMPONENT)) {
                Some(Tag::String(variant)) => Some(Rc::from(variant.to_string_lossy().as_str())),
                _ => None,
            }
        }
        _ => None,
    };
    let amount = match stored.get(&JavaString::from(AMOUNT)) {
        Some(Tag::Long(amount)) => u32::try_from(*amount).ok(),
        Some(Tag::Int(amount)) => u32::try_from(*amount).ok(),
        _ => None,
    };
    let max_stack_size = game.native_max_stack_size(&id).unwrap_or(UNKNOWN_STACK);
    let resource = ItemResource {
        id: Rc::from(id.as_str()),
        max_stack_size,
        variant,
    };
    SlotContents::new(Some(resource), amount.unwrap_or(0))
}

/// `TileComponentUpgrade.deserialize`: `upgrades` (o mapa de tipo para total) e os dois slots, na lista `items`.
fn read_upgrades(game: &Game<'_>, entity: &ModBlockEntity) -> UpgradeSaved {
    let mut saved = UpgradeSaved::default();
    let Some(Tag::Compound(component)) = entity.nbt.get(&JavaString::from(COMPONENT_UPGRADE)) else {
        return saved;
    };
    if let Some(Tag::Compound(upgrades)) = component.get(&JavaString::from(UPGRADES)) {
        for (id, total) in upgrades {
            if let Tag::Int(total) = total {
                saved.upgrades.insert(id.to_string_lossy(), *total);
            }
        }
    }
    if let Some(Tag::List(items)) = component.get(&JavaString::from(ITEMS)) {
        for entry in items {
            let Tag::Compound(entry) = entry else {
                continue;
            };
            let slot = match entry.get(&JavaString::from(CONTAINER)) {
                Some(Tag::Byte(slot)) => *slot,
                _ => -1,
            };
            let contents = match entry.get(&JavaString::from(STORED)) {
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

/// `TileComponentUpgrade.serialize`, escrito pelo `write` do componente: o filho some quando não tem nada.
fn write_upgrades(entity: &mut ModBlockEntity, saved: &UpgradeSaved) {
    let mut component = Compound::default();
    if !saved.upgrades.is_empty() {
        let mut upgrades = Compound::default();
        for (id, total) in &saved.upgrades {
            upgrades.insert(JavaString::from(id.as_str()), Tag::Int(*total));
        }
        component.insert(JavaString::from(UPGRADES), Tag::Compound(upgrades));
    }
    let mut items = Vec::new();
    for (index, contents) in [&saved.input, &saved.output].into_iter().enumerate() {
        let Some(resource) = &contents.resource else {
            continue;
        };
        let mut kind = Compound::default();
        kind.insert(JavaString::from(ITEM_ID), Tag::String(JavaString::from(&*resource.id)));
        if let Some(variant) = &resource.variant {
            let mut components = Compound::default();
            components.insert(
                JavaString::from(rules::UPGRADE_TYPE_COMPONENT),
                Tag::String(JavaString::from(&**variant)),
            );
            kind.insert(JavaString::from(COMPONENTS), Tag::Compound(components));
        }
        let mut stored = Compound::default();
        stored.insert(JavaString::from(TYPE), Tag::Compound(kind));
        stored.insert(JavaString::from(AMOUNT), Tag::Long(i64::from(contents.amount)));
        let mut entry = Compound::default();
        entry.insert(JavaString::from(CONTAINER), Tag::Byte(index as i8));
        entry.insert(JavaString::from(STORED), Tag::Compound(stored));
        items.push(Tag::Compound(entry));
    }
    if !items.is_empty() {
        component.insert(JavaString::from(ITEMS), Tag::List(items));
    }
    let key = JavaString::from(COMPONENT_UPGRADE);
    if component.is_empty() {
        entity.nbt.remove(&key);
    } else {
        entity.nbt.insert(key, Tag::Compound(component));
    }
}

/// Os dados de upgrade desta carga: os tipos, as tags e o limite de pilha do item.
fn upgrade_data(game: &Game<'_>, types: &UpgradeTypes, tags: &UpgradeTags) -> Rc<upgrade::UpgradeData> {
    let stack = game.native_max_stack_size(rules::UPGRADE_ITEM).unwrap_or(UNKNOWN_STACK);
    rules::upgrade_data(types, tags, stack)
}

fn put(entity: &mut ModBlockEntity, key: &str, tag: Tag) {
    entity.nbt.insert(JavaString::from(key), tag);
}

/// `saveAdditional` do que muda a cada tick, só dos campos que mudaram desde `old`. `used_so_far` só o Metallurgic Infuser grava.
fn write_scalars(entity: &mut ModBlockEntity, old: Option<&SavedState>, new: &SavedState, with_used_so_far: bool) {
    let changed = |same: fn(&SavedState, &SavedState) -> bool| old.is_none_or(|old| !same(old, new));
    if changed(|a, b| a.redstone == b.redstone) {
        put(entity, REDSTONE, Tag::Byte(i8::from(new.redstone)));
    }
    if changed(|a, b| a.control_type == b.control_type) {
        let name = JavaString::from(new.control_type.serialized_name());
        put(entity, CONTROL_TYPE, Tag::String(name));
    }
    if changed(|a, b| a.active == b.active) {
        put(entity, ACTIVE_STATE, Tag::Byte(i8::from(new.active)));
    }
    if changed(|a, b| a.update_delay == b.update_delay) {
        put(entity, UPDATE_DELAY, Tag::Int(new.update_delay));
    }
    if changed(|a, b| a.current_redstone == b.current_redstone) {
        put(entity, CURRENT_REDSTONE, Tag::Int(new.current_redstone));
    }
    if changed(|a, b| a.progress == b.progress) {
        put(entity, PROGRESS, Tag::Int(new.progress));
    }
    if with_used_so_far && changed(|a, b| a.used_so_far == b.used_so_far) {
        put(entity, USED_SO_FAR, Tag::Int(new.used_so_far));
    }
    if changed(|a, b| a.ejecting == b.ejecting) {
        put(entity, EJECT_ITEM, Tag::Byte(i8::from(new.ejecting)));
    }
}

/// `readFrom` do `IListContainerType` para o tanque 0: as entradas com `container` 0, sem checar a capacidade (o Java lê o
/// tanque "sem checar", para um conteúdo que deixou de ser válido não derrubar o carregamento). Uma entrada que não se lê vale vazio.
fn read_tank(entity: &ModBlockEntity) -> TankContents {
    let Some(Tag::List(list)) = entity.nbt.get(&JavaString::from(CHEMICAL_TANKS)) else {
        return TankContents::default();
    };
    let mut contents = TankContents::default();
    for entry in list {
        let Tag::Compound(entry) = entry else {
            continue;
        };
        if !matches!(entry.get(&JavaString::from(CONTAINER)), Some(Tag::Byte(0))) {
            continue;
        }
        contents = match entry.get(&JavaString::from(STORED)) {
            Some(Tag::Compound(stored)) => stored_contents(stored),
            _ => TankContents::default(),
        };
    }
    contents
}

/// `LargeResourceStack.codec`: `type` (o id do químico) e `amount` (um `long` que não é negativo).
fn stored_contents(stored: &Compound) -> TankContents {
    let id = match stored.get(&JavaString::from(TYPE)) {
        Some(Tag::String(id)) => id.to_string_lossy(),
        _ => return TankContents::default(),
    };
    let amount = match stored.get(&JavaString::from(AMOUNT)) {
        Some(Tag::Long(amount)) => u64::try_from(*amount).ok(),
        Some(Tag::Int(amount)) => u64::try_from(*amount).ok(),
        _ => None,
    };
    match amount {
        Some(amount) => TankContents::new(Some(ChemicalResource::new(&id)), amount),
        None => TankContents::default(),
    }
}

/// `saveTo` do `IListContainerType` para o tanque 0, que só grava o que não está vazio.
fn write_tank(entity: &mut ModBlockEntity, contents: &TankContents) {
    let key = JavaString::from(CHEMICAL_TANKS);
    let Some(resource) = &contents.resource else {
        entity.nbt.remove(&key);
        return;
    };
    let mut stored = Compound::default();
    stored.insert(JavaString::from(TYPE), Tag::String(JavaString::from(&*resource.id)));
    stored.insert(JavaString::from(AMOUNT), Tag::Long(i64::try_from(contents.amount).unwrap_or(i64::MAX)));
    let mut entry = Compound::default();
    entry.insert(JavaString::from(CONTAINER), Tag::Byte(0));
    entry.insert(JavaString::from(STORED), Tag::Compound(stored));
    entity.nbt.insert(key, Tag::List(vec![Tag::Compound(entry)]));
}

/// O tanque da versão anterior ficava em `machinery_basis.tanks`. Num bloco que ainda só tem esse, leva o conteúdo para o
/// formato do Java, e tira o antigo, para um tanque esvaziado não voltar a encher.
fn migrate_old_tank(entity: &mut ModBlockEntity) {
    if entity.nbt.contains_key(&JavaString::from(CHEMICAL_TANKS)) {
        return;
    }
    let old = basis::load_tanks(entity, &[rules::TANK_CAPACITY]);
    if let Some(resource) = old[0].resource() {
        let contents = TankContents::new(Some(ChemicalResource::new(resource)), old[0].amount());
        write_tank(entity, &contents);
    }
    basis::discard_tanks(entity);
}

/// `saveAdditional` do monitor: as receitas usadas, a última e quanto dela saiu. Como no Java, a quantidade só vai se
/// for maior que zero, e a última só se existe.
fn write_used(entity: &mut ModBlockEntity, new: &SavedState) {
    let mut used = Compound::default();
    for (id, amount) in &new.recipes_used {
        used.insert(JavaString::from(id.as_str()), Tag::Int(*amount));
    }
    put(entity, RECIPES_USED, Tag::Compound(used));
    match &new.last_recipe_used {
        Some(id) => put(entity, LAST_RECIPE_USED, Tag::String(JavaString::from(id.as_str()))),
        None => {
            entity.nbt.remove(&JavaString::from(LAST_RECIPE_USED));
        }
    }
    if new.last_recipe_amount > 0 {
        put(entity, LAST_RECIPE_AMOUNT, Tag::Int(new.last_recipe_amount));
    } else {
        entity.nbt.remove(&JavaString::from(LAST_RECIPE_AMOUNT));
    }
}

/// `ProgressMachine` nova, do que a `ModBlockEntity` guarda. Com o índice do Infuser é o Metallurgic Infuser.
fn born(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, machine: &Machine, indexes: &Indexes) -> Live {
    // `ThreadLocalRandom.nextInt(RECIPE_CHECK_FREQUENCY)`: o sorteio de `shouldRecheckAllErrors`.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    let check_offset = nanos % RECIPE_CHECK_FREQUENCY;
    let (upgrade_types, upgrade_tags) = (upgrade::current_types(), upgrade::current_tags());
    let upgrades = upgrade_data(game, &upgrade_types, &upgrade_tags);
    let mut progress = match indexes {
        Indexes::Electric { recipes, energy } => {
            let mut config = rules::electric_config(machine.energy_per_tick, recipes.clone(), check_offset, upgrades);
            config.energy_conversion = energy.clone();
            ProgressMachine::electric(config)
        }
        Indexes::Infuser { recipes, conversions, energy } => {
            let mut config = rules::infuser_config(
                machine.energy_per_tick,
                recipes.clone(),
                conversions.clone(),
                check_offset,
                upgrades,
            );
            config.energy_conversion = energy.clone();
            ProgressMachine::chemical(config)
        }
    };
    let block_active = game.native_block_property(pos, "lit").as_deref() == Some("true");
    progress.restore(&read_saved(game, entity), block_active);
    // Quem já tinha a tela aberta antes de o objeto existir: a contagem guardada é aplicada uma vez, aqui, num objeto novo.
    let viewers =
        VIEWERS.with_borrow(|counts| counts.get(&(game.native_dimension(), pos)).copied());
    for _ in 0..viewers.unwrap_or(0) {
        progress.open();
    }
    Live {
        machine: progress,
        generation: basis::data_generation(),
        last_seen: i64::MIN,
        saved: None,
        used_version: None,
        reported_level: None,
        lit: None,
        upgrade_sync_seen: None,
        eject_side: None,
        upgrade_types,
        upgrade_tags,
        upgrades_version: None,
        energy_capacity: None,
    }
}

/// O que a `ModBlockEntity` mudou fora do tick vira `setContents` no slot, que dispara os listeners como o Java.
fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let same = slot.resource().is_some_and(|held| *held.id == *stack.item) && slot.amount() == stack.count;
            if !same {
                slot.set_contents(Some(resource_of(game, stack)), stack.count, None);
            }
        }
    }
}

/// O que o tick mudou vai para a `ModBlockEntity`. Uma pilha do mesmo item guarda o que tinha (dano, componentes); um item
/// novo é o da saída de uma receita.
fn slot_to_entity(entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot, recipes: &[basis::MachineRecipe]) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource.id == *stack.item && contents.amount == stack.count,
        _ => false,
    };
    if same {
        return;
    }
    let new = contents.resource.map(|resource| {
        let mut stack = match current {
            Some(old) if *old.item == *resource.id => old.clone(),
            _ => recipes
                .iter()
                .find(|recipe| *recipe.output.item == *resource.id)
                .map_or_else(
                    || ItemStack {
                        item: Arc::from(&*resource.id),
                        count: 1,
                        damage: 0,
                        extra: None,
                    },
                    |recipe| recipe.output.clone(),
                ),
        };
        stack.count = contents.amount;
        stack
    });
    entity.items[index] = new;
}

/// O recurso de um item de upgrade: o item e o tipo que o componente dele guarda.
fn upgrade_resource(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK);
    match game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) {
        Ok(Some(Tag::String(kind))) => ItemResource::with_variant(&stack.item, limit, &kind.to_string_lossy()),
        _ => ItemResource::new(&stack.item, limit),
    }
}

/// Os slots de upgrade (os virtuais do menu) lidos do que a tela mudou, como `slot_from_entity`, com o tipo do upgrade.
fn upgrade_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let resource = upgrade_resource(game, stack);
            if slot.resource().as_ref() != Some(&resource) || slot.amount() != stack.count {
                slot.set_contents(Some(resource), stack.count, None);
            }
        }
    }
}

/// O item que um slot de upgrade guarda, com o tipo no componente.
fn upgrade_slot_stack(game: &Game<'_>, slot: &InventorySlot) -> Option<ItemStack> {
    let contents = slot.contents();
    contents.resource.map(|resource| {
        let mut stack = ItemStack::new(&resource.id, contents.amount);
        if let Some(kind) = &resource.variant {
            let _ = game.native_set_component(&mut stack, rules::UPGRADE_TYPE_COMPONENT, Tag::String(JavaString::from(&**kind)));
        }
        stack
    })
}

/// O que o tick mudou nos slots de upgrade (instalar tira da entrada, remover enche a saída) volta para o menu.
fn upgrade_to_entity(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot) {
    let contents = slot.contents();
    let same = match (&contents.resource, entity.items[index].as_ref()) {
        (None, None) => true,
        (Some(resource), Some(stack)) => upgrade_resource(game, stack) == *resource && contents.amount == stack.count,
        _ => false,
    };
    if !same {
        entity.items[index] = upgrade_slot_stack(game, slot);
    }
}

/// Os vizinhos como o ejetor os vê: o inventário do outro lado, copiado do jogo, com a regra real de cada slot para o item que
/// está saindo. O Horizon não simula inserção, então o diário guarda a cópia, e o que entrou nela vai ao vizinho de verdade, por
/// `native_insert_item`, depois que a transação commitou. O ejetor roda antes da receita: só o item da saída pode sair no tick.
struct GameNeighbours<'a, 'g> {
    game: &'a Game<'g>,
    dimension: &'static str,
    pos: [i32; 3],
    offered: Option<(Rc<str>, ItemStack)>,
    created: RefCell<Vec<CopiedContainer>>,
}

struct CopiedContainer {
    at: [i32; 3],
    reached: Direction,
    container: Rc<VanillaContainer>,
    original: Vec<(usize, ItemResource, u32)>,
}

impl ItemCapabilities for GameNeighbours<'_, '_> {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        let [x, y, z] = face.offset();
        let at = [self.pos[0] + x, self.pos[1] + y, self.pos[2] + z];
        let reached = basis::direction(face.opposite());
        let slots = self.game.native_container_slots(self.dimension, at, reached)?;
        let container = VanillaContainer::new(slots.len(), Rc::new(|| {}));
        for (index, slot) in slots.iter().enumerate() {
            if let Some(stack) = &slot.stack {
                container.set_item(index, Some(resource_of(self.game, stack)), stack.count);
            }
        }
        let original = container.stacks();
        self.created.borrow_mut().push(CopiedContainer {
            at,
            reached,
            container: container.clone(),
            original,
        });
        let (offered, accepted) = match &self.offered {
            Some((id, stack)) => (
                Some(id.clone()),
                slots
                    .iter()
                    .map(|slot| self.game.machinery_slot_accepts(at, slot.slot, stack, reached))
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        Some(Rc::new(RuledHandler::new(container, move |index, resource: &ItemResource| {
            offered.as_ref() == Some(&resource.id) && accepted.get(index).copied().unwrap_or(false)
        })))
    }
}

/// O que entrou nas cópias durante o tick: por vizinho, quanto de cada item.
struct Ejected {
    at: [i32; 3],
    reached: Direction,
    id: Rc<str>,
    count: u32,
}

impl GameNeighbours<'_, '_> {
    fn ejected(self) -> Vec<Ejected> {
        let mut ejected = Vec::new();
        for copied in self.created.into_inner() {
            for (index, resource, amount) in copied.container.stacks() {
                let before = copied
                    .original
                    .iter()
                    .find(|(slot, held, _)| *slot == index && *held == resource)
                    .map_or(0, |(_, _, amount)| *amount);
                if amount > before {
                    ejected.push(Ejected {
                        at: copied.at,
                        reached: copied.reached,
                        id: resource.id.clone(),
                        count: amount - before,
                    });
                }
            }
        }
        ejected
    }
}

/// A pilha de um item que sai da máquina: a da receita, com dano e componentes, ou a do id.
fn stack_of(recipes: &[basis::MachineRecipe], id: &str) -> ItemStack {
    let template = recipes.iter().find(|recipe| *recipe.output.item == *id);
    template.map_or_else(
        || ItemStack {
            item: Arc::from(id),
            count: 1,
            damage: 0,
            extra: None,
        },
        |recipe| recipe.output.clone(),
    )
}

/// Entrega ao vizinho de verdade o que o diário mandou. A regra de cada slot já foi perguntada e nada mudou no vizinho desde
/// então, então ele aceita tudo. O resto, se houver, volta ao slot de saída da `ModBlockEntity`.
fn deliver(game: &mut Game<'_>, entity: &mut ModBlockEntity, recipes: &[basis::MachineRecipe], output_slot: usize, ejected: Vec<Ejected>) {
    for sent in ejected {
        let mut stack = stack_of(recipes, &sent.id);
        stack.count = sent.count;
        let Some(rest) = game.native_insert_item(sent.at, sent.reached, stack) else {
            continue;
        };
        let slot = &mut entity.items[output_slot];
        match slot.as_mut() {
            None => *slot = Some(rest),
            Some(held) if held.item == rest.item => held.count += rest.count,
            Some(_) => eprintln!(
                "mekanism: {} x{} recusados pelo vizinho se perderam",
                rest.item, rest.count
            ),
        }
    }
}

/// O baú como o rastro o grava, lido do jogo: `[slot, id, quantidade]` dos slots com algo.
fn chest_of(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], face: Face) -> Vec<(usize, String, u32)> {
    let [x, y, z] = face.offset();
    let at = [pos[0] + x, pos[1] + y, pos[2] + z];
    let reached = basis::direction(face.opposite());
    let slots = game.native_container_slots(dimension, at, reached).unwrap_or_default();
    slots
        .into_iter()
        .filter_map(|slot| {
            let stack = slot.stack?;
            let path = stack.item.split_once(':').map_or(&*stack.item, |(_, path)| path).to_owned();
            Some((slot.slot, path, stack.count))
        })
        .collect()
}

/// Um tick de uma máquina de progresso sobre o objeto vivo.
fn tick_machine(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, spec: &Machine) {
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let slots = &spec.slots;
    let recipes = basis::machine_recipes(game, &spec.recipes);
    let indexes = indexes_of(game, spec);

    let started = basis::MEDIR.then(Instant::now);
    // Um mundo antigo tem menos slots salvos que o menu de agora (os dois virtuais vêm depois dos da máquina).
    let menu_slots = if slots.infusion.is_some() { rules::MENU_SLOTS } else { rules::ELECTRIC_MENU_SLOTS };
    if entity.items.len() < menu_slots {
        entity.items.resize(menu_slots, None);
    }
    let alive = LIVE_MACHINES.with_borrow_mut(|machines| machines.remove(&key));
    let mut fresh = false;
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && has_saved(entity) => live,
        _ => {
            if slots.infusion.is_some() {
                migrate_old_tank(entity);
            }
            fresh = true;
            born(game, pos, entity, spec, &indexes)
        }
    };
    // Uma carga nova de dados limpou os índices (`MekanismRecipeType.clearCache`) e o monitor larga a receita que tinha
    // (`invalidateCache`).
    let generation = basis::data_generation();
    if live.generation != generation {
        live.machine.invalidate_recipe_cache();
        live.generation = generation;
    }
    let (upgrade_types, upgrade_tags) = (upgrade::current_types(), upgrade::current_tags());
    if !Arc::ptr_eq(&live.upgrade_types, &upgrade_types) || !Arc::ptr_eq(&live.upgrade_tags, &upgrade_tags) {
        live.machine.upgrades().set_data(upgrade_data(game, &upgrade_types, &upgrade_tags));
        live.upgrade_types = upgrade_types;
        live.upgrade_tags = upgrade_tags;
    }
    let machine = &live.machine;
    slot_from_entity(game, machine.input(), entity.items[slots.input].as_ref());
    slot_from_entity(game, machine.output(), entity.items[slots.output].as_ref());
    slot_from_entity(game, machine.energy_slot(), entity.items[slots.energy].as_ref());
    // O conteúdo dos slots de upgrade é o do NBT do componente, que o objeto novo acabou de ler: o menu só o copia nesse tick.
    if !fresh {
        upgrade_from_entity(game, machine.upgrades().upgrade_slot(), entity.items[slots.upgrade].as_ref());
        upgrade_from_entity(game, machine.upgrades().upgrade_output_slot(), entity.items[slots.upgrade_output].as_ref());
    }
    if let (Some(side), Some(index)) = (machine.chemical_side(), slots.infusion) {
        slot_from_entity(game, &side.slot, entity.items[index].as_ref());
        // O tanque também muda fora do tick: o botão de esvaziar e o que a tela faz.
        let contents = read_tank(entity);
        if side.tank.contents() != contents {
            side.tank.set_contents(contents.resource, contents.amount, None);
        }
    }
    let stored = i64::from(basis::stored_energy(entity));
    if machine.energy().amount() != stored {
        machine.energy().set_energy(stored, None);
    }
    // `updatePower`: o sinal dos vizinhos, lido a cada tick.
    let powered = game.native_neighbor_signal(dimension, pos) > 0;
    if machine.tile().is_powered() != powered {
        machine.tile_mut().update_power(powered);
    }
    // O ejetor liga e desliga por `mekanism_eject`; sai pela direita da frente.
    let switch = match entity.nbt.get(&JavaString::from(EJECT_SWITCH)) {
        Some(Tag::Byte(on)) => Some(*on != 0),
        Some(Tag::Int(on)) => Some(*on != 0),
        _ => None,
    };
    if let Some(on) = switch
        && live.machine.ejector().is_ejecting() != on
    {
        live.machine.ejector_mut().set_ejecting(on);
    }
    let front = basis::front(game, pos);
    let right = front.right_of();
    if live.machine.ejector().is_ejecting() && live.eject_side != Some(right) {
        live.machine.ejector_mut().set_sides(0, vec![right]);
        live.eject_side = Some(right);
    }

    let offered = live.machine.output().resource().map(|resource| {
        let stack = stack_of(&recipes, &resource.id);
        (resource.id, stack)
    });
    let neighbours = GameNeighbours {
        game: &*game,
        dimension,
        pos,
        offered,
        created: RefCell::new(Vec::new()),
    };
    live.machine.tick_server(now as u64, &neighbours);
    let ejected = neighbours.ejected();

    let machine = &live.machine;
    slot_to_entity(entity, slots.input, machine.input(), &recipes);
    slot_to_entity(entity, slots.output, machine.output(), &recipes);
    slot_to_entity(entity, slots.energy, machine.energy_slot(), &recipes);
    upgrade_to_entity(game, entity, slots.upgrade, machine.upgrades().upgrade_slot());
    upgrade_to_entity(game, entity, slots.upgrade_output, machine.upgrades().upgrade_output_slot());
    if let (Some(side), Some(index)) = (machine.chemical_side(), slots.infusion) {
        slot_to_entity(entity, index, &side.slot, &recipes);
        let contents = side.tank.contents();
        if read_tank(entity) != contents {
            write_tank(entity, &contents);
        }
    }
    let energy = basis::energy_as_int(machine.energy().amount(), "a energia guardada");
    if basis::stored_energy(entity) != energy {
        basis::set_stored_energy(entity, energy);
    }
    // A capacidade segue os upgrades instalados, e o Horizon só a sabe por instância se dissermos.
    let capacity = basis::energy_as_int(machine.energy().capacity(), "a capacidade de energia");
    if live.energy_capacity != Some(capacity) {
        basis::set_energy_capacity(dimension, pos, &entity.id, capacity);
        live.energy_capacity = Some(capacity);
    }
    let scalars = machine.saved_scalars();
    if live.saved.as_ref() != Some(&scalars) {
        write_scalars(entity, live.saved.as_ref(), &scalars, slots.infusion.is_some());
        live.saved = Some(scalars);
    }
    let version = machine.used_version();
    if live.used_version != Some(version) {
        write_used(entity, &machine.saved_state());
        live.used_version = Some(version);
    }
    let upgrades_version = machine.upgrades_version();
    if live.upgrades_version != Some(upgrades_version) {
        write_upgrades(entity, &machine.upgrades().saved());
        live.upgrades_version = Some(upgrades_version);
    }
    deliver(game, entity, &recipes, slots.output, ejected);
    let progress = machine.recipe_progress();
    if let Some(started) = started {
        basis::bench_record(now, u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let chest = machine
            .ejector()
            .is_ejecting()
            .then(|| chest_of(game, dimension, pos, right));
        basis::trace(&rules::trace_line(now, machine, chest.as_deref(), true));
    }

    let lit = machine.tile().block_active();
    if live.lit != Some(lit) {
        game.native_set_block_property(pos, "lit", if lit { "true" } else { "false" });
        live.lit = Some(lit);
    }
    let mut screen = rules::ScreenData {
        energy: energy as u32,
        capacity: capacity as u32,
        progress,
        ticks_required: machine.ticks_required(),
        errors: machine.tracked_errors_mask(),
        redstone: machine.tile().redstone_sync(),
        ..Default::default()
    };
    if let Some(side) = machine.chemical_side() {
        let held = side.tank.resource();
        let id = held.as_ref().map(|held| held.id.to_string()).unwrap_or_default();
        let registry = resource::current();
        let infusion = registry.get(&id);
        screen.chemical_amount = side.tank.amount() as i32;
        screen.chemical_tint = infusion.map_or(0, |infusion| infusion.tint);
        screen.chemical_icon = rules::chemical_icon_slot(infusion.and_then(|infusion| infusion.icon.as_deref()));
        screen.chemical_id = data_slots::pack_text(&id);
    }
    let mut data = screen.write(spec.screen.data);
    // O Java só sincroniza `upgradeTicks` e o mapa com a janela aberta, por jogador; aqui os data slots são do bloco, então eles só
    // levam o valor enquanto alguém acompanha, e ficam em zero (sem tráfego) no resto do tempo.
    let upgrade_sync = UpgradeSync::of(machine.upgrades());
    let shown = live.upgrade_sync_seen.replace(upgrade_sync.clone()).unwrap_or(upgrade_sync);
    if Game::mb_window_tracking(pos) {
        rules::write_upgrade_sync(&mut data, spec.screen.data, &shown);
    }
    if entity.data != data {
        entity.data = data;
    }

    match live.machine.chemical_side() {
        None => {
            let held = Held { chemical: None, input: None };
            let accepts = |_: &Game<'_>, slot: usize, stack: &ItemStack| {
                slot_accepts(&indexes, &spec.slots, slot, &typed_item(stack), &held)
            };
            let slots = |face| rules::electric_item_slots(front, face);
            let sides = rules::sides(front, rules::electric_item_slots);
            // `DataType.INPUT` não deixa tirar: a base não entrega, e as costas (`DataType.ENERGY`) entregam só o que o slot de
            // energia deixa tirar de quem está de fora (`!external || !canFillOrConvert`).
            let energy_slot = spec.slots.energy;
            let can_give = |_: &Game<'_>, slot: usize, stack: &ItemStack| {
                slot == energy_slot
                    && machine.energy_slot().is_valid_for_extraction(&typed_item(stack), AutomationType::External)
            };
            basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts, &can_give);
        }
        Some(side) => {
            let held = Held { chemical: side.tank.resource(), input: machine.input().resource() };
            let accepts = |_: &Game<'_>, slot: usize, stack: &ItemStack| {
                slot_accepts(&indexes, &spec.slots, slot, &typed_item(stack), &held)
            };
            let slots = |face| rules::item_slots(front, face);
            let sides = rules::sides(front, rules::item_slots);
            // `ChemicalInventorySlot.fillOrConvert`, `canExtract`: um hopper (`EXTERNAL`) só tira do slot de infusão o que o
            // tanque não aceita nem converte.
            let (infusion_slot, energy_slot) = (spec.slots.infusion, spec.slots.energy);
            let can_give = |_: &Game<'_>, slot: usize, stack: &ItemStack| {
                let held = if Some(slot) == infusion_slot {
                    &side.slot
                } else if slot == energy_slot {
                    machine.energy_slot()
                } else {
                    return false;
                };
                held.is_valid_for_extraction(&typed_item(stack), AutomationType::External)
            };
            basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts, &can_give);
        }
    }

    report_comparator(&mut live, key);
    live.last_seen = now;
    LIVE_MACHINES.with_borrow_mut(|machines| {
        machines.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            machines.retain(|_, live| now - live.last_seen <= STALE_TICKS);
            COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.retain(|key, _| machines.contains_key(key)));
            basis::retain_energy_capacities(|dimension, pos| machines.contains_key(&(dimension, pos)));
        }
    });
}

/// `notifyComparatorChange`, mais o nível que `getAnalogOutputSignal` devolve. O objeto vivo sai da tabela durante o próprio
/// tick, e um comparador pode perguntar nessa hora, por isso o nível fica em outra tabela.
fn report_comparator(live: &mut Live, key: (&'static str, [i32; 3])) {
    let level = live.machine.tile().current_redstone_level();
    if live.reported_level != Some(level) {
        live.reported_level = Some(level);
        COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert(key, level));
        COMPARATOR_QUEUE.with_borrow_mut(|queue| queue.push(key));
    }
}

/// `NativeComponentTypes`: o componente que guarda o tipo do upgrade no item (`MekanismDataComponents.UPGRADE_TYPE`).
pub(in crate::game) fn component_types(types: &mut Vec<String>) {
    types.push(rules::UPGRADE_TYPE_COMPONENT.to_owned());
}

/// `ItemUpgrade.useOn`: com shift, o upgrade na mão entra na máquina que o aceita, e o que entrou sai da pilha. Um clique em máquina
/// que não aceita o tipo segue o caminho normal (`PASS`). Só as máquinas deste mod, que são as que têm o componente.
pub(in crate::game) fn use_item_on(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    if *claimed || *stack.item != *rules::UPGRADE_ITEM || !game.native_sneaking() {
        return;
    }
    let block = game.native_block(pos);
    let Some(spec) = machine_of(&block) else {
        return;
    };
    let upgrade = match game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) {
        Ok(Some(Tag::String(upgrade))) => upgrade.to_string_lossy(),
        _ => return,
    };
    let (types, tags) = (upgrade::current_types(), upgrade::current_tags());
    if !upgrade_data(game, &types, &tags).supports(&upgrade) {
        return;
    }
    // `InteractionResult.SUCCESS_SERVER`: o clique é do upgrade, tenha entrado algum ou não.
    *claimed = true;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let at = (pos[0], pos[1], pos[2]);
    let alive = LIVE_MACHINES.with_borrow_mut(|machines| machines.remove(&key));
    let was_alive = alive.is_some();
    let mut live = match alive {
        Some(live) => live,
        None => {
            // Uma máquina que ainda não andou nasce do NBT só para receber o upgrade, e o primeiro tick dela a refaz de lá.
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
                return;
            };
            born(game, pos, entity, spec, &indexes_of(game, spec))
        }
    };
    let available = i32::try_from(stack.count).unwrap_or(i32::MAX);
    let added = live.machine.upgrades().add_upgrades(&upgrade, available);
    if added > 0 {
        // O NBT já guarda o que entrou: salvar o mundo antes do próximo tick da máquina não perde o upgrade nem o item.
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
            write_upgrades(entity, &live.machine.upgrades().saved());
        }
        live.upgrades_version = Some(live.machine.upgrades_version());
        game.world.modified.insert((at.0 >> 4, at.2 >> 4));
        // `stack.consume(added, player)`: quem está no criativo não gasta.
        let creative = game
            .native_acting_player()
            .is_some_and(|player| player.game_mode == CREATIVE);
        if !creative {
            let held = game.held_mut();
            let emptied = held.as_mut().is_some_and(|in_hand| {
                in_hand.count = in_hand.count.saturating_sub(added.unsigned_abs());
                in_hand.count == 0
            });
            if emptied {
                *held = None;
            }
        }
    }
    if was_alive {
        LIVE_MACHINES.with_borrow_mut(|machines| machines.insert(key, live));
    }
}

/// `hasAnalogOutputSignal`: toda máquina do mod tem `AttributeComparator`.
pub(in crate::game) fn has_analog_output(level: &GameLevel<'_>, state: BlockStateId, output: &mut bool) {
    if level.native_state_block(state).is_some_and(|block| machine_of(block).is_some()) {
        *output = true;
    }
}

/// `getAnalogOutputSignal`: o `currentRedstoneLevel` do tile.
pub(in crate::game) fn analog_output(dimension: &'static str, pos: [i32; 3], output: &mut i32) {
    if let Some(level) = COMPARATOR_LEVELS.with_borrow(|levels| levels.get(&(dimension, pos)).copied()) {
        *output = level;
    }
}

/// Avisa os comparadores depois do tick do nível: dentro do `block_entity_tick` a máquina está fora do mundo.
pub(in crate::game) fn notify_comparators(game: &mut Game<'_>, dimension: &str) {
    let pending = COMPARATOR_QUEUE.with_borrow_mut(std::mem::take);
    for (at, pos) in pending {
        if at == dimension {
            game.native_update_output_neighbors(dimension, pos);
        }
    }
}

/// O nome destas pastas entre as fontes de dados da lib.
const DATA_SOURCE: &str = "mekanism";
const CHEMICALS_LISTENER: &str = "mekanism:chemicals";
const CHEMICAL_TAGS_LISTENER: &str = "mekanism:chemical_tags";
const UPGRADES_LISTENER: &str = "mekanism:upgrades";
const UPGRADE_TAGS_LISTENER: &str = "mekanism:upgrade_tags";

/// As pastas do original. A lib não sabe delas: o mod lê os arquivos e entrega o resultado.
pub(in crate::game) fn reload_listeners(listeners: &mut Vec<NativeReloadListener>) {
    listeners.push(NativeReloadListener::json(CHEMICALS_LISTENER, rules::CHEMICAL_FOLDER));
    listeners.push(NativeReloadListener::json(CHEMICAL_TAGS_LISTENER, rules::CHEMICAL_TAGS_FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADES_LISTENER, rules::UPGRADE_FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADE_TAGS_LISTENER, rules::UPGRADE_TAGS_FOLDER));
}

pub(in crate::game) fn reload_apply(listener: &NativeReloadListener, files: &[(String, serde_json::Value)]) {
    if listener.id == CHEMICALS_LISTENER {
        let chemicals = files.iter().filter_map(|(id, json)| {
            let chemical = rules::chemical_resource(id, json);
            if chemical.is_none() && id != rules::EMPTY_CHEMICAL {
                eprintln!("mekanism: químico {id} recusado");
            }
            chemical
        });
        let registry = resource::Registry::from_resources(chemicals);
        eprintln!("mekanism: {} químicos", registry.len());
        resource::set_source(DATA_SOURCE, registry);
    } else if listener.id == CHEMICAL_TAGS_LISTENER {
        resource::set_tags_source(DATA_SOURCE, resource::Tags::load(files));
    } else if listener.id == UPGRADES_LISTENER {
        let (types, errors) = upgrade::UpgradeTypes::load(files);
        for (id, error) in errors {
            eprintln!("mekanism: upgrade {id} recusado: {error}");
        }
        upgrade::set_types_source(DATA_SOURCE, types);
    } else if listener.id == UPGRADE_TAGS_LISTENER {
        upgrade::set_tags_source(DATA_SOURCE, upgrade::UpgradeTags::load(files));
    }
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if is_cable(&entity.id) {
        basis::transmit_energy(game, pos, entity);
    } else if let Some(machine) = machine_of(&entity.id) {
        tick_machine(game, pos, entity, machine);
    }
}

/// `native_mod_container_viewers`: a contagem de jogadores com a tela do bloco em `pos` foi de `previous` para `viewers`.
/// Guarda a contagem atual, e se a máquina tem objeto vivo, aplica só a diferença, com `open()` e `close()`. Chega para
/// qualquer mod container: uma posição que não é máquina nossa não tem objeto, então só fica guardada até a contagem zerar.
pub(in crate::game) fn viewers_changed(
    game: &Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    viewers: u32,
    previous: u32,
) {
    // A chave usa a dimensão do jogo, com o mesmo valor (`self.dim.id()`) que o hook recebe. Outra dimensão não é deste nível.
    let dim = game.native_dimension();
    if dim != dimension {
        return;
    }
    // Quem saiu sem fechar o menu (conexão perdida, longe demais) deixa a janela escolhida para trás: larga o que não tem mais menu.
    game.mb_window_prune(pos, viewers);
    let key = (dim, pos);
    VIEWERS.with_borrow_mut(|counts| {
        if viewers == 0 {
            counts.remove(&key);
        } else {
            counts.insert(key, viewers);
        }
    });
    LIVE_MACHINES.with_borrow(|machines| {
        if let Some(live) = machines.get(&key) {
            for _ in previous..viewers {
                live.machine.open();
            }
            for _ in viewers..previous {
                live.machine.close();
            }
        }
    });
}

/// O menu de uma máquina visto pelo `quickMoveStack` do Mekanism: os slots do bloco (`entity.items`, os virtuais da janela no fim)
/// e o inventário do jogador. Um slot do bloco tem o índice dele; o do jogador, `items.len() + índice no inventário` (a barra de
/// atalhos é 0 a 8, o inventário 9 a 35).
struct MachineMenu<'a, 'g> {
    game: &'a Game<'g>,
    entity: &'a mut ModBlockEntity,
    inventory: &'a mut [Option<ItemStack>],
    machine: &'a Machine,
    window: Option<SelectedWindow>,
    indexes: Indexes,
    held: Held,
    upgrades: Rc<upgrade::UpgradeData>,
    selected: SelectedWindow,
}

impl MachineMenu<'_, '_> {
    fn first_player(&self) -> usize {
        self.entity.items.len()
    }

    fn limit(&self, stack: &ItemStack) -> u32 {
        self.game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK)
    }

    /// Se o slot do bloco `slot` aceita o item (`RecipeInputInventorySlot`, `InfusionInventorySlot`, `EnergyInventorySlot` e
    /// `UpgradeInventorySlot.input`): o `isValidForInsertion(.., MANUAL)` de cada um. A saída e a saída de upgrades não aceitam
    /// nada de quem clica.
    fn accepts(&self, slot: usize, stack: &ItemStack) -> bool {
        let slots = &self.machine.slots;
        if slot == slots.input || Some(slot) == slots.infusion || slot == slots.energy {
            return slot_accepts(&self.indexes, slots, slot, &resource_of(self.game, stack), &self.held);
        }
        if slot == slots.upgrade {
            return *stack.item == *rules::UPGRADE_ITEM
                && self.upgrade_type(stack).is_some_and(|kind| self.upgrades.supports(&kind));
        }
        false
    }

    fn upgrade_type(&self, stack: &ItemStack) -> Option<String> {
        match self.game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) {
            Ok(Some(Tag::String(kind))) => Some(kind.to_string_lossy()),
            _ => None,
        }
    }

    fn cell(&mut self, slot: usize) -> &mut Option<ItemStack> {
        let first = self.first_player();
        if slot < first {
            &mut self.entity.items[slot]
        } else {
            &mut self.inventory[slot - first]
        }
    }
}

impl QuickMenu for MachineMenu<'_, '_> {
    type Item = ItemStack;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        let first = self.first_player();
        let slots = &self.machine.slots;
        match region {
            // A ordem do Java: os dois virtuais primeiro, depois os da máquina em `getInitialInventory`.
            Region::Container => {
                let mut order = vec![slots.upgrade, slots.upgrade_output];
                order.extend(slots.infusion);
                order.extend([slots.input, slots.output, slots.energy]);
                order
            }
            Region::Hotbar => (first..first + 9).collect(),
            Region::Main => (first + 9..first + 36).collect(),
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        let first = self.first_player();
        if slot < first {
            Region::Container
        } else if slot - first < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    fn exists(&self, slot: usize) -> bool {
        let slots = &self.machine.slots;
        if slot == slots.upgrade || slot == slots.upgrade_output {
            return SelectedWindow::exists(self.selected, self.window);
        }
        true
    }

    fn stack(&self, slot: usize) -> Option<(ItemStack, u32)> {
        let first = self.first_player();
        let held = if slot < first { self.entity.items[slot].as_ref() } else { self.inventory[slot - first].as_ref() };
        held.filter(|stack| stack.count > 0).map(|stack| (stack.clone(), stack.count))
    }

    fn insert(&mut self, slot: usize, item: &ItemStack, amount: u32) -> u32 {
        if slot < self.first_player() && !self.accepts(slot, item) {
            return 0;
        }
        let limit = self.limit(item);
        match self.cell(slot) {
            Some(held) if held.same_item_same_components(item) => {
                let moved = limit.saturating_sub(held.count).min(amount);
                held.count += moved;
                moved
            }
            Some(_) => 0,
            empty => {
                let moved = amount.min(limit);
                let mut placed = item.clone();
                placed.count = moved;
                *empty = Some(placed);
                moved
            }
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        let cell = self.cell(slot);
        let emptied = match &mut *cell {
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
}

/// Shift-click do menu da máquina: `MekanismContainer.quickMoveStack`. Do inventário vai para o slot da máquina que aceita o item
/// (a entrada, a infusão, e o de upgrades quando a janela do jogador que clica está aberta) e, não cabendo, para a outra região do
/// inventário; da máquina vai para a barra de atalhos e depois o inventário.
pub(in crate::game) fn quick_move(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    container_slots: usize,
    slot: usize,
    stack: &mut ItemStack,
    handled: &mut bool,
) {
    let Some(machine) = machine_of(&entity.id) else {
        return;
    };
    let indexes = indexes_of(game, machine);
    let held = Held {
        chemical: machine.slots.infusion.and_then(|_| read_tank(entity).resource),
        input: entity.items.get(machine.slots.input).and_then(Option::as_ref).map(|stack| resource_of(game, stack)),
    };
    let window = game.mb_window_selected(pos);
    let upgrades = window_upgrade_data();
    let first = entity.items.len();
    // O menu numera o inventário do jogador depois dos slots do bloco: o principal (9 a 35), depois a barra de atalhos (0 a 8).
    let from = if slot < container_slots {
        slot
    } else {
        let n = slot - container_slots;
        first + if n < 27 { 9 + n } else { n - 27 }
    };
    let mut inventory = std::mem::take(&mut game.inventory);
    let (moved, left) = {
        let mut menu = MachineMenu {
            game: &*game,
            entity,
            inventory: &mut inventory,
            machine,
            window,
            indexes,
            held,
            upgrades,
            selected: SelectedWindow::new(rules::UPGRADE_WINDOW, 0),
        };
        let moved = quick_move::quick_move(&mut menu, from);
        (moved, menu.stack(from))
    };
    game.inventory = inventory;
    // O slot clicado guarda o que sobrou; `handled` impede o roteamento do jogo, que levaria o item a outros slots.
    match left {
        Some((remaining, count)) => {
            *stack = remaining;
            stack.count = count;
        }
        None => stack.count = 0,
    }
    let _ = moved;
    *handled = true;
}

pub(in crate::game) fn menu_button(entity: &mut ModBlockEntity, id: i32) {
    if !is_infuser(&entity.id) || id != DUMP_BUTTON {
        return;
    }
    // `dump()`: `dumpOrClearContents` no tanque de infusão, que fora a radiação (que não existe aqui) só o esvazia. O objeto
    // vivo o relê no tick seguinte, como um `setContents` de fora. O pacote age na hora, e o menu sincroniza o tanque ao jogador no
    // mesmo instante, mesmo com o servidor parado: os data slots da tela mudam junto, não no próximo tick.
    if !read_tank(entity).is_empty() {
        write_tank(entity, &TankContents::default());
        if let Some(machine) = machine_of(&entity.id) {
            rules::clear_chemical_screen(machine.screen.data, &mut entity.data);
        }
    }
}

/// `GuiDumpButton` é um `MekanismImageButton` com a mesma ação nos dois botões do mouse: o clique direito também esvazia o tanque.
pub(in crate::game) fn screen_click(block: &str, at: [f32; 2], _right: bool, button: &mut Option<i32>) {
    if is_infuser(block) && inside(DUMP_RECT, at) {
        *button = Some(DUMP_BUTTON);
    }
}

/// O retângulo da barra de energia de uma máquina.
fn energy_bar(machine: &Machine) -> [f32; 4] {
    if machine.slots.infusion.is_some() {
        ENERGY_BAR
    } else {
        ELECTRIC_ENERGY_BAR
    }
}

/// A capacidade que a tela usa: antes do primeiro tick o cliente ainda não recebeu a dela, e vale a do bloco sem upgrades, como o
/// contêiner que o tile do cliente tem desde que nasce.
fn screen_capacity(machine: &Machine, read: &rules::ScreenData) -> u32 {
    if read.capacity == 0 {
        rules::energy_capacity(machine.energy_per_tick) as u32
    } else {
        read.capacity
    }
}

/// Os tooltips dos widgets da tela (`GuiElement.renderToolTip`): o da aba de avisos (`GuiWarningTab.updateTooltip`), o da barra de
/// energia (`GuiVerticalPowerBar`, `EnergyDisplay`) e o da barra de químico (`GuiChemicalBar`, `GuiTankBar.getContentsTooltips`).
/// O `native_screen_tooltip` só entrega texto com os códigos `§` do vanilla e o posiciona sobre o cursor, e o amarelo de
/// `EnumColor.YELLOW` é `0xFFDD4F`. O tooltip de um widget fica por cima ou por baixo dele, então os três saem daqui, pela injeção
/// em `draw_native_screen_tooltip`. Devolve se desenhou. `at` é o cursor relativo ao canto da tela, e `cursor` o mesmo em pixels de
/// GUI da janela.
pub(in crate::game) fn widget_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    block: &str,
    data: &[i32],
    at: [f32; 2],
    cursor: (f32, f32),
) -> bool {
    let Some(machine) = machine_of(block) else {
        return false;
    };
    // A janela de upgrades, quando aberta, e a aba ficam por cima do resto da tela.
    if game.mb_window_tooltip(batch, block, at, cursor) {
        return true;
    }
    let read = rules::ScreenData::read(machine.screen.data, data);
    let translate = |key: &str| lang::get(key);
    let (widget, lines): ([f32; 4], Vec<(String, Option<u32>)>) = if gui::contains(gui::WARNING_TAB_LEFT, at) {
        let view = machine.screen.view(data);
        // Sem aviso a aba some (`updateVisibility`), e não há o que mostrar.
        if !view.tab() {
            return false;
        }
        let lines = rules::tab_tooltip(&view.issues, &translate, &|text: &str| game.native_text_width(text));
        (gui::WARNING_TAB_LEFT, lines.into_iter().map(|line| (line.text, line.color)).collect())
    } else if inside(energy_bar(machine), at) {
        let capacity = screen_capacity(machine, &read);
        let text = rules::energy_tooltip(u64::from(read.energy), u64::from(capacity), &translate);
        (energy_bar(machine), vec![(text, None)])
    } else if machine.slots.infusion.is_some() && inside(INFUSION_BAR, at) {
        let amount = read.chemical_amount.max(0) as u64;
        let id = data_slots::unpack_text(&read.chemical_id);
        let name = (amount > 0).then(|| translate(&rules::chemical_lang_key(&id)));
        (INFUSION_BAR, vec![(rules::chemical_tooltip(name.as_deref(), amount, &translate), None)])
    } else {
        return false;
    };
    // O retângulo do widget na janela: o canto da tela é o cursor menos a posição relativa dele.
    let [x, y, w, h] = widget;
    let rect = [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h];
    basis::menu_tooltip(game, batch, &lines, cursor, rect);
    true
}

/// `GuiSlot.draw` de cada slot de item da máquina: a moldura pelo papel do slot (`findDataType`), ou a do aviso quando a verificação
/// dele diz que sim, e o desenho por cima dela.
fn draw_slots(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2], machine: &Machine, view: &rules::WarningView) {
    for (index, slot) in machine.screen.slots.iter().enumerate() {
        let frame = if slot_warned(machine, view, index) {
            Some(SLOT_WARNING)
        } else {
            machine.screen.slot_type(index).map(slot_sprite)
        };
        basis::slot(game, batch, origin, slot.item, frame, slot.overlay.and_then(overlay_sprite));
    }
}

/// Quais slots de item estão em aviso: o de entrada e o de saída registram o deles, com a verificação do erro de cada um.
fn slot_warned(machine: &Machine, view: &rules::WarningView, index: usize) -> bool {
    (index == machine.slots.input && view.element(rules::ScreenElement::InputSlot))
        || (index == machine.slots.output && view.element(rules::ScreenElement::OutputSlot))
}

/// `GuiVerticalPowerBar`: o nível é `ContainerType.ENERGY.divideToLevel`, e o conteúdo, o sprite de mosaico de 2 por 2.
fn draw_energy_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    machine: &Machine,
    read: &rules::ScreenData,
    rect: [f32; 4],
    warning: Option<&basis::WarningSprites>,
) {
    let level = gui::divide_to_level(f64::from(read.energy), f64::from(screen_capacity(machine, read)));
    basis::vertical_bar(game, batch, &BAR_STYLE, rect, level, warning, &|game, batch, dst| {
        basis::power_bar_contents(game, batch, POWER_BAR_SPRITE, rules::POWER_BAR_TILE, dst);
    });
}

fn draw_infuser(game: &Game<'_>, machine: &Machine, data: &[i32], batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let read = rules::ScreenData::read(machine.screen.data, data);
    let view = machine.screen.view(data);
    let warning = |element| view.element(element).then_some(&WARNING_SPRITES);

    let title = lang::get("container.mekanism.metallurgic_infuser");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, machine, &view);
    draw_energy_bar(game, batch, machine, &read, at(ENERGY_BAR), warning(rules::ScreenElement::EnergyBar));
    let progress = gui::scaled_progress(read.progress, read.ticks_required, rules::TICKS_REQUIRED as i32);
    basis::progress_bar(
        game,
        batch,
        &PROGRESS_RIGHT,
        at(INFUSER_PROGRESS),
        progress,
        PROGRESS_RIGHT_FRAME,
        view.element(rules::ScreenElement::Progress),
    );
    draw_chemical_bar(game, batch, &read, at(INFUSION_BAR), warning(rules::ScreenElement::ChemicalBar));
    sprite_rect(game, batch, DUMP_SPRITE, at(DUMP_RECT));
    // `GuiMekanism.init` a põe por último, depois de todos os elementos.
    game.mb_window_draw_tab(batch, machine.block, origin);
    if view.tab() {
        basis::warning_tab(game, batch, WARNING_TAB_SPRITE, origin);
    }
}

/// `GuiChemicalBar`: o nível é `capacityAsLong`, e o conteúdo o sprite do químico, animado, em mosaico e multiplicado pelo `tint`.
fn draw_chemical_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    read: &rules::ScreenData,
    rect: [f32; 4],
    warning: Option<&basis::WarningSprites>,
) {
    let level = gui::divide_to_level(f64::from(read.chemical_amount.max(0)), rules::TANK_CAPACITY as f64);
    basis::vertical_bar(game, batch, &BAR_STYLE, rect, level, warning, &|game, batch, dst| {
        let Some(icon) = rules::chemical_icon(read.chemical_icon) else {
            return;
        };
        basis::tank_bar_contents(game, batch, icon, [CHEMICAL_FRAME; 2], dst, read.chemical_tint);
    });
}

fn draw_electric(
    game: &Game<'_>,
    electric: &Machine,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let read = rules::ScreenData::read(electric.screen.data, data);
    let view = electric.screen.view(data);

    let title = lang::get(electric.title);
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, electric, &view);
    sprite_rect(game, batch, UP_ARROW, at(ELECTRIC_ARROW));
    let warning = view.element(rules::ScreenElement::EnergyBar).then_some(&WARNING_SPRITES);
    draw_energy_bar(game, batch, electric, &read, at(ELECTRIC_ENERGY_BAR), warning);
    let progress = gui::scaled_progress(read.progress, read.ticks_required, rules::TICKS_REQUIRED as i32);
    basis::progress_bar(
        game,
        batch,
        &PROGRESS_BAR_TYPE,
        at(ELECTRIC_PROGRESS_BAR),
        progress,
        PROGRESS_BAR_FRAME,
        view.element(rules::ScreenElement::Progress),
    );
    game.mb_window_draw_tab(batch, electric.block, origin);
    if view.tab() {
        basis::warning_tab(game, batch, WARNING_TAB_SPRITE, origin);
    }
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if let Some(machine) = machine_of(block) {
        if is_infuser(block) {
            draw_infuser(game, machine, data, batch, origin);
        } else {
            draw_electric(game, machine, data, batch, origin);
        }
    }
}
