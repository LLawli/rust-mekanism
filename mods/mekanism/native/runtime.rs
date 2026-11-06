// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::cached_recipe::ItemToItemRecipe;
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::containers::InventorySlot;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::resource::Resource;
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::tile::{
    ChemicalMachineConfig, ElectricMachineConfig, MachineConfig, ProgressMachine, RedstoneControl,
    UpgradeConfig,
};
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::warning::{ErrorMask, WarningTracker};
use serde_json::Value;
use std::rc::Rc;

pub const INFUSER: &str = "mekanism:metallurgic_infuser";
pub const INFUSING: &str = "mekanism:metallurgic_infusing";
pub const CHEMICAL_CONVERSION: &str = "mekanism:chemical_conversion";
pub const INFUSING_FIELDS: RecipeFields = RecipeFields {
    item: "item_input",
    resource: Some("chemical_input"),
    output: "output",
};
pub const ELECTRIC_FIELDS: RecipeFields = RecipeFields {
    item: "input",
    resource: None,
    output: "output",
};
pub const CONVERSION_FIELDS: ConversionFields = ConversionFields {
    item: "input",
    output: "output",
};

pub const ENRICHMENT_CHAMBER: &str = "mekanism:enrichment_chamber";
pub const CRUSHER: &str = "mekanism:crusher";
pub const ENERGIZED_SMELTER: &str = "mekanism:energized_smelter";
pub const ENRICHING: &str = "mekanism:enriching";
pub const CRUSHING: &str = "mekanism:crushing";
pub const SMELTING: &str = "smelting";

/// `CableTier`: o id de cada cabo e quanto ele guarda.
pub const UNIVERSAL_CABLES: [(&str, i32); 4] = [
    ("mekanism:basic_universal_cable", 8_000),
    ("mekanism:advanced_universal_cable", 128_000),
    ("mekanism:elite_universal_cable", 1_024_000),
    ("mekanism:ultimate_universal_cable", 8_192_000),
];

/// `BASE_TICKS_REQUIRED`, `MAX_INFUSE` e os padrões de `UsageConfig`.
pub const TICKS_REQUIRED: u32 = 200;
pub const TANK_CAPACITY: u64 = 1_000;
pub const INFUSER_USAGE: i32 = 5;
pub const ENRICHMENT_USAGE: i32 = 5;
pub const CRUSHER_USAGE: i32 = 8;
pub const SMELTER_USAGE: i32 = 5;

/// `blockDeactivationDelay`, o padrão de `GeneralConfig`: quantos ticks o bloco espera para apagar.
pub const BLOCK_DEACTIVATION_DELAY: i32 = 60;
/// `UpgradeUtils`: o multiplicador dos upgrades (`upgradesMultiplier`). O máximo de cada tipo vem dos dados.
pub const UPGRADE_MULTIPLIER: f64 = 10.0;

/// `MekanismItems.UPGRADE` e o componente `MekanismDataComponents.UPGRADE_TYPE`, que guarda o tipo no item.
pub const UPGRADE_ITEM: &str = "mekanism:upgrade";
pub const UPGRADE_TYPE_COMPONENT: &str = "mekanism:upgrade_type";
/// `UpgradeIds.SPEED` e `UpgradeIds.ENERGY`.
pub const SPEED_UPGRADE: &str = "mekanism:speed";
pub const ENERGY_UPGRADE: &str = "mekanism:energy";
/// `MekanismTags.Upgrades.DEFAULT_MACHINE_UPGRADES`: o que `AttributeUpgradeSupport.DEFAULT_MACHINE_UPGRADES` aceita, e que as
/// quatro máquinas portadas usam.
pub const DEFAULT_MACHINE_UPGRADES: &str = "mekanism:upgrade_support/default_machine";

/// Os dados de upgrade que as quatro máquinas portadas usam: os tipos, o que a tag `default_machine` aceita, e o item que os
/// carrega, com o limite de pilha dele.
pub fn upgrade_data(
    types: &UpgradeTypes,
    tags: &UpgradeTags,
    item_max_stack_size: u32,
) -> Rc<UpgradeData> {
    Rc::new(UpgradeData::new(
        UPGRADE_ITEM,
        item_max_stack_size,
        types,
        tags,
        DEFAULT_MACHINE_UPGRADES,
    ))
}

