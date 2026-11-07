// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::cached_recipe::ItemToItemCache;
use machinery_basis_core::chemical::ConversionCache;
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::data_slots;
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::resource::Resource;
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::slot_view::{
    ContainerSlotType, DataType, ItemConfig, SlotOverlay, SlotSpec, SlotType, slot_type,
};
use machinery_basis_core::tile::{
    ChemicalMachineConfig, ElectricMachineConfig, MachineConfig, ProgressMachine, RedstoneControl,
    RedstoneSync, UpgradeConfig,
};
use machinery_basis_core::tooltip;
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use machinery_basis_core::units::{energy_fraction, stored_mb};
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::upgrade_window::UpgradeSync;
use machinery_basis_core::warning::{ErrorMask, WarningTracker};
use machinery_basis_core::window::WindowType;
use serde_json::Value;
use std::rc::Rc;

pub const INFUSER: &str = "mekanism:metallurgic_infuser";

/// O `gui.scaling` dos sprites que a tela desenha com escala própria, como os `.png.mcmeta` do original dizem (um teste confere
/// cada um contra o arquivo): o painel `base`, a moldura `bar/base`, a moldura `gauge/normal` e o mosaico de `bar/vertical_power`.
pub const PANEL_SLICE: NineSlice = NineSlice {
    size: [16.0, 16.0],
    border: [4.0; 4],
    stretch_inner: false,
};
pub const BAR_FRAME_SLICE: NineSlice = NineSlice {
    size: [5.0, 5.0],
    border: [1.0; 4],
    stretch_inner: false,
};
pub const GAUGE_FRAME_SLICE: NineSlice = NineSlice {
    size: [5.0, 5.0],
    border: [2.0; 4],
    stretch_inner: false,
};
pub const POWER_BAR_TILE: [f32; 2] = [2.0, 2.0];
pub const INFUSING: &str = "mekanism:metallurgic_infusing";
pub const CHEMICAL_CONVERSION: &str = "mekanism:chemical_conversion";
pub const ENERGY_CONVERSION: &str = "mekanism:energy_conversion";
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

/// Um pedaço do texto de uma linha de tooltip: o que o `Component` de tradução recebe como argumento.
#[derive(Clone, Debug, PartialEq)]
pub enum TooltipArg {
    /// `{"translate": chave, "color": cor}`.
    Translate { key: String, color: u32 },
    /// `{"text": "8", "color": cor}`.
    Number { value: i32, color: u32 },
}

/// Uma linha de tooltip: `MekanismLang.X.translateColored(cor, args...)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTooltipLine {
    pub translate: String,
    pub args: Vec<TooltipArg>,
    /// `None`: a linha usa a cor de texto padrão.
    pub color: Option<u32>,
}

/// `EnumColor.PURPLE`, `GRAY` e `AQUA`, a cor primária (RGB).
pub const PURPLE: u32 = 0xA4_60_D9;
pub const GRAY: u32 = 0xCF_CF_CF;
pub const AQUA: u32 = 0x30_FF_F9;

/// `Upgrade` → `Util.makeDescriptionId("upgrade", id)`: o nome e a descrição do tipo.
pub fn upgrade_name_key(upgrade: &str) -> String {
    let (namespace, path) = upgrade.split_once(':').unwrap_or(("minecraft", upgrade));
    format!("upgrade.{namespace}.{}", path.replace('/', "."))
}

/// O appender de `MekanismDataComponents.UPGRADE_TYPE`: o tipo (na cor dele), o máximo instalado e a descrição.
pub fn upgrade_tooltip(
    upgrade: &str,
    kind: &machinery_basis_core::upgrade::UpgradeType,
) -> Vec<ItemTooltipLine> {
    let name = upgrade_name_key(upgrade);
    vec![
        ItemTooltipLine {
            translate: "tooltip.mekanism.upgrade.type".to_owned(),
            args: vec![TooltipArg::Translate {
                key: name.clone(),
                color: enum_color(&kind.color) & 0xFF_FF_FF,
            }],
            color: Some(PURPLE),
        },
        ItemTooltipLine {
            translate: "tooltip.mekanism.upgrade.max_installed".to_owned(),
            args: vec![TooltipArg::Number {
                value: kind.max,
                color: AQUA,
            }],
            color: Some(GRAY),
        },
        ItemTooltipLine {
            translate: format!("{name}.description"),
            args: Vec::new(),
            color: None,
        },
    ]
}
/// `UpgradeIds.SPEED` e `UpgradeIds.ENERGY`.
pub const SPEED_UPGRADE: &str = "mekanism:speed";
pub const ENERGY_UPGRADE: &str = "mekanism:energy";
/// `MekanismSounds.ENRICHMENT_CHAMBER`, `CRUSHER`, `ENERGIZED_SMELTER` e `METALLURGIC_INFUSER`.
pub const ENRICHMENT_SOUND: &str = "mekanism:tile.machine.enrichment_chamber";
pub const CRUSHER_SOUND: &str = "mekanism:tile.machine.crusher";
pub const SMELTER_SOUND: &str = "mekanism:tile.machine.energized_smelter";
pub const INFUSER_SOUND: &str = "mekanism:tile.machine.metallurgic_infuser";
/// `Holiday.XMAS_FILTERABLE_SOUNDS`, dos que estão portados (`PURIFICATION_CHAMBER` / `CHRISTMAS3` não).
pub const CHRISTMAS_SOUNDS: [(&str, &str); 4] = [
    (ENRICHMENT_SOUND, "mekanism:tile.christmas.1"),
    (INFUSER_SOUND, "mekanism:tile.christmas.2"),
    (SMELTER_SOUND, "mekanism:tile.christmas.4"),
    (CRUSHER_SOUND, "mekanism:tile.christmas.5"),
];
/// `Holiday.Christmas`: 25 de dezembro.
pub const CHRISTMAS: (u32, u32) = (12, 25);
pub const XP_CONTAINMENT_UPGRADE: &str = "mekanism:xp_containment";

