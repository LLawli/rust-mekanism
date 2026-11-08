// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::mekanism_rules::{Held, Indexes};
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
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalIngredient, ChemicalResource, ConversionCache, TankContents};
use machinery_basis_core::composter::{self, ComposterBlock, ComposterWrapper};
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::gui;
use machinery_basis_core::menu_source;
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::slot_rules;
use machinery_basis_core::resource;
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::tile::{ProgressMachine, RECIPE_CHECK_FREQUENCY, RedstoneControl, SavedState};
use machinery_basis_core::quick_move::{self, QuickMenu, Region};
use machinery_basis_core::recipe_cache::{ComponentsKey, IngredientShape, ShapedIngredient, TypedInstance};
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use machinery_basis_core::container_sync::{Mirror, StackValue, Tracked, TrackedSource};
use machinery_basis_core::machine_sync::MachineSync;
use machinery_basis_core::upgrade_window::UpgradeWindowSpec;
use machinery_basis_core::window::SelectedWindow;
use machinery_basis_core::upgrade::{self, UpgradeSaved, UpgradeTags, UpgradeTypes};
use machinery_basis_core::slot_view::{SlotOverlay, SlotType};
use basis::RecipeSource::{Cooking, Custom};
use std::cell::{Cell, RefCell};
use horizon_core::geometry::Direction;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DUMP_BUTTON: i32 = 0;
/// `Tooltip.MAX_WIDTH`: onde `Tooltip.create` quebra o texto.
const TOOLTIP_WRAP: i32 = 170;
/// `GuiInteraction.COLLECT_XP`: o botão da aba de xp.
const XP_BUTTON: i32 = 1;
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

impl Slots {
    /// Os slots que as regras de inserção conhecem.
    fn rule_slots(&self) -> rules::SlotIndexes {
        rules::SlotIndexes { input: self.input, energy: self.energy, infusion: self.infusion }
    }
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
    /// `AttributeSound`: o evento do som da máquina.
    sound: &'static str,
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
    sound: &'static str,
) -> Machine {
    Machine {
        block,
        title,
        recipes,
        energy_per_tick,
        slots: ELECTRIC_SLOT_LAYOUT,
        screen: &rules::ELECTRIC_SCREEN,
        sound,
    }
}

static MACHINES: [Machine; 4] = [
    electric(
        rules::ENRICHMENT_CHAMBER,
        "container.mekanism.enrichment_chamber",
        Custom(rules::ENRICHING, rules::ELECTRIC_FIELDS),
        rules::ENRICHMENT_USAGE,
        rules::ENRICHMENT_SOUND,
    ),
    electric(
        rules::CRUSHER,
        "container.mekanism.crusher",
        Custom(rules::CRUSHING, rules::ELECTRIC_FIELDS),
        rules::CRUSHER_USAGE,
        rules::CRUSHER_SOUND,
    ),
    electric(
        rules::ENERGIZED_SMELTER,
        "container.mekanism.energized_smelter",
        Cooking(rules::SMELTING),
        rules::SMELTER_USAGE,
        rules::SMELTER_SOUND,
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
        sound: rules::INFUSER_SOUND,
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
    if is_machine(block) {
        // Os blocos do `TileEntityMekanism` com `supportsRedstone()`: o jogo os avisa quando um vizinho muda (`basis::neighbor_changed`).
        basis::watch_redstone(REDSTONE_OWNER, is_machine);
        // A janela de upgrades do bloco (`ItemUpgrade.useOn`, `TileComponentUpgrade.addUpgrades`) vale desde que o bloco existe, e não só depois
        // de a tela dele abrir: o jogo pergunta por este hook ao criar o tile, antes de qualquer uso do item.
        if let Some(spec) = window_spec(block) {
            Game::mb_window_register(spec, remove_upgrade, add_upgrades, window_source);
        }
    }
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
        // O cliente pergunta a cada clique se ele cai sobre uma aba (`GuiMekanism.hasClickedOutside`), com os data slots de agora.
        machinery_basis_core::screen_elements::declare_once(block, || rules::screen_elements(*machine.screen));
    }
}

// A janela de upgrades de cada máquina: só dados. A mecânica (aba, janela, slots virtuais) é da machinery_basis.
const fn window_spec_of(block: &'static str, slots: &Slots, trackers: fn() -> Vec<Tracked>) -> UpgradeWindowSpec {
    UpgradeWindowSpec {
        block,
        virtual_slots: [slots.upgrade, slots.upgrade_output],
        trackers,
        window_trackers: rules::upgrade_window_data,
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
    window_spec_of(rules::ENRICHMENT_CHAMBER, &ELECTRIC_SLOT_LAYOUT, rules::electric_trackers),
    window_spec_of(rules::CRUSHER, &ELECTRIC_SLOT_LAYOUT, rules::electric_trackers),
    window_spec_of(rules::ENERGIZED_SMELTER, &ELECTRIC_SLOT_LAYOUT, rules::electric_trackers),
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
        rules::infuser_trackers,
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
    // `recalculateUpgrades` → `xpContainmentChanged`: a experiência guardada cai ao tirar o upgrade (se ainda tiver contenção).
    let drops = live.machine.take_experience_drops();
    pop_experience(game, center_of(pos), &drops);
    sync_muffling(game, key.0, pos, &mut live);
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
            basis::write_upgrades(entity, &live.machine.upgrades().saved());
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
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
    // Se o objeto vivo que a varredura tirou já tinha recalculado a capacidade (`capacity_recalculated`). O tile do Java continua o
    // mesmo quando o port larga o objeto de uma máquina que ficou sem andar; o objeto novo recebe isto de volta.
    static SWEPT: RefCell<BTreeMap<(&'static str, [i32; 3]), bool>> = const { RefCell::new(BTreeMap::new()) };
}

/// Quantos ticks do jogo uma máquina posta por um item espera o primeiro tick antes de a marca se perder.
const PLACED_TICKS: i64 = 200;

/// As máquinas que um item acabou de pôr e ainda não tiveram o primeiro tick: `TileEntityMekanism.applyImplicitComponents` roda
/// na colocação, e só nela (um `setBlock` de comando ou de estrutura, e um tile lido do NBT, não passam por ali). A marca vale para
/// o processo todo, e não só para a thread: quem põe o bloco e quem anda a máquina são threads diferentes no jogo de um jogador só.
static PLACED: Mutex<BTreeMap<(&'static str, [i32; 3]), i64>> = Mutex::new(BTreeMap::new());

fn note_placed(game: &Game<'_>, pos: [i32; 3]) {
    let now = game.world.game_time;
    let mut placed = PLACED.lock().unwrap_or_else(|e| e.into_inner());
    placed.retain(|_, at| now.saturating_sub(*at) <= PLACED_TICKS);
    placed.insert((game.native_dimension(), pos), now);
}

/// Se o objeto vivo que nasce em `pos` vem de um item que acabou de pôr a máquina. `consume` apaga a marca: o primeiro tick da
/// máquina a consome, e quem só quer um objeto provisório (o upgrade posto antes desse tick) a deixa para ele.
fn placed_here(game: &Game<'_>, pos: [i32; 3], consume: bool) -> bool {
    let now = game.world.game_time;
    let mut placed = PLACED.lock().unwrap_or_else(|e| e.into_inner());
    let key = (game.native_dimension(), pos);
    let at = if consume { placed.remove(&key) } else { placed.get(&key).copied() };
    at.is_some_and(|at| now.saturating_sub(at) <= PLACED_TICKS)
}

/// Os nomes de `SerializationConstants` que o Java grava, em tags simples no NBT da `ModBlockEntity`. `REDSTONE` é a
/// marca de que o tile já foi salvo: o Java a escreve sempre.
const REDSTONE: &str = "redstone";
/// O nome com que o mod diz à `machinery_basis` quais blocos têm `redstone`.
const REDSTONE_OWNER: &str = "mekanism";
const CONTROL_TYPE: &str = "control_type";
const ACTIVE_STATE: &str = "active_state";
const UPDATE_DELAY: &str = "update_delay";
const CURRENT_REDSTONE: &str = "current_redstone";
const PROGRESS: &str = "progress";
const USED_SO_FAR: &str = "used_so_far";
const RECIPES_USED: &str = "recipes_used";
const LAST_RECIPE_USED: &str = "last_recipe_used";
const LAST_RECIPE_AMOUNT: &str = "last_recipe_amount";
/// `SerializationConstants.HAS_CONTAINMENT`: se o upgrade de contenção de xp estava instalado.
const HAS_CONTAINMENT: &str = "has_containment";
/// `SerializationConstants.EJECT + TransmissionType.ITEM.ordinal()`: ENERGY, FLUID, CHEMICAL, ITEM.
const EJECT_ITEM: &str = "eject3";
/// A chave que liga o ejetor por comando: `/data merge block x y z {mekanism_eject:1}`.
const EJECT_SWITCH: &str = "mekanism_eject";
/// A face de saída do ejetor, enquanto a configuração de lados (Fase 2) não existe: `/data merge block x y z
/// {mekanism_eject_face:"down"}` (`up`, `down`, ou o nome de uma face horizontal). Sem a chave sai pela direita, como o item posto.
const EJECT_FACE: &str = "mekanism_eject_face";
const COMPOSTER: &str = "minecraft:composter";
const BONE_MEAL: &str = "minecraft:bone_meal";
/// `ChemicalContainerType.getTag()` e o que `IListContainerType.saveTo` e `LargeResourceStack.codec` gravam: uma lista de
/// `{container: byte, stored: {type, amount}}`, sem o item da lista quando o tanque está vazio, e sem a lista inteira.
const CHEMICAL_TANKS: &str = "chemical_tanks";
const CONTAINER: &str = "container";
const STORED: &str = "stored";
const TYPE: &str = "type";
const AMOUNT: &str = "amount";
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
    lit: Option<bool>,
    /// Quantos jogadores o objeto sabe que têm a tela aberta (`playersUsing`): a diferença para as sessões vira `open()` e `close()`.
    players: usize,
    /// A face de saída do ejetor, a direita da frente.
    eject_side: Option<Face>,
    /// Os tipos e as tags de upgrade de onde os dados da máquina saíram: uma carga nova de dados os troca.
    upgrade_types: Arc<UpgradeTypes>,
    upgrade_tags: Arc<UpgradeTags>,
    /// Quando os upgrades e os slots mudam, o NBT é escrito de novo.
    upgrades_version: Option<u32>,
    /// A capacidade que o Horizon já sabe desta máquina.
    energy_capacity: Option<i32>,
    /// Quantos upgrades de silêncio o cliente já sabe que o tile tem (`MUFFLING_COUNT` do pacote de atualização).
    muffling_sent: i32,
}

/// A experiência de cada receita da máquina, por id.
fn experience_of(recipes: &Arc<Vec<basis::MachineRecipe>>) -> machinery_basis_core::tile::ExperienceLookup {
    let recipes = recipes.clone();
    Rc::new(move |id: &str| recipes.iter().find(|recipe| recipe.id == id).map(|recipe| recipe.experience))
}

/// `AbstractFurnaceBlockEntity.createExperience` + `ExperienceOrb.award`: as bolas de experiência de uma remessa de receitas, em
/// `at`. O sorteio da parte fracionária é o `Math.random()` do Java (`java_random::math_random`, o mesmo gerador): não há semente a
/// reproduzir, e a distribuição é a mesma, uniforme em `[0, 1)`.
fn pop_experience(game: &mut Game<'_>, at: [f64; 3], drops: &[machinery_basis_core::experience::ExperienceDrop]) {
    for drop in drops {
        let points = drop.points(machinery_basis_core::java_random::math_random());
        if points > 0 {
            game.spawn_orbs(at, points as u32);
        }
    }
}

/// `Vec3.atCenterOf(pos)`.
fn center_of(pos: [i32; 3]) -> [f64; 3] {
    [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5]
}

/// `ItemResource.of(stack)`: o item, a pilha máxima e os componentes que o distinguem (nome, dados, dano...).
fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    basis::item_resource(game, stack)
}

/// O ingrediente de uma receita do jogo e, se é um `neoforge:components`, os componentes dele.
type Source<'a> = (&'a Ingredient, Option<&'a Arc<basis::ComponentsInput>>);

fn recipe_ingredient(recipe: &basis::MachineRecipe) -> Source<'_> {
    (&recipe.item, recipe.components.as_ref())
}