/// `TileEntityEnrichmentChamber.TRACKED_ERROR_TYPES`, na ordem da tela.
pub const ELECTRIC_TRACKED_ERRORS: [RecipeError; 4] = [
    RecipeError::NOT_ENOUGH_ENERGY,
    RecipeError::NOT_ENOUGH_INPUT,
    RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
];

/// `TileEntityMetallurgicInfuser.TRACKED_ERROR_TYPES`, na ordem da tela.
pub const INFUSER_TRACKED_ERRORS: [RecipeError; 5] = [
    RecipeError::NOT_ENOUGH_ENERGY,
    RecipeError::NOT_ENOUGH_INPUT,
    RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
];

/// `WarningTracker.WarningType`. A ordem das variantes é a do `enum` do Java: ela decide a ordem dos avisos na tela, por causa do
/// `EnumMap`, e o corte de `getWarnings` depende dela. Não reordene. `expectedWarnings` (a capacidade inicial da lista) não muda
/// o comportamento e não entra.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarningType {
    InputDoesntProduceOutput,
    NoMatchingRecipe,
    NoSpaceInOutput,
    NoSpaceInOutputOverflow,
    NotEnoughEnergy,
    NotEnoughEnergyReducedRate,
    InvalidOredictionificatorFilter,
    FilterHasBlacklistedElement,
    RedstoneSignalAbsent,
    RedstoneSignalPresent,
    RedstonePulseRequired,
}

impl WarningType {
    /// O nome do `enum` em minúsculas, como os rastros o gravam.
    pub const fn name(self) -> &'static str {
        match self {
            Self::InputDoesntProduceOutput => "input_doesnt_produce_output",
            Self::NoMatchingRecipe => "no_matching_recipe",
            Self::NoSpaceInOutput => "no_space_in_output",
            Self::NoSpaceInOutputOverflow => "no_space_in_output_overflow",
            Self::NotEnoughEnergy => "not_enough_energy",
            Self::NotEnoughEnergyReducedRate => "not_enough_energy_reduced_rate",
            Self::InvalidOredictionificatorFilter => "invalid_oredictionificator_filter",
            Self::FilterHasBlacklistedElement => "filter_has_blacklisted_element",
            Self::RedstoneSignalAbsent => "redstone_signal_absent",
            Self::RedstoneSignalPresent => "redstone_signal_present",
            Self::RedstonePulseRequired => "redstone_pulse_required",
        }
    }

    /// `langEntry`: o `MekanismLang.ISSUE_*` do tipo, que `getWarnings` traduz.
    pub const fn lang_key(self) -> &'static str {
        match self {
            Self::InputDoesntProduceOutput => "gui.mekanism.issues.input_doesnt_produce_output",
            Self::NoMatchingRecipe => "gui.mekanism.issues.no_recipe",
            Self::NoSpaceInOutput => "gui.mekanism.issues.no_space",
            Self::NoSpaceInOutputOverflow => "gui.mekanism.issues.no_space.overflow",
            Self::NotEnoughEnergy => "gui.mekanism.issues.no_energy",
            Self::NotEnoughEnergyReducedRate => "gui.mekanism.issues.no_energy.reduced_rate",
            Self::InvalidOredictionificatorFilter => {
                "gui.mekanism.issues.invalid_oredictionificator_filter"
            }
            Self::FilterHasBlacklistedElement => {
                "gui.mekanism.issues.filter_has_blacklisted_element"
            }
            Self::RedstoneSignalAbsent => "gui.mekanism.issues.redstone_signal_absent",
            Self::RedstoneSignalPresent => "gui.mekanism.issues.redstone_signal_present",
            Self::RedstonePulseRequired => "gui.mekanism.issues.redstone_pulse_required",
        }
    }
}

/// O tipo em que `WarningTracker.getWarnings` para: depois da mensagem de `NOT_ENOUGH_ENERGY` não lista mais nada. No Java o
/// comentário fala só da taxa reduzida, mas o corte também esconde os avisos de redstone, que vêm depois dela no `enum`.
pub const WARNINGS_STOP_AFTER: WarningType = WarningType::NotEnoughEnergy;