/// `GuiMekanismTile.addGenericTabs`: `new GuiXpContainmentTab(this, recipeLookupHandler, false, 90)`, na lateral direita (`imageWidth`) a
/// 90. As quatro máquinas são `IRecipeLookupHandler` e têm o livro de receitas (`!Mekanism.hooks.isRecipeBookDisabled()`), então a aba
/// fica onde o Java a põe, sem o deslocamento de 28.
pub const XP_TAB_Y: i32 = 90;
/// `AbstractContainerScreen.imageWidth`.
pub const SCREEN_IMAGE_WIDTH: i32 = 176;
/// `SpecialColors.TAB_XP_CONTAINMENT`: o tom do suporte da aba.
pub const XP_TAB_TINT: u32 = 0xFF_80_FF_20;
/// `GuiXpContainmentTab.ICON` (`tab/xp_containment`), 18 por 18 sobre o botão.
pub const XP_TAB_ICON: &str = "mekanism:gui/sprites/tab/xp_containment";
/// `MekanismLang.XP_CONTAINMENT_TAB`, o tooltip da aba.
pub const XP_TAB_TOOLTIP: &str = "tab.mekanism.xp_containment";

/// `GuiEnergyTab`: `super(Mekanism.rl("tab/energy_info"), gui, -26, 137, 26, 26)`, um elemento de textura na lateral esquerda, sem clique.
pub const ENERGY_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 137.0, 26.0, 26.0],
    };
pub const ENERGY_TAB_SPRITE: &str = "mekanism:gui/sprites/tab/energy_info";
/// `MekanismLang.USING`, `NEEDED` e `UNIT`.
pub const USING_LANG: &str = "gui.mekanism.using";
pub const NEEDED_LANG: &str = "gui.mekanism.needed";
pub const UNIT_LANG: &str = "gui.mekanism.unit";

/// O tooltip de `GuiEnergyTab(gui, energyContainer, isActive)` das máquinas: o que usa por tick (`getEnergyPerTick` com a máquina ativa, senão
/// zero), o que falta para encher (`getNeededAsLong`: `max(0, capacidade - energia)`) e a unidade.
pub fn energy_tab_tooltip(
    active: bool,
    energy_per_tick: u32,
    energy: u64,
    capacity: u64,
    translate: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let using = if active {
        u64::from(energy_per_tick)
    } else {
        0
    };
    let needed = capacity.saturating_sub(energy);
    machinery_basis_core::units::energy_info_lines(
        &[
            (&translate(USING_LANG), using),
            (&translate(NEEDED_LANG), needed),
        ],
        &translate(ENERGY_UNIT_LANG),
        &translate(UNIT_LANG),
    )
}

/// A aba de xp das telas das máquinas.
pub fn xp_tab() -> machinery_basis_core::side_tab::XpTab {
    machinery_basis_core::side_tab::XpTab::new(SCREEN_IMAGE_WIDTH, XP_TAB_Y, true)
}
pub const MUFFLING_UPGRADE: &str = "mekanism:muffling";
/// `MekanismTags.Upgrades.DEFAULT_MACHINE_UPGRADES`: o que `AttributeUpgradeSupport.DEFAULT_MACHINE_UPGRADES` aceita, e que as
/// quatro máquinas portadas usam.
pub const DEFAULT_MACHINE_UPGRADES: &str = "mekanism:upgrade_support/default_machine";

/// O tipo de upgrade que um item carrega no componente `mekanism:upgrade_type`, se carrega: o valor do componente é um texto.
pub fn upgrade_type_of(resource: &ItemResource) -> Option<String> {
    Components::parse_text_value(resource.components.get(UPGRADE_TYPE_COMPONENT)?)
}