fn conversion_ingredient(conversion: &basis::ItemConversion) -> Source<'_> {
    (&conversion.item, conversion.components.as_ref())
}

fn energy_conversion_ingredient(conversion: &basis::ItemEnergyConversion) -> Source<'_> {
    (&conversion.item, conversion.components.as_ref())
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
/// qualquer outro é complexo (`Custom`). O `DataComponentIngredient` não vem do Horizon: o mod o lê do arquivo (`basis::ComponentsInput`).
fn item_shape(ingredient: &Ingredient, components: Option<&Arc<basis::ComponentsInput>>) -> IngredientShape {
    // O `DataComponentIngredient` (`neoforge:components`) o Horizon não entrega: o mod o lê do arquivo da receita. Com `strict` é exato
    // (`addComponentInputCache`), senão complexo.
    if let Some(components) = components {
        return components.shape();
    }
    if is_plain(ingredient) {
        return IngredientShape::Types(ingredient.order.iter().map(|item| Rc::from(item.as_str())).collect());
    }
    match &ingredient.children {
        Some((Shape::Compound, children)) if ingredient.custom.is_none() => {
            IngredientShape::Compound(children.iter().map(|child| item_shape(child, None)).collect())
        }
        _ => IngredientShape::Custom,
    }
}

/// O ingrediente do Horizon (item ou tag) vira o da lib, com a classificação do índice e o teste de tipo (`testType`, sem a
/// quantidade), e guarda a última resposta, porque a receita em cache pergunta todo tick pelo mesmo item. `source` e `index` dizem
/// de qual entrada da lista ele sai.
fn item_ingredient<T: 'static>(source: &Arc<Vec<T>>, index: usize, ingredient: fn(&T) -> Source<'_>) -> ItemIngredient {
    let components = ingredient(&source[index]).1.cloned();
    let shape = {
        let (found, components) = ingredient(&source[index]);
        item_shape(found, components)
    };
    let source = source.clone();
    // A última resposta, por item e, num ingrediente de componentes, pelo patch da entrada.
    let last: RefCell<Option<(Rc<str>, Option<Rc<str>>, bool)>> = RefCell::new(None);
    ItemIngredient::shaped(ShapedIngredient::new(shape, move |input: &dyn TypedInstance| {
        let id = input.type_id();
        let patch = match input.components_key() {
            ComponentsKey::Patch(key) if components.is_some() => Some(key),
            _ => None,
        };
        if let Some((held, held_patch, answer)) = &*last.borrow()
            && **held == *id
            && held_patch.as_deref() == patch
        {
            return *answer;
        }
        let answer = match &components {
            Some(components) => components.accepts(id, patch),
            None => {
                let stack = ItemStack {
                    item: Arc::from(id),
                    count: 1,
                    damage: 0,
                    extra: None,
                };
                ingredient(&source[index]).0.test(&stack)
            }
        };
        *last.borrow_mut() = Some((Rc::from(id), patch.map(Rc::from), answer));
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
            id: conversion.id.clone(),
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
            id: conversion.id.clone(),
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

/// O item como o `TypedInstance` que o índice pergunta (`ItemResource`): só o id conta para o tipo.
fn typed_item(stack: &ItemStack) -> ItemResource {
    ItemResource::new(&stack.item, UNKNOWN_STACK)
}

/// Se o slot `slot` da máquina aceita o item (veja `rules::slot_accepts`): a regra do hopper vizinho, a do shift-click e a do clique
/// do cursor são a mesma.
fn slot_accepts(indexes: &Indexes, slots: &Slots, slot: usize, item: &ItemResource, held: &Held) -> bool {
    rules::slot_accepts(indexes, &slots.rule_slots(), slot, item, held)
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
        had_containment: nbt_bool(entity, HAS_CONTAINMENT),
        ejecting: nbt_bool(entity, EJECT_ITEM),
        upgrades: basis::read_upgrades(game, entity),
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
    put(entity, HAS_CONTAINMENT, Tag::Byte(i8::from(new.had_containment)));
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

/// De onde vem o tile que o objeto vivo novo representa.
struct Origin {
    /// Um item acabou de pôr a máquina e ela ainda não andou.
    placed: bool,
    /// O tile é o mesmo de antes (o Horizon ainda tem os data slots que o objeto velho escreveu) e o objeto velho já tinha recalculado
    /// a capacidade.
    recalculated_before: bool,
}

/// `ProgressMachine` nova, do que a `ModBlockEntity` guarda. Com o índice do Infuser é o Metallurgic Infuser.
fn born(
    game: &Game<'_>,
    pos: [i32; 3],
    entity: &ModBlockEntity,
    machine: &Machine,
    indexes: &Indexes,
    origin: Origin,
) -> Live {
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
    // `loadAdditional` só com o que o mundo guardou. Um tile novo não lê nada, e o de um item recalcula os upgrades
    // (`applyImplicitComponents`): uma máquina de `setBlock` fica com a capacidade do construtor, quatro ticks de consumo.
    progress.restore(&read_saved(game, entity), block_active);
    if (origin.placed && !has_saved(entity)) || (origin.recalculated_before && !progress.capacity_recalculated()) {
        progress.apply_item_components(&UpgradeSaved::default());
    }
    progress.set_experience_lookup(experience_of(&basis::machine_recipes(game, &machine.recipes)));
    // Quem já tinha a tela aberta antes de o objeto existir: as sessões são aplicadas uma vez, aqui, num objeto novo.
    let players = Game::mb_container_players(pos);
    for _ in 0..players {
        progress.open();
    }
    Live {
        machine: progress,
        generation: basis::data_generation(),
        last_seen: i64::MIN,
        saved: None,
        used_version: None,
        lit: None,
        players,
        eject_side: None,
        upgrade_types,
        upgrade_tags,
        upgrades_version: None,
        energy_capacity: None,
        // Um tile novo avisa o cliente na primeira vez, mesmo com zero: o `getUpdateTag` do Java sempre leva a contagem.
        muffling_sent: -1,
    }
}

/// O que a `ModBlockEntity` mudou fora do tick vira `setContents` no slot, que dispara os listeners como o Java. O slot guarda um
/// recurso, o item e os componentes: o mesmo item com outro nome é outro recurso.
fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let resource = resource_of(game, stack);
            if slot.resource().as_ref() != Some(&resource) || slot.amount() != stack.count {
                slot.set_contents(Some(resource), stack.count, None);
            }
        }
    }
}

/// O que o tick mudou vai para a `ModBlockEntity`. Uma pilha do mesmo recurso (o item e os componentes) guarda o que tinha; um
/// recurso novo é refeito com o dano e os componentes que ele tem, que são os da receita quando é a saída dela.
fn slot_to_entity(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource == resource_of(game, stack) && contents.amount == stack.count,
        _ => false,
    };
    if same {
        return;
    }
    let new = contents.resource.map(|resource| {
        let mut stack = match current {
            Some(old) if resource_of(game, old) == resource => old.clone(),
            _ => basis::stack_of_resource(&resource, contents.amount).unwrap_or_else(|| {
                eprintln!("mekanism: o recurso {} perdeu os componentes ao sair do slot", resource.id);
                ItemStack::new(&resource.id, contents.amount)
            }),
        };
        stack.count = contents.amount;
        stack
    });
    entity.items[index] = new;
}

/// Os slots de upgrade (os virtuais do menu) lidos do que a tela mudou, como `slot_from_entity`.
fn upgrade_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    slot_from_entity(game, slot, stack);
}