/// Os pares (tipo, erro) que a tela de uma `TileEntityElectricMachine` registra: `GuiElectricMachine` (a barra de energia e a
/// seta de progresso) e os slots de `TileEntityElectricMachine.getInitialInventory` (entrada e saída).
pub const ELECTRIC_WARNINGS: [(WarningType, RecipeError); 4] = [
    (WarningType::NotEnoughEnergy, RecipeError::NOT_ENOUGH_ENERGY),
    (
        WarningType::InputDoesntProduceOutput,
        RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
    ),
    (WarningType::NoMatchingRecipe, RecipeError::NOT_ENOUGH_INPUT),
    (
        WarningType::NoSpaceInOutput,
        RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    ),
];

/// O mesmo do Metallurgic Infuser: `GuiMetallurgicInfuser` e `TileEntityMetallurgicInfuser.getInitialInventory`. A barra do
/// químico e o slot de entrada dão os dois `NO_MATCHING_RECIPE`, e a falta de saída vem do slot de saída.
pub const INFUSER_WARNINGS: [(WarningType, RecipeError); 5] = [
    (WarningType::NotEnoughEnergy, RecipeError::NOT_ENOUGH_ENERGY),
    (
        WarningType::InputDoesntProduceOutput,
        RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
    ),
    (
        WarningType::NoMatchingRecipe,
        RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    ),
    (WarningType::NoMatchingRecipe, RecipeError::NOT_ENOUGH_INPUT),
    (
        WarningType::NoSpaceInOutput,
        RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    ),
];

/// O que as verificações de aviso leem de uma máquina: no servidor, o objeto vivo; no cliente, o que os data slots trouxeram.
pub trait WarningSource {
    /// `getWarningCheck(error)`.
    fn has_error(&self, error: RecipeError) -> bool;
    /// `getControlType`.
    fn control_type(&self) -> RedstoneControl;
    /// `isRedstoneActivated`.
    fn is_redstone_activated(&self) -> bool;
}

impl WarningSource for ProgressMachine {
    fn has_error(&self, error: RecipeError) -> bool {
        ProgressMachine::has_error(self, error)
    }

    fn control_type(&self) -> RedstoneControl {
        self.tile().control_type()
    }

    fn is_redstone_activated(&self) -> bool {
        self.tile().is_redstone_activated()
    }
}

/// O que o cliente tem da máquina: a máscara de erros rastreados de um data slot, a lista rastreada que o mod conhece (a mesma da
/// máquina) e o modo de redstone com o estado dele.
#[derive(Clone, Copy, Debug)]
pub struct SyncedMachine {
    pub tracked: &'static [RecipeError],
    pub errors: ErrorMask,
    pub control_type: RedstoneControl,
    pub redstone_activated: bool,
}

impl SyncedMachine {
    /// O que o servidor manda de uma máquina: a máscara e o modo de redstone. Os mesmos valores que o cliente lê dos data slots.
    pub fn of(machine: &ProgressMachine, tracked: &'static [RecipeError]) -> Self {
        Self {
            tracked,
            errors: machine.tracked_errors_mask(),
            control_type: machine.tile().control_type(),
            redstone_activated: machine.tile().is_redstone_activated(),
        }
    }
}

impl WarningSource for SyncedMachine {
    fn has_error(&self, error: RecipeError) -> bool {
        self.errors.has(self.tracked, error)
    }

    fn control_type(&self) -> RedstoneControl {
        self.control_type
    }

    fn is_redstone_activated(&self) -> bool {
        self.redstone_activated
    }
}

/// O `WarningTracker` de uma tela de máquina: os pares da máquina, e os três de redstone de `GuiMekanismTile.addGenericTabs`, que
/// toda máquina com controle de redstone registra.
pub fn machine_warnings<'a>(
    source: &'a dyn WarningSource,
    pairs: &[(WarningType, RecipeError)],
) -> WarningTracker<'a, WarningType> {
    let mut tracker = WarningTracker::new(Some(WARNINGS_STOP_AFTER));
    for &(kind, error) in pairs {
        tracker.track(kind, move || source.has_error(error));
    }
    for (kind, control) in [
        (WarningType::RedstoneSignalAbsent, RedstoneControl::High),
        (WarningType::RedstoneSignalPresent, RedstoneControl::Low),
        (WarningType::RedstonePulseRequired, RedstoneControl::Pulse),
    ] {
        tracker.track(kind, move || {
            source.control_type() == control && !source.is_redstone_activated()
        });
    }
    tracker
}