/// Os dados de upgrade que as quatro máquinas portadas usam: os tipos, o que a tag `default_machine` aceita, e o item que os
/// carrega, com o limite de pilha dele.
pub fn upgrade_data(
    types: &UpgradeTypes,
    tags: &UpgradeTags,
    item_max_stack_size: u32,
) -> Rc<UpgradeData> {
    Rc::new(UpgradeData::new(
        UPGRADE_ITEM,
        UPGRADE_TYPE_COMPONENT,
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

/// Os elementos da tela que mostram um aviso: cada um tem a própria verificação (`ISupportsWarning.warning`), e o que ele desenha
/// depende só dela, não do aviso que a aba lista.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenElement {
    /// `GuiVerticalPowerBar`.
    EnergyBar,
    /// `GuiProgress`.
    Progress,
    /// `GuiChemicalBar`, a barra de infusão do Metallurgic Infuser.
    ChemicalBar,
    /// O slot de entrada, `InputInventorySlot`.
    InputSlot,
    /// O slot de saída, `OutputInventorySlot`.
    OutputSlot,
}

/// Os registros de aviso de uma tela de máquina: o elemento, o tipo que a aba lista e o erro que o liga.
pub type ScreenWarnings = [(ScreenElement, WarningType, RecipeError)];

/// O que a tela de uma `TileEntityElectricMachine` registra: `GuiElectricMachine` (a barra de energia e a seta de progresso) e os
/// slots de `TileEntityElectricMachine.getInitialInventory` (entrada e saída).
pub const ELECTRIC_WARNINGS: [(ScreenElement, WarningType, RecipeError); 4] = [
    (
        ScreenElement::EnergyBar,
        WarningType::NotEnoughEnergy,
        RecipeError::NOT_ENOUGH_ENERGY,
    ),
    (
        ScreenElement::Progress,
        WarningType::InputDoesntProduceOutput,
        RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
    ),
    (
        ScreenElement::InputSlot,
        WarningType::NoMatchingRecipe,
        RecipeError::NOT_ENOUGH_INPUT,
    ),
    (
        ScreenElement::OutputSlot,
        WarningType::NoSpaceInOutput,
        RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    ),
];

/// O mesmo do Metallurgic Infuser: `GuiMetallurgicInfuser` e `TileEntityMetallurgicInfuser.getInitialInventory`. A barra do
/// químico e o slot de entrada dão os dois `NO_MATCHING_RECIPE`, e a falta de saída vem do slot de saída.
pub const INFUSER_WARNINGS: [(ScreenElement, WarningType, RecipeError); 5] = [
    (
        ScreenElement::EnergyBar,
        WarningType::NotEnoughEnergy,
        RecipeError::NOT_ENOUGH_ENERGY,
    ),
    (
        ScreenElement::Progress,
        WarningType::InputDoesntProduceOutput,
        RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
    ),
    (
        ScreenElement::ChemicalBar,
        WarningType::NoMatchingRecipe,
        RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    ),
    (
        ScreenElement::InputSlot,
        WarningType::NoMatchingRecipe,
        RecipeError::NOT_ENOUGH_INPUT,
    ),
    (
        ScreenElement::OutputSlot,
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
/// máquina) e o que `addContainerTrackers` manda de redstone (o modo, `redstone` e `redstoneLastTick`).
#[derive(Clone, Copy, Debug)]
pub struct SyncedMachine {
    pub tracked: &'static [RecipeError],
    pub errors: ErrorMask,
    pub redstone: RedstoneSync,
}

impl SyncedMachine {
    /// O que o servidor manda de uma máquina: a máscara e o redstone. Os mesmos valores que o cliente lê dos data slots.
    pub fn of(machine: &ProgressMachine, tracked: &'static [RecipeError]) -> Self {
        Self {
            tracked,
            errors: machine.tracked_errors_mask(),
            redstone: machine.tile().redstone_sync(),
        }
    }
}

impl WarningSource for SyncedMachine {
    fn has_error(&self, error: RecipeError) -> bool {
        self.errors.has(self.tracked, error)
    }

    fn control_type(&self) -> RedstoneControl {
        self.redstone.control_type
    }

    fn is_redstone_activated(&self) -> bool {
        self.redstone.is_activated()
    }
}

/// O `WarningTracker` de uma tela de máquina: os pares da máquina, e os três de redstone de `GuiMekanismTile.addGenericTabs`, que
/// toda máquina com controle de redstone registra.
pub fn machine_warnings<'a>(
    source: &'a dyn WarningSource,
    pairs: &ScreenWarnings,
) -> WarningTracker<'a, WarningType> {
    let mut tracker = WarningTracker::new(Some(WARNINGS_STOP_AFTER));
    for &(_, kind, error) in pairs {
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

/// Onde cada valor fica nos data slots da tela de uma máquina. Cada slot trafega em 16 bits: energia, tint e capacidade ocupam dois.
/// A capacidade muda com os upgrades e a tela não a sabe de cabeça, e os `ticksRequired` também (`TileEntityProgressMachine`
/// sincroniza os dois). A máscara de erros e o redstone são o que `SyncedMachine` lê.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLayout {
    pub energy: usize,
    /// O tanque de químico, nas máquinas que têm um.
    pub chemical: Option<ChemicalSlots>,
    pub progress: usize,
    pub capacity: usize,
    pub errors: usize,
    pub redstone: usize,
    /// `ticksRequired`: com upgrades de velocidade a seta de progresso se enche por ele, não pelo de base.
    pub ticks_required: usize,
    /// Onde começam os data slots dos upgrades (`UPGRADE_SYNC_LEN` deles): `upgradeTicks` e uma contagem por tipo.
    pub upgrade: usize,
    /// `hasXpContainment`, o `SyncableBoolean` que `TileEntityRecipeMachine.addContainerTrackers` registra: a aba de xp só aparece com ele.
    /// Fica depois dos upgrades, para os índices dos outros valores não mudarem.
    pub xp_containment: usize,
    /// `getEnergyPerTick` (`SyncableInt` de `addContainerTrackers`, dois slots): o `Using: N/t` da aba de energia. A aba pede o que o
    /// contêiner do cliente tem, e ele só sabe depois do primeiro valor do servidor.
    pub energy_per_tick: usize,
    /// Quantos data slots o menu declara: o `data_slots` do arquivo de menu.
    pub len: usize,
}

/// Quantos tipos de upgrade o mod declara em `data/mekanism/mekanism/upgrade`: cada um tem a contagem dele num data slot.
pub const UPGRADE_TYPES: usize = 8;

/// Os data slots dos upgrades: `upgradeTicks` e uma contagem por tipo, na ordem do registro. O Java os sincroniza só com a janela
/// aberta, por jogador, e aqui eles são do bloco (veja `docs/mekanism-no-horizon.md`).
pub const UPGRADE_SYNC_LEN: usize = 1 + UPGRADE_TYPES;

/// Os data slots do tanque de químico: a quantidade (um slot), o `tint` ARGB do que ele guarda (dois), o sprite (um, o índice em
/// [`CHEMICAL_ICONS`] mais um, e zero sem sprite) e o id (`CHEMICAL_ID_SLOTS` slots, dois bytes em cada), de onde sai o nome do
/// tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChemicalSlots {
    pub amount: usize,
    pub tint: usize,
    pub icon: usize,
    pub id: usize,
}

/// Quantos data slots levam o id do químico: 32 caracteres, o bastante para `mekanism:refined_obsidian`.
pub const CHEMICAL_ID_SLOTS: usize = 16;

pub const ELECTRIC_DATA: DataLayout = DataLayout {
    energy: 0,
    chemical: None,
    progress: 2,
    capacity: 3,
    errors: 5,
    redstone: 6,
    ticks_required: 7,
    upgrade: 8,
    xp_containment: 8 + UPGRADE_SYNC_LEN,
    energy_per_tick: 9 + UPGRADE_SYNC_LEN,
    len: 11 + UPGRADE_SYNC_LEN,
};

pub const INFUSER_DATA: DataLayout = DataLayout {
    energy: 0,
    chemical: Some(ChemicalSlots {
        amount: 2,
        tint: 3,
        icon: 11,
        id: 12,
    }),
    progress: 5,
    capacity: 6,
    errors: 8,
    redstone: 9,
    ticks_required: 10,
    upgrade: 12 + CHEMICAL_ID_SLOTS,
    xp_containment: 12 + CHEMICAL_ID_SLOTS + UPGRADE_SYNC_LEN,
    energy_per_tick: 13 + CHEMICAL_ID_SLOTS + UPGRADE_SYNC_LEN,
    len: 15 + CHEMICAL_ID_SLOTS + UPGRADE_SYNC_LEN,
};

/// Escreve o `upgradeTicks` e o mapa de upgrades nos data slots. Fora de `ScreenData` porque o mapa não é `Copy`.
pub fn write_upgrade_sync(data: &mut [i32], layout: &DataLayout, sync: &UpgradeSync) {
    for (slot, value) in data
        .iter_mut()
        .skip(layout.upgrade)
        .zip(sync.write(UPGRADE_SYNC_LEN))
    {
        *slot = value;
    }
}

/// O tanque de químico esvaziado fora do tick (`IHasDumpButton.dump`, que o pacote `GuiInteraction.DUMP_BUTTON` roda quando chega):
/// põe nos data slots o que o tick escreveria para um tanque vazio. O Java sincroniza o tanque ao jogador no `broadcastChanges` do
/// menu, que roda mesmo com o servidor parado (`tick freeze`); aqui os data slots são do bloco e só o tick os escrevia.
pub fn clear_chemical_screen(layout: &DataLayout, data: &mut [i32]) {
    let Some(chemical) = layout.chemical else {
        return;
    };
    let empty = ScreenData::default().write(layout);
    let slots = [
        chemical.amount,
        chemical.tint,
        chemical.tint + 1,
        chemical.icon,
    ];
    for index in slots
        .into_iter()
        .chain(chemical.id..chemical.id + CHEMICAL_ID_SLOTS)
    {
        if let Some(slot) = data.get_mut(index) {
            *slot = empty[index];
        }
    }
}

/// O que o cliente lê dos data slots dos upgrades.
pub fn read_upgrade_sync(layout: &DataLayout, data: &[i32]) -> UpgradeSync {
    // Só os `UPGRADE_SYNC_LEN` do mapa: o data slot da aba de xp vem logo depois.
    let slots = data.get(layout.upgrade..).unwrap_or_default();
    UpgradeSync::read(&slots[..slots.len().min(UPGRADE_SYNC_LEN)])
}

/// O que o servidor escreve nos data slots de uma máquina e o cliente lê.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenData {
    pub energy: u32,
    pub capacity: u32,
    pub progress: i32,
    /// `ticksRequired`, zero antes de o servidor mandar.
    pub ticks_required: i32,
    /// A quantidade do tanque de químico, o `tint` ARGB do que ele guarda e o sprite (índice em [`CHEMICAL_ICONS`] mais um).
    pub chemical_amount: i32,
    pub chemical_tint: u32,
    pub chemical_icon: u32,
    /// O id do químico, empacotado como [`data_slots::pack_text`]; vazio sem químico.
    pub chemical_id: [u16; CHEMICAL_ID_SLOTS],
    pub errors: ErrorMask,
    pub redstone: RedstoneSync,
    /// `hasXpContainment()` no servidor: o upgrade de xp está instalado (ou o tipo saiu do registro).
    pub xp_containment: bool,
    /// `getEnergyPerTick()` no servidor, zero antes de o servidor mandar.
    pub energy_per_tick: u32,
}

impl ScreenData {
    pub fn write(&self, layout: &DataLayout) -> Vec<i32> {
        let mut data = vec![0; layout.len];
        let [low, high] = data_slots::split(self.energy);
        data[layout.energy] = low;
        data[layout.energy + 1] = high;
        data[layout.progress] = self.progress;
        let [low, high] = data_slots::split(self.capacity);
        data[layout.capacity] = low;
        data[layout.capacity + 1] = high;
        if let Some(chemical) = layout.chemical {
            data[chemical.amount] = self.chemical_amount;
            let [low, high] = data_slots::split(self.chemical_tint);
            data[chemical.tint] = low;
            data[chemical.tint + 1] = high;
            data[chemical.icon] = self.chemical_icon as i32;
            for (index, slot) in self.chemical_id.iter().enumerate() {
                data[chemical.id + index] = i32::from(*slot as i16);
            }
        }
        data[layout.errors] = self.errors.to_slot();
        data[layout.redstone] = self.redstone.to_slot();
        data[layout.ticks_required] = self.ticks_required;
        data[layout.xp_containment] = i32::from(self.xp_containment);
        let [low, high] = data_slots::split(self.energy_per_tick);
        data[layout.energy_per_tick] = low;
        data[layout.energy_per_tick + 1] = high;
        data
    }

    /// O que o cliente lê. Antes dos primeiros valores do servidor os data slots estão vazios, e o que falta vale zero.
    pub fn read(layout: &DataLayout, data: &[i32]) -> Self {
        let get = |index: usize| data.get(index).copied().unwrap_or(0);
        let chemical = layout.chemical.map(|chemical| {
            let mut id = [0u16; CHEMICAL_ID_SLOTS];
            for (index, slot) in id.iter_mut().enumerate() {
                *slot = get(chemical.id + index) as u16;
            }
            (
                get(chemical.amount),
                data_slots::join(get(chemical.tint), get(chemical.tint + 1)),
                get(chemical.icon) as u32 & 0xFFFF,
                id,
            )
        });
        let (chemical_amount, chemical_tint, chemical_icon, chemical_id) =
            chemical.unwrap_or_default();
        Self {
            energy: data_slots::join(get(layout.energy), get(layout.energy + 1)),
            capacity: data_slots::join(get(layout.capacity), get(layout.capacity + 1)),
            progress: get(layout.progress),
            ticks_required: get(layout.ticks_required),
            chemical_amount,
            chemical_tint,
            chemical_icon,
            chemical_id,
            errors: ErrorMask::from_slot(get(layout.errors)),
            redstone: RedstoneSync::from_slot(get(layout.redstone)),
            xp_containment: get(layout.xp_containment) != 0,
            energy_per_tick: data_slots::join(
                get(layout.energy_per_tick),
                get(layout.energy_per_tick + 1),
            ),
        }
    }
}

/// O que a tela de um tipo de máquina sabe: os avisos (os erros que a máquina rastreia e os registros dos elementos), os data
/// slots e os slots de item com a configuração de lados que decide a moldura deles.
#[derive(Clone, Copy, Debug)]
pub struct ScreenSpec {
    pub tracked: &'static [RecipeError],
    pub warnings: &'static ScreenWarnings,
    pub data: &'static DataLayout,
    pub item_config: &'static ItemConfig,
    pub slots: &'static [SlotSpec],
}

/// `TileComponentConfig.setupItemIOConfig`, o que `TileEntityElectricMachine` registra: entrada, saída, os dois juntos e energia.
pub const ELECTRIC_ITEM_CONFIG: ItemConfig = ItemConfig {
    entries: &[
        (DataType::Input, &[ELECTRIC_INPUT_SLOT]),
        (DataType::Output, &[ELECTRIC_OUTPUT_SLOT]),
        (
            DataType::InputOutput,
            &[ELECTRIC_INPUT_SLOT, ELECTRIC_OUTPUT_SLOT],
        ),
        (DataType::Energy, &[ELECTRIC_ENERGY_SLOT]),
    ],
};

/// `setupItemIOExtraConfig`, do Metallurgic Infuser: a mesma coisa mais o slot extra, o do químico.
pub const INFUSER_ITEM_CONFIG: ItemConfig = ItemConfig {
    entries: &[
        (DataType::Input, &[INPUT_SLOT]),
        (DataType::Output, &[OUTPUT_SLOT]),
        (DataType::InputOutput, &[INPUT_SLOT, OUTPUT_SLOT]),
        (DataType::Extra, &[INFUSION_SLOT]),
        (DataType::Energy, &[ENERGY_SLOT]),
    ],
};

/// Os slots de `TileEntityElectricMachine.getInitialInventory`, na ordem dos índices `ELECTRIC_*_SLOT`: a posição é a do item,
/// e o `EnergyInventorySlot` leva o raio por cima da moldura.
pub const ELECTRIC_SLOTS_SPEC: [SlotSpec; ELECTRIC_SLOTS] = [
    SlotSpec {
        item: [64.0, 17.0],
        container: ContainerSlotType::Input,
        overlay: None,
    },
    SlotSpec {
        item: [116.0, 35.0],
        container: ContainerSlotType::Output,
        overlay: None,
    },
    SlotSpec {
        item: [64.0, 53.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Power),
    },
];

/// Os slots de `TileEntityMetallurgicInfuser.getInitialInventory`, na ordem dos índices: o do químico é `EXTRA` no contêiner.
pub const INFUSER_SLOTS_SPEC: [SlotSpec; SLOTS] = [
    SlotSpec {
        item: [17.0, 35.0],
        container: ContainerSlotType::Extra,
        overlay: None,
    },
    SlotSpec {
        item: [51.0, 43.0],
        container: ContainerSlotType::Input,
        overlay: None,
    },
    SlotSpec {
        item: [109.0, 43.0],
        container: ContainerSlotType::Output,
        overlay: None,
    },
    SlotSpec {
        item: [143.0, 35.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Power),
    },
];

pub const ELECTRIC_SCREEN: ScreenSpec = ScreenSpec {
    tracked: &ELECTRIC_TRACKED_ERRORS,
    warnings: &ELECTRIC_WARNINGS,
    data: &ELECTRIC_DATA,
    item_config: &ELECTRIC_ITEM_CONFIG,
    slots: &ELECTRIC_SLOTS_SPEC,
};

pub const INFUSER_SCREEN: ScreenSpec = ScreenSpec {
    tracked: &INFUSER_TRACKED_ERRORS,
    warnings: &INFUSER_WARNINGS,
    data: &INFUSER_DATA,
    item_config: &INFUSER_ITEM_CONFIG,
    slots: &INFUSER_SLOTS_SPEC,
};

/// O que a tela desenha de aviso, decidido do que o cliente tem dos data slots: a aba (`hasWarning`), as linhas dela
/// (`getWarnings`) e que elementos estão em aviso.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarningView {
    /// `getWarnings`, na ordem da tela e com o corte de `WARNINGS_STOP_AFTER`.
    pub issues: Vec<WarningType>,
    tab: bool,
    elements: Vec<ScreenElement>,
}