/// O item que um slot de upgrade guarda, com o tipo no componente.
fn upgrade_slot_stack(game: &Game<'_>, slot: &InventorySlot) -> Option<ItemStack> {
    basis::upgrade_slot_stack(game, rules::UPGRADE_TYPE_COMPONENT, &slot.contents())
}

/// O que o tick mudou nos slots de upgrade (instalar tira da entrada, remover enche a saída) volta para o menu.
fn upgrade_to_entity(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot) {
    basis::upgrade_to_cell(game, rules::UPGRADE_TYPE_COMPONENT, &mut entity.items[index], &slot.contents());
}

/// Os vizinhos como o ejetor os vê: o inventário do outro lado, copiado do jogo, com a regra real de cada slot para o item que
/// está saindo. O Horizon não simula inserção, então o diário guarda a cópia, e o que entrou nela vai ao vizinho de verdade, por
/// `native_insert_item`, depois que a transação commitou. O ejetor roda antes da receita: só o item da saída pode sair no tick.
/// O jogo vem num `RefCell` porque o sorteio das camadas de uma composteira muda o jogo (o sorteio do nível).
struct GameNeighbours<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
    pos: [i32; 3],
    offered: Option<(ItemResource, ItemStack)>,
    created: RefCell<Vec<CopiedContainer>>,
    composters: RefCell<Vec<CopiedComposter>>,
}

struct CopiedContainer {
    at: [i32; 3],
    reached: Direction,
    container: Rc<VanillaContainer>,
    original: Vec<(usize, ItemResource, u32)>,
}

/// Uma composteira vizinha, vista pelo `ComposterWrapper`: o nível que o jogo tem, o sorteio das camadas do item que sai (feito
/// quando o ejetor pede a capacidade, e uma vez só: o Java sorteia uma vez por tentativa de inserir) e o nível a que o wrapper
/// levou o bloco ao commitar. A lib decide tudo; o efeito no mundo só é aplicado depois, por [`GameNeighbours::finish`].
struct GameComposter {
    level: Cell<i32>,
    drawn: Option<(ItemResource, Option<i32>)>,
    committed: Cell<Option<i32>>,
}

impl ComposterBlock for GameComposter {
    fn exists(&self) -> bool {
        true
    }

    fn level(&self) -> i32 {
        self.level.get()
    }

    fn set_level(&self, level: i32, notify: bool) {
        self.level.set(level);
        if notify {
            self.committed.set(Some(level));
        }
    }

    fn layers(&self, resource: &ItemResource) -> Option<i32> {
        match &self.drawn {
            Some((offered, layers)) if offered == resource => *layers,
            _ => None,
        }
    }

    // O tick agendado e o evento de nível saem do bloco de verdade, na hora de aplicar (`machinery_composter_raise`).
    fn schedule_tick(&self, _ticks: u32) {}

    fn level_event(&self, _rose: bool) {}
}

struct CopiedComposter {
    at: [i32; 3],
    before: i32,
    block: Rc<GameComposter>,
}

impl ItemCapabilities for GameNeighbours<'_, '_> {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        let [x, y, z] = face.offset();
        let at = [self.pos[0] + x, self.pos[1] + y, self.pos[2] + z];
        let reached = basis::direction(face.opposite());
        let slots = self.game.borrow().native_container_slots(self.dimension, at, reached);
        let Some(slots) = slots else {
            return self.composter_handler(at, face);
        };
        let game = self.game.borrow();
        let game: &Game<'_> = &game;
        let container = VanillaContainer::new(slots.len(), Rc::new(|| {}));
        for (index, slot) in slots.iter().enumerate() {
            if let Some(stack) = &slot.stack {
                container.set_item(index, Some(resource_of(game, stack)), stack.count);
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
            Some((resource, stack)) => (
                Some(resource.clone()),
                slots
                    .iter()
                    .map(|slot| game.machinery_slot_accepts(at, slot.slot, stack, reached))
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        Some(Rc::new(RuledHandler::new(container, move |index, resource: &ItemResource| {
            offered.as_ref() == Some(resource) && accepted.get(index).copied().unwrap_or(false)
        })))
    }
}

impl GameNeighbours<'_, '_> {
    /// A composteira do outro lado, se há uma (`Capabilities.Item.BLOCK` dela, `ComposterWrapper.get`): o Horizon não a mostra em
    /// `native_container_slots`. Só as faces de cima e de baixo dela têm capacidade, e a lib sabe isso. O item que sai é sorteado
    /// aqui, uma vez, e só quando o wrapper chegaria a sortear: composteira abaixo do nível 7 e item com o componente.
    fn composter_handler(&self, at: [i32; 3], face: Face) -> Option<Rc<dyn ItemHandler>> {
        let mut game = self.game.borrow_mut();
        if game.native_block(at) != COMPOSTER {
            return None;
        }
        let level = game.native_block_property(at, "level")?.parse::<i32>().ok()?;
        let drawn = match &self.offered {
            Some((resource, stack)) if face.opposite() == Face::Up && level < composter::MAX_LEVEL => {
                Some((resource.clone(), game.machinery_composter_layers(at, stack)))
            }
            Some((resource, _)) => Some((resource.clone(), None)),
            None => None,
        };
        let block = Rc::new(GameComposter {
            level: Cell::new(level),
            drawn,
            committed: Cell::new(None),
        });
        self.composters.borrow_mut().push(CopiedComposter {
            at,
            before: level,
            block: block.clone(),
        });
        let bone_meal = basis::item_resource(&game, &ItemStack::new(BONE_MEAL, 1));
        ComposterWrapper::new(block, bone_meal).handler(face.opposite())
    }
}

/// O que entrou nas cópias durante o tick: por vizinho, quanto de cada recurso; e as composteiras a que o wrapper subiu o nível.
struct Ejected {
    at: [i32; 3],
    reached: Direction,
    resource: ItemResource,
    count: u32,
}

impl GameNeighbours<'_, '_> {
    fn finish(self) -> (Vec<Ejected>, Vec<([i32; 3], i32)>) {
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
                        resource,
                        count: amount - before,
                    });
                }
            }
        }
        let raised = self
            .composters
            .into_inner()
            .into_iter()
            .filter_map(|copied| {
                let rose = copied.block.committed.get()? - copied.before;
                (rose > 0).then_some((copied.at, rose))
            })
            .collect();
        (ejected, raised)
    }
}