/// O que as máquinas de progresso do Mekanism partilham: os valores de `UsageConfig`, `GeneralConfig` e `UpgradeUtils`.
fn machine_config(
    energy_per_tick: i32,
    tracked_errors: &[RecipeError],
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> MachineConfig {
    MachineConfig {
        energy_per_tick,
        base_ticks_required: TICKS_REQUIRED as i32,
        deactivation_delay: BLOCK_DEACTIVATION_DELAY,
        upgrades: UpgradeConfig {
            max_upgrade_multiplier: UPGRADE_MULTIPLIER,
            speed: Rc::from(SPEED_UPGRADE),
            energy: Rc::from(ENERGY_UPGRADE),
            data: upgrades,
        },
        tracked_errors: tracked_errors.to_vec(),
        check_offset,
    }
}

/// Uma máquina elétrica como a lib a recebe: valores do mod, receitas de quem as leu do jogo. O teste dos rastros usa esta
/// mesma configuração.
pub fn electric_config(
    energy_per_tick: i32,
    recipes: Vec<Rc<ItemToItemRecipe>>,
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> ElectricMachineConfig {
    ElectricMachineConfig {
        machine: machine_config(
            energy_per_tick,
            &ELECTRIC_TRACKED_ERRORS,
            check_offset,
            upgrades,
        ),
        recipes,
    }
}

/// O Metallurgic Infuser como a lib o recebe, também usado pelo teste dos rastros.
pub fn infuser_config(
    energy_per_tick: i32,
    recipes: Vec<Rc<ItemChemicalToItemRecipe>>,
    conversions: Vec<ChemicalConversion>,
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> ChemicalMachineConfig {
    ChemicalMachineConfig {
        machine: machine_config(
            energy_per_tick,
            &INFUSER_TRACKED_ERRORS,
            check_offset,
            upgrades,
        ),
        recipes,
        conversions,
        tank_capacity: TANK_CAPACITY,
    }
}

/// O `MEKTRACE` de um tick: os campos dos rastros de `dev/referencia/rastros`, na mesma ordem. O Metallurgic Infuser acrescenta
/// `tank` e `infusion` depois dos erros. Com `upgrades`, vêm no fim `speed`, `energy_upgrades`, `upgrade_progress`, `upgrade_in` e
/// `upgrade_out`, que só o rastro dos slots de upgrade grava.
pub fn trace_line(
    now: i64,
    machine: &ProgressMachine,
    chest: Option<&[(usize, String, u32)]>,
    upgrades: bool,
) -> String {
    let path = |id: &str| {
        id.split_once(':')
            .map_or(id.to_owned(), |(_, path)| path.to_owned())
    };
    let slot = |slot: &InventorySlot| {
        let id = slot
            .resource()
            .map_or(String::new(), |resource| path(&resource.id));
        format!("[\"{id}\",{}]", slot.amount())
    };
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
        (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
    ];
    let errors: Vec<String> = names
        .iter()
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| format!("\"{name}\""))
        .collect();
    let tile = machine.tile();
    let chemical = machine.chemical_side().map_or(String::new(), |side| {
        format!(
            ",\"tank\":{},\"infusion\":{}",
            side.tank.amount(),
            slot(&side.slot)
        )
    });
    // Com o ejetor ligado o rastro grava o baú: `[slot, id, quantidade]` dos slots com algo.
    let chest = chest.map_or(String::new(), |stacks| {
        let stacks: Vec<String> = stacks
            .iter()
            .map(|(index, id, amount)| format!("[{index},\"{id}\",{amount}]"))
            .collect();
        format!(",\"chest\":[{}]", stacks.join(","))
    });
    // `upgradeSlot` do `MachineTraceTest`: o tipo sem o namespace e a quantidade, ou vazio.
    let upgrade_slot = |slot: &InventorySlot| {
        let kind = slot
            .resource()
            .and_then(|resource| resource.variant)
            .map_or(String::new(), |variant| path(&variant));
        format!("[\"{kind}\",{}]", slot.amount())
    };
    let upgrades = if upgrades {
        let component = machine.upgrades();
        format!(
            ",\"speed\":{},\"energy_upgrades\":{},\"upgrade_progress\":{:?},\"upgrade_in\":{},\"upgrade_out\":{}",
            component.get_upgrades(SPEED_UPGRADE),
            component.get_upgrades(ENERGY_UPGRADE),
            component.scaled_upgrade_progress(),
            upgrade_slot(component.upgrade_slot()),
            upgrade_slot(component.upgrade_output_slot())
        )
    } else {
        String::new()
    };
    format!(
        "{{\"t\":{now},\"marks\":[],\"progress\":{},\"required\":{},\"energy\":{},\"capacity\":{},\"per_tick\":{},\"active\":{},\"block_active\":{},\"powered\":{},\"comparator\":{},\"input\":{},\"output\":{},\"errors\":[{}]{}{}{}}}",
        machine.recipe_progress(),
        machine.ticks_required(),
        machine.energy().amount(),
        machine.energy().capacity(),
        machine.energy().energy_per_tick(),
        tile.active(),
        tile.block_active(),
        tile.is_powered(),
        tile.current_redstone_level(),
        slot(machine.input()),
        slot(machine.output()),
        errors.join(","),
        chemical,
        chest,
        upgrades
    )
}

/// `AttributeEnergy.STORAGE_MULTIPLIER`: o buffer guarda quatro ticks de uso.
pub const fn energy_capacity(usage: i32) -> i32 {
    4 * usage
}

// Na ordem de getInitialInventory.
pub const INFUSION_SLOT: usize = 0;
pub const INPUT_SLOT: usize = 1;
pub const OUTPUT_SLOT: usize = 2;
pub const ENERGY_SLOT: usize = 3;
pub const SLOTS: usize = 4;

// Na ordem de TileEntityElectricMachine.getInitialInventory.
pub const ELECTRIC_INPUT_SLOT: usize = 0;
pub const ELECTRIC_OUTPUT_SLOT: usize = 1;
pub const ELECTRIC_ENERGY_SLOT: usize = 2;
pub const ELECTRIC_SLOTS: usize = 3;

const INPUT_SLOTS: &[usize] = &[INPUT_SLOT];
const INFUSION_SLOTS: &[usize] = &[INFUSION_SLOT];
const ELECTRIC_INPUT_SLOTS: &[usize] = &[ELECTRIC_INPUT_SLOT];

/// Quanto o cabo guarda, se o bloco for um cabo.
pub fn cable_capacity(block: &str) -> Option<i32> {
    let cable = UNIVERSAL_CABLES.iter().find(|(id, _)| *id == block);
    cable.map(|&(_, capacity)| capacity)
}

/// `LightConfigInfo.EXTRA_MACHINE`: a direita é saída, a base é o slot extra, as costas são o slot
/// de energia e o resto é entrada. O slot de energia ainda não aceita nada.
pub fn item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.right_of() || face == front.opposite() {
        FaceSlots::default()
    } else if face == Face::Down {
        FaceSlots {
            inputs: INFUSION_SLOTS,
            outputs: &[],
        }
    } else {
        FaceSlots {
            inputs: INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// `LightConfigInfo.MACHINE`: a direita é saída, as costas são o slot de energia, e o resto entrada.
pub fn electric_item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.right_of() || face == front.opposite() {
        FaceSlots::default()
    } else {
        FaceSlots {
            inputs: ELECTRIC_INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// Para um hopper: recebe por tudo que tem slot de entrada, e não entrega por nenhuma face. A saída
/// fica à direita, e um hopper só puxa do bloco acima dele.
pub fn sides(front: Face, slots: fn(Face, Face) -> FaceSlots) -> Sides {
    let mut sides = Sides::default();
    for face in Face::ALL {
        sides.input[face as usize] = !slots(front, face).inputs.is_empty();
    }
    sides
}

/// As pastas do original: `data/<ns>/mekanism/chemical` e as outras, para datapacks do Mekanism valerem aqui.
pub const CHEMICAL_FOLDER: &str = "mekanism/chemical";
pub const CHEMICAL_TAGS_FOLDER: &str = "tags/mekanism/chemical";
pub const UPGRADE_FOLDER: &str = "mekanism/upgrade";
pub const UPGRADE_TAGS_FOLDER: &str = "tags/mekanism/upgrade";
/// `MekanismAPI.EMPTY_CHEMICAL_KEY`: o químico vazio é a ausência dele, não algo que um tanque guarde.
pub const EMPTY_CHEMICAL: &str = "mekanism:empty";
const INFUSION_KIND: &str = "infusion";
const CHEMICAL_KIND: &str = "chemical";
const INFUSE_ICON: &str = "/infuse_type/";

/// `EnumColor`, pelo nome que os pigmentos usam em `color`.
const ENUM_COLORS: [(&str, u32); 18] = [
    ("black", 0x40_40_40),
    ("blue", 0x36_6B_D0),
    ("green", 0x59_C1_5F),
    ("cyan", 0x00_F3_D0),
    ("dark_red", 0xC9_07_1F),
    ("purple", 0xA4_60_D9),
    ("orange", 0xFF_A1_60),
    ("light_gray", 0xCF_CF_CF),
    ("gray", 0x7A_7A_7A),
    ("light_blue", 0x55_9E_FF),
    ("lime", 0x75_FF_89),
    ("aqua", 0x30_FF_F9),
    ("red", 0xFF_38_3C),
    ("magenta", 0xD5_5E_CB),
    ("yellow", 0xFF_DD_4F),
    ("white", 0xFF_FF_FF),
    ("brown", 0xA1_76_49),
    ("pink", 0xFF_BC_C4),
];

/// A cor de um químico. `color_representation` no Java cai na `tint`, e os pigmentos têm só o nome da cor.
fn chemical_color(json: &Value) -> Option<u32> {
    let text = ["color_representation", "tint"]
        .iter()
        .find_map(|key| json.get(*key).and_then(Value::as_str));
    if let Some(text) = text {
        return Some(u32::from_str_radix(text.strip_prefix('#')?, 16).ok()? & 0xFF_FFFF);
    }
    let name = json.get("color").and_then(Value::as_str)?;
    ENUM_COLORS
        .iter()
        .find(|(color, _)| *color == name)
        .map(|(_, rgb)| *rgb)
}

/// Lê um arquivo de `mekanism/chemical`, no formato do original.
pub fn chemical_resource(id: &str, json: &Value) -> Option<Resource> {
    if id == EMPTY_CHEMICAL {
        return None;
    }
    let color = chemical_color(json)?;
    let icon = json.get("icon").and_then(Value::as_str).unwrap_or_default();
    let kind = if icon.contains(INFUSE_ICON) {
        INFUSION_KIND
    } else {
        CHEMICAL_KIND
    };
    Some(Resource {
        id: id.to_owned(),
        kind: kind.to_owned(),
        color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_enter_by_the_front_the_left_and_the_top() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up] {
            assert_eq!(item_slots(front, face).inputs, [INPUT_SLOT], "{face:?}");
        }
    }

    #[test]
    fn the_base_feeds_the_infusion_slot() {
        assert_eq!(item_slots(Face::East, Face::Down).inputs, [INFUSION_SLOT]);
    }

    #[test]
    fn the_right_and_the_back_take_nothing() {
        for front in [Face::North, Face::South, Face::East, Face::West] {
            let sides = sides(front, item_slots);
            assert!(!sides.accepts_input(front.right_of()));
            assert!(!sides.accepts_input(front.opposite()));
            assert!(sides.accepts_input(front));
        }
    }

    #[test]
    fn no_face_gives_to_a_hopper() {
        assert_eq!(sides(Face::North, item_slots).output, [false; 6]);
        assert_eq!(sides(Face::North, electric_item_slots).output, [false; 6]);
    }

    #[test]
    fn an_electric_machine_also_takes_by_the_base() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up, Face::Down] {
            let slots = electric_item_slots(front, face);
            assert_eq!(slots.inputs, [ELECTRIC_INPUT_SLOT], "{face:?}");
        }
        assert!(
            electric_item_slots(front, front.right_of())
                .inputs
                .is_empty()
        );
        assert!(
            electric_item_slots(front, front.opposite())
                .inputs
                .is_empty()
        );
    }

    #[test]
    fn each_cable_tier_has_its_capacity() {
        assert_eq!(
            cable_capacity("mekanism:basic_universal_cable"),
            Some(8_000)
        );
        assert_eq!(
            cable_capacity("mekanism:ultimate_universal_cable"),
            Some(8_192_000)
        );
        assert_eq!(cable_capacity(INFUSER), None);
    }

    #[test]
    fn the_buffer_holds_four_ticks() {
        assert_eq!(energy_capacity(INFUSER_USAGE), 20);
        assert_eq!(energy_capacity(CRUSHER_USAGE), 32);
    }
}
