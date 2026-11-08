// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::cached_recipe::ItemToItemCache;
use machinery_basis_core::chemical::{ChemicalResource, ConversionCache, can_fill_or_convert_with};
use machinery_basis_core::client_config::{
    Entry as ClientEntry, Loaded as ClientLoaded, Section as ClientSection,
    Setting as ClientSetting, Spec as ClientSpec,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::container_sync::{Mirror, StackValue, Tracked};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::resource::Resource;
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::slot_view::{
    ContainerSlotType, DataType, ItemConfig, SlotOverlay, SlotSpec, SlotType, slot_type,
};
use machinery_basis_core::sound::SoundConfig;
use machinery_basis_core::tile::{
    ChemicalMachineConfig, ElectricMachineConfig, MachineConfig, ProgressMachine, RedstoneControl,
    RedstoneSync, UpgradeConfig, machine_energy_slot_accepts,
};
use machinery_basis_core::tile_trackers::{
    EnergyTrackers, TileTrackers, position_of, progress_machine_trackers, recipe_machine_trackers,
    source, upgrade_window_trackers,
};
use machinery_basis_core::tooltip;
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use machinery_basis_core::units::{energy_fraction, stored_mb};
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::warning::{ErrorMask, WarningTracker};
use machinery_basis_core::window::WindowType;
use serde_json::Value;
use std::rc::Rc;
use std::sync::LazyLock;

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

/// O arquivo de cliente: o do original é `Mekanism/client.toml` (`MekanismConfigHelper.registerConfig`: `Mekanism.MOD_NAME` + `/` + `ClientConfig.getFileName()` +
/// `.toml`). O port o chama `mekanism/client.toml`, com a pasta em minúscula que ele já usa para as posições das janelas (`config/<ns>/client.toml`) e para
/// `local.toml`: um arquivo só, como no original (decisão 12).
pub const CLIENT_CONFIG_FILE: &str = "mekanism/client.toml";
/// A seção `sounds` do `ClientConfig` (`ClientConfig.java:74-81`), com o texto que o `MekanismConfigTranslations` dá a cada comentário. O
/// original traz também `enablePlayer` (os sons do jetpack, da máscara de mergulho, do lança-chamas e do modulador de gravidade, que não
/// estão portados) e as demais seções (acessibilidade, renderização, HUD, QIO e janelas), que entram junto de quem as lê.
pub const CLIENT_CONFIG: ClientSpec = ClientSpec {
    sections: &[ClientSection {
        name: "sounds",
        comment: "Settings for configuring Mekanism's Sounds",
        entries: &[
            ClientEntry {
                key: "baseVolume",
                comment: "Adjust Mekanism sounds' base volume. < 1 is softer, higher is louder.",
                setting: ClientSetting::Float {
                    default: 1.0,
                    min: 0.0,
                    max: 10.0,
                },
            },
            ClientEntry {
                key: "enableMachine",
                comment: "Play sounds for active machines.",
                setting: ClientSetting::Bool { default: true },
            },
        ],
    }],
};

/// `MekanismConfig.client.baseSoundVolume` (`CachedFloatValue`: o `double` do arquivo como `float`) e `enableMachineSounds`.
pub fn sound_config(loaded: &ClientLoaded) -> SoundConfig {
    let defaults = SoundConfig::default();
    SoundConfig {
        enabled: loaded
            .bool(&CLIENT_CONFIG, "sounds", "enableMachine")
            .unwrap_or(defaults.enabled),
        base_volume: loaded
            .float(&CLIENT_CONFIG, "sounds", "baseVolume")
            .map_or(defaults.base_volume, |volume| volume as f32),
    }
}

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

/// Os elementos que a tela de uma máquina tem fora da imagem de 176 por 166 e que `GuiMekanism.findElementOver` acha (os que a tela do port
/// desenha; a segurança, a redstone, a configuração de itens e de transporte e o livro de receitas do original ainda não existem aqui):
/// - `GuiEnergyTab` (`addGuiElements` de `GuiElectricMachine` e de `GuiMetallurgicInfuser`): -26, 137, 26 por 26, sempre ativa e visível.
/// - `GuiWarningTab` (`GuiMekanism.addWarningTab`, à esquerda): -26, 109, 26 por 26, visível só com algum aviso (`updateVisibility`).
/// - `GuiXpContainmentTab`: o botão de dentro (`GuiInsetElement.isMouseOver` só olha o botão de 18 por 18, não o suporte de 26), visível só
///   com o upgrade de xp instalado (`hasXpContainment`).
///
/// A aba de upgrades é da janela de upgrades da lib.
pub fn screen_elements(screen: ScreenSpec) -> Vec<machinery_basis_core::screen_elements::Element> {
    use machinery_basis_core::screen_elements::Element;
    vec![
        Element::when(machinery_basis_core::gui::WARNING_TAB_LEFT, move |mirror| {
            screen.view(mirror).tab()
        }),
        Element::when(xp_tab().tab.button(), move |mirror| {
            mirror.boolean((screen.indexes)().xp_containment)
        }),
        Element::always(ENERGY_TAB.rect),
    ]
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

/// O que as verificações de aviso leem de uma máquina: no servidor, o objeto vivo; no cliente, o que o servidor sincronizou.
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

/// O que o cliente tem da máquina: a máscara dos erros rastreados (um `SyncableBoolean` por erro), a lista rastreada que o mod conhece (a
/// mesma da máquina) e o que `addContainerTrackers` manda de redstone (o modo, `redstone` e `redstoneLastTick`).
#[derive(Clone, Copy, Debug)]
pub struct SyncedMachine {
    pub tracked: &'static [RecipeError],
    pub errors: ErrorMask,
    pub redstone: RedstoneSync,
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

/// Onde cada dado da tela fica na lista do menu (`trackedData`): a posição que o servidor mandou no `PacketUpdateContainer` e o cliente
/// lê do espelho. Sai da própria lista de `addContainerTrackers` (`electric_trackers`, `infuser_trackers`), a mesma que o servidor
/// sincroniza, então não há outro lugar onde combinar posições. A capacidade muda com os upgrades, e a tela não a sabe de cabeça; os
/// `ticksRequired` também (`TileEntityProgressMachine` sincroniza os dois).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenIndexes {
    pub control_type: usize,
    pub powered: usize,
    pub was_powered: usize,
    /// O tanque de químico, nas máquinas que têm um.
    pub chemical: Option<usize>,
    pub capacity: usize,
    pub energy_per_tick: usize,
    pub energy: usize,
    /// `trackedErrors[i]`, na ordem de `TRACKED_ERROR_TYPES`.
    pub errors: Vec<usize>,
    /// `hasXpContainment`: a aba de xp só aparece com ele.
    pub xp_containment: usize,
    /// `operatingTicks` e `ticksRequired`: com upgrades de velocidade a seta de progresso se enche por ele, não pelo de base.
    pub progress: usize,
    pub ticks_required: usize,
}

impl ScreenIndexes {
    /// As posições na lista `list` de uma máquina com `tracked_errors` erros rastreados.
    pub fn of(list: &[Tracked], tracked_errors: usize) -> Self {
        let at = |source| position_of(list, source);
        Self {
            control_type: at(source::CONTROL_TYPE),
            powered: at(source::POWERED),
            was_powered: at(source::WAS_POWERED),
            chemical: list
                .iter()
                .any(|tracked| tracked.source == source::CHEMICAL_TANK)
                .then(|| at(source::CHEMICAL_TANK)),
            capacity: at(source::MAX_ENERGY),
            energy_per_tick: at(source::ENERGY_PER_TICK),
            energy: at(source::ENERGY),
            errors: (0..tracked_errors)
                .map(|index| at(source::TRACKED_ERROR + index as u16))
                .collect(),
            xp_containment: at(source::HAS_XP_CONTAINMENT),
            progress: at(source::OPERATING_TICKS),
            ticks_required: at(source::TICKS_REQUIRED),
        }
    }

    /// Onde a janela de upgrades põe os dados dela (`upgradeTicks`, depois o mapa): logo depois dos do construtor.
    pub fn window(&self, base: usize) -> [usize; 2] {
        [base, base + 1]
    }
}

static ELECTRIC_INDEXES: LazyLock<ScreenIndexes> =
    LazyLock::new(|| ScreenIndexes::of(&electric_trackers(), ELECTRIC_TRACKED_ERRORS.len()));
static INFUSER_INDEXES: LazyLock<ScreenIndexes> =
    LazyLock::new(|| ScreenIndexes::of(&infuser_trackers(), INFUSER_TRACKED_ERRORS.len()));

fn electric_indexes() -> &'static ScreenIndexes {
    &ELECTRIC_INDEXES
}

fn infuser_indexes() -> &'static ScreenIndexes {
    &INFUSER_INDEXES
}