/// Entrega ao vizinho de verdade o que o diário mandou. A regra de cada slot já foi perguntada e nada mudou no vizinho desde
/// então, então ele aceita tudo. O resto, se houver, volta ao slot de saída da `ModBlockEntity`. As composteiras sobem o nível
/// como o commit do wrapper manda: de um em um, com os avisos, o tick agendado e o evento de enchimento.
fn deliver(
    game: &mut Game<'_>,
    entity: &mut ModBlockEntity,
    output_slot: usize,
    ejected: Vec<Ejected>,
    raised: Vec<([i32; 3], i32)>,
) {
    for (at, levels) in raised {
        for _ in 0..levels {
            game.machinery_composter_raise(at);
        }
    }
    for sent in ejected {
        let stack = basis::stack_of_resource(&sent.resource, sent.count).unwrap_or_else(|| {
            eprintln!("mekanism: o recurso {} perdeu os componentes ao sair para o vizinho", sent.resource.id);
            ItemStack::new(&sent.resource.id, sent.count)
        });
        let Some(rest) = game.native_insert_item(sent.at, sent.reached, stack) else {
            continue;
        };
        let slot = &mut entity.items[output_slot];
        match slot.as_mut() {
            None => *slot = Some(rest),
            Some(held) if held.same_item_same_components(&rest) => held.count += rest.count,
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

/// A face por onde o ejetor sai: a direita da frente, ou a que `mekanism_eject_face` manda.
fn eject_face(entity: &ModBlockEntity, front: Face) -> Face {
    match entity.nbt.get(&JavaString::from(EJECT_FACE)) {
        Some(Tag::String(name)) => Face::from_property(&name.to_string_lossy()).unwrap_or(front.right_of()),
        _ => front.right_of(),
    }
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
    flush_deferred_awards(game);
    result_take_after(game, pos, entity, spec);
    basis::name_apply_pending(entity, pos);
    let alive = LIVE_MACHINES.with_borrow_mut(|machines| machines.remove(&key));
    // O tile é o mesmo se o Horizon ainda guarda a marca que o tick anterior pôs (um tile lido do disco nasce sem ela).
    let continuing = (!entity.data.is_empty())
        .then(|| match &alive {
            Some(live) => Some(live.machine.capacity_recalculated()),
            None => SWEPT.with_borrow_mut(|swept| swept.remove(&key)),
        })
        .flatten();
    let mut fresh = false;
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && has_saved(entity) => live,
        _ => {
            if slots.infusion.is_some() {
                migrate_old_tank(entity);
            }
            fresh = true;
            let origin = Origin { placed: placed_here(game, pos, true), recalculated_before: continuing == Some(true) };
            born(game, pos, entity, spec, &indexes, origin)
        }
    };
    // Uma carga nova de dados limpou os índices (`MekanismRecipeType.clearCache`) e o monitor larga a receita que tinha
    // (`invalidateCache`).
    let generation = basis::data_generation();
    if live.generation != generation {
        live.machine.invalidate_recipe_cache();
        live.machine.set_experience_lookup(experience_of(&recipes));
        live.generation = generation;
    }
    let (upgrade_types, upgrade_tags) = (upgrade::current_types(), upgrade::current_tags());
    if !Arc::ptr_eq(&live.upgrade_types, &upgrade_types) || !Arc::ptr_eq(&live.upgrade_tags, &upgrade_tags) {
        live.machine.upgrades().set_data(upgrade_data(game, &upgrade_types, &upgrade_tags));
        live.upgrade_types = upgrade_types;
        live.upgrade_tags = upgrade_tags;
    }
    // `MekanismTileContainer.openInventory` e `closeInventory`: quem abriu ou fechou a tela desde o último tick entra ou sai do `playersUsing`.
    let players = Game::mb_container_players(pos);
    while live.players < players {
        live.machine.open();
        live.players += 1;
    }
    while live.players > players {
        live.machine.close();
        live.players -= 1;
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
    // `updatePower` só roda quando o jogo avisa o bloco (`BlockTile.neighborChanged`, `onNeighborChange`, `onAdded`): o `redstone` não é lido
    // a cada tick. O que o aviso deixou chega aqui, antes de qualquer leitura do tick.
    if let Some(powered) = basis::take_power(game, pos) {
        live.machine.tile_mut().update_power(powered);
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
    let side = eject_face(entity, front);
    if live.machine.ejector().is_ejecting() && live.eject_side != Some(side) {
        live.machine.ejector_mut().set_sides(0, vec![side]);
        live.eject_side = Some(side);
    }

    let offered = live.machine.output().resource().and_then(|resource| {
        let stack = basis::stack_of_resource(&resource, 1);
        if stack.is_none() {
            eprintln!("mekanism: o recurso {} da saída não tem pilha no jogo, não sai", resource.id);
        }
        Some((resource, stack?))
    });
    let neighbours = GameNeighbours {
        game: RefCell::new(&mut *game),
        dimension,
        pos,
        offered,
        created: RefCell::new(Vec::new()),
        composters: RefCell::new(Vec::new()),
    };
    let comparator_changed = live.machine.tick_server(now as u64, &neighbours);
    let (ejected, raised) = neighbours.finish();
    // O fim de `TileEntityMekanism.tickServer` (`:645-652`): o nível do comparador mudou, e os vizinhos são avisados dentro deste tick, antes do
    // ticker seguinte (o Java chama `notifyComparatorChange` ali, e uma máquina vizinha que tica depois já vê o aviso).
    publish_comparator(game, dimension, pos, live.machine.tile().current_redstone_level(), comparator_changed);
    // Um upgrade de contenção de xp instalado neste tick (`recalculateUpgrades`) pode ter soltado a experiência guardada.
    let drops = live.machine.take_experience_drops();
    if !drops.is_empty() {
        pop_experience(game, center_of(pos), &drops);
    }
    sync_muffling(game, dimension, pos, &mut live);

    let machine = &live.machine;
    slot_to_entity(game, entity, slots.input, machine.input());
    slot_to_entity(game, entity, slots.output, machine.output());
    slot_to_entity(game, entity, slots.energy, machine.energy_slot());
    upgrade_to_entity(game, entity, slots.upgrade, machine.upgrades().upgrade_slot());
    upgrade_to_entity(game, entity, slots.upgrade_output, machine.upgrades().upgrade_output_slot());
    if let (Some(side), Some(index)) = (machine.chemical_side(), slots.infusion) {
        slot_to_entity(game, entity, index, &side.slot);
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
        basis::write_upgrades(entity, &machine.upgrades().saved());
        live.upgrades_version = Some(upgrades_version);
    }
    deliver(game, entity, slots.output, ejected, raised);
    if let Some(started) = started {
        basis::bench_record(now, u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let chest = machine
            .ejector()
            .is_ejecting()
            .then(|| chest_of(game, dimension, pos, side));
        basis::trace(&rules::trace_line(now, machine, chest.as_deref(), true));
    }

    let lit = machine.tile().block_active();
    if live.lit != Some(lit) {
        game.native_set_block_property(pos, "lit", if lit { "true" } else { "false" });
        live.lit = Some(lit);
    }
    // O que a tela mostra não sai deste tick: o Java lê o valor vivo de cada dado no `broadcastChanges` do tick seguinte (`window_source`).
    // O Horizon só guarda o `data` do tile enquanto o objeto é o mesmo; a marca diz ao tick seguinte que ele continua.
    if entity.data.is_empty() {
        entity.data = vec![TILE_TICKED];
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

    live.last_seen = now;
    LIVE_MACHINES.with_borrow_mut(|machines| {
        machines.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            let stale: Vec<_> = machines
                .iter()
                .filter(|(_, live)| now - live.last_seen > STALE_TICKS)
                .map(|(key, live)| (*key, live.machine.capacity_recalculated()))
                .collect();
            for (key, recalculated) in stale {
                machines.remove(&key);
                SWEPT.with_borrow_mut(|swept| swept.insert(key, recalculated));
            }
            COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.retain(|key, _| machines.contains_key(key)));
            basis::retain_energy_capacities(|dimension, pos| machines.contains_key(&(dimension, pos)));
        }
    });
}

/// O nível que `getAnalogOutputSignal` devolve (`currentRedstoneLevel`) e, quando `tickServer` o mudou, `notifyComparatorChange`
/// (`Level.updateNeighbourForOutputSignal`): os comparadores em volta releem, e as máquinas vizinhas recebem o `onNeighborChange` do
/// NeoForge (`updatePower`). O objeto vivo está fora da tabela durante o próprio tick (e o Horizon tira o tile do mundo), e um
/// comparador pode perguntar nessa hora, por isso o nível fica numa tabela à parte, posta antes do aviso.
fn publish_comparator(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], level: i32, changed: bool) {
    COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert((dimension, pos), level));
    if changed {
        game.native_update_output_neighbors(dimension, pos);
        basis::on_neighbor_change(game, dimension, pos);
    }
}

/// `NativeComponentTypes`: o componente que guarda o tipo do upgrade no item (`MekanismDataComponents.UPGRADE_TYPE`).
pub(in crate::game) fn component_types(types: &mut Vec<String>) {
    types.push(rules::UPGRADE_TYPE_COMPONENT.to_owned());
}

/// `TileComponentUpgrade.addUpgrades`, que `ItemUpgrade.useOn` (na `machinery_basis`) chama depois de conferir o suporte e o shift:
/// quantos upgrades do tipo entraram na máquina em `pos`, de até `available`.
fn add_upgrades(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, available: i32) -> i32 {
    let block = game.native_block(pos);
    let Some(spec) = machine_of(&block) else {
        return 0;
    };
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
                return 0;
            };
            let origin = Origin { placed: placed_here(game, pos, false), recalculated_before: false };
            born(game, pos, entity, spec, &indexes_of(game, spec), origin)
        }
    };
    let added = live.machine.upgrades().add_upgrades(upgrade, available);
    if added > 0 {
        // O NBT já guarda o que entrou: salvar o mundo antes do próximo tick da máquina não perde o upgrade nem o item.
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
            basis::write_upgrades(entity, &live.machine.upgrades().saved());
        }
        live.upgrades_version = Some(live.machine.upgrades_version());
        game.world.modified.insert((at.0 >> 4, at.2 >> 4));
    }
    if was_alive {
        LIVE_MACHINES.with_borrow_mut(|machines| machines.insert(key, live));
    }
    added
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