impl WarningView {
    /// A aba aparece (`GuiWarningTab.updateVisibility`).
    pub fn tab(&self) -> bool {
        self.tab
    }

    /// O elemento está em aviso: alguma das verificações dele diz que sim (`ISupportsWarning.compound`).
    pub fn element(&self, element: ScreenElement) -> bool {
        self.elements.contains(&element)
    }
}

impl ScreenSpec {
    /// A moldura do slot `index` (`GuiMekanism.addSlots` com `findDataType`).
    pub fn slot_type(&self, index: usize) -> Option<SlotType> {
        slot_type(Some(self.item_config), index, self.slots[index].container)
    }

    /// A máquina como o cliente a vê.
    pub fn synced(&self, data: &[i32]) -> SyncedMachine {
        let read = ScreenData::read(self.data, data);
        SyncedMachine {
            tracked: self.tracked,
            errors: read.errors,
            redstone: read.redstone,
        }
    }

    pub fn view(&self, data: &[i32]) -> WarningView {
        let source = self.synced(data);
        let tracker = machine_warnings(&source, self.warnings);
        let mut elements = Vec::new();
        for &(element, _, error) in self.warnings {
            if source.has_error(error) && !elements.contains(&element) {
                elements.push(element);
            }
        }
        WarningView {
            issues: tracker.warnings(),
            tab: tracker.has_warning(),
            elements,
        }
    }
}