/// O que o cliente lê do espelho do menu de uma máquina.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenData {
    pub energy: u32,
    pub capacity: u32,
    pub progress: i32,
    /// `ticksRequired`, zero antes de o servidor mandar.
    pub ticks_required: i32,
    /// O tanque de químico (o id do químico, vazio sem nenhum, e a quantidade); o `tint` e o sprite saem do registro, pelo id.
    pub chemical: StackValue,
    pub errors: ErrorMask,
    pub redstone: RedstoneSync,
    /// `hasXpContainment()` no servidor: o upgrade de xp está instalado (ou o tipo saiu do registro).
    pub xp_containment: bool,
    /// `getEnergyPerTick()` no servidor, zero antes de o servidor mandar.
    pub energy_per_tick: u32,
}

impl ScreenData {
    /// O que o cliente lê. Antes dos primeiros valores do servidor o espelho está vazio, e o que falta vale o padrão do dado.
    pub fn read(spec: &ScreenSpec, mirror: &Mirror) -> Self {
        let indexes = (spec.indexes)();
        let unsigned = |value: i64| u32::try_from(value.clamp(0, i64::from(u32::MAX))).unwrap_or(0);
        let flags: Vec<bool> = indexes
            .errors
            .iter()
            .map(|index| mirror.boolean(*index))
            .collect();
        Self {
            energy: unsigned(mirror.long(indexes.energy)),
            capacity: unsigned(mirror.long(indexes.capacity)),
            progress: mirror.int(indexes.progress),
            ticks_required: mirror.int(indexes.ticks_required),
            chemical: indexes
                .chemical
                .map(|index| mirror.stack(index))
                .unwrap_or_default(),
            errors: ErrorMask::from_flags(&flags),
            redstone: RedstoneSync {
                control_type: RedstoneControl::from_ordinal(
                    mirror.enum_ordinal(indexes.control_type),
                ),
                powered: mirror.boolean(indexes.powered),
                was_powered: mirror.boolean(indexes.was_powered),
            },
            xp_containment: mirror.boolean(indexes.xp_containment),
            energy_per_tick: unsigned(i64::from(mirror.int(indexes.energy_per_tick))),
        }
    }
}