/// `TrackingEnergyHandler.insert` → `LastEnergyTracker.received`: o buffer da máquina em `pos` passou a guardar `energy` (`native_energy_store`, o
/// fim de todo `insert` e `move`). O que subiu entrou pela capability e soma no `lastEnergyReceived` do tick; o que desceu saiu (`extract`) e
/// o Java não conta. O objeto vivo que está no meio do próprio tick não recebe energia (o Horizon o tira do mundo), então o empréstimo da
/// tabela só falha se um tick da própria máquina estiver chamando isto, e então não há o que contar.
pub(in crate::game) fn energy_stored(game: &Game<'_>, pos: [i32; 3], energy: i32) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    if machine_of(&entity.id).is_none() {
        return;
    }
    let before = basis::stored_energy(entity);
    let Some(received) = energy.checked_sub(before).filter(|received| *received > 0) else {
        return;
    };
    let key = (game.native_dimension(), pos);
    LIVE_MACHINES.with(|machines| {
        if let Ok(machines) = machines.try_borrow()
            && let Some(live) = machines.get(&key)
        {
            live.machine.energy_received(i64::from(received), game.world.game_time as u64);
        }
    });
}

/// A marca que o tick de uma máquina deixa em `ModBlockEntity.data` (que o Horizon não salva, e que o menu da máquina não usa: ela declara
/// zero data slots): o tile é o mesmo objeto de antes. Um tile novo, ou lido do disco, nasce sem ela.
const TILE_TICKED: i32 = 1;

/// O valor vivo dos dados do menu da máquina em `pos`, o que o `MekanismContainer` lê no `broadcastChanges` (`ServerPlayer.tick`,
/// `ServerPlayer.java:606`, antes dos block entities) e no `sendAllDataToRemote` da abertura. A energia e o tanque vêm do que o tile
/// guarda agora, porque chegam de fora do tick da máquina (um gerador, o botão de esvaziar); o resto é o objeto vivo, com o erro que ele
/// ainda não recalculou. O `redstone` que um aviso deixou depois do tick da máquina já vale aqui (`updatePower` troca o campo na hora).
/// O objeto vivo só vale para o tile que ele andou (`entity.data` guarda a marca do tick, e um tile novo ou lido do disco nasce sem ela):
/// até o primeiro tick do tile o Java já tem o tile carregado do NBT (`loadAdditional`), então o menu lê um objeto feito do NBT, como o
/// `born` do primeiro tick o fará (`menu_source`). Devolve se havia o que ler (um tile que já andou e cujo objeto saiu da tabela não tem).
fn window_source(game: &mut Game<'_>, pos: [i32; 3], act: &mut dyn FnMut(&dyn TrackedSource)) -> bool {
    let at = (pos[0], pos[1], pos[2]);
    let ticked = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if machine_of(&entity.id).is_some() => !entity.data.is_empty(),
        _ => return false,
    };
    // O aviso que o tile novo ainda não leu fica na fila para o primeiro tick dele, que o pega.
    let power = if ticked { basis::take_power(game, pos) } else { basis::peek_power(game, pos) };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
        return false;
    };
    let Some(spec) = machine_of(&entity.id) else {
        return false;
    };
    let stored = i64::from(basis::stored_energy(entity));
    let tank = spec.slots.infusion.map(|_| {
        let tank = read_tank(entity);
        StackValue {
            resource: tank.resource.as_ref().map(|held| held.id.to_string()).unwrap_or_default(),
            amount: tank.amount,
        }
    });
    let key = (game.native_dimension(), pos);
    let mut loaded = None;
    LIVE_MACHINES.with_borrow_mut(|machines| {
        let live = machines.get_mut(&key).map(|live| &mut live.machine);
        let Some(machine) = menu_source::object(ticked, live, &mut loaded, || {
            // `placed_here` sem consumir: o primeiro tick ainda a vai pedir.
            let origin = Origin { placed: placed_here(game, pos, false), recalculated_before: false };
            born(game, pos, entity, spec, &indexes_of(game, spec), origin).machine
        }) else {
            return false;
        };
        if let Some(powered) = power {
            machine.tile_mut().update_power(powered);
        }
        act(&MachineSync { machine, stored, tank });
        true
    })
}

/// `TileComponentUpgrade.setUpgrades` → `sendUpdatePacket`: o cliente só quer saber quantos upgrades de silêncio há.
fn sync_muffling(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], live: &mut Live) {
    let count = live.machine.upgrades().get_upgrades(rules::MUFFLING_UPGRADE);
    if count != live.muffling_sent {
        live.muffling_sent = count;
        game.mb_muffling_changed(dimension, pos, count);
    }
}