/// Uma linha do tooltip da aba, e a cor dela quando não é a do padrão.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TooltipLine {
    pub text: String,
    pub color: Option<u32>,
}

/// O tooltip de `GuiWarningTab.updateTooltip`: `MekanismLang.ISSUES` em amarelo e uma linha por aviso, cada uma cortada na
/// largura do tooltip (`Tooltip.create`). `translate` traduz uma chave de idioma e `width` mede texto em pixels de GUI.
pub fn tab_tooltip(
    issues: &[WarningType],
    translate: &dyn Fn(&str) -> String,
    width: &dyn Fn(&str) -> f32,
) -> Vec<TooltipLine> {
    let mut lines = Vec::new();
    for text in tooltip::split_lines(&translate(ISSUES_LANG), tooltip::MAX_WIDTH, width) {
        lines.push(TooltipLine {
            text,
            color: Some(ENUM_YELLOW),
        });
    }
    for issue in issues {
        for text in tooltip::split_lines(&translate(issue.lang_key()), tooltip::MAX_WIDTH, width) {
            lines.push(TooltipLine { text, color: None });
        }
    }
    lines
}

/// As chaves de idioma que os tooltips das barras usam: `MekanismLang.GENERIC_FRACTION`, `GENERIC_STORED_MB`, `EMPTY` e a unidade
/// de energia (`ENERGY_FORGE`).
pub const FRACTION_LANG: &str = "generic.mekanism.fraction";
pub const STORED_MB_LANG: &str = "generic.mekanism.stored.mb";
pub const EMPTY_LANG: &str = "gui.mekanism.empty";
pub const ENERGY_UNIT_LANG: &str = "energy.mekanism.forge.short";