/// O que a tela de um tipo de máquina sabe: os avisos (os erros que a máquina rastreia e os registros dos elementos), os data
/// slots e os slots de item com a configuração de lados que decide a moldura deles.
#[derive(Clone, Copy, Debug)]
pub struct ScreenSpec {
    pub tracked: &'static [RecipeError],
    pub warnings: &'static ScreenWarnings,
    /// Os dados que o menu sincroniza (`addContainerTrackers`) e onde cada um da tela fica nessa lista.
    pub trackers: fn() -> Vec<Tracked>,
    pub indexes: fn() -> &'static ScreenIndexes,
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
    trackers: electric_trackers,
    indexes: electric_indexes,
    item_config: &ELECTRIC_ITEM_CONFIG,
    slots: &ELECTRIC_SLOTS_SPEC,
};

pub const INFUSER_SCREEN: ScreenSpec = ScreenSpec {
    tracked: &INFUSER_TRACKED_ERRORS,
    warnings: &INFUSER_WARNINGS,
    trackers: infuser_trackers,
    indexes: infuser_indexes,
    item_config: &INFUSER_ITEM_CONFIG,
    slots: &INFUSER_SLOTS_SPEC,
};

/// O que a tela desenha de aviso, decidido do que o cliente tem do menu sincronizado: a aba (`hasWarning`), as linhas dela
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
    pub fn synced(&self, mirror: &Mirror) -> SyncedMachine {
        let read = ScreenData::read(self, mirror);
        SyncedMachine {
            tracked: self.tracked,
            errors: read.errors,
            redstone: read.redstone,
        }
    }

    pub fn view(&self, mirror: &Mirror) -> WarningView {
        let source = self.synced(mirror);
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

/// Os índices de receita que as regras de slot de uma máquina consultam.
#[derive(Clone)]
pub enum Indexes {
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
    pub fn energy(&self) -> &Rc<EnergyConversionCache> {
        match self {
            Self::Electric { energy, .. } | Self::Infuser { energy, .. } => energy,
        }
    }
}

/// O que o tanque e o slot de entrada guardam, para as regras de inserção que olham um pelo outro.
pub struct Held {
    pub chemical: Option<ChemicalResource>,
    pub input: Option<ItemResource>,
}

/// Onde ficam os slots que aceitam item de fora, na lista de itens da máquina.
pub struct SlotIndexes {
    pub input: usize,
    pub energy: usize,
    /// O slot que enche o tanque de químico, nas máquinas que têm tanque.
    pub infusion: Option<usize>,
}

/// Se o slot `slot` da máquina aceita o item: o slot de entrada (`InputInventorySlot`, com `containsRecipe` na Chamber e
/// `containsRecipeAB` mais `containsRecipeA` no Infuser), o de infusão (`ChemicalInventorySlot.canFillOrConvert`) e o de energia
/// (`EnergyInventorySlot.canFillOrConvert`). A saída não aceita nada de fora. É o `isValidForInsertion(item, MANUAL)` que o clique do
/// cursor (`InventoryContainerSlot.mayPlace`) pergunta, e o que o shift-click e o hopper vizinho perguntam também.
pub fn slot_accepts(
    indexes: &Indexes,
    slots: &SlotIndexes,
    slot: usize,
    item: &ItemResource,
    held: &Held,
) -> bool {
    if slot == slots.energy {
        return machine_energy_slot_accepts(indexes.energy(), item);
    }
    match indexes {
        Indexes::Electric { recipes, .. } => slot == slots.input && recipes.contains_input(item),
        Indexes::Infuser {
            recipes,
            conversions,
            ..
        } => {
            if slot == slots.input {
                recipes.contains_input_a(item) && recipes.contains_input_ab(item, &held.chemical)
            } else if Some(slot) == slots.infusion {
                // `simulateCanInsert(tank, conversion, MANUAL)`: o `isValidForInsertion` do tanque do Infuser é o `containsRecipeB`
                // e o `containsRecipeBA` com o item que está na entrada.
                can_fill_or_convert_with(conversions, item, held.chemical.as_ref(), |chemical| {
                    recipes.contains_input_b(chemical)
                        && recipes.contains_input_ba(&held.input, chemical)
                })
            } else {
                false
            }
        }
    }
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

/// Os dados que o menu de uma máquina elétrica (Enrichment Chamber, Crusher, Energized Smelter) sincroniza, na ordem de
/// `addContainerTrackers`: a base do tile (`TileEntityMekanism`), os erros rastreados e `hasXpContainment`
/// (`TileEntityRecipeMachine`), e `operatingTicks` e `ticksRequired` (`TileEntityProgressMachine`). `TileEntityElectricMachine` e as
/// classes das máquinas não acrescentam nada. O rastro `*_trackers` do Java confere a ordem, a classe e o valor de cada um.
pub fn electric_trackers() -> Vec<Tracked> {
    machine_trackers(0, ELECTRIC_TRACKED_ERRORS.len())
}

/// O mesmo do Metallurgic Infuser: o tanque de infusão entra depois da redstone, e há um erro rastreado a mais.
pub fn infuser_trackers() -> Vec<Tracked> {
    machine_trackers(1, INFUSER_TRACKED_ERRORS.len())
}

fn machine_trackers(chemical_tanks: usize, tracked_errors: usize) -> Vec<Tracked> {
    let mut list = TileTrackers {
        security: true,
        redstone: true,
        chemical_tanks,
        energy: Some(EnergyTrackers {
            last_energy: true,
            adjustable: true,
        }),
        ..TileTrackers::default()
    }
    .trackers();
    list.extend(recipe_machine_trackers(tracked_errors));
    list.extend(progress_machine_trackers());
    list
}

/// O que a janela de upgrades acrescenta (`TileComponentUpgrade.getSpecificSyncableData`), na chave `UPGRADE_WINDOW_KEY`.
pub fn upgrade_window_data() -> [Tracked; 2] {
    upgrade_window_trackers()
}

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

/// Os sprites que um tanque de infusão desenha (`Chemical.icon`). São tiras animadas do atlas de blocos do original
/// (`assets/minecraft/atlases/blocks.json` lista a pasta `mek_chemical`), e o HUD do Horizon as anima como o jogo. Um químico com sprite
/// fora desta lista não desenha o sprite; o teste dos dados confere que os de infusão estão todos aqui.
pub const CHEMICAL_ICONS: [&str; 3] = [
    "mekanism:mek_chemical/infuse_type/base",
    "mekanism:mek_chemical/infuse_type/bio",
    "mekanism:mek_chemical/infuse_type/fungi",
];

/// O sprite que a tela desenha para o `icon` de um químico: ele, se é um dos que a lista traz.
pub fn chemical_icon(icon: Option<&str>) -> Option<&'static str> {
    CHEMICAL_ICONS
        .iter()
        .copied()
        .find(|known| Some(*known) == icon)
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

/// `MekanismLang.LOG_FORMAT`, `MEKANISM` e `RECIPE_WARNING`: as chaves de tradução da mensagem de receita incompleta
/// (`IncompleteRecipeScanner.RECIPE_WARNING`, `IncompleteRecipeScanner.java:14`).
pub const LOG_FORMAT_KEY: &str = "constants.mekanism.log_format";
pub const MOD_NAME_KEY: &str = "constants.mekanism.mod_name";
pub const RECIPE_WARNING_KEY: &str = "constants.mekanism.recipe_warning";
/// `EnumColor.RED.getTextColor()`: a cor da mensagem inteira (`translateColored(EnumColor.RED, ..)`).
pub const RECIPE_WARNING_COLOR: u32 = 0xFF_38_3C;

/// O componente da mensagem como o `ComponentSerialization` do original o escreve: `LOG_FORMAT.translateColored(EnumColor.RED, MEKANISM,
/// RECIPE_WARNING.translate())` é um `translatable` de `log_format` com os dois outros `translatable` de argumento, e a cor no estilo do
/// pai (os argumentos a herdam).
pub fn recipe_warning_json() -> serde_json::Value {
    serde_json::json!({
        "color": format!("#{RECIPE_WARNING_COLOR:06X}"),
        "translate": LOG_FORMAT_KEY,
        "with": [{ "translate": MOD_NAME_KEY }, { "translate": RECIPE_WARNING_KEY }],
    })
}

/// As receitas que ficam incompletas hoje só porque o port ainda não tem o conteúdo delas (decisão 13): um ingrediente de tag cujos itens o port não traz
/// (estanho, urânio, chumbo, fluorita, bronze, os pós sujos e os tijolos de bio fuel). O original, com o conteúdo inteiro, não tem nenhuma, e por isso a
/// mensagem do `IncompleteRecipeScanner` só sai quando sobra receita incompleta fora desta lista (veja `unexplained_incomplete`). A lista encolhe com o
/// conteúdo, e `tests/incomplete_recipes_gaps.rs` confere que ela é exatamente o que os arquivos do conteúdo dão: um conteúdo novo que a deixa velha falha o teste.
pub const CONTENT_GAPS: [&str; 40] = [
    "mekanism:chemical_conversion/bio/from_bio_fuel_block",
    "mekanism:chemical_conversion/carbon/from_charcoal_block",
    "mekanism:chemical_conversion/tin/from_dust",
    "mekanism:chemical_conversion/tin/from_enriched",
    "mekanism:enriching/charcoal_dust",
    "mekanism:enriching/enriched/tin",
    "mekanism:processing/bronze/dust/from_ingot",
    "mekanism:processing/copper/dirty_dust/from_clump",
    "mekanism:processing/copper/dust/from_dirty_dust",
    "mekanism:processing/fluorite/from_dust",
    "mekanism:processing/fluorite/from_ore",
    "mekanism:processing/fluorite/to_dust",
    "mekanism:processing/gold/dirty_dust/from_clump",
    "mekanism:processing/gold/dust/from_dirty_dust",
    "mekanism:processing/iron/dirty_dust/from_clump",
    "mekanism:processing/iron/dust/from_dirty_dust",
    "mekanism:processing/lead/dirty_dust/from_clump",
    "mekanism:processing/lead/dust/from_dirty_dust",
    "mekanism:processing/lead/dust/from_ingot",
    "mekanism:processing/lead/dust/from_ore",
    "mekanism:processing/lead/dust/from_raw_block",
    "mekanism:processing/lead/dust/from_raw_ore",
    "mekanism:processing/osmium/dirty_dust/from_clump",
    "mekanism:processing/osmium/dust/from_dirty_dust",
    "mekanism:processing/osmium/dust/from_raw_block",
    "mekanism:processing/refined_glowstone/ingot_to_dust",
    "mekanism:processing/refined_obsidian/dust/from_ingot",
    "mekanism:processing/tin/dirty_dust/from_clump",
    "mekanism:processing/tin/dust/from_dirty_dust",
    "mekanism:processing/tin/dust/from_ingot",
    "mekanism:processing/tin/dust/from_ore",
    "mekanism:processing/tin/dust/from_raw_block",
    "mekanism:processing/tin/dust/from_raw_ore",
    "mekanism:processing/uranium/dirty_dust/from_clump",
    "mekanism:processing/uranium/dust/from_dirty_dust",
    "mekanism:processing/uranium/dust/from_ingot",
    "mekanism:processing/uranium/dust/from_ore",
    "mekanism:processing/uranium/dust/from_raw_block",
    "mekanism:processing/uranium/dust/from_raw_ore",
    "mekanism:processing/uranium/yellow_cake_uranium",
];

/// Se alguma receita incompleta (os ids que a varredura achou) não é uma das que só faltam por conteúdo do port. É o `foundIncompleteRecipes` do port.
pub fn unexplained_incomplete(incomplete: &[String]) -> bool {
    incomplete
        .iter()
        .any(|id| !CONTENT_GAPS.contains(&id.as_str()))
}

/// Quem recebe a mensagem de receita incompleta (`IncompleteRecipeScanner`). O original guarda um `boolean` estático
/// (`foundIncompleteRecipes`) que o `ServerStartedEvent` e o `OnDatapackSyncEvent` sem jogador (o `/reload`) refazem com a varredura
/// (`MekanismRecipeType.checkIncompleteRecipes`), e o `OnDatapackSyncEvent` de um jogador que entra só lê.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IncompleteScanner {
    found: bool,
}

impl IncompleteScanner {
    pub const fn new() -> Self {
        Self { found: false }
    }

    /// `IncompleteRecipeScanner.foundIncompleteRecipes`.
    pub fn found(&self) -> bool {
        self.found
    }

    /// `ServerStartedEvent`: refaz a varredura. "Em teoria não há jogadores a esta altura", então ninguém recebe a mensagem.
    pub fn server_started(&mut self, found: bool) {
        self.found = found;
    }

    /// `OnDatapackSyncEvent` sem jogador (o `/reload`): refaz a varredura e devolve se a mensagem vai a todos os jogadores.
    pub fn reloaded(&mut self, found: bool) -> bool {
        self.found = found;
        self.found
    }

    /// `OnDatapackSyncEvent` de um jogador que entra: não varre ("já rodou ao começar ou na última recarga") e devolve se ele recebe a
    /// mensagem.
    pub fn player_joined(&self) -> bool {
        self.found
    }
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