/// `TileEntityMekanism.tickClient` → `updateSound` de cada máquina que o cliente vê: o som em laço de quem está ligado. O cliente
/// não tem o objeto vivo: vê o bloco (`lit`), os tipos de upgrade que o servidor mandou e quantos de silêncio o tile tem.
pub(in crate::game) fn client_tick(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    let dimension = game.native_dimension();
    let (types, tags) = (upgrade::current_types(), upgrade::current_tags());
    let muffling_max = types.get(rules::MUFFLING_UPGRADE).map(|kind| kind.max);
    let supports = upgrade_data(game, &types, &tags).supports(rules::MUFFLING_UPGRADE);
    let christmas = rules::CHRISTMAS_SOUNDS;
    let today = crate::game::machinery_tile_sound::today();
    let tiles: Vec<crate::game::machinery_tile_sound::SoundTile> = game
        .world
        .block_entities
        .iter()
        .filter_map(|(&(x, y, z), entity)| {
            let BlockEntity::Mod(entity) = entity else {
                return None;
            };
            let machine = machine_of(&entity.id)?;
            let pos = [x, y, z];
            let muffling = muffling_max.filter(|_| supports).map(|max| (crate::game::machinery_tile_sound::muffling_count(dimension, pos), max));
            // `HolidayManager.filterSound`: `MekanismConfig.local.holidays` vale `true` por padrão.
            let sound = machinery_basis_core::sound::holiday_sound(machine.sound, true, today, rules::CHRISTMAS, &christmas);
            Some(crate::game::machinery_tile_sound::SoundTile {
                key: (dimension, pos),
                lit: game.native_block_property(pos, "lit").as_deref() == Some("true"),
                fully_muffled: machinery_basis_core::sound::fully_muffled(muffling),
                volume_factor: machinery_basis_core::sound::volume_factor(muffling),
                sound: sound.to_owned(),
            })
        })
        .collect();
    game.mb_tile_sounds(dimension, &tiles);
}

/// `TileEntityRecipeMachine.preRemoveSideEffects`: o jogador quebra a máquina e a experiência guardada cai, se tiver o upgrade.
pub(in crate::game) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], canceled: bool) {
    if canceled || machine_of(&game.native_block(pos)).is_none() {
        return;
    }
    basis::name_destroying(game, pos, &is_machine_id);
    let key = (game.native_dimension(), pos);
    let drops = LIVE_MACHINES.with_borrow(|machines| machines.get(&key).map(|live| live.machine.recipes_to_award_on_removal().drops));
    if let Some(drops) = drops {
        pop_experience(game, center_of(pos), &drops);
    }
}

/// `TileEntityRecipeMachine.preRemoveSideEffects` numa explosão: os blocos que ela vai quebrar levam a experiência guardada (se tiverem o
/// upgrade), no centro de cada um. O tile ainda existe, e o nome do item solto sai dele (`loot_drops`).
pub(in crate::game) fn explosion_detonate(game: &mut Game<'_>, blocks: &[[i32; 3]]) {
    let dimension = game.native_dimension();
    for &pos in blocks {
        if machine_of(&game.native_block(pos)).is_none() {
            continue;
        }
        let drops = LIVE_MACHINES.with_borrow(|machines| machines.get(&(dimension, pos)).map(|live| live.machine.recipes_to_award_on_removal().drops));
        if let Some(drops) = drops {
            pop_experience(game, center_of(pos), &drops);
        }
    }
}

/// O slot de saída da máquina aberta antes do clique do jogador, e onde o jogador estava: o `onTakeCheck` do Java roda no clique.
struct PendingTake {
    pos: [i32; 3],
    before: ItemStack,
    player: [f64; 3],
    /// A conexão do jogador que clicou: as receitas desbloqueiam no livro dele.
    conn: u32,
}

thread_local! {
    static OUTPUT_BEFORE: RefCell<Option<PendingTake>> = const { RefCell::new(None) };
}

/// Antes de um clique do jogador no menu de uma máquina (`lan_machine_click`, que o servidor roda para o anfitrião e para os
/// convidados): confere o clique anterior e guarda a saída da máquina aberta. O Horizon não avisa quando um jogador tira de um slot
/// de resultado de contêiner de mod, e o único ponto depois do clique a que se chega sem reescrever o clique é o próximo tick da
/// máquina, que lê o slot antes de tudo.
pub(in crate::game) fn result_take_before(game: &mut Game<'_>, host: &crate::game::lan::LanHost, conn: u32) {
    if let Some(pending) = OUTPUT_BEFORE.with_borrow_mut(Option::take) {
        let at = pending.pos;
        let now = match game.world.block_entities.get(&(at[0], at[1], at[2])) {
            Some(BlockEntity::Mod(entity)) => machine_of(&entity.id).map(|machine| entity.items.get(machine.slots.output).cloned().flatten()),
            _ => None,
        };
        if let Some(now) = now {
            settle_take(game, pending, now, None);
        }
    }
    slot_rules::clear(RULES_OWNER);
    let Some(guest) = host.hub.guests.iter().find(|guest| guest.conn == conn) else {
        return;
    };
    let Some(horizon_client::lan::menu::Slots::Block((x, y, z))) = guest.menu.as_ref().map(|menu| menu.slots) else {
        return;
    };
    publish_slot_rules(game, [x, y, z]);
    let pending = match game.world.block_entities.get(&(x, y, z)) {
        Some(BlockEntity::Mod(entity)) => machine_of(&entity.id).and_then(|machine| {
            entity
                .items
                .get(machine.slots.output)
                .cloned()
                .flatten()
                .map(|before| PendingTake { pos: [x, y, z], before, player: guest.position, conn: guest.conn })
        }),
        _ => None,
    };
    OUTPUT_BEFORE.with_borrow_mut(|slot| *slot = pending);
}

/// O dono das regras de slot que este mod deixa na `slot_rules` do núcleo.
const RULES_OWNER: &str = "mekanism";

/// `InventoryContainerSlot.mayPlace` do clique que vai rodar: o `isValidForInsertion(item, MANUAL)` do slot da máquina aberta, com o
/// que o tanque e a entrada guardam agora. O hook do Horizon (`menu_slot_rules`) roda dentro do menu, sem o jogo, e consulta isto.
/// Vale o mesmo para o clique esquerdo e o direito, a tecla numérica, o arrasto e o shift-click: o menu pergunta o mesmo.
fn publish_slot_rules(game: &Game<'_>, pos: [i32; 3]) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let Some(machine) = machine_of(&entity.id) else {
        return;
    };
    let indexes = indexes_of(game, machine);
    let held = Held {
        chemical: machine.slots.infusion.and_then(|_| read_tank(entity).resource),
        input: entity.items.get(machine.slots.input).and_then(Option::as_ref).map(|stack| resource_of(game, stack)),
    };
    let rule_slots = machine.slots.rule_slots();
    let (upgrade_input, upgrade_output) = (machine.slots.upgrade, machine.slots.upgrade_output);
    let upgrades = window_upgrade_data();
    slot_rules::set(
        RULES_OWNER,
        machine.block,
        Rc::new(move |slot, item, stack| {
            // Os dois slots virtuais da janela de upgrades (`UpgradeInventorySlot`): a entrada aceita o upgrade que a máquina suporta
            // (`isValidForInsertion(.., MANUAL)`: o item tem o componente do tipo e o tipo está na tag), a saída só deixa o jogador tirar.
            if slot == upgrade_output {
                return false;
            }
            if slot == upgrade_input {
                return stack.downcast_ref::<ItemStack>().is_some_and(|stack| {
                    item == rules::UPGRADE_ITEM && upgrade_type_of(stack).is_some_and(|kind| upgrades.supports(&kind))
                });
            }
            rules::slot_accepts(&indexes, &rule_slots, slot, &ItemResource::new(item, UNKNOWN_STACK), &held)
        }),
    );
}

/// O tipo de upgrade que o item carrega (`MekanismDataComponents.UPGRADE_TYPE`), lido da pilha que o hook recebeu. O hook roda dentro do
/// menu, sem o jogo, então lê o componente guardado na própria pilha, como `native_component` faz antes do padrão do item.
fn upgrade_type_of(stack: &ItemStack) -> Option<String> {
    match stack.data()?.other(rules::UPGRADE_TYPE_COMPONENT)? {
        Tag::String(kind) => Some(kind.to_string_lossy()),
        _ => None,
    }
}

/// No começo do tick da máquina, antes de o espelho ver o slot: o clique tirou o resultado?
fn result_take_after(game: &mut Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, spec: &Machine) {
    let pending = OUTPUT_BEFORE.with_borrow_mut(|slot| match slot {
        Some(pending) if pending.pos == pos => slot.take(),
        _ => None,
    });
    if let Some(pending) = pending {
        let now = entity.items.get(spec.slots.output).cloned().flatten();
        settle_take(game, pending, now, Some(entity));
    }
}