/// A chave de idioma do nome de um químico (`Chemical.getTranslationKey`: `chemical.<namespace>.<nome>`).
pub fn chemical_lang_key(id: &str) -> String {
    match id.split_once(':') {
        Some((namespace, name)) => format!("chemical.{namespace}.{name}"),
        None => format!("chemical.{id}"),
    }
}

/// O tooltip de `GuiVerticalPowerBar`: `EnergyDisplay.getTextComponent`, a energia e a capacidade como fração (só a energia se a
/// capacidade for zero).
pub fn energy_tooltip(energy: u64, capacity: u64, translate: &dyn Fn(&str) -> String) -> String {
    energy_fraction(
        energy,
        capacity,
        &translate(ENERGY_UNIT_LANG),
        &translate(FRACTION_LANG),
    )
}

/// O tooltip de `GuiChemicalBar` (`GuiTankBar.getContentsTooltips`): `Empty`, ou o nome e a quantidade em mB.
pub fn chemical_tooltip(
    name: Option<&str>,
    amount: u64,
    translate: &dyn Fn(&str) -> String,
) -> String {
    match name {
        None => translate(EMPTY_LANG),
        Some(name) => stored_mb(&translate(STORED_MB_LANG), name, amount),
    }
}

/// `MekanismLang.ISSUES`.
pub const ISSUES_LANG: &str = "gui.mekanism.issues";

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
            xp_containment: Rc::from(XP_CONTAINMENT_UPGRADE),
            muffling: Rc::from(MUFFLING_UPGRADE),
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
    recipes: Rc<ItemToItemCache>,
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
        energy_conversion: no_energy_conversion(),
    }
}

/// Um `MekanismRecipeType.ENERGY_CONVERSION.getInputCache()` sem receita: o que as máquinas têm antes de quem as monta dar o índice
/// do jogo (`glue.rs` o põe em `energy_conversion`), e o que os testes sem conversão usam.
pub fn no_energy_conversion() -> Rc<EnergyConversionCache> {
    EnergyConversion::cache_of(Vec::new())
}

/// O `getInputCache` de `MekanismRecipeType.METALLURGIC_INFUSING` (`InputRecipeCache.ItemChemical`): o índice do químico não guarda
/// `ChemicalIds.EMPTY`. `source` é o `getRecipes(world)`, lido na primeira pergunta depois de um `clear`.
pub fn infusing_cache(
    source: impl Fn() -> Vec<Rc<ItemChemicalToItemRecipe>> + 'static,
) -> Rc<ItemChemicalCache> {
    ItemChemicalToItemRecipe::cache(source, EMPTY_CHEMICAL)
}