/// Se a saída diminuiu, o jogador a tirou (`InventoryContainerResultSlot.onTake`): a experiência sai dele.
fn settle_take(game: &mut Game<'_>, pending: PendingTake, now: Option<ItemStack>, ticking: Option<&ModBlockEntity>) {
    let taken = match &now {
        None => true,
        Some(now) => now.count < pending.before.count || now.item != pending.before.item,
    };
    if !taken {
        return;
    }
    let key = (game.native_dimension(), pending.pos);
    let awarded = LIVE_MACHINES.with_borrow(|machines| {
        machines.get(&key).map(|live| {
            // O resultado já saiu do slot (`outputDecreased`) quando o `onTakeCheck` roda: o espelho ainda não viu.
            slot_from_entity(game, live.machine.output(), now.as_ref());
            live.machine.award_used_recipes()
        })
    });
    if let Some(awarded) = awarded {
        pop_experience(game, pending.player, &awarded.drops);
        let inputs = ticking.map_or_else(|| input_stacks(game, pending.pos), input_stacks_of);
        award_recipes_to(game, pending.conn, awarded.recipes, inputs);
    }
}

/// Os slots de entrada da máquina em `pos` (`getRecipeContainers(cacheIndex).inputs()`, só os `IInventorySlot`), para o gatilho de receita.
fn input_stacks(game: &Game<'_>, pos: [i32; 3]) -> Vec<Option<ItemStack>> {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => machine_of(&entity.id)
            .map(|machine| vec![entity.items.get(machine.slots.input).cloned().flatten()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `ServerPlayer.awardRecipes(recipes)` e depois `triggerRecipeCrafted(recipe, items)` de cada uma (`awardUsedRecipesAndPopExperience`): as
/// receitas entram no livro do jogador que age, com o aviso de desbloqueio, e o critério `recipe_crafted` anda. Com `items` como o Java
/// os junta: os slots de entrada, vazios inclusive.
fn award_recipes(game: &mut Game<'_>, recipes: &[String], items: &[Option<ItemStack>]) {
    if recipes.is_empty() {
        return;
    }
    let index = &world_stations!(game).recipe_index;
    let (_, unlocked) = game.level.recipe_book.add_recipes(index, recipes);
    game.recipe_unlocked(&unlocked);
    for recipe in recipes {
        game.trigger_recipe_crafted(recipe, items);
    }
}

thread_local! {
    /// As receitas de um jogador que ainda não deu para premiar (o jogo não tinha o servidor da conexão à mão), para o próximo tick.
    static DEFERRED_AWARDS: RefCell<Vec<(u32, Vec<String>, Vec<Option<ItemStack>>)>> = const { RefCell::new(Vec::new()) };
}

/// Premia as receitas ao jogador da conexão `conn`: se é o que está agindo, na hora; senão com o estado dele no lugar do anfitrião
/// (`as_guest`), ou no próximo tick se o servidor não está à mão.
fn award_recipes_to(game: &mut Game<'_>, conn: u32, recipes: Vec<String>, items: Vec<Option<ItemStack>>) {
    if game.lan_acting == Some(conn) {
        award_recipes(game, &recipes, &items);
        return;
    }
    let Some(mut host) = game.lan.take() else {
        DEFERRED_AWARDS.with_borrow_mut(|list| list.push((conn, recipes, items)));
        return;
    };
    game.as_guest(&mut host, conn, |game| award_recipes(game, &recipes, &items));
    game.lan = Some(host);
}

/// O que ficou para depois, no começo do tick de uma máquina.
fn flush_deferred_awards(game: &mut Game<'_>) {
    let list = DEFERRED_AWARDS.with_borrow_mut(std::mem::take);
    for (conn, recipes, items) in list {
        award_recipes_to(game, conn, recipes, items);
    }
}

/// O nome do tile é o da `machinery_basis` (`basis::name_*`): o componente inteiro, só para os blocos das máquinas daqui.
fn is_machine_id(id: &str) -> bool {
    machine_of(id).is_some()
}

fn screen_title(game: &Game<'_>, container_description: &str) -> horizon_client::text::Text {
    basis::screen_title(game, container_description)
}

pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    basis::name_placed(game, stack, pos, &is_machine_id);
    let block = game.native_block(pos);
    if is_machine_id(&block) {
        note_placed(game, pos);
        // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`: só o bloco posto com um item lê o sinal ao nascer.
        basis::on_added(game, pos);
        placed_capacity(game, pos, &block);
    }
}

/// `TileEntityMekanism.applyImplicitComponents` (de `setPlacedBy`) -> `recalculateUpgrades` -> `MachineEnergyContainer.updateMaxEnergy`: a
/// capacidade do item (1.000, ou o que os upgrades dão) vale desde a colocação, e não só desde o primeiro tick da máquina. Um gerador ou um
/// cabo que encha a máquina nesse tick lê a capacidade nova, e não os quatro ticks de consumo do construtor.
fn placed_capacity(game: &Game<'_>, pos: [i32; 3], block: &str) {
    let Some(spec) = machine_of(block) else {
        return;
    };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let origin = Origin { placed: true, recalculated_before: false };
    let live = born(game, pos, entity, spec, &indexes_of(game, spec), origin);
    let capacity = basis::energy_as_int(live.machine.energy().capacity(), "a capacidade de energia");
    basis::set_energy_capacity(game.native_dimension(), pos, &entity.id, capacity);
    // O tile novo guarda `currentRedstoneLevel = 0`: um comparador que olhe agora não lê o nível de uma máquina que esteve ali antes.
    COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert((game.native_dimension(), pos), live.machine.tile().current_redstone_level()));
}

pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    basis::name_loot(game, context, drops, &is_machine_id);
}

pub(in crate::game) fn container_opened(game: &mut Game<'_>, player: u128, pos: Option<[i32; 3]>) {
    basis::name_container_opened(game, player, pos, &is_machine_id);
    // `MekanismTileContainer.openInventory`: o jogador entra no `playersUsing` e recebe os dados do menu.
    if let Some(pos) = pos {
        let block = game.native_block(pos);
        if is_machine(&block) {
            game.mb_container_open(player, pos, &block);
        }
    }
}

/// `MekanismDataComponents.UPGRADE_TYPE`: as linhas que o item de upgrade ganha no tooltip, depois do nome.
pub(in crate::game) fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(horizon_client::mod_api::Text, u32)>) {
    use horizon_client::mod_api::Text;
    if &*stack.item != rules::UPGRADE_ITEM {
        return;
    }
    let Ok(Some(Tag::String(kind))) = game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) else {
        return;
    };
    let id = kind.to_string_lossy();
    let types = upgrade::current_types();
    let Some(upgrade_type) = types.get(&id) else {
        return;
    };
    // `u32::MAX` mantém a cor do próprio texto; a cor de `rgba` vem com o alfa embaixo.
    let rgba = |rgb: u32| (rgb << 8) | 0xFF;
    for line in rules::upgrade_tooltip(&id, upgrade_type) {
        let args: Vec<Text> = line
            .args
            .iter()
            .map(|arg| match arg {
                rules::TooltipArg::Translate { key, color } => {
                    let mut text = Text::translate(key, &[]);
                    paint(&mut text, *color);
                    text
                }
                rules::TooltipArg::Number { value, color } => Text::colored(value.to_string(), *color),
            })
            .collect();
        lines.push((Text::translate(&line.translate, &args), line.color.map_or(u32::MAX, rgba)));
    }
}

/// A cor `rgb` em todos os pedaços do texto.
fn paint(text: &mut horizon_client::mod_api::Text, rgb: u32) {
    for span in &mut text.spans {
        span.style.color = Some(rgb);
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
        basis::insert_into_cell(self.game, self.cell(slot), item, amount)
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        basis::remove_from_cell(self.cell(slot), amount);
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

pub(in crate::game) fn menu_button(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) {
    if id == XP_BUTTON {
        collect_xp(game, pos, entity);
        return;
    }
    if !is_infuser(&entity.id) || id != DUMP_BUTTON {
        return;
    }
    // `dump()`: `dumpOrClearContents` no tanque de infusão, que fora a radiação (que não existe aqui) só o esvazia. O objeto
    // vivo o relê no tick seguinte, como um `setContents` de fora.
    if !read_tank(entity).is_empty() {
        write_tank(entity, &TankContents::default());
    }
}

/// `PacketGuiInteract.COLLECT_XP`: `SupportsXpContainment.popExperience(player)`, que só age com `hasXpContainment()`. As bolas nascem no
/// jogador que clicou (`awardUsedRecipesAndPopExperience` usa `player.position()`) e as receitas guardadas zeram.
fn collect_xp(game: &mut Game<'_>, pos: [i32; 3], entity: &ModBlockEntity) {
    if machine_of(&entity.id).is_none() {
        return;
    }
    let Some(player) = game.native_acting_player() else {
        return;
    };
    let key = (game.native_dimension(), pos);
    let awarded = LIVE_MACHINES.with_borrow(|machines| machines.get(&key).map(|live| live.machine.collect_xp()));
    if let Some(awarded) = awarded {
        pop_experience(game, player.position, &awarded.drops);
        // O clique chega com o jogador que o fez no lugar do anfitrião: o livro é o dele.
        let inputs = input_stacks_of(entity);
        award_recipes(game, &awarded.recipes, &inputs);
    }
}

/// O slot de entrada do tile que o menu entregou (o tile está fora do mundo durante o clique).
fn input_stacks_of(entity: &ModBlockEntity) -> Vec<Option<ItemStack>> {
    machine_of(&entity.id).map(|machine| vec![entity.items.get(machine.slots.input).cloned().flatten()]).unwrap_or_default()
}

/// `GuiDumpButton` é um `MekanismImageButton` com a mesma ação nos dois botões do mouse: o clique direito também esvazia o tanque.
pub(in crate::game) fn screen_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if is_infuser(block) && inside(DUMP_RECT, at) {
        *button = Some(DUMP_BUTTON);
        return;
    }
    // `GuiXpContainmentTab.onClick`: só o botão esquerdo, com a aba visível e fora de uma janela por cima dela.
    if let Some(machine) = machine_of(block) {
        let read = rules::ScreenData::read(machine.screen, &game.mb_mirror(block));
        if rules::xp_tab().click(read.xp_containment, at, right, game.mb_window_covers(at)) {
            *button = Some(XP_BUTTON);
        }
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
    let mirror = game.mb_mirror(block);
    let read = rules::ScreenData::read(machine.screen, &mirror);
    let translate = |key: &str| lang::get(key);
    // `GuiXpContainmentTab`: `setTooltip(MekanismLang.XP_CONTAINMENT_TAB)`, que `Tooltip.create` quebra em 170 pixels. A janela de upgrades
    // por cima do botão o esconde (`isMouseOverCheckWindows`).
    let xp_tab = rules::xp_tab();
    if xp_tab.hovered(read.xp_containment, at, game.mb_window_covers(at)) {
        let lines: Vec<(String, Option<u32>)> = machinery_basis_core::upgrade_window::wrap_lines(&translate(rules::XP_TAB_TOOLTIP), TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece))
            .into_iter()
            .map(|line| (line, None))
            .collect();
        let [bx, by, bw, bh] = xp_tab.tab.button();
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + bx, cursor.1 - at[1] + by, bw, bh]);
        return true;
    }
    // `GuiEnergyTab.updateTooltip`: `Tooltip.create` de uma lista de linhas, que quebra cada uma em 170 pixels.
    if rules::ENERGY_TAB.is_mouse_over(at, false) {
        // `tile.getActive()` no cliente é o estado `lit` do bloco.
        let active = game.mb_open_screen_pos().is_some_and(|pos| game.native_block_property(pos, "lit").as_deref() == Some("true"));
        // Antes do primeiro valor do servidor o contêiner do cliente tem o consumo de base.
        let per_tick = if read.energy_per_tick == 0 { machine.energy_per_tick as u32 } else { read.energy_per_tick };
        let capacity = u64::from(screen_capacity(machine, &read));
        let lines: Vec<(String, Option<u32>)> =
            rules::energy_tab_tooltip(active, per_tick, u64::from(read.energy), capacity, &translate)
                .iter()
                .flat_map(|line| machinery_basis_core::upgrade_window::wrap_lines(line, TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)))
                .map(|line| (line, None))
                .collect();
        let [x, y, w, h] = rules::ENERGY_TAB.rect;
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h]);
        return true;
    }
    let (widget, lines): ([f32; 4], Vec<(String, Option<u32>)>) = if gui::contains(gui::WARNING_TAB_LEFT, at) {
        let view = machine.screen.view(&mirror);
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
        let amount = read.chemical.amount;
        let name = (amount > 0).then(|| translate(&rules::chemical_lang_key(&read.chemical.resource)));
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

fn draw_infuser(game: &Game<'_>, machine: &Machine, mirror: &Mirror, batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let read = rules::ScreenData::read(machine.screen, mirror);
    let view = machine.screen.view(mirror);
    let warning = |element| view.element(element).then_some(&WARNING_SPRITES);

    let title = screen_title(game, "container.mekanism.metallurgic_infuser");
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
    draw_xp_tab(game, batch, origin, &read);
    draw_energy_tab(game, batch, origin);
    if view.tab() {
        basis::warning_tab(game, batch, WARNING_TAB_SPRITE, origin);
    }
}

/// `GuiEnergyTab`: o sprite `tab/energy_info` em (-26, 137), sem clique. O tooltip está em `widget_tooltip`.
fn draw_energy_tab(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y, w, h] = rules::ENERGY_TAB.rect;
    sprite_rect(game, batch, rules::ENERGY_TAB_SPRITE, [origin[0] + x, origin[1] + y, w, h]);
}

/// `GuiXpContainmentTab`, depois da aba de upgrades (`GuiMekanismTile.addGenericTabs`): só com o upgrade instalado (`tick`: `visible =
/// hasXpContainment()`), com o fundo do botão em foco quando o mouse está sobre ele e nenhuma janela por cima.
fn draw_xp_tab(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2], read: &rules::ScreenData) {
    use machinery_basis_core::side_tab::{ButtonLook, XpTab};
    if !XpTab::visible(read.xp_containment) {
        return;
    }
    let tab = rules::xp_tab();
    let cursor = game.native_cursor();
    let at = [cursor[0] - origin[0], cursor[1] - origin[1]];
    let hovered = tab.hovered(read.xp_containment, at, game.mb_window_covers(at));
    basis::inset_tab(game, batch, "mekanism", tab.tab, origin, rules::XP_TAB_TINT, rules::XP_TAB_ICON, ButtonLook::of(true, hovered));
}

/// `GuiChemicalBar`: o nível é `capacityAsLong`, e o conteúdo o sprite do químico, animado, em mosaico e multiplicado pelo `tint`.
fn draw_chemical_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    read: &rules::ScreenData,
    rect: [f32; 4],
    warning: Option<&basis::WarningSprites>,
) {
    let level = gui::divide_to_level(read.chemical.amount as f64, rules::TANK_CAPACITY as f64);
    // O cliente resolve o químico pelo registro dele (cor e sprite), como o do original resolve o `Chemical` do pacote.
    let registry = resource::current();
    let chemical = registry.get(&read.chemical.resource);
    basis::vertical_bar(game, batch, &BAR_STYLE, rect, level, warning, &|game, batch, dst| {
        let Some(icon) = rules::chemical_icon(chemical.and_then(|chemical| chemical.icon.as_deref())) else {
            return;
        };
        basis::tank_bar_contents(game, batch, icon, [CHEMICAL_FRAME; 2], dst, chemical.map_or(0, |chemical| chemical.tint));
    });
}

fn draw_electric(
    game: &Game<'_>,
    electric: &Machine,
    mirror: &Mirror,
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let read = rules::ScreenData::read(electric.screen, mirror);
    let view = electric.screen.view(mirror);

    let title = screen_title(game, electric.title);
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
    draw_xp_tab(game, batch, origin, &read);
    draw_energy_tab(game, batch, origin);
    if view.tab() {
        basis::warning_tab(game, batch, WARNING_TAB_SPRITE, origin);
    }
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if let Some(machine) = machine_of(block) {
        let mirror = game.mb_mirror(block);
        if is_infuser(block) {
            draw_infuser(game, machine, &mirror, batch, origin);
        } else {
            draw_electric(game, machine, &mirror, batch, origin);
        }
    }
}