/// O mesmo índice sobre uma lista que não muda.
pub fn infusing_cache_of(recipes: Vec<Rc<ItemChemicalToItemRecipe>>) -> Rc<ItemChemicalCache> {
    ItemChemicalToItemRecipe::cache_of(recipes, EMPTY_CHEMICAL)
}

/// O Metallurgic Infuser como a lib o recebe, também usado pelo teste dos rastros.
pub fn infuser_config(
    energy_per_tick: i32,
    recipes: Rc<ItemChemicalCache>,
    conversions: Rc<ConversionCache>,
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
        energy_conversion: no_energy_conversion(),
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
            .and_then(|resource| upgrade_type_of(&resource))
            .map_or(String::new(), |kind| path(&kind));
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
/// Os dois slots virtuais da janela de upgrades vêm depois dos da máquina (`TileComponentUpgrade`: entrada e saída).
pub const UPGRADE_SLOT: usize = SLOTS;
pub const UPGRADE_OUTPUT_SLOT: usize = SLOTS + 1;
/// Quantos slots o contêiner do Metallurgic Infuser declara, os virtuais inclusos.
pub const MENU_SLOTS: usize = SLOTS + 2;

// Na ordem de TileEntityElectricMachine.getInitialInventory.
pub const ELECTRIC_INPUT_SLOT: usize = 0;
pub const ELECTRIC_OUTPUT_SLOT: usize = 1;
pub const ELECTRIC_ENERGY_SLOT: usize = 2;
pub const ELECTRIC_SLOTS: usize = 3;
pub const ELECTRIC_UPGRADE_SLOT: usize = ELECTRIC_SLOTS;
pub const ELECTRIC_UPGRADE_OUTPUT_SLOT: usize = ELECTRIC_SLOTS + 1;
/// Quantos slots o contêiner das máquinas elétricas declara, os virtuais inclusos.
pub const ELECTRIC_MENU_SLOTS: usize = ELECTRIC_SLOTS + 2;

/// `MekanismContainer.UPGRADE_WINDOW`: a chave de `startTracking` da janela de upgrades.
pub const UPGRADE_TRACKING_KEY: u8 = 2;
/// Quantos dados a janela acrescenta ao contêiner (`getSpecificSyncableData`: `upgradeTicks` e o mapa).
pub const UPGRADE_TRACKED_ITEMS: usize = 2;
/// Quantos dados o contêiner já sincroniza ao nascer (`addContainerTrackers`), medidos nos rastros `*_container_tracking`.
pub const ELECTRIC_TRACKED_BASE: usize = 16;
pub const INFUSER_TRACKED_BASE: usize = 18;

/// `EnumColor.getColor` pelo nome que o arquivo do tipo de upgrade usa (`color`), a cor primária (`colormap/primary.png`) em ARGB. Só as
/// que os tipos declarados usam; um nome que não é daqui vale branco, o que o `Upgrade.argb()` de uma cor sem tinta dá.
pub fn enum_color(name: &str) -> u32 {
    match name {
        "green" => 0xFF_59_C1_5F,
        "yellow" => 0xFF_FF_DD_4F,
        "lime" => 0xFF_75_FF_89,
        "cyan" => 0xFF_00_F3_D0,
        "light_blue" => 0xFF_55_9E_FF,
        "red" => 0xFF_FF_38_3C,
        "orange" => 0xFF_FF_A1_60,
        _ => 0xFF_FF_FF_FF,
    }
}

/// `WindowType`, na ordem do enum do Java. `CRAFTING` aceita `MAX_CRAFTING_WINDOWS` (3) janelas.
pub const WINDOW_TYPES: [WindowType; 11] = [
    window_type(0, Some("color"), false, 1),
    window_type(1, Some("confirmation"), false, 1),
    window_type(2, Some("crafting"), true, 3),
    window_type(3, Some("mekasuit_helmet"), false, 1),
    window_type(4, Some("recipe_book"), false, 1),
    window_type(5, Some("rename"), false, 1),
    window_type(6, Some("skin_select"), false, 1),
    window_type(7, Some("side_config"), true, 1),
    window_type(8, Some("transporter_config"), true, 1),
    window_type(9, Some("upgrade"), true, 1),
    window_type(10, None, false, 1),
];
/// `WindowType.UPGRADE`.
pub const UPGRADE_WINDOW: WindowType = WINDOW_TYPES[9];

const fn window_type(
    ordinal: u8,
    save_name: Option<&'static str>,
    can_pin: bool,
    max_data: u8,
) -> WindowType {
    WindowType {
        ordinal,
        save_name,
        can_pin,
        max_data,
    }
}

const INPUT_SLOTS: &[usize] = &[INPUT_SLOT];
const INFUSION_SLOTS: &[usize] = &[INFUSION_SLOT];
const ELECTRIC_INPUT_SLOTS: &[usize] = &[ELECTRIC_INPUT_SLOT];
const ENERGY_SLOTS: &[usize] = &[ENERGY_SLOT];
const ELECTRIC_ENERGY_SLOTS: &[usize] = &[ELECTRIC_ENERGY_SLOT];

/// Quanto o cabo guarda, se o bloco for um cabo.
pub fn cable_capacity(block: &str) -> Option<i32> {
    let cable = UNIVERSAL_CABLES.iter().find(|(id, _)| *id == block);
    cable.map(|&(_, capacity)| capacity)
}

/// `LightConfigInfo.EXTRA_MACHINE`: a direita é saída, a base é o slot extra, as costas são o slot
/// de energia e o resto é entrada. As costas são `DataType.ENERGY` (`InventorySlotInfo(true, true, ..)`): põem e tiram pela regra
/// do slot de energia (`EnergyInventorySlot.fillOrConvert`). A base é `DataType.EXTRA`
/// (`InventorySlotInfo(true, true, ..)`), então quem a alcança pode também tirar o que a regra do
/// slot de infusão deixa tirar (`ChemicalInventorySlot.fillOrConvert`: o que ele não converte).
pub fn item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.opposite() {
        FaceSlots {
            inputs: ENERGY_SLOTS,
            outputs: ENERGY_SLOTS,
        }
    } else if face == front.right_of() {
        FaceSlots::default()
    } else if face == Face::Down {
        FaceSlots {
            inputs: INFUSION_SLOTS,
            outputs: INFUSION_SLOTS,
        }
    } else {
        FaceSlots {
            inputs: INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// `LightConfigInfo.MACHINE`: a direita é saída, as costas são o slot de energia, e o resto entrada.
/// Todas as faces de entrada são `DataType.INPUT`, `InventorySlotInfo(true, false, ..)`: quem está de fora põe e não tira. As costas
/// são `DataType.ENERGY`, `InventorySlotInfo(true, true, ..)`, e decidem pela regra do slot de energia.
pub fn electric_item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.opposite() {
        FaceSlots {
            inputs: ELECTRIC_ENERGY_SLOTS,
            outputs: ELECTRIC_ENERGY_SLOTS,
        }
    } else if face == front.right_of() {
        FaceSlots::default()
    } else {
        FaceSlots {
            inputs: ELECTRIC_INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// Para um hopper: recebe por tudo que tem slot de entrada, e entrega por onde há slot que se tira. A saída
/// (`DataType.OUTPUT`) fica à direita, e um hopper só puxa do bloco acima dele, então só a base vale.
pub fn sides(front: Face, slots: fn(Face, Face) -> FaceSlots) -> Sides {
    let mut sides = Sides::default();
    for face in Face::ALL {
        let reached = slots(front, face);
        sides.input[face as usize] = !reached.inputs.is_empty();
        sides.output[face as usize] = !reached.outputs.is_empty();
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
/// `EnumColor.YELLOW`, que o cabeçalho da aba de avisos usa.
const ENUM_YELLOW: u32 = 0xFF_DD_4F;

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
    ("yellow", ENUM_YELLOW),
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

/// Os sprites que um tanque de infusão desenha (`Chemical.icon`), na ordem em que o data slot os numera. São tiras animadas do atlas
/// de blocos do original (`assets/minecraft/atlases/blocks.json` lista a pasta `mek_chemical`), e o HUD do Horizon as anima como o
/// jogo. Um químico com sprite fora desta lista não desenha o sprite; o teste dos dados confere que os de infusão estão todos aqui.
pub const CHEMICAL_ICONS: [&str; 3] = [
    "mekanism:mek_chemical/infuse_type/base",
    "mekanism:mek_chemical/infuse_type/bio",
    "mekanism:mek_chemical/infuse_type/fungi",
];

/// O valor do data slot para um sprite: o índice em [`CHEMICAL_ICONS`] mais um, e zero sem sprite.
pub fn chemical_icon_slot(icon: Option<&str>) -> u32 {
    icon.and_then(|icon| CHEMICAL_ICONS.iter().position(|known| *known == icon))
        .map_or(0, |index| index as u32 + 1)
}

/// O sprite de um valor de data slot.
pub fn chemical_icon(slot: u32) -> Option<&'static str> {
    CHEMICAL_ICONS.get((slot as usize).checked_sub(1)?).copied()
}

/// `tint` e `color_representation` de um químico: ARGB com o alfa que o arquivo traz, `#aarrggbb`.
fn chemical_tint(json: &Value) -> Option<u32> {
    let text = json.get("tint").and_then(Value::as_str)?;
    u32::from_str_radix(text.strip_prefix('#')?, 16).ok()
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
    // Os pigmentos não têm `icon` nem `tint`: a tela deles ainda não existe.
    Some(Resource {
        id: id.to_owned(),
        kind: kind.to_owned(),
        color,
        icon: json.get("icon").and_then(Value::as_str).map(str::to_owned),
        tint: chemical_tint(json).unwrap_or(machinery_basis_core::resource::NO_TINT),
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
    fn the_right_takes_nothing_and_the_back_only_the_energy_slot() {
        for front in [Face::North, Face::South, Face::East, Face::West] {
            assert!(item_slots(front, front.right_of()).inputs.is_empty());
            assert!(item_slots(front, front.right_of()).outputs.is_empty());
            assert_eq!(item_slots(front, front.opposite()).inputs, [ENERGY_SLOT]);
            assert_eq!(item_slots(front, front.opposite()).outputs, [ENERGY_SLOT]);
            assert!(sides(front, item_slots).accepts_input(front));
        }
    }

    #[test]
    fn a_hopper_below_the_infuser_gets_the_infusion_slot_and_no_other_face_can_be_below() {
        let sides = sides(Face::North, item_slots);
        assert!(sides.gives_output(Face::Down));
        assert_eq!(item_slots(Face::North, Face::Down).outputs, [INFUSION_SLOT]);
        // O back (`ENERGY`) também entrega, mas um hopper só puxa do bloco de cima: ele não está embaixo de nenhuma face lateral.
        assert!(sides.gives_output(Face::South));
        assert!(!sides.gives_output(Face::North));
        assert!(!sides.gives_output(Face::East));
        assert!(!sides.gives_output(Face::Up));
    }

    #[test]
    fn a_hopper_below_an_electric_machine_gets_nothing() {
        let sides = sides(Face::North, electric_item_slots);
        assert!(!sides.gives_output(Face::Down));
        assert!(sides.gives_output(Face::South));
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
        assert_eq!(
            electric_item_slots(front, front.opposite()).inputs,
            [ELECTRIC_ENERGY_SLOT]
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
