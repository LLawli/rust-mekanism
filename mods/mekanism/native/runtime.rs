// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::active_fx::{FxParticle, Particle, REDSTONE, face_position};
use machinery_basis_core::cached_recipe::ItemToItemCache;
use machinery_basis_core::chemical::{ChemicalResource, ConversionCache, can_fill_or_convert_with};
use machinery_basis_core::chemical_tank::GasMode;
use machinery_basis_core::client_config::{
    Entry as ClientEntry, Loaded as ClientLoaded, Section as ClientSection,
    Setting as ClientSetting, Spec as ClientSpec,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::container_sync::{Mirror, StackValue, Tracked};
use machinery_basis_core::containers::{Access, AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::energy_cube::{self, EnergyCubeConfig, ItemEnergy, ItemEnergySpec};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::handler_slot::ContainerEditMode;
use machinery_basis_core::item_instance::InstanceItems;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::resource::Resource;
use machinery_basis_core::settings::{self, ConfigValue, MachineSettings};
use machinery_basis_core::side_config::{
    AttachedSideConfig, LightConfigInfo, RelativeSide, TileComponentConfig, TransmissionType,
};
use machinery_basis_core::sides::{Face, FaceSlots};
use machinery_basis_core::slot_view::{
    ContainerSlotType, DataType, ItemConfig, SlotOverlay, SlotSpec, SlotType, slot_type,
};
use machinery_basis_core::sound::SoundConfig;
use machinery_basis_core::tile::{
    ChemicalMachineConfig, ElectricMachineConfig, MachineConfig, ProgressMachine, RedstoneControl,
    RedstoneSync, SideConfigSpec, UpgradeConfig, machine_energy_slot_accepts,
};
use machinery_basis_core::tile_trackers::{
    EnergyTrackers, TileTrackers, position_of, progress_machine_trackers, recipe_machine_trackers,
    source, upgrade_window_trackers,
};
use machinery_basis_core::tooltip;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::host_resource::ResourceSnapshot;
use machinery_basis_core::transmission::thermodynamic_conductor::TierValues;
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use machinery_basis_core::units::{energy_fraction, stored_mb};
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::voxel_edges::union_edges_in;
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

/// `CableTier`: o id de cada cabo e quanto ele guarda de padrão (`TierConfig`, `basicCapacity` e as outras três, as capacidades que o arquivo `tiers.toml`
/// muda).
pub const UNIVERSAL_CABLES: [(&str, i32); 4] = [
    ("mekanism:basic_universal_cable", 8_000),
    ("mekanism:advanced_universal_cable", 128_000),
    ("mekanism:elite_universal_cable", 1_024_000),
    ("mekanism:ultimate_universal_cable", 8_192_000),
];

/// `BASE_TICKS_REQUIRED`, `MAX_INFUSE` e os padrões de `UsageConfig` (`enrichmentChamber`, `crusher`, `energizedSmelter`, `metallurgicInfuser`).
pub const TICKS_REQUIRED: u32 = 200;
pub const TANK_CAPACITY: u64 = 1_000;
pub const INFUSER_USAGE: i32 = 5;
pub const ENRICHMENT_USAGE: i32 = 5;
pub const CRUSHER_USAGE: i32 = 8;
pub const SMELTER_USAGE: i32 = 5;

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

/// O arquivo de cliente: `Mekanism/client.toml` (`MekanismConfigHelper.registerConfig`: `Mekanism.MOD_NAME` + `/` + `ClientConfig.getFileName()` + `.toml`), na
/// pasta do original, com o M maiúsculo (decisão 14). É o arquivo onde as janelas do Mekanism e do Generators guardam a posição (`window.<nome>`), como
/// no original.
pub const CLIENT_CONFIG_FILE: &str = "Mekanism/client.toml";
/// A pasta do `local.toml` (`LocalConfig.getFileName()`, `holidays`): a mesma do `client.toml`.
pub const LOCAL_CONFIG_NAMESPACE: &str = "Mekanism";
/// A seção `sounds` do `ClientConfig` (`ClientConfig.java:74-81`), com o texto que o `MekanismConfigTranslations` dá a cada comentário. O
/// original traz também `enablePlayer` (os sons do jetpack, da máscara de mergulho, do lança-chamas e do modulador de gravidade, que não
/// estão portados) e as demais seções (acessibilidade, renderização, HUD e QIO), que entram junto de quem as lê. As posições das janelas (`window`) são de
/// `WindowPositions`.
pub const CLIENT_CONFIG: ClientSpec = ClientSpec {
    sections: &[
        // `ClientConfig.java:61-71`: `accessibility`. `opaqueTransmitters` (`.gameRestart()`) só diz se o conteúdo do transmissor se desenha (`RenderTransmitterBase.shouldRenderTransmitter`):
        // o modelo dos blocos é o mesmo nos dois valores (as partes `_Opaque` do `.mtl` não têm face no OBJ).
        ClientSection {
            name: "accessibility",
            comment: "Settings for configuring Accessibility settings provided by Mekanism. Note: Some settings such as HUD scale, may be located throughout the rest of this config.",
            entries: &[
                ClientEntry {
                    key: "whiteRadialText",
                    comment: "Tries to force all text rendered as part of radial menus to be white instead of the color of the slice's mode.",
                    setting: ClientSetting::Bool { default: false },
                },
                ClientEntry {
                    key: "opaqueTransmitters",
                    comment: "If true, renders sides for Universal Cables, Mechanical Pipes, Pressurized Tubes, Logistical Transporters, and Thermodynamic Conductors instead of rendering their contents. If transmitters are causing you fps issues, this should hopefully help.",
                    setting: ClientSetting::Bool { default: false },
                },
                ClientEntry {
                    key: "allowModeScroll",
                    comment: "Allows changing the mode of held items by holding sneak and using the scroll wheel.",
                    setting: ClientSetting::Bool { default: true },
                },
            ],
        },
        ClientSection {
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
        },
        // `ClientConfig.java:84-93`: `rendering` > `particle` > `machineEffects`. A seção pai vem antes (um comentário e nenhuma chave: o `berRange` e as
        // demais de `rendering`, e as outras de `particle` (formação de multibloco, radiação, atração magnética, área de ferramenta), entram junto de quem as lê).
        ClientSection {
            name: "rendering",
            comment: "Settings for adjusting how Mekanism renders certain blocks and how many particles Mekanism adds.",
            // `ClientConfig.java:84-85`: `energyColor` (`define("energyColor", 0x3CFE9A)`, sem faixa) vem antes de `berRange`, que o port ainda não lê.
            entries: &[ClientEntry {
                key: "energyColor",
                comment: "The color of energy when displayed as the durability bar on items.",
                setting: ClientSetting::IntWhere {
                    default: ENERGY_COLOR,
                    valid: |_, _| true,
                },
            }],
        },
        ClientSection {
            name: "rendering.particle",
            comment: "Settings for configuring Mekanism's Particles",
            entries: &[ClientEntry {
                key: "machineEffects",
                comment: "Show particles when machines active.",
                setting: ClientSetting::Bool { default: true },
            }],
        },
        // `ClientConfig.java:105-111`: `hud`. O resto da seção (opacidade, cores, tremor, bússola, legendas e a barra de energia) é do HUD do MekaSuit.
        ClientSection {
            name: "hud",
            comment: "Settings for configuring Mekanism's HUD",
            entries: &[
                ClientEntry {
                    key: "enabled",
                    comment: "Enable a HUD that displays information about equipped Mekanism items, and displays additional information when wearing a MekaSuit Helmet.",
                    setting: ClientSetting::Bool { default: true },
                },
                ClientEntry {
                    key: "scale",
                    comment: "Scale of the text displayed on the HUD.",
                    setting: ClientSetting::Float {
                        default: 0.6,
                        min: 0.25,
                        max: 1.0,
                    },
                },
                ClientEntry {
                    key: "reverse",
                    comment: "Reverses the HUD's text alignment and compass rendering to the right side of the screen, and moves the MekaSuit module rendering to the left side. Warning: This may cause subtitles to overlap parts of the HUD such as the compass.",
                    setting: ClientSetting::Bool { default: false },
                },
            ],
        },
    ],
};

/// O arquivo de cliente que o Horizon escreve quando ele não existe: a seção `sounds` e as posições das janelas que o port abre (`window.upgrade`), no
/// texto do NightConfig. O original escreve as seis seções e as onze janelas; o port, só o que lê (decisão 12, revista pela 14).
pub fn client_default_text() -> String {
    use machinery_basis_core::window::WindowPositions;
    let windows = WindowPositions::defaults_for(&[
        SIDE_CONFIG_WINDOW,
        TRANSPORTER_CONFIG_WINDOW,
        UPGRADE_WINDOW,
    ])
    .to_toml(&WINDOW_TYPES);
    machinery_basis_core::client_config::replace_sections(
        &CLIENT_CONFIG.default_text(),
        "window",
        &windows,
    )
}

/// `MekanismConfig.client.machineEffects`: as máquinas acesas soltam partículas (`BlockTile.animateTick`).
/// `ClientConfig.energyColor`: o padrão, `0x3CFE9A`.
pub const ENERGY_COLOR: i32 = 0x3C_FE_9A;

/// `MekanismConfig.client.energyColor`: a cor da barra de energia dos itens.
pub fn energy_color(loaded: &ClientLoaded) -> i32 {
    loaded
        .int(&CLIENT_CONFIG, "rendering", "energyColor")
        .unwrap_or(ENERGY_COLOR)
}

/// `MekanismConfig.client.opaqueTransmitters` (`accessibility`, `false` por padrão, só vale ao reiniciar o jogo): se o conteúdo dos transmissores deixa de se desenhar
/// (`RenderTransmitterBase.shouldRenderTransmitter`). Não troca nenhum modelo.
pub fn opaque_transmitters(loaded: &ClientLoaded) -> bool {
    loaded
        .bool(&CLIENT_CONFIG, "accessibility", "opaqueTransmitters")
        .unwrap_or(false)
}

pub fn machine_effects(loaded: &ClientLoaded) -> bool {
    loaded
        .bool(&CLIENT_CONFIG, "rendering.particle", "machineEffects")
        .unwrap_or(true)
}

/// `Machine.java:23`: `SMOKE` e `DustParticleOptions.REDSTONE`, ambas em `(rand * 0,6 - 0,3, rand * 6/16, 0,52)` relativo ao centro do bloco, na face
/// da frente (o `facing` gira a posição).
pub static MACHINE_FX: [FxParticle; 2] = [
    FxParticle {
        particle: Particle::Smoke,
        position: |next| face_position(0.52, next),
    },
    FxParticle {
        particle: REDSTONE,
        position: |next| face_position(0.52, next),
    },
];

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

/// Os arquivos de servidor do `MekanismConfig`: `GeneralConfig`, `UsageConfig` e `TierConfig`, todos `Type.SYNCED` (`getConfigType`), registrados por
/// `MekanismConfigHelper.registerConfig` com o nome `Mekanism/<arquivo>.toml`. O NeoForge lê o arquivo de `config/` quando o servidor vai começar e deixa
/// um do mundo (`syncedconfig/`) trocá-lo; o Horizon tem o tipo `Server` para isso (decisão 15). Cada especificação traz só as chaves das mecânicas portadas.
pub const GENERAL_CONFIG_FILE: &str = "Mekanism/general.toml";
pub const USAGE_CONFIG_FILE: &str = "Mekanism/machine-usage.toml";
pub const TIERS_CONFIG_FILE: &str = "Mekanism/tiers.toml";

/// `GeneralConfig`: `blockDeactivationDelay` (`:126-127`, `defineInRange(.., 3 * TICKS_PER_SECOND, 0, TICKS_PER_MINUTE)`), `transmitterAlloyUpgrade` (`:130-131`, `define(.., true)`)
/// e `maxUpgradeMultiplier` (`:133-134`, `defineInRange(.., 10, 1, Integer.MAX_VALUE)`), as três de fora de qualquer seção. O resto do arquivo entra com a fase que o lê.
pub const GENERAL_CONFIG: ClientSpec = ClientSpec {
    sections: &[
        ClientSection {
            name: "",
            comment: "",
            entries: &[
                ClientEntry {
                    key: "blockDeactivationDelay",
                    comment: "How many ticks must pass until a block's active state is synced with the client when the block stops being active. This prevents rapid state changes that can cause lag. Note: Mekanism's blocks always sync immediately when they go from inactive to active; this only controls the time it takes to go from active to inactive.",
                    setting: ClientSetting::Int {
                        default: 60,
                        min: 0,
                        max: 1_200,
                    },
                },
                // `GeneralConfig.java:130-131`, depois do `aestheticWorldDamage` (que o port ainda não tem).
                ClientEntry {
                    key: "transmitterAlloyUpgrade",
                    comment: "Allow upgrading Universal Cables, Mechanical Pipes, Pressurized Tubes, Logistical Transporters, and Thermodynamic Conductors by right clicking them with the next tier of alloy.",
                    setting: ClientSetting::Bool { default: true },
                },
                ClientEntry {
                    key: "maxUpgradeMultiplier",
                    comment: "Base factor for working out machine performance with upgrades - UpgradeModifier * (UpgradesInstalled/UpgradesPossible).",
                    setting: ClientSetting::Int {
                        default: 10,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `GeneralConfig.java:262-267`: a seção `nutritional_paste` (`MekanismConfigTranslations.GENERAL_PASTE`), depois da `security`, que o port ainda não tem.
        ClientSection {
            name: "nutritional_paste",
            comment: "Settings for configuring Nutritional Paste",
            entries: &[
                ClientEntry {
                    key: "saturation",
                    comment: "Saturation level of Nutritional Paste when eaten.",
                    setting: ClientSetting::Float {
                        default: 0.8,
                        min: 0.0,
                        max: 100.0,
                    },
                },
                ClientEntry {
                    key: "mbPerFood",
                    comment: "How much mB of Nutritional Paste equates to one 'half-food'.",
                    setting: ClientSetting::Int {
                        default: 50,
                        min: 1,
                        max: PASTE_MB_PER_FOOD_MAX,
                    },
                },
            ],
        },
    ],
};

/// `Integer.MAX_VALUE / FoodConstants.MAX_FOOD` (20): o limite de `nutritionalPasteMBPerFood`.
const PASTE_MB_PER_FOOD_MAX: i32 = i32::MAX / 20;
/// `FluidType.BUCKET_VOLUME`.
const BUCKET_VOLUME: i32 = 1000;

/// O que `PasteBucketConsumption.onConsume` dá a quem bebe o balde: `FluidType.BUCKET_VOLUME / nutritionalPasteMBPerFood` de fome (divisão inteira) e
/// `nutritionalPasteSaturation` de saturação, de `GeneralConfig`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PasteSettings {
    pub nutrition: i32,
    pub saturation: f32,
}

impl PasteSettings {
    pub const DEFAULT: Self = Self {
        nutrition: BUCKET_VOLUME / 50,
        saturation: 0.8,
    };
}

/// `nutritionalPasteMBPerFood` e `nutritionalPasteSaturation` de um arquivo lido.
pub fn paste_settings(loaded: &ClientLoaded) -> PasteSettings {
    let per_food = loaded
        .int(&GENERAL_CONFIG, "nutritional_paste", "mbPerFood")
        .unwrap_or(50);
    PasteSettings {
        nutrition: BUCKET_VOLUME / per_food,
        saturation: loaded
            .float(&GENERAL_CONFIG, "nutritional_paste", "saturation")
            .map_or(PasteSettings::DEFAULT.saturation, |v| v as f32),
    }
}

/// `GeneralConfig.blockDeactivationDelay`, `transmitterAlloyUpgrade` e `maxUpgradeMultiplier` de um arquivo lido.
pub fn machine_settings(loaded: &ClientLoaded) -> MachineSettings {
    let defaults = MachineSettings::DEFAULT;
    MachineSettings {
        block_deactivation_delay: loaded
            .int(&GENERAL_CONFIG, "", "blockDeactivationDelay")
            .unwrap_or(defaults.block_deactivation_delay),
        max_upgrade_multiplier: loaded
            .int(&GENERAL_CONFIG, "", "maxUpgradeMultiplier")
            .unwrap_or(defaults.max_upgrade_multiplier),
        transmitter_alloy_upgrade: loaded
            .bool(&GENERAL_CONFIG, "", "transmitterAlloyUpgrade")
            .unwrap_or(defaults.transmitter_alloy_upgrade),
    }
}

/// `UsageConfig` (`:51-66`): a energia por operação de cada máquina portada, `defineInRange(.., padrão, 0, Integer.MAX_VALUE)`, com o texto de
/// `TranslationPreset.ENERGY_USAGE` (`Energy per operation of: <máquina>`). A chave do Metallurgic Infuser vem depois da do Crusher, na ordem do original.
pub const USAGE_CONFIG: ClientSpec = ClientSpec {
    sections: &[ClientSection {
        name: "",
        comment: "",
        entries: &[
            ClientEntry {
                key: "enrichmentChamber",
                comment: "Energy per operation of: Enrichment Chamber",
                setting: ClientSetting::Int {
                    default: ENRICHMENT_USAGE,
                    min: 0,
                    max: i32::MAX,
                },
            },
            ClientEntry {
                key: "crusher",
                comment: "Energy per operation of: Crusher",
                setting: ClientSetting::Int {
                    default: CRUSHER_USAGE,
                    min: 0,
                    max: i32::MAX,
                },
            },
            ClientEntry {
                key: "metallurgicInfuser",
                comment: "Energy per operation of: Metallurgic Infuser",
                setting: ClientSetting::Int {
                    default: INFUSER_USAGE,
                    min: 0,
                    max: i32::MAX,
                },
            },
            ClientEntry {
                key: "energizedSmelter",
                comment: "Energy per operation of: Energized Smelter",
                setting: ClientSetting::Int {
                    default: SMELTER_USAGE,
                    min: 0,
                    max: i32::MAX,
                },
            },
        ],
    }],
};

/// `MekanismConfig.usage.*` das quatro máquinas portadas. O tile guarda o valor quando nasce (`MachineEnergyContainer.input` lê `AttributeEnergy.getUsage()` no
/// construtor), então uma mudança do arquivo vale para as máquinas novas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub enrichment_chamber: i32,
    pub crusher: i32,
    pub metallurgic_infuser: i32,
    pub energized_smelter: i32,
}

impl Usage {
    pub const DEFAULT: Self = Self {
        enrichment_chamber: ENRICHMENT_USAGE,
        crusher: CRUSHER_USAGE,
        metallurgic_infuser: INFUSER_USAGE,
        energized_smelter: SMELTER_USAGE,
    };

    pub fn of(loaded: &ClientLoaded) -> Self {
        let read = |key: &str, default: i32| loaded.int(&USAGE_CONFIG, "", key).unwrap_or(default);
        Self {
            enrichment_chamber: read("enrichmentChamber", ENRICHMENT_USAGE),
            crusher: read("crusher", CRUSHER_USAGE),
            metallurgic_infuser: read("metallurgicInfuser", INFUSER_USAGE),
            energized_smelter: read("energizedSmelter", SMELTER_USAGE),
        }
    }
}

/// `TierConfig.addUniversalCableCategory` (`:121-130`): as capacidades dos cabos, `defineInRange(<tier>Capacity, CableTier.getBaseCapacity(), 1,
/// Long.MAX_VALUE)`, nas seções `transmitters` e `transmitters.energy`, com o texto de `TierTranslations.create(CableTier)`.
pub const TIERS_CONFIG: ClientSpec = ClientSpec {
    sections: &[
        // `TierConfig.addEnergyCubeCategory` (`:38-49`): a capacidade e a taxa de saída de cada tier (`defineInRange(<tier>Capacity, getBaseCapacity(), 1, Long.MAX_VALUE)` e
        // `<tier>Output`, `1..Integer.MAX_VALUE`), com o texto de `TierTranslations.create(EnergyCubeTier)`. A seção vem antes de `transmitters`, como no original.
        ClientSection {
            name: "energy_cubes",
            comment: "Settings for configuring Energy Cubes",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic energy cubes.",
                    setting: ClientSetting::Long {
                        default: 4_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicOutput",
                    comment: "Output rate of Basic energy cubes.",
                    setting: ClientSetting::Int {
                        default: 4_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced energy cubes.",
                    setting: ClientSetting::Long {
                        default: 16_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedOutput",
                    comment: "Output rate of Advanced energy cubes.",
                    setting: ClientSetting::Int {
                        default: 16_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite energy cubes.",
                    setting: ClientSetting::Long {
                        default: 64_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteOutput",
                    comment: "Output rate of Elite energy cubes.",
                    setting: ClientSetting::Int {
                        default: 64_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate energy cubes.",
                    setting: ClientSetting::Long {
                        default: 256_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateOutput",
                    comment: "Output rate of Ultimate energy cubes.",
                    setting: ClientSetting::Int {
                        default: 256_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeCapacity",
                    comment: "Capacity of Creative energy cubes.",
                    setting: ClientSetting::Long {
                        default: i64::MAX,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeOutput",
                    comment: "Output rate of Creative energy cubes.",
                    setting: ClientSetting::Int {
                        default: i32::MAX,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addFluidTankCategory`: a capacidade e a taxa de cada tier, como a dos cubos.
        ClientSection {
            name: "fluid_tanks",
            comment: "Settings for configuring Fluid Tanks",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic fluid tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 32_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicOutput",
                    comment: "Output rate in mB of Basic fluid tanks.",
                    setting: ClientSetting::Int {
                        default: 1_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced fluid tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 64_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedOutput",
                    comment: "Output rate in mB of Advanced fluid tanks.",
                    setting: ClientSetting::Int {
                        default: 4_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite fluid tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 128_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteOutput",
                    comment: "Output rate in mB of Elite fluid tanks.",
                    setting: ClientSetting::Int {
                        default: 16_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate fluid tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 256_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateOutput",
                    comment: "Output rate in mB of Ultimate fluid tanks.",
                    setting: ClientSetting::Int {
                        default: 64_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeCapacity",
                    comment: "Capacity of Creative fluid tanks in mB.",
                    setting: ClientSetting::Long {
                        default: i64::MAX,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeOutput",
                    comment: "Output rate in mB of Creative fluid tanks.",
                    setting: ClientSetting::Int {
                        default: i32::MAX,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addChemicalTankCategory`: a capacidade e a taxa de cada tier, como a dos cubos.
        ClientSection {
            name: "chemical_tanks",
            comment: "Settings for configuring Chemical Tanks",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic chemical tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 64_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicOutput",
                    comment: "Output rate in mB of Basic chemical tanks.",
                    setting: ClientSetting::Int {
                        default: 1_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced chemical tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 256_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedOutput",
                    comment: "Output rate in mB of Advanced chemical tanks.",
                    setting: ClientSetting::Int {
                        default: 16_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite chemical tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 1_024_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteOutput",
                    comment: "Output rate in mB of Elite chemical tanks.",
                    setting: ClientSetting::Int {
                        default: 128_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate chemical tanks in mB.",
                    setting: ClientSetting::Long {
                        default: 8_192_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateOutput",
                    comment: "Output rate in mB of Ultimate chemical tanks.",
                    setting: ClientSetting::Int {
                        default: 512_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeCapacity",
                    comment: "Capacity of Creative chemical tanks in mB.",
                    setting: ClientSetting::Long {
                        default: i64::MAX,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "creativeOutput",
                    comment: "Output rate in mB of Creative chemical tanks.",
                    setting: ClientSetting::Int {
                        default: i32::MAX,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addInductionCategory` (`:92-108`): a capacidade de cada célula (`<tier>Capacity`, `1..Long.MAX_VALUE`); a saída dos provedores, que vem depois, espera o provedor.
        ClientSection {
            name: "induction",
            comment: "Settings for configuring Induction Cells and Providers",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Maximum energy Basic induction cells can store.",
                    setting: ClientSetting::Long {
                        default: 8_000_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Maximum energy Advanced induction cells can store.",
                    setting: ClientSetting::Long {
                        default: 64_000_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Maximum energy Elite induction cells can store.",
                    setting: ClientSetting::Long {
                        default: 512_000_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Maximum energy Ultimate induction cells can store.",
                    setting: ClientSetting::Long {
                        default: 4_000_000_000_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "transmitters",
            comment: "Settings for configuring Transmitters",
            entries: &[],
        },
        ClientSection {
            name: "transmitters.energy",
            comment: "Settings for configuring Universal Cables",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic universal cables in FE.",
                    setting: ClientSetting::Long {
                        default: 8_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced universal cables in FE.",
                    setting: ClientSetting::Long {
                        default: 128_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite universal cables in FE.",
                    setting: ClientSetting::Long {
                        default: 1_024_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate universal cables in FE.",
                    setting: ClientSetting::Long {
                        default: 8_192_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addMechanicalPipeCategory`: a capacidade (`<tier>Capacity`, `1..Long.MAX_VALUE`) e a vazão (`<tier>PullAmount`, `1..Integer.MAX_VALUE`) de cada cano mecânico, com
        // o texto de `TierTranslations.create(PipeTier)`.
        ClientSection {
            name: "transmitters.fluid",
            comment: "Settings for configuring Mechanical Pipes",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic mechanical pipes in mB.",
                    setting: ClientSetting::Long {
                        default: 2_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicPullAmount",
                    comment: "Pump rate of Basic mechanical pipes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 250,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced mechanical pipes in mB.",
                    setting: ClientSetting::Long {
                        default: 8_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedPullAmount",
                    comment: "Pump rate of Advanced mechanical pipes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 1_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite mechanical pipes in mB.",
                    setting: ClientSetting::Long {
                        default: 32_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "elitePullAmount",
                    comment: "Pump rate of Elite mechanical pipes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 8_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate mechanical pipes in mB.",
                    setting: ClientSetting::Long {
                        default: 128_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimatePullAmount",
                    comment: "Pump rate of Ultimate mechanical pipes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 32_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addPressurizedTubesCategory`: a capacidade (`<tier>Capacity`, `1..Long.MAX_VALUE`) e a vazão (`<tier>PullAmount`, `1..Integer.MAX_VALUE`) de cada tubo pressurizado, com
        // o texto de `TierTranslations.create(TubeTier)`.
        ClientSection {
            name: "transmitters.chemical",
            comment: "Settings for configuring Pressurized Tubes",
            entries: &[
                ClientEntry {
                    key: "basicCapacity",
                    comment: "Capacity of Basic pressurized tubes in mB.",
                    setting: ClientSetting::Long {
                        default: 4_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicPullAmount",
                    comment: "Pump rate of Basic pressurized tubes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 750,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedCapacity",
                    comment: "Capacity of Advanced pressurized tubes in mB.",
                    setting: ClientSetting::Long {
                        default: 16_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedPullAmount",
                    comment: "Pump rate of Advanced pressurized tubes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 2_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteCapacity",
                    comment: "Capacity of Elite pressurized tubes in mB.",
                    setting: ClientSetting::Long {
                        default: 256_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "elitePullAmount",
                    comment: "Pump rate of Elite pressurized tubes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 64_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateCapacity",
                    comment: "Capacity of Ultimate pressurized tubes in mB.",
                    setting: ClientSetting::Long {
                        default: 1_024_000,
                        min: 1,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimatePullAmount",
                    comment: "Pump rate of Ultimate pressurized tubes in mB/t.",
                    setting: ClientSetting::Int {
                        default: 256_000,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addLogisticalTransportersCategory`: a quantidade que cada transportador logístico puxa por vez (`<tier>PullAmount`, itens por meio segundo) e a velocidade (`<tier>Speed`, cinco
        // vezes os m/s), de 1 a `Integer.MAX_VALUE`, com o texto de `TierTranslations.create(TransporterTier)`.
        ClientSection {
            name: "transmitters.items",
            comment: "Settings for configuring Logistical Transporters",
            entries: &[
                ClientEntry {
                    key: "basicPullAmount",
                    comment: "Item throughput rate of Basic logistical transporters in items/half second. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 1,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "basicSpeed",
                    comment: "Five times the travel speed in m/s of Basic logistical transporter. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 5,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedPullAmount",
                    comment: "Item throughput rate of Advanced logistical transporters in items/half second. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 16,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedSpeed",
                    comment: "Five times the travel speed in m/s of Advanced logistical transporter. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 10,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "elitePullAmount",
                    comment: "Item throughput rate of Elite logistical transporters in items/half second. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 32,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteSpeed",
                    comment: "Five times the travel speed in m/s of Elite logistical transporter. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 20,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimatePullAmount",
                    comment: "Item throughput rate of Ultimate logistical transporters in items/half second. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 64,
                        min: 1,
                        max: i32::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateSpeed",
                    comment: "Five times the travel speed in m/s of Ultimate logistical transporter. This value assumes a target tick rate of 20 ticks per second.",
                    setting: ClientSetting::Int {
                        default: 50,
                        min: 1,
                        max: i32::MAX,
                    },
                },
            ],
        },
        // `TierConfig.addThermodynamicConductorsCategory`: a condução inversa (`<tier>InverseConduction`, `1..Double.MAX_VALUE`), a capacidade de calor (`<tier>HeatCapacity`, `1..Double.MAX_VALUE`) e a
        // isolação (`<tier>Insulation`, `0..Double.MAX_VALUE`) de cada condutor termodinâmico, com o texto de `TierTranslations.create(ConductorTier)`.
        ClientSection {
            name: "transmitters.heat",
            comment: "Settings for configuring Thermodynamic Conductors",
            entries: &[
                ClientEntry {
                    key: "basicInverseConduction",
                    comment: "Conduction value of Basic thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 5.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicHeatCapacity",
                    comment: "Heat capacity of Basic thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 1.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "basicInsulation",
                    comment: "Insulation value of Basic thermodynamic conductor.",
                    setting: ClientSetting::Float {
                        default: 10.0,
                        min: 0.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedInverseConduction",
                    comment: "Conduction value of Advanced thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 5.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedHeatCapacity",
                    comment: "Heat capacity of Advanced thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 1.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "advancedInsulation",
                    comment: "Insulation value of Advanced thermodynamic conductor.",
                    setting: ClientSetting::Float {
                        default: 400.0,
                        min: 0.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteInverseConduction",
                    comment: "Conduction value of Elite thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 5.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteHeatCapacity",
                    comment: "Heat capacity of Elite thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 1.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "eliteInsulation",
                    comment: "Insulation value of Elite thermodynamic conductor.",
                    setting: ClientSetting::Float {
                        default: 8_000.0,
                        min: 0.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateInverseConduction",
                    comment: "Conduction value of Ultimate thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 5.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateHeatCapacity",
                    comment: "Heat capacity of Ultimate thermodynamic conductors.",
                    setting: ClientSetting::Float {
                        default: 1.0,
                        min: 1.0,
                        max: f64::MAX,
                    },
                },
                ClientEntry {
                    key: "ultimateInsulation",
                    comment: "Insulation value of Ultimate thermodynamic conductor.",
                    setting: ClientSetting::Float {
                        default: 100_000.0,
                        min: 0.0,
                        max: f64::MAX,
                    },
                },
            ],
        },
    ],
};

/// As capacidades dos quatro cabos (`CableTier.getCableCapacity`: o valor do arquivo, ou o padrão sem ele), na ordem de [`UNIVERSAL_CABLES`].
pub fn cable_capacities(loaded: &ClientLoaded) -> [i64; 4] {
    const KEYS: [&str; 4] = [
        "basicCapacity",
        "advancedCapacity",
        "eliteCapacity",
        "ultimateCapacity",
    ];
    let mut out = DEFAULT_CABLE_CAPACITIES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .long(&TIERS_CONFIG, "transmitters.energy", key)
            .unwrap_or(*slot);
    }
    out
}

/// Os padrões de `UNIVERSAL_CABLES` como `long`.
pub const DEFAULT_CABLE_CAPACITIES: [i64; 4] = [8_000, 128_000, 1_024_000, 8_192_000];

/// As ligas (`ItemAlloy`) e o nível do tier que cada uma dá (`IAlloyTier.getBaseTierLevel`: avançado 1, elite 2, supremo 3).
pub const ALLOYS: [(&str, i32); 3] = [
    ("mekanism:alloy_infused", 1),
    ("mekanism:alloy_reinforced", 2),
    ("mekanism:alloy_atomic", 3),
];

/// O nível do tier da liga `item`, ou `None` se o item não é uma liga.
pub fn alloy_level(item: &str) -> Option<i32> {
    ALLOYS
        .iter()
        .find(|(id, _)| *id == item)
        .map(|(_, level)| *level)
}

/// `PipeTier`: o id de cada cano mecânico, na ordem dos tiers.
pub const MECHANICAL_PIPES: [&str; 4] = [
    "mekanism:basic_mechanical_pipe",
    "mekanism:advanced_mechanical_pipe",
    "mekanism:elite_mechanical_pipe",
    "mekanism:ultimate_mechanical_pipe",
];

/// Qual cano mecânico é o bloco, na ordem de [`MECHANICAL_PIPES`].
pub fn pipe_tier(block: &str) -> Option<usize> {
    MECHANICAL_PIPES.iter().position(|id| *id == block)
}

/// Os blocos do transportador logístico, do básico ao supremo (`TransporterTier`).
pub const LOGISTICAL_TRANSPORTERS: [&str; 4] = [
    "mekanism:basic_logistical_transporter",
    "mekanism:advanced_logistical_transporter",
    "mekanism:elite_logistical_transporter",
    "mekanism:ultimate_logistical_transporter",
];
pub const RESTRICTIVE_TRANSPORTER: &str = "mekanism:restrictive_transporter";
pub const DIVERSION_TRANSPORTER: &str = "mekanism:diversion_transporter";

/// O que um bloco de transportador é.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransporterBlock {
    /// `LogisticalTransporter`, com o tier na ordem de [`LOGISTICAL_TRANSPORTERS`].
    Logistical(usize),
    /// `RestrictiveTransporter`.
    Restrictive,
    /// `DiversionTransporter`.
    Diversion,
}

/// Que transportador é o bloco, se é um.
pub fn transporter_block(block: &str) -> Option<TransporterBlock> {
    if let Some(tier) = LOGISTICAL_TRANSPORTERS.iter().position(|id| *id == block) {
        return Some(TransporterBlock::Logistical(tier));
    }
    match block {
        RESTRICTIVE_TRANSPORTER => Some(TransporterBlock::Restrictive),
        DIVERSION_TRANSPORTER => Some(TransporterBlock::Diversion),
        _ => None,
    }
}

/// As capacidades dos quatro canos (`PipeTier.getCapacity`: o valor do arquivo, ou o padrão sem ele), na ordem de [`MECHANICAL_PIPES`].
pub fn pipe_capacities(loaded: &ClientLoaded) -> [i64; 4] {
    const KEYS: [&str; 4] = [
        "basicCapacity",
        "advancedCapacity",
        "eliteCapacity",
        "ultimateCapacity",
    ];
    let mut out = DEFAULT_PIPE_CAPACITIES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .long(&TIERS_CONFIG, "transmitters.fluid", key)
            .unwrap_or(*slot);
    }
    out
}

/// As vazões dos quatro canos (`PipeTier.getTransferRate`: o valor do arquivo, ou o padrão sem ele).
pub fn pipe_rates(loaded: &ClientLoaded) -> [i32; 4] {
    const KEYS: [&str; 4] = [
        "basicPullAmount",
        "advancedPullAmount",
        "elitePullAmount",
        "ultimatePullAmount",
    ];
    let mut out = DEFAULT_PIPE_RATES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .int(&TIERS_CONFIG, "transmitters.fluid", key)
            .unwrap_or(*slot);
    }
    out
}

/// Os padrões de `TransporterTier` (`getBasePullAmount` e `getBaseSpeed`).
pub const DEFAULT_TRANSPORTER_PULL_AMOUNTS: [i32; 4] = [1, 16, 32, 64];
pub const DEFAULT_TRANSPORTER_SPEEDS: [i32; 4] = [5, 10, 20, 50];

/// A quantidade que cada transportador logístico puxa por vez (`TransporterTier.getPullAmount`: o valor do arquivo, ou o padrão sem ele), na ordem de [`LOGISTICAL_TRANSPORTERS`].
pub fn transporter_pull_amounts(loaded: &ClientLoaded) -> [i32; 4] {
    const KEYS: [&str; 4] = [
        "basicPullAmount",
        "advancedPullAmount",
        "elitePullAmount",
        "ultimatePullAmount",
    ];
    let mut out = DEFAULT_TRANSPORTER_PULL_AMOUNTS;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .int(&TIERS_CONFIG, "transmitters.items", key)
            .unwrap_or(*slot);
    }
    out
}

/// A velocidade de cada transportador logístico (`TransporterTier.getSpeed`: o valor do arquivo, ou o padrão sem ele).
pub fn transporter_speeds(loaded: &ClientLoaded) -> [i32; 4] {
    const KEYS: [&str; 4] = ["basicSpeed", "advancedSpeed", "eliteSpeed", "ultimateSpeed"];
    let mut out = DEFAULT_TRANSPORTER_SPEEDS;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .int(&TIERS_CONFIG, "transmitters.items", key)
            .unwrap_or(*slot);
    }
    out
}

/// `ConductorTier`: o id de cada condutor termodinâmico, na ordem dos tiers.
pub const THERMODYNAMIC_CONDUCTORS: [&str; 4] = [
    "mekanism:basic_thermodynamic_conductor",
    "mekanism:advanced_thermodynamic_conductor",
    "mekanism:elite_thermodynamic_conductor",
    "mekanism:ultimate_thermodynamic_conductor",
];

/// Qual condutor termodinâmico é o bloco, na ordem de [`THERMODYNAMIC_CONDUCTORS`].
pub fn conductor_tier(block: &str) -> Option<usize> {
    THERMODYNAMIC_CONDUCTORS.iter().position(|id| *id == block)
}

/// Os valores dos quatro condutores (`ConductorTier.getInverseConduction`, `getHeatCapacity`, `getInverseConductionInsulation`: o valor do arquivo, ou o padrão sem ele), na ordem de
/// [`THERMODYNAMIC_CONDUCTORS`].
pub fn conductor_values(loaded: &ClientLoaded) -> [TierValues; 4] {
    const KEYS: [[&str; 3]; 4] = [
        [
            "basicInverseConduction",
            "basicHeatCapacity",
            "basicInsulation",
        ],
        [
            "advancedInverseConduction",
            "advancedHeatCapacity",
            "advancedInsulation",
        ],
        [
            "eliteInverseConduction",
            "eliteHeatCapacity",
            "eliteInsulation",
        ],
        [
            "ultimateInverseConduction",
            "ultimateHeatCapacity",
            "ultimateInsulation",
        ],
    ];
    let mut out = DEFAULT_CONDUCTOR_VALUES;
    for (slot, keys) in out.iter_mut().zip(KEYS) {
        let read = |key: &str, default: f64| {
            loaded
                .float(&TIERS_CONFIG, "transmitters.heat", key)
                .unwrap_or(default)
        };
        *slot = TierValues {
            inverse_conduction: read(keys[0], slot.inverse_conduction),
            heat_capacity: read(keys[1], slot.heat_capacity),
            insulation: read(keys[2], slot.insulation),
        };
    }
    out
}

/// Os padrões de `ConductorTier` (`baseConduction`, `baseHeatCapacity`, `baseConductionInsulation`).
pub const DEFAULT_CONDUCTOR_VALUES: [TierValues; 4] = [
    TierValues {
        inverse_conduction: 5.0,
        heat_capacity: 1.0,
        insulation: 10.0,
    },
    TierValues {
        inverse_conduction: 5.0,
        heat_capacity: 1.0,
        insulation: 400.0,
    },
    TierValues {
        inverse_conduction: 5.0,
        heat_capacity: 1.0,
        insulation: 8_000.0,
    },
    TierValues {
        inverse_conduction: 5.0,
        heat_capacity: 1.0,
        insulation: 100_000.0,
    },
];

/// Os padrões de `PipeTier` (`getBaseCapacity` e `getBaseTransferRate`).
pub const DEFAULT_PIPE_CAPACITIES: [i64; 4] = [2_000, 8_000, 32_000, 128_000];
pub const DEFAULT_PIPE_RATES: [i32; 4] = [250, 1_000, 8_000, 32_000];

/// `TubeTier`: o id de cada tubo pressurizado, na ordem dos tiers.
pub const PRESSURIZED_TUBES: [&str; 4] = [
    "mekanism:basic_pressurized_tube",
    "mekanism:advanced_pressurized_tube",
    "mekanism:elite_pressurized_tube",
    "mekanism:ultimate_pressurized_tube",
];

/// Qual tubo pressurizado é o bloco, na ordem de [`PRESSURIZED_TUBES`].
pub fn tube_tier(block: &str) -> Option<usize> {
    PRESSURIZED_TUBES.iter().position(|id| *id == block)
}

/// As capacidades dos quatro tubos (`TubeTier.getCapacity`: o valor do arquivo, ou o padrão sem ele), na ordem de [`PRESSURIZED_TUBES`].
pub fn tube_capacities(loaded: &ClientLoaded) -> [i64; 4] {
    const KEYS: [&str; 4] = [
        "basicCapacity",
        "advancedCapacity",
        "eliteCapacity",
        "ultimateCapacity",
    ];
    let mut out = DEFAULT_TUBE_CAPACITIES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .long(&TIERS_CONFIG, "transmitters.chemical", key)
            .unwrap_or(*slot);
    }
    out
}

/// As vazões dos quatro tubos (`TubeTier.getTransferRate`: o valor do arquivo, ou o padrão sem ele).
pub fn tube_rates(loaded: &ClientLoaded) -> [i32; 4] {
    const KEYS: [&str; 4] = [
        "basicPullAmount",
        "advancedPullAmount",
        "elitePullAmount",
        "ultimatePullAmount",
    ];
    let mut out = DEFAULT_TUBE_RATES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .int(&TIERS_CONFIG, "transmitters.chemical", key)
            .unwrap_or(*slot);
    }
    out
}

/// Os padrões de `TubeTier` (`getBaseCapacity` e `getBaseTransferRate`).
pub const DEFAULT_TUBE_CAPACITIES: [i64; 4] = [4_000, 16_000, 256_000, 1_024_000];
pub const DEFAULT_TUBE_RATES: [i32; 4] = [750, 2_000, 64_000, 256_000];

/// O que os arquivos de servidor dizem às mecânicas portadas do Mekanism, de uma vez.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ServerConfig {
    pub usage: Usage,
    pub cables: [i64; 4],
    pub pipes: [i64; 4],
    pub pipe_rates: [i32; 4],
    /// `ConductorTier`: a condução inversa, a capacidade de calor e a isolação de cada condutor termodinâmico.
    pub conductors: [TierValues; 4],
    pub tubes: [i64; 4],
    pub tube_rates: [i32; 4],
    /// `TransporterTier.getPullAmount` e `getSpeed`.
    pub transporter_pulls: [i32; 4],
    pub transporter_speeds: [i32; 4],
    pub cubes: [i64; 5],
    /// `EnergyCubeTier.getTransferRate`, na mesma ordem.
    pub cube_rates: [i32; 5],
    pub cells: [i64; 4],
    /// `FluidTankTier.getCapacity` e `getTransferRate`, na ordem de [`FLUID_TANK_TIERS`].
    pub fluid_tanks: [i64; 5],
    pub fluid_tank_rates: [i32; 5],
    /// `ChemicalTankTier.getCapacity` e `getTransferRate`, na ordem de [`CHEMICAL_TANK_TIERS`].
    pub chemical_tanks: [i64; 5],
    pub chemical_tank_rates: [i32; 5],
    pub gear: Gear,
}

impl ServerConfig {
    pub const DEFAULT: Self = Self {
        usage: Usage::DEFAULT,
        cables: DEFAULT_CABLE_CAPACITIES,
        pipes: DEFAULT_PIPE_CAPACITIES,
        pipe_rates: DEFAULT_PIPE_RATES,
        conductors: DEFAULT_CONDUCTOR_VALUES,
        tubes: DEFAULT_TUBE_CAPACITIES,
        tube_rates: DEFAULT_TUBE_RATES,
        transporter_pulls: DEFAULT_TRANSPORTER_PULL_AMOUNTS,
        transporter_speeds: DEFAULT_TRANSPORTER_SPEEDS,
        cubes: DEFAULT_CUBE_CAPACITIES,
        cube_rates: DEFAULT_CUBE_RATES,
        fluid_tanks: DEFAULT_FLUID_TANK_CAPACITIES,
        fluid_tank_rates: DEFAULT_FLUID_TANK_RATES,
        chemical_tanks: DEFAULT_CHEMICAL_TANK_CAPACITIES,
        chemical_tank_rates: DEFAULT_CHEMICAL_TANK_RATES,
        cells: DEFAULT_CELL_CAPACITIES,
        gear: Gear::DEFAULT,
    };
}

/// `GearConfig` (`gear.toml`): a energia máxima e a taxa de carga de cada item que guarda energia (`definePositive` de `long`, de 0 a `Long.MAX_VALUE`, e `defineInRange(.., 0, Integer.MAX_VALUE)`
/// de `int`), nas seções do original e na ordem dele (`GearConfig.java:117-259`). O Meka-Tool e o MekaSuit levam `baseEnergyCapacity`, a capacidade sem módulos. O resto do arquivo
/// (uso de energia, dano, módulos...) entra com a fase que o lê.
pub const GEAR_CONFIG_FILE: &str = "Mekanism/gear.toml";
pub const GEAR_CONFIG: ClientSpec = ClientSpec {
    sections: &[
        ClientSection {
            name: "atomic_disassembler",
            comment: "Settings for configuring the Atomic Disassembler",
            entries: &[
                ClientEntry {
                    key: "maxEnergy",
                    comment: "Maximum amount of energy the Atomic Disassembler can contain.",
                    setting: ClientSetting::Long {
                        default: 1_000_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Atomic Disassembler can accept per tick.",
                    setting: ClientSetting::Int {
                        default: 5_000,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "electric_bow",
            comment: "Settings for configuring the Electric Bow",
            entries: &[
                ClientEntry {
                    key: "maxEnergy",
                    comment: "Maximum amount of energy the Electric Bow can contain.",
                    setting: ClientSetting::Long {
                        default: 120_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Electric Bow can accept per tick.",
                    setting: ClientSetting::Int {
                        default: 600,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "energy_tablet",
            comment: "Settings for configuring Energy Tablets",
            entries: &[
                ClientEntry {
                    key: "maxEnergy",
                    comment: "Maximum amount of energy the Energy Tablet can contain.",
                    setting: ClientSetting::Long {
                        default: 1_000_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Energy Tablet can accept per tick.",
                    setting: ClientSetting::Int {
                        default: 5_000,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "free_runner",
            comment: "Settings for configuring Free Runners",
            entries: &[
                ClientEntry {
                    key: "maxEnergy",
                    comment: "Maximum amount of energy Free Runners can contain.",
                    setting: ClientSetting::Long {
                        default: 64_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Free Runners can accept per tick.",
                    setting: ClientSetting::Int {
                        default: 320,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "portable_teleporter",
            comment: "Settings for configuring the Portable Teleporter",
            entries: &[
                ClientEntry {
                    key: "maxEnergy",
                    comment: "Maximum amount of energy the Portable Teleporter can contain.",
                    setting: ClientSetting::Long {
                        default: 1_000_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Portable Teleporter can accept per tick.",
                    setting: ClientSetting::Int {
                        default: 5_000,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "mekatool",
            comment: "Settings for configuring the Meka-Tool",
            entries: &[
                ClientEntry {
                    key: "baseEnergyCapacity",
                    comment: "Energy capacity of the Meka-Tool without any installed upgrades. Quadratically scaled by upgrades.",
                    setting: ClientSetting::Long {
                        default: 16_000_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the Meka-Tool can accept per tick. Quadratically scaled by upgrades.",
                    setting: ClientSetting::Int {
                        default: 100_000,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
        ClientSection {
            name: "mekasuit",
            comment: "Settings for configuring the MekaSuit",
            entries: &[
                ClientEntry {
                    key: "baseEnergyCapacity",
                    comment: "Energy capacity of MekaSuit items without any installed upgrades. Quadratically scaled by upgrades.",
                    setting: ClientSetting::Long {
                        default: 16_000_000,
                        min: 0,
                        max: i64::MAX,
                    },
                },
                ClientEntry {
                    key: "chargeRate",
                    comment: "Amount of energy the MekaSuit can accept per tick. Quadratically scaled by upgrades.",
                    setting: ClientSetting::Int {
                        default: 100_000,
                        min: 0,
                        max: i32::MAX,
                    },
                },
            ],
        },
    ],
};

/// A capacidade e a taxa de carga de um item que guarda energia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnergyItem {
    pub capacity: i64,
    pub rate: i32,
}

/// `MekanismConfig.gear`, na parte da energia dos itens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gear {
    pub disassembler: EnergyItem,
    pub bow: EnergyItem,
    pub tablet: EnergyItem,
    pub free_runners: EnergyItem,
    pub teleporter: EnergyItem,
    pub meka_tool: EnergyItem,
    pub meka_suit: EnergyItem,
}

impl Gear {
    pub const DEFAULT: Self = Self {
        disassembler: EnergyItem {
            capacity: 1_000_000,
            rate: 5_000,
        },
        bow: EnergyItem {
            capacity: 120_000,
            rate: 600,
        },
        tablet: EnergyItem {
            capacity: 1_000_000,
            rate: 5_000,
        },
        free_runners: EnergyItem {
            capacity: 64_000,
            rate: 320,
        },
        teleporter: EnergyItem {
            capacity: 1_000_000,
            rate: 5_000,
        },
        meka_tool: EnergyItem {
            capacity: 16_000_000,
            rate: 100_000,
        },
        meka_suit: EnergyItem {
            capacity: 16_000_000,
            rate: 100_000,
        },
    };

    pub fn of(loaded: &ClientLoaded) -> Self {
        let read = |section: &str, max: &str, default: EnergyItem| EnergyItem {
            capacity: loaded
                .long(&GEAR_CONFIG, section, max)
                .unwrap_or(default.capacity),
            rate: loaded
                .int(&GEAR_CONFIG, section, "chargeRate")
                .unwrap_or(default.rate),
        };
        let default = Self::DEFAULT;
        Self {
            disassembler: read("atomic_disassembler", "maxEnergy", default.disassembler),
            bow: read("electric_bow", "maxEnergy", default.bow),
            tablet: read("energy_tablet", "maxEnergy", default.tablet),
            free_runners: read("free_runner", "maxEnergy", default.free_runners),
            teleporter: read("portable_teleporter", "maxEnergy", default.teleporter),
            meka_tool: read("mekatool", "baseEnergyCapacity", default.meka_tool),
            meka_suit: read("mekasuit", "baseEnergyCapacity", default.meka_suit),
        }
    }
}

/// `EnergyCubeTier.getBaseCapacity`, na ordem de [`CUBE_TIERS`] (o criativo é `Long.MAX_VALUE`).
pub const DEFAULT_CUBE_CAPACITIES: [i64; 5] =
    [4_000_000, 16_000_000, 64_000_000, 256_000_000, i64::MAX];
/// `EnergyCubeTier.getBaseTransferRate`, na mesma ordem (o criativo é `Integer.MAX_VALUE`).
pub const DEFAULT_CUBE_RATES: [i32; 5] = [4_000, 16_000, 64_000, 256_000, i32::MAX];
/// Os itens dos cubos de energia, um por tier (`EnergyCubeTier.VALUES`).
pub const CUBE_TIERS: [&str; 5] = [
    "mekanism:basic_energy_cube",
    "mekanism:advanced_energy_cube",
    "mekanism:elite_energy_cube",
    "mekanism:ultimate_energy_cube",
    "mekanism:creative_energy_cube",
];

/// As capacidades dos cinco cubos de energia (`EnergyCubeTier.getCapacity`: o valor do arquivo, ou o padrão sem ele).
pub fn cube_capacities(loaded: &ClientLoaded) -> [i64; 5] {
    const KEYS: [&str; 5] = [
        "basicCapacity",
        "advancedCapacity",
        "eliteCapacity",
        "ultimateCapacity",
        "creativeCapacity",
    ];
    let mut out = DEFAULT_CUBE_CAPACITIES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .long(&TIERS_CONFIG, "energy_cubes", key)
            .unwrap_or(*slot);
    }
    out
}

/// `InductionCellTier.getBaseMaxEnergy`, na ordem de [`CELL_TIERS`].
pub const DEFAULT_CELL_CAPACITIES: [i64; 4] = [
    8_000_000_000,
    64_000_000_000,
    512_000_000_000,
    4_000_000_000_000,
];
/// Os itens das células de indução, um por tier (`InductionCellTier.VALUES`).
pub const CELL_TIERS: [&str; 4] = [
    "mekanism:basic_induction_cell",
    "mekanism:advanced_induction_cell",
    "mekanism:elite_induction_cell",
    "mekanism:ultimate_induction_cell",
];

/// As capacidades das quatro células de indução (`InductionCellTier.getMaxEnergy`: o valor do arquivo, ou o padrão sem ele).
pub fn cell_capacities(loaded: &ClientLoaded) -> [i64; 4] {
    const KEYS: [&str; 4] = [
        "basicCapacity",
        "advancedCapacity",
        "eliteCapacity",
        "ultimateCapacity",
    ];
    let mut out = DEFAULT_CELL_CAPACITIES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .long(&TIERS_CONFIG, "induction", key)
            .unwrap_or(*slot);
    }
    out
}

/// A célula de indução que o item é, na ordem de [`CELL_TIERS`].
pub fn cell_tier(item: &str) -> Option<usize> {
    CELL_TIERS.iter().position(|id| *id == item)
}

/// As taxas de saída dos cinco cubos (`EnergyCubeTier.getTransferRate`: o valor do arquivo, ou o padrão sem ele).
pub fn cube_rates(loaded: &ClientLoaded) -> [i32; 5] {
    const KEYS: [&str; 5] = [
        "basicOutput",
        "advancedOutput",
        "eliteOutput",
        "ultimateOutput",
        "creativeOutput",
    ];
    let mut out = DEFAULT_CUBE_RATES;
    for (slot, key) in out.iter_mut().zip(KEYS) {
        *slot = loaded
            .int(&TIERS_CONFIG, "energy_cubes", key)
            .unwrap_or(*slot);
    }
    out
}

/// `EntityRobit.MAX_ENERGY`.
pub const ROBIT_MAX_ENERGY: i64 = 100_000;
/// O componente que guarda a energia do item (`MekanismDataComponents.ATTACHED_ENERGY`, um `long`).
pub const ENERGY_COMPONENT: &str = "mekanism:energy";
/// `MekanismLang.STORED_ENERGY` e `INFINITE`.
pub const STORED_ENERGY_LANG: &str = "tooltip.mekanism.stored_energy";
pub const INFINITE_LANG: &str = "gui.mekanism.infinite";

/// `EnumColor.BRIGHT_GREEN` e `EnumColor.GRAY`: o rótulo e o argumento da linha `STORED_ENERGY.translateColored(BRIGHT_GREEN, GRAY, ..)`.
pub const STORED_ENERGY_COLOR: u32 = 0x75_FF_89;
pub const STORED_ENERGY_ARG_COLOR: u32 = 0xCF_CF_CF;

/// Se o item tem a barra de durabilidade de energia: `ItemEnergized`, `ItemBlockEnergyCube`, `ItemFreeRunners`, `ItemElectricBow`, `ItemMekaSuitArmor`, `ItemRobit` e os outros que a
/// sobrescrevem (`isBarVisible` -> `ContainerType.ENERGY.isBarVisible`). As máquinas e as células guardam energia sem a barra (o rastro dá `bar_visible` falso para elas).
pub fn shows_energy_bar(item: &str) -> bool {
    gear_energy(item, &Gear::DEFAULT).is_some() || cube_tier(item).is_some()
}

/// `MekanismUtils.calculateUsage`: `max(1, (int) (0.005 * capacity))` com o corte em `int`.
pub fn calculate_usage(capacity: i64) -> i32 {
    machinery_basis_core::item_energy::calculate_usage(capacity)
}

/// Os itens que não são de bloco e guardam energia (`addAttachedContainerCapabilities(ContainerType.ENERGY, ..)` de `MekanismItems`): a capacidade e a taxa pela configuração.
/// O Meka-Tool e o MekaSuit sem módulos (`ModuleEnergyUnit`: o módulo de energia multiplica por 2 elevado ao número instalado, e os módulos são de outra fase). O Configurator não
/// guarda energia no 26.3 (`MekanismItems.CONFIGURATOR` não tem contêiner).
pub fn gear_energy(item: &str, gear: &Gear) -> Option<EnergyItem> {
    Some(match item {
        "mekanism:energy_tablet" => gear.tablet,
        "mekanism:atomic_disassembler" => gear.disassembler,
        "mekanism:electric_bow" => gear.bow,
        "mekanism:portable_teleporter" => gear.teleporter,
        "mekanism:free_runners" | "mekanism:free_runners_armored" => gear.free_runners,
        "mekanism:meka_tool" => gear.meka_tool,
        "mekanism:mekasuit_helmet"
        | "mekanism:mekasuit_bodyarmor"
        | "mekanism:mekasuit_pants"
        | "mekanism:mekasuit_boots" => gear.meka_suit,
        "mekanism:robit" => EnergyItem {
            capacity: ROBIT_MAX_ENERGY,
            rate: calculate_usage(ROBIT_MAX_ENERGY),
        },
        _ => return None,
    })
}

/// Os ids dos itens de equipamento que guardam energia (os que [`gear_energy`] responde).
pub const GEAR_ITEMS: [&str; 12] = [
    "mekanism:energy_tablet",
    "mekanism:atomic_disassembler",
    "mekanism:electric_bow",
    "mekanism:portable_teleporter",
    "mekanism:free_runners",
    "mekanism:free_runners_armored",
    "mekanism:meka_tool",
    "mekanism:mekasuit_helmet",
    "mekanism:mekasuit_bodyarmor",
    "mekanism:mekasuit_pants",
    "mekanism:mekasuit_boots",
    "mekanism:robit",
];

/// A energia da pilha cheia de um item que a aba criativa mostra vazio e cheio (`ContainerType.ENERGY.getFilledVariant`: a capacidade que o item tem agora, pela configuração),
/// ou `None` se a aba não o mostra assim: o equipamento e o Robit e os quatro cubos que não são o criativo (o criativo vai em duas pilhas com a configuração de lados, do tile do cubo).
pub fn filled_variant_energy(item: &str, config: &ServerConfig) -> Option<i64> {
    if GEAR_ITEMS.contains(&item) {
        return gear_energy(item, &config.gear).map(|gear| gear.capacity);
    }
    cube_tier(item)
        .filter(|tier| *tier < 4)
        .map(|tier| config.cubes[tier])
}

/// Os cinco Fluid Tanks, do básico ao criativo (`FluidTankTier`).
pub const FLUID_TANK_TIERS: [&str; 5] = [
    "mekanism:basic_fluid_tank",
    "mekanism:advanced_fluid_tank",
    "mekanism:elite_fluid_tank",
    "mekanism:ultimate_fluid_tank",
    "mekanism:creative_fluid_tank",
];
/// `FluidTankTier.getBaseCapacity` e `getBaseTransferRate` (`FluidTankTier.java:27-31`).
pub const DEFAULT_FLUID_TANK_CAPACITIES: [i64; 5] = [32_000, 64_000, 128_000, 256_000, i64::MAX];
pub const DEFAULT_FLUID_TANK_RATES: [i32; 5] = [1_000, 4_000, 16_000, 64_000, i32::MAX];
/// Os cinco Chemical Tanks, do básico ao criativo (`ChemicalTankTier`).
pub const CHEMICAL_TANK_TIERS: [&str; 5] = [
    "mekanism:basic_chemical_tank",
    "mekanism:advanced_chemical_tank",
    "mekanism:elite_chemical_tank",
    "mekanism:ultimate_chemical_tank",
    "mekanism:creative_chemical_tank",
];
/// `ChemicalTankTier.getBaseCapacity` e `getBaseTransferRate` (`ChemicalTankTier.java:27-31`).
pub const DEFAULT_CHEMICAL_TANK_CAPACITIES: [i64; 5] =
    [64_000, 256_000, 1_024_000, 8_192_000, i64::MAX];
pub const DEFAULT_CHEMICAL_TANK_RATES: [i32; 5] = [1_000, 16_000, 128_000, 512_000, i32::MAX];
/// O tier criativo dos dois tanques.
pub const TANK_CREATIVE: usize = 4;
/// `MekanismDataComponents.EDIT_MODE` e `BUCKET_MODE`.
pub const EDIT_MODE_COMPONENT: &str = "mekanism:edit_mode";
pub const BUCKET_MODE_COMPONENT: &str = "mekanism:bucket_mode";
/// `MekanismDataComponents.DUMP_MODE`.
pub const DUMP_MODE_COMPONENT: &str = "mekanism:dump_mode";
/// `MekanismConfig.general.dumpExcessKeepRatio` (`GeneralConfig.java:173`), o padrão: o port ainda não lê essa chave do arquivo.
pub const DUMP_EXCESS_KEEP_RATIO: f64 = 0.85;
/// `MekanismConfig.general.radiationEnabled` (`GeneralConfig.java:188`), o padrão: com a radiação ligada o atributo `radioactivity` pede validação.
pub const RADIATION_ENABLED: bool = true;
/// A pasta dos mapas de dados dos químicos (`IMekanismDataMapTypes`: `chemical_attribute_fuel`, `_radioactivity`, `_cooled_coolant` e `_heated_coolant`).
pub const CHEMICAL_ATTRIBUTE_FOLDER: &str = "data_maps/mekanism/chemical";
const CHEMICAL_ATTRIBUTE_PREFIX: &str = "chemical_attribute_";
/// `ChemicalRadioactivity`, o único atributo que pede validação.
pub const RADIOACTIVITY_ATTRIBUTE: &str = "radioactivity";

/// Os atributos de cada químico (`IChemicalAttribute`), dos mapas de dados `chemical_attribute_*`: cada arquivo é um tipo de atributo, e as chaves de `values` são os
/// químicos que o têm. Só `radioactivity` pede validação (`ChemicalRadioactivity.needsValidation`, com a radiação ligada). `remove` tira o químico do mapa, e uma chave
/// de tag (`#`) fica de fora: a datagen do original só tem ids.
pub fn chemical_attribute_maps(
    files: &[(String, Value)],
    radiation_enabled: bool,
) -> ChemicalAttributeMap {
    let mut out = ChemicalAttributeMap::new();
    let mut sorted: Vec<&(String, Value)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, json) in sorted {
        let path = id.split_once(':').map_or(id.as_str(), |(_, path)| path);
        let Some(kind) = path.strip_prefix(CHEMICAL_ATTRIBUTE_PREFIX) else {
            continue;
        };
        let needs_validation = kind == RADIOACTIVITY_ATTRIBUTE && radiation_enabled;
        let removed: Vec<&str> = json
            .get("remove")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let Some(values) = json.get("values").and_then(Value::as_object) else {
            continue;
        };
        for chemical in values.keys() {
            if chemical.starts_with('#') || removed.contains(&chemical.as_str()) {
                continue;
            }
            out.entry(chemical.clone())
                .or_default()
                .push((kind.to_owned(), needs_validation));
        }
    }
    out
}

/// O químico e os atributos dele: o tipo e se pede validação.
pub type ChemicalAttributeMap = std::collections::HashMap<String, Vec<(String, bool)>>;

/// Os atributos como a lib os pede (`ChemicalAttributes`), de um mapa já lido.
pub fn chemical_attributes_of(
    map: std::sync::Arc<ChemicalAttributeMap>,
) -> machinery_basis_core::chemical::ChemicalAttributes {
    Rc::new(move |chemical: &ChemicalResource| {
        map.get(&*chemical.id)
            .map(|list| {
                list.iter()
                    .map(|(kind, needs_validation)| {
                        machinery_basis_core::chemical::ChemicalAttribute::new(
                            kind,
                            *needs_validation,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// O que os mapas de dados `chemical_attribute_*` dizem de um químico, com os valores que o tooltip mostra. Os campos estão na ordem de
/// `MekanismDataMapTypes.ATTRIBUTE_TYPES` (`:45-50`): a radioatividade, o combustível, o refrigerante frio e o quente.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChemicalAttributeData {
    /// `ChemicalRadioactivity.radioactivity`, em Sv/h.
    pub radioactivity: Option<f64>,
    /// `ChemicalFuel`: `maxBurnPerTick` e `energy`.
    pub fuel: Option<(i32, i32)>,
    /// `CooledCoolant`: `thermal_enthalpy` e `conductivity` (o padrão é 1).
    pub cooled: Option<(f64, f64)>,
    /// `HeatedCoolant`: `thermal_enthalpy`, `conductivity` (o padrão é 0,4) e `temperature` (o padrão é 100.000 K).
    pub heated: Option<(f64, f64, f64)>,
}

/// `CooledCoolant.CODEC`: a condutividade que falta vale 1.
const COOLED_CONDUCTIVITY: f64 = 1.0;
/// `HeatedCoolant.BASE_COOLING_EFFICIENCY` e `BASE_COOLANT_TEMP`.
const HEATED_CONDUCTIVITY: f64 = 0.4;
const HEATED_TEMPERATURE: f64 = 100_000.0;
/// `MekanismAPITags.Chemicals.WASTE_BARREL_DECAY_BLACKLIST`.
pub const DECAY_BLACKLIST_TAG: &str = "mekanism:waste_barrel_decay_blacklist";
/// `RadiationUnit.SVH`.
const RADIATION_RATE_SYMBOL: &str = "Sv/h";
/// As casas de `TooltipHelper.getRadioactivityDisplayShort` (`UnitDisplayUtils.getRadiationDisplay(radioactivity, SVH, 2)`).
const RADIATION_DECIMALS: i32 = 2;

/// Os valores dos atributos de cada químico, dos mesmos arquivos de [`chemical_attribute_maps`] e com as mesmas regras (`remove`, as chaves de tag de fora).
pub fn chemical_attribute_data(
    files: &[(String, Value)],
) -> std::collections::HashMap<String, ChemicalAttributeData> {
    let mut out: std::collections::HashMap<String, ChemicalAttributeData> =
        std::collections::HashMap::new();
    for (id, json) in files {
        let path = id.split_once(':').map_or(id.as_str(), |(_, path)| path);
        let Some(kind) = path.strip_prefix(CHEMICAL_ATTRIBUTE_PREFIX) else {
            continue;
        };
        let removed: Vec<&str> = json
            .get("remove")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let Some(values) = json.get("values").and_then(Value::as_object) else {
            continue;
        };
        for (chemical, value) in values {
            if chemical.starts_with('#') || removed.contains(&chemical.as_str()) {
                continue;
            }
            let number = |key: &str| value.get(key).and_then(Value::as_f64);
            let whole = |key: &str| {
                value
                    .get(key)
                    .and_then(Value::as_i64)
                    .and_then(|n| i32::try_from(n).ok())
            };
            let entry = out.entry(chemical.clone()).or_default();
            match kind {
                // `ChemicalRadioactivity.CODEC` aceita o objeto e o número sozinho (`Codec.withAlternative`).
                RADIOACTIVITY_ATTRIBUTE => {
                    entry.radioactivity = number("radioactivity").or_else(|| value.as_f64());
                }
                "fuel" => {
                    entry.fuel = whole("maxBurnPerTick").zip(whole("energy"));
                }
                "cooled_coolant" => {
                    entry.cooled = number("thermal_enthalpy").map(|enthalpy| {
                        (
                            enthalpy,
                            number("conductivity").unwrap_or(COOLED_CONDUCTIVITY),
                        )
                    });
                }
                "heated_coolant" => {
                    entry.heated = number("thermal_enthalpy").map(|enthalpy| {
                        (
                            enthalpy,
                            number("conductivity").unwrap_or(HEATED_CONDUCTIVITY),
                            number("temperature").unwrap_or(HEATED_TEMPERATURE),
                        )
                    });
                }
                _ => {}
            }
        }
    }
    out
}

/// O que o cliente tem ao montar as linhas dos atributos.
pub struct AttributeTooltip<'a> {
    /// `IRadiationManager.isRadiationEnabled`.
    pub radiation_enabled: bool,
    /// O químico está em [`DECAY_BLACKLIST_TAG`].
    pub decay_immune: bool,
    /// `MekanismConfig.local.tempUnit` e `compactEnergyDisplay`.
    pub temperature_unit: machinery_basis_core::temperature::TemperatureUnit,
    pub compact_energy: bool,
    pub translate: &'a dyn Fn(&str) -> String,
}

/// `Math.round(double)`: o piso de `x + 0,5`.
fn java_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// `MathUtils.clampToLong(double)`.
fn clamp_to_long(value: f64) -> i64 {
    value as i64
}

/// As linhas que os atributos de um químico acrescentam ao tooltip (`ChemicalStack.getTooltipLines`, `:262-280`, sem a primeira, o nome, e sem a do nome de
/// registro, que é [`chemical_registry_line`]): cada atributo de `ATTRIBUTE_TYPES`, na ordem, escreve as dele por `collectTooltips`. Todas são
/// `translateColored(GRAY, INDIGO, valor)`, menos a do barril (`DECAY_IMMUNE`, `AQUA`).
pub fn chemical_attribute_lines(
    data: &ChemicalAttributeData,
    tip: &AttributeTooltip<'_>,
) -> Vec<Chat> {
    let line = |key: &str, value: Chat| Chat::new(key, vec![value], Some(TIP_GRAY));
    let value = |text: &str| Chat::literal(text, TIP_INDIGO);
    let wrapped = |key: &str, text: &str| Chat::new(key, vec![value(text)], Some(TIP_INDIGO));
    // `UnitDisplayUtils.getEnergyDisplay` (`:42-48`).
    let energy = |amount: i64| {
        let unit = (tip.translate)(ENERGY_UNIT_LANG);
        if tip.compact_energy {
            machinery_basis_core::units::display_base(amount as f64, Some(&unit), 2, true)
        } else {
            format!(
                "{} {unit}",
                machinery_basis_core::units::grouped(amount.max(0) as u64)
            )
        }
    };
    let mut lines = Vec::new();
    // `ChemicalRadioactivity.collectTooltips` (`:55-64`): só com a radiação ligada.
    if let Some(radioactivity) = data.radioactivity.filter(|_| tip.radiation_enabled) {
        let text = machinery_basis_core::units::display_base(
            radioactivity,
            Some(RADIATION_RATE_SYMBOL),
            RADIATION_DECIMALS,
            true,
        );
        lines.push(line("chemical.mekanism.attribute.radiation", value(&text)));
        if tip.decay_immune {
            lines.push(Chat::new(
                "tooltip.mekanism.decay_immune",
                Vec::new(),
                Some(TIP_AQUA),
            ));
        }
    }
    // `ChemicalFuel.collectTooltips` (`:54-61`).
    if let Some((max_burn, energy_per_tick)) = data.fuel {
        lines.push(line(
            "chemical.mekanism.attribute.fuel.max_burn",
            wrapped(
                PER_TICK_LANG,
                &machinery_basis_core::units::grouped(max_burn.max(0) as u64),
            ),
        ));
        lines.push(line(
            "chemical.mekanism.attribute.fuel.energy_density",
            wrapped(PER_MB_LANG, &energy(i64::from(energy_per_tick))),
        ));
        // `MathUtils.multiplyClamped(int, int)`.
        let total = (i64::from(max_burn) * i64::from(energy_per_tick))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        lines.push(line(
            "chemical.mekanism.attribute.fuel.energy_max_total",
            wrapped(PER_TICK_LANG, &energy(total)),
        ));
    }
    // `IChemicalCoolant.collectTooltips` (`:38-43`), e a temperatura do quente (`HeatedCoolant.java:61-64`).
    let mut coolant = |enthalpy: f64, conductivity: f64| {
        lines.push(line(
            "chemical.mekanism.attribute.coolant.efficiency",
            wrapped(
                "generic.mekanism.percent",
                &java_round(conductivity * 100.0).to_string(),
            ),
        ));
        lines.push(line(
            "chemical.mekanism.attribute.coolant.heat_capacity",
            wrapped(PER_MB_LANG, &energy(clamp_to_long(enthalpy))),
        ));
    };
    if let Some((enthalpy, conductivity)) = data.cooled {
        coolant(enthalpy, conductivity);
    }
    if let Some((enthalpy, conductivity, temperature)) = data.heated {
        coolant(enthalpy, conductivity);
        let text = machinery_basis_core::temperature::local_display(
            temperature,
            machinery_basis_core::temperature::TemperatureUnit::Kelvin,
            tip.temperature_unit,
            true,
        );
        lines.push(line(
            "chemical.mekanism.attribute.coolant.temperature",
            value(&text),
        ));
    }
    lines
}

/// `MekanismLang.GENERIC_PER_TICK` e `GENERIC_PER_MB`.
const PER_TICK_LANG: &str = "generic.mekanism.per_tick";
const PER_MB_LANG: &str = "generic.mekanism.per_mb";

/// A última linha de `ChemicalStack.getTooltipLines` com os tooltips avançados ligados (`:275-278`): o nome de registro, em `DARK_GRAY` do vanilla
/// (`ChatFormatting.DARK_GRAY`).
pub fn chemical_registry_line(id: &str) -> Chat {
    Chat::literal(id, VANILLA_DARK_GRAY)
}

/// `ChatFormatting.DARK_GRAY`.
const VANILLA_DARK_GRAY: u32 = 0x55_55_55;

/// O Chemical Tank como um tubo pressurizado vizinho o vê por um lado (`TileEntityMekanism.getCapability` com a capability de químico): `None` onde a configuração de
/// lados desliga a capability, e senão o contêiner, o que ele aceita de fora (`ChemicalAttributeValidator`) e se o lado deixa entrar e sair. O criativo aceita e entrega
/// sem mudar.
pub fn chemical_tank_view(
    tile: &machinery_basis_core::chemical_tank::ChemicalTankTile,
    face: Face,
) -> Option<ResourceSnapshot<ChemicalResource>> {
    use machinery_basis_core::handler_slot::ResourceContainer;
    let (can_insert, can_extract) = tile.chemical_face(face)?;
    let tank = tile.tank().clone();
    Some(ResourceSnapshot {
        contents: tank.inner().contents(),
        capacity: tank.inner().capacity_for(None),
        accepts: {
            let tank = tank.clone();
            Rc::new(move |chemical: &ChemicalResource| {
                tank.is_valid_for_insertion(chemical, AutomationType::External)
            })
        },
        can_insert,
        can_extract,
        creative: tank.is_creative(),
        increment: 1,
    })
}

/// O que cada lado do Chemical Tank deixa fazer (`None` sem capability, e senão se entra e se sai), na ordem de `Face::ALL`.
pub fn chemical_tank_faces(
    tile: &machinery_basis_core::chemical_tank::ChemicalTankTile,
) -> [Option<(bool, bool)>; 6] {
    Face::ALL.map(|face| tile.chemical_face(face))
}

/// `ItemBlockChemicalTank.SIDE_CONFIG`, o padrão do item dos Chemical Tanks: a frente sai nos itens e no químico, e o químico ejeta.
pub fn chemical_tank_default() -> AttachedSideConfig {
    AttachedSideConfig::new(&[
        (TransmissionType::Item, light_front_out(false)),
        (TransmissionType::Chemical, light_front_out(true)),
    ])
}

/// `MekanismConfig.general.blockDeactivationDelay` de agora: o atraso da luz de um Fluid Tank que esvazia.
pub fn block_deactivation_delay_now() -> i32 {
    settings::block_deactivation_delay()
}

/// O Fluid Tank que o bloco ou o item é, na ordem de [`FLUID_TANK_TIERS`].
pub fn fluid_tank_tier(id: &str) -> Option<usize> {
    FLUID_TANK_TIERS.iter().position(|tank| *tank == id)
}

/// O Chemical Tank que o bloco ou o item é, na ordem de [`CHEMICAL_TANK_TIERS`].
pub fn chemical_tank_tier(id: &str) -> Option<usize> {
    CHEMICAL_TANK_TIERS.iter().position(|tank| *tank == id)
}

/// A capacidade e a taxa dos cinco tiers de uma seção de tanques do `tiers.toml` (`fluid_tanks` ou `chemical_tanks`): o valor do arquivo, ou o padrão sem ele.
pub fn tank_tiers(
    loaded: &ClientLoaded,
    section: &str,
    capacities: [i64; 5],
    rates: [i32; 5],
) -> ([i64; 5], [i32; 5]) {
    const TIERS: [&str; 5] = ["basic", "advanced", "elite", "ultimate", "creative"];
    let (mut capacities, mut rates) = (capacities, rates);
    for (index, tier) in TIERS.iter().enumerate() {
        if let Some(value) = loaded.long(&TIERS_CONFIG, section, &format!("{tier}Capacity")) {
            capacities[index] = value;
        }
        if let Some(value) = loaded.int(&TIERS_CONFIG, section, &format!("{tier}Output")) {
            rates[index] = value;
        }
    }
    (capacities, rates)
}

// ---------------------------------------------------------------------------------------------------------------------------
// As telas dos tanques (`GuiFluidTank` e `GuiChemicalTank`).

/// `MekanismContainerTypes.FLUID_TANK` e `CHEMICAL_TANK`: `armorSideBar()` sem argumentos, que é `armorSideBar(-20, 67, 0)` (`ContainerTypeDeferredRegister.java:145-147`).
pub fn tank_armor_bar_declaration() -> machinery_basis_core::armor_bar::Declaration {
    machinery_basis_core::armor_bar::Declaration {
        x: -20,
        y: 67,
        offhand_offset: 0,
    }
}

/// `GuiSideHolder.armorHolder`: `create(gui, -26, 62, 98, true, true, SpecialColors.TAB_ARMOR_SLOTS)`, o suporte da armadura à esquerda (a cor é a branca do registro).
pub const TANK_ARMOR_HOLDER: machinery_basis_core::gui::Rect = [-26.0, 62.0, 26.0, 98.0];
pub const TANK_ARMOR_HOLDER_SPRITE: &str = "mekanism:gui/sprites/holder_left";

/// Os dois slots do Fluid Tank, na ordem de `getInitialInventory` (`TileEntityFluidTank.java:125-128`): o de entrada (`FluidInventorySlot.input`, que nasce `EXTRA`, com
/// `SlotOverlay.INPUT`) em (146, 19) e o de saída (`OutputInventorySlot.at`, `OUTPUT`, com `SlotOverlay.OUTPUT`) em (146, 51). O tanque não é `ISideConfiguration`: a moldura
/// é a normal.
pub const FLUID_TANK_SLOTS_SPEC: [SlotSpec; 2] = [
    SlotSpec {
        item: [146.0, 19.0],
        container: ContainerSlotType::Extra,
        overlay: Some(SlotOverlay::Input),
    },
    SlotSpec {
        item: [146.0, 51.0],
        container: ContainerSlotType::Output,
        overlay: Some(SlotOverlay::Output),
    },
];

/// `GuiFluidGauge(() -> tile.fluidTank, tile::getFluidTanks, GaugeType.WIDE, this, 48, 18)`: o medidor de 64 por 48 mais a moldura.
pub const FLUID_TANK_GAUGE: [f32; 4] = [48.0, 18.0, 66.0, 50.0];

/// `GuiContainerEditModeTab`: `super(BOTH, gui, tile, gui.getImageWidth(), 138, 26, 18, false)` e `SpecialColors.TAB_CONTAINER_EDIT_MODE`.
pub const EDIT_MODE_TAB_Y: i32 = 138;
pub const EDIT_MODE_TAB_HEIGHT: i32 = 26;
pub const EDIT_MODE_TAB_TINT: u32 = 0xFF_36_6B_D0;

pub fn edit_mode_tab() -> machinery_basis_core::side_tab::InsetTab {
    machinery_basis_core::side_tab::InsetTab::right(
        SCREEN_IMAGE_WIDTH,
        EDIT_MODE_TAB_Y,
        EDIT_MODE_TAB_HEIGHT,
    )
}

/// `GuiContainerEditModeTab.getOverlay`: o sprite do modo de agora (`container_edit_mode/both`, `fill` e `empty`).
pub fn edit_mode_icon(mode: ContainerEditMode) -> &'static str {
    match mode {
        ContainerEditMode::Both => "mekanism:gui/sprites/container_edit_mode/both",
        ContainerEditMode::Fill => "mekanism:gui/sprites/container_edit_mode/fill",
        ContainerEditMode::Empty => "mekanism:gui/sprites/container_edit_mode/empty",
    }
}

/// `ContainerEditMode.getTextComponent` (`MekanismLang.FLUID_CONTAINER_BOTH`, `FILL` e `EMPTY`): o tooltip da aba.
pub fn edit_mode_lang(mode: ContainerEditMode) -> &'static str {
    match mode {
        ContainerEditMode::Both => "fluid_container.mekanism.both",
        ContainerEditMode::Fill => "fluid_container.mekanism.fill",
        ContainerEditMode::Empty => "fluid_container.mekanism.empty",
    }
}

/// Os botões de menu das telas dos tanques: `GuiInteraction.NEXT_MODE` e `PREVIOUS_MODE` (a aba do modo de edição do Fluid Tank) e `GAS_MODE_BUTTON` (o botão do modo de
/// descarte do Chemical Tank). Os ids são do port e não se repetem nos das abas de segurança e de redstone (100 a 103).
pub const TANK_NEXT_MODE_BUTTON: i32 = 110;
pub const TANK_PREVIOUS_MODE_BUTTON: i32 = 111;
pub const TANK_GAS_MODE_BUTTON: i32 = 112;

/// `GuiContainerEditModeTab.onClick`: o botão esquerdo manda `NEXT_MODE` e o direito `PREVIOUS_MODE`.
pub fn edit_mode_click(right: bool) -> i32 {
    if right {
        TANK_PREVIOUS_MODE_BUTTON
    } else {
        TANK_NEXT_MODE_BUTTON
    }
}

/// `TileEntityFluidTank.nextMode` e `previousMode` pelo botão que chegou: `Some(modo novo)`, ou `None` se o botão não é da aba.
pub fn edit_mode_button(mode: ContainerEditMode, button: i32) -> Option<ContainerEditMode> {
    match button {
        TANK_NEXT_MODE_BUTTON => Some(mode.next()),
        TANK_PREVIOUS_MODE_BUTTON => Some(mode.previous()),
        _ => None,
    }
}

/// O dado próprio do menu de cada tanque: `SyncableEnum` do `editMode` (`TileEntityFluidTank.java:300`) e do `dumping` (`TileEntityChemicalTank.java:207`).
pub const SOURCE_TANK_MODE: u16 = source::MOD;

/// `TileEntityFluidTank.addContainerTrackers`: o que `TileEntityMekanism` sincroniza de um tile com segurança, sem redstone (`without(AttributeRedstone.class)`) e com um
/// tanque de fluido, e depois o modo de edição.
pub fn fluid_tank_trackers() -> Vec<Tracked> {
    let mut list = TileTrackers {
        security: true,
        fluid_tanks: 1,
        ..TileTrackers::default()
    }
    .trackers();
    list.push(Tracked::new(
        SOURCE_TANK_MODE,
        machinery_basis_core::container_sync::Kind::Enum { default: 0 },
    ));
    list
}

/// `TileEntityChemicalTank.addContainerTrackers`: a segurança, a redstone, o tanque de químico e depois o modo de descarte.
pub fn chemical_tank_trackers() -> Vec<Tracked> {
    let mut list = TileTrackers {
        security: true,
        redstone: true,
        chemical_tanks: 1,
        ..TileTrackers::default()
    }
    .trackers();
    list.push(Tracked::new(
        SOURCE_TANK_MODE,
        machinery_basis_core::container_sync::Kind::Enum { default: 0 },
    ));
    list
}

/// O que o cliente lê do espelho do menu do Fluid Tank.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluidTankScreenData {
    pub fluid: StackValue,
    pub mode: ContainerEditMode,
}

impl FluidTankScreenData {
    pub fn read(mirror: &Mirror) -> Self {
        let list = fluid_tank_trackers();
        // `ContainerEditMode.BY_ID` com `OutOfBoundsStrategy.ZERO`: um ordinal fora da lista é `BOTH`.
        let ordinal = mirror.enum_ordinal(position_of(&list, SOURCE_TANK_MODE));
        Self {
            fluid: mirror.stack(position_of(&list, source::FLUID_TANK)),
            mode: usize::try_from(ordinal)
                .ok()
                .filter(|ordinal| *ordinal < ContainerEditMode::VALUES.len())
                .map_or(ContainerEditMode::Both, ContainerEditMode::from_ordinal),
        }
    }
}

/// A segurança que o cliente lê do menu do Fluid Tank (`GuiSecurityTab`).
pub fn fluid_tank_security(mirror: &Mirror) -> machinery_basis_core::security::SecurityView {
    let list = fluid_tank_trackers();
    machinery_basis_core::security::SecurityView::read(
        mirror,
        position_of(&list, source::SECURITY_FREQUENCY),
        position_of(&list, source::SECURITY_MODE),
    )
}

/// O que o cliente lê do espelho do menu do Chemical Tank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemicalTankScreenData {
    pub chemical: StackValue,
    pub dumping: GasMode,
}

impl Default for ChemicalTankScreenData {
    fn default() -> Self {
        Self {
            chemical: StackValue::default(),
            dumping: GasMode::Idle,
        }
    }
}

impl ChemicalTankScreenData {
    pub fn read(mirror: &Mirror) -> Self {
        let list = chemical_tank_trackers();
        // `GasMode.BY_ID` com `OutOfBoundsStrategy.ZERO`: um ordinal fora da lista é `IDLE`.
        let ordinal = mirror.enum_ordinal(position_of(&list, SOURCE_TANK_MODE));
        Self {
            chemical: mirror.stack(position_of(&list, source::CHEMICAL_TANK)),
            dumping: usize::try_from(ordinal)
                .ok()
                .filter(|ordinal| *ordinal < GasMode::VALUES.len())
                .map_or(GasMode::Idle, GasMode::from_ordinal),
        }
    }
}

pub fn chemical_tank_security(mirror: &Mirror) -> machinery_basis_core::security::SecurityView {
    let list = chemical_tank_trackers();
    machinery_basis_core::security::SecurityView::read(
        mirror,
        position_of(&list, source::SECURITY_FREQUENCY),
        position_of(&list, source::SECURITY_MODE),
    )
}

/// O controle de redstone que o cliente lê do menu do Chemical Tank (`GuiRedstoneControlTab`).
pub fn chemical_tank_redstone(mirror: &Mirror) -> RedstoneSync {
    let list = chemical_tank_trackers();
    machinery_basis_core::redstone_tab::read(
        mirror,
        position_of(&list, source::CONTROL_TYPE),
        position_of(&list, source::POWERED),
        position_of(&list, source::WAS_POWERED),
    )
}

/// Os dois slots do Chemical Tank, na ordem de `getInitialInventory` (`TileEntityChemicalTank.java:104-109`): o de drenar (`ChemicalInventorySlot.drain`, posto como `OUTPUT`,
/// com `SlotOverlay.PLUS`) em (16, 16) e o de encher (`fill`, posto como `INPUT`, com `SlotOverlay.MINUS`) em (16, 48). A moldura vem do papel que a configuração de itens dá
/// ao slot ([`CHEMICAL_TANK_ITEM_CONFIG`]).
pub const CHEMICAL_TANK_SLOTS_SPEC: [SlotSpec; 2] = [
    SlotSpec {
        item: [16.0, 16.0],
        container: ContainerSlotType::Output,
        overlay: Some(SlotOverlay::Plus),
    },
    SlotSpec {
        item: [16.0, 48.0],
        container: ContainerSlotType::Input,
        overlay: Some(SlotOverlay::Minus),
    },
];

/// `setupIOConfig(ITEM, drainSlot, fillSlot, true)`: a entrada é o slot de drenar, a saída o de encher, e os dois juntos.
pub const CHEMICAL_TANK_ITEM_CONFIG: ItemConfig = ItemConfig {
    entries: &[
        (
            DataType::Input,
            &[machinery_basis_core::chemical_tank::DRAIN_SLOT],
        ),
        (
            DataType::Output,
            &[machinery_basis_core::chemical_tank::FILL_SLOT],
        ),
        (
            DataType::InputOutput,
            &[
                machinery_basis_core::chemical_tank::DRAIN_SLOT,
                machinery_basis_core::chemical_tank::FILL_SLOT,
            ],
        ),
    ],
};

/// `GuiChemicalBar(.., 42, 16, 116, 10, true)`: a barra horizontal, com a moldura (`GuiBar` soma dois à largura e à altura).
pub const CHEMICAL_TANK_BAR: [f32; 4] = [42.0, 16.0, 118.0, 12.0];
/// `GuiInnerScreen(this, 42, 37, 118, 28, ..)`.
pub const CHEMICAL_TANK_SCREEN: [f32; 4] = [42.0, 37.0, 118.0, 28.0];
/// `GuiGasMode(this, 159, 72, true, ..)`: o botão de 10 por 10 e o texto do modo à esquerda dele.
pub const CHEMICAL_TANK_MODE_BUTTON: [f32; 4] = [159.0, 72.0, 10.0, 10.0];
/// `GuiGasMode.renderForeground` com o texto à esquerda: `drawScrollingString(.., -69, 1, RIGHT, titleTextColor(), 69, 2, false)`, relativo ao botão. A área vai de
/// `x - 69 + 2` a `x - 2`, na linha que começa um pixel abaixo do topo.
pub const GAS_MODE_TEXT_START: f32 = -69.0;
pub const GAS_MODE_TEXT_WIDTH: f32 = 69.0;
pub const GAS_MODE_TEXT_PAD: f32 = 2.0;
pub const GAS_MODE_TEXT_Y: f32 = 1.0;

/// A área `[min_x, min_y, max_x, max_y]` do texto do modo de descarte, relativa ao canto da tela (`line_height` é `font().lineHeight`).
pub fn gas_mode_text_area(line_height: f32) -> [f32; 4] {
    let [x, y, _, _] = CHEMICAL_TANK_MODE_BUTTON;
    let start = x + GAS_MODE_TEXT_START;
    [
        start + GAS_MODE_TEXT_PAD,
        y + GAS_MODE_TEXT_Y,
        start + GAS_MODE_TEXT_WIDTH - GAS_MODE_TEXT_PAD,
        y + GAS_MODE_TEXT_Y + line_height,
    ]
}

/// `renderInventoryText(guiGraphics, 85)` de `GuiChemicalTank`: o rótulo do inventário rola numa área que acaba em `85 - inventoryLabelX - 6` depois do começo.
pub const CHEMICAL_TANK_INVENTORY_TEXT_END: f32 = 85.0;

/// `GuiGasMode.getResource`: o sprite do modo de agora (`chemical_mode/idle`, `dump_excess` e `dump`).
pub fn gas_mode_icon(mode: GasMode) -> &'static str {
    match mode {
        GasMode::Idle => "mekanism:gui/sprites/chemical_mode/idle",
        GasMode::DumpingExcess => "mekanism:gui/sprites/chemical_mode/dump_excess",
        GasMode::Dumping => "mekanism:gui/sprites/chemical_mode/dump",
    }
}

/// `GasMode.getTextComponent` (`MekanismLang.IDLE`, `DUMPING_EXCESS` e `DUMPING`): o texto ao lado do botão.
pub fn gas_mode_lang(mode: GasMode) -> &'static str {
    match mode {
        GasMode::Idle => "chemical_mode.mekanism.idle",
        GasMode::DumpingExcess => "chemical_mode.mekanism.dumping_excess",
        GasMode::Dumping => "chemical_mode.mekanism.dumping",
    }
}

/// `SpecialColors.TEXT_SCREEN`: a cor do texto de uma `GuiInnerScreen`.
pub const SCREEN_TEXT_COLOR: u32 = 0xFF_3C_FE_9A;
pub const CHEMICAL_LANG: &str = "gui.mekanism.chemical";
pub const NONE_LANG: &str = "gui.mekanism.none";
pub const STORED_LANG: &str = "generic.mekanism.stored";

/// As duas linhas da tela interna de `GuiChemicalTank` (`GuiChemicalTank.java:36-53`): o químico (`None` sem nada) e a fração guardada. No criativo o máximo é
/// `Infinite`, e com algo guardado a linha inteira é `Infinite`. `name` é o nome do químico já traduzido, `None` com o tanque vazio.
pub fn chemical_tank_screen_lines(
    name: Option<&str>,
    amount: u64,
    capacity: i64,
    creative: bool,
    translate: &dyn Fn(&str) -> String,
) -> [String; 2] {
    let format = |template: &str, args: &[&str]| {
        machinery_basis_core::units::format_lang(&translate(template), args)
    };
    let infinite = translate(INFINITE_LANG);
    let max = if creative {
        infinite.clone()
    } else {
        machinery_basis_core::units::grouped(capacity.max(0) as u64)
    };
    match name {
        None => [
            format(CHEMICAL_LANG, &[&translate(NONE_LANG)]),
            format(FRACTION_LANG, &["0", &max]),
        ],
        Some(name) => [
            format(CHEMICAL_LANG, &[name]),
            if creative {
                infinite
            } else {
                format(
                    FRACTION_LANG,
                    &[&machinery_basis_core::units::grouped(amount), &max],
                )
            },
        ],
    }
}

/// A primeira linha do tooltip de um medidor ou de uma barra de tanque (`GuiTankGauge.getContentsTooltips` e `GuiTankBar.getContentsTooltips`): `Empty`, o nome e
/// `Infinite` com `Long.MAX_VALUE` guardado, ou o nome e a quantidade em mB. `name` é o nome já traduzido, `None` com o tanque vazio.
pub fn tank_contents_tooltip(
    name: Option<&str>,
    amount: u64,
    translate: &dyn Fn(&str) -> String,
) -> String {
    match name {
        None => translate(EMPTY_LANG),
        Some(name) if amount == i64::MAX as u64 => machinery_basis_core::units::format_lang(
            &translate(STORED_LANG),
            &[name, &translate(INFINITE_LANG)],
        ),
        Some(name) => stored_mb(&translate(STORED_MB_LANG), name, amount),
    }
}

/// `GuiChemicalTank.drawForegroundText` (`:60`): `renderInventoryText(guiGraphics, 85)`.
pub const CHEMICAL_TANK_INVENTORY_END: f32 = 85.0;

/// O nível da barra do Chemical Tank (`ResourceTankInfoProvider.getLevel`: `MathUtils.divideToLevel(amountAsLong, capacityAsLong)`).
pub fn chemical_tank_bar_level(amount: u64, capacity: i64) -> f64 {
    machinery_basis_core::gui::divide_to_level(amount as f64, capacity as f64)
}

/// O cubo de energia que o item é, na ordem de [`CUBE_TIERS`].
pub fn cube_tier(item: &str) -> Option<usize> {
    CUBE_TIERS.iter().position(|id| *id == item)
}

/// A capacidade e a taxa de um cubo de energia (`ComponentBackedEnergyCubeContainer`: `tier::getCapacity` e `tier::getTransferRate`, os do arquivo `tiers.toml`).
pub fn cube_energy(tier: usize, capacities: &[i64; 5], rates: &[i32; 5]) -> EnergyItem {
    EnergyItem {
        capacity: capacities[tier],
        rate: rates[tier],
    }
}

/// O handler de energia de um item (`Capabilities.ENERGY` do item) que o slot de energia de um cubo usa: o equipamento (`EnergyContainerBuilder.basicCreator`: só `MANUAL` tira,
/// qualquer um põe, e a taxa é a de carga; o tablet deixa tirar) e os cinco cubos (`ComponentBackedEnergyCubeContainer`: qualquer um põe e tira, pela taxa do tier, e o criativo aceita e entrega sem mudar).
/// `amount` é a energia da pilha (o componente `mekanism:energy`). Os outros itens com energia (as máquinas e as células) ainda não têm handler no port.
pub fn item_energy_spec(item: &str, amount: i64, config: &ServerConfig) -> Option<ItemEnergySpec> {
    if let Some(gear) = gear_energy(item, &config.gear) {
        return Some(ItemEnergySpec {
            amount,
            capacity: gear.capacity,
            rate: gear.rate,
            // O Energy Tablet é o único que deixa tirar de fora (`basicCreator(alwaysTrue, alwaysTrue, ..)`); o resto do equipamento é `manualOnly`.
            can_extract: if item == "mekanism:energy_tablet" {
                Access::Any
            } else {
                Access::ManualOnly
            },
            can_insert: Access::Any,
            creative: false,
        });
    }
    let tier = cube_tier(item)?;
    Some(ItemEnergySpec {
        amount,
        capacity: config.cubes[tier],
        rate: config.cube_rates[tier],
        can_extract: Access::Any,
        can_insert: Access::Any,
        creative: tier == energy_cube::CREATIVE,
    })
}

/// O handler de energia do item de uma máquina (`ItemBlockTooltip.getDefaultEnergyContainer`: `BasicEnergyContainer.manualOnly` para tirar, qualquer um para pôr, a taxa de
/// carga `calculateUsage(capacity)`).
pub fn machine_item_spec(amount: i64, capacity: i64) -> ItemEnergySpec {
    ItemEnergySpec {
        amount,
        capacity,
        rate: calculate_usage(capacity),
        can_extract: Access::ManualOnly,
        can_insert: Access::Any,
        creative: false,
    }
}

/// O `TileEntityEnergyCube` de um tier: a capacidade e a taxa do arquivo, e o handler de energia dos itens.
pub fn energy_cube_config(
    tier: usize,
    config: &ServerConfig,
    items: ItemEnergy,
) -> EnergyCubeConfig {
    EnergyCubeConfig {
        tier,
        capacity: config.cubes[tier],
        transfer_rate: config.cube_rates[tier],
        items,
    }
}

/// Qual cabo é o bloco, na ordem de [`UNIVERSAL_CABLES`].
pub fn cable_tier(block: &str) -> Option<usize> {
    UNIVERSAL_CABLES.iter().position(|(id, _)| *id == block)
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

/// `GuiRecipeBookWindowTab`: `super(Mekanism.rl("button/recipe_book"), ...)`, o ícone de 18 por 18 sobre o botão.
pub const RECIPE_BOOK_TAB_ICON: &str = "mekanism:gui/sprites/button/recipe_book";
/// `MekanismLang.RECIPE_BOOK` (`gui.mekanism.recipe_book`), o tooltip da aba e o título da janela do livro.
pub const RECIPE_BOOK_TAB_TOOLTIP: &str = "gui.mekanism.recipe_book";

/// `GuiEnergyTab`: `super(Mekanism.rl("tab/energy_info"), gui, -26, 137, 26, 26)`, um elemento de textura na lateral esquerda, sem clique.
pub const ENERGY_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 137.0, 26.0, 26.0],
    };
pub const ENERGY_TAB_SPRITE: &str = "mekanism:gui/sprites/tab/energy_info";
// ---------------------------------------------------------------------------------------------------------------------------
// A tela do cubo de energia (`GuiEnergyCube`).

/// `GuiEnergyCube`: a tela tem o tamanho padrão (176 por 166) e `dynamicSlots`.
/// Os dois slots do menu, na ordem de `getInitialInventory`: o de descarga (`EnergyInventorySlot.fillOrConvert`, `SlotOverlay.MINUS`) em (17, 35) e o
/// de carga (`EnergyInventorySlot.drain`, `SlotOverlay.PLUS`) em (143, 35). Ambos nascem `POWER`, e a moldura vem do papel que a configuração de itens dá
/// ao slot (`CUBE_ITEM_CONFIG`).
pub const CUBE_SLOTS_SPEC: [SlotSpec; 2] = [
    SlotSpec {
        item: [17.0, 35.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Minus),
    },
    SlotSpec {
        item: [143.0, 35.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Plus),
    },
];

/// `setupIOConfig(ITEM, chargeSlot, dischargeSlot, true)`: a entrada é o slot de carga, a saída o de descarga, e os dois juntos.
pub const CUBE_ITEM_CONFIG: ItemConfig = ItemConfig {
    entries: &[
        (DataType::Input, &[energy_cube::CHARGE_SLOT]),
        (DataType::Output, &[energy_cube::DISCHARGE_SLOT]),
        (
            DataType::InputOutput,
            &[energy_cube::CHARGE_SLOT, energy_cube::DISCHARGE_SLOT],
        ),
    ],
};

/// `GuiEnergyGauge(.., GaugeType.WIDE, this, 55, 18)`: o medidor de 64 por 48 mais a moldura de um pixel de cada lado.
pub const CUBE_GAUGE: [f32; 4] = [55.0, 18.0, 66.0, 50.0];
/// `GuiSideHolder.create(this, imageWidth, 36, 98, false, true, TAB_ARMOR_SLOTS)`: o suporte da armadura, à direita.
pub const CUBE_ARMOR_HOLDER: machinery_basis_core::gui::Rect = [176.0, 36.0, 26.0, 98.0];
pub const CUBE_ARMOR_HOLDER_SPRITE: &str = "mekanism:gui/sprites/holder_right";
pub const CUBE_ENERGY_SPRITE: &str = "mekanism:mek_liquid/energy";
/// `MekanismRenderer.energyIcon`: cada quadro da animação tem 16 por 16.
pub const CUBE_ENERGY_FRAME: [f32; 2] = [16.0, 16.0];
/// `MekanismLang.MATRIX_INPUT_RATE`, `MAX_OUTPUT`, `EMPTY` e `INFINITE`.
pub const MATRIX_INPUT_RATE_LANG: &str = "matrix.mekanism.input_rate";
pub const MAX_OUTPUT_LANG: &str = "gui.mekanism.max_output";

pub fn cube_armor_bar_declaration() -> machinery_basis_core::armor_bar::Declaration {
    machinery_basis_core::armor_bar::Declaration {
        x: 180,
        y: 41,
        offhand_offset: 0,
    }
}

/// A frente do menu do cubo: o que `TileEntityMekanism.addContainerTrackers` sincroniza (a segurança, a redstone, o último recebido e a energia). O cubo não tem
/// upgrades, então o consumo e a capacidade não vão antes da energia.
pub fn cube_trackers() -> Vec<Tracked> {
    TileTrackers {
        security: true,
        redstone: true,
        energy: Some(EnergyTrackers {
            last_energy: true,
            adjustable: false,
        }),
        ..TileTrackers::default()
    }
    .trackers()
}

static CUBE_INDEXES: LazyLock<(usize, usize)> = LazyLock::new(|| {
    let list = cube_trackers();
    (
        position_of(&list, source::LAST_ENERGY_RECEIVED),
        position_of(&list, source::ENERGY),
    )
});

static CUBE_SECURITY_INDEXES: LazyLock<(usize, usize)> = LazyLock::new(|| {
    let list = cube_trackers();
    (
        position_of(&list, source::SECURITY_FREQUENCY),
        position_of(&list, source::SECURITY_MODE),
    )
});

/// A segurança que o cliente lê do menu do cubo (`GuiSecurityTab`).
pub fn cube_security(mirror: &Mirror) -> machinery_basis_core::security::SecurityView {
    let (frequency, mode) = *CUBE_SECURITY_INDEXES;
    machinery_basis_core::security::SecurityView::read(mirror, frequency, mode)
}

static CUBE_REDSTONE_INDEXES: LazyLock<(usize, usize, usize)> = LazyLock::new(|| {
    let list = cube_trackers();
    (
        position_of(&list, source::CONTROL_TYPE),
        position_of(&list, source::POWERED),
        position_of(&list, source::WAS_POWERED),
    )
});

/// O controle de redstone que o cliente lê do menu do cubo (`GuiRedstoneControlTab`).
pub fn cube_redstone(mirror: &Mirror) -> RedstoneSync {
    let (control, powered, was_powered) = *CUBE_REDSTONE_INDEXES;
    machinery_basis_core::redstone_tab::read(mirror, control, powered, was_powered)
}

/// O que o cliente lê do espelho do menu do cubo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CubeScreenData {
    pub energy: i64,
    pub input_rate: i64,
}

impl CubeScreenData {
    pub fn read(mirror: &Mirror) -> Self {
        let (input_rate, energy) = *CUBE_INDEXES;
        Self {
            energy: mirror.long(energy),
            input_rate: mirror.long(input_rate),
        }
    }
}

/// `GuiEnergyGauge.getScaledLevel` com `height - 2` de área útil.
pub fn cube_gauge_level(energy: i64, capacity: i64, height: i32) -> i32 {
    if energy == 0 {
        0
    } else if energy == i64::MAX {
        height - 2
    } else {
        std::cmp::max(
            1,
            ((height - 2) as f64
                * machinery_basis_core::gui::divide_to_level(energy as f64, capacity as f64))
                as i32,
        )
    }
}

/// `EnergyDisplay.of(energy, max).getTextComponent()`: `Infinite` para `Long.MAX_VALUE`, e a fração, com `Infinite` no máximo do cubo criativo.
pub fn cube_energy_text(energy: i64, max: i64, translate: &dyn Fn(&str) -> String) -> String {
    let (energy, max) = (energy.max(0), max.max(0));
    if energy == i64::MAX {
        return translate(INFINITE_LANG);
    }
    let unit = translate(ENERGY_UNIT_LANG);
    let stored = machinery_basis_core::units::compact_display(energy as u64, &unit);
    if max == 0 {
        return stored;
    }
    let capacity = if max == i64::MAX {
        translate(INFINITE_LANG)
    } else {
        machinery_basis_core::units::compact_display(max as u64, &unit)
    };
    machinery_basis_core::units::format_lang(&translate(FRACTION_LANG), &[&stored, &capacity])
}

/// O tooltip do medidor (`GuiEnergyGauge.getTooltipText`): `Empty`, ou a energia e a capacidade.
pub fn cube_gauge_tooltip(
    energy: i64,
    capacity: i64,
    translate: &dyn Fn(&str) -> String,
) -> String {
    if energy == 0 {
        translate(EMPTY_LANG)
    } else {
        cube_energy_text(energy, capacity, translate)
    }
}

/// `GuiEnergyTab` do cubo: `MATRIX_INPUT_RATE` com o último recebido, `MAX_OUTPUT` com a taxa do tier, e a unidade.
pub fn cube_energy_tab_tooltip(
    input_rate: i64,
    transfer_rate: i64,
    translate: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let unit = translate(ENERGY_UNIT_LANG);
    let show = |value: i64| cube_energy_text(value, 0, translate);
    vec![
        machinery_basis_core::units::format_lang(
            &translate(MATRIX_INPUT_RATE_LANG),
            &[&show(input_rate)],
        ),
        machinery_basis_core::units::format_lang(
            &translate(MAX_OUTPUT_LANG),
            &[&show(transfer_rate)],
        ),
        machinery_basis_core::units::format_lang(&translate(UNIT_LANG), &[&unit]),
    ]
}

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

/// A aba do livro de receitas das telas das máquinas.
pub fn recipe_book_tab() -> machinery_basis_core::side_tab::InsetTab {
    machinery_basis_core::side_tab::recipe_book_tab(SCREEN_IMAGE_WIDTH)
}

/// A aba de xp das telas das máquinas.
pub fn xp_tab() -> machinery_basis_core::side_tab::XpTab {
    machinery_basis_core::side_tab::XpTab::new(SCREEN_IMAGE_WIDTH, XP_TAB_Y, true)
}

/// Os elementos que a tela de uma máquina tem fora da imagem de 176 por 166 e que `GuiMekanism.findElementOver` acha (os que a tela do port
/// desenha; a janela do livro de receitas do original ainda não existe aqui, só a aba):
/// - `GuiEnergyTab` (`addGuiElements` de `GuiElectricMachine` e de `GuiMetallurgicInfuser`): -26, 137, 26 por 26, sempre ativa e visível.
/// - `GuiWarningTab` (`GuiMekanism.addWarningTab`, à esquerda): -26, 109, 26 por 26, visível só com algum aviso (`updateVisibility`).
/// - `GuiRecipeBookWindowTab`: o botão de dentro em (179, 66), sempre ativo e visível (`GuiInsetElement.isMouseOver` só olha o botão de 18 por 18).
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
        // `GuiRecipeBookWindowTab`: o botão de dentro em (179, 66), sempre ativo e visível (sem janela do livro aberta, que o port ainda não tem).
        Element::always(recipe_book_tab().button()),
        // `GuiSecurityTab`: o botão de dentro, sempre ativo e visível (a máquina tem dono ou não).
        Element::always(
            machinery_basis_core::side_tab::security_tab(
                SCREEN_IMAGE_WIDTH,
                machinery_basis_core::side_tab::SECURITY_TAB_Y,
            )
            .button(),
        ),
        // `GuiRedstoneControlTab`: o botão de dentro em (179, 141), sempre ativo e visível.
        Element::always(machinery_basis_core::redstone_tab::tab(SCREEN_IMAGE_WIDTH).button()),
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
    /// `TileComponentSecurity`: a frequência de segurança e o modo, que a aba de segurança lê.
    pub security_frequency: usize,
    pub security_mode: usize,
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
            security_frequency: at(source::SECURITY_FREQUENCY),
            security_mode: at(source::SECURITY_MODE),
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

/// A segurança que o cliente lê do menu de uma máquina (`GuiSecurityTab`).
pub fn machine_security(
    spec: &ScreenSpec,
    mirror: &Mirror,
) -> machinery_basis_core::security::SecurityView {
    let indexes = (spec.indexes)();
    machinery_basis_core::security::SecurityView::read(
        mirror,
        indexes.security_frequency,
        indexes.security_mode,
    )
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

/// A chave de idioma do nome de um fluido (`FluidType.getDescriptionId`).
pub fn fluid_lang_key(id: &str) -> String {
    // `FluidType.getDescriptionId`: a água e a lava do jogo usam a chave do bloco, e o resto `fluid_type.<ns>.<nome>` (`Util.makeDescriptionId`).
    match id.split_once(':') {
        Some(("minecraft", name @ ("water" | "lava"))) => format!("block.minecraft.{name}"),
        Some((namespace, name)) => format!("fluid_type.{namespace}.{name}"),
        None => format!("fluid_type.{id}"),
    }
}

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
    side_config: SideConfigSpec,
) -> MachineConfig {
    MachineConfig {
        energy_per_tick,
        base_ticks_required: TICKS_REQUIRED as i32,
        // O original guarda o fornecedor de `blockDeactivationDelay` e lê `maxUpgradeMultiplier` a cada conta: o valor de agora, não o do nascimento.
        deactivation_delay: ConfigValue::Live(settings::block_deactivation_delay),
        upgrades: UpgradeConfig {
            max_upgrade_multiplier: ConfigValue::Live(settings::max_upgrade_multiplier_f64),
            speed: Rc::from(SPEED_UPGRADE),
            energy: Rc::from(ENERGY_UPGRADE),
            xp_containment: Rc::from(XP_CONTAINMENT_UPGRADE),
            muffling: Rc::from(MUFFLING_UPGRADE),
            data: upgrades,
        },
        tracked_errors: tracked_errors.to_vec(),
        check_offset,
        side_config,
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
            electric_side_config(),
        ),
        recipes,
        items: no_item_energy(),
    }
}

/// Um `MekanismRecipeType.ENERGY_CONVERSION.getInputCache()` sem receita: o que as máquinas têm antes de quem as monta dar o índice
/// do jogo (`glue.rs` o põe em `energy_conversion`), e o que os testes sem conversão usam.
pub fn no_energy_conversion() -> Rc<EnergyConversionCache> {
    EnergyConversion::cache_of(Vec::new())
}

/// O handler de energia dos itens sem item nenhum e sem conversão: o que as máquinas têm antes de quem as monta dar o do jogo (`glue.rs` o põe em `items`).
pub fn no_item_energy() -> ItemEnergy {
    item_energy_of(no_energy_conversion())
}

/// O handler de energia dos itens sem item que guarde energia, com o índice de conversões dado: o que os testes usam.
pub fn item_energy_of(conversions: Rc<EnergyConversionCache>) -> ItemEnergy {
    ItemEnergy {
        lookup: Rc::new(|_| None),
        write: Rc::new(|_, _| None),
        conversions,
    }
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

/// O que um tubo pressurizado vizinho vê no tanque de infusão do Metallurgic Infuser pelo lado `face` do bloco (`TileEntityMekanism.getCapability` com `Capabilities.CHEMICAL.block()`):
/// `None` onde a capability está desligada (`TileComponentConfig.isCapabilityDisabled`: o `DataType` do lado é `NONE`), e senão o tanque com as regras do lado
/// (`ConfigHolder.canInsert` e `canExtract` do `SlotInfo`) e as dele (`BasicChemicalTank.create(MAX_INFUSE, alwaysTrueBi, (chemical, _) -> containsRecipeBA(inputSlot.resource(), chemical),
/// containsRecipeB, ..)`: só entra de fora o químico que uma receita usa com o item do slot de entrada, e quem está fora sempre pode tirar). O tanque é o vivo da máquina, e a
/// regra de entrada o lê na hora de inserir.
pub fn infusion_tank_view(
    machine: &ProgressMachine,
    face: Face,
) -> Option<ResourceSnapshot<ChemicalResource>> {
    let side = machine.chemical_side()?;
    let config = machine.side_config();
    if config.is_capability_disabled(TransmissionType::Chemical, machine.facing(), Some(face)) {
        return None;
    }
    let info = config.slot_info(TransmissionType::Chemical, machine.facing(), Some(face))?;
    let tank = side.tank.clone();
    Some(ResourceSnapshot {
        contents: tank.contents(),
        capacity: tank.capacity_for(None),
        accepts: {
            let tank = tank.clone();
            Rc::new(move |chemical: &ChemicalResource| {
                tank.is_valid_for_insertion(chemical, AutomationType::External)
            })
        },
        can_insert: info.can_input,
        can_extract: info.can_output,
        creative: false,
        increment: 1,
    })
}

/// O que cada lado do tanque de infusão deixa fazer (`None` sem capability, e senão se entra e se sai), na ordem de `Face::ALL`. Quem anda a máquina diz isso aos tubos vizinhos
/// a cada tick, e só uma mudança os avisa (`TileComponentConfig.sideChanged` invalida a capability).
pub fn infusion_tank_faces(machine: &ProgressMachine) -> [Option<(bool, bool)>; 6] {
    Face::ALL.map(|face| {
        infusion_tank_view(machine, face).map(|view| (view.can_insert, view.can_extract))
    })
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
            infuser_side_config(),
        ),
        recipes,
        conversions,
        tank_capacity: TANK_CAPACITY,
        items: no_item_energy(),
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
        energy: ItemEnergy,
    },
    Infuser {
        recipes: Rc<ItemChemicalCache>,
        conversions: Rc<ConversionCache>,
        energy: ItemEnergy,
    },
}

impl Indexes {
    /// `MekanismRecipeType.ENERGY_CONVERSION.getInputCache()`, que as quatro máquinas partilham.
    pub fn energy(&self) -> &ItemEnergy {
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

/// Se um item pode sair do slot `slot` para quem está de fora (`isValidForExtraction(item, EXTERNAL)`): o slot de entrada
/// (`InputInventorySlot`, `canExtract` só para o interno) nunca deixa; o de saída (`OutputInventorySlot`, `alwaysTrue`) sempre; o de
/// energia e o de infusão (`fillOrConvert`: `!external || !canFillOrConvert(item)`) deixam só o item que o tanque ou o buffer não
/// converteria. É a regra que o `can_take` do Horizon não tem (H2).
pub fn slot_extractable(
    indexes: &Indexes,
    slots: &SlotIndexes,
    slot: usize,
    item: &ItemResource,
    held: &Held,
) -> bool {
    if slot == slots.energy || Some(slot) == slots.infusion {
        return !slot_accepts(indexes, slots, slot, item, held);
    }
    slot != slots.input
}

/// As faces de item da instância (H2): o que a configuração de lados dá a uma máquina virada para `front`.
pub fn item_instance(infuser: bool, config: &TileComponentConfig, front: Face) -> InstanceItems {
    let all: &[usize] = if infuser {
        &INFUSER_ALL_SLOTS
    } else {
        &ELECTRIC_ALL_SLOTS
    };
    InstanceItems::of_config(config, front, all)
}

/// As faces de item de um cubo de energia (H2, decisão 94): o `ConfigHolder` dele só tem os slots de carga e de descarga, então a lista de "todos os slots" (o `NO_CONFIG`) é vazia.
pub fn cube_item_instance(config: &TileComponentConfig, front: Face) -> InstanceItems {
    InstanceItems::of_config(config, front, &[])
}

/// As regras dos slots de carga e de descarga do cubo de `tier` com `stored` de energia (`EnergyInventorySlot.drain` e `fillOrConvert`), para quem pergunta de fora.
pub fn cube_slot_rules(
    tier: usize,
    stored: i64,
    config: &ServerConfig,
    items: &ItemEnergy,
) -> energy_cube::SlotRules {
    energy_cube::SlotRules::new(
        tier,
        config.cubes[tier],
        config.cube_rates[tier],
        stored,
        items,
    )
}

/// `AttributeEnergy.STORAGE_MULTIPLIER`: o buffer guarda quatro ticks de uso.
/// O Horizon guarda a capacidade em `i32` (`ModEnergyBuffer`): o que passa disso fica em `i32::MAX`; a máquina viva guarda a conta inteira em `i64`.
pub const fn energy_capacity(usage: i32) -> i32 {
    let capacity = 4 * (usage as i64);
    if capacity > i32::MAX as i64 {
        i32::MAX
    } else {
        capacity as i32
    }
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
/// `WindowType.SIDE_CONFIG` e `TRANSPORTER_CONFIG`.
pub const SIDE_CONFIG_WINDOW: WindowType = WINDOW_TYPES[7];
pub const TRANSPORTER_CONFIG_WINDOW: WindowType = WINDOW_TYPES[8];
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

/// Quanto o cabo guarda, se o bloco for um cabo.
pub fn cable_capacity(block: &str) -> Option<i32> {
    let cable = UNIVERSAL_CABLES.iter().find(|(id, _)| *id == block);
    cable.map(|&(_, capacity)| capacity)
}

/// `MekanismDataComponents.SIDE_CONFIG` e `EJECTOR`: os componentes do item que guardam a configuração de lados e o ejetor.
pub const SIDE_CONFIG_COMPONENT: &str = "mekanism:side_config";
pub const EJECTOR_COMPONENT: &str = "mekanism:ejector";

/// `LightConfigInfo.MACHINE`: tudo entrada, a direita saída e as costas a energia. Não ejeta.
pub fn light_machine() -> LightConfigInfo {
    LightConfigInfo::create(
        Some(RelativeSide::Right),
        None,
        Some(RelativeSide::Back),
        false,
    )
}

/// `LightConfigInfo.EXTRA_MACHINE`: como a `MACHINE`, mais a base como o slot extra.
pub fn light_extra_machine() -> LightConfigInfo {
    LightConfigInfo::create(
        Some(RelativeSide::Right),
        Some(RelativeSide::Bottom),
        Some(RelativeSide::Back),
        false,
    )
}

/// `LightConfigInfo.INPUT_ONLY`: tudo entrada.
pub fn light_input_only() -> LightConfigInfo {
    LightConfigInfo::create(None, None, None, false)
}

/// `LightConfigInfo.OUT_NO_EJECT`: tudo entrada e a direita saída, sem ejetar.
pub fn light_out_no_eject() -> LightConfigInfo {
    LightConfigInfo::create(Some(RelativeSide::Right), None, None, false)
}

/// `AttachedSideConfig.ELECTRIC_MACHINE`, o padrão do item da Enrichment Chamber, do Crusher e do Energized Smelter.
pub fn electric_machine_default() -> AttachedSideConfig {
    AttachedSideConfig::new(&[
        (TransmissionType::Item, light_machine()),
        (TransmissionType::Energy, light_input_only()),
    ])
}

/// `LightConfigInfo.FRONT_OUT_NO_EJECT` e `FRONT_OUT_EJECT`: tudo entrada e a frente saída.
pub fn light_front_out(ejecting: bool) -> LightConfigInfo {
    LightConfigInfo::create(Some(RelativeSide::Front), None, None, ejecting)
}

/// `ItemBlockEnergyCube.SIDE_CONFIG`, o padrão do item dos cubos: a frente sai nos itens e na energia, e a energia ejeta.
pub fn energy_cube_default() -> AttachedSideConfig {
    AttachedSideConfig::new(&[
        (TransmissionType::Item, light_front_out(false)),
        (TransmissionType::Energy, light_front_out(true)),
    ])
}

/// `ItemBlockEnergyCube.ALL_INPUT` e `ALL_OUTPUT`, as duas pilhas do cubo criativo na aba criativa: a energia toda de entrada, sem ejetar, ou toda de saída, ejetando.
pub fn energy_cube_creative_default(all_output: bool) -> AttachedSideConfig {
    let kind = if all_output {
        machinery_basis_core::side_config::DataType::Output
    } else {
        machinery_basis_core::side_config::DataType::Input
    };
    let sides: Vec<_> = RelativeSide::VALUES
        .iter()
        .map(|side| (*side, kind))
        .collect();
    AttachedSideConfig::new(&[
        (TransmissionType::Item, light_front_out(false)),
        (
            TransmissionType::Energy,
            LightConfigInfo::new(&sides, all_output),
        ),
    ])
}

/// `AttachedSideConfig.ADVANCED_MACHINE`, o padrão do item do Metallurgic Infuser: o slot extra na base, e o químico sai pela direita.
pub fn advanced_machine_default() -> AttachedSideConfig {
    AttachedSideConfig::new(&[
        (TransmissionType::Item, light_extra_machine()),
        (TransmissionType::Chemical, light_out_no_eject()),
        (TransmissionType::Energy, light_input_only()),
    ])
}

/// `TileEntityElectricMachine`: `AttributeSideConfig.ELECTRIC_MACHINE` (itens e energia), `setupItemIOConfig(input, output, energy)`,
/// `setupInputConfig(ENERGY, energyContainer)` e `ejectorComponent.setOutputData(config, ITEM)`.
pub fn electric_side_config() -> SideConfigSpec {
    SideConfigSpec {
        types: vec![TransmissionType::Item, TransmissionType::Energy],
        setup: |config| {
            config.setup_item_io_config(
                &[ELECTRIC_INPUT_SLOT],
                &[ELECTRIC_OUTPUT_SLOT],
                ELECTRIC_ENERGY_SLOT,
                false,
            );
            config.setup_input_config(TransmissionType::Energy, 0);
        },
        eject: vec![TransmissionType::Item],
    }
}

/// `TileEntityMetallurgicInfuser`: `ADVANCED_ELECTRIC_MACHINE` (itens, químico e energia), `setupItemIOExtraConfig`,
/// `setupInputConfig(ENERGY)`, `setupIOConfig(CHEMICAL, infusionTank).setCanEject(false)` e o ejetor só de itens.
pub fn infuser_side_config() -> SideConfigSpec {
    SideConfigSpec {
        types: vec![
            TransmissionType::Item,
            TransmissionType::Chemical,
            TransmissionType::Energy,
        ],
        setup: |config| {
            config.setup_item_io_extra_config(INPUT_SLOT, OUTPUT_SLOT, INFUSION_SLOT, ENERGY_SLOT);
            config.setup_input_config(TransmissionType::Energy, 0);
            config.setup_io_config(TransmissionType::Chemical, 0, false);
            if let Some(chemical) = config.config_mut(TransmissionType::Chemical) {
                chemical.set_can_eject(false);
            }
        },
        eject: vec![TransmissionType::Item],
    }
}

/// Os slots de itens de cada máquina, na ordem de `getInventorySlots`: o que `ConfigHolder` devolve quando não há configuração.
pub const ELECTRIC_ALL_SLOTS: [usize; 3] = [
    ELECTRIC_INPUT_SLOT,
    ELECTRIC_OUTPUT_SLOT,
    ELECTRIC_ENERGY_SLOT,
];
pub const INFUSER_ALL_SLOTS: [usize; 4] = [INFUSION_SLOT, INPUT_SLOT, OUTPUT_SLOT, ENERGY_SLOT];

/// Uma máquina com o padrão do item aplicado, só a configuração (sem o resto do tile): o que o rastro dos hoppers e os testes de
/// face usam. `infuser` escolhe o Metallurgic Infuser.
pub fn item_placed_config(infuser: bool) -> TileComponentConfig {
    let (spec, default) = if infuser {
        (infuser_side_config(), advanced_machine_default())
    } else {
        (electric_side_config(), electric_machine_default())
    };
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    config.apply_implicit_components(Some(&default), Face::North, false);
    config
}

/// Os slots que a face alcança numa máquina posta por um item (`ConfigHolder`): a regra que o hopper vizinho segue.
pub fn item_slots(infuser: bool, front: Face, face: Face) -> FaceSlots {
    let all: &[usize] = if infuser {
        &INFUSER_ALL_SLOTS
    } else {
        &ELECTRIC_ALL_SLOTS
    };
    item_placed_config(infuser).item_face(front, face, all)
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
        light: json
            .get("light_level")
            .and_then(Value::as_u64)
            .map_or(0, |level| level.min(15) as u8),
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

/// A experiência que o minério dá ao quebrar (`BlockOre.getExpDrop`, `BlockOre.java`): `Mth.nextInt(random, minExp, maxExp)` com o `minExp` e o `maxExp` do
/// `OreType` (`OreType.java`). Só a fluorita dá (1 a 4: `FLUORITE(MiscResource.FLUORITE, 1, 4, 0.7F, ...)`); estanho, ósmio, urânio e chumbo têm 0 e 0 e não sorteiam
/// nada. O Horizon só dá experiência aos minérios do vanilla pelo nome (`break_experience`), então o que o mod acrescenta passa por esta função. `Mth.nextInt` não
/// sorteia quando `min >= max`.
pub fn ore_experience(block: &str, mut next_int_bound: impl FnMut(i32) -> i32) -> Option<u32> {
    let (min, max) = match block {
        "mekanism:fluorite_ore" | "mekanism:deepslate_fluorite_ore" => (1, 4),
        _ => return None,
    };
    Some(if min >= max {
        min as u32
    } else {
        (next_int_bound(max - min + 1) + min) as u32
    })
}

/// Os itens do Mekanism com o componente `MekanismDataComponents.DETAILS` (`.component(MekanismDataComponents.DETAILS, Unit.INSTANCE)` nos `ItemBlock*` e em
/// `BlockDeferredRegister.registerBlock`): o tooltip deles esconde os detalhes sem a tecla de detalhes e põe a dica `Hold Left Shift for details.`. A lista é a do rastro
/// (`rastros/upgrade_history_details.jsonl`, `UpgradeHistoryTraceTest.detailsItems`), e o teste `details_items_trace` a confere.
pub const DETAILS_ITEMS: [&str; 91] = [
    "mekanism:basic_smelting_factory",
    "mekanism:basic_enriching_factory",
    "mekanism:basic_crushing_factory",
    "mekanism:basic_compressing_factory",
    "mekanism:basic_combining_factory",
    "mekanism:basic_purifying_factory",
    "mekanism:basic_injecting_factory",
    "mekanism:basic_infusing_factory",
    "mekanism:basic_sawing_factory",
    "mekanism:advanced_smelting_factory",
    "mekanism:advanced_enriching_factory",
    "mekanism:advanced_crushing_factory",
    "mekanism:advanced_compressing_factory",
    "mekanism:advanced_combining_factory",
    "mekanism:advanced_purifying_factory",
    "mekanism:advanced_injecting_factory",
    "mekanism:advanced_infusing_factory",
    "mekanism:advanced_sawing_factory",
    "mekanism:elite_smelting_factory",
    "mekanism:elite_enriching_factory",
    "mekanism:elite_crushing_factory",
    "mekanism:elite_compressing_factory",
    "mekanism:elite_combining_factory",
    "mekanism:elite_purifying_factory",
    "mekanism:elite_injecting_factory",
    "mekanism:elite_infusing_factory",
    "mekanism:elite_sawing_factory",
    "mekanism:ultimate_smelting_factory",
    "mekanism:ultimate_enriching_factory",
    "mekanism:ultimate_crushing_factory",
    "mekanism:ultimate_compressing_factory",
    "mekanism:ultimate_combining_factory",
    "mekanism:ultimate_purifying_factory",
    "mekanism:ultimate_injecting_factory",
    "mekanism:ultimate_infusing_factory",
    "mekanism:ultimate_sawing_factory",
    "mekanism:security_desk",
    "mekanism:industrial_alarm",
    "mekanism:enrichment_chamber",
    "mekanism:osmium_compressor",
    "mekanism:combiner",
    "mekanism:crusher",
    "mekanism:digital_miner",
    "mekanism:metallurgic_infuser",
    "mekanism:purification_chamber",
    "mekanism:energized_smelter",
    "mekanism:teleporter",
    "mekanism:electric_pump",
    "mekanism:personal_barrel",
    "mekanism:personal_chest",
    "mekanism:chargepad",
    "mekanism:logistical_sorter",
    "mekanism:rotary_condensentrator",
    "mekanism:chemical_oxidizer",
    "mekanism:chemical_infuser",
    "mekanism:chemical_injection_chamber",
    "mekanism:electrolytic_separator",
    "mekanism:precision_sawmill",
    "mekanism:chemical_dissolution_chamber",
    "mekanism:chemical_washer",
    "mekanism:chemical_crystallizer",
    "mekanism:seismic_vibrator",
    "mekanism:pressurized_reaction_chamber",
    "mekanism:isotopic_centrifuge",
    "mekanism:nutritional_liquifier",
    "mekanism:fluidic_plenisher",
    "mekanism:laser",
    "mekanism:laser_amplifier",
    "mekanism:laser_tractor_beam",
    "mekanism:quantum_entangloporter",
    "mekanism:solar_neutron_activator",
    "mekanism:oredictionificator",
    "mekanism:resistive_heater",
    "mekanism:formulaic_assemblicator",
    "mekanism:fuelwood_heater",
    "mekanism:modification_station",
    "mekanism:antiprotonic_nucleosynthesizer",
    "mekanism:pigment_extractor",
    "mekanism:pigment_mixer",
    "mekanism:painting_machine",
    "mekanism:dimensional_stabilizer",
    "mekanism:qio_drive_array",
    "mekanism:qio_dashboard",
    "mekanism:qio_importer",
    "mekanism:qio_exporter",
    "mekanism:qio_redstone_adapter",
    "mekanism:basic_energy_cube",
    "mekanism:advanced_energy_cube",
    "mekanism:elite_energy_cube",
    "mekanism:ultimate_energy_cube",
    "mekanism:creative_energy_cube",
];

/// Se o item tem o componente `DETAILS` (a dica de detalhes e o esconde-detalhes do tooltip).
pub fn has_details(item: &str) -> bool {
    DETAILS_ITEMS.contains(&item)
}

/// A chave da descrição de um item com `DETAILS` (`IHasDescription.getDescription`: a chave `description.mekanism.<nome>` do `MekanismLang`): a do id, menos as fábricas (as 36
/// dividem `description.mekanism.factory`) e os cubos de energia (os 5 dividem `description.mekanism.energy_cube`).
pub fn description_key(item: &str) -> String {
    let name = item.strip_prefix("mekanism:").unwrap_or(item);
    if name.ends_with("_factory") {
        return "description.mekanism.factory".to_owned();
    }
    if name.ends_with("_energy_cube") {
        return "description.mekanism.energy_cube".to_owned();
    }
    // Os cinco Fluid Tanks e os cinco Chemical Tanks dividem uma descrição cada (o rastro `tank_fluid_item_use`).
    for shared in ["fluid_tank", "chemical_tank"] {
        if name.ends_with(shared) {
            return format!("description.mekanism.{shared}");
        }
    }
    format!("description.mekanism.{name}")
}

/// `ItemRegistryObject.getNameColor` (`:91-104`): o item de um bloco com tier (`Attribute.getBaseTier`) leva a cor do tier no nome (`BaseTier.getTextColor`: Basic `0x5FFFB8`, Advanced
/// `0xFF806A`, Elite `0x4BF8FF`, Ultimate `0xF787FF`, Creative `0x585858`). Aqui os blocos com tier que têm `DETAILS`: as 36 fábricas e os 5 cubos de energia.
pub fn tier_block_color(item: &str) -> Option<u32> {
    let name = item.strip_prefix("mekanism:")?;
    if !(name.ends_with("_factory")
        || name.ends_with("_energy_cube")
        || name.ends_with("_fluid_tank")
        || name.ends_with("_chemical_tank"))
    {
        return None;
    }
    match name.split('_').next()? {
        "basic" => Some(0x5FFFB8),
        "advanced" => Some(0xFF806A),
        "elite" => Some(0x4BF8FF),
        "ultimate" => Some(0xF787FF),
        "creative" => Some(0x585858),
        _ => None,
    }
}

/// A cor do nome de um item (`ItemRegistryObject.setNameColor`, `ItemDeferredRegister`: o componente `item_name` com a cor do tier ou do `EnumColor`): os quatro
/// circuitos, os quatro discos do QIO e os quatro instaladores de tier (`BaseTier.getTextColor`, `BaseTier.java:22-25`; o instalador pela cor do tier de destino,
/// `registerInstaller`, `MekanismItems.java:299-302`), os três pellets (`EnumColor.PURPLE`, `GRAY` e `INDIGO`, `EnumColor.java`) e o Configurator (`EnumColor.AQUA`, `MekanismItems.java:93-94`).
/// O Horizon lê o `item_name` só do dado de cada pilha, não do componente padrão do item, então a cor vai pelo hook do tooltip (`glue::tooltip`).
pub fn name_color(item: &str) -> Option<u32> {
    if let Some(color) = tier_block_color(item) {
        return Some(color);
    }
    match item {
        "mekanism:basic_control_circuit"
        | "mekanism:qio_drive_base"
        | "mekanism:basic_tier_installer" => Some(0x5FFFB8),
        "mekanism:advanced_control_circuit"
        | "mekanism:qio_drive_hyper_dense"
        | "mekanism:advanced_tier_installer" => Some(0xFF806A),
        "mekanism:elite_control_circuit"
        | "mekanism:qio_drive_time_dilating"
        | "mekanism:elite_tier_installer" => Some(0x4BF8FF),
        "mekanism:ultimate_control_circuit"
        | "mekanism:qio_drive_supermassive"
        | "mekanism:ultimate_tier_installer" => Some(0xF787FF),
        "mekanism:configurator" => Some(0x30FFF9),
        "mekanism:pellet_antimatter" => Some(0xA460D9),
        "mekanism:pellet_plutonium" => Some(0xCFCFCF),
        "mekanism:pellet_polonium" => Some(0x559EFF),
        _ => None,
    }
}

/// O desenho vestido do MekaSuit, do jetpack, das botas e do tanque de mergulho: o que o `IClientItemExtensions` deles (`ISpecialGear.gearModel`) faz no original
/// (`client/render/armor`), como tabela para `machinery_basis_core::worn_gear`. Os números saem do Java (`rastros/armor_gear.json`, que `tests/worn_gear.rs` confere).
pub mod worn_gear {
    use machinery_basis_core::worn_gear::{Attachment, Foil, Gear, Op, Slot, SpriteRect};

    /// `OBJModelData.modelState`: a rotação de PI em Z que o original põe em todo quad do OBJ do MekaSuit ao assar (`Transformation(matrix.rotate(Axis.ZP.rotation(PI)))`, em volta do
    /// centro do bloco): o ponto (x, y, z) vira (1 - x, 1 - y, z). Como passos da pose, `translate(1, 1, 0)` e `scale(-1, -1, 1)`: o Horizon assa o OBJ sem ela.
    const OBJ_STATE: [Op; 2] = [Op::Translate([1.0, 1.0, 0.0]), Op::Scale([-1.0, -1.0, 1.0])];

    /// O modelo de um item (`ItemStackRenderState`, o do jetpack e o do tanque) é desenhado com o centro do bloco na origem da pose; o modelo avulso do Horizon parte do canto.
    const ITEM_CENTER: Op = Op::Translate([-0.5, -0.5, -0.5]);

    /// Uma posição do corpo do `MekaSuitArmor.ModelPos`: a parte, a translação `BASE_TRANSLATION + deslocamento` (`translateModel`) e a do bebê (`scaleBaby`).
    struct Position {
        part: &'static str,
        ops: &'static [Op],
        baby_ops: &'static [Op],
    }

    const HEAD: Position = Position {
        part: "head",
        ops: &[Op::Translate([-1.0, 0.5, 0.0]), OBJ_STATE[0], OBJ_STATE[1]],
        // A cabeça do bebê não muda de escala nem de lugar (`if (this != HEAD) scale`, sem `case HEAD`).
        baby_ops: &[Op::Translate([-1.0, 0.5, 0.0]), OBJ_STATE[0], OBJ_STATE[1]],
    };
    const BODY: Position = Position {
        part: "body",
        ops: &[Op::Translate([-1.0, 0.5, 0.0]), OBJ_STATE[0], OBJ_STATE[1]],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([0.0, -0.55, 0.0]),
            Op::Translate([-1.0, 0.5, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
    };
    const LEFT_ARM: Position = Position {
        part: "left_arm",
        ops: &[
            Op::Translate([-1.3125, 0.375, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([-0.15625, 0.0, 0.0]),
            Op::Translate([-1.3125, 0.375, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
    };
    const RIGHT_ARM: Position = Position {
        part: "right_arm",
        ops: &[
            Op::Translate([-0.6875, 0.375, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([0.15625, 0.0, 0.0]),
            Op::Translate([-0.6875, 0.375, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
    };
    const LEFT_LEG: Position = Position {
        part: "left_leg",
        ops: &[
            Op::Translate([-1.125, -0.25, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([-0.0625, -0.25, 0.0]),
            Op::Translate([-1.125, -0.25, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
    };
    const RIGHT_LEG: Position = Position {
        part: "right_leg",
        ops: &[
            Op::Translate([-0.875, -0.25, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([0.0625, -0.25, 0.0]),
            Op::Translate([-0.875, -0.25, 0.0]),
            OBJ_STATE[0],
            OBJ_STATE[1],
        ],
    };

    /// Um grupo de objetos do OBJ numa posição (`mekanism:entity/mekasuit/<peça>_<posição>_<classe>[_<etiqueta>]`). Sem luz nos LEDs (`_led`: `RenderTypes.eyes`). O MekaSuit esconde
    /// as partes do corpo e desenha mesmo assim, então não pede a parte visível.
    const fn suit(
        position: Position,
        model: &'static str,
        unlit: bool,
        hidden_if_worn: Option<Slot>,
    ) -> Attachment {
        Attachment {
            part: position.part,
            model,
            ops: position.ops,
            baby_ops: position.baby_ops,
            unlit,
            hidden_if_worn,
            needs_visible_part: false,
        }
    }

    /// `ICustomArmor` do jetpack (`ChestArmor.JETPACK`): o modelo do item no corpo, `translate(0, 0.3, 0)` (adulto) ou `scale(ageScale)` e `translate(0, -0.05, 0)` (bebê), depois
    /// `translate(0, 0, 0.145)` e `scale(1.02, -1.02, -1.02)`.
    const fn jetpack(model: &'static str) -> Attachment {
        Attachment {
            part: "body",
            model,
            ops: &[
                Op::Translate([0.0, 0.3, 0.0]),
                Op::Translate([0.0, 0.0, 0.145]),
                Op::Scale([1.02, -1.02, -1.02]),
                ITEM_CENTER,
            ],
            baby_ops: &[
                Op::AgeScale,
                Op::Translate([0.0, -0.05, 0.0]),
                Op::Translate([0.0, 0.0, 0.145]),
                Op::Scale([1.02, -1.02, -1.02]),
                ITEM_CENTER,
            ],
            unlit: false,
            hidden_if_worn: None,
            needs_visible_part: true,
        }
    }

    /// `ChestArmor.SCUBA_TANK`: `translate(0, 0.41, 0)` (o bebê só escala por `ageScale`), depois `translate(0, 0, 0.28)` e `scale(1, -1, -1)`.
    const SCUBA_TANK_MODEL: Attachment = Attachment {
        part: "body",
        model: "mekanism:item/scuba_tank",
        ops: &[
            Op::Translate([0.0, 0.41, 0.0]),
            Op::Translate([0.0, 0.0, 0.28]),
            Op::Scale([1.0, -1.0, -1.0]),
            ITEM_CENTER,
        ],
        baby_ops: &[
            Op::AgeScale,
            Op::Translate([0.0, 0.0, 0.28]),
            Op::Scale([1.0, -1.0, -1.0]),
            ITEM_CENTER,
        ],
        unlit: false,
        hidden_if_worn: None,
        needs_visible_part: true,
    };

    /// `FreeRunnerArmor.applyTransforms`: a perna, `scale(ageScale)` e `translate(0, -0.2, 0)` no bebê, depois `translate(xo, 1, 0.525)` e `scale(1.02, -1.02, -1.02)`.
    macro_rules! runner {
        ($part:literal, $model:literal, $xo:literal) => {
            Attachment {
                part: $part,
                model: $model,
                ops: &[
                    Op::Translate([$xo, 1.0, 0.525]),
                    Op::Scale([1.02, -1.02, -1.02]),
                ],
                baby_ops: &[
                    Op::AgeScale,
                    Op::Translate([0.0, -0.2, 0.0]),
                    Op::Translate([$xo, 1.0, 0.525]),
                    Op::Scale([1.02, -1.02, -1.02]),
                ],
                unlit: false,
                hidden_if_worn: None,
                needs_visible_part: true,
            }
        };
    }

    /// A família do traje: `ItemMekaSuitArmor` (as peças `wornParts` de `MekaSuitArmor.key`).
    const SUIT: &str = "mekasuit";
    const SUIT_HIDES_HEAD: &[&str] = &["head"];
    const SUIT_HIDES_CHEST: &[&str] = &["body", "left_arm", "right_arm"];
    const SUIT_HIDES_LEGS: &[&str] = &["left_leg", "right_leg"];

    // `MekaSuitArmor.HELMET`: o vizinho é o peito. `shared_` e `excl_` valem quando o vizinho veste uma peça do traje (o `shared_` só se o vizinho vem depois na ordem
    // de `EquipmentSlot`: cabeça 5, peito 4, pernas 3, pés 2): o peito (vizinho cabeça, 5 > 4) esconde as suas peças compartilhadas com a cabeça vestida (o que parece um
    // engano do original, porque as compartilhadas são entre peito e pernas), as pernas (vizinho pés, 2 > 3 é falso) nunca, os pés (vizinho pernas, 3 > 2) com as pernas vestidas.
    const HELMET_PARTS: [Attachment; 2] = [
        suit(
            HEAD,
            "mekanism:entity/mekasuit/head_head_opaque",
            false,
            None,
        ),
        suit(
            HEAD,
            "mekanism:entity/mekasuit/head_head_emissive",
            true,
            None,
        ),
    ];
    const BODYARMOR_PARTS: [Attachment; 7] = [
        suit(
            BODY,
            "mekanism:entity/mekasuit/chest_body_opaque",
            false,
            None,
        ),
        suit(
            BODY,
            "mekanism:entity/mekasuit/chest_body_opaque_shared",
            false,
            Some(Slot::Head),
        ),
        suit(
            BODY,
            "mekanism:entity/mekasuit/chest_body_emissive",
            true,
            None,
        ),
        suit(
            LEFT_ARM,
            "mekanism:entity/mekasuit/chest_left_arm_opaque",
            false,
            None,
        ),
        suit(
            LEFT_ARM,
            "mekanism:entity/mekasuit/chest_left_arm_emissive",
            true,
            None,
        ),
        suit(
            RIGHT_ARM,
            "mekanism:entity/mekasuit/chest_right_arm_opaque",
            false,
            None,
        ),
        suit(
            RIGHT_ARM,
            "mekanism:entity/mekasuit/chest_right_arm_emissive",
            true,
            None,
        ),
    ];
    const PANTS_PARTS: [Attachment; 7] = [
        suit(
            BODY,
            "mekanism:entity/mekasuit/legs_body_opaque_shared",
            false,
            None,
        ),
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/legs_left_leg_opaque",
            false,
            None,
        ),
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/legs_left_leg_opaque_shared",
            false,
            None,
        ),
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/legs_left_leg_emissive",
            true,
            None,
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/legs_right_leg_opaque",
            false,
            None,
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/legs_right_leg_opaque_shared",
            false,
            None,
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/legs_right_leg_emissive",
            true,
            None,
        ),
    ];
    const BOOTS_PARTS: [Attachment; 6] = [
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/feet_left_leg_opaque",
            false,
            None,
        ),
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/feet_left_leg_opaque_excl",
            false,
            Some(Slot::Legs),
        ),
        suit(
            LEFT_LEG,
            "mekanism:entity/mekasuit/feet_left_leg_opaque_shared",
            false,
            Some(Slot::Legs),
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/feet_right_leg_opaque",
            false,
            None,
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/feet_right_leg_opaque_excl",
            false,
            Some(Slot::Legs),
        ),
        suit(
            RIGHT_LEG,
            "mekanism:entity/mekasuit/feet_right_leg_opaque_shared",
            false,
            Some(Slot::Legs),
        ),
    ];

    const JETPACK_PARTS: [Attachment; 1] = [jetpack("mekanism:item/jetpack")];
    const JETPACK_ARMORED_PARTS: [Attachment; 1] = [jetpack("mekanism:item/jetpack_armored")];
    const SCUBA_TANK_PARTS: [Attachment; 1] = [SCUBA_TANK_MODEL];
    const FREE_RUNNERS_PARTS: [Attachment; 2] = [
        runner!("left_leg", "mekanism:item/left_free_runner", -0.65),
        runner!("right_leg", "mekanism:item/right_free_runner", -0.375),
    ];
    const FREE_RUNNERS_ARMORED_PARTS: [Attachment; 2] = [
        runner!("left_leg", "mekanism:item/left_free_runner_armored", -0.65),
        runner!(
            "right_leg",
            "mekanism:item/right_free_runner_armored",
            -0.375
        ),
    ];

    const fn gear(
        item: &'static str,
        family: &'static str,
        slot: Slot,
        hides: &'static [&'static str],
        hides_cape: bool,
        foil: Foil,
        attachments: &'static [Attachment],
    ) -> Gear {
        Gear {
            item,
            family,
            slot,
            hides,
            hides_cape,
            foil,
            attachments,
        }
    }

    /// Os itens vestidos que o Mekanism desenha por código. A capa some com o peito do MekaSuit, o jetpack e o tanque (`avatarState.showCape = false`).
    pub const WORN_GEAR: [Gear; 9] = [
        gear(
            "mekanism:mekasuit_helmet",
            SUIT,
            Slot::Head,
            SUIT_HIDES_HEAD,
            false,
            Foil::Armor,
            &HELMET_PARTS,
        ),
        gear(
            "mekanism:mekasuit_bodyarmor",
            SUIT,
            Slot::Chest,
            SUIT_HIDES_CHEST,
            true,
            Foil::Armor,
            &BODYARMOR_PARTS,
        ),
        gear(
            "mekanism:mekasuit_pants",
            SUIT,
            Slot::Legs,
            SUIT_HIDES_LEGS,
            false,
            Foil::Armor,
            &PANTS_PARTS,
        ),
        gear(
            "mekanism:mekasuit_boots",
            SUIT,
            Slot::Feet,
            &[],
            false,
            Foil::Armor,
            &BOOTS_PARTS,
        ),
        gear(
            "mekanism:jetpack",
            "jetpack",
            Slot::Chest,
            &[],
            true,
            Foil::Item,
            &JETPACK_PARTS,
        ),
        gear(
            "mekanism:jetpack_armored",
            "jetpack",
            Slot::Chest,
            &[],
            true,
            Foil::Item,
            &JETPACK_ARMORED_PARTS,
        ),
        gear(
            "mekanism:scuba_tank",
            "scuba",
            Slot::Chest,
            &[],
            true,
            Foil::Item,
            &SCUBA_TANK_PARTS,
        ),
        gear(
            "mekanism:free_runners",
            "free_runners",
            Slot::Feet,
            &[],
            false,
            Foil::Item,
            &FREE_RUNNERS_PARTS,
        ),
        gear(
            "mekanism:free_runners_armored",
            "free_runners",
            Slot::Feet,
            &[],
            false,
            Foil::Item,
            &FREE_RUNNERS_ARMORED_PARTS,
        ),
    ];

    /// Onde o atlas de itens do original põe cada sprite dos modelos vestidos (`rastros/armor_atlas.json`, do `CenaEquip`): o brilho de encantamento amostra a textura do brilho com a UV0 do quad
    /// no atlas, e o atlas do Horizon põe os sprites em outro lugar.
    pub const ATLAS_SPRITES: [SpriteRect; 7] = [
        SpriteRect {
            name: "entity/armor/mekasuit_armor_body",
            rect: [0.06542969, 0.25585938, 0.09667969, 0.31835938],
        },
        SpriteRect {
            name: "entity/armor/mekasuit_armor_exoskeleton",
            rect: [0.06542969, 0.38476562, 0.09667969, 0.44726562],
        },
        SpriteRect {
            name: "entity/armor/mekasuit_armor_helmet",
            rect: [0.12792969, 0.13085938, 0.15917969, 0.19335938],
        },
        SpriteRect {
            name: "entity/armor/mekasuit_player",
            rect: [0.12792969, 0.39648438, 0.15917969, 0.45898438],
        },
        SpriteRect {
            name: "item/free_runners",
            rect: [0.19238281, 0.26757812, 0.22363281, 0.33007812],
        },
        SpriteRect {
            name: "item/jetpack",
            rect: [0.0009765625, 0.25585938, 0.06347656, 0.38085938],
        },
        SpriteRect {
            name: "item/scuba_tank",
            rect: [0.25097656, 0.40039062, 0.28222656, 0.46289062],
        },
    ];

    /// Os modelos avulsos que o jogo tem de assar (`MekanismModelCache`: `MEKASUIT`, as quatro botas) e os que as peças citam: o que `native_standalone_models` recebe.
    pub fn models() -> Vec<&'static str> {
        let mut models: Vec<&'static str> = Vec::new();
        for gear in &WORN_GEAR {
            for attachment in gear.attachments {
                if !models.contains(&attachment.model) {
                    models.push(attachment.model);
                }
            }
        }
        models
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O Configurator (`ItemConfigurator`) e o cartão de configuração (`ItemConfigurationCard`).

/// `MekanismItems.CONFIGURATOR` e `CONFIGURATION_CARD`.
pub const CONFIGURATOR_ITEM: &str = "mekanism:configurator";
pub const CONFIGURATION_CARD_ITEM: &str = "mekanism:configuration_card";
pub const NETWORK_READER_ITEM: &str = "mekanism:network_reader";
/// `MekanismDataComponents.CONFIGURATOR_MODE` (o nome serializado do `ConfiguratorMode`) e `CONFIGURATION_DATA` (o `CompoundTag` do cartão).
pub const CONFIGURATOR_MODE_COMPONENT: &str = "mekanism:configurator_mode";
pub const CONFIGURATION_DATA_COMPONENT: &str = "mekanism:configuration_data";

/// `EnumColor`, o RGB da cor primária de cada uma na ordem do `ordinal` (`EnumColor.java:27-46`); os nomes estão em `side_config::ENUM_COLOR_NAMES`.
pub const ENUM_COLOR_RGB: [u32; 18] = [
    0x40_40_40, 0x36_6B_D0, 0x59_C1_5F, 0x00_F3_D0, 0xC9_07_1F, 0xA4_60_D9, 0xFF_A1_60, 0xCF_CF_CF,
    0x7A_7A_7A, 0x55_9E_FF, 0x75_FF_89, 0x30_FF_F9, 0xFF_38_3C, 0xD5_5E_CB, 0xFF_DD_4F, 0xFF_FF_FF,
    0xA1_76_49, 0xFF_BC_C4,
];
/// Os `ordinal` de `EnumColor` que o Configurator usa: `BRIGHT_GREEN`, `DARK_RED`, `YELLOW`, `PINK` e a cor do texto de `GRAY`.
pub const COLOR_BRIGHT_GREEN: u8 = 10;
pub const COLOR_DARK_RED: u8 = 4;
/// `EnumColor.RED`.
const COLOR_RED: u8 = 5;
pub const COLOR_YELLOW: u8 = 14;
pub const COLOR_PINK: u8 = 13;
pub const COLOR_GRAY: u8 = 7;

/// `ItemConfigurator.ConfiguratorMode`, na ordem do `ordinal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfiguratorMode {
    ConfigurateItems,
    ConfigurateFluids,
    ConfigurateChemicals,
    ConfigurateEnergy,
    ConfigurateHeat,
    Empty,
    Rotate,
    Wrench,
}

impl ConfiguratorMode {
    /// `VALUES`.
    pub const VALUES: [ConfiguratorMode; 8] = [
        ConfiguratorMode::ConfigurateItems,
        ConfiguratorMode::ConfigurateFluids,
        ConfiguratorMode::ConfigurateChemicals,
        ConfiguratorMode::ConfigurateEnergy,
        ConfiguratorMode::ConfigurateHeat,
        ConfiguratorMode::Empty,
        ConfiguratorMode::Rotate,
        ConfiguratorMode::Wrench,
    ];
    /// `getDefaultMode`.
    pub const DEFAULT: ConfiguratorMode = ConfiguratorMode::ConfigurateItems;

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `getSerializedName`: `name().toLowerCase(Locale.ROOT)`.
    pub fn serialized_name(self) -> &'static str {
        match self {
            ConfiguratorMode::ConfigurateItems => "configurate_items",
            ConfiguratorMode::ConfigurateFluids => "configurate_fluids",
            ConfiguratorMode::ConfigurateChemicals => "configurate_chemicals",
            ConfiguratorMode::ConfigurateEnergy => "configurate_energy",
            ConfiguratorMode::ConfigurateHeat => "configurate_heat",
            ConfiguratorMode::Empty => "empty",
            ConfiguratorMode::Rotate => "rotate",
            ConfiguratorMode::Wrench => "wrench",
        }
    }

    /// `CODEC`: um nome que não existe não decodifica.
    pub fn from_serialized_name(name: &str) -> Option<ConfiguratorMode> {
        Self::VALUES
            .into_iter()
            .find(|mode| mode.serialized_name() == name)
    }

    /// `isConfigurating`.
    pub fn is_configurating(self) -> bool {
        self.transmission().is_some()
    }

    /// `getTransmission`.
    pub fn transmission(self) -> Option<TransmissionType> {
        match self {
            ConfiguratorMode::ConfigurateItems => Some(TransmissionType::Item),
            ConfiguratorMode::ConfigurateFluids => Some(TransmissionType::Fluid),
            ConfiguratorMode::ConfigurateChemicals => Some(TransmissionType::Chemical),
            ConfiguratorMode::ConfigurateEnergy => Some(TransmissionType::Energy),
            ConfiguratorMode::ConfigurateHeat => Some(TransmissionType::Heat),
            ConfiguratorMode::Empty | ConfiguratorMode::Rotate | ConfiguratorMode::Wrench => None,
        }
    }

    /// `color()`: o `ordinal` do `EnumColor`.
    pub fn color(self) -> u8 {
        match self {
            ConfiguratorMode::Empty => COLOR_DARK_RED,
            ConfiguratorMode::Rotate => COLOR_YELLOW,
            ConfiguratorMode::Wrench => COLOR_PINK,
            _ => COLOR_BRIGHT_GREEN,
        }
    }

    /// `ItemConfigurator.canPerformAction`: as nove `MekanismItemAbilities` (pelo nome em minúsculas) que a pilha neste modo responde.
    pub fn abilities(self) -> std::collections::BTreeMap<&'static str, bool> {
        let configures = |kind: TransmissionType| self.transmission() == Some(kind);
        std::collections::BTreeMap::from([
            ("wrench_configure", self.is_configurating()),
            (
                "wrench_configure_chemicals",
                configures(TransmissionType::Chemical),
            ),
            (
                "wrench_configure_energy",
                configures(TransmissionType::Energy),
            ),
            (
                "wrench_configure_fluids",
                configures(TransmissionType::Fluid),
            ),
            ("wrench_configure_heat", configures(TransmissionType::Heat)),
            ("wrench_configure_items", configures(TransmissionType::Item)),
            ("wrench_dismantle", self == ConfiguratorMode::Wrench),
            ("wrench_empty", self == ConfiguratorMode::Empty),
            ("wrench_rotate", self == ConfiguratorMode::Rotate),
        ])
    }

    /// `doesSneakBypassUse`: só a chave inglesa deixa o bloco agir com o jogador agachado.
    pub fn sneak_bypasses_use(self) -> bool {
        self == ConfiguratorMode::Wrench
    }

    /// `IIncrementalEnum.adjust`: `byIndexWrapping(ordinal() + shift)`, e o próprio modo se o deslocamento é 0.
    pub fn adjust(self, shift: i32) -> ConfiguratorMode {
        if shift == 0 {
            return self;
        }
        let count = Self::VALUES.len() as i64;
        Self::VALUES[(self.ordinal() as i64 + i64::from(shift)).rem_euclid(count) as usize]
    }
}

/// Um `Component` de tradução com argumentos e a cor do estilo: o que as mensagens e o tooltip do Configurator e do cartão levam, no formato que o
/// `ComponentSerialization.CODEC` grava (`translate`, `with`, `color`). `color` é o RGB, sem alfa.
#[derive(Clone, Debug, PartialEq)]
pub struct Chat {
    pub translate: String,
    pub with: Vec<Chat>,
    pub color: Option<u32>,
    /// Um texto literal (`Component.literal`, o `text` do JSON) no lugar da tradução: os números de um tooltip.
    pub literal: Option<String>,
}

impl Chat {
    fn new(translate: &str, with: Vec<Chat>, color: Option<u32>) -> Chat {
        Chat {
            translate: translate.to_owned(),
            with,
            color,
            literal: None,
        }
    }

    /// `TextComponentUtil.build(color, texto)`: um literal na cor `rgb`.
    fn literal(text: &str, rgb: u32) -> Chat {
        Chat {
            translate: String::new(),
            with: Vec::new(),
            color: Some(rgb),
            literal: Some(text.to_owned()),
        }
    }

    /// `translate` sem argumentos, na cor `color` (o `ordinal` de `EnumColor`).
    fn plain(translate: &str, color: Option<u8>) -> Chat {
        Chat::new(translate, Vec::new(), color.map(enum_color_rgb))
    }

    /// O JSON de componente, como o Java o grava.
    pub fn to_json(&self) -> serde_json::Value {
        let mut json = match &self.literal {
            Some(text) => serde_json::json!({ "text": text }),
            None => serde_json::json!({ "translate": self.translate }),
        };
        if !self.with.is_empty() {
            json["with"] = serde_json::Value::Array(self.with.iter().map(Chat::to_json).collect());
        }
        if let Some(color) = self.color {
            json["color"] = serde_json::Value::String(format!("#{color:06X}"));
        }
        json
    }
}

/// `EnumColor.getRgb` pelo `ordinal`.
pub fn enum_color_rgb(color: u8) -> u32 {
    ENUM_COLOR_RGB[usize::from(color)]
}

/// `EnumColor.getColoredName`: `color.mekanism.<prefixo>` na própria cor.
fn colored_name(color: u8) -> Chat {
    Chat::plain(
        &format!(
            "color.mekanism.{}",
            machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(color)]
        ),
        Some(color),
    )
}

/// `TransmissionType.getLangEntry`: `transmission.mekanism.<transmission>`.
fn transmission_name(kind: TransmissionType) -> Chat {
    Chat::new(
        &format!("transmission.mekanism.{}", kind.serialized_name()),
        Vec::new(),
        None,
    )
}

impl ConfiguratorMode {
    /// `getTextComponent`: o nome do modo na cor dele; o de configurar leva o tipo de transmissão como argumento.
    pub fn text(self) -> Chat {
        let color = Some(enum_color_rgb(self.color()));
        match self.transmission() {
            Some(kind) => Chat::new(
                "configurator.mekanism.configurate",
                vec![transmission_name(kind)],
                color,
            ),
            None => Chat::new(
                match self {
                    ConfiguratorMode::Empty => "configurator.mekanism.empty",
                    ConfiguratorMode::Rotate => "configurator.mekanism.rotate",
                    _ => "configurator.mekanism.wrench",
                },
                Vec::new(),
                color,
            ),
        }
    }

    /// `addToTooltip` (o appender de `CONFIGURATOR_MODE`): `MekanismLang.STATE.translateColored(EnumColor.PINK, this)`.
    pub fn tooltip_line(self) -> Chat {
        Chat::new(
            "configurator.mekanism.state",
            vec![self.text()],
            Some(enum_color_rgb(COLOR_PINK)),
        )
    }

    /// `changeMode` -> `DisplayChange.sendMessage`: `MekanismLang.CONFIGURE_STATE.translate(newMode)`, sem cor própria.
    pub fn configure_state_message(self) -> Chat {
        Chat::new(
            "configurator.mekanism.configure_state",
            vec![self.text()],
            None,
        )
    }
}

/// `CONFIGURATOR_VIEW_MODE` (sem agachar) e `CONFIGURATOR_TOGGLE_MODE` (agachado): `translateColored(GRAY, transmissionType, dataType.getColor(),
/// dataType, dataType.getColor().getColoredName())`, a cor do tipo de dado valendo para os dois últimos argumentos.
pub fn configurator_side_message(
    toggled: bool,
    kind: TransmissionType,
    data_type: DataType,
) -> Chat {
    let color = data_type.color();
    let data = Chat::plain(
        &format!("side_data.mekanism.{}", data_type.serialized_name()),
        Some(color),
    );
    Chat::new(
        if toggled {
            "configurator.mekanism.toggle_mode"
        } else {
            "configurator.mekanism.view_mode"
        },
        vec![transmission_name(kind), data, colored_name(color)],
        Some(enum_color_rgb(COLOR_GRAY)),
    )
}

/// `LogisticalTransporter.getColorName`: o nome da cor na própria cor, ou `NONE` em branco.
fn transporter_color_name(color: Option<u8>) -> Chat {
    match color {
        Some(color) => colored_name(color),
        None => Chat::plain("gui.mekanism.none", Some(15)),
    }
}

/// As mensagens que o cabo manda ao jogador quando o Configurator mexe nele (`TileEntityTransmitter.onSneakRightClick`, `Transmitter.onRightClick`):
/// `CONNECTION_TYPE.translateColored(GRAY, connectionType)` e `REDSTONE_SENSITIVITY.translateColored(GRAY, INDIGO, OnOff.of(redstoneReactive))`. O tipo de conexão
/// (`ConnectionType.getTextComponent`) leva a cor dele: `ORANGE`, `BRIGHT_GREEN`, `YELLOW` e `WHITE`.
pub fn cable_overlay_message(
    overlay: &machinery_basis_core::transmission::transmitter::Overlay,
) -> Chat {
    use machinery_basis_core::transmission::connection::ConnectionType;
    use machinery_basis_core::transmission::transmitter::Overlay;
    const COLOR_ORANGE: u8 = 6;
    const COLOR_WHITE: u8 = 15;
    match overlay {
        Overlay::ConnectionType { connection } => {
            let color = match connection {
                ConnectionType::Normal => COLOR_ORANGE,
                ConnectionType::Push => COLOR_BRIGHT_GREEN,
                ConnectionType::Pull => COLOR_YELLOW,
                ConnectionType::None => COLOR_WHITE,
            };
            Chat::new(
                "configurator.mekanism.mode_change",
                vec![Chat::plain(
                    &format!("connection.mekanism.{}", connection.serialized_name()),
                    Some(color),
                )],
                Some(enum_color_rgb(COLOR_GRAY)),
            )
        }
        // `LogisticalTransporter.onConfigure` e `onRightClick`: `TOGGLE_COLOR.translateColored(GRAY, getColorName())` e `CURRENT_COLOR.translateColored(GRAY, getColorName())`; o
        // nome é o da cor na própria cor, ou `NONE` em branco.
        Overlay::ToggleColor { color } => Chat::new(
            "configurator.mekanism.toggle_color",
            vec![transporter_color_name(*color)],
            Some(enum_color_rgb(COLOR_GRAY)),
        ),
        Overlay::CurrentColor { color } => Chat::new(
            "configurator.mekanism.view_color",
            vec![transporter_color_name(*color)],
            Some(enum_color_rgb(COLOR_GRAY)),
        ),
        // `DiversionTransporter.onRightClick`: `TOGGLE_DIVERTER.translateColored(GRAY, RED, newMode)`, com o texto do modo (`control.<modo>.desc`) em vermelho.
        Overlay::ToggleDiverter { mode } => {
            use machinery_basis_core::transporter::node::DiversionControl;
            let name = match mode {
                DiversionControl::Disabled => "disabled",
                DiversionControl::High => "high",
                DiversionControl::Low => "low",
            };
            Chat::new(
                "transmitter.mekanism.configurator.toggle_diverter",
                vec![Chat::plain(
                    &format!("transmitter.mekanism.control.{name}.desc"),
                    Some(COLOR_RED),
                )],
                Some(enum_color_rgb(COLOR_GRAY)),
            )
        }
        Overlay::RedstoneSensitivity { on } => Chat::new(
            "configurator.mekanism.redstone_sensitivity",
            vec![Chat::plain(
                if *on {
                    "boolean.mekanism.on"
                } else {
                    "boolean.mekanism.off"
                },
                Some(COLOR_INDIGO),
            )],
            Some(enum_color_rgb(COLOR_GRAY)),
        ),
    }
}

/// O que a mão principal segura no clique de um bloco de Mekanism, para `tile_use`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldItem {
    /// Nenhuma mão (principal e secundária) segura nada.
    Hands,
    /// O Configurator, no modo da pilha.
    Configurator(ConfiguratorMode),
    /// O cartão de configuração.
    Card,
    /// Qualquer outra coisa (ou só a mão secundária cheia).
    Other,
}

/// O que o bloco de Mekanism (`TileEntityMekanism`) faz no clique da mão principal antes de o item agir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileUse {
    /// `BlockTile.useItemOn` -> `tryWrenchRotate`: o bloco gira e responde `SUCCESS` (o balanço é do cliente e do servidor).
    Rotate,
    /// `TileEntityMekanism.openGui` devolve `TRY_WITH_EMPTY_HAND` (o cartão, e o Configurator em `ROTATE`): a tela não abre e `Item.useOn` tem o clique.
    PassToItem,
    /// `openGui` abre a tela (`CONSUME`, sem balanço).
    Menu,
    /// O bloco não abre a tela: agachado, `openGui` passa (`!player.isShiftKeyDown()`), e com algo numa das mãos o bloco nem chega a agir (`doesSneakBypassUse`).
    Nothing,
}

/// `BlockTile.useItemOn` seguido de `TileEntityMekanism.openGui` (`TileEntityMekanism.java:565-587`) para um bloco que gira e tem tela, no turno da mão principal.
pub fn tile_use(held: HeldItem, sneaking: bool) -> TileUse {
    if sneaking {
        return TileUse::Nothing;
    }
    match held {
        HeldItem::Configurator(ConfiguratorMode::Rotate) => TileUse::Rotate,
        HeldItem::Card => TileUse::PassToItem,
        HeldItem::Hands | HeldItem::Other | HeldItem::Configurator(_) => TileUse::Menu,
    }
}

/// `ItemConfigurator.useOn` -> `MekanismUtils.rotate(orientation, supportY)`.
pub fn rotate_facing(facing: Face, support_y: bool) -> Face {
    if support_y {
        return match facing {
            Face::Up => Face::North,
            Face::Down => Face::South,
            Face::North => Face::East,
            Face::South => Face::West,
            Face::West => Face::Up,
            Face::East => Face::Down,
        };
    }
    match facing {
        Face::North => Face::East,
        Face::East => Face::South,
        Face::South => Face::West,
        Face::West => Face::North,
        other => other,
    }
}

/// `IModeItem.DisplayChange`: o que acontece depois de trocar o modo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayChange {
    None,
    /// `displayModeChange`: o jogador recebe `showModeChange` e o cliente mostra o texto do modo sobre a barra.
    MainHand,
    /// `player.sendOverlayMessage(CONFIGURE_STATE)`.
    Other,
}

/// O que `ItemConfigurator.changeMode` faz: o modo novo, se mudou (e então o componente é escrito), a mensagem de overlay e se o cliente mostra o
/// texto do modo.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeChange {
    pub mode: ConfiguratorMode,
    pub changed: bool,
    pub overlay: Option<Chat>,
    pub show: bool,
}

/// `changeMode(player, itemAccess, shift, displayChange, transaction)`.
pub fn change_mode(mode: ConfiguratorMode, shift: i32, display: DisplayChange) -> ModeChange {
    let new = mode.adjust(shift);
    let changed = new != mode;
    ModeChange {
        mode: new,
        changed,
        overlay: (changed && display == DisplayChange::Other)
            .then(|| new.configure_state_message()),
        show: changed && display == DisplayChange::MainHand,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O item dos tanques: o modo balde do Fluid Tank e as linhas do tooltip dos dez tanques.

/// `MekanismLang.BUCKET_MODE`.
pub const BUCKET_MODE_LANG: &str = "tooltip.mekanism.bucket_mode";
/// `EnumColor.getRgb` das cores que o tooltip dos tanques usa, como o `ComponentSerialization.CODEC` as grava no rastro `tank_fluid_item_use`.
const TIP_GRAY: u32 = 0xCFCFCF;
const TIP_DARK_RED: u32 = 0xC9071F;
const TIP_PINK: u32 = 0xD55ECB;
const TIP_ORANGE: u32 = 0xFFA160;
const TIP_AQUA: u32 = 0x30FFF9;
const TIP_INDIGO: u32 = 0x559EFF;
const TIP_BRIGHT_GREEN: u32 = 0x75FF89;
const TIP_RED: u32 = 0xFF383C;
/// `BaseTier.getColor` dos cinco tiers (`IStorageTier.getTextColor`).
pub const TANK_TIER_COLORS: [u32; 5] = [0x5FFFB8, 0xFF806A, 0x4BF8FF, 0xF787FF, 0x585858];

/// O que `ItemBlockFluidTank.changeMode` faz (`:186-195`): um `shift` ímpar inverte o modo, um par não muda nada. `overlay` é a mensagem do `DisplayChange.OTHER`
/// (`BUCKET_MODE.translate(OnOff.of(novo, true))`) e `show` o aviso do `MAIN_HAND` (o texto do modo sobre a barra).
#[derive(Clone, Debug, PartialEq)]
pub struct BucketModeChange {
    pub mode: bool,
    pub changed: bool,
    pub overlay: Option<Chat>,
    pub show: bool,
}

/// `OnOff.of(valor, true)`: `boolean.mekanism.on` em verde ou `off` em vermelho.
fn on_off_colored(value: bool) -> Chat {
    if value {
        Chat::new("boolean.mekanism.on", Vec::new(), Some(TIP_BRIGHT_GREEN))
    } else {
        Chat::new("boolean.mekanism.off", Vec::new(), Some(TIP_RED))
    }
}

pub fn bucket_mode_change(mode: bool, shift: i32, display: DisplayChange) -> BucketModeChange {
    if shift.unsigned_abs() % 2 != 1 {
        return BucketModeChange {
            mode,
            changed: false,
            overlay: None,
            show: false,
        };
    }
    let new = !mode;
    BucketModeChange {
        mode: new,
        changed: true,
        overlay: (display == DisplayChange::Other)
            .then(|| Chat::new(BUCKET_MODE_LANG, vec![on_off_colored(new)], None)),
        show: display == DisplayChange::MainHand,
    }
}

/// `ItemBlockFluidTank.getScrollTextComponent`: `BUCKET_MODE.translateColored(GRAY, OnOff.of(modo, true))`.
pub fn bucket_mode_scroll_text(mode: bool) -> Chat {
    Chat::new(BUCKET_MODE_LANG, vec![on_off_colored(mode)], Some(TIP_GRAY))
}

/// O que o tooltip de um item de tanque mostra entre a linha da segurança e a descrição.
#[derive(Clone, Debug, PartialEq)]
pub struct TankTooltip<'a> {
    /// `true` no Chemical Tank (`ATTACHED_CHEMICALS`, laranja), `false` no Fluid Tank (`ATTACHED_FLUIDS`, rosa).
    pub chemical: bool,
    /// O tier, de 0 (basic) a 4 (creative).
    pub tier: usize,
    /// O id do que o item guarda e quanto; `None` é o contêiner vazio.
    pub stored: Option<(&'a str, u64)>,
    /// `IStorageTier.getCapacity` de agora.
    pub capacity: u64,
    /// `ATTACHED_ITEMS` com alguma pilha.
    pub inventory: bool,
    /// `mekanism:bucket_mode`, que só o Fluid Tank tem.
    pub bucket_mode: Option<bool>,
}

/// As linhas dos appenders de `MekanismDataComponents` para um item de tanque, na ordem em que saem (`ATTACHED_CHEMICALS` ou `ATTACHED_FLUIDS`, o tier, `ATTACHED_ITEMS` e
/// `BUCKET_MODE`): `ResourceContainerType.addStoredResource` (`:140-177`), `IStorageTier.addCapacityTooltip` (`:27-33`), `HAS_INVENTORY` e `BUCKET_MODE` com `YesNo`.
pub fn tank_tooltip_lines(tip: &TankTooltip<'_>) -> Vec<Chat> {
    let creative = tip.tier == TANK_CREATIVE;
    let infinite = || Chat::new("gui.mekanism.infinite", Vec::new(), Some(TIP_GRAY));
    let mut lines = Vec::new();
    match tip.stored {
        Some((id, amount)) => {
            let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let kind = if tip.chemical { "chemical" } else { "block" };
            let name = Chat::new(
                &format!("{kind}.{namespace}.{}", path.replace('/', ".")),
                Vec::new(),
                None,
            );
            let color = Some(if tip.chemical { TIP_ORANGE } else { TIP_PINK });
            lines.push(if creative {
                Chat::new("generic.mekanism.stored", vec![name, infinite()], color)
            } else {
                Chat::new(
                    STORED_MB_LANG,
                    vec![
                        name,
                        Chat::literal(&machinery_basis_core::units::grouped(amount), TIP_GRAY),
                    ],
                    color,
                )
            });
        }
        None => lines.push(Chat::new(
            if tip.chemical {
                "tooltip.mekanism.no_chemical"
            } else {
                "tooltip.mekanism.no_fluid"
            },
            Vec::new(),
            Some(TIP_DARK_RED),
        )),
    }
    let tier_color = Some(TANK_TIER_COLORS[tip.tier.min(TANK_CREATIVE)]);
    lines.push(if creative {
        Chat::new("capacity.mekanism.generic", vec![infinite()], tier_color)
    } else {
        Chat::new(
            "capacity.mekanism.mb",
            vec![Chat::literal(
                &machinery_basis_core::units::grouped(tip.capacity),
                TIP_GRAY,
            )],
            tier_color,
        )
    });
    lines.push(Chat::new(
        "tooltip.mekanism.inventory",
        vec![Chat::new(
            if tip.inventory {
                "boolean.mekanism.yes"
            } else {
                "boolean.mekanism.no"
            },
            Vec::new(),
            Some(TIP_GRAY),
        )],
        Some(TIP_AQUA),
    ));
    if let Some(mode) = tip.bucket_mode {
        let answer = if mode {
            Chat::new("boolean.mekanism.yes", Vec::new(), Some(TIP_BRIGHT_GREEN))
        } else {
            Chat::new("boolean.mekanism.no", Vec::new(), Some(TIP_RED))
        };
        lines.push(Chat::new(BUCKET_MODE_LANG, vec![answer], Some(TIP_INDIGO)));
    }
    lines
}

/// `BlockMekanism.canBeReplaced(state, fluid)` (falso) contra o balde (`BucketItem.emptyContents`: `blockState.canBeReplaced(this.content)`): o balde cheio que não derrama no bloco do Mekanism que o
/// Horizon deixaria o fluido trocar. O Horizon troca um bloco por fluido quando ele é substituível ou não sólido (`mayReplace`), e o gabarito `chorus_plant` dos cabos e transmissores não é sólido
/// (`facts.json`, bit `SOLID` zerado); o gabarito `lodestone` e os outros dos blocos do Mekanism são sólidos, e o Horizon já os recusa como o original (decisão 22 do plano). `bucket` é o item na mão.
pub fn bucket_refused_by(bucket: &str, block: &str) -> bool {
    // Um balde que não derrama fluido: o vazio, o de leite (`Item` comum) e o de neve em pó (`SolidBucketItem`, que põe um bloco: `canBeReplaced(context)`, não o do fluido).
    const NOT_POURING: [&str; 3] = [
        "minecraft:bucket",
        "minecraft:milk_bucket",
        "minecraft:powder_snow_bucket",
    ];
    bucket.ends_with("_bucket")
        && !NOT_POURING.contains(&bucket)
        && transmitter_center(block).is_some()
}

/// `BucketItem.use` pergunta ao gancho `fill_bucket` pelo bloco que recebe o fluido (o `target` de `Game::bucket_target`: o bloco clicado, se é um contêiner de água, senão o ao lado) e, quando o
/// clicado recebe, também pelo bloco ao lado (`player.mayUseItemAt(blockPos2, ...)`, uma conferência de permissão que não passa fluido por ele). O original só consulta `canBeReplaced(state, fluid)` do
/// bloco que recebe (`BucketItem.emptyContents`); então um transmissor ao lado de uma laje ou escada que se enche de água não recusa o balde. `target` é o que a injeção `bucket_target` anotou
/// (`None`: nenhum clique visto, nada a recusar).
pub fn bucket_refused_at(
    bucket: &str,
    block: &str,
    pos: [i32; 3],
    target: Option<[i32; 3]>,
) -> bool {
    target == Some(pos) && bucket_refused_by(bucket, block)
}

/// Se o bloco é um transmissor do Mekanism: `Some(false)` nos pequenos (`BlockSmallTransmitter`), `Some(true)` nos grandes (`BlockLargeTransmitter`).
fn transmitter_is_large(block: &str) -> Option<bool> {
    let name = block.strip_prefix("mekanism:")?;
    let small = [
        "universal_cable",
        "pressurized_tube",
        "thermodynamic_conductor",
    ]
    .iter()
    .any(|kind| name.ends_with(kind));
    let large = [
        "mechanical_pipe",
        "logistical_transporter",
        "restrictive_transporter",
        "diversion_transporter",
    ]
    .iter()
    .any(|kind| name.ends_with(kind));
    if small {
        Some(false)
    } else if large {
        Some(true)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O conteúdo animado dos transmissores (frente `cabo-conteudo`, Fase 4): o que cada fluido e cada químico mostram (`MekanismRenderer.getFluidTexture`, `getColorARGB`).

/// O prefixo dos 24 modelos avulsos do conteúdo (`TransmitterContentsManager`, `<prefixo>/<lado><TIPO>`).
pub const CONTENTS_MODEL_PREFIX: &str = "mekanism:block/transmitter_contents";
/// `MekanismRenderer.ENERGY_ICON_LOCATION` e `RenderThermodynamicConductor.HEAT_ICON_LOCATION`.
pub const ENERGY_SPRITE: &str = "mekanism:mek_liquid/energy";
pub const HEAT_SPRITE: &str = "mekanism:mek_liquid/heat";
/// O sprite sem cor própria dos fluidos e dos químicos (`liquid.png`, que o original tinge no vértice).
pub const LIQUID_SPRITE: &str = "mekanism:mek_liquid/liquid";
/// Os fluidos do Mekanism com textura própria já tingida no pixel (`textures/mek_liquid/<nome>.png`: o Horizon não tinge o modelo, ver `test_texturas_fluidos.py`); os outros usam
/// `liquid.png` com a cor do fluido nos vértices.
pub const FLUIDS_WITH_OWN_TEXTURE: [&str; 16] = [
    "brine",
    "chlorine",
    "ethene",
    "hydrofluoric_acid",
    "hydrogen_chloride",
    "lithium",
    "nutritional_paste",
    "oxygen",
    "sodium",
    "steam",
    "sulfur_dioxide",
    "sulfur_trioxide",
    "sulfuric_acid",
    "superheated_sodium",
    "uranium_hexafluoride",
    "uranium_oxide",
];
/// Os quatro fluidos do Mekanism Generators, todos com textura própria já tingida (`mekanismgenerators:mek_liquid/<nome>`).
pub const GENERATORS_FLUIDS: [&str; 4] = ["bioethanol", "deuterium", "fusion_fuel", "tritium"];
/// `MekanismUtils.lighterThanAirGas`: o fluido da tag `c:gaseous` com `isLighterThanAir`, só o vapor.
pub const LIGHTER_THAN_AIR_FLUIDS: [&str; 1] = ["mekanism:steam"];

/// A água pesada, o único fluido do Mekanism com a textura de outro (`MekanismFluids.java:42-44`).
pub const HEAVY_WATER: &str = "mekanism:heavy_water";

/// `getFluidTexture(fluid, FLOWING)` a partir do sprite parado: a água e a lava do vanilla (`block/<nome>_still` e `block/<nome>_flow`) e os fluidos do Mekanism
/// (`mek_liquid/<nome>` e `mek_liquid/<nome>_flow`: `FluidDeferredRegister.LIQUID_FLOW`, o vapor em `MekanismFluids.java:41`, e as texturas próprias do port, que têm as duas).
pub fn flowing_sprite(still: &str) -> String {
    match still.strip_suffix("_still") {
        Some(base) => format!("{base}_flow"),
        None => format!("{still}_flow"),
    }
}

/// O fluido de um cano como o desenho o lê.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentsFluid {
    pub sprite: String,
    pub rgb: u32,
    pub light: u32,
    pub lighter_than_air: bool,
}

/// `FluidTintSources.water().color(FluidState)`, a tinta da água do `FluidModel` (sem o alfa).
pub const WATER_TINT: u32 = 0x3F_76_E4;

/// `getFluidTexture(fluid, STILL)`, a cor do `FluidTintSource` (`getColorARGB(FluidResource)`) e a luz (`FluidType.getLightLevel`) de um fluido. `spec` é a cor e a luz do arquivo
/// `machinery_basis/liquid` do fluido, se ele tem. A água leva `0xFF3F76E4`: `MekanismRenderer.getColorARGB(TypedInstance<Fluid>)` chama `tintSource.color(defaultFluidState())`, e o
/// `FluidTintSources.water()` do NeoForge devolve essa cor para o `FluidState` (o `-1` é só do `color(BlockState)`, que o Mekanism não usa); a lava não tem `FluidTintSource` e fica branca.
/// `None` para um fluido que o port não conhece.
pub fn contents_fluid(id: &str, spec: Option<(u32, u32)>) -> Option<ContentsFluid> {
    let white = 0xFF_FF_FF;
    match id {
        "minecraft:water" => {
            return Some(ContentsFluid {
                sprite: "minecraft:block/water_still".to_owned(),
                rgb: WATER_TINT,
                light: 0,
                lighter_than_air: false,
            });
        }
        "minecraft:lava" => {
            return Some(ContentsFluid {
                sprite: "minecraft:block/lava_still".to_owned(),
                rgb: white,
                light: 15,
                lighter_than_air: false,
            });
        }
        _ => {}
    }
    let (color, light) = spec?;
    // `MekanismFluids.HEAVY_WATER`: a água do vanilla com a tinta 0xFF0D1455 (`texture(block/water_still, block/water_flow).tint(..)`). O desenho tinge no vértice, então
    // aqui a textura é a do original (o bloco no mundo e o balde ficam como o inventário diz, H48).
    if id == HEAVY_WATER {
        return Some(ContentsFluid {
            sprite: "minecraft:block/water_still".to_owned(),
            rgb: color,
            light,
            lighter_than_air: false,
        });
    }
    let lighter_than_air = LIGHTER_THAN_AIR_FLUIDS.contains(&id);
    if let Some(name) = id
        .strip_prefix("mekanismgenerators:")
        .filter(|name| GENERATORS_FLUIDS.contains(name))
    {
        return Some(ContentsFluid {
            sprite: format!("mekanismgenerators:mek_liquid/{name}"),
            rgb: white,
            light,
            lighter_than_air: false,
        });
    }
    let own = id
        .strip_prefix("mekanism:")
        .filter(|name| FLUIDS_WITH_OWN_TEXTURE.contains(name));
    Some(match own {
        Some(name) => ContentsFluid {
            sprite: format!("mekanism:mek_liquid/{name}"),
            rgb: white,
            light,
            lighter_than_air,
        },
        None => ContentsFluid {
            sprite: LIQUID_SPRITE.to_owned(),
            rgb: color,
            light,
            lighter_than_air,
        },
    })
}

/// `Chemical.icon` e `tint` de um químico para o tubo; o ícone que falta vira `liquid.png`.
pub fn contents_chemical(icon: Option<&str>, tint: u32) -> (String, u32) {
    (icon.unwrap_or(LIQUID_SPRITE).to_owned(), tint & 0xFF_FFFF)
}

/// Um transmissor para o desenho do modelo por instância (`TransmitterBlockStateModel`): o bloco, o começo do nome dos modelos das partes que o port escreve por lado e por estado visual
/// (`<prefixo>/<lado>_<estado>`), o `hideContiguousJoin` do blockstate, se a block entity dá o dado do modelo do desviador (`TileEntityDiversionTransporter.initModelData`) e se o blockstate
/// tem o vidro (`glass`: os quatro transportadores logísticos, que se pintam; as partes dele vêm depois das do corpo, `<prefixo>/glass_<lado>_<estado>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransmitterModel {
    pub block: String,
    pub prefix: String,
    pub hide_contiguous_join: bool,
    pub diversion: bool,
    pub glass: bool,
}

/// Os 22 transmissores, na ordem do original (`MekanismBlocks`: os cabos, os canos, os tubos, os condutores, os transportadores, o restritivo e o desviador). Só o desviador desliga o
/// `hideContiguousJoin` (`diversion_transporter.json` da datagen) e dá o dado `Diversion`.
pub fn transmitter_models() -> Vec<TransmitterModel> {
    let mut blocks: Vec<String> = Vec::new();
    for kind in [
        "universal_cable",
        "mechanical_pipe",
        "pressurized_tube",
        "thermodynamic_conductor",
        "logistical_transporter",
    ] {
        for tier in ["basic", "advanced", "elite", "ultimate"] {
            blocks.push(format!("mekanism:{tier}_{kind}"));
        }
    }
    blocks.push(RESTRICTIVE_TRANSPORTER.to_owned());
    blocks.push(DIVERSION_TRANSPORTER.to_owned());
    blocks
        .into_iter()
        .map(|block| {
            let diversion = block == DIVERSION_TRANSPORTER;
            let glass = block.ends_with("_logistical_transporter");
            let name = block.strip_prefix("mekanism:").unwrap_or(&block).to_owned();
            TransmitterModel {
                prefix: format!("mekanism:block/transmitter_part/{name}"),
                hide_contiguous_join: !diversion,
                diversion,
                glass,
                block,
            }
        })
        .collect()
}

/// Os blocos que o `TransmitterTypeDecorator` enfeita (`ClientRegistration.registerItemDecorations`: os tubos pressurizados, os condutores termodinâmicos e os cabos universais, os quatro tiers
/// de cada; os canos e os transportadores não têm).
pub const TRANSMITTER_ICON_KINDS: [&str; 3] = [
    "pressurized_tube",
    "thermodynamic_conductor",
    "universal_cable",
];

/// `TransmitterTypeDecorator`: o sprite que o item de `item` (`mekanism:<tier>_<tipo>`) leva por cima, `mekanism:gui/sprites/transmitter_icon/<id>` (`Mekanism.rl("transmitter_icon/" +
/// blockId.getPath())`, que `blitSprite` lê do atlas de GUI); `None` se o item não tem o ícone.
pub fn transmitter_icon_sprite(item: &str) -> Option<String> {
    let path = item.strip_prefix("mekanism:")?;
    let (tier, kind) = path.split_once('_')?;
    if ["basic", "advanced", "elite", "ultimate"].contains(&tier)
        && TRANSMITTER_ICON_KINDS.contains(&kind)
    {
        Some(format!("mekanism:gui/sprites/transmitter_icon/{path}"))
    } else {
        None
    }
}

/// `BlockSmallTransmitter` e `BlockLargeTransmitter`: o centro (`box(5..11)` e `box(4..12)`) em coordenadas do bloco, pelo id; `None` se o bloco não é um transmissor.
pub fn transmitter_center(block: &str) -> Option<[f64; 6]> {
    let (low, high) = if transmitter_is_large(block)? {
        (4.0, 12.0)
    } else {
        (5.0, 11.0)
    };
    Some([low, low, low, high, high, high].map(|pixels| pixels / 16.0))
}

/// Uma peça de `TileEntityTransmitter.getCollisionBoxes`: o braço de um lado (`side`) ou o centro (`None`), com as caixas da forma dela em coordenadas do bloco (o braço
/// `PUSH` e o `PULL` são de três caixas).
#[derive(Clone, Debug, PartialEq)]
pub struct TransmitterPiece {
    pub side: Option<Face>,
    pub boxes: Vec<[f64; 6]>,
}

impl TransmitterPiece {
    /// A menor caixa que contém a peça inteira.
    pub fn bounds(&self) -> [f64; 6] {
        let mut bounds = self.boxes[0];
        for part in &self.boxes[1..] {
            for axis in 0..3 {
                bounds[axis] = bounds[axis].min(part[axis]);
                bounds[axis + 3] = bounds[axis + 3].max(part[axis + 3]);
            }
        }
        bounds
    }

    /// As arestas que o contorno desenha da peça quando ela é a união de várias caixas (o braço `PUSH` e o `PULL`: `VoxelShape.forAllEdges` da forma de três caixas), em fração de
    /// `bounds()`, que é a caixa que o `context_outline` devolve. Uma peça de uma caixa só (o braço normal e o centro) não precisa delas: o cubo unitário escalado por `bounds()` já é o
    /// contorno dela.
    pub fn outline_edges(&self) -> Option<Vec<[f64; 6]>> {
        (self.boxes.len() > 1).then(|| union_edges_in(&self.boxes, &self.bounds()))
    }
}

/// As caixas de `box(x, y, z, ...)` do braço que sai para baixo (`SIDES`, `SIDES_PULL` e `SIDES_PUSH` de `BlockSmallTransmitter` e `BlockLargeTransmitter`), em pixels:
/// `(meia largura, de y, até y)`. Todas são simétricas em torno do eixo do braço, então girar para um lado é só trocar o eixo e, para o lado positivo, espelhar o intervalo.
fn arm_parts(large: bool, kind: ConnectionType) -> &'static [(f64, f64, f64)] {
    match (large, kind) {
        (false, ConnectionType::Pull) => &[(3.0, 4.0, 5.0), (2.0, 2.0, 4.0), (4.0, 0.0, 2.0)],
        (false, ConnectionType::Push) => &[(3.0, 3.0, 5.0), (2.0, 1.0, 3.0), (1.0, 0.0, 1.0)],
        (false, _) => &[(3.0, 0.0, 5.0)],
        (true, ConnectionType::Pull) => &[(4.0, 3.0, 4.0), (3.0, 2.0, 3.0), (5.0, 0.0, 2.0)],
        (true, ConnectionType::Push) => &[(4.0, 3.0, 4.0), (3.0, 1.0, 3.0), (2.0, 0.0, 1.0)],
        (true, _) => &[(4.0, 0.0, 4.0)],
    }
}

/// `getSideForType(type, side)` dos dois blocos: as caixas do braço de `side` para o tipo de conexão `kind` (o que não é `PUSH` nem `PULL` é o braço normal).
fn arm_boxes(large: bool, kind: ConnectionType, side: Face) -> Vec<[f64; 6]> {
    let axis = match side {
        Face::Down | Face::Up => 1,
        Face::North | Face::South => 2,
        Face::West | Face::East => 0,
    };
    let positive = matches!(side, Face::Up | Face::South | Face::East);
    arm_parts(large, kind)
        .iter()
        .map(|&(half, from, to)| {
            let (near, far) = if positive {
                (16.0 - to, 16.0 - from)
            } else {
                (from, to)
            };
            let mut bounds = [
                8.0 - half,
                8.0 - half,
                8.0 - half,
                8.0 + half,
                8.0 + half,
                8.0 + half,
            ];
            bounds[axis] = near;
            bounds[axis + 3] = far;
            bounds.map(|pixels| pixels / 16.0)
        })
        .collect()
}

/// `TileEntityTransmitter.getCollisionBoxes`: um braço por lado cujo tipo visível (`getConnectionType`) não é `NONE`, na ordem dos lados, e o centro por último. `kinds` está na
/// ordem de `Direction`. `None` se o bloco não é um transmissor.
pub fn transmitter_pieces(
    block: &str,
    kinds: &[ConnectionType; 6],
) -> Option<Vec<TransmitterPiece>> {
    let large = transmitter_is_large(block)?;
    let mut pieces: Vec<TransmitterPiece> = Face::ALL
        .into_iter()
        .filter(|side| kinds[*side as usize] != ConnectionType::None)
        .map(|side| TransmitterPiece {
            side: Some(side),
            boxes: arm_boxes(large, kinds[side as usize], side),
        })
        .collect();
    pieces.push(TransmitterPiece {
        side: None,
        boxes: vec![transmitter_center(block)?],
    });
    Some(pieces)
}

/// `BlockTransmitter.getRealShape`: a forma inteira do transmissor, a de seleção sem o Configurator (`getShape`), a de colisão (`getCollisionShape`, que nem o Configurator troca) e a
/// visual (a de colisão, que o `BlockBehaviour.getVisualShape` devolve e o Mekanism não troca nos transmissores). É o centro e o braço de cada lado cujo tipo visível
/// (`Transmitter.getConnectionType`) não é `NONE`, cada braço com a forma do tipo dele (`getSideForType`: o `PUSH` e o `PULL` não são o braço normal); `kinds` está na ordem de
/// `Direction`. Um transmissor sem braço é o centro. A peça devolvida não tem lado: as caixas dela são o centro e os braços, e `outline_edges` dá as arestas da união
/// (`VoxelShapeUtils.batchCombine(center, OR, true, shapes)`, o `forAllEdges` dela). `None` se o bloco não é um transmissor.
pub fn transmitter_shape(block: &str, kinds: &[ConnectionType; 6]) -> Option<TransmitterPiece> {
    let large = transmitter_is_large(block)?;
    let mut boxes = vec![transmitter_center(block)?];
    for side in Face::ALL {
        let kind = kinds[side as usize];
        if kind != ConnectionType::None {
            boxes.extend(arm_boxes(large, kind, side));
        }
    }
    Some(TransmitterPiece { side: None, boxes })
}

/// `MultipartUtils.collisionRayTrace`: a peça mais perto que o raio de `origin` a `end` atravessa (a primeira em empate). Cada peça vale pela caixa dela que o raio toca antes.
pub fn looked_piece(
    pieces: &[TransmitterPiece],
    origin: [f64; 3],
    end: [f64; 3],
) -> Option<&TransmitterPiece> {
    let mut best: Option<(f64, &TransmitterPiece)> = None;
    for piece in pieces {
        let nearest = piece
            .boxes
            .iter()
            .filter_map(|bounds| clip_box(origin, end, bounds))
            .min_by(f64::total_cmp);
        if let Some(distance) = nearest
            && best.is_none_or(|(taken, _)| distance < taken)
        {
            best = Some((distance, piece));
        }
    }
    best.map(|(_, piece)| piece)
}

/// `TileEntityTransmitter.getSideLookingAt`: o lado do braço que o raio toca, e `None` no centro ou se o raio erra tudo.
pub fn side_looking_at(
    pieces: &[TransmitterPiece],
    origin: [f64; 3],
    end: [f64; 3],
) -> Option<Face> {
    looked_piece(pieces, origin, end)?.side
}

/// `BlockTransmitter.getShape` com o Configurator na mão: a peça que o raio toca, e o centro se nenhuma (`getCenter`).
pub fn partial_piece(
    pieces: &[TransmitterPiece],
    origin: [f64; 3],
    end: [f64; 3],
) -> &TransmitterPiece {
    looked_piece(pieces, origin, end)
        .unwrap_or_else(|| pieces.last().expect("o centro está na lista"))
}

/// A caixa da peça que o raio toca (a menor que a contém, nos braços `PUSH` e `PULL`), e o centro se nenhuma: o que o `context_outline` do Horizon devolve.
pub fn partial_shape(pieces: &[TransmitterPiece], origin: [f64; 3], end: [f64; 3]) -> [f64; 6] {
    partial_piece(pieces, origin, end).bounds()
}

/// `VoxelShape.clip` de uma caixa: a distância (como fração do raio) até a caixa, se o raio de `origin` a `end` a atravessa.
pub fn clip_box(origin: [f64; 3], end: [f64; 3], bounds: &[f64; 6]) -> Option<f64> {
    let (mut near, mut far) = (0.0f64, 1.0f64);
    for axis in 0..3 {
        let delta = end[axis] - origin[axis];
        if delta.abs() < 1.0e-12 {
            if origin[axis] < bounds[axis] || origin[axis] > bounds[axis + 3] {
                return None;
            }
            continue;
        }
        let (a, b) = (
            (bounds[axis] - origin[axis]) / delta,
            (bounds[axis + 3] - origin[axis]) / delta,
        );
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// `SerializationConstants.DATA_NAME`, `DATA_TYPE` e `CONFIG`: as chaves do compound do cartão.
pub const CARD_DATA_NAME: &str = "data_name";
pub const CARD_DATA_TYPE: &str = "data_type";
pub const CARD_CONFIG: &str = "config";

/// A tradução do nome de um bloco (`Block.getDescriptionId`): `block.<namespace>.<path>`.
pub fn block_description_id(block: &str) -> String {
    let (namespace, path) = block.split_once(':').unwrap_or(("minecraft", block));
    format!("block.{namespace}.{}", path.replace('/', "."))
}

/// `INDIGO`, a cor dos nomes nas mensagens do cartão.
pub const COLOR_INDIGO: u8 = 9;

/// O nome de um bloco na cor `INDIGO`, ou `gui.mekanism.none` quando o cartão não tem nome (`MekanismLang.NONE`).
fn card_name(name_key: Option<&str>) -> Chat {
    Chat::plain(name_key.unwrap_or("gui.mekanism.none"), Some(COLOR_INDIGO))
}

/// `CONFIG_CARD_GOT`, `SET`: `translate(EnumColor.INDIGO, name)`; a cor vale só para o argumento, e a mensagem não tem cor própria.
pub fn card_got_message(name_key: &str) -> Chat {
    Chat::new(
        "configuration_card.mekanism.got",
        vec![card_name(Some(name_key))],
        None,
    )
}

pub fn card_set_message(name_key: Option<&str>) -> Chat {
    Chat::new(
        "configuration_card.mekanism.set",
        vec![card_name(name_key)],
        None,
    )
}

/// `CONFIG_CARD_UNEQUAL.translateColored(EnumColor.RED)`.
pub fn card_unequal_message() -> Chat {
    Chat::plain("configuration_card.mekanism.unequal", Some(12))
}

/// `CONFIG_CARD_CLEARED.translate()`.
pub fn card_cleared_message() -> Chat {
    Chat::plain("configuration_card.mekanism.cleared", None)
}

/// `ConfigurationData.addToTooltip`: `CONFIG_CARD_HAS_DATA.translateColored(GRAY, INDIGO, name)`.
pub fn card_tooltip_line(name_key: Option<&str>) -> Chat {
    Chat::new(
        "configuration_card.mekanism.has_data",
        vec![card_name(name_key)],
        Some(enum_color_rgb(COLOR_GRAY)),
    )
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os efeitos de tela do Configurator e dos itens de modo (decisão 48): a sobreposição da face, o texto do modo sobre a barra, o menu radial e o texto do
// HUD. A conta de cada um (posição, tempo, geometria) é da `machinery_basis_core`; aqui ficam os dados do Mekanism.

/// `ItemConfigurator.LAZY_RADIAL_DATA`: `Mekanism.rl("configurator_mode")`.
pub const CONFIGURATOR_RADIAL_ID: &str = "mekanism:configurator_mode";

/// Onde cada textura que os efeitos desenham fica no atlas de telas do Horizon: `gui/sprites/...` do conteúdo do mod.
const SPRITES: &str = "mekanism:gui/sprites";

impl ConfiguratorMode {
    /// `sliceName`: o nome do tipo de transmissão na cor do modo para os de configurar, e o texto do modo para os outros.
    pub fn slice_name(self) -> Chat {
        match self.transmission() {
            Some(kind) if self.is_configurating() => {
                let mut name = transmission_name(kind);
                name.color = Some(enum_color_rgb(self.color()));
                name
            }
            _ => self.text(),
        }
    }

    /// `icon`: o sprite do tipo de transmissão (`TransmissionType.guiTexture`) ou `radial/<modo>`.
    pub fn radial_icon(self) -> String {
        match self.transmission() {
            Some(kind) => format!("{SPRITES}/transmission/{}", kind.serialized_name()),
            None => format!("{SPRITES}/radial/{}", self.serialized_name()),
        }
    }

    /// `getScrollTextComponent`: o texto do modo.
    pub fn scroll_text(self) -> Chat {
        self.text()
    }

    /// `ItemConfigurator.addHUDStrings`: `MekanismLang.MODE.translateColored(EnumColor.PINK, getMode(instance))`.
    pub fn hud_line(self) -> Chat {
        Chat::new(
            "tooltip.mekanism.mode",
            vec![self.text()],
            Some(enum_color_rgb(COLOR_PINK)),
        )
    }
}

/// `RenderTickHandler.onBlockHover`, o ramo do Configurator: com um modo de configurar, uma máquina que aceita o tipo e o lado ligado, a face que o olhar toca
/// recebe a sobreposição do tipo de transmissão na cor do `DataType` do lado. Devolve o tipo e o `DataType`.
pub fn configurator_overlay(
    mode: ConfiguratorMode,
    config: &TileComponentConfig,
    facing: Face,
    face: Face,
) -> Option<(TransmissionType, DataType)> {
    if !mode.is_configurating() {
        return None;
    }
    let kind = mode.transmission()?;
    if !config.supports(kind) {
        return None;
    }
    let info = config.config(kind)?;
    let side = RelativeSide::from_directions(facing, face);
    info.is_side_enabled(side)
        .then(|| (kind, info.data_type(side)))
}

/// `MekanismRenderer.getColorARGB(dataType.getColor(), 0.6F)`: a cor do `EnumColor` com o alfa de 60 % (`ARGB.color(float, int)`, `floor(0,6F * 255F)` = 153).
pub fn overlay_argb(data_type: DataType) -> u32 {
    (153_u32 << 24) | enum_color_rgb(data_type.color())
}

/// `MekanismRenderer.overlays`: o sprite `block/overlay/<transmissão>_overlay`.
pub fn overlay_sprite(kind: TransmissionType) -> String {
    format!("{SPRITES}/overlay/{}_overlay", kind.serialized_name())
}

/// `RenderTickHandler.onBlockHover`, o ramo do desviador: o sprite do modo do lado que o olhar toca (`MekanismRenderer.GUNPOWDER_SPRITE`, `REDSTONE_TORCH_SPRITE` e
/// `REDSTONE_TORCH_OFF_SPRITE`, do atlas de itens e do de blocos do original; o atlas do Horizon os tem com estes nomes).
pub fn diversion_overlay_sprite(
    mode: machinery_basis_core::transporter::node::DiversionControl,
) -> &'static str {
    use machinery_basis_core::transporter::node::DiversionControl;
    match mode {
        DiversionControl::Disabled => "item/gunpowder",
        DiversionControl::High => "block/redstone_torch",
        DiversionControl::Low => "block/redstone_torch_off",
    }
}

/// `ConfiguratorOverlayHandler.DIVERSION_OVERLAY_ARGB = ARGB.white(0.8F)`: `floor(0,8F * 255F)` = 204 de alfa, branco.
pub const DIVERSION_OVERLAY_ARGB: u32 = 0xCC_FF_FF_FF;

/// `MekanismLang.GENERIC_WITH_PARENTHESIS.translateColored(color, dataType, color.getName())`: `"Input (Red)"`, na cor do tipo.
fn data_type_with_color(data_type: DataType) -> Chat {
    let color = data_type.color();
    Chat::new(
        "generic.mekanism.with_parenthesis",
        vec![
            Chat::plain(
                &format!("side_data.mekanism.{}", data_type.serialized_name()),
                Some(color),
            ),
            Chat::new(
                &format!(
                    "color.mekanism.{}",
                    machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(color)]
                ),
                Vec::new(),
                None,
            ),
        ],
        Some(enum_color_rgb(color)),
    )
}

/// `GuiSlot.updateTooltip` com o Configurator no cursor: sobre um slot de inventário da máquina, o primeiro `DataType` (na ordem do `ordinal`) cujo `SlotInfo` tem o slot,
/// com o nome da cor. `None` quando nenhum tem (o tooltip do slot segue o caminho normal).
pub fn configurator_slot_tooltip(config: &TileComponentConfig, container: usize) -> Option<Chat> {
    let info = config.config(TransmissionType::Item)?;
    info.supported_data_types()
        .into_iter()
        .find(|data_type| {
            info.slot_info_of(*data_type)
                .is_some_and(|slot| slot.containers.contains(&container))
        })
        .map(data_type_with_color)
}

/// `GuiGauge.updateTooltip` com o Configurator no cursor, num medidor de cor `color` (o `ordinal` do `EnumColor`, o `getGaugeColor().getColor()`) e tipo `kind`: o
/// `DataType` do tipo cuja cor é a do medidor, ou só o nome da cor entre parênteses se não há. Sem a configuração da máquina (a tela não é de uma máquina configurável) ou sem
/// tipo, o tooltip fica vazio.
pub fn configurator_gauge_tooltip(
    config: Option<&TileComponentConfig>,
    kind: Option<TransmissionType>,
    color: u8,
) -> Vec<Chat> {
    let (Some(config), Some(kind)) = (config, kind) else {
        return Vec::new();
    };
    let found = config.config(kind).and_then(|info| {
        info.supported_data_types()
            .into_iter()
            .find(|data_type| data_type.color() == color)
    });
    vec![match found {
        Some(data_type) => data_type_with_color(data_type),
        None => Chat::new(
            "generic.mekanism.parenthesis",
            vec![Chat::new(
                &format!(
                    "color.mekanism.{}",
                    machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(color)]
                ),
                Vec::new(),
                None,
            )],
            Some(enum_color_rgb(color)),
        ),
    }]
}

/// A configuração de cliente que os efeitos leem (`MekanismConfig.client`): `allowModeScroll` e `whiteRadialText` (seção `accessibility`), `enabled`, `scale` e
/// `reverse` (seção `hud`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectsConfig {
    pub allow_mode_scroll: bool,
    pub white_radial_text: bool,
    pub hud_enabled: bool,
    pub hud_scale: f32,
    pub hud_reverse: bool,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self {
            allow_mode_scroll: true,
            white_radial_text: false,
            hud_enabled: true,
            hud_scale: 0.6,
            hud_reverse: false,
        }
    }
}

/// `ClientConfig.java:61-71` (`accessibility`) e `:105-111` (`hud`); `CachedFloatValue` guarda o `double` do arquivo como `float`.
pub fn effects_config(loaded: &ClientLoaded) -> EffectsConfig {
    let defaults = EffectsConfig::default();
    EffectsConfig {
        allow_mode_scroll: loaded
            .bool(&CLIENT_CONFIG, "accessibility", "allowModeScroll")
            .unwrap_or(defaults.allow_mode_scroll),
        white_radial_text: loaded
            .bool(&CLIENT_CONFIG, "accessibility", "whiteRadialText")
            .unwrap_or(defaults.white_radial_text),
        hud_enabled: loaded
            .bool(&CLIENT_CONFIG, "hud", "enabled")
            .unwrap_or(defaults.hud_enabled),
        hud_scale: loaded
            .float(&CLIENT_CONFIG, "hud", "scale")
            .map_or(defaults.hud_scale, |scale| scale as f32),
        hud_reverse: loaded
            .bool(&CLIENT_CONFIG, "hud", "reverse")
            .unwrap_or(defaults.hud_reverse),
    }
}

/// `BlockEnergyCube.getShape` (a seleção e a colisão: o cubo não sobrescreve `getCollisionShape`) do cubo posto, com o tile (`bounds[getIndex(...)]`): o quadro, com os LEDs, e a
/// placa (o conector e a porta) de cada lado absoluto habilitado. Com o `facing` leste ou oeste o quadro e as placas de cima e de baixo giram 90 graus em Y; com o `facing` para
/// cima ou para baixo, o quadro e as placas da esquerda e da direita giram 270 graus em X (`Shapes.rotate` com `BLOCK_ROT_Y_90` e `BLOCK_ROT_X_270`). As caixas saem em unidades de
/// bloco e não são fundidas como o `VoxelShape.toAabbs` do original: o conjunto de pontos é o mesmo.
pub mod cube_shape {
    use machinery_basis_core::side_config::RelativeSide;
    use machinery_basis_core::sides::Face;

    type Box16 = [f64; 6];

    const FRAME: [Box16; 24] = [
        [0.0, 0.0, 0.0, 3.0, 3.0, 16.0],
        [0.0, 3.0, 0.0, 3.0, 16.0, 3.0],
        [0.0, 3.0, 13.0, 3.0, 16.0, 16.0],
        [0.0, 13.0, 3.0, 3.0, 16.0, 13.0],
        [3.0, 0.0, 0.0, 16.0, 3.0, 3.0],
        [3.0, 0.0, 13.0, 16.0, 3.0, 16.0],
        [3.0, 13.0, 0.0, 16.0, 16.0, 3.0],
        [3.0, 13.0, 13.0, 16.0, 16.0, 16.0],
        [13.0, 0.0, 3.0, 16.0, 3.0, 13.0],
        [13.0, 3.0, 0.0, 16.0, 13.0, 3.0],
        [13.0, 3.0, 13.0, 16.0, 13.0, 16.0],
        [13.0, 13.0, 3.0, 16.0, 16.0, 13.0],
        [12.5, 15.0, 7.5, 13.5, 15.9, 8.5],
        [2.5, 15.0, 7.5, 3.5, 15.9, 8.5],
        [12.5, 7.5, 0.1, 13.5, 8.5, 1.0],
        [2.5, 7.5, 0.1, 3.5, 8.5, 1.0],
        [2.5, 0.1, 7.5, 3.5, 1.0, 8.5],
        [12.5, 0.1, 7.5, 13.5, 1.0, 8.5],
        [12.5, 7.5, 15.0, 13.5, 8.5, 15.9],
        [2.5, 7.5, 15.0, 3.5, 8.5, 15.9],
        [0.1, 7.5, 2.5, 1.0, 8.5, 3.5],
        [0.1, 7.5, 12.5, 1.0, 8.5, 13.5],
        [15.0, 7.5, 2.5, 15.9, 8.5, 3.5],
        [15.0, 7.5, 12.5, 15.9, 8.5, 13.5],
    ];
    const FRONT: [Box16; 2] = [
        [3.0, 5.0, 14.0, 13.0, 11.0, 15.0],
        [4.0, 4.0, 15.0, 12.0, 12.0, 16.0],
    ];
    const RIGHT: [Box16; 2] = [
        [1.0, 5.0, 3.0, 2.0, 11.0, 13.0],
        [0.0, 4.0, 4.0, 1.0, 12.0, 12.0],
    ];
    const LEFT: [Box16; 2] = [
        [14.0, 5.0, 3.0, 15.0, 11.0, 13.0],
        [15.0, 4.0, 4.0, 16.0, 12.0, 12.0],
    ];
    const BACK: [Box16; 2] = [
        [3.0, 5.0, 1.0, 13.0, 11.0, 2.0],
        [4.0, 4.0, 0.0, 12.0, 12.0, 1.0],
    ];
    const TOP: [Box16; 2] = [
        [3.0, 14.0, 5.0, 13.0, 15.0, 11.0],
        [4.0, 15.0, 4.0, 12.0, 16.0, 12.0],
    ];
    const BOTTOM: [Box16; 2] = [
        [3.0, 1.0, 5.0, 13.0, 2.0, 11.0],
        [4.0, 0.0, 4.0, 12.0, 1.0, 12.0],
    ];

    /// `BLOCK_ROT_Y_90`: o norte vai para o leste, `(x, z) -> (16 - z, x)`.
    fn rotate_y90(b: Box16) -> Box16 {
        [16.0 - b[5], b[1], b[0], 16.0 - b[2], b[4], b[3]]
    }

    /// `BLOCK_ROT_X_270`: o norte vai para cima, `(y, z) -> (16 - z, y)`.
    fn rotate_x270(b: Box16) -> Box16 {
        [b[0], 16.0 - b[5], b[1], b[3], 16.0 - b[2], b[4]]
    }

    /// As caixas, em unidades de bloco, do cubo virado para `facing` com os lados absolutos `enabled` (cima, baixo, sul, norte, leste, oeste: a ordem do `getIndex`).
    pub fn boxes(facing: Face, enabled: [bool; 6]) -> Vec<[f64; 6]> {
        let rotate_vertical = matches!(facing, Face::East | Face::West);
        let rotate_horizontal = matches!(facing, Face::Up | Face::Down);
        let turn = |boxes: &[Box16], rotate: Option<fn(Box16) -> Box16>| -> Vec<Box16> {
            boxes.iter().map(|b| rotate.map_or(*b, |f| f(*b))).collect()
        };
        let y = rotate_vertical.then_some(rotate_y90 as fn(Box16) -> Box16);
        let x = rotate_horizontal.then_some(rotate_x270 as fn(Box16) -> Box16);
        let mut out = turn(&FRAME, y.or(x));
        let [top, bottom, front, back, left, right] = enabled;
        for (on, panel, rotate) in [
            (top, &TOP, y),
            (bottom, &BOTTOM, y),
            (front, &FRONT, None),
            (back, &BACK, None),
            (left, &LEFT, x),
            (right, &RIGHT, x),
        ] {
            if on {
                out.extend(turn(panel, rotate));
            }
        }
        out.into_iter().map(|b| b.map(|v| v / 16.0)).collect()
    }

    /// `isSideEnabled` de cada lado absoluto, na ordem de `boxes`: o lado relativo ao `facing` tem de estar habilitado (`slotInfo.isEnabled()`).
    pub fn absolute_enabled(
        facing: Face,
        relative_enabled: impl Fn(RelativeSide) -> bool,
    ) -> [bool; 6] {
        [
            Face::Up,
            Face::Down,
            Face::South,
            Face::North,
            Face::East,
            Face::West,
        ]
        .map(|side| relative_enabled(RelativeSide::from_directions(facing, side)))
    }
}

/// `CommandMek`: a árvore do comando `/mek` que o port tem. O original registra `build`, `chunk`, `debug`, `retrogen`, `givegear`, `radiation`, `testrules`, `tp` e `tpop`;
/// o port tem `debug` (o que falta está no inventário do plano).
pub mod command {
    use machinery_basis_core::commands::{CommandNode, LEVEL_ALL, LEVEL_GAMEMASTERS};

    /// `Commands.literal("mek")`.
    pub const ROOT: &str = "mek";

    /// `CommandMek.register`: `mek` (`MekanismPermissions.COMMAND`, `Commands.LEVEL_ALL`) e `debug` (`COMMAND_DEBUG`, `nodeOpCommand`: `Commands.LEVEL_GAMEMASTERS`), que roda `debug`.
    pub fn tree<E>(debug: E) -> CommandNode<E> {
        CommandNode::literal(ROOT, LEVEL_ALL)
            .then(CommandNode::literal("debug", LEVEL_GAMEMASTERS).executes(debug))
    }
}

/// O leitor de rede (`ItemNetworkReader`): as mensagens que o clique manda ao jogador. Os números vêm das redes (`machinery_basis_core::transmission::reading`); aqui ficam os
/// textos, as chaves de tradução e as cores (`TextComponentUtil.smartTranslate`, `ILangEntry.translateColored`, `EnergyDisplay`, `MekanismUtils.getTemperatureDisplay`).
pub mod network_reader {
    use super::{ENUM_COLOR_RGB, Face, TransmissionType};
    use machinery_basis_core::temperature::{TemperatureUnit, temperature_display_parts};
    use machinery_basis_core::transmission::heat_network::flow_efficiency_percent;
    use machinery_basis_core::transmission::reading::{
        NetworkReading, ReadingDetail, TransmitterReading,
    };
    use machinery_basis_core::units::{display_number, grouped, java_float_to_string};
    use serde_json::{Value, json};

    /// `EnumColor.GRAY`, `DARK_GRAY` e `DARK_BLUE` pelo `ordinal`.
    const GRAY: u8 = 7;
    const DARK_GRAY: u8 = 8;
    const DARK_BLUE: u8 = 1;

    /// O traço da borda (`displayBorder`) e o texto do fim.
    const BORDER_DASHES: &str = "-------------";
    const END_BORDER: &str = "[=======]";
    /// A casa decimal dos números de energia e de temperatura (`getDisplayBase(value, unit, 2, ...)`).
    const DECIMALS: i32 = 2;
    /// `FluidType.BUCKET_VOLUME`.
    const BUCKET_VOLUME: f32 = 1000.0;

    /// Um argumento de `Component.translatable`: o que o `ComponentSerialization.CODEC` grava (um texto cru, um número ou outro componente).
    #[derive(Clone, Debug, PartialEq)]
    pub enum Arg {
        Str(String),
        Int(i64),
        Float(f32),
        Comp(Comp),
    }

    /// Um `Component` com o estilo que as mensagens usam (a cor).
    #[derive(Clone, Debug, PartialEq)]
    pub struct Comp {
        pub body: Body,
        pub color: Option<u32>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum Body {
        /// `Component.translatable(key, with)`.
        Translate { key: String, with: Vec<Arg> },
        /// `Component.literal(text)` com os irmãos (`extra`).
        Text { text: String, extra: Vec<Arg> },
    }

    impl Comp {
        fn translate(key: &str, with: Vec<Arg>) -> Comp {
            Comp {
                body: Body::Translate {
                    key: key.to_owned(),
                    with,
                },
                color: None,
            }
        }

        /// `Component.translatable(key)`, sem argumentos nem estilo: o nome de um recurso.
        pub fn translate_key(key: &str) -> Comp {
            Comp::translate(key, Vec::new())
        }

        fn literal(text: &str, extra: Vec<Arg>) -> Comp {
            Comp {
                body: Body::Text {
                    text: text.to_owned(),
                    extra,
                },
                color: None,
            }
        }

        /// `MutableComponent.setStyle(Style.EMPTY.withColor(color))`: troca o estilo inteiro pela cor.
        fn with_color(mut self, ordinal: u8) -> Comp {
            self.color = Some(ENUM_COLOR_RGB[usize::from(ordinal)]);
            self
        }

        /// O JSON do `ComponentSerialization.CODEC`: um texto sem irmãos nem estilo vira só o texto.
        pub fn to_json(&self) -> Value {
            let mut object = serde_json::Map::new();
            match &self.body {
                Body::Translate { key, with } => {
                    object.insert("translate".to_owned(), json!(key));
                    if !with.is_empty() {
                        object.insert(
                            "with".to_owned(),
                            Value::Array(with.iter().map(Arg::to_json).collect()),
                        );
                    }
                }
                Body::Text { text, extra } => {
                    if extra.is_empty() && self.color.is_none() {
                        return json!(text);
                    }
                    object.insert("text".to_owned(), json!(text));
                    if !extra.is_empty() {
                        object.insert(
                            "extra".to_owned(),
                            Value::Array(extra.iter().map(Arg::to_json).collect()),
                        );
                    }
                }
            }
            if let Some(color) = self.color {
                object.insert("color".to_owned(), json!(format!("#{color:06X}")));
            }
            Value::Object(object)
        }
    }

    impl Arg {
        pub fn to_json(&self) -> Value {
            match self {
                Arg::Str(text) => json!(text),
                Arg::Int(number) => json!(number),
                // `Float.toString`, lido de volta: o JSON do original guarda o `float` por esse texto.
                Arg::Float(number) => java_float_to_string(*number)
                    .parse::<f64>()
                    .map_or(Value::Null, |parsed| json!(parsed)),
                Arg::Comp(comp) => comp.to_json(),
            }
        }

        /// O argumento no estilo de `translateColored(.., DARK_GRAY, arg)`: um componente leva a cor no lugar do estilo dele, e qualquer outro vira texto (`getString(arg.toString())`).
        fn colored(self, ordinal: u8) -> Arg {
            Arg::Comp(match self {
                Arg::Comp(comp) => comp.with_color(ordinal),
                Arg::Str(text) => Comp::literal(&text, Vec::new()).with_color(ordinal),
                Arg::Int(number) => {
                    Comp::literal(&number.to_string(), Vec::new()).with_color(ordinal)
                }
                Arg::Float(number) => {
                    Comp::literal(&java_float_to_string(number), Vec::new()).with_color(ordinal)
                }
            })
        }
    }

    /// `lang.translateColored(GRAY, DARK_GRAY, arg)`.
    fn gray(key: &str, arg: Arg) -> Comp {
        Comp::translate(key, vec![arg.colored(DARK_GRAY)]).with_color(GRAY)
    }

    /// `displayBorder`: `NETWORK_READER_BORDER.translateColored(GRAY, "-------------", DARK_BLUE, toDisplay)`.
    fn border(inner: Arg) -> Comp {
        Comp::translate(
            "network_reader.mekanism.border",
            vec![Arg::Str(BORDER_DASHES.to_owned()), inner.colored(DARK_BLUE)],
        )
        .with_color(GRAY)
    }

    /// `MekanismLang.GENERIC_SQUARE_BRACKET.translate(inner)`.
    fn square_bracket(inner: Arg) -> Comp {
        Comp::translate("generic.mekanism.square_bracket", vec![inner])
    }

    /// `displayBorder(player, MekanismLang.MEKANISM, true)`.
    fn start_border() -> Comp {
        border(Arg::Comp(square_bracket(Arg::Comp(Comp::translate(
            "constants.mekanism.mod_name",
            Vec::new(),
        )))))
    }

    /// `displayEndBorder`.
    fn end_border() -> Comp {
        border(Arg::Str(END_BORDER.to_owned()))
    }

    /// A configuração local que as mensagens leem (`MekanismConfig.local`).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct LocalConfig {
        pub temperature_unit: TemperatureUnit,
        pub compact_energy: bool,
    }

    impl Default for LocalConfig {
        fn default() -> Self {
            Self {
                temperature_unit: TemperatureUnit::default(),
                compact_energy: true,
            }
        }
    }

    /// `EnergyDisplay.of(energy).getTextComponent()` (sem o máximo).
    fn energy_display(energy: i64, local: LocalConfig) -> Comp {
        let energy = energy.max(0);
        if energy == i64::MAX {
            return Comp::translate("gui.mekanism.infinite", Vec::new());
        }
        let symbol = vec![Arg::Comp(Comp::translate(
            "energy.mekanism.forge.short",
            Vec::new(),
        ))];
        if local.compact_energy {
            Comp::literal(&display_number(energy as f64, DECIMALS, true), symbol)
        } else {
            // `FORGE_ENERGY.appendTo(TextUtils.format(value), true, ...)`: o número agrupado e um espaço.
            Comp::literal(&format!("{} ", grouped(energy as u64)), symbol)
        }
    }

    /// `MekanismUtils.getTemperatureDisplay(temp, KELVIN, shift)`: o número na unidade local e o símbolo dela como irmão.
    fn temperature(temp: f64, shift: bool, local: LocalConfig) -> Comp {
        let (number, symbol) =
            temperature_display_parts(temp, local.temperature_unit, shift, DECIMALS);
        Comp::literal(&number, vec![Arg::Str(symbol.to_owned())])
    }

    /// O que a rede conta de si, com o recurso guardado já em componente (`FluidResource.getHoverName`, o nome do químico).
    pub type Reading = NetworkReading<Comp>;

    /// `displayTransmitterInfo`: o que o clique num transmissor de `kind` escreve. `heat` é a temperatura da capability `HEAT` da face clicada, quando o transmissor tem uma.
    pub fn transmitter_messages(
        kind: TransmissionType,
        reading: &TransmitterReading<Comp>,
        heat: Option<f64>,
        local: LocalConfig,
    ) -> Vec<Comp> {
        let mut lines = vec![start_border()];
        match reading {
            TransmitterReading::NoTransmitter => {}
            TransmitterReading::NoNetwork => {
                lines.push(Comp::translate("network.mekanism.no_network", Vec::new()))
            }
            TransmitterReading::Network(network) => {
                lines.push(gray(
                    "network_reader.mekanism.transmitters",
                    Arg::Int(network.transmitters as i64),
                ));
                lines.push(gray(
                    "network_reader.mekanism.acceptors",
                    Arg::Int(network.acceptors as i64),
                ));
                lines.extend(detail_lines(kind, network, local));
                if let Some(temp) = heat {
                    lines.push(temperature_line(temp, local));
                }
            }
        }
        lines.push(end_border());
        lines
    }

    /// `sendTemperature`.
    fn temperature_line(temp: f64, local: LocalConfig) -> Comp {
        gray(
            "network_reader.mekanism.temperature",
            Arg::Comp(temperature(temp, true, local)),
        )
    }

    /// `getNeededInfo`, `getStoredInfo`, `getFlowInfo` e `getNetworkReaderCapacity`, na ordem do item, sem as que a rede deixa nulas.
    fn detail_lines(kind: TransmissionType, network: &Reading, local: LocalConfig) -> Vec<Comp> {
        let needed_key = "network_reader.mekanism.needed";
        let buffer_key = "network_reader.mekanism.buffer";
        let throughput_key = "network_reader.mekanism.throughput";
        let capacity_key = "network_reader.mekanism.capacity";
        match &network.detail {
            ReadingDetail::Energy {
                needed,
                stored,
                flow,
                capacity,
            } => vec![
                gray(needed_key, Arg::Comp(energy_display(*needed, local))),
                gray(buffer_key, Arg::Comp(energy_display(*stored, local))),
                gray(
                    throughput_key,
                    Arg::Comp(Comp::translate(
                        "generic.mekanism.per_tick",
                        vec![Arg::Comp(energy_display(*flow, local))],
                    )),
                ),
                gray(capacity_key, Arg::Int(*capacity)),
            ],
            ReadingDetail::Resource {
                needed,
                stored,
                flow,
                capacity,
            } => {
                let stored = match stored {
                    None => Comp::translate("gui.mekanism.none", Vec::new()),
                    Some((resource, amount)) => Comp::translate(
                        "network.mekanism.mb_stored",
                        vec![Arg::Comp(resource.clone()), Arg::Int(*amount)],
                    ),
                };
                vec![
                    gray(needed_key, Arg::Comp(resource_needed(kind, *needed))),
                    gray(buffer_key, Arg::Comp(stored)),
                    gray(
                        throughput_key,
                        Arg::Comp(Comp::translate(
                            "network.mekanism.mb_per_tick",
                            vec![Arg::Int(*flow)],
                        )),
                    ),
                    gray(capacity_key, Arg::Int(*capacity)),
                ]
            }
            ReadingDetail::Heat {
                mean_temp,
                transferred,
                lost,
            } => vec![
                gray(
                    buffer_key,
                    Arg::Comp(Comp::translate(
                        "network.mekanism.heat_stored",
                        vec![Arg::Comp(temperature(*mean_temp, true, local))],
                    )),
                ),
                gray(
                    throughput_key,
                    Arg::Comp(heat_flow(*transferred, *lost, local)),
                ),
            ],
            ReadingDetail::Inventory => Vec::new(),
        }
    }

    /// `getNeededInfo` das redes de recurso: o fluido em baldes (`float`, `FluidNetwork`), o químico como o número (`TextComponentUtil.build(long)`, `ChemicalNetwork`).
    fn resource_needed(kind: TransmissionType, needed: i64) -> Comp {
        if kind == TransmissionType::Fluid {
            fluid_needed(needed)
        } else {
            Comp::literal(&needed.to_string(), Vec::new())
        }
    }

    /// `HeatNetwork.getFlowInfo`.
    fn heat_flow(transferred: f64, lost: f64, local: LocalConfig) -> Comp {
        let transferred_text = temperature(transferred, false, local);
        let lost_text = temperature(lost, false, local);
        match flow_efficiency_percent(transferred, lost) {
            None => Comp::translate(
                "network.mekanism.heat_flow",
                vec![Arg::Comp(transferred_text), Arg::Comp(lost_text)],
            ),
            Some(percent) => Comp::translate(
                "network.mekanism.heat_flow.efficiency",
                vec![
                    Arg::Comp(transferred_text),
                    Arg::Comp(lost_text),
                    Arg::Comp(Comp::translate(
                        "generic.mekanism.percent",
                        vec![Arg::Float(percent)],
                    )),
                ],
            ),
        }
    }

    /// `FluidNetwork.getNeededInfo`: `FLUID_NETWORK_NEEDED.translate(needed / (float) BUCKET_VOLUME)`.
    fn fluid_needed(needed: i64) -> Comp {
        Comp::translate(
            "network.mekanism.fluid_needed",
            vec![Arg::Float(needed as f32 / BUCKET_VOLUME)],
        )
    }

    /// `TextComponentUtil.build(enum)`: o nome traduzido do lado (`APILang.DOWN` e os outros).
    fn direction(side: Face) -> Comp {
        Comp::translate(&format!("direction.mekanism.{}", side.name()), Vec::new())
    }

    /// `TextComponentUtil.build(TransmissionType)`: `IHasTranslationKey`.
    fn transmission(kind: TransmissionType) -> Comp {
        Comp::translate(
            &format!("transmission.mekanism.{}", kind.serialized_name()),
            Vec::new(),
        )
    }

    /// `compileList`: os elementos juntos por `GENERIC_WITH_COMMA` (aninhado à esquerda) e entre colchetes.
    fn compile_list(elements: Vec<Comp>) -> Comp {
        let mut joined: Option<Comp> = None;
        for element in elements {
            joined = Some(match joined {
                None => element,
                Some(previous) => Comp::translate(
                    "generic.mekanism.with_comma",
                    vec![Arg::Comp(previous), Arg::Comp(element)],
                ),
            });
        }
        square_bracket(joined.map_or_else(|| Arg::Str(String::new()), Arg::Comp))
    }

    /// `EnumColor.BRIGHT_GREEN` e `RED` pelo `ordinal` (a cor do `OnOff` colorido).
    const BRIGHT_GREEN: u8 = 10;
    const RED: u8 = 12;

    /// `DynamicNetwork.getTextComponent()` de cada família: `NETWORK_DESCRIPTION.translate(<tipo da rede>, transmittersSize(), getAcceptorCount())`.
    pub fn network_description(
        kind: TransmissionType,
        transmitters: usize,
        acceptors: usize,
    ) -> Comp {
        let name = match kind {
            TransmissionType::Energy => "network.mekanism.energy",
            TransmissionType::Fluid => "network.mekanism.fluid",
            TransmissionType::Chemical => "network.mekanism.chemical",
            TransmissionType::Heat => "network.mekanism.heat",
            TransmissionType::Item => "network.mekanism.inventory",
        };
        Comp::translate(
            "network.mekanism.description",
            vec![
                Arg::Comp(Comp::translate_key(name)),
                Arg::Int(transmitters as i64),
                Arg::Int(acceptors as i64),
            ],
        )
    }

    /// O ramo de depuração de `ItemNetworkReader.useOn` (agachado, com `MekanismAPI.debug`, num bloco sem block entity): a borda `DEBUG_TITLE` entre colchetes, cada rede do
    /// registro (`TransmitterNetworkRegistry.toComponents`, na ordem do conjunto) como `TextComponentUtil.build(DARK_GRAY, component)` e a borda final.
    pub fn debug_messages(networks: &[(TransmissionType, usize, usize)]) -> Vec<Comp> {
        let mut out = vec![border(Arg::Comp(square_bracket(Arg::Comp(
            Comp::translate_key("constants.mekanism.debug_title"),
        ))))];
        out.extend(networks.iter().map(|(kind, transmitters, acceptors)| {
            network_description(*kind, *transmitters, *acceptors).with_color(DARK_GRAY)
        }));
        out.push(end_border());
        out
    }

    /// `/mek debug`: `COMMAND_DEBUG.translateColored(GRAY, OnOff.of(MekanismAPI.debug, true))`.
    pub fn debug_toggle_message(on: bool) -> Comp {
        let state = Comp::translate_key(if on {
            "boolean.mekanism.on"
        } else {
            "boolean.mekanism.off"
        })
        .with_color(if on { BRIGHT_GREEN } else { RED });
        Comp::translate("command.mekanism.debug", vec![Arg::Comp(state)]).with_color(GRAY)
    }

    /// `displayConnectedNetworks`, uma rede vizinha: o tipo de transmissão da rede, os lados do aceitador por onde ela entra.
    pub fn connected_messages(kind: TransmissionType, sides: &[Face]) -> Vec<Comp> {
        vec![
            border(Arg::Comp(compile_list(vec![transmission(kind)]))),
            gray(
                "network_reader.mekanism.connected",
                Arg::Comp(compile_list(
                    sides.iter().map(|side| direction(*side)).collect(),
                )),
            ),
            end_border(),
        ]
    }

    /// `displayBorder(MEKANISM, true)`, a temperatura e o fim: o clique numa máquina com a capability `HEAT`.
    pub fn heat_machine_messages(temp: f64, local: LocalConfig) -> Vec<Comp> {
        vec![start_border(), temperature_line(temp, local), end_border()]
    }
}

// recipe-book-rows:inicio
#[rustfmt::skip]
pub mod recipe_book_rows {
    //! GERADO por `dev/referencia/livro_receitas_tabela.py` a partir de `rastros/recipe_book_displays.jsonl`: não edite à mão.
    /// Um display que o original manda ao livro: `(id, receita, tipo, grupo (-1 sem grupo), categoria, item do resultado, quantidade, tooltip, requisitos)`, com `requisitos` o tamanho de `craftingRequirements` (-1 sem a lista).
    pub type Row = (i32, &'static str, &'static str, i32, &'static str, &'static str, i32, &'static [&'static str], i32);
    pub const ROWS: &[Row] = &[
        (31, "minecraft:baked_potato", "smelting", -1, "minecraft:furnace_food", "minecraft:baked_potato", 1, &["Baked Potato"], 1),
        (139, "minecraft:black_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:black_glazed_terracotta", 1, &["Black Glazed Terracotta"], 1),
        (170, "minecraft:blue_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:blue_glazed_terracotta", 1, &["Blue Glazed Terracotta"], 1),
        (194, "minecraft:brick", "smelting", -1, "minecraft:furnace_misc", "minecraft:brick", 1, &["Brick"], 1),
        (209, "mekanismtools:bronze/nugget_from_smelting", "smelting", 38, "minecraft:furnace_misc", "mekanism:nugget_bronze", 1, &["Bronze Nugget"], 1),
        (230, "minecraft:brown_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:brown_glazed_terracotta", 1, &["Brown Glazed Terracotta"], 1),
        (252, "minecraft:charcoal", "smelting", -1, "minecraft:furnace_misc", "minecraft:charcoal", 1, &["Charcoal"], 1),
        (370, "minecraft:coal_from_smelting_coal_ore", "smelting", 51, "minecraft:furnace_misc", "minecraft:coal", 1, &["Coal"], 1),
        (371, "minecraft:coal_from_smelting_deepslate_coal_ore", "smelting", 51, "minecraft:furnace_misc", "minecraft:coal", 1, &["Coal"], 1),
        (481, "mekanism:control_circuit/basic", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:basic_control_circuit", 1, &["Basic Control Circuit"], 1),
        (483, "mekanism:control_circuit/infused_advanced", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:advanced_control_circuit", 1, &["Advanced Control Circuit"], 1),
        (484, "mekanism:control_circuit/infused_elite", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:elite_control_circuit", 1, &["Elite Control Circuit"], 1),
        (485, "mekanism:control_circuit/infused_ultimate", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:ultimate_control_circuit", 1, &["Ultimate Control Circuit"], 1),
        (487, "minecraft:cooked_beef", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_beef", 1, &["Steak"], 1),
        (490, "minecraft:cooked_chicken", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_chicken", 1, &["Cooked Chicken"], 1),
        (493, "minecraft:cooked_cod", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_cod", 1, &["Cooked Cod"], 1),
        (496, "minecraft:cooked_mutton", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_mutton", 1, &["Cooked Mutton"], 1),
        (499, "minecraft:cooked_porkchop", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_porkchop", 1, &["Cooked Porkchop"], 1),
        (502, "minecraft:cooked_rabbit", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_rabbit", 1, &["Cooked Rabbit"], 1),
        (505, "minecraft:cooked_salmon", "smelting", -1, "minecraft:furnace_food", "minecraft:cooked_salmon", 1, &["Cooked Salmon"], 1),
        (527, "minecraft:copper_ingot_from_smelting_copper_ore", "smelting", 68, "minecraft:furnace_misc", "minecraft:copper_ingot", 1, &["Copper Ingot"], 1),
        (528, "minecraft:copper_ingot_from_smelting_deepslate_copper_ore", "smelting", 68, "minecraft:furnace_misc", "minecraft:copper_ingot", 1, &["Copper Ingot"], 1),
        (529, "minecraft:copper_ingot_from_smelting_raw_copper", "smelting", 68, "minecraft:furnace_misc", "minecraft:copper_ingot", 1, &["Copper Ingot"], 1),
        (536, "mekanismtools:copper_nugget_from_smelting", "smelting", 69, "minecraft:furnace_misc", "minecraft:copper_nugget", 1, &["Copper Nugget"], 1),
        (537, "minecraft:copper_nugget_from_smelting", "smelting", -1, "minecraft:furnace_misc", "minecraft:copper_nugget", 1, &["Copper Nugget"], 1),
        (545, "minecraft:cracked_deepslate_bricks", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cracked_deepslate_bricks", 1, &["Cracked Deepslate Bricks"], 1),
        (546, "minecraft:cracked_deepslate_tiles", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cracked_deepslate_tiles", 1, &["Cracked Deepslate Tiles"], 1),
        (547, "minecraft:cracked_nether_bricks", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cracked_nether_bricks", 1, &["Cracked Nether Bricks"], 1),
        (548, "minecraft:cracked_polished_blackstone_bricks", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cracked_polished_blackstone_bricks", 1, &["Cracked Polished Blackstone Bricks"], 1),
        (549, "minecraft:cracked_stone_bricks", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cracked_stone_bricks", 1, &["Cracked Stone Bricks"], 1),
        (570, "mekanism:crushing/andesite/from_polished", "crushing", 71, "mekanism:crushing_building", "minecraft:andesite", 1, &["Andesite"], 1),
        (571, "mekanism:crushing/andesite/slab_from_polished_slab", "crushing", 71, "mekanism:crushing_building", "minecraft:andesite_slab", 1, &["Andesite Slab"], 1),
        (572, "mekanism:crushing/andesite/stairs_from_polished_stairs", "crushing", 71, "mekanism:crushing_building", "minecraft:andesite_stairs", 1, &["Andesite Stairs"], 1),
        (573, "mekanism:crushing/biofuel/apple", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (574, "mekanism:crushing/biofuel/baked_potato", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (575, "mekanism:crushing/biofuel/bamboo", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (576, "mekanism:crushing/biofuel/berries", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (577, "mekanism:crushing/biofuel/big_dripleaf", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (578, "mekanism:crushing/biofuel/bread", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 4, &["Bio Fuel"], 1),
        (579, "mekanism:crushing/biofuel/bush", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (580, "mekanism:crushing/biofuel/cactus_flower", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (581, "mekanism:crushing/biofuel/cake", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 6, &["Bio Fuel"], 1),
        (582, "mekanism:crushing/biofuel/carved_pumpkin", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (583, "mekanism:crushing/biofuel/cookie", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 3, &["Bio Fuel"], 1),
        (584, "mekanism:crushing/biofuel/crimson_roots", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (585, "mekanism:crushing/biofuel/crops", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (586, "mekanism:crushing/biofuel/crops_tags", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (587, "mekanism:crushing/biofuel/dried_kelp", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (588, "mekanism:crushing/biofuel/dried_kelp_block", "crushing", 73, "mekanism:crushing_misc", "mekanism:block_bio_fuel", 1, &["Block of Bio Fuel"], 1),
        (589, "mekanism:crushing/biofuel/fern", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (590, "mekanism:crushing/biofuel/firefly_bush", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (591, "mekanism:crushing/biofuel/fungus", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (592, "mekanism:crushing/biofuel/glow_lichen", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (593, "mekanism:crushing/biofuel/hanging_roots", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (594, "mekanism:crushing/biofuel/hay_block", "crushing", 73, "mekanism:crushing_misc", "mekanism:block_bio_fuel", 2, &["Block of Bio Fuel"], 1),
        (595, "mekanism:crushing/biofuel/kelp", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (596, "mekanism:crushing/biofuel/large_fern", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 3, &["Bio Fuel"], 1),
        (597, "mekanism:crushing/biofuel/leaf_litter", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (598, "mekanism:crushing/biofuel/leaves", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (599, "mekanism:crushing/biofuel/lily_pad", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (600, "mekanism:crushing/biofuel/mangrove_roots", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (601, "mekanism:crushing/biofuel/melon", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 6, &["Bio Fuel"], 1),
        (602, "mekanism:crushing/biofuel/melon_slice", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (603, "mekanism:crushing/biofuel/moss_block", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (604, "mekanism:crushing/biofuel/moss_carpet", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 4, &["Bio Fuel"], 1),
        (605, "mekanism:crushing/biofuel/mushroom_blocks", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 4, &["Bio Fuel"], 1),
        (606, "mekanism:crushing/biofuel/mushrooms", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (607, "mekanism:crushing/biofuel/nether_sprouts", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (608, "mekanism:crushing/biofuel/nether_wart_block", "crushing", 73, "mekanism:crushing_misc", "mekanism:block_bio_fuel", 2, &["Block of Bio Fuel"], 1),
        (609, "mekanism:crushing/biofuel/pale_hanging_moss", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (610, "mekanism:crushing/biofuel/pink_petals", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (611, "mekanism:crushing/biofuel/pitcher_pod", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (612, "mekanism:crushing/biofuel/pumpkin", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 6, &["Bio Fuel"], 1),
        (613, "mekanism:crushing/biofuel/pumpkin_pie", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 7, &["Bio Fuel"], 1),
        (614, "mekanism:crushing/biofuel/rotten_flesh", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (615, "mekanism:crushing/biofuel/saplings", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (616, "mekanism:crushing/biofuel/sea_pickle", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (617, "mekanism:crushing/biofuel/seagrass", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (618, "mekanism:crushing/biofuel/seeds", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (619, "mekanism:crushing/biofuel/short_grass", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (620, "mekanism:crushing/biofuel/shroomlight", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 4, &["Bio Fuel"], 1),
        (621, "mekanism:crushing/biofuel/small_dripleaf", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (622, "mekanism:crushing/biofuel/small_flowers", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (623, "mekanism:crushing/biofuel/spore_blossom", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (624, "mekanism:crushing/biofuel/straw_bed", "crushing", 73, "mekanism:crushing_misc", "mekanism:block_bio_fuel", 3, &["Block of Bio Fuel"], 1),
        (625, "mekanism:crushing/biofuel/sugar_cane", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (626, "mekanism:crushing/biofuel/tall_flowers", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (627, "mekanism:crushing/biofuel/tall_grass", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 2, &["Bio Fuel"], 1),
        (628, "mekanism:crushing/biofuel/vines", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (629, "mekanism:crushing/biofuel/warped_roots", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (630, "mekanism:crushing/biofuel/warped_wart_block", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 4, &["Bio Fuel"], 1),
        (631, "mekanism:crushing/biofuel/wildflowers", "crushing", 72, "mekanism:crushing_misc", "mekanism:bio_fuel", 1, &["Bio Fuel"], 1),
        (632, "mekanism:crushing/blackstone/bricks_to_cracked_bricks", "crushing", 74, "mekanism:crushing_building", "minecraft:cracked_polished_blackstone_bricks", 1, &["Cracked Polished Blackstone Bricks"], 1),
        (633, "mekanism:crushing/blackstone/chiseled_bricks_to_bricks", "crushing", 75, "mekanism:crushing_building", "minecraft:polished_blackstone_bricks", 1, &["Polished Blackstone Bricks"], 1),
        (634, "mekanism:crushing/blackstone/from_cracked_bricks", "crushing", 75, "mekanism:crushing_building", "minecraft:polished_blackstone", 1, &["Polished Blackstone"], 1),
        (635, "mekanism:crushing/blackstone/from_polished", "crushing", 76, "mekanism:crushing_building", "minecraft:blackstone", 1, &["Blackstone"], 1),
        (636, "mekanism:crushing/blackstone/polished_slabs_to_slabs", "crushing", 76, "mekanism:crushing_building", "minecraft:blackstone_slab", 1, &["Blackstone Slab"], 1),
        (637, "mekanism:crushing/blackstone/polished_stairs_to_stairs", "crushing", 76, "mekanism:crushing_building", "minecraft:blackstone_stairs", 1, &["Blackstone Stairs"], 1),
        (638, "mekanism:crushing/blackstone/polished_wall_to_wall", "crushing", 76, "mekanism:crushing_building", "minecraft:blackstone_wall", 1, &["Blackstone Wall"], 1),
        (639, "mekanism:crushing/blaze_rod", "crushing", -1, "mekanism:crushing_efficiency", "minecraft:blaze_powder", 4, &["Blaze Powder"], 1),
        (640, "mekanism:crushing/bone", "crushing", 77, "mekanism:crushing_efficiency", "minecraft:bone_meal", 6, &["Bone Meal"], 1),
        (641, "mekanism:crushing/bone_block", "crushing", 77, "mekanism:crushing_efficiency", "minecraft:bone_meal", 9, &["Bone Meal"], 1),
        (642, "mekanism:crushing/break_disc_5", "crushing", -1, "mekanism:crushing_misc", "minecraft:disc_fragment_5", 9, &["Disc Fragment", "Music Disc - 5"], 1),
        (643, "mekanism:crushing/breeze_rod", "crushing", -1, "mekanism:crushing_efficiency", "minecraft:wind_charge", 6, &["Wind Charge"], 1),
        (644, "mekanism:crushing/charcoal_dust", "crushing", -1, "mekanism:crushing_misc", "mekanism:dust_charcoal", 1, &["Charcoal Dust"], 1),
        (645, "mekanism:crushing/chiseled_nether_bricks_to_nether_bricks", "crushing", -1, "mekanism:crushing_building", "minecraft:nether_bricks", 1, &["Nether Bricks"], 1),
        (646, "mekanism:crushing/cinnabar/brick_slabs_to_polished", "crushing", 78, "mekanism:crushing_building", "minecraft:polished_cinnabar_slab", 1, &["Polished Cinnabar Slab"], 1),
        (647, "mekanism:crushing/cinnabar/brick_stairs_to_polished", "crushing", 78, "mekanism:crushing_building", "minecraft:polished_cinnabar_stairs", 1, &["Polished Cinnabar Stairs"], 1),
        (648, "mekanism:crushing/cinnabar/brick_wall_to_polished", "crushing", 78, "mekanism:crushing_building", "minecraft:polished_cinnabar_wall", 1, &["Polished Cinnabar Wall"], 1),
        (649, "mekanism:crushing/cinnabar/bricks_to_polished", "crushing", 78, "mekanism:crushing_building", "minecraft:polished_cinnabar", 1, &["Polished Cinnabar"], 1),
        (650, "mekanism:crushing/cinnabar/chiseled_to_brick", "crushing", 79, "mekanism:crushing_building", "minecraft:cinnabar_bricks", 1, &["Cinnabar Bricks"], 1),
        (651, "mekanism:crushing/cinnabar/from_polished", "crushing", 80, "mekanism:crushing_building", "minecraft:cinnabar", 1, &["Cinnabar"], 1),
        (652, "mekanism:crushing/cinnabar/slabs_from_polished", "crushing", 80, "mekanism:crushing_building", "minecraft:cinnabar_slab", 1, &["Cinnabar Slab"], 1),
        (653, "mekanism:crushing/cinnabar/stairs_from_polished", "crushing", 80, "mekanism:crushing_building", "minecraft:cinnabar_stairs", 1, &["Cinnabar Stairs"], 1),
        (654, "mekanism:crushing/cinnabar/wall_from_polished", "crushing", 80, "mekanism:crushing_building", "minecraft:cinnabar_wall", 1, &["Cinnabar Wall"], 1),
        (655, "mekanism:crushing/cobblestone_to_gravel", "crushing", -1, "mekanism:crushing_building", "minecraft:gravel", 1, &["Gravel"], 1),
        (656, "mekanism:crushing/deepslate/brick_slabs_to_tile", "crushing", 81, "mekanism:crushing_building", "minecraft:deepslate_tile_slab", 1, &["Deepslate Tile Slab"], 1),
        (657, "mekanism:crushing/deepslate/brick_stairs_to_tile", "crushing", 81, "mekanism:crushing_building", "minecraft:deepslate_tile_stairs", 1, &["Deepslate Tile Stairs"], 1),
        (658, "mekanism:crushing/deepslate/brick_wall_to_tile", "crushing", 81, "mekanism:crushing_building", "minecraft:deepslate_tile_wall", 1, &["Deepslate Tile Wall"], 1),
        (659, "mekanism:crushing/deepslate/bricks_to_cracked_bricks", "crushing", 82, "mekanism:crushing_building", "minecraft:cracked_deepslate_bricks", 1, &["Cracked Deepslate Bricks"], 1),
        (660, "mekanism:crushing/deepslate/cracked_bricks_to_tile", "crushing", 81, "mekanism:crushing_building", "minecraft:deepslate_tiles", 1, &["Deepslate Tiles"], 1),
        (661, "mekanism:crushing/deepslate/cracked_tile_to_chiseled", "crushing", 83, "mekanism:crushing_building", "minecraft:chiseled_deepslate", 1, &["Chiseled Deepslate"], 1),
        (662, "mekanism:crushing/deepslate/from_chiseled", "crushing", 84, "mekanism:crushing_building", "minecraft:deepslate", 1, &["Deepslate"], 1),
        (663, "mekanism:crushing/deepslate/polished_slabs_to_brick", "crushing", 85, "mekanism:crushing_building", "minecraft:deepslate_brick_slab", 1, &["Deepslate Brick Slab"], 1),
        (664, "mekanism:crushing/deepslate/polished_stairs_to_brick", "crushing", 85, "mekanism:crushing_building", "minecraft:deepslate_brick_stairs", 1, &["Deepslate Brick Stairs"], 1),
        (665, "mekanism:crushing/deepslate/polished_to_bricks", "crushing", 85, "mekanism:crushing_building", "minecraft:deepslate_bricks", 1, &["Deepslate Bricks"], 1),
        (666, "mekanism:crushing/deepslate/polished_wall_to_brick", "crushing", 85, "mekanism:crushing_building", "minecraft:deepslate_brick_wall", 1, &["Deepslate Brick Wall"], 1),
        (667, "mekanism:crushing/deepslate/tile_slabs_to_cobbled", "crushing", 86, "mekanism:crushing_building", "minecraft:cobbled_deepslate_slab", 1, &["Cobbled Deepslate Slab"], 1),
        (668, "mekanism:crushing/deepslate/tile_stairs_to_cobbled", "crushing", 86, "mekanism:crushing_building", "minecraft:cobbled_deepslate_stairs", 1, &["Cobbled Deepslate Stairs"], 1),
        (669, "mekanism:crushing/deepslate/tile_to_cracked_tile", "crushing", 82, "mekanism:crushing_building", "minecraft:cracked_deepslate_tiles", 1, &["Cracked Deepslate Tiles"], 1),
        (670, "mekanism:crushing/deepslate/tile_wall_to_cobbled", "crushing", 86, "mekanism:crushing_building", "minecraft:cobbled_deepslate_wall", 1, &["Cobbled Deepslate Wall"], 1),
        (671, "mekanism:crushing/deepslate/to_cobbled", "crushing", 86, "mekanism:crushing_building", "minecraft:cobbled_deepslate", 1, &["Cobbled Deepslate"], 1),
        (672, "mekanism:crushing/dewax/chiseled_copper", "crushing", 87, "mekanism:crushing_building", "minecraft:chiseled_copper", 1, &["Chiseled Copper"], 1),
        (673, "mekanism:crushing/dewax/copper_bars", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_bars", 1, &["Copper Bars"], 1),
        (674, "mekanism:crushing/dewax/copper_block", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_block", 1, &["Block of Copper"], 1),
        (675, "mekanism:crushing/dewax/copper_bulb", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_bulb", 1, &["Copper Bulb"], 1),
        (676, "mekanism:crushing/dewax/copper_chain", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_chain", 1, &["Copper Chain"], 1),
        (677, "mekanism:crushing/dewax/copper_chest", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_chest", 1, &["Copper Chest"], 1),
        (678, "mekanism:crushing/dewax/copper_door", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_door", 1, &["Copper Door"], 1),
        (679, "mekanism:crushing/dewax/copper_golem_statue", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_golem_statue", 1, &["Copper Golem Statue"], 1),
        (680, "mekanism:crushing/dewax/copper_grate", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_grate", 1, &["Copper Grate"], 1),
        (681, "mekanism:crushing/dewax/copper_lantern", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_lantern", 1, &["Copper Lantern"], 1),
        (682, "mekanism:crushing/dewax/copper_trapdoor", "crushing", 87, "mekanism:crushing_building", "minecraft:copper_trapdoor", 1, &["Copper Trapdoor"], 1),
        (683, "mekanism:crushing/dewax/cut_copper", "crushing", 87, "mekanism:crushing_building", "minecraft:cut_copper", 1, &["Cut Copper"], 1),
        (684, "mekanism:crushing/dewax/cut_copper_slab", "crushing", 87, "mekanism:crushing_building", "minecraft:cut_copper_slab", 1, &["Cut Copper Slab"], 1),
        (685, "mekanism:crushing/dewax/cut_copper_stairs", "crushing", 87, "mekanism:crushing_building", "minecraft:cut_copper_stairs", 1, &["Cut Copper Stairs"], 1),
        (686, "mekanism:crushing/dewax/exposed_chiseled_copper", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_chiseled_copper", 1, &["Exposed Chiseled Copper"], 1),
        (687, "mekanism:crushing/dewax/exposed_copper", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper", 1, &["Exposed Copper"], 1),
        (688, "mekanism:crushing/dewax/exposed_copper_bars", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_bars", 1, &["Exposed Copper Bars"], 1),
        (689, "mekanism:crushing/dewax/exposed_copper_bulb", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_bulb", 1, &["Exposed Copper Bulb"], 1),
        (690, "mekanism:crushing/dewax/exposed_copper_chain", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_chain", 1, &["Exposed Copper Chain"], 1),
        (691, "mekanism:crushing/dewax/exposed_copper_chest", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_chest", 1, &["Exposed Copper Chest"], 1),
        (692, "mekanism:crushing/dewax/exposed_copper_door", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_door", 1, &["Exposed Copper Door"], 1),
        (693, "mekanism:crushing/dewax/exposed_copper_golem_statue", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_golem_statue", 1, &["Exposed Copper Golem Statue"], 1),
        (694, "mekanism:crushing/dewax/exposed_copper_grate", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_grate", 1, &["Exposed Copper Grate"], 1),
        (695, "mekanism:crushing/dewax/exposed_copper_lantern", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_lantern", 1, &["Exposed Copper Lantern"], 1),
        (696, "mekanism:crushing/dewax/exposed_copper_trapdoor", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_copper_trapdoor", 1, &["Exposed Copper Trapdoor"], 1),
        (697, "mekanism:crushing/dewax/exposed_cut_copper", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_cut_copper", 1, &["Exposed Cut Copper"], 1),
        (698, "mekanism:crushing/dewax/exposed_cut_copper_slab", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_cut_copper_slab", 1, &["Exposed Cut Copper Slab"], 1),
        (699, "mekanism:crushing/dewax/exposed_cut_copper_stairs", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_cut_copper_stairs", 1, &["Exposed Cut Copper Stairs"], 1),
        (700, "mekanism:crushing/dewax/exposed_lightning_rod", "crushing", 88, "mekanism:crushing_building", "minecraft:exposed_lightning_rod", 1, &["Exposed Lightning Rod"], 1),
        (701, "mekanism:crushing/dewax/lightning_rod", "crushing", 87, "mekanism:crushing_building", "minecraft:lightning_rod", 1, &["Lightning Rod"], 1),
        (702, "mekanism:crushing/dewax/oxidized_chiseled_copper", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_chiseled_copper", 1, &["Oxidized Chiseled Copper"], 1),
        (703, "mekanism:crushing/dewax/oxidized_copper", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper", 1, &["Oxidized Copper"], 1),
        (704, "mekanism:crushing/dewax/oxidized_copper_bars", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_bars", 1, &["Oxidized Copper Bars"], 1),
        (705, "mekanism:crushing/dewax/oxidized_copper_bulb", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_bulb", 1, &["Oxidized Copper Bulb"], 1),
        (706, "mekanism:crushing/dewax/oxidized_copper_chain", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_chain", 1, &["Oxidized Copper Chain"], 1),
        (707, "mekanism:crushing/dewax/oxidized_copper_chest", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_chest", 1, &["Oxidized Copper Chest"], 1),
        (708, "mekanism:crushing/dewax/oxidized_copper_door", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_door", 1, &["Oxidized Copper Door"], 1),
        (709, "mekanism:crushing/dewax/oxidized_copper_golem_statue", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_golem_statue", 1, &["Oxidized Copper Golem Statue"], 1),
        (710, "mekanism:crushing/dewax/oxidized_copper_grate", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_grate", 1, &["Oxidized Copper Grate"], 1),
        (711, "mekanism:crushing/dewax/oxidized_copper_lantern", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_lantern", 1, &["Oxidized Copper Lantern"], 1),
        (712, "mekanism:crushing/dewax/oxidized_copper_trapdoor", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_copper_trapdoor", 1, &["Oxidized Copper Trapdoor"], 1),
        (713, "mekanism:crushing/dewax/oxidized_cut_copper", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_cut_copper", 1, &["Oxidized Cut Copper"], 1),
        (714, "mekanism:crushing/dewax/oxidized_cut_copper_slab", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_cut_copper_slab", 1, &["Oxidized Cut Copper Slab"], 1),
        (715, "mekanism:crushing/dewax/oxidized_cut_copper_stairs", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_cut_copper_stairs", 1, &["Oxidized Cut Copper Stairs"], 1),
        (716, "mekanism:crushing/dewax/oxidized_lightning_rod", "crushing", 89, "mekanism:crushing_building", "minecraft:oxidized_lightning_rod", 1, &["Oxidized Lightning Rod"], 1),
        (717, "mekanism:crushing/dewax/weathered_chiseled_copper", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_chiseled_copper", 1, &["Weathered Chiseled Copper"], 1),
        (718, "mekanism:crushing/dewax/weathered_copper", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper", 1, &["Weathered Copper"], 1),
        (719, "mekanism:crushing/dewax/weathered_copper_bars", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_bars", 1, &["Weathered Copper Bars"], 1),
        (720, "mekanism:crushing/dewax/weathered_copper_bulb", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_bulb", 1, &["Weathered Copper Bulb"], 1),
        (721, "mekanism:crushing/dewax/weathered_copper_chain", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_chain", 1, &["Weathered Copper Chain"], 1),
        (722, "mekanism:crushing/dewax/weathered_copper_chest", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_chest", 1, &["Weathered Copper Chest"], 1),
        (723, "mekanism:crushing/dewax/weathered_copper_door", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_door", 1, &["Weathered Copper Door"], 1),
        (724, "mekanism:crushing/dewax/weathered_copper_golem_statue", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_golem_statue", 1, &["Weathered Copper Golem Statue"], 1),
        (725, "mekanism:crushing/dewax/weathered_copper_grate", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_grate", 1, &["Weathered Copper Grate"], 1),
        (726, "mekanism:crushing/dewax/weathered_copper_lantern", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_lantern", 1, &["Weathered Copper Lantern"], 1),
        (727, "mekanism:crushing/dewax/weathered_copper_trapdoor", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_copper_trapdoor", 1, &["Weathered Copper Trapdoor"], 1),
        (728, "mekanism:crushing/dewax/weathered_cut_copper", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_cut_copper", 1, &["Weathered Cut Copper"], 1),
        (729, "mekanism:crushing/dewax/weathered_cut_copper_slab", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_cut_copper_slab", 1, &["Weathered Cut Copper Slab"], 1),
        (730, "mekanism:crushing/dewax/weathered_cut_copper_stairs", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_cut_copper_stairs", 1, &["Weathered Cut Copper Stairs"], 1),
        (731, "mekanism:crushing/dewax/weathered_lightning_rod", "crushing", 90, "mekanism:crushing_building", "minecraft:weathered_lightning_rod", 1, &["Weathered Lightning Rod"], 1),
        (732, "mekanism:crushing/diorite/from_polished", "crushing", 91, "mekanism:crushing_building", "minecraft:diorite", 1, &["Diorite"], 1),
        (733, "mekanism:crushing/diorite/slab_from_polished_slab", "crushing", 91, "mekanism:crushing_building", "minecraft:diorite_slab", 1, &["Diorite Slab"], 1),
        (734, "mekanism:crushing/diorite/stairs_from_polished_stairs", "crushing", 91, "mekanism:crushing_building", "minecraft:diorite_stairs", 1, &["Diorite Stairs"], 1),
        (735, "mekanism:crushing/dried_ghast", "crushing", -1, "mekanism:crushing_efficiency", "minecraft:ghast_tear", 3, &["Ghast Tear"], 1),
        (736, "mekanism:crushing/flint_to_gunpowder", "crushing", -1, "mekanism:crushing_misc", "minecraft:gunpowder", 1, &["Gunpowder"], 1),
        (737, "mekanism:crushing/granite/from_polished", "crushing", 92, "mekanism:crushing_building", "minecraft:granite", 1, &["Granite"], 1),
        (738, "mekanism:crushing/granite/slab_from_polished_slab", "crushing", 92, "mekanism:crushing_building", "minecraft:granite_slab", 1, &["Granite Slab"], 1),
        (739, "mekanism:crushing/granite/stairs_from_polished_stairs", "crushing", 92, "mekanism:crushing_building", "minecraft:granite_stairs", 1, &["Granite Stairs"], 1),
        (740, "mekanism:crushing/gravel_to_sand", "crushing", -1, "mekanism:crushing_building", "minecraft:sand", 1, &["Sand"], 1),
        (741, "mekanism:crushing/honeycomb_from_block", "crushing", -1, "mekanism:crushing_misc", "minecraft:honeycomb", 4, &["Honeycomb"], 1),
        (742, "mekanism:crushing/mud_bricks_to_packed", "crushing", -1, "mekanism:crushing_building", "minecraft:packed_mud", 1, &["Packed Mud"], 1),
        (743, "mekanism:crushing/nether_bricks_to_cracked_nether_bricks", "crushing", -1, "mekanism:crushing_building", "minecraft:cracked_nether_bricks", 1, &["Cracked Nether Bricks"], 1),
        (744, "mekanism:crushing/obsidian_to_dust", "crushing", -1, "mekanism:crushing_misc", "mekanism:dust_obsidian", 4, &["Obsidian Dust"], 1),
        (745, "mekanism:crushing/pointed_dripstone_from_block", "crushing", -1, "mekanism:crushing_misc", "minecraft:pointed_dripstone", 4, &["Pointed Dripstone"], 1),
        (746, "mekanism:crushing/polished_or_smooth_basalt_to_basalt", "crushing", -1, "mekanism:crushing_building", "minecraft:basalt", 1, &["Basalt"], 1),
        (747, "mekanism:crushing/prismarine/shard_from_block", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 4, &["Prismarine Shard"], 1),
        (748, "mekanism:crushing/prismarine/shard_from_brick", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 9, &["Prismarine Shard"], 1),
        (749, "mekanism:crushing/prismarine/shard_from_brick_slabs", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 9, &["Prismarine Shard"], 1),
        (750, "mekanism:crushing/prismarine/shard_from_brick_stairs", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 9, &["Prismarine Shard"], 1),
        (751, "mekanism:crushing/prismarine/shard_from_slabs", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 2, &["Prismarine Shard"], 1),
        (752, "mekanism:crushing/prismarine/shard_from_stairs", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 4, &["Prismarine Shard"], 1),
        (753, "mekanism:crushing/prismarine/shard_from_wall", "crushing", 93, "mekanism:crushing_misc", "minecraft:prismarine_shard", 4, &["Prismarine Shard"], 1),
        (754, "mekanism:crushing/purpur_block_from_pillar", "crushing", -1, "mekanism:crushing_building", "minecraft:purpur_block", 1, &["Purpur Block"], 1),
        (755, "mekanism:crushing/quartz/bricks_to_chiseled", "crushing", 94, "mekanism:crushing_building", "minecraft:chiseled_quartz_block", 1, &["Chiseled Quartz Block"], 1),
        (756, "mekanism:crushing/quartz/chiseled_to_pillar", "crushing", 95, "mekanism:crushing_building", "minecraft:quartz_pillar", 1, &["Quartz Pillar"], 1),
        (757, "mekanism:crushing/quartz/from_pillar", "crushing", 95, "mekanism:crushing_building", "minecraft:quartz_block", 1, &["Block of Quartz"], 1),
        (758, "mekanism:crushing/quartz/slab_to_smooth_slab", "crushing", 96, "mekanism:crushing_building", "minecraft:smooth_quartz_slab", 1, &["Smooth Quartz Slab"], 1),
        (759, "mekanism:crushing/quartz/smooth_to_bricks", "crushing", 97, "mekanism:crushing_building", "minecraft:quartz_bricks", 1, &["Quartz Bricks"], 1),
        (760, "mekanism:crushing/quartz/stairs_to_smooth_stairs", "crushing", 96, "mekanism:crushing_building", "minecraft:smooth_quartz_stairs", 1, &["Smooth Quartz Stairs"], 1),
        (761, "mekanism:crushing/quartz/to_smooth_quartz", "crushing", 96, "mekanism:crushing_building", "minecraft:smooth_quartz", 1, &["Smooth Quartz Block"], 1),
        (762, "mekanism:crushing/red_sandstone_to_sand", "crushing", -1, "mekanism:crushing_building", "minecraft:red_sand", 2, &["Red Sand"], 1),
        (763, "mekanism:crushing/resin/chiseled_resin_bricks_to_bricks", "crushing", -1, "mekanism:crushing_building", "minecraft:resin_bricks", 1, &["Resin Bricks"], 1),
        (764, "mekanism:crushing/resin/clump_from_block", "crushing", -1, "mekanism:crushing_misc", "minecraft:resin_clump", 9, &["Resin Clump"], 1),
        (765, "mekanism:crushing/resin/resin_bricks_to_block", "crushing", -1, "mekanism:crushing_building", "minecraft:resin_block", 1, &["Block of Resin"], 1),
        (766, "mekanism:crushing/sandstone_to_sand", "crushing", -1, "mekanism:crushing_building", "minecraft:sand", 2, &["Sand"], 1),
        (767, "mekanism:crushing/soul_soil_to_soul_sand", "crushing", -1, "mekanism:crushing_building", "minecraft:soul_sand", 1, &["Soul Sand"], 1),
        (768, "mekanism:crushing/stone/bricks_to_cracked_bricks", "crushing", 98, "mekanism:crushing_building", "minecraft:cracked_stone_bricks", 1, &["Cracked Stone Bricks"], 1),
        (769, "mekanism:crushing/stone/chiseled_bricks_to_bricks", "crushing", 99, "mekanism:crushing_building", "minecraft:stone_bricks", 1, &["Stone Bricks"], 1),
        (770, "mekanism:crushing/stone/from_cracked_bricks", "crushing", 100, "mekanism:crushing_building", "minecraft:stone", 1, &["Stone"], 1),
        (771, "mekanism:crushing/stone/slabs_to_cobblestone_slabs", "crushing", 101, "mekanism:crushing_building", "minecraft:cobblestone_slab", 1, &["Cobblestone Slab"], 1),
        (772, "mekanism:crushing/stone/stairs_to_cobblestone_stairs", "crushing", 101, "mekanism:crushing_building", "minecraft:cobblestone_stairs", 1, &["Cobblestone Stairs"], 1),
        (773, "mekanism:crushing/stone/to_cobblestone", "crushing", 101, "mekanism:crushing_building", "minecraft:cobblestone", 1, &["Cobblestone"], 1),
        (774, "mekanism:crushing/string/from_cushions", "crushing", 102, "mekanism:crushing_misc", "minecraft:string", 6, &["String"], 1),
        (775, "mekanism:crushing/string/from_wool", "crushing", 102, "mekanism:crushing_misc", "minecraft:string", 4, &["String"], 1),
        (776, "mekanism:crushing/string/from_wool_slabs", "crushing", 102, "mekanism:crushing_misc", "minecraft:string", 2, &["String"], 1),
        (777, "mekanism:crushing/string/from_wool_stairs", "crushing", 102, "mekanism:crushing_misc", "minecraft:string", 6, &["String"], 1),
        (778, "mekanism:crushing/sulfur/brick_slabs_to_polished", "crushing", 103, "mekanism:crushing_building", "minecraft:polished_sulfur_slab", 1, &["Polished Sulfur Slab"], 1),
        (779, "mekanism:crushing/sulfur/brick_stairs_to_polished", "crushing", 103, "mekanism:crushing_building", "minecraft:polished_sulfur_stairs", 1, &["Polished Sulfur Stairs"], 1),
        (780, "mekanism:crushing/sulfur/brick_wall_to_polished", "crushing", 103, "mekanism:crushing_building", "minecraft:polished_sulfur_wall", 1, &["Polished Sulfur Wall"], 1),
        (781, "mekanism:crushing/sulfur/bricks_to_polished", "crushing", 103, "mekanism:crushing_building", "minecraft:polished_sulfur", 1, &["Polished Sulfur"], 1),
        (782, "mekanism:crushing/sulfur/chiseled_to_brick", "crushing", 104, "mekanism:crushing_building", "minecraft:sulfur_bricks", 1, &["Sulfur Bricks"], 1),
        (783, "mekanism:crushing/sulfur/from_polished", "crushing", 105, "mekanism:crushing_building", "minecraft:sulfur", 1, &["Sulfur"], 1),
        (784, "mekanism:crushing/sulfur/potent_to_dust", "crushing", 106, "mekanism:crushing_misc", "mekanism:dust_sulfur", 36, &["Sulfur Dust"], 1),
        (785, "mekanism:crushing/sulfur/slabs_from_polished", "crushing", 105, "mekanism:crushing_building", "minecraft:sulfur_slab", 1, &["Sulfur Slab"], 1),
        (786, "mekanism:crushing/sulfur/spike_to_dust", "crushing", 106, "mekanism:crushing_misc", "mekanism:dust_sulfur", 1, &["Sulfur Dust"], 1),
        (787, "mekanism:crushing/sulfur/stairs_from_polished", "crushing", 105, "mekanism:crushing_building", "minecraft:sulfur_stairs", 1, &["Sulfur Stairs"], 1),
        (788, "mekanism:crushing/sulfur/to_dust", "crushing", 106, "mekanism:crushing_misc", "mekanism:dust_sulfur", 4, &["Sulfur Dust"], 1),
        (789, "mekanism:crushing/sulfur/wall_from_polished", "crushing", 105, "mekanism:crushing_building", "minecraft:sulfur_wall", 1, &["Sulfur Wall"], 1),
        (790, "mekanism:crushing/tuff/brick_slabs_to_polished", "crushing", 107, "mekanism:crushing_building", "minecraft:polished_tuff_slab", 1, &["Polished Tuff Slab"], 1),
        (791, "mekanism:crushing/tuff/brick_stairs_to_polished", "crushing", 107, "mekanism:crushing_building", "minecraft:polished_tuff_stairs", 1, &["Polished Tuff Stairs"], 1),
        (792, "mekanism:crushing/tuff/brick_wall_to_polished", "crushing", 107, "mekanism:crushing_building", "minecraft:polished_tuff_wall", 1, &["Polished Tuff Wall"], 1),
        (793, "mekanism:crushing/tuff/bricks_to_polished", "crushing", 107, "mekanism:crushing_building", "minecraft:polished_tuff", 1, &["Polished Tuff"], 1),
        (794, "mekanism:crushing/tuff/from_polished", "crushing", 108, "mekanism:crushing_building", "minecraft:tuff", 1, &["Tuff"], 1),
        (795, "mekanism:crushing/tuff/slabs_from_polished", "crushing", 108, "mekanism:crushing_building", "minecraft:tuff_slab", 1, &["Tuff Slab"], 1),
        (796, "mekanism:crushing/tuff/stairs_from_polished", "crushing", 108, "mekanism:crushing_building", "minecraft:tuff_stairs", 1, &["Tuff Stairs"], 1),
        (797, "mekanism:crushing/tuff/tuff_brick_conversion", "crushing", 109, "mekanism:crushing_building", "minecraft:tuff_bricks", 1, &["Tuff Bricks"], 1),
        (798, "mekanism:crushing/tuff/wall_from_polished", "crushing", 108, "mekanism:crushing_building", "minecraft:tuff_wall", 1, &["Tuff Wall"], 1),
        (832, "minecraft:cyan_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:cyan_glazed_terracotta", 1, &["Cyan Glazed Terracotta"], 1),
        (863, "minecraft:deepslate", "smelting", -1, "minecraft:furnace_blocks", "minecraft:deepslate", 1, &["Deepslate"], 1),
        (914, "minecraft:diamond_from_smelting_deepslate_diamond_ore", "smelting", 110, "minecraft:furnace_misc", "minecraft:diamond", 1, &["Diamond"], 1),
        (915, "minecraft:diamond_from_smelting_diamond_ore", "smelting", 110, "minecraft:furnace_misc", "minecraft:diamond", 1, &["Diamond"], 1),
        (941, "minecraft:dried_kelp_from_smelting", "smelting", -1, "minecraft:furnace_food", "minecraft:dried_kelp", 1, &["Dried Kelp"], 1),
        (1073, "minecraft:emerald_from_smelting_deepslate_emerald_ore", "smelting", 119, "minecraft:furnace_misc", "minecraft:emerald", 1, &["Emerald"], 1),
        (1074, "minecraft:emerald_from_smelting_emerald_ore", "smelting", 119, "minecraft:furnace_misc", "minecraft:emerald", 1, &["Emerald"], 1),
        (1100, "mekanism:enriching/charcoal", "enriching", -1, "mekanism:enriching_misc", "minecraft:charcoal", 1, &["Charcoal"], 1),
        (1101, "mekanism:enriching/charcoal_dust", "enriching", -1, "mekanism:enriching_misc", "mekanism:dust_charcoal", 1, &["Charcoal Dust"], 1),
        (1102, "mekanism:enriching/clay_ball", "enriching", -1, "mekanism:enriching_misc", "minecraft:clay_ball", 4, &["Clay Ball"], 1),
        (1103, "mekanism:enriching/conversion/andesite/slab_to_polished_slab", "enriching", 122, "mekanism:enriching_building", "minecraft:polished_andesite_slab", 1, &["Polished Andesite Slab"], 1),
        (1104, "mekanism:enriching/conversion/andesite/stairs_to_polished_stairs", "enriching", 122, "mekanism:enriching_building", "minecraft:polished_andesite_stairs", 1, &["Polished Andesite Stairs"], 1),
        (1105, "mekanism:enriching/conversion/andesite/to_polished", "enriching", 122, "mekanism:enriching_building", "minecraft:polished_andesite", 1, &["Polished Andesite"], 1),
        (1106, "mekanism:enriching/conversion/basalt_or_smooth_to_polished_basalt", "enriching", -1, "mekanism:enriching_building", "minecraft:polished_basalt", 1, &["Polished Basalt"], 1),
        (1107, "mekanism:enriching/conversion/blackstone/bricks_to_chiseled_bricks", "enriching", 123, "mekanism:enriching_building", "minecraft:chiseled_polished_blackstone", 1, &["Chiseled Polished Blackstone"], 1),
        (1108, "mekanism:enriching/conversion/blackstone/cracked_bricks_to_bricks", "enriching", 75, "mekanism:enriching_building", "minecraft:polished_blackstone_bricks", 1, &["Polished Blackstone Bricks"], 1),
        (1109, "mekanism:enriching/conversion/blackstone/to_cracked_bricks", "enriching", 74, "mekanism:enriching_building", "minecraft:cracked_polished_blackstone_bricks", 1, &["Cracked Polished Blackstone Bricks"], 1),
        (1110, "mekanism:enriching/conversion/cinnabar/brick_to_chiseled", "enriching", 124, "mekanism:enriching_building", "minecraft:chiseled_cinnabar", 1, &["Chiseled Cinnabar"], 1),
        (1111, "mekanism:enriching/conversion/cinnabar/polished_slabs_to_brick", "enriching", 79, "mekanism:enriching_building", "minecraft:cinnabar_brick_slab", 1, &["Cinnabar Brick Slab"], 1),
        (1112, "mekanism:enriching/conversion/cinnabar/polished_stairs_to_brick", "enriching", 79, "mekanism:enriching_building", "minecraft:cinnabar_brick_stairs", 1, &["Cinnabar Brick Stairs"], 1),
        (1113, "mekanism:enriching/conversion/cinnabar/polished_to_brick", "enriching", 79, "mekanism:enriching_building", "minecraft:cinnabar_bricks", 1, &["Cinnabar Bricks"], 1),
        (1114, "mekanism:enriching/conversion/cinnabar/polished_wall_to_brick", "enriching", 79, "mekanism:enriching_building", "minecraft:cinnabar_brick_wall", 1, &["Cinnabar Brick Wall"], 1),
        (1115, "mekanism:enriching/conversion/cinnabar/slabs_to_polished", "enriching", 78, "mekanism:enriching_building", "minecraft:polished_cinnabar_slab", 1, &["Polished Cinnabar Slab"], 1),
        (1116, "mekanism:enriching/conversion/cinnabar/stairs_to_polished", "enriching", 78, "mekanism:enriching_building", "minecraft:polished_cinnabar_stairs", 1, &["Polished Cinnabar Stairs"], 1),
        (1117, "mekanism:enriching/conversion/cinnabar/to_polished", "enriching", 78, "mekanism:enriching_building", "minecraft:polished_cinnabar", 1, &["Polished Cinnabar"], 1),
        (1118, "mekanism:enriching/conversion/cinnabar/wall_to_polished", "enriching", 78, "mekanism:enriching_building", "minecraft:polished_cinnabar_wall", 1, &["Polished Cinnabar Wall"], 1),
        (1119, "mekanism:enriching/conversion/cracked_nether_bricks_to_nether_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:nether_bricks", 1, &["Nether Bricks"], 1),
        (1120, "mekanism:enriching/conversion/deepslate/chiseled_to_cracked_tile", "enriching", 85, "mekanism:enriching_building", "minecraft:cracked_deepslate_tiles", 1, &["Cracked Deepslate Tiles"], 1),
        (1121, "mekanism:enriching/conversion/deepslate/cobbled_slabs_to_polished", "enriching", 125, "mekanism:enriching_building", "minecraft:polished_deepslate_slab", 1, &["Polished Deepslate Slab"], 1),
        (1122, "mekanism:enriching/conversion/deepslate/cobbled_stairs_to_polished", "enriching", 125, "mekanism:enriching_building", "minecraft:polished_deepslate_stairs", 1, &["Polished Deepslate Stairs"], 1),
        (1123, "mekanism:enriching/conversion/deepslate/cobbled_wall_to_polished", "enriching", 125, "mekanism:enriching_building", "minecraft:polished_deepslate_wall", 1, &["Polished Deepslate Wall"], 1),
        (1124, "mekanism:enriching/conversion/deepslate/cracked_bricks_to_bricks", "enriching", 85, "mekanism:enriching_building", "minecraft:deepslate_bricks", 1, &["Deepslate Bricks"], 1),
        (1125, "mekanism:enriching/conversion/deepslate/cracked_tile_to_tile", "enriching", 81, "mekanism:enriching_building", "minecraft:deepslate_tiles", 1, &["Deepslate Tiles"], 1),
        (1126, "mekanism:enriching/conversion/deepslate/from_cobbled", "enriching", 84, "mekanism:enriching_building", "minecraft:deepslate", 1, &["Deepslate"], 1),
        (1127, "mekanism:enriching/conversion/deepslate/polished_chiseled", "enriching", 83, "mekanism:enriching_building", "minecraft:chiseled_deepslate", 1, &["Chiseled Deepslate"], 1),
        (1128, "mekanism:enriching/conversion/deepslate/polished_slabs_to_tile", "enriching", 81, "mekanism:enriching_building", "minecraft:deepslate_tile_slab", 1, &["Deepslate Tile Slab"], 1),
        (1129, "mekanism:enriching/conversion/deepslate/polished_stairs_to_tile", "enriching", 81, "mekanism:enriching_building", "minecraft:deepslate_tile_stairs", 1, &["Deepslate Tile Stairs"], 1),
        (1130, "mekanism:enriching/conversion/deepslate/polished_wall_to_tile", "enriching", 81, "mekanism:enriching_building", "minecraft:deepslate_tile_wall", 1, &["Deepslate Tile Wall"], 1),
        (1131, "mekanism:enriching/conversion/deepslate/tile_slabs_to_brick", "enriching", 85, "mekanism:enriching_building", "minecraft:deepslate_brick_slab", 1, &["Deepslate Brick Slab"], 1),
        (1132, "mekanism:enriching/conversion/deepslate/tile_stairs_to_brick", "enriching", 85, "mekanism:enriching_building", "minecraft:deepslate_brick_stairs", 1, &["Deepslate Brick Stairs"], 1),
        (1133, "mekanism:enriching/conversion/deepslate/tile_wall_to_brick", "enriching", 85, "mekanism:enriching_building", "minecraft:deepslate_brick_wall", 1, &["Deepslate Brick Wall"], 1),
        (1134, "mekanism:enriching/conversion/deepslate/tiles_to_cracked_bricks", "enriching", 82, "mekanism:enriching_building", "minecraft:cracked_deepslate_bricks", 1, &["Cracked Deepslate Bricks"], 1),
        (1135, "mekanism:enriching/conversion/deepslate/to_polished", "enriching", 125, "mekanism:enriching_building", "minecraft:polished_deepslate", 1, &["Polished Deepslate"], 1),
        (1136, "mekanism:enriching/conversion/diorite/slab_to_polished_slab", "enriching", 126, "mekanism:enriching_building", "minecraft:polished_diorite_slab", 1, &["Polished Diorite Slab"], 1),
        (1137, "mekanism:enriching/conversion/diorite/stairs_to_polished_stairs", "enriching", 126, "mekanism:enriching_building", "minecraft:polished_diorite_stairs", 1, &["Polished Diorite Stairs"], 1),
        (1138, "mekanism:enriching/conversion/diorite/to_polished", "enriching", 126, "mekanism:enriching_building", "minecraft:polished_diorite", 1, &["Polished Diorite"], 1),
        (1139, "mekanism:enriching/conversion/granite/slab_to_polished_slab", "enriching", 127, "mekanism:enriching_building", "minecraft:polished_granite_slab", 1, &["Polished Granite Slab"], 1),
        (1140, "mekanism:enriching/conversion/granite/stairs_to_polished_stairs", "enriching", 127, "mekanism:enriching_building", "minecraft:polished_granite_stairs", 1, &["Polished Granite Stairs"], 1),
        (1141, "mekanism:enriching/conversion/granite/to_polished", "enriching", 127, "mekanism:enriching_building", "minecraft:polished_granite", 1, &["Polished Granite"], 1),
        (1142, "mekanism:enriching/conversion/gravel_to_flint", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:flint", 1, &["Flint"], 1),
        (1143, "mekanism:enriching/conversion/gunpowder_to_flint", "enriching", -1, "mekanism:enriching_misc", "minecraft:flint", 1, &["Flint"], 1),
        (1144, "mekanism:enriching/conversion/mossy/cobblestone", "enriching", 128, "mekanism:enriching_building", "minecraft:cobblestone", 1, &["Cobblestone"], 1),
        (1145, "mekanism:enriching/conversion/mossy/cobblestone_slabs", "enriching", 128, "mekanism:enriching_building", "minecraft:cobblestone_slab", 1, &["Cobblestone Slab"], 1),
        (1146, "mekanism:enriching/conversion/mossy/cobblestone_stairs", "enriching", 128, "mekanism:enriching_building", "minecraft:cobblestone_stairs", 1, &["Cobblestone Stairs"], 1),
        (1147, "mekanism:enriching/conversion/mossy/cobblestone_walls", "enriching", 128, "mekanism:enriching_building", "minecraft:cobblestone_wall", 1, &["Cobblestone Wall"], 1),
        (1148, "mekanism:enriching/conversion/mossy/stone_brick_slabs", "enriching", 129, "mekanism:enriching_building", "minecraft:stone_brick_slab", 1, &["Stone Brick Slab"], 1),
        (1149, "mekanism:enriching/conversion/mossy/stone_brick_stairs", "enriching", 129, "mekanism:enriching_building", "minecraft:stone_brick_stairs", 1, &["Stone Brick Stairs"], 1),
        (1150, "mekanism:enriching/conversion/mossy/stone_brick_walls", "enriching", 129, "mekanism:enriching_building", "minecraft:stone_brick_wall", 1, &["Stone Brick Wall"], 1),
        (1151, "mekanism:enriching/conversion/mossy/stone_bricks", "enriching", 129, "mekanism:enriching_building", "minecraft:stone_bricks", 1, &["Stone Bricks"], 1),
        (1152, "mekanism:enriching/conversion/mud/packed_to_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:mud_bricks", 1, &["Mud Bricks"], 1),
        (1153, "mekanism:enriching/conversion/mud/to_clay", "enriching", -1, "mekanism:enriching_building", "minecraft:clay", 1, &["Clay"], 1),
        (1154, "mekanism:enriching/conversion/nether_bricks_to_chiseled_nether_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:chiseled_nether_bricks", 1, &["Chiseled Nether Bricks"], 1),
        (1155, "mekanism:enriching/conversion/purpur_pillar_from_block", "enriching", -1, "mekanism:enriching_building", "minecraft:purpur_pillar", 1, &["Purpur Pillar"], 1),
        (1156, "mekanism:enriching/conversion/quartz/from_smooth_quartz", "enriching", 95, "mekanism:enriching_building", "minecraft:quartz_block", 1, &["Block of Quartz"], 1),
        (1157, "mekanism:enriching/conversion/quartz/smooth_slab_to_slab", "enriching", 95, "mekanism:enriching_building", "minecraft:quartz_slab", 1, &["Quartz Slab"], 1),
        (1158, "mekanism:enriching/conversion/quartz/smooth_stairs_to_stairs", "enriching", 95, "mekanism:enriching_building", "minecraft:quartz_stairs", 1, &["Quartz Stairs"], 1),
        (1159, "mekanism:enriching/conversion/quartz/to_item", "enriching", -1, "mekanism:enriching_misc", "minecraft:quartz", 4, &["Nether Quartz"], 1),
        (1160, "mekanism:enriching/conversion/resin/block_to_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:resin_bricks", 1, &["Resin Bricks"], 1),
        (1161, "mekanism:enriching/conversion/resin/bricks_to_chiseled", "enriching", -1, "mekanism:enriching_building", "minecraft:chiseled_resin_bricks", 1, &["Chiseled Resin Bricks"], 1),
        (1162, "mekanism:enriching/conversion/sand_to_gravel", "enriching", -1, "mekanism:enriching_building", "minecraft:gravel", 1, &["Gravel"], 1),
        (1163, "mekanism:enriching/conversion/soul_sand_to_soul_soil", "enriching", -1, "mekanism:enriching_building", "minecraft:soul_soil", 1, &["Soul Soil"], 1),
        (1164, "mekanism:enriching/conversion/stone/bricks_to_chiseled_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:chiseled_stone_bricks", 1, &["Chiseled Stone Bricks"], 1),
        (1165, "mekanism:enriching/conversion/stone/cracked_bricks_to_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:stone_bricks", 1, &["Stone Bricks"], 1),
        (1166, "mekanism:enriching/conversion/stone/to_cracked_bricks", "enriching", -1, "mekanism:enriching_building", "minecraft:cracked_stone_bricks", 1, &["Cracked Stone Bricks"], 1),
        (1167, "mekanism:enriching/conversion/sulfur/brick_to_chiseled", "enriching", 130, "mekanism:enriching_building", "minecraft:chiseled_sulfur", 1, &["Chiseled Sulfur"], 1),
        (1168, "mekanism:enriching/conversion/sulfur/dust_to_spike", "enriching", -1, "mekanism:enriching_misc", "minecraft:sulfur_spike", 1, &["Sulfur Spike"], 1),
        (1169, "mekanism:enriching/conversion/sulfur/polished_slabs_to_brick", "enriching", 104, "mekanism:enriching_building", "minecraft:sulfur_brick_slab", 1, &["Sulfur Brick Slab"], 1),
        (1170, "mekanism:enriching/conversion/sulfur/polished_stairs_to_brick", "enriching", 104, "mekanism:enriching_building", "minecraft:sulfur_brick_stairs", 1, &["Sulfur Brick Stairs"], 1),
        (1171, "mekanism:enriching/conversion/sulfur/polished_to_brick", "enriching", 104, "mekanism:enriching_building", "minecraft:sulfur_bricks", 1, &["Sulfur Bricks"], 1),
        (1172, "mekanism:enriching/conversion/sulfur/polished_wall_to_brick", "enriching", 104, "mekanism:enriching_building", "minecraft:sulfur_brick_wall", 1, &["Sulfur Brick Wall"], 1),
        (1173, "mekanism:enriching/conversion/sulfur/slabs_to_polished", "enriching", 103, "mekanism:enriching_building", "minecraft:polished_sulfur_slab", 1, &["Polished Sulfur Slab"], 1),
        (1174, "mekanism:enriching/conversion/sulfur/spike_to_gunpowder", "enriching", -1, "mekanism:enriching_misc", "minecraft:gunpowder", 1, &["Gunpowder"], 1),
        (1175, "mekanism:enriching/conversion/sulfur/stairs_to_polished", "enriching", 103, "mekanism:enriching_building", "minecraft:polished_sulfur_stairs", 1, &["Polished Sulfur Stairs"], 1),
        (1176, "mekanism:enriching/conversion/sulfur/to_polished", "enriching", 103, "mekanism:enriching_building", "minecraft:polished_sulfur", 1, &["Polished Sulfur"], 1),
        (1177, "mekanism:enriching/conversion/sulfur/wall_to_polished", "enriching", 103, "mekanism:enriching_building", "minecraft:polished_sulfur_wall", 1, &["Polished Sulfur Wall"], 1),
        (1178, "mekanism:enriching/conversion/tuff/brick_to_chiseled", "enriching", 131, "mekanism:enriching_building", "minecraft:chiseled_tuff", 1, &["Chiseled Tuff"], 1),
        (1179, "mekanism:enriching/conversion/tuff/polished_slabs_to_brick", "enriching", 109, "mekanism:enriching_building", "minecraft:tuff_brick_slab", 1, &["Tuff Brick Slab"], 1),
        (1180, "mekanism:enriching/conversion/tuff/polished_stairs_to_brick", "enriching", 109, "mekanism:enriching_building", "minecraft:tuff_brick_stairs", 1, &["Tuff Brick Stairs"], 1),
        (1181, "mekanism:enriching/conversion/tuff/polished_to_brick", "enriching", 109, "mekanism:enriching_building", "minecraft:tuff_bricks", 1, &["Tuff Bricks"], 1),
        (1182, "mekanism:enriching/conversion/tuff/polished_wall_to_brick", "enriching", 109, "mekanism:enriching_building", "minecraft:tuff_brick_wall", 1, &["Tuff Brick Wall"], 1),
        (1183, "mekanism:enriching/conversion/tuff/slabs_to_polished", "enriching", 107, "mekanism:enriching_building", "minecraft:polished_tuff_slab", 1, &["Polished Tuff Slab"], 1),
        (1184, "mekanism:enriching/conversion/tuff/stairs_to_polished", "enriching", 107, "mekanism:enriching_building", "minecraft:polished_tuff_stairs", 1, &["Polished Tuff Stairs"], 1),
        (1185, "mekanism:enriching/conversion/tuff/to_polished", "enriching", 107, "mekanism:enriching_building", "minecraft:polished_tuff", 1, &["Polished Tuff"], 1),
        (1186, "mekanism:enriching/conversion/tuff/wall_to_polished", "enriching", 107, "mekanism:enriching_building", "minecraft:polished_tuff_wall", 1, &["Polished Tuff Wall"], 1),
        (1187, "mekanism:enriching/deoxidizing/chiseled_copper", "enriching", 132, "mekanism:enriching_building", "minecraft:chiseled_copper", 1, &["Chiseled Copper"], 1),
        (1188, "mekanism:enriching/deoxidizing/copper_bars", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_bars", 1, &["Copper Bars"], 1),
        (1189, "mekanism:enriching/deoxidizing/copper_block", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_block", 1, &["Block of Copper"], 1),
        (1190, "mekanism:enriching/deoxidizing/copper_bulb", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_bulb", 1, &["Copper Bulb"], 1),
        (1191, "mekanism:enriching/deoxidizing/copper_chain", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_chain", 1, &["Copper Chain"], 1),
        (1192, "mekanism:enriching/deoxidizing/copper_chest", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_chest", 1, &["Copper Chest"], 1),
        (1193, "mekanism:enriching/deoxidizing/copper_door", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_door", 1, &["Copper Door"], 1),
        (1194, "mekanism:enriching/deoxidizing/copper_golem_statue", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_golem_statue", 1, &["Copper Golem Statue"], 1),
        (1195, "mekanism:enriching/deoxidizing/copper_grate", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_grate", 1, &["Copper Grate"], 1),
        (1196, "mekanism:enriching/deoxidizing/copper_lantern", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_lantern", 1, &["Copper Lantern"], 1),
        (1197, "mekanism:enriching/deoxidizing/copper_trapdoor", "enriching", 132, "mekanism:enriching_building", "minecraft:copper_trapdoor", 1, &["Copper Trapdoor"], 1),
        (1198, "mekanism:enriching/deoxidizing/cut_copper", "enriching", 132, "mekanism:enriching_building", "minecraft:cut_copper", 1, &["Cut Copper"], 1),
        (1199, "mekanism:enriching/deoxidizing/cut_copper_slab", "enriching", 132, "mekanism:enriching_building", "minecraft:cut_copper_slab", 1, &["Cut Copper Slab"], 1),
        (1200, "mekanism:enriching/deoxidizing/cut_copper_stairs", "enriching", 132, "mekanism:enriching_building", "minecraft:cut_copper_stairs", 1, &["Cut Copper Stairs"], 1),
        (1201, "mekanism:enriching/deoxidizing/exposed_chiseled_copper", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_chiseled_copper", 1, &["Exposed Chiseled Copper"], 1),
        (1202, "mekanism:enriching/deoxidizing/exposed_copper", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper", 1, &["Exposed Copper"], 1),
        (1203, "mekanism:enriching/deoxidizing/exposed_copper_bars", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_bars", 1, &["Exposed Copper Bars"], 1),
        (1204, "mekanism:enriching/deoxidizing/exposed_copper_bulb", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_bulb", 1, &["Exposed Copper Bulb"], 1),
        (1205, "mekanism:enriching/deoxidizing/exposed_copper_chain", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_chain", 1, &["Exposed Copper Chain"], 1),
        (1206, "mekanism:enriching/deoxidizing/exposed_copper_chest", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_chest", 1, &["Exposed Copper Chest"], 1),
        (1207, "mekanism:enriching/deoxidizing/exposed_copper_door", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_door", 1, &["Exposed Copper Door"], 1),
        (1208, "mekanism:enriching/deoxidizing/exposed_copper_golem_statue", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_golem_statue", 1, &["Exposed Copper Golem Statue"], 1),
        (1209, "mekanism:enriching/deoxidizing/exposed_copper_grate", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_grate", 1, &["Exposed Copper Grate"], 1),
        (1210, "mekanism:enriching/deoxidizing/exposed_copper_lantern", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_lantern", 1, &["Exposed Copper Lantern"], 1),
        (1211, "mekanism:enriching/deoxidizing/exposed_copper_trapdoor", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_copper_trapdoor", 1, &["Exposed Copper Trapdoor"], 1),
        (1212, "mekanism:enriching/deoxidizing/exposed_cut_copper", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_cut_copper", 1, &["Exposed Cut Copper"], 1),
        (1213, "mekanism:enriching/deoxidizing/exposed_cut_copper_slab", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_cut_copper_slab", 1, &["Exposed Cut Copper Slab"], 1),
        (1214, "mekanism:enriching/deoxidizing/exposed_cut_copper_stairs", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_cut_copper_stairs", 1, &["Exposed Cut Copper Stairs"], 1),
        (1215, "mekanism:enriching/deoxidizing/exposed_lightning_rod", "enriching", 133, "mekanism:enriching_building", "minecraft:exposed_lightning_rod", 1, &["Exposed Lightning Rod"], 1),
        (1216, "mekanism:enriching/deoxidizing/lightning_rod", "enriching", 132, "mekanism:enriching_building", "minecraft:lightning_rod", 1, &["Lightning Rod"], 1),
        (1217, "mekanism:enriching/deoxidizing/weathered_chiseled_copper", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_chiseled_copper", 1, &["Weathered Chiseled Copper"], 1),
        (1218, "mekanism:enriching/deoxidizing/weathered_copper", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper", 1, &["Weathered Copper"], 1),
        (1219, "mekanism:enriching/deoxidizing/weathered_copper_bars", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_bars", 1, &["Weathered Copper Bars"], 1),
        (1220, "mekanism:enriching/deoxidizing/weathered_copper_bulb", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_bulb", 1, &["Weathered Copper Bulb"], 1),
        (1221, "mekanism:enriching/deoxidizing/weathered_copper_chain", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_chain", 1, &["Weathered Copper Chain"], 1),
        (1222, "mekanism:enriching/deoxidizing/weathered_copper_chest", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_chest", 1, &["Weathered Copper Chest"], 1),
        (1223, "mekanism:enriching/deoxidizing/weathered_copper_door", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_door", 1, &["Weathered Copper Door"], 1),
        (1224, "mekanism:enriching/deoxidizing/weathered_copper_golem_statue", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_golem_statue", 1, &["Weathered Copper Golem Statue"], 1),
        (1225, "mekanism:enriching/deoxidizing/weathered_copper_grate", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_grate", 1, &["Weathered Copper Grate"], 1),
        (1226, "mekanism:enriching/deoxidizing/weathered_copper_lantern", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_lantern", 1, &["Weathered Copper Lantern"], 1),
        (1227, "mekanism:enriching/deoxidizing/weathered_copper_trapdoor", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_copper_trapdoor", 1, &["Weathered Copper Trapdoor"], 1),
        (1228, "mekanism:enriching/deoxidizing/weathered_cut_copper", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_cut_copper", 1, &["Weathered Cut Copper"], 1),
        (1229, "mekanism:enriching/deoxidizing/weathered_cut_copper_slab", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_cut_copper_slab", 1, &["Weathered Cut Copper Slab"], 1),
        (1230, "mekanism:enriching/deoxidizing/weathered_cut_copper_stairs", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_cut_copper_stairs", 1, &["Weathered Cut Copper Stairs"], 1),
        (1231, "mekanism:enriching/deoxidizing/weathered_lightning_rod", "enriching", 134, "mekanism:enriching_building", "minecraft:weathered_lightning_rod", 1, &["Weathered Lightning Rod"], 1),
        (1232, "mekanism:enriching/dye/black", "enriching", 28, "mekanism:enriching_decorative", "minecraft:black_dye", 2, &["Black Dye"], 1),
        (1233, "mekanism:enriching/dye/blue", "enriching", 36, "mekanism:enriching_decorative", "minecraft:blue_dye", 2, &["Blue Dye"], 1),
        (1234, "mekanism:enriching/dye/brown", "enriching", 39, "mekanism:enriching_decorative", "minecraft:brown_dye", 2, &["Brown Dye"], 1),
        (1235, "mekanism:enriching/dye/gray", "enriching", 135, "mekanism:enriching_decorative", "minecraft:gray_dye", 2, &["Gray Dye"], 1),
        (1236, "mekanism:enriching/dye/green", "enriching", 136, "mekanism:enriching_decorative", "minecraft:green_dye", 2, &["Green Dye"], 1),
        (1237, "mekanism:enriching/dye/large_cyan", "enriching", 54, "mekanism:enriching_decorative", "minecraft:cyan_dye", 4, &["Cyan Dye"], 1),
        (1238, "mekanism:enriching/dye/large_magenta", "enriching", 57, "mekanism:enriching_decorative", "minecraft:magenta_dye", 4, &["Magenta Dye"], 1),
        (1239, "mekanism:enriching/dye/large_pink", "enriching", 59, "mekanism:enriching_decorative", "minecraft:pink_dye", 4, &["Pink Dye"], 1),
        (1240, "mekanism:enriching/dye/large_red", "enriching", 137, "mekanism:enriching_decorative", "minecraft:red_dye", 4, &["Red Dye"], 1),
        (1241, "mekanism:enriching/dye/large_yellow", "enriching", 138, "mekanism:enriching_decorative", "minecraft:yellow_dye", 4, &["Yellow Dye"], 1),
        (1242, "mekanism:enriching/dye/light_blue", "enriching", 55, "mekanism:enriching_decorative", "minecraft:light_blue_dye", 2, &["Light Blue Dye"], 1),
        (1243, "mekanism:enriching/dye/light_gray", "enriching", 53, "mekanism:enriching_decorative", "minecraft:light_gray_dye", 2, &["Light Gray Dye"], 1),
        (1244, "mekanism:enriching/dye/lime", "enriching", 56, "mekanism:enriching_decorative", "minecraft:lime_dye", 2, &["Lime Dye"], 1),
        (1245, "mekanism:enriching/dye/orange", "enriching", 58, "mekanism:enriching_decorative", "minecraft:orange_dye", 2, &["Orange Dye"], 1),
        (1246, "mekanism:enriching/dye/small_magenta", "enriching", 57, "mekanism:enriching_decorative", "minecraft:magenta_dye", 2, &["Magenta Dye"], 1),
        (1247, "mekanism:enriching/dye/small_pink", "enriching", 59, "mekanism:enriching_decorative", "minecraft:pink_dye", 2, &["Pink Dye"], 1),
        (1248, "mekanism:enriching/dye/small_red", "enriching", 137, "mekanism:enriching_decorative", "minecraft:red_dye", 2, &["Red Dye"], 1),
        (1249, "mekanism:enriching/dye/small_yellow", "enriching", 138, "mekanism:enriching_decorative", "minecraft:yellow_dye", 2, &["Yellow Dye"], 1),
        (1250, "mekanism:enriching/dye/white", "enriching", 139, "mekanism:enriching_decorative", "minecraft:white_dye", 2, &["White Dye"], 1),
        (1251, "mekanism:enriching/enriched/carbon", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_carbon", 1, &["Enriched Carbon"], 1),
        (1252, "mekanism:enriching/enriched/diamond", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_diamond", 1, &["Enriched Diamond"], 1),
        (1253, "mekanism:enriching/enriched/gold", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_gold", 1, &["Enriched Gold"], 1),
        (1254, "mekanism:enriching/enriched/redstone", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_redstone", 1, &["Enriched Redstone"], 1),
        (1255, "mekanism:enriching/enriched/refined_obsidian", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_refined_obsidian", 1, &["Enriched Obsidian"], 1),
        (1256, "mekanism:enriching/enriched/tin", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:enriched_tin", 1, &["Enriched Tin"], 1),
        (1257, "mekanism:enriching/glowstone_dust", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:glowstone_dust", 4, &["Glowstone Dust"], 1),
        (1258, "mekanism:enriching/hdpe_sheet", "enriching", -1, "mekanism:enriching_misc", "mekanism:hdpe_sheet", 1, &["HDPE Sheet"], 1),
        (1259, "mekanism:enriching/salt", "enriching", -1, "mekanism:enriching_misc", "mekanism:salt", 4, &["Salt"], 1),
        (1260, "mekanism:enriching/string", "enriching", -1, "mekanism:enriching_misc", "minecraft:string", 9, &["String"], 1),
        (1358, "minecraft:glass", "smelting", -1, "minecraft:furnace_blocks", "minecraft:glass", 1, &["Glass"], 1),
        (1421, "minecraft:gold_ingot_from_smelting_deepslate_gold_ore", "smelting", 152, "minecraft:furnace_misc", "minecraft:gold_ingot", 1, &["Gold Ingot"], 1),
        (1422, "minecraft:gold_ingot_from_smelting_gold_ore", "smelting", 152, "minecraft:furnace_misc", "minecraft:gold_ingot", 1, &["Gold Ingot"], 1),
        (1423, "minecraft:gold_ingot_from_smelting_nether_gold_ore", "smelting", 152, "minecraft:furnace_misc", "minecraft:gold_ingot", 1, &["Gold Ingot"], 1),
        (1424, "minecraft:gold_ingot_from_smelting_raw_gold", "smelting", 152, "minecraft:furnace_misc", "minecraft:gold_ingot", 1, &["Gold Ingot"], 1),
        (1428, "mekanismtools:gold_nugget_from_smelting", "smelting", 153, "minecraft:furnace_misc", "minecraft:gold_nugget", 1, &["Gold Nugget"], 1),
        (1429, "minecraft:gold_nugget_from_smelting", "smelting", -1, "minecraft:furnace_misc", "minecraft:gold_nugget", 1, &["Gold Nugget"], 1),
        (1465, "minecraft:gray_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:gray_glazed_terracotta", 1, &["Gray Glazed Terracotta"], 1),
        (1485, "minecraft:green_dye", "smelting", -1, "minecraft:furnace_misc", "minecraft:green_dye", 1, &["Green Dye"], 1),
        (1486, "minecraft:green_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:green_glazed_terracotta", 1, &["Green Glazed Terracotta"], 1),
        (1506, "mekanismgenerators:hohlraum", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanismgenerators:hohlraum", 1, &["Hohlraum", "D-T Fuel: 0 mB", "Insufficient Fuel"], 1),
        (1622, "minecraft:iron_ingot_from_smelting_deepslate_iron_ore", "smelting", 161, "minecraft:furnace_misc", "minecraft:iron_ingot", 1, &["Iron Ingot"], 1),
        (1623, "minecraft:iron_ingot_from_smelting_iron_ore", "smelting", 161, "minecraft:furnace_misc", "minecraft:iron_ingot", 1, &["Iron Ingot"], 1),
        (1624, "minecraft:iron_ingot_from_smelting_raw_iron", "smelting", 161, "minecraft:furnace_misc", "minecraft:iron_ingot", 1, &["Iron Ingot"], 1),
        (1629, "mekanismtools:iron_nugget_from_smelting", "smelting", 162, "minecraft:furnace_misc", "minecraft:iron_nugget", 1, &["Iron Nugget"], 1),
        (1630, "minecraft:iron_nugget_from_smelting", "smelting", -1, "minecraft:furnace_misc", "minecraft:iron_nugget", 1, &["Iron Nugget"], 1),
        (1679, "minecraft:lapis_lazuli_from_smelting_deepslate_lapis_ore", "smelting", 163, "minecraft:furnace_misc", "minecraft:lapis_lazuli", 1, &["Lapis Lazuli"], 1),
        (1680, "minecraft:lapis_lazuli_from_smelting_lapis_ore", "smelting", 163, "minecraft:furnace_misc", "minecraft:lapis_lazuli", 1, &["Lapis Lazuli"], 1),
        (1687, "minecraft:leaf_litter", "smelting", -1, "minecraft:furnace_blocks", "minecraft:leaf_litter", 1, &["Leaf Litter"], 1),
        (1714, "minecraft:light_blue_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:light_blue_glazed_terracotta", 1, &["Light Blue Glazed Terracotta"], 1),
        (1739, "minecraft:light_gray_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:light_gray_glazed_terracotta", 1, &["Light Gray Glazed Terracotta"], 1),
        (1762, "minecraft:lime_dye_from_smelting", "smelting", -1, "minecraft:furnace_misc", "minecraft:lime_dye", 1, &["Lime Dye"], 1),
        (1763, "minecraft:lime_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:lime_glazed_terracotta", 1, &["Lime Glazed Terracotta"], 1),
        (1793, "minecraft:magenta_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:magenta_glazed_terracotta", 1, &["Magenta Glazed Terracotta"], 1),
        (1837, "mekanism:metallurgic_infusing/alloy/atomic", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:alloy_atomic", 1, &["Atomic Alloy"], 1),
        (1838, "mekanism:metallurgic_infusing/alloy/infused", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:alloy_infused", 1, &["Infused Alloy"], 1),
        (1839, "mekanism:metallurgic_infusing/alloy/reinforced", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_misc", "mekanism:alloy_reinforced", 1, &["Reinforced Alloy"], 1),
        (1840, "mekanism:metallurgic_infusing/blackstone_to_gilded_blackstone", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:gilded_blackstone", 1, &["Gilded Blackstone"], 1),
        (1841, "mekanism:metallurgic_infusing/clay_to_slime_ball", "metallurgic_infusing", 166, "mekanism:metallurgic_infusing_misc", "minecraft:slime_ball", 1, &["Slimeball"], 1),
        (1842, "mekanism:metallurgic_infusing/clay_to_slime_block", "metallurgic_infusing", 166, "mekanism:metallurgic_infusing_misc", "minecraft:slime_ball", 4, &["Slimeball"], 1),
        (1843, "mekanism:metallurgic_infusing/crimson_nylium_to_warped_nylium", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:warped_nylium", 1, &["Warped Nylium"], 1),
        (1844, "mekanism:metallurgic_infusing/dirt_to_mycelium", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:mycelium", 1, &["Mycelium"], 1),
        (1845, "mekanism:metallurgic_infusing/dirt_to_podzol", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:podzol", 1, &["Podzol"], 1),
        (1846, "mekanism:metallurgic_infusing/mossy/cobblestone", "metallurgic_infusing", 167, "mekanism:metallurgic_infusing_building", "minecraft:mossy_cobblestone", 1, &["Mossy Cobblestone"], 1),
        (1847, "mekanism:metallurgic_infusing/mossy/cobblestone_slab", "metallurgic_infusing", 167, "mekanism:metallurgic_infusing_building", "minecraft:mossy_cobblestone_slab", 1, &["Mossy Cobblestone Slab"], 1),
        (1848, "mekanism:metallurgic_infusing/mossy/cobblestone_stairs", "metallurgic_infusing", 167, "mekanism:metallurgic_infusing_building", "minecraft:mossy_cobblestone_stairs", 1, &["Mossy Cobblestone Stairs"], 1),
        (1849, "mekanism:metallurgic_infusing/mossy/cobblestone_wall", "metallurgic_infusing", 167, "mekanism:metallurgic_infusing_building", "minecraft:mossy_cobblestone_wall", 1, &["Mossy Cobblestone Wall"], 1),
        (1850, "mekanism:metallurgic_infusing/mossy/stone_brick", "metallurgic_infusing", 168, "mekanism:metallurgic_infusing_building", "minecraft:mossy_stone_bricks", 1, &["Mossy Stone Bricks"], 1),
        (1851, "mekanism:metallurgic_infusing/mossy/stone_brick_slab", "metallurgic_infusing", 168, "mekanism:metallurgic_infusing_building", "minecraft:mossy_stone_brick_slab", 1, &["Mossy Stone Brick Slab"], 1),
        (1852, "mekanism:metallurgic_infusing/mossy/stone_brick_stairs", "metallurgic_infusing", 168, "mekanism:metallurgic_infusing_building", "minecraft:mossy_stone_brick_stairs", 1, &["Mossy Stone Brick Stairs"], 1),
        (1853, "mekanism:metallurgic_infusing/mossy/stone_brick_wall", "metallurgic_infusing", 168, "mekanism:metallurgic_infusing_building", "minecraft:mossy_stone_brick_wall", 1, &["Mossy Stone Brick Wall"], 1),
        (1854, "mekanism:metallurgic_infusing/netherrack_to_crimson_nylium", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:crimson_nylium", 1, &["Crimson Nylium"], 1),
        (1855, "mekanism:metallurgic_infusing/sand_to_dirt", "metallurgic_infusing", -1, "mekanism:metallurgic_infusing_building", "minecraft:dirt", 1, &["Dirt"], 1),
        (1889, "minecraft:nether_brick", "smelting", -1, "minecraft:furnace_misc", "minecraft:nether_brick", 1, &["Nether Brick"], 1),
        (1912, "minecraft:netherite_scrap", "smelting", -1, "minecraft:furnace_misc", "minecraft:netherite_scrap", 1, &["Netherite Scrap"], 1),
        (1981, "minecraft:orange_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:orange_glazed_terracotta", 1, &["Orange Glazed Terracotta"], 1),
        (1998, "mekanismtools:osmium/nugget_from_smelting", "smelting", 174, "minecraft:furnace_misc", "mekanism:nugget_osmium", 1, &["Osmium Nugget"], 1),
        (2881, "minecraft:pink_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:pink_glazed_terracotta", 1, &["Pink Glazed Terracotta"], 1),
        (3417, "mekanismadditions:plastic/slick/enriching/aqua", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:aqua_slick_plastic", 1, &["Aqua Slick Plastic Block"], 1),
        (3418, "mekanismadditions:plastic/slick/enriching/black", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:black_slick_plastic", 1, &["Black Slick Plastic Block"], 1),
        (3419, "mekanismadditions:plastic/slick/enriching/blue", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:blue_slick_plastic", 1, &["Blue Slick Plastic Block"], 1),
        (3420, "mekanismadditions:plastic/slick/enriching/brown", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:brown_slick_plastic", 1, &["Brown Slick Plastic Block"], 1),
        (3421, "mekanismadditions:plastic/slick/enriching/cyan", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:cyan_slick_plastic", 1, &["Cyan Slick Plastic Block"], 1),
        (3422, "mekanismadditions:plastic/slick/enriching/dark_red", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:dark_red_slick_plastic", 1, &["Dark Red Slick Plastic Block"], 1),
        (3423, "mekanismadditions:plastic/slick/enriching/gray", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:gray_slick_plastic", 1, &["Gray Slick Plastic Block"], 1),
        (3424, "mekanismadditions:plastic/slick/enriching/green", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:green_slick_plastic", 1, &["Green Slick Plastic Block"], 1),
        (3425, "mekanismadditions:plastic/slick/enriching/light_blue", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:light_blue_slick_plastic", 1, &["Light Blue Slick Plastic Block"], 1),
        (3426, "mekanismadditions:plastic/slick/enriching/light_gray", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:light_gray_slick_plastic", 1, &["Light Gray Slick Plastic Block"], 1),
        (3427, "mekanismadditions:plastic/slick/enriching/lime", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:lime_slick_plastic", 1, &["Lime Slick Plastic Block"], 1),
        (3428, "mekanismadditions:plastic/slick/enriching/magenta", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:magenta_slick_plastic", 1, &["Magenta Slick Plastic Block"], 1),
        (3429, "mekanismadditions:plastic/slick/enriching/orange", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:orange_slick_plastic", 1, &["Orange Slick Plastic Block"], 1),
        (3430, "mekanismadditions:plastic/slick/enriching/pink", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:pink_slick_plastic", 1, &["Pink Slick Plastic Block"], 1),
        (3431, "mekanismadditions:plastic/slick/enriching/purple", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:purple_slick_plastic", 1, &["Purple Slick Plastic Block"], 1),
        (3432, "mekanismadditions:plastic/slick/enriching/red", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:red_slick_plastic", 1, &["Red Slick Plastic Block"], 1),
        (3433, "mekanismadditions:plastic/slick/enriching/white", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:white_slick_plastic", 1, &["White Slick Plastic Block"], 1),
        (3434, "mekanismadditions:plastic/slick/enriching/yellow", "enriching", 239, "mekanism:enriching_building", "mekanismadditions:yellow_slick_plastic", 1, &["Yellow Slick Plastic Block"], 1),
        (3804, "minecraft:popped_chorus_fruit", "smelting", -1, "minecraft:furnace_misc", "minecraft:popped_chorus_fruit", 1, &["Popped Chorus Fruit"], 1),
        (3824, "mekanism:processing/bronze/dust/from_infusing", "metallurgic_infusing", 254, "mekanism:metallurgic_infusing_misc", "mekanism:dust_bronze", 4, &["Bronze Dust"], 1),
        (3825, "mekanism:processing/bronze/dust/from_ingot", "crushing", 254, "mekanism:crushing_misc", "mekanism:dust_bronze", 1, &["Bronze Dust"], 1),
        (3828, "mekanism:processing/bronze/ingot/from_dust_smelting", "smelting", 255, "minecraft:furnace_misc", "mekanism:ingot_bronze", 1, &["Bronze Ingot"], 1),
        (3829, "mekanism:processing/bronze/ingot/from_infusing", "metallurgic_infusing", 255, "mekanism:metallurgic_infusing_misc", "mekanism:ingot_bronze", 4, &["Bronze Ingot"], 1),
        (3831, "mekanism:processing/coal/from_dust", "enriching", 51, "mekanism:enriching_efficiency", "minecraft:coal", 1, &["Coal"], 1),
        (3832, "mekanism:processing/coal/from_ore", "enriching", 51, "mekanism:enriching_efficiency", "minecraft:coal", 2, &["Coal"], 1),
        (3834, "mekanism:processing/coal/to_dust", "crushing", 257, "mekanism:crushing_misc", "mekanism:dust_coal", 1, &["Coal Dust"], 1),
        (3841, "mekanism:processing/copper/dirty_dust/from_clump", "crushing", 259, "mekanism:crushing_efficiency", "mekanism:dirty_dust_copper", 1, &["Dirty Copper Dust"], 1),
        (3842, "mekanism:processing/copper/dust/from_dirty_dust", "enriching", 260, "mekanism:enriching_efficiency", "mekanism:dust_copper", 1, &["Copper Dust"], 1),
        (3843, "mekanism:processing/copper/dust/from_ingot", "crushing", 260, "mekanism:crushing_misc", "mekanism:dust_copper", 1, &["Copper Dust"], 1),
        (3844, "mekanism:processing/copper/dust/from_ore", "enriching", 260, "mekanism:enriching_efficiency", "mekanism:dust_copper", 2, &["Copper Dust"], 1),
        (3845, "mekanism:processing/copper/dust/from_raw_block", "enriching", 260, "mekanism:enriching_efficiency", "mekanism:dust_copper", 12, &["Copper Dust"], 1),
        (3846, "mekanism:processing/copper/dust/from_raw_ore", "enriching", 260, "mekanism:enriching_efficiency", "mekanism:dust_copper", 4, &["Copper Dust"], 1),
        (3848, "mekanism:processing/copper/ingot/from_dust_smelting", "smelting", 68, "minecraft:furnace_misc", "minecraft:copper_ingot", 1, &["Copper Ingot"], 1),
        (3859, "mekanism:processing/diamond/from_dust", "enriching", 110, "mekanism:enriching_misc", "minecraft:diamond", 1, &["Diamond"], 1),
        (3860, "mekanism:processing/diamond/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:diamond", 2, &["Diamond"], 1),
        (3862, "mekanism:processing/diamond/to_dust", "crushing", 265, "mekanism:crushing_misc", "mekanism:dust_diamond", 1, &["Diamond Dust"], 1),
        (3864, "mekanism:processing/emerald/from_dust", "enriching", 119, "mekanism:enriching_misc", "minecraft:emerald", 1, &["Emerald"], 1),
        (3865, "mekanism:processing/emerald/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:emerald", 2, &["Emerald"], 1),
        (3867, "mekanism:processing/emerald/to_dust", "crushing", 267, "mekanism:crushing_misc", "mekanism:dust_emerald", 1, &["Emerald Dust"], 1),
        (3871, "mekanism:processing/fluorite/from_deepslate_ore_smelting", "smelting", 268, "minecraft:furnace_misc", "mekanism:fluorite_gem", 1, &["Fluorite"], 1),
        (3872, "mekanism:processing/fluorite/from_dust", "enriching", 268, "mekanism:enriching_misc", "mekanism:fluorite_gem", 1, &["Fluorite"], 1),
        (3873, "mekanism:processing/fluorite/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "mekanism:fluorite_gem", 6, &["Fluorite"], 1),
        (3875, "mekanism:processing/fluorite/from_ore_smelting", "smelting", 268, "minecraft:furnace_misc", "mekanism:fluorite_gem", 1, &["Fluorite"], 1),
        (3877, "mekanism:processing/fluorite/to_dust", "crushing", 270, "mekanism:crushing_misc", "mekanism:dust_fluorite", 1, &["Fluorite Dust"], 1),
        (3884, "mekanism:processing/gold/dirty_dust/from_clump", "crushing", 272, "mekanism:crushing_efficiency", "mekanism:dirty_dust_gold", 1, &["Dirty Gold Dust"], 1),
        (3885, "mekanism:processing/gold/dust/from_dirty_dust", "enriching", 273, "mekanism:enriching_efficiency", "mekanism:dust_gold", 1, &["Gold Dust"], 1),
        (3886, "mekanism:processing/gold/dust/from_ingot", "crushing", 273, "mekanism:crushing_misc", "mekanism:dust_gold", 1, &["Gold Dust"], 1),
        (3887, "mekanism:processing/gold/dust/from_ore", "enriching", 273, "mekanism:enriching_efficiency", "mekanism:dust_gold", 2, &["Gold Dust"], 1),
        (3888, "mekanism:processing/gold/dust/from_raw_block", "enriching", 273, "mekanism:enriching_efficiency", "mekanism:dust_gold", 12, &["Gold Dust"], 1),
        (3889, "mekanism:processing/gold/dust/from_raw_ore", "enriching", 273, "mekanism:enriching_efficiency", "mekanism:dust_gold", 4, &["Gold Dust"], 1),
        (3891, "mekanism:processing/gold/ingot/from_dust_smelting", "smelting", 152, "minecraft:furnace_misc", "minecraft:gold_ingot", 1, &["Gold Ingot"], 1),
        (3908, "mekanism:processing/iron/dirty_dust/from_clump", "crushing", 278, "mekanism:crushing_efficiency", "mekanism:dirty_dust_iron", 1, &["Dirty Iron Dust"], 1),
        (3909, "mekanism:processing/iron/dust/from_dirty_dust", "enriching", 279, "mekanism:enriching_efficiency", "mekanism:dust_iron", 1, &["Iron Dust"], 1),
        (3910, "mekanism:processing/iron/dust/from_ingot", "crushing", 279, "mekanism:crushing_misc", "mekanism:dust_iron", 1, &["Iron Dust"], 1),
        (3911, "mekanism:processing/iron/dust/from_ore", "enriching", 279, "mekanism:enriching_efficiency", "mekanism:dust_iron", 2, &["Iron Dust"], 1),
        (3912, "mekanism:processing/iron/dust/from_raw_block", "enriching", 279, "mekanism:enriching_efficiency", "mekanism:dust_iron", 12, &["Iron Dust"], 1),
        (3913, "mekanism:processing/iron/dust/from_raw_ore", "enriching", 279, "mekanism:enriching_efficiency", "mekanism:dust_iron", 4, &["Iron Dust"], 1),
        (3914, "mekanism:processing/iron/enriched", "metallurgic_infusing", 280, "mekanism:metallurgic_infusing_misc", "mekanism:enriched_iron", 1, &["Enriched Iron"], 1),
        (3915, "mekanism:processing/iron/enriched_dust", "metallurgic_infusing", 280, "mekanism:metallurgic_infusing_misc", "mekanism:enriched_iron", 1, &["Enriched Iron"], 1),
        (3917, "mekanism:processing/iron/ingot/from_dust_smelting", "smelting", 161, "minecraft:furnace_misc", "minecraft:iron_ingot", 1, &["Iron Ingot"], 1),
        (3928, "mekanism:processing/lapis_lazuli/from_dust", "enriching", 163, "mekanism:enriching_misc", "minecraft:lapis_lazuli", 1, &["Lapis Lazuli"], 1),
        (3929, "mekanism:processing/lapis_lazuli/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:lapis_lazuli", 12, &["Lapis Lazuli"], 1),
        (3931, "mekanism:processing/lapis_lazuli/to_dust", "crushing", 285, "mekanism:crushing_misc", "mekanism:dust_lapis_lazuli", 1, &["Lapis Lazuli Dust"], 1),
        (3944, "mekanism:processing/lead/dirty_dust/from_clump", "crushing", 288, "mekanism:crushing_efficiency", "mekanism:dirty_dust_lead", 1, &["Dirty Lead Dust"], 1),
        (3945, "mekanism:processing/lead/dust/from_dirty_dust", "enriching", 289, "mekanism:enriching_efficiency", "mekanism:dust_lead", 1, &["Lead Dust"], 1),
        (3946, "mekanism:processing/lead/dust/from_ingot", "crushing", 289, "mekanism:crushing_misc", "mekanism:dust_lead", 1, &["Lead Dust"], 1),
        (3947, "mekanism:processing/lead/dust/from_ore", "enriching", 289, "mekanism:enriching_efficiency", "mekanism:dust_lead", 2, &["Lead Dust"], 1),
        (3948, "mekanism:processing/lead/dust/from_raw_block", "enriching", 289, "mekanism:enriching_efficiency", "mekanism:dust_lead", 12, &["Lead Dust"], 1),
        (3949, "mekanism:processing/lead/dust/from_raw_ore", "enriching", 289, "mekanism:enriching_efficiency", "mekanism:dust_lead", 4, &["Lead Dust"], 1),
        (3952, "mekanism:processing/lead/ingot/from_deepslate_ore_smelting", "smelting", 290, "minecraft:furnace_misc", "mekanism:ingot_lead", 1, &["Lead Ingot"], 1),
        (3954, "mekanism:processing/lead/ingot/from_dust_smelting", "smelting", 290, "minecraft:furnace_misc", "mekanism:ingot_lead", 1, &["Lead Ingot"], 1),
        (3957, "mekanism:processing/lead/ingot/from_ore_smelting", "smelting", 290, "minecraft:furnace_misc", "mekanism:ingot_lead", 1, &["Lead Ingot"], 1),
        (3959, "mekanism:processing/lead/ingot/from_raw_smelting", "smelting", 290, "minecraft:furnace_misc", "mekanism:ingot_lead", 1, &["Lead Ingot"], 1),
        (3974, "mekanism:processing/netherite/ancient_debris_to_dirty_scrap", "crushing", -1, "mekanism:crushing_efficiency", "mekanism:dirty_netherite_scrap", 3, &["Dirty Netherite Scrap"], 1),
        (3975, "mekanism:processing/netherite/ancient_debris_to_scrap", "enriching", 294, "mekanism:enriching_efficiency", "minecraft:netherite_scrap", 2, &["Netherite Scrap"], 1),
        (3976, "mekanism:processing/netherite/dirty_scrap_to_scrap", "enriching", 294, "mekanism:enriching_efficiency", "minecraft:netherite_scrap", 1, &["Netherite Scrap"], 1),
        (3979, "mekanism:processing/netherite/ingot_from_dust_smelting", "smelting", -1, "minecraft:furnace_misc", "minecraft:netherite_ingot", 1, &["Netherite Ingot"], 1),
        (3980, "mekanism:processing/netherite/ingot_to_dust", "crushing", 296, "mekanism:crushing_misc", "mekanism:dust_netherite", 1, &["Netherite Dust"], 1),
        (3981, "mekanism:processing/netherite/scrap_to_dust", "metallurgic_infusing", 296, "mekanism:metallurgic_infusing_misc", "mekanism:dust_netherite", 1, &["Netherite Dust"], 1),
        (3987, "mekanism:processing/osmium/dirty_dust/from_clump", "crushing", 298, "mekanism:crushing_efficiency", "mekanism:dirty_dust_osmium", 1, &["Dirty Osmium Dust"], 1),
        (3988, "mekanism:processing/osmium/dust/from_dirty_dust", "enriching", 299, "mekanism:enriching_efficiency", "mekanism:dust_osmium", 1, &["Osmium Dust"], 1),
        (3989, "mekanism:processing/osmium/dust/from_ingot", "crushing", 299, "mekanism:crushing_misc", "mekanism:dust_osmium", 1, &["Osmium Dust"], 1),
        (3990, "mekanism:processing/osmium/dust/from_ore", "enriching", 299, "mekanism:enriching_efficiency", "mekanism:dust_osmium", 2, &["Osmium Dust"], 1),
        (3991, "mekanism:processing/osmium/dust/from_raw_block", "enriching", 299, "mekanism:enriching_efficiency", "mekanism:dust_osmium", 12, &["Osmium Dust"], 1),
        (3992, "mekanism:processing/osmium/dust/from_raw_ore", "enriching", 299, "mekanism:enriching_efficiency", "mekanism:dust_osmium", 4, &["Osmium Dust"], 1),
        (3995, "mekanism:processing/osmium/ingot/from_deepslate_ore_smelting", "smelting", 300, "minecraft:furnace_misc", "mekanism:ingot_osmium", 1, &["Osmium Ingot"], 1),
        (3997, "mekanism:processing/osmium/ingot/from_dust_smelting", "smelting", 300, "minecraft:furnace_misc", "mekanism:ingot_osmium", 1, &["Osmium Ingot"], 1),
        (4000, "mekanism:processing/osmium/ingot/from_ore_smelting", "smelting", 300, "minecraft:furnace_misc", "mekanism:ingot_osmium", 1, &["Osmium Ingot"], 1),
        (4002, "mekanism:processing/osmium/ingot/from_raw_smelting", "smelting", 300, "minecraft:furnace_misc", "mekanism:ingot_osmium", 1, &["Osmium Ingot"], 1),
        (4017, "mekanism:processing/quartz/from_dust", "enriching", 304, "mekanism:enriching_misc", "minecraft:quartz", 1, &["Nether Quartz"], 1),
        (4018, "mekanism:processing/quartz/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:quartz", 2, &["Nether Quartz"], 1),
        (4019, "mekanism:processing/quartz/to_dust", "crushing", 305, "mekanism:crushing_misc", "mekanism:dust_quartz", 1, &["Quartz Dust"], 1),
        (4021, "mekanism:processing/redstone/from_ore", "enriching", -1, "mekanism:enriching_efficiency", "minecraft:redstone", 12, &["Redstone Dust"], 1),
        (4027, "mekanism:processing/refined_glowstone/ingot_to_dust", "crushing", -1, "mekanism:crushing_misc", "minecraft:glowstone_dust", 1, &["Glowstone Dust"], 1),
        (4028, "mekanism:processing/refined_obsidian/dust/from_ingot", "crushing", 309, "mekanism:crushing_misc", "mekanism:dust_refined_obsidian", 1, &["Refined Obsidian Dust"], 1),
        (4029, "mekanism:processing/refined_obsidian/dust/from_obsidian_dust", "metallurgic_infusing", 309, "mekanism:metallurgic_infusing_misc", "mekanism:dust_refined_obsidian", 1, &["Refined Obsidian Dust"], 1),
        (4033, "mekanism:processing/steel/enriched_iron_to_dust", "metallurgic_infusing", 311, "mekanism:metallurgic_infusing_misc", "mekanism:dust_steel", 1, &["Steel Dust"], 1),
        (4036, "mekanism:processing/steel/ingot/from_dust_smelting", "smelting", 312, "minecraft:furnace_misc", "mekanism:ingot_steel", 1, &["Steel Ingot"], 1),
        (4038, "mekanism:processing/steel/ingot_to_dust", "crushing", 311, "mekanism:crushing_misc", "mekanism:dust_steel", 1, &["Steel Dust"], 1),
        (4044, "mekanism:processing/tin/dirty_dust/from_clump", "crushing", 314, "mekanism:crushing_efficiency", "mekanism:dirty_dust_tin", 1, &["Dirty Tin Dust"], 1),
        (4045, "mekanism:processing/tin/dust/from_dirty_dust", "enriching", 315, "mekanism:enriching_efficiency", "mekanism:dust_tin", 1, &["Tin Dust"], 1),
        (4046, "mekanism:processing/tin/dust/from_ingot", "crushing", 315, "mekanism:crushing_misc", "mekanism:dust_tin", 1, &["Tin Dust"], 1),
        (4047, "mekanism:processing/tin/dust/from_ore", "enriching", 315, "mekanism:enriching_efficiency", "mekanism:dust_tin", 2, &["Tin Dust"], 1),
        (4048, "mekanism:processing/tin/dust/from_raw_block", "enriching", 315, "mekanism:enriching_efficiency", "mekanism:dust_tin", 12, &["Tin Dust"], 1),
        (4049, "mekanism:processing/tin/dust/from_raw_ore", "enriching", 315, "mekanism:enriching_efficiency", "mekanism:dust_tin", 4, &["Tin Dust"], 1),
        (4052, "mekanism:processing/tin/ingot/from_deepslate_ore_smelting", "smelting", 316, "minecraft:furnace_misc", "mekanism:ingot_tin", 1, &["Tin Ingot"], 1),
        (4054, "mekanism:processing/tin/ingot/from_dust_smelting", "smelting", 316, "minecraft:furnace_misc", "mekanism:ingot_tin", 1, &["Tin Ingot"], 1),
        (4057, "mekanism:processing/tin/ingot/from_ore_smelting", "smelting", 316, "minecraft:furnace_misc", "mekanism:ingot_tin", 1, &["Tin Ingot"], 1),
        (4059, "mekanism:processing/tin/ingot/from_raw_smelting", "smelting", 316, "minecraft:furnace_misc", "mekanism:ingot_tin", 1, &["Tin Ingot"], 1),
        (4079, "mekanism:processing/uranium/dirty_dust/from_clump", "crushing", 321, "mekanism:crushing_efficiency", "mekanism:dirty_dust_uranium", 1, &["Dirty Uranium Dust"], 1),
        (4080, "mekanism:processing/uranium/dust/from_dirty_dust", "enriching", 322, "mekanism:enriching_efficiency", "mekanism:dust_uranium", 1, &["Uranium Dust"], 1),
        (4081, "mekanism:processing/uranium/dust/from_ingot", "crushing", 322, "mekanism:crushing_misc", "mekanism:dust_uranium", 1, &["Uranium Dust"], 1),
        (4082, "mekanism:processing/uranium/dust/from_ore", "enriching", 322, "mekanism:enriching_efficiency", "mekanism:dust_uranium", 2, &["Uranium Dust"], 1),
        (4083, "mekanism:processing/uranium/dust/from_raw_block", "enriching", 322, "mekanism:enriching_efficiency", "mekanism:dust_uranium", 12, &["Uranium Dust"], 1),
        (4084, "mekanism:processing/uranium/dust/from_raw_ore", "enriching", 322, "mekanism:enriching_efficiency", "mekanism:dust_uranium", 4, &["Uranium Dust"], 1),
        (4090, "mekanism:processing/uranium/ingot/from_deepslate_ore_smelting", "smelting", 324, "minecraft:furnace_misc", "mekanism:ingot_uranium", 1, &["Uranium Ingot"], 1),
        (4092, "mekanism:processing/uranium/ingot/from_dust_smelting", "smelting", 324, "minecraft:furnace_misc", "mekanism:ingot_uranium", 1, &["Uranium Ingot"], 1),
        (4095, "mekanism:processing/uranium/ingot/from_ore_smelting", "smelting", 324, "minecraft:furnace_misc", "mekanism:ingot_uranium", 1, &["Uranium Ingot"], 1),
        (4097, "mekanism:processing/uranium/ingot/from_raw_smelting", "smelting", 324, "minecraft:furnace_misc", "mekanism:ingot_uranium", 1, &["Uranium Ingot"], 1),
        (4116, "mekanism:processing/uranium/yellow_cake_uranium", "enriching", -1, "mekanism:enriching_misc", "mekanism:yellow_cake_uranium", 2, &["Yellow Cake Uranium"], 1),
        (4132, "minecraft:purple_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:purple_glazed_terracotta", 1, &["Purple Glazed Terracotta"], 1),
        (4158, "minecraft:quartz", "smelting", -1, "minecraft:furnace_misc", "minecraft:quartz", 1, &["Nether Quartz"], 1),
        (4217, "minecraft:red_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:red_glazed_terracotta", 1, &["Red Glazed Terracotta"], 1),
        (4244, "minecraft:redstone_from_smelting_deepslate_redstone_ore", "smelting", 333, "minecraft:furnace_blocks", "minecraft:redstone", 1, &["Redstone Dust"], 1),
        (4245, "minecraft:redstone_from_smelting_redstone_ore", "smelting", 333, "minecraft:furnace_blocks", "minecraft:redstone", 1, &["Redstone Dust"], 1),
        (4255, "mekanismtools:refined_glowstone/nugget_from_smelting", "smelting", 334, "minecraft:furnace_misc", "mekanism:nugget_refined_glowstone", 1, &["Refined Glowstone Nugget"], 1),
        (4271, "mekanismtools:refined_obsidian/nugget_from_smelting", "smelting", 335, "minecraft:furnace_misc", "mekanism:nugget_refined_obsidian", 1, &["Refined Obsidian Nugget"], 1),
        (4282, "minecraft:resin_brick", "smelting", -1, "minecraft:furnace_misc", "minecraft:resin_brick", 1, &["Resin Brick"], 1),
        (4530, "minecraft:smooth_basalt", "smelting", -1, "minecraft:furnace_blocks", "minecraft:smooth_basalt", 1, &["Smooth Basalt"], 1),
        (4531, "minecraft:smooth_quartz", "smelting", -1, "minecraft:furnace_blocks", "minecraft:smooth_quartz", 1, &["Smooth Quartz Block"], 1),
        (4536, "minecraft:smooth_red_sandstone", "smelting", -1, "minecraft:furnace_blocks", "minecraft:smooth_red_sandstone", 1, &["Smooth Red Sandstone"], 1),
        (4541, "minecraft:smooth_sandstone", "smelting", -1, "minecraft:furnace_blocks", "minecraft:smooth_sandstone", 1, &["Smooth Sandstone"], 1),
        (4546, "minecraft:smooth_stone", "smelting", -1, "minecraft:furnace_blocks", "minecraft:smooth_stone", 1, &["Smooth Stone"], 1),
        (4563, "minecraft:sponge", "smelting", -1, "minecraft:furnace_blocks", "minecraft:sponge", 1, &["Sponge"], 1),
        (4589, "mekanismtools:steel/nugget_from_smelting", "smelting", 350, "minecraft:furnace_misc", "mekanism:nugget_steel", 1, &["Steel Nugget"], 1),
        (4602, "minecraft:stone", "smelting", -1, "minecraft:furnace_blocks", "minecraft:stone", 1, &["Stone"], 1),
        (4699, "minecraft:terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:terracotta", 1, &["Terracotta"], 1),
        (4940, "minecraft:white_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:white_glazed_terracotta", 1, &["White Glazed Terracotta"], 1),
        (4978, "minecraft:yellow_glazed_terracotta", "smelting", -1, "minecraft:furnace_blocks", "minecraft:yellow_glazed_terracotta", 1, &["Yellow Glazed Terracotta"], 1),
    ];
    /// O que cada display pede: `(id, craftingRequirements (os itens de cada ingrediente), quantidades)`, com `quantidades` o `resolveForFirstStack(...).count()` de cada ingrediente de um `MekanismRecipeDisplay` (`None` nos outros displays).
    pub const REQUIREMENTS: &[machinery_basis_core::recipe_book_view::Requirement] = &[
        (31, Some(&[&["minecraft:potato"]]), None),
        (139, Some(&[&["minecraft:black_terracotta"]]), None),
        (170, Some(&[&["minecraft:blue_terracotta"]]), None),
        (194, Some(&[&["minecraft:clay_ball"]]), None),
        (209, Some(&[&["mekanismtools:bronze_axe", "mekanismtools:bronze_boots", "mekanismtools:bronze_chestplate", "mekanismtools:bronze_helmet", "mekanismtools:bronze_hoe", "mekanismtools:bronze_horse_armor", "mekanismtools:bronze_leggings", "mekanismtools:bronze_nautilus_armor", "mekanismtools:bronze_paxel", "mekanismtools:bronze_pickaxe", "mekanismtools:bronze_shield", "mekanismtools:bronze_shovel", "mekanismtools:bronze_spear", "mekanismtools:bronze_sword"]]), None),
        (230, Some(&[&["minecraft:brown_terracotta"]]), None),
        (252, Some(&[&["minecraft:acacia_log", "minecraft:acacia_wood", "minecraft:birch_log", "minecraft:birch_wood", "minecraft:cherry_log", "minecraft:cherry_wood", "minecraft:dark_oak_log", "minecraft:dark_oak_wood", "minecraft:jungle_log", "minecraft:jungle_wood", "minecraft:mangrove_log", "minecraft:mangrove_wood", "minecraft:oak_log", "minecraft:oak_wood", "minecraft:pale_oak_log", "minecraft:pale_oak_wood", "minecraft:poplar_log", "minecraft:poplar_wood", "minecraft:spruce_log", "minecraft:spruce_wood", "minecraft:stripped_acacia_log", "minecraft:stripped_acacia_wood", "minecraft:stripped_birch_log", "minecraft:stripped_birch_wood", "minecraft:stripped_cherry_log", "minecraft:stripped_cherry_wood", "minecraft:stripped_dark_oak_log", "minecraft:stripped_dark_oak_wood", "minecraft:stripped_jungle_log", "minecraft:stripped_jungle_wood", "minecraft:stripped_mangrove_log", "minecraft:stripped_mangrove_wood", "minecraft:stripped_oak_log", "minecraft:stripped_oak_wood", "minecraft:stripped_pale_oak_log", "minecraft:stripped_pale_oak_wood", "minecraft:stripped_poplar_log", "minecraft:stripped_poplar_wood", "minecraft:stripped_spruce_log", "minecraft:stripped_spruce_wood"]]), None),
        (370, Some(&[&["minecraft:coal_ore"]]), None),
        (371, Some(&[&["minecraft:deepslate_coal_ore"]]), None),
        (481, Some(&[&["mekanism:ingot_osmium"]]), Some(&[1, 0])),
        (483, Some(&[&["mekanism:basic_control_circuit"]]), Some(&[1, 0])),
        (484, Some(&[&["mekanism:advanced_control_circuit"]]), Some(&[1, 0])),
        (485, Some(&[&["mekanism:elite_control_circuit"]]), Some(&[1, 0])),
        (487, Some(&[&["minecraft:beef"]]), None),
        (490, Some(&[&["minecraft:chicken"]]), None),
        (493, Some(&[&["minecraft:cod"]]), None),
        (496, Some(&[&["minecraft:mutton"]]), None),
        (499, Some(&[&["minecraft:porkchop"]]), None),
        (502, Some(&[&["minecraft:rabbit"]]), None),
        (505, Some(&[&["minecraft:salmon"]]), None),
        (527, Some(&[&["minecraft:copper_ore"]]), None),
        (528, Some(&[&["minecraft:deepslate_copper_ore"]]), None),
        (529, Some(&[&["minecraft:raw_copper"]]), None),
        (536, Some(&[&["mekanismtools:copper_paxel"]]), None),
        (537, Some(&[&["minecraft:copper_axe", "minecraft:copper_boots", "minecraft:copper_chestplate", "minecraft:copper_helmet", "minecraft:copper_hoe", "minecraft:copper_horse_armor", "minecraft:copper_leggings", "minecraft:copper_nautilus_armor", "minecraft:copper_pickaxe", "minecraft:copper_shovel", "minecraft:copper_spear", "minecraft:copper_sword"]]), None),
        (545, Some(&[&["minecraft:deepslate_bricks"]]), None),
        (546, Some(&[&["minecraft:deepslate_tiles"]]), None),
        (547, Some(&[&["minecraft:nether_bricks"]]), None),
        (548, Some(&[&["minecraft:polished_blackstone_bricks"]]), None),
        (549, Some(&[&["minecraft:stone_bricks"]]), None),
        (570, Some(&[&["minecraft:polished_andesite"]]), Some(&[1])),
        (571, Some(&[&["minecraft:polished_andesite_slab"]]), Some(&[1])),
        (572, Some(&[&["minecraft:polished_andesite_stairs"]]), Some(&[1])),
        (573, Some(&[&["minecraft:apple"]]), Some(&[1])),
        (574, Some(&[&["minecraft:baked_potato"]]), Some(&[1])),
        (575, Some(&[&["minecraft:bamboo"]]), Some(&[1])),
        (576, Some(&[&["minecraft:glow_berries", "minecraft:sweet_berries"]]), Some(&[1])),
        (577, Some(&[&["minecraft:big_dripleaf"]]), Some(&[1])),
        (578, Some(&[&["minecraft:bread"]]), Some(&[1])),
        (579, Some(&[&["minecraft:bush", "minecraft:red_shrub"]]), Some(&[1])),
        (580, Some(&[&["minecraft:cactus_flower"]]), Some(&[1])),
        (581, Some(&[&["minecraft:cake"]]), Some(&[1])),
        (582, Some(&[&["minecraft:carved_pumpkin"]]), Some(&[1])),
        (583, Some(&[&["minecraft:cookie"]]), Some(&[4])),
        (584, Some(&[&["minecraft:crimson_roots"]]), Some(&[1])),
        (585, Some(&[&["minecraft:poisonous_potato"]]), Some(&[1])),
        (586, Some(&[&["minecraft:beetroot", "minecraft:cactus", "minecraft:carrot", "minecraft:cocoa_beans", "minecraft:nether_wart", "minecraft:potato", "minecraft:wheat"]]), Some(&[1])),
        (587, Some(&[&["minecraft:dried_kelp"]]), Some(&[1])),
        (588, Some(&[&["minecraft:dried_kelp_block"]]), Some(&[1])),
        (589, Some(&[&["minecraft:fern"]]), Some(&[1])),
        (590, Some(&[&["minecraft:firefly_bush"]]), Some(&[1])),
        (591, Some(&[&["minecraft:crimson_fungus", "minecraft:warped_fungus"]]), Some(&[1])),
        (592, Some(&[&["minecraft:glow_lichen"]]), Some(&[1])),
        (593, Some(&[&["minecraft:hanging_roots"]]), Some(&[1])),
        (594, Some(&[&["minecraft:hay_block"]]), Some(&[1])),
        (595, Some(&[&["minecraft:kelp"]]), Some(&[1])),
        (596, Some(&[&["minecraft:large_fern"]]), Some(&[1])),
        (597, Some(&[&["minecraft:leaf_litter"]]), Some(&[10])),
        (598, Some(&[&["minecraft:acacia_leaves", "minecraft:azalea_leaves", "minecraft:birch_leaves", "minecraft:cherry_leaves", "minecraft:dark_oak_leaves", "minecraft:flowering_azalea_leaves", "minecraft:jungle_leaves", "minecraft:mangrove_leaves", "minecraft:oak_leaves", "minecraft:orange_poplar_leaves", "minecraft:pale_oak_leaves", "minecraft:red_poplar_leaves", "minecraft:spruce_leaves", "minecraft:yellow_poplar_leaves"]]), Some(&[10])),
        (599, Some(&[&["minecraft:lily_pad"]]), Some(&[1])),
        (600, Some(&[&["minecraft:mangrove_roots"]]), Some(&[1])),
        (601, Some(&[&["minecraft:melon"]]), Some(&[1])),
        (602, Some(&[&["minecraft:melon_slice"]]), Some(&[1])),
        (603, Some(&[&["minecraft:moss_block", "minecraft:pale_moss_block"]]), Some(&[1])),
        (604, Some(&[&["minecraft:moss_carpet", "minecraft:pale_moss_carpet"]]), Some(&[3])),
        (605, Some(&[&["minecraft:brown_mushroom_block", "minecraft:mushroom_stem", "minecraft:red_mushroom_block"]]), Some(&[1])),
        (606, Some(&[&["minecraft:brown_mushroom", "minecraft:red_mushroom", "minecraft:shelf_mushroom"]]), Some(&[1])),
        (607, Some(&[&["minecraft:nether_sprouts"]]), Some(&[1])),
        (608, Some(&[&["minecraft:nether_wart_block"]]), Some(&[1])),
        (609, Some(&[&["minecraft:pale_hanging_moss"]]), Some(&[1])),
        (610, Some(&[&["minecraft:pink_petals"]]), Some(&[1])),
        (611, Some(&[&["minecraft:pitcher_pod"]]), Some(&[1])),
        (612, Some(&[&["minecraft:pumpkin"]]), Some(&[1])),
        (613, Some(&[&["minecraft:pumpkin_pie"]]), Some(&[1])),
        (614, Some(&[&["minecraft:rotten_flesh"]]), Some(&[1])),
        (615, Some(&[&["minecraft:acacia_sapling", "minecraft:azalea", "minecraft:birch_sapling", "minecraft:cherry_sapling", "minecraft:dark_oak_sapling", "minecraft:flowering_azalea", "minecraft:jungle_sapling", "minecraft:mangrove_propagule", "minecraft:oak_sapling", "minecraft:pale_oak_sapling", "minecraft:poplar_sapling", "minecraft:spruce_sapling"]]), Some(&[1])),
        (616, Some(&[&["minecraft:sea_pickle"]]), Some(&[1])),
        (617, Some(&[&["minecraft:seagrass"]]), Some(&[1])),
        (618, Some(&[&["minecraft:beetroot_seeds", "minecraft:melon_seeds", "minecraft:pitcher_pod", "minecraft:pumpkin_seeds", "minecraft:torchflower_seeds", "minecraft:wheat_seeds"]]), Some(&[1])),
        (619, Some(&[&["minecraft:short_dry_grass", "minecraft:short_grass"]]), Some(&[1])),
        (620, Some(&[&["minecraft:shroomlight"]]), Some(&[1])),
        (621, Some(&[&["minecraft:small_dripleaf"]]), Some(&[1])),
        (622, Some(&[&["minecraft:allium", "minecraft:azure_bluet", "minecraft:blue_orchid", "minecraft:closed_eyeblossom", "minecraft:cornflower", "minecraft:dandelion", "minecraft:golden_dandelion", "minecraft:lily_of_the_valley", "minecraft:open_eyeblossom", "minecraft:orange_tulip", "minecraft:oxeye_daisy", "minecraft:pink_tulip", "minecraft:poppy", "minecraft:red_tulip", "minecraft:torchflower", "minecraft:white_tulip", "minecraft:wither_rose"]]), Some(&[1])),
        (623, Some(&[&["minecraft:spore_blossom"]]), Some(&[1])),
        (624, Some(&[&["minecraft:straw_bed"]]), Some(&[2])),
        (625, Some(&[&["minecraft:sugar_cane"]]), Some(&[1])),
        (626, Some(&[&["minecraft:lilac", "minecraft:peony", "minecraft:pitcher_plant", "minecraft:rose_bush", "minecraft:sunflower"]]), Some(&[1])),
        (627, Some(&[&["minecraft:tall_dry_grass", "minecraft:tall_grass"]]), Some(&[1])),
        (628, Some(&[&["minecraft:twisting_vines", "minecraft:vine", "minecraft:weeping_vines"]]), Some(&[1])),
        (629, Some(&[&["minecraft:warped_roots"]]), Some(&[1])),
        (630, Some(&[&["minecraft:warped_wart_block"]]), Some(&[1])),
        (631, Some(&[&["minecraft:wildflowers"]]), Some(&[1])),
        (632, Some(&[&["minecraft:polished_blackstone_bricks"]]), Some(&[1])),
        (633, Some(&[&["minecraft:chiseled_polished_blackstone"]]), Some(&[1])),
        (634, Some(&[&["minecraft:cracked_polished_blackstone_bricks"]]), Some(&[1])),
        (635, Some(&[&["minecraft:polished_blackstone"]]), Some(&[1])),
        (636, Some(&[&["minecraft:polished_blackstone_slab"]]), Some(&[1])),
        (637, Some(&[&["minecraft:polished_blackstone_stairs"]]), Some(&[1])),
        (638, Some(&[&["minecraft:polished_blackstone_wall"]]), Some(&[1])),
        (639, Some(&[&["minecraft:blaze_rod"]]), Some(&[1])),
        (640, Some(&[&["minecraft:bone"]]), Some(&[1])),
        (641, Some(&[&["minecraft:bone_block"]]), Some(&[1])),
        (642, Some(&[&["minecraft:music_disc_5"]]), Some(&[1])),
        (643, Some(&[&["minecraft:breeze_rod"]]), Some(&[1])),
        (644, Some(&[&["minecraft:charcoal"]]), Some(&[1])),
        (645, Some(&[&["minecraft:chiseled_nether_bricks"]]), Some(&[1])),
        (646, Some(&[&["minecraft:cinnabar_brick_slab"]]), Some(&[1])),
        (647, Some(&[&["minecraft:cinnabar_brick_stairs"]]), Some(&[1])),
        (648, Some(&[&["minecraft:cinnabar_brick_wall"]]), Some(&[1])),
        (649, Some(&[&["minecraft:cinnabar_bricks"]]), Some(&[1])),
        (650, Some(&[&["minecraft:chiseled_cinnabar"]]), Some(&[1])),
        (651, Some(&[&["minecraft:polished_cinnabar"]]), Some(&[1])),
        (652, Some(&[&["minecraft:polished_cinnabar_slab"]]), Some(&[1])),
        (653, Some(&[&["minecraft:polished_cinnabar_stairs"]]), Some(&[1])),
        (654, Some(&[&["minecraft:polished_cinnabar_wall"]]), Some(&[1])),
        (655, Some(&[&["minecraft:cobblestone"]]), Some(&[1])),
        (656, Some(&[&["minecraft:deepslate_brick_slab"]]), Some(&[1])),
        (657, Some(&[&["minecraft:deepslate_brick_stairs"]]), Some(&[1])),
        (658, Some(&[&["minecraft:deepslate_brick_wall"]]), Some(&[1])),
        (659, Some(&[&["minecraft:deepslate_bricks"]]), Some(&[1])),
        (660, Some(&[&["minecraft:cracked_deepslate_bricks"]]), Some(&[1])),
        (661, Some(&[&["minecraft:cracked_deepslate_tiles"]]), Some(&[1])),
        (662, Some(&[&["minecraft:chiseled_deepslate"]]), Some(&[1])),
        (663, Some(&[&["minecraft:polished_deepslate_slab"]]), Some(&[1])),
        (664, Some(&[&["minecraft:polished_deepslate_stairs"]]), Some(&[1])),
        (665, Some(&[&["minecraft:polished_deepslate"]]), Some(&[1])),
        (666, Some(&[&["minecraft:polished_deepslate_wall"]]), Some(&[1])),
        (667, Some(&[&["minecraft:deepslate_tile_slab"]]), Some(&[1])),
        (668, Some(&[&["minecraft:deepslate_tile_stairs"]]), Some(&[1])),
        (669, Some(&[&["minecraft:deepslate_tiles"]]), Some(&[1])),
        (670, Some(&[&["minecraft:deepslate_tile_wall"]]), Some(&[1])),
        (671, Some(&[&["minecraft:deepslate"]]), Some(&[1])),
        (672, Some(&[&["minecraft:waxed_chiseled_copper"]]), Some(&[1])),
        (673, Some(&[&["minecraft:waxed_copper_bars"]]), Some(&[1])),
        (674, Some(&[&["minecraft:waxed_copper_block"]]), Some(&[1])),
        (675, Some(&[&["minecraft:waxed_copper_bulb"]]), Some(&[1])),
        (676, Some(&[&["minecraft:waxed_copper_chain"]]), Some(&[1])),
        (677, Some(&[&["minecraft:waxed_copper_chest"]]), Some(&[1])),
        (678, Some(&[&["minecraft:waxed_copper_door"]]), Some(&[1])),
        (679, Some(&[&["minecraft:waxed_copper_golem_statue"]]), Some(&[1])),
        (680, Some(&[&["minecraft:waxed_copper_grate"]]), Some(&[1])),
        (681, Some(&[&["minecraft:waxed_copper_lantern"]]), Some(&[1])),
        (682, Some(&[&["minecraft:waxed_copper_trapdoor"]]), Some(&[1])),
        (683, Some(&[&["minecraft:waxed_cut_copper"]]), Some(&[1])),
        (684, Some(&[&["minecraft:waxed_cut_copper_slab"]]), Some(&[1])),
        (685, Some(&[&["minecraft:waxed_cut_copper_stairs"]]), Some(&[1])),
        (686, Some(&[&["minecraft:waxed_exposed_chiseled_copper"]]), Some(&[1])),
        (687, Some(&[&["minecraft:waxed_exposed_copper"]]), Some(&[1])),
        (688, Some(&[&["minecraft:waxed_exposed_copper_bars"]]), Some(&[1])),
        (689, Some(&[&["minecraft:waxed_exposed_copper_bulb"]]), Some(&[1])),
        (690, Some(&[&["minecraft:waxed_exposed_copper_chain"]]), Some(&[1])),
        (691, Some(&[&["minecraft:waxed_exposed_copper_chest"]]), Some(&[1])),
        (692, Some(&[&["minecraft:waxed_exposed_copper_door"]]), Some(&[1])),
        (693, Some(&[&["minecraft:waxed_exposed_copper_golem_statue"]]), Some(&[1])),
        (694, Some(&[&["minecraft:waxed_exposed_copper_grate"]]), Some(&[1])),
        (695, Some(&[&["minecraft:waxed_exposed_copper_lantern"]]), Some(&[1])),
        (696, Some(&[&["minecraft:waxed_exposed_copper_trapdoor"]]), Some(&[1])),
        (697, Some(&[&["minecraft:waxed_exposed_cut_copper"]]), Some(&[1])),
        (698, Some(&[&["minecraft:waxed_exposed_cut_copper_slab"]]), Some(&[1])),
        (699, Some(&[&["minecraft:waxed_exposed_cut_copper_stairs"]]), Some(&[1])),
        (700, Some(&[&["minecraft:waxed_exposed_lightning_rod"]]), Some(&[1])),
        (701, Some(&[&["minecraft:waxed_lightning_rod"]]), Some(&[1])),
        (702, Some(&[&["minecraft:waxed_oxidized_chiseled_copper"]]), Some(&[1])),
        (703, Some(&[&["minecraft:waxed_oxidized_copper"]]), Some(&[1])),
        (704, Some(&[&["minecraft:waxed_oxidized_copper_bars"]]), Some(&[1])),
        (705, Some(&[&["minecraft:waxed_oxidized_copper_bulb"]]), Some(&[1])),
        (706, Some(&[&["minecraft:waxed_oxidized_copper_chain"]]), Some(&[1])),
        (707, Some(&[&["minecraft:waxed_oxidized_copper_chest"]]), Some(&[1])),
        (708, Some(&[&["minecraft:waxed_oxidized_copper_door"]]), Some(&[1])),
        (709, Some(&[&["minecraft:waxed_oxidized_copper_golem_statue"]]), Some(&[1])),
        (710, Some(&[&["minecraft:waxed_oxidized_copper_grate"]]), Some(&[1])),
        (711, Some(&[&["minecraft:waxed_oxidized_copper_lantern"]]), Some(&[1])),
        (712, Some(&[&["minecraft:waxed_oxidized_copper_trapdoor"]]), Some(&[1])),
        (713, Some(&[&["minecraft:waxed_oxidized_cut_copper"]]), Some(&[1])),
        (714, Some(&[&["minecraft:waxed_oxidized_cut_copper_slab"]]), Some(&[1])),
        (715, Some(&[&["minecraft:waxed_oxidized_cut_copper_stairs"]]), Some(&[1])),
        (716, Some(&[&["minecraft:waxed_oxidized_lightning_rod"]]), Some(&[1])),
        (717, Some(&[&["minecraft:waxed_weathered_chiseled_copper"]]), Some(&[1])),
        (718, Some(&[&["minecraft:waxed_weathered_copper"]]), Some(&[1])),
        (719, Some(&[&["minecraft:waxed_weathered_copper_bars"]]), Some(&[1])),
        (720, Some(&[&["minecraft:waxed_weathered_copper_bulb"]]), Some(&[1])),
        (721, Some(&[&["minecraft:waxed_weathered_copper_chain"]]), Some(&[1])),
        (722, Some(&[&["minecraft:waxed_weathered_copper_chest"]]), Some(&[1])),
        (723, Some(&[&["minecraft:waxed_weathered_copper_door"]]), Some(&[1])),
        (724, Some(&[&["minecraft:waxed_weathered_copper_golem_statue"]]), Some(&[1])),
        (725, Some(&[&["minecraft:waxed_weathered_copper_grate"]]), Some(&[1])),
        (726, Some(&[&["minecraft:waxed_weathered_copper_lantern"]]), Some(&[1])),
        (727, Some(&[&["minecraft:waxed_weathered_copper_trapdoor"]]), Some(&[1])),
        (728, Some(&[&["minecraft:waxed_weathered_cut_copper"]]), Some(&[1])),
        (729, Some(&[&["minecraft:waxed_weathered_cut_copper_slab"]]), Some(&[1])),
        (730, Some(&[&["minecraft:waxed_weathered_cut_copper_stairs"]]), Some(&[1])),
        (731, Some(&[&["minecraft:waxed_weathered_lightning_rod"]]), Some(&[1])),
        (732, Some(&[&["minecraft:polished_diorite"]]), Some(&[1])),
        (733, Some(&[&["minecraft:polished_diorite_slab"]]), Some(&[1])),
        (734, Some(&[&["minecraft:polished_diorite_stairs"]]), Some(&[1])),
        (735, Some(&[&["minecraft:dried_ghast"]]), Some(&[1])),
        (736, Some(&[&["minecraft:flint"]]), Some(&[1])),
        (737, Some(&[&["minecraft:polished_granite"]]), Some(&[1])),
        (738, Some(&[&["minecraft:polished_granite_slab"]]), Some(&[1])),
        (739, Some(&[&["minecraft:polished_granite_stairs"]]), Some(&[1])),
        (740, Some(&[&["minecraft:gravel"]]), Some(&[1])),
        (741, Some(&[&["minecraft:honeycomb_block"]]), Some(&[1])),
        (742, Some(&[&["minecraft:mud_bricks"]]), Some(&[1])),
        (743, Some(&[&["minecraft:nether_bricks"]]), Some(&[1])),
        (744, Some(&[&["minecraft:obsidian"]]), Some(&[1])),
        (745, Some(&[&["minecraft:dripstone_block"]]), Some(&[1])),
        (746, Some(&[&["minecraft:polished_basalt", "minecraft:smooth_basalt"]]), Some(&[1])),
        (747, Some(&[&["minecraft:prismarine"]]), Some(&[1])),
        (748, Some(&[&["minecraft:prismarine_bricks"]]), Some(&[1])),
        (749, Some(&[&["minecraft:prismarine_brick_slab"]]), Some(&[2])),
        (750, Some(&[&["minecraft:prismarine_brick_stairs"]]), Some(&[1])),
        (751, Some(&[&["minecraft:prismarine_slab"]]), Some(&[1])),
        (752, Some(&[&["minecraft:prismarine_stairs"]]), Some(&[1])),
        (753, Some(&[&["minecraft:prismarine_wall"]]), Some(&[1])),
        (754, Some(&[&["minecraft:purpur_pillar"]]), Some(&[1])),
        (755, Some(&[&["minecraft:quartz_bricks"]]), Some(&[1])),
        (756, Some(&[&["minecraft:chiseled_quartz_block"]]), Some(&[1])),
        (757, Some(&[&["minecraft:quartz_pillar"]]), Some(&[1])),
        (758, Some(&[&["minecraft:quartz_slab"]]), Some(&[1])),
        (759, Some(&[&["minecraft:smooth_quartz"]]), Some(&[1])),
        (760, Some(&[&["minecraft:quartz_stairs"]]), Some(&[1])),
        (761, Some(&[&["minecraft:quartz_block"]]), Some(&[1])),
        (762, Some(&[&["minecraft:chiseled_red_sandstone", "minecraft:cut_red_sandstone", "minecraft:red_sandstone", "minecraft:smooth_red_sandstone"]]), Some(&[1])),
        (763, Some(&[&["minecraft:chiseled_resin_bricks"]]), Some(&[1])),
        (764, Some(&[&["minecraft:resin_block"]]), Some(&[1])),
        (765, Some(&[&["minecraft:resin_bricks"]]), Some(&[1])),
        (766, Some(&[&["minecraft:chiseled_sandstone", "minecraft:cut_sandstone", "minecraft:sandstone", "minecraft:smooth_sandstone"]]), Some(&[1])),
        (767, Some(&[&["minecraft:soul_soil"]]), Some(&[1])),
        (768, Some(&[&["minecraft:stone_bricks"]]), Some(&[1])),
        (769, Some(&[&["minecraft:chiseled_stone_bricks"]]), Some(&[1])),
        (770, Some(&[&["minecraft:cracked_stone_bricks"]]), Some(&[1])),
        (771, Some(&[&["minecraft:stone_slab"]]), Some(&[1])),
        (772, Some(&[&["minecraft:stone_stairs"]]), Some(&[1])),
        (773, Some(&[&["minecraft:stone"]]), Some(&[1])),
        (774, Some(&[&["minecraft:black_cushion", "minecraft:blue_cushion", "minecraft:brown_cushion", "minecraft:cyan_cushion", "minecraft:gray_cushion", "minecraft:green_cushion", "minecraft:light_blue_cushion", "minecraft:light_gray_cushion", "minecraft:lime_cushion", "minecraft:magenta_cushion", "minecraft:orange_cushion", "minecraft:pink_cushion", "minecraft:purple_cushion", "minecraft:red_cushion", "minecraft:white_cushion", "minecraft:yellow_cushion"]]), Some(&[1])),
        (775, Some(&[&["minecraft:black_wool", "minecraft:blue_wool", "minecraft:brown_wool", "minecraft:cyan_wool", "minecraft:gray_wool", "minecraft:green_wool", "minecraft:light_blue_wool", "minecraft:light_gray_wool", "minecraft:lime_wool", "minecraft:magenta_wool", "minecraft:orange_wool", "minecraft:pink_wool", "minecraft:purple_wool", "minecraft:red_wool", "minecraft:white_wool", "minecraft:yellow_wool"]]), Some(&[1])),
        (776, Some(&[&["minecraft:black_wool_slab", "minecraft:blue_wool_slab", "minecraft:brown_wool_slab", "minecraft:cyan_wool_slab", "minecraft:gray_wool_slab", "minecraft:green_wool_slab", "minecraft:light_blue_wool_slab", "minecraft:light_gray_wool_slab", "minecraft:lime_wool_slab", "minecraft:magenta_wool_slab", "minecraft:orange_wool_slab", "minecraft:pink_wool_slab", "minecraft:purple_wool_slab", "minecraft:red_wool_slab", "minecraft:white_wool_slab", "minecraft:yellow_wool_slab"]]), Some(&[1])),
        (777, Some(&[&["minecraft:black_wool_stairs", "minecraft:blue_wool_stairs", "minecraft:brown_wool_stairs", "minecraft:cyan_wool_stairs", "minecraft:gray_wool_stairs", "minecraft:green_wool_stairs", "minecraft:light_blue_wool_stairs", "minecraft:light_gray_wool_stairs", "minecraft:lime_wool_stairs", "minecraft:magenta_wool_stairs", "minecraft:orange_wool_stairs", "minecraft:pink_wool_stairs", "minecraft:purple_wool_stairs", "minecraft:red_wool_stairs", "minecraft:white_wool_stairs", "minecraft:yellow_wool_stairs"]]), Some(&[1])),
        (778, Some(&[&["minecraft:sulfur_brick_slab"]]), Some(&[1])),
        (779, Some(&[&["minecraft:sulfur_brick_stairs"]]), Some(&[1])),
        (780, Some(&[&["minecraft:sulfur_brick_wall"]]), Some(&[1])),
        (781, Some(&[&["minecraft:sulfur_bricks"]]), Some(&[1])),
        (782, Some(&[&["minecraft:chiseled_sulfur"]]), Some(&[1])),
        (783, Some(&[&["minecraft:polished_sulfur"]]), Some(&[1])),
        (784, Some(&[&["minecraft:potent_sulfur"]]), Some(&[1])),
        (785, Some(&[&["minecraft:polished_sulfur_slab"]]), Some(&[1])),
        (786, Some(&[&["minecraft:sulfur_spike"]]), Some(&[1])),
        (787, Some(&[&["minecraft:polished_sulfur_stairs"]]), Some(&[1])),
        (788, Some(&[&["minecraft:sulfur"]]), Some(&[1])),
        (789, Some(&[&["minecraft:polished_sulfur_wall"]]), Some(&[1])),
        (790, Some(&[&["minecraft:tuff_brick_slab"]]), Some(&[1])),
        (791, Some(&[&["minecraft:tuff_brick_stairs"]]), Some(&[1])),
        (792, Some(&[&["minecraft:tuff_brick_wall"]]), Some(&[1])),
        (793, Some(&[&["minecraft:tuff_bricks"]]), Some(&[1])),
        (794, Some(&[&["minecraft:polished_tuff"]]), Some(&[1])),
        (795, Some(&[&["minecraft:polished_tuff_slab"]]), Some(&[1])),
        (796, Some(&[&["minecraft:polished_tuff_stairs"]]), Some(&[1])),
        (797, Some(&[&["minecraft:chiseled_tuff"]]), Some(&[1])),
        (798, Some(&[&["minecraft:polished_tuff_wall"]]), Some(&[1])),
        (832, Some(&[&["minecraft:cyan_terracotta"]]), None),
        (863, Some(&[&["minecraft:cobbled_deepslate"]]), None),
        (914, Some(&[&["minecraft:deepslate_diamond_ore"]]), None),
        (915, Some(&[&["minecraft:diamond_ore"]]), None),
        (941, Some(&[&["minecraft:kelp"]]), None),
        (1073, Some(&[&["minecraft:deepslate_emerald_ore"]]), None),
        (1074, Some(&[&["minecraft:emerald_ore"]]), None),
        (1100, Some(&[&["mekanism:dust_charcoal"]]), Some(&[1])),
        (1101, Some(&[&["mekanism:sawdust"]]), Some(&[8])),
        (1102, Some(&[&["minecraft:clay"]]), Some(&[1])),
        (1103, Some(&[&["minecraft:andesite_slab"]]), Some(&[1])),
        (1104, Some(&[&["minecraft:andesite_stairs"]]), Some(&[1])),
        (1105, Some(&[&["minecraft:andesite"]]), Some(&[1])),
        (1106, Some(&[&["minecraft:basalt", "minecraft:smooth_basalt"]]), Some(&[1])),
        (1107, Some(&[&["minecraft:polished_blackstone_bricks"]]), Some(&[1])),
        (1108, Some(&[&["minecraft:cracked_polished_blackstone_bricks"]]), Some(&[1])),
        (1109, Some(&[&["minecraft:polished_blackstone"]]), Some(&[1])),
        (1110, Some(&[&["minecraft:cinnabar_bricks"]]), Some(&[1])),
        (1111, Some(&[&["minecraft:polished_cinnabar_slab"]]), Some(&[1])),
        (1112, Some(&[&["minecraft:polished_cinnabar_stairs"]]), Some(&[1])),
        (1113, Some(&[&["minecraft:polished_cinnabar"]]), Some(&[1])),
        (1114, Some(&[&["minecraft:polished_cinnabar_wall"]]), Some(&[1])),
        (1115, Some(&[&["minecraft:cinnabar_slab"]]), Some(&[1])),
        (1116, Some(&[&["minecraft:cinnabar_stairs"]]), Some(&[1])),
        (1117, Some(&[&["minecraft:cinnabar"]]), Some(&[1])),
        (1118, Some(&[&["minecraft:cinnabar_wall"]]), Some(&[1])),
        (1119, Some(&[&["minecraft:cracked_nether_bricks"]]), Some(&[1])),
        (1120, Some(&[&["minecraft:chiseled_deepslate"]]), Some(&[1])),
        (1121, Some(&[&["minecraft:cobbled_deepslate_slab"]]), Some(&[1])),
        (1122, Some(&[&["minecraft:cobbled_deepslate_stairs"]]), Some(&[1])),
        (1123, Some(&[&["minecraft:cobbled_deepslate_wall"]]), Some(&[1])),
        (1124, Some(&[&["minecraft:cracked_deepslate_bricks"]]), Some(&[1])),
        (1125, Some(&[&["minecraft:cracked_deepslate_tiles"]]), Some(&[1])),
        (1126, Some(&[&["minecraft:cobbled_deepslate"]]), Some(&[1])),
        (1127, Some(&[&["minecraft:polished_deepslate"]]), Some(&[1])),
        (1128, Some(&[&["minecraft:polished_deepslate_slab"]]), Some(&[1])),
        (1129, Some(&[&["minecraft:polished_deepslate_stairs"]]), Some(&[1])),
        (1130, Some(&[&["minecraft:polished_deepslate_wall"]]), Some(&[1])),
        (1131, Some(&[&["minecraft:deepslate_tile_slab"]]), Some(&[1])),
        (1132, Some(&[&["minecraft:deepslate_tile_stairs"]]), Some(&[1])),
        (1133, Some(&[&["minecraft:deepslate_tile_wall"]]), Some(&[1])),
        (1134, Some(&[&["minecraft:deepslate_tiles"]]), Some(&[1])),
        (1135, Some(&[&["minecraft:deepslate"]]), Some(&[1])),
        (1136, Some(&[&["minecraft:diorite_slab"]]), Some(&[1])),
        (1137, Some(&[&["minecraft:diorite_stairs"]]), Some(&[1])),
        (1138, Some(&[&["minecraft:diorite"]]), Some(&[1])),
        (1139, Some(&[&["minecraft:granite_slab"]]), Some(&[1])),
        (1140, Some(&[&["minecraft:granite_stairs"]]), Some(&[1])),
        (1141, Some(&[&["minecraft:granite"]]), Some(&[1])),
        (1142, Some(&[&["minecraft:gravel"]]), Some(&[1])),
        (1143, Some(&[&["minecraft:gunpowder"]]), Some(&[1])),
        (1144, Some(&[&["minecraft:mossy_cobblestone"]]), Some(&[1])),
        (1145, Some(&[&["minecraft:mossy_cobblestone_slab"]]), Some(&[1])),
        (1146, Some(&[&["minecraft:mossy_cobblestone_stairs"]]), Some(&[1])),
        (1147, Some(&[&["minecraft:mossy_cobblestone_wall"]]), Some(&[1])),
        (1148, Some(&[&["minecraft:mossy_stone_brick_slab"]]), Some(&[1])),
        (1149, Some(&[&["minecraft:mossy_stone_brick_stairs"]]), Some(&[1])),
        (1150, Some(&[&["minecraft:mossy_stone_brick_wall"]]), Some(&[1])),
        (1151, Some(&[&["minecraft:mossy_stone_bricks"]]), Some(&[1])),
        (1152, Some(&[&["minecraft:packed_mud"]]), Some(&[1])),
        (1153, Some(&[&["minecraft:mud"]]), Some(&[1])),
        (1154, Some(&[&["minecraft:nether_bricks"]]), Some(&[1])),
        (1155, Some(&[&["minecraft:purpur_block"]]), Some(&[1])),
        (1156, Some(&[&["minecraft:smooth_quartz"]]), Some(&[1])),
        (1157, Some(&[&["minecraft:smooth_quartz_slab"]]), Some(&[1])),
        (1158, Some(&[&["minecraft:smooth_quartz_stairs"]]), Some(&[1])),
        (1159, Some(&[&["minecraft:chiseled_quartz_block", "minecraft:quartz_block", "minecraft:quartz_bricks", "minecraft:quartz_pillar"]]), Some(&[1])),
        (1160, Some(&[&["minecraft:resin_block"]]), Some(&[1])),
        (1161, Some(&[&["minecraft:resin_bricks"]]), Some(&[1])),
        (1162, Some(&[&["minecraft:red_sand", "minecraft:sand"]]), Some(&[1])),
        (1163, Some(&[&["minecraft:soul_sand"]]), Some(&[1])),
        (1164, Some(&[&["minecraft:stone_bricks"]]), Some(&[1])),
        (1165, Some(&[&["minecraft:cracked_stone_bricks"]]), Some(&[1])),
        (1166, Some(&[&["minecraft:stone"]]), Some(&[1])),
        (1167, Some(&[&["minecraft:sulfur_bricks"]]), Some(&[1])),
        (1168, Some(&[&["mekanism:dust_sulfur"]]), Some(&[1])),
        (1169, Some(&[&["minecraft:polished_sulfur_slab"]]), Some(&[1])),
        (1170, Some(&[&["minecraft:polished_sulfur_stairs"]]), Some(&[1])),
        (1171, Some(&[&["minecraft:polished_sulfur"]]), Some(&[1])),
        (1172, Some(&[&["minecraft:polished_sulfur_wall"]]), Some(&[1])),
        (1173, Some(&[&["minecraft:sulfur_slab"]]), Some(&[1])),
        (1174, Some(&[&["minecraft:sulfur_spike"]]), Some(&[1])),
        (1175, Some(&[&["minecraft:sulfur_stairs"]]), Some(&[1])),
        (1176, Some(&[&["minecraft:sulfur"]]), Some(&[1])),
        (1177, Some(&[&["minecraft:sulfur_wall"]]), Some(&[1])),
        (1178, Some(&[&["minecraft:tuff_bricks"]]), Some(&[1])),
        (1179, Some(&[&["minecraft:polished_tuff_slab"]]), Some(&[1])),
        (1180, Some(&[&["minecraft:polished_tuff_stairs"]]), Some(&[1])),
        (1181, Some(&[&["minecraft:polished_tuff"]]), Some(&[1])),
        (1182, Some(&[&["minecraft:polished_tuff_wall"]]), Some(&[1])),
        (1183, Some(&[&["minecraft:tuff_slab"]]), Some(&[1])),
        (1184, Some(&[&["minecraft:tuff_stairs"]]), Some(&[1])),
        (1185, Some(&[&["minecraft:tuff"]]), Some(&[1])),
        (1186, Some(&[&["minecraft:tuff_wall"]]), Some(&[1])),
        (1187, Some(&[&["minecraft:exposed_chiseled_copper"]]), Some(&[1])),
        (1188, Some(&[&["minecraft:exposed_copper_bars"]]), Some(&[1])),
        (1189, Some(&[&["minecraft:exposed_copper"]]), Some(&[1])),
        (1190, Some(&[&["minecraft:exposed_copper_bulb"]]), Some(&[1])),
        (1191, Some(&[&["minecraft:exposed_copper_chain"]]), Some(&[1])),
        (1192, Some(&[&["minecraft:exposed_copper_chest"]]), Some(&[1])),
        (1193, Some(&[&["minecraft:exposed_copper_door"]]), Some(&[1])),
        (1194, Some(&[&["minecraft:exposed_copper_golem_statue"]]), Some(&[1])),
        (1195, Some(&[&["minecraft:exposed_copper_grate"]]), Some(&[1])),
        (1196, Some(&[&["minecraft:exposed_copper_lantern"]]), Some(&[1])),
        (1197, Some(&[&["minecraft:exposed_copper_trapdoor"]]), Some(&[1])),
        (1198, Some(&[&["minecraft:exposed_cut_copper"]]), Some(&[1])),
        (1199, Some(&[&["minecraft:exposed_cut_copper_slab"]]), Some(&[1])),
        (1200, Some(&[&["minecraft:exposed_cut_copper_stairs"]]), Some(&[1])),
        (1201, Some(&[&["minecraft:weathered_chiseled_copper"]]), Some(&[1])),
        (1202, Some(&[&["minecraft:weathered_copper"]]), Some(&[1])),
        (1203, Some(&[&["minecraft:weathered_copper_bars"]]), Some(&[1])),
        (1204, Some(&[&["minecraft:weathered_copper_bulb"]]), Some(&[1])),
        (1205, Some(&[&["minecraft:weathered_copper_chain"]]), Some(&[1])),
        (1206, Some(&[&["minecraft:weathered_copper_chest"]]), Some(&[1])),
        (1207, Some(&[&["minecraft:weathered_copper_door"]]), Some(&[1])),
        (1208, Some(&[&["minecraft:weathered_copper_golem_statue"]]), Some(&[1])),
        (1209, Some(&[&["minecraft:weathered_copper_grate"]]), Some(&[1])),
        (1210, Some(&[&["minecraft:weathered_copper_lantern"]]), Some(&[1])),
        (1211, Some(&[&["minecraft:weathered_copper_trapdoor"]]), Some(&[1])),
        (1212, Some(&[&["minecraft:weathered_cut_copper"]]), Some(&[1])),
        (1213, Some(&[&["minecraft:weathered_cut_copper_slab"]]), Some(&[1])),
        (1214, Some(&[&["minecraft:weathered_cut_copper_stairs"]]), Some(&[1])),
        (1215, Some(&[&["minecraft:weathered_lightning_rod"]]), Some(&[1])),
        (1216, Some(&[&["minecraft:exposed_lightning_rod"]]), Some(&[1])),
        (1217, Some(&[&["minecraft:oxidized_chiseled_copper"]]), Some(&[1])),
        (1218, Some(&[&["minecraft:oxidized_copper"]]), Some(&[1])),
        (1219, Some(&[&["minecraft:oxidized_copper_bars"]]), Some(&[1])),
        (1220, Some(&[&["minecraft:oxidized_copper_bulb"]]), Some(&[1])),
        (1221, Some(&[&["minecraft:oxidized_copper_chain"]]), Some(&[1])),
        (1222, Some(&[&["minecraft:oxidized_copper_chest"]]), Some(&[1])),
        (1223, Some(&[&["minecraft:oxidized_copper_door"]]), Some(&[1])),
        (1224, Some(&[&["minecraft:oxidized_copper_golem_statue"]]), Some(&[1])),
        (1225, Some(&[&["minecraft:oxidized_copper_grate"]]), Some(&[1])),
        (1226, Some(&[&["minecraft:oxidized_copper_lantern"]]), Some(&[1])),
        (1227, Some(&[&["minecraft:oxidized_copper_trapdoor"]]), Some(&[1])),
        (1228, Some(&[&["minecraft:oxidized_cut_copper"]]), Some(&[1])),
        (1229, Some(&[&["minecraft:oxidized_cut_copper_slab"]]), Some(&[1])),
        (1230, Some(&[&["minecraft:oxidized_cut_copper_stairs"]]), Some(&[1])),
        (1231, Some(&[&["minecraft:oxidized_lightning_rod"]]), Some(&[1])),
        (1232, Some(&[&["minecraft:ink_sac", "minecraft:wither_rose"]]), Some(&[1])),
        (1233, Some(&[&["minecraft:cornflower", "minecraft:lapis_lazuli"]]), Some(&[1])),
        (1234, Some(&[&["minecraft:cocoa_beans"]]), Some(&[1])),
        (1235, Some(&[&["minecraft:closed_eyeblossom"]]), Some(&[1])),
        (1236, Some(&[&["minecraft:cactus"]]), Some(&[1])),
        (1237, Some(&[&["minecraft:pitcher_plant"]]), Some(&[1])),
        (1238, Some(&[&["minecraft:lilac"]]), Some(&[1])),
        (1239, Some(&[&["minecraft:peony"]]), Some(&[1])),
        (1240, Some(&[&["minecraft:rose_bush"]]), Some(&[1])),
        (1241, Some(&[&["minecraft:sunflower"]]), Some(&[1])),
        (1242, Some(&[&["minecraft:blue_orchid"]]), Some(&[1])),
        (1243, Some(&[&["minecraft:azure_bluet", "minecraft:oxeye_daisy", "minecraft:white_tulip"]]), Some(&[1])),
        (1244, Some(&[&["minecraft:sea_pickle"]]), Some(&[1])),
        (1245, Some(&[&["minecraft:open_eyeblossom", "minecraft:orange_tulip", "minecraft:torchflower"]]), Some(&[1])),
        (1246, Some(&[&["minecraft:allium"]]), Some(&[1])),
        (1247, Some(&[&["minecraft:cactus_flower", "minecraft:pink_petals", "minecraft:pink_tulip"]]), Some(&[1])),
        (1248, Some(&[&["minecraft:beetroot", "minecraft:poppy", "minecraft:red_tulip"]]), Some(&[1])),
        (1249, Some(&[&["minecraft:dandelion", "minecraft:golden_dandelion", "minecraft:wildflowers"]]), Some(&[1])),
        (1250, Some(&[&["minecraft:bone_meal", "minecraft:lily_of_the_valley"]]), Some(&[1])),
        (1251, Some(&[&["minecraft:charcoal", "minecraft:coal"]]), Some(&[1])),
        (1252, Some(&[&["minecraft:diamond"]]), Some(&[1])),
        (1253, Some(&[&["mekanism:dust_gold"]]), Some(&[1])),
        (1254, Some(&[&["minecraft:redstone"]]), Some(&[1])),
        (1255, Some(&[&["mekanism:dust_refined_obsidian"]]), Some(&[1])),
        (1256, Some(&[&["mekanism:dust_tin"]]), Some(&[1])),
        (1257, Some(&[&["minecraft:glowstone"]]), Some(&[1])),
        (1258, Some(&[&["mekanism:hdpe_pellet"]]), Some(&[3])),
        (1259, Some(&[&["mekanism:block_salt"]]), Some(&[1])),
        (1260, Some(&[&["minecraft:cobweb"]]), Some(&[1])),
        (1358, Some(&[&["minecraft:red_sand", "minecraft:sand"]]), None),
        (1421, Some(&[&["minecraft:deepslate_gold_ore"]]), None),
        (1422, Some(&[&["minecraft:gold_ore"]]), None),
        (1423, Some(&[&["minecraft:nether_gold_ore"]]), None),
        (1424, Some(&[&["minecraft:raw_gold"]]), None),
        (1428, Some(&[&["mekanismtools:gold_paxel"]]), None),
        (1429, Some(&[&["minecraft:golden_axe", "minecraft:golden_boots", "minecraft:golden_chestplate", "minecraft:golden_helmet", "minecraft:golden_hoe", "minecraft:golden_horse_armor", "minecraft:golden_leggings", "minecraft:golden_nautilus_armor", "minecraft:golden_pickaxe", "minecraft:golden_shovel", "minecraft:golden_spear", "minecraft:golden_sword"]]), None),
        (1465, Some(&[&["minecraft:gray_terracotta"]]), None),
        (1485, Some(&[&["minecraft:cactus"]]), None),
        (1486, Some(&[&["minecraft:green_terracotta"]]), None),
        (1506, Some(&[&["mekanism:dust_gold"]]), Some(&[4, 0])),
        (1622, Some(&[&["minecraft:deepslate_iron_ore"]]), None),
        (1623, Some(&[&["minecraft:iron_ore"]]), None),
        (1624, Some(&[&["minecraft:raw_iron"]]), None),
        (1629, Some(&[&["mekanismtools:iron_paxel"]]), None),
        (1630, Some(&[&["minecraft:chainmail_boots", "minecraft:chainmail_chestplate", "minecraft:chainmail_helmet", "minecraft:chainmail_leggings", "minecraft:iron_axe", "minecraft:iron_boots", "minecraft:iron_chestplate", "minecraft:iron_helmet", "minecraft:iron_hoe", "minecraft:iron_horse_armor", "minecraft:iron_leggings", "minecraft:iron_nautilus_armor", "minecraft:iron_pickaxe", "minecraft:iron_shovel", "minecraft:iron_spear", "minecraft:iron_sword"]]), None),
        (1679, Some(&[&["minecraft:deepslate_lapis_ore"]]), None),
        (1680, Some(&[&["minecraft:lapis_ore"]]), None),
        (1687, Some(&[&["minecraft:acacia_leaves", "minecraft:azalea_leaves", "minecraft:birch_leaves", "minecraft:cherry_leaves", "minecraft:dark_oak_leaves", "minecraft:flowering_azalea_leaves", "minecraft:jungle_leaves", "minecraft:mangrove_leaves", "minecraft:oak_leaves", "minecraft:orange_poplar_leaves", "minecraft:pale_oak_leaves", "minecraft:red_poplar_leaves", "minecraft:spruce_leaves", "minecraft:yellow_poplar_leaves"]]), None),
        (1714, Some(&[&["minecraft:light_blue_terracotta"]]), None),
        (1739, Some(&[&["minecraft:light_gray_terracotta"]]), None),
        (1762, Some(&[&["minecraft:sea_pickle"]]), None),
        (1763, Some(&[&["minecraft:lime_terracotta"]]), None),
        (1793, Some(&[&["minecraft:magenta_terracotta"]]), None),
        (1837, Some(&[&["mekanism:alloy_reinforced"]]), Some(&[1, 0])),
        (1838, Some(&[&["minecraft:copper_ingot"]]), Some(&[1, 0])),
        (1839, Some(&[&["mekanism:alloy_infused"]]), Some(&[1, 0])),
        (1840, Some(&[&["minecraft:blackstone"]]), Some(&[1, 0])),
        (1841, Some(&[&["minecraft:clay_ball"]]), Some(&[1, 0])),
        (1842, Some(&[&["minecraft:clay"]]), Some(&[1, 0])),
        (1843, Some(&[&["minecraft:crimson_nylium"]]), Some(&[1, 0])),
        (1844, Some(&[&["minecraft:dirt"]]), Some(&[1, 0])),
        (1845, Some(&[&["minecraft:dirt"]]), Some(&[1, 0])),
        (1846, Some(&[&["minecraft:cobblestone"]]), Some(&[1, 0])),
        (1847, Some(&[&["minecraft:cobblestone_slab"]]), Some(&[1, 0])),
        (1848, Some(&[&["minecraft:cobblestone_stairs"]]), Some(&[1, 0])),
        (1849, Some(&[&["minecraft:cobblestone_wall"]]), Some(&[1, 0])),
        (1850, Some(&[&["minecraft:stone_bricks"]]), Some(&[1, 0])),
        (1851, Some(&[&["minecraft:stone_brick_slab"]]), Some(&[1, 0])),
        (1852, Some(&[&["minecraft:stone_brick_stairs"]]), Some(&[1, 0])),
        (1853, Some(&[&["minecraft:stone_brick_wall"]]), Some(&[1, 0])),
        (1854, Some(&[&["minecraft:netherrack"]]), Some(&[1, 0])),
        (1855, Some(&[&["minecraft:red_sand", "minecraft:sand"]]), Some(&[1, 0])),
        (1889, Some(&[&["minecraft:netherrack"]]), None),
        (1912, Some(&[&["minecraft:ancient_debris"]]), None),
        (1981, Some(&[&["minecraft:orange_terracotta"]]), None),
        (1998, Some(&[&["mekanismtools:osmium_axe", "mekanismtools:osmium_boots", "mekanismtools:osmium_chestplate", "mekanismtools:osmium_helmet", "mekanismtools:osmium_hoe", "mekanismtools:osmium_horse_armor", "mekanismtools:osmium_leggings", "mekanismtools:osmium_nautilus_armor", "mekanismtools:osmium_paxel", "mekanismtools:osmium_pickaxe", "mekanismtools:osmium_shield", "mekanismtools:osmium_shovel", "mekanismtools:osmium_spear", "mekanismtools:osmium_sword"]]), None),
        (2881, Some(&[&["minecraft:pink_terracotta"]]), None),
        (3417, Some(&[&["mekanismadditions:aqua_plastic"]]), Some(&[1])),
        (3418, Some(&[&["mekanismadditions:black_plastic"]]), Some(&[1])),
        (3419, Some(&[&["mekanismadditions:blue_plastic"]]), Some(&[1])),
        (3420, Some(&[&["mekanismadditions:brown_plastic"]]), Some(&[1])),
        (3421, Some(&[&["mekanismadditions:cyan_plastic"]]), Some(&[1])),
        (3422, Some(&[&["mekanismadditions:dark_red_plastic"]]), Some(&[1])),
        (3423, Some(&[&["mekanismadditions:gray_plastic"]]), Some(&[1])),
        (3424, Some(&[&["mekanismadditions:green_plastic"]]), Some(&[1])),
        (3425, Some(&[&["mekanismadditions:light_blue_plastic"]]), Some(&[1])),
        (3426, Some(&[&["mekanismadditions:light_gray_plastic"]]), Some(&[1])),
        (3427, Some(&[&["mekanismadditions:lime_plastic"]]), Some(&[1])),
        (3428, Some(&[&["mekanismadditions:magenta_plastic"]]), Some(&[1])),
        (3429, Some(&[&["mekanismadditions:orange_plastic"]]), Some(&[1])),
        (3430, Some(&[&["mekanismadditions:pink_plastic"]]), Some(&[1])),
        (3431, Some(&[&["mekanismadditions:purple_plastic"]]), Some(&[1])),
        (3432, Some(&[&["mekanismadditions:red_plastic"]]), Some(&[1])),
        (3433, Some(&[&["mekanismadditions:white_plastic"]]), Some(&[1])),
        (3434, Some(&[&["mekanismadditions:yellow_plastic"]]), Some(&[1])),
        (3804, Some(&[&["minecraft:chorus_fruit"]]), None),
        (3824, Some(&[&["mekanism:dust_copper"]]), Some(&[3, 0])),
        (3825, Some(&[&["mekanism:ingot_bronze"]]), Some(&[1])),
        (3828, Some(&[&["mekanism:dust_bronze"]]), None),
        (3829, Some(&[&["minecraft:copper_ingot"]]), Some(&[3, 0])),
        (3831, Some(&[&["mekanism:dust_coal"]]), Some(&[1])),
        (3832, Some(&[&["minecraft:coal_ore", "minecraft:deepslate_coal_ore"]]), Some(&[1])),
        (3834, Some(&[&["minecraft:coal"]]), Some(&[1])),
        (3841, Some(&[&["mekanism:clump_copper"]]), Some(&[1])),
        (3842, Some(&[&["mekanism:dirty_dust_copper"]]), Some(&[1])),
        (3843, Some(&[&["minecraft:copper_ingot"]]), Some(&[1])),
        (3844, Some(&[&["minecraft:copper_ore", "minecraft:deepslate_copper_ore"]]), Some(&[1])),
        (3845, Some(&[&["minecraft:raw_copper_block"]]), Some(&[1])),
        (3846, Some(&[&["minecraft:raw_copper"]]), Some(&[3])),
        (3848, Some(&[&["mekanism:dust_copper"]]), None),
        (3859, Some(&[&["mekanism:dust_diamond"]]), Some(&[1])),
        (3860, Some(&[&["minecraft:deepslate_diamond_ore", "minecraft:diamond_ore"]]), Some(&[1])),
        (3862, Some(&[&["minecraft:diamond"]]), Some(&[1])),
        (3864, Some(&[&["mekanism:dust_emerald"]]), Some(&[1])),
        (3865, Some(&[&["minecraft:deepslate_emerald_ore", "minecraft:emerald_ore"]]), Some(&[1])),
        (3867, Some(&[&["minecraft:emerald"]]), Some(&[1])),
        (3871, Some(&[&["mekanism:deepslate_fluorite_ore"]]), None),
        (3872, Some(&[&["mekanism:dust_fluorite"]]), Some(&[1])),
        (3873, Some(&[&["mekanism:deepslate_fluorite_ore", "mekanism:fluorite_ore"]]), Some(&[1])),
        (3875, Some(&[&["mekanism:fluorite_ore"]]), None),
        (3877, Some(&[&["mekanism:fluorite_gem"]]), Some(&[1])),
        (3884, Some(&[&["mekanism:clump_gold"]]), Some(&[1])),
        (3885, Some(&[&["mekanism:dirty_dust_gold"]]), Some(&[1])),
        (3886, Some(&[&["minecraft:gold_ingot"]]), Some(&[1])),
        (3887, Some(&[&["minecraft:deepslate_gold_ore", "minecraft:gold_ore", "minecraft:nether_gold_ore"]]), Some(&[1])),
        (3888, Some(&[&["minecraft:raw_gold_block"]]), Some(&[1])),
        (3889, Some(&[&["minecraft:raw_gold"]]), Some(&[3])),
        (3891, Some(&[&["mekanism:dust_gold"]]), None),
        (3908, Some(&[&["mekanism:clump_iron"]]), Some(&[1])),
        (3909, Some(&[&["mekanism:dirty_dust_iron"]]), Some(&[1])),
        (3910, Some(&[&["minecraft:iron_ingot"]]), Some(&[1])),
        (3911, Some(&[&["minecraft:deepslate_iron_ore", "minecraft:iron_ore"]]), Some(&[1])),
        (3912, Some(&[&["minecraft:raw_iron_block"]]), Some(&[1])),
        (3913, Some(&[&["minecraft:raw_iron"]]), Some(&[3])),
        (3914, Some(&[&["minecraft:iron_ingot"]]), Some(&[1, 0])),
        (3915, Some(&[&["mekanism:dust_iron"]]), Some(&[1, 0])),
        (3917, Some(&[&["mekanism:dust_iron"]]), None),
        (3928, Some(&[&["mekanism:dust_lapis_lazuli"]]), Some(&[1])),
        (3929, Some(&[&["minecraft:deepslate_lapis_ore", "minecraft:lapis_ore"]]), Some(&[1])),
        (3931, Some(&[&["minecraft:lapis_lazuli"]]), Some(&[1])),
        (3944, Some(&[&["mekanism:clump_lead"]]), Some(&[1])),
        (3945, Some(&[&["mekanism:dirty_dust_lead"]]), Some(&[1])),
        (3946, Some(&[&["mekanism:ingot_lead"]]), Some(&[1])),
        (3947, Some(&[&["mekanism:deepslate_lead_ore", "mekanism:lead_ore"]]), Some(&[1])),
        (3948, Some(&[&["mekanism:block_raw_lead"]]), Some(&[1])),
        (3949, Some(&[&["mekanism:raw_lead"]]), Some(&[3])),
        (3952, Some(&[&["mekanism:deepslate_lead_ore"]]), None),
        (3954, Some(&[&["mekanism:dust_lead"]]), None),
        (3957, Some(&[&["mekanism:lead_ore"]]), None),
        (3959, Some(&[&["mekanism:raw_lead"]]), None),
        (3974, Some(&[&["minecraft:ancient_debris"]]), Some(&[1])),
        (3975, Some(&[&["minecraft:ancient_debris"]]), Some(&[1])),
        (3976, Some(&[&["mekanism:dirty_netherite_scrap"]]), Some(&[1])),
        (3979, Some(&[&["mekanism:dust_netherite"]]), None),
        (3980, Some(&[&["minecraft:netherite_ingot"]]), Some(&[1])),
        (3981, Some(&[&["minecraft:netherite_scrap"]]), Some(&[4, 0])),
        (3987, Some(&[&["mekanism:clump_osmium"]]), Some(&[1])),
        (3988, Some(&[&["mekanism:dirty_dust_osmium"]]), Some(&[1])),
        (3989, Some(&[&["mekanism:ingot_osmium"]]), Some(&[1])),
        (3990, Some(&[&["mekanism:deepslate_osmium_ore", "mekanism:osmium_ore"]]), Some(&[1])),
        (3991, Some(&[&["mekanism:block_raw_osmium"]]), Some(&[1])),
        (3992, Some(&[&["mekanism:raw_osmium"]]), Some(&[3])),
        (3995, Some(&[&["mekanism:deepslate_osmium_ore"]]), None),
        (3997, Some(&[&["mekanism:dust_osmium"]]), None),
        (4000, Some(&[&["mekanism:osmium_ore"]]), None),
        (4002, Some(&[&["mekanism:raw_osmium"]]), None),
        (4017, Some(&[&["mekanism:dust_quartz"]]), Some(&[1])),
        (4018, Some(&[&["minecraft:nether_quartz_ore"]]), Some(&[1])),
        (4019, Some(&[&["minecraft:quartz"]]), Some(&[1])),
        (4021, Some(&[&["minecraft:deepslate_redstone_ore", "minecraft:redstone_ore"]]), Some(&[1])),
        (4027, Some(&[&["mekanism:ingot_refined_glowstone"]]), Some(&[1])),
        (4028, Some(&[&["mekanism:ingot_refined_obsidian"]]), Some(&[1])),
        (4029, Some(&[&["mekanism:dust_obsidian"]]), Some(&[1, 0])),
        (4033, Some(&[&["mekanism:enriched_iron"]]), Some(&[1, 0])),
        (4036, Some(&[&["mekanism:dust_steel"]]), None),
        (4038, Some(&[&["mekanism:ingot_steel"]]), Some(&[1])),
        (4044, Some(&[&["mekanism:clump_tin"]]), Some(&[1])),
        (4045, Some(&[&["mekanism:dirty_dust_tin"]]), Some(&[1])),
        (4046, Some(&[&["mekanism:ingot_tin"]]), Some(&[1])),
        (4047, Some(&[&["mekanism:deepslate_tin_ore", "mekanism:tin_ore"]]), Some(&[1])),
        (4048, Some(&[&["mekanism:block_raw_tin"]]), Some(&[1])),
        (4049, Some(&[&["mekanism:raw_tin"]]), Some(&[3])),
        (4052, Some(&[&["mekanism:deepslate_tin_ore"]]), None),
        (4054, Some(&[&["mekanism:dust_tin"]]), None),
        (4057, Some(&[&["mekanism:tin_ore"]]), None),
        (4059, Some(&[&["mekanism:raw_tin"]]), None),
        (4079, Some(&[&["mekanism:clump_uranium"]]), Some(&[1])),
        (4080, Some(&[&["mekanism:dirty_dust_uranium"]]), Some(&[1])),
        (4081, Some(&[&["mekanism:ingot_uranium"]]), Some(&[1])),
        (4082, Some(&[&["mekanism:deepslate_uranium_ore", "mekanism:uranium_ore"]]), Some(&[1])),
        (4083, Some(&[&["mekanism:block_raw_uranium"]]), Some(&[1])),
        (4084, Some(&[&["mekanism:raw_uranium"]]), Some(&[3])),
        (4090, Some(&[&["mekanism:deepslate_uranium_ore"]]), None),
        (4092, Some(&[&["mekanism:dust_uranium"]]), None),
        (4095, Some(&[&["mekanism:uranium_ore"]]), None),
        (4097, Some(&[&["mekanism:raw_uranium"]]), None),
        (4116, Some(&[&["mekanism:ingot_uranium"]]), Some(&[1])),
        (4132, Some(&[&["minecraft:purple_terracotta"]]), None),
        (4158, Some(&[&["minecraft:nether_quartz_ore"]]), None),
        (4217, Some(&[&["minecraft:red_terracotta"]]), None),
        (4244, Some(&[&["minecraft:deepslate_redstone_ore"]]), None),
        (4245, Some(&[&["minecraft:redstone_ore"]]), None),
        (4255, Some(&[&["mekanismtools:refined_glowstone_axe", "mekanismtools:refined_glowstone_boots", "mekanismtools:refined_glowstone_chestplate", "mekanismtools:refined_glowstone_helmet", "mekanismtools:refined_glowstone_hoe", "mekanismtools:refined_glowstone_horse_armor", "mekanismtools:refined_glowstone_leggings", "mekanismtools:refined_glowstone_nautilus_armor", "mekanismtools:refined_glowstone_paxel", "mekanismtools:refined_glowstone_pickaxe", "mekanismtools:refined_glowstone_shield", "mekanismtools:refined_glowstone_shovel", "mekanismtools:refined_glowstone_spear", "mekanismtools:refined_glowstone_sword"]]), None),
        (4271, Some(&[&["mekanismtools:refined_obsidian_axe", "mekanismtools:refined_obsidian_boots", "mekanismtools:refined_obsidian_chestplate", "mekanismtools:refined_obsidian_helmet", "mekanismtools:refined_obsidian_hoe", "mekanismtools:refined_obsidian_horse_armor", "mekanismtools:refined_obsidian_leggings", "mekanismtools:refined_obsidian_nautilus_armor", "mekanismtools:refined_obsidian_paxel", "mekanismtools:refined_obsidian_pickaxe", "mekanismtools:refined_obsidian_shield", "mekanismtools:refined_obsidian_shovel", "mekanismtools:refined_obsidian_spear", "mekanismtools:refined_obsidian_sword"]]), None),
        (4282, Some(&[&["minecraft:resin_clump"]]), None),
        (4530, Some(&[&["minecraft:basalt"]]), None),
        (4531, Some(&[&["minecraft:quartz_block"]]), None),
        (4536, Some(&[&["minecraft:red_sandstone"]]), None),
        (4541, Some(&[&["minecraft:sandstone"]]), None),
        (4546, Some(&[&["minecraft:stone"]]), None),
        (4563, Some(&[&["minecraft:wet_sponge"]]), None),
        (4589, Some(&[&["mekanismtools:steel_axe", "mekanismtools:steel_boots", "mekanismtools:steel_chestplate", "mekanismtools:steel_helmet", "mekanismtools:steel_hoe", "mekanismtools:steel_horse_armor", "mekanismtools:steel_leggings", "mekanismtools:steel_nautilus_armor", "mekanismtools:steel_paxel", "mekanismtools:steel_pickaxe", "mekanismtools:steel_shield", "mekanismtools:steel_shovel", "mekanismtools:steel_spear", "mekanismtools:steel_sword"]]), None),
        (4602, Some(&[&["minecraft:cobblestone"]]), None),
        (4699, Some(&[&["minecraft:clay"]]), None),
        (4940, Some(&[&["minecraft:white_terracotta"]]), None),
        (4978, Some(&[&["minecraft:yellow_terracotta"]]), None),
    ];
    /// O que o fantasma de cada display desenha: `(id, itens de cada ingrediente, itens de cada saída, químicos de cada ingrediente, químicos de cada saída)`.
    pub const GHOSTS: &[machinery_basis_core::recipe_book_view::GhostRow] = &[
        (31, &[&[("minecraft:potato", 1)]], &[&[("minecraft:baked_potato", 1)]], &[&[]], &[&[]]),
        (139, &[&[("minecraft:black_terracotta", 1)]], &[&[("minecraft:black_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (170, &[&[("minecraft:blue_terracotta", 1)]], &[&[("minecraft:blue_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (194, &[&[("minecraft:clay_ball", 1)]], &[&[("minecraft:brick", 1)]], &[&[]], &[&[]]),
        (209, &[&[("mekanismtools:bronze_helmet", 1), ("mekanismtools:bronze_chestplate", 1), ("mekanismtools:bronze_leggings", 1), ("mekanismtools:bronze_boots", 1), ("mekanismtools:bronze_horse_armor", 1), ("mekanismtools:bronze_nautilus_armor", 1), ("mekanismtools:bronze_axe", 1), ("mekanismtools:bronze_hoe", 1), ("mekanismtools:bronze_paxel", 1), ("mekanismtools:bronze_pickaxe", 1), ("mekanismtools:bronze_shield", 1), ("mekanismtools:bronze_shovel", 1), ("mekanismtools:bronze_sword", 1), ("mekanismtools:bronze_spear", 1)]], &[&[("mekanism:nugget_bronze", 1)]], &[&[]], &[&[]]),
        (230, &[&[("minecraft:brown_terracotta", 1)]], &[&[("minecraft:brown_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (252, &[&[("minecraft:dark_oak_log", 1), ("minecraft:dark_oak_wood", 1), ("minecraft:stripped_dark_oak_log", 1), ("minecraft:stripped_dark_oak_wood", 1), ("minecraft:pale_oak_log", 1), ("minecraft:pale_oak_wood", 1), ("minecraft:stripped_pale_oak_log", 1), ("minecraft:stripped_pale_oak_wood", 1), ("minecraft:oak_log", 1), ("minecraft:oak_wood", 1), ("minecraft:stripped_oak_log", 1), ("minecraft:stripped_oak_wood", 1), ("minecraft:acacia_log", 1), ("minecraft:acacia_wood", 1), ("minecraft:stripped_acacia_log", 1), ("minecraft:stripped_acacia_wood", 1), ("minecraft:birch_log", 1), ("minecraft:birch_wood", 1), ("minecraft:stripped_birch_log", 1), ("minecraft:stripped_birch_wood", 1), ("minecraft:jungle_log", 1), ("minecraft:jungle_wood", 1), ("minecraft:stripped_jungle_log", 1), ("minecraft:stripped_jungle_wood", 1), ("minecraft:spruce_log", 1), ("minecraft:spruce_wood", 1), ("minecraft:stripped_spruce_log", 1), ("minecraft:stripped_spruce_wood", 1), ("minecraft:mangrove_log", 1), ("minecraft:mangrove_wood", 1), ("minecraft:stripped_mangrove_log", 1), ("minecraft:stripped_mangrove_wood", 1), ("minecraft:cherry_log", 1), ("minecraft:cherry_wood", 1), ("minecraft:stripped_cherry_log", 1), ("minecraft:stripped_cherry_wood", 1), ("minecraft:poplar_log", 1), ("minecraft:poplar_wood", 1), ("minecraft:stripped_poplar_log", 1), ("minecraft:stripped_poplar_wood", 1)]], &[&[("minecraft:charcoal", 1)]], &[&[]], &[&[]]),
        (370, &[&[("minecraft:coal_ore", 1)]], &[&[("minecraft:coal", 1)]], &[&[]], &[&[]]),
        (371, &[&[("minecraft:deepslate_coal_ore", 1)]], &[&[("minecraft:coal", 1)]], &[&[]], &[&[]]),
        (481, &[&[("mekanism:ingot_osmium", 1)], &[]], &[&[("mekanism:basic_control_circuit", 1)]], &[&[], &[("mekanism:redstone", 20)]], &[&[]]),
        (483, &[&[("mekanism:basic_control_circuit", 1)], &[]], &[&[("mekanism:advanced_control_circuit", 1)]], &[&[], &[("mekanism:redstone", 60)]], &[&[]]),
        (484, &[&[("mekanism:advanced_control_circuit", 1)], &[]], &[&[("mekanism:elite_control_circuit", 1)]], &[&[], &[("mekanism:diamond", 120)]], &[&[]]),
        (485, &[&[("mekanism:elite_control_circuit", 1)], &[]], &[&[("mekanism:ultimate_control_circuit", 1)]], &[&[], &[("mekanism:refined_obsidian", 240)]], &[&[]]),
        (487, &[&[("minecraft:beef", 1)]], &[&[("minecraft:cooked_beef", 1)]], &[&[]], &[&[]]),
        (490, &[&[("minecraft:chicken", 1)]], &[&[("minecraft:cooked_chicken", 1)]], &[&[]], &[&[]]),
        (493, &[&[("minecraft:cod", 1)]], &[&[("minecraft:cooked_cod", 1)]], &[&[]], &[&[]]),
        (496, &[&[("minecraft:mutton", 1)]], &[&[("minecraft:cooked_mutton", 1)]], &[&[]], &[&[]]),
        (499, &[&[("minecraft:porkchop", 1)]], &[&[("minecraft:cooked_porkchop", 1)]], &[&[]], &[&[]]),
        (502, &[&[("minecraft:rabbit", 1)]], &[&[("minecraft:cooked_rabbit", 1)]], &[&[]], &[&[]]),
        (505, &[&[("minecraft:salmon", 1)]], &[&[("minecraft:cooked_salmon", 1)]], &[&[]], &[&[]]),
        (527, &[&[("minecraft:copper_ore", 1)]], &[&[("minecraft:copper_ingot", 1)]], &[&[]], &[&[]]),
        (528, &[&[("minecraft:deepslate_copper_ore", 1)]], &[&[("minecraft:copper_ingot", 1)]], &[&[]], &[&[]]),
        (529, &[&[("minecraft:raw_copper", 1)]], &[&[("minecraft:copper_ingot", 1)]], &[&[]], &[&[]]),
        (536, &[&[("mekanismtools:copper_paxel", 1)]], &[&[("minecraft:copper_nugget", 1)]], &[&[]], &[&[]]),
        (537, &[&[("minecraft:copper_pickaxe", 1), ("minecraft:copper_shovel", 1), ("minecraft:copper_axe", 1), ("minecraft:copper_hoe", 1), ("minecraft:copper_sword", 1), ("minecraft:copper_spear", 1), ("minecraft:copper_helmet", 1), ("minecraft:copper_chestplate", 1), ("minecraft:copper_leggings", 1), ("minecraft:copper_boots", 1), ("minecraft:copper_horse_armor", 1), ("minecraft:copper_nautilus_armor", 1)]], &[&[("minecraft:copper_nugget", 1)]], &[&[]], &[&[]]),
        (545, &[&[("minecraft:deepslate_bricks", 1)]], &[&[("minecraft:cracked_deepslate_bricks", 1)]], &[&[]], &[&[]]),
        (546, &[&[("minecraft:deepslate_tiles", 1)]], &[&[("minecraft:cracked_deepslate_tiles", 1)]], &[&[]], &[&[]]),
        (547, &[&[("minecraft:nether_bricks", 1)]], &[&[("minecraft:cracked_nether_bricks", 1)]], &[&[]], &[&[]]),
        (548, &[&[("minecraft:polished_blackstone_bricks", 1)]], &[&[("minecraft:cracked_polished_blackstone_bricks", 1)]], &[&[]], &[&[]]),
        (549, &[&[("minecraft:stone_bricks", 1)]], &[&[("minecraft:cracked_stone_bricks", 1)]], &[&[]], &[&[]]),
        (570, &[&[("minecraft:polished_andesite", 1)]], &[&[("minecraft:andesite", 1)]], &[&[]], &[&[]]),
        (571, &[&[("minecraft:polished_andesite_slab", 1)]], &[&[("minecraft:andesite_slab", 1)]], &[&[]], &[&[]]),
        (572, &[&[("minecraft:polished_andesite_stairs", 1)]], &[&[("minecraft:andesite_stairs", 1)]], &[&[]], &[&[]]),
        (573, &[&[("minecraft:apple", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (574, &[&[("minecraft:baked_potato", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (575, &[&[("minecraft:bamboo", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (576, &[&[("minecraft:sweet_berries", 1), ("minecraft:glow_berries", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (577, &[&[("minecraft:big_dripleaf", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (578, &[&[("minecraft:bread", 1)]], &[&[("mekanism:bio_fuel", 4)]], &[&[]], &[&[]]),
        (579, &[&[("minecraft:bush", 1), ("minecraft:red_shrub", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (580, &[&[("minecraft:cactus_flower", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (581, &[&[("minecraft:cake", 1)]], &[&[("mekanism:bio_fuel", 6)]], &[&[]], &[&[]]),
        (582, &[&[("minecraft:carved_pumpkin", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (583, &[&[("minecraft:cookie", 4)]], &[&[("mekanism:bio_fuel", 3)]], &[&[]], &[&[]]),
        (584, &[&[("minecraft:crimson_roots", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (585, &[&[("minecraft:poisonous_potato", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (586, &[&[("minecraft:potato", 1), ("minecraft:nether_wart", 1), ("minecraft:carrot", 1), ("minecraft:cactus", 1), ("minecraft:beetroot", 1), ("minecraft:cocoa_beans", 1), ("minecraft:wheat", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (587, &[&[("minecraft:dried_kelp", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (588, &[&[("minecraft:dried_kelp_block", 1)]], &[&[("mekanism:block_bio_fuel", 1)]], &[&[]], &[&[]]),
        (589, &[&[("minecraft:fern", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (590, &[&[("minecraft:firefly_bush", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (591, &[&[("minecraft:crimson_fungus", 1), ("minecraft:warped_fungus", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (592, &[&[("minecraft:glow_lichen", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (593, &[&[("minecraft:hanging_roots", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (594, &[&[("minecraft:hay_block", 1)]], &[&[("mekanism:block_bio_fuel", 2)]], &[&[]], &[&[]]),
        (595, &[&[("minecraft:kelp", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (596, &[&[("minecraft:large_fern", 1)]], &[&[("mekanism:bio_fuel", 3)]], &[&[]], &[&[]]),
        (597, &[&[("minecraft:leaf_litter", 10)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (598, &[&[("minecraft:jungle_leaves", 10), ("minecraft:oak_leaves", 10), ("minecraft:spruce_leaves", 10), ("minecraft:pale_oak_leaves", 10), ("minecraft:dark_oak_leaves", 10), ("minecraft:acacia_leaves", 10), ("minecraft:birch_leaves", 10), ("minecraft:azalea_leaves", 10), ("minecraft:flowering_azalea_leaves", 10), ("minecraft:mangrove_leaves", 10), ("minecraft:cherry_leaves", 10), ("minecraft:red_poplar_leaves", 10), ("minecraft:orange_poplar_leaves", 10), ("minecraft:yellow_poplar_leaves", 10)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (599, &[&[("minecraft:lily_pad", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (600, &[&[("minecraft:mangrove_roots", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (601, &[&[("minecraft:melon", 1)]], &[&[("mekanism:bio_fuel", 6)]], &[&[]], &[&[]]),
        (602, &[&[("minecraft:melon_slice", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (603, &[&[("minecraft:moss_block", 1), ("minecraft:pale_moss_block", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (604, &[&[("minecraft:moss_carpet", 3), ("minecraft:pale_moss_carpet", 3)]], &[&[("mekanism:bio_fuel", 4)]], &[&[]], &[&[]]),
        (605, &[&[("minecraft:brown_mushroom_block", 1), ("minecraft:red_mushroom_block", 1), ("minecraft:mushroom_stem", 1)]], &[&[("mekanism:bio_fuel", 4)]], &[&[]], &[&[]]),
        (606, &[&[("minecraft:brown_mushroom", 1), ("minecraft:red_mushroom", 1), ("minecraft:shelf_mushroom", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (607, &[&[("minecraft:nether_sprouts", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (608, &[&[("minecraft:nether_wart_block", 1)]], &[&[("mekanism:block_bio_fuel", 2)]], &[&[]], &[&[]]),
        (609, &[&[("minecraft:pale_hanging_moss", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (610, &[&[("minecraft:pink_petals", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (611, &[&[("minecraft:pitcher_pod", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (612, &[&[("minecraft:pumpkin", 1)]], &[&[("mekanism:bio_fuel", 6)]], &[&[]], &[&[]]),
        (613, &[&[("minecraft:pumpkin_pie", 1)]], &[&[("mekanism:bio_fuel", 7)]], &[&[]], &[&[]]),
        (614, &[&[("minecraft:rotten_flesh", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (615, &[&[("minecraft:oak_sapling", 1), ("minecraft:spruce_sapling", 1), ("minecraft:birch_sapling", 1), ("minecraft:jungle_sapling", 1), ("minecraft:acacia_sapling", 1), ("minecraft:dark_oak_sapling", 1), ("minecraft:pale_oak_sapling", 1), ("minecraft:azalea", 1), ("minecraft:flowering_azalea", 1), ("minecraft:mangrove_propagule", 1), ("minecraft:cherry_sapling", 1), ("minecraft:poplar_sapling", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (616, &[&[("minecraft:sea_pickle", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (617, &[&[("minecraft:seagrass", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (618, &[&[("minecraft:beetroot_seeds", 1), ("minecraft:melon_seeds", 1), ("minecraft:pumpkin_seeds", 1), ("minecraft:torchflower_seeds", 1), ("minecraft:pitcher_pod", 1), ("minecraft:wheat_seeds", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (619, &[&[("minecraft:short_dry_grass", 1), ("minecraft:short_grass", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (620, &[&[("minecraft:shroomlight", 1)]], &[&[("mekanism:bio_fuel", 4)]], &[&[]], &[&[]]),
        (621, &[&[("minecraft:small_dripleaf", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (622, &[&[("minecraft:dandelion", 1), ("minecraft:poppy", 1), ("minecraft:blue_orchid", 1), ("minecraft:allium", 1), ("minecraft:azure_bluet", 1), ("minecraft:red_tulip", 1), ("minecraft:orange_tulip", 1), ("minecraft:white_tulip", 1), ("minecraft:pink_tulip", 1), ("minecraft:oxeye_daisy", 1), ("minecraft:cornflower", 1), ("minecraft:lily_of_the_valley", 1), ("minecraft:wither_rose", 1), ("minecraft:torchflower", 1), ("minecraft:open_eyeblossom", 1), ("minecraft:closed_eyeblossom", 1), ("minecraft:golden_dandelion", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (623, &[&[("minecraft:spore_blossom", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (624, &[&[("minecraft:straw_bed", 2)]], &[&[("mekanism:block_bio_fuel", 3)]], &[&[]], &[&[]]),
        (625, &[&[("minecraft:sugar_cane", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (626, &[&[("minecraft:sunflower", 1), ("minecraft:lilac", 1), ("minecraft:peony", 1), ("minecraft:rose_bush", 1), ("minecraft:pitcher_plant", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (627, &[&[("minecraft:tall_dry_grass", 1), ("minecraft:tall_grass", 1)]], &[&[("mekanism:bio_fuel", 2)]], &[&[]], &[&[]]),
        (628, &[&[("minecraft:vine", 1), ("minecraft:twisting_vines", 1), ("minecraft:weeping_vines", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (629, &[&[("minecraft:warped_roots", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (630, &[&[("minecraft:warped_wart_block", 1)]], &[&[("mekanism:bio_fuel", 4)]], &[&[]], &[&[]]),
        (631, &[&[("minecraft:wildflowers", 1)]], &[&[("mekanism:bio_fuel", 1)]], &[&[]], &[&[]]),
        (632, &[&[("minecraft:polished_blackstone_bricks", 1)]], &[&[("minecraft:cracked_polished_blackstone_bricks", 1)]], &[&[]], &[&[]]),
        (633, &[&[("minecraft:chiseled_polished_blackstone", 1)]], &[&[("minecraft:polished_blackstone_bricks", 1)]], &[&[]], &[&[]]),
        (634, &[&[("minecraft:cracked_polished_blackstone_bricks", 1)]], &[&[("minecraft:polished_blackstone", 1)]], &[&[]], &[&[]]),
        (635, &[&[("minecraft:polished_blackstone", 1)]], &[&[("minecraft:blackstone", 1)]], &[&[]], &[&[]]),
        (636, &[&[("minecraft:polished_blackstone_slab", 1)]], &[&[("minecraft:blackstone_slab", 1)]], &[&[]], &[&[]]),
        (637, &[&[("minecraft:polished_blackstone_stairs", 1)]], &[&[("minecraft:blackstone_stairs", 1)]], &[&[]], &[&[]]),
        (638, &[&[("minecraft:polished_blackstone_wall", 1)]], &[&[("minecraft:blackstone_wall", 1)]], &[&[]], &[&[]]),
        (639, &[&[("minecraft:blaze_rod", 1)]], &[&[("minecraft:blaze_powder", 4)]], &[&[]], &[&[]]),
        (640, &[&[("minecraft:bone", 1)]], &[&[("minecraft:bone_meal", 6)]], &[&[]], &[&[]]),
        (641, &[&[("minecraft:bone_block", 1)]], &[&[("minecraft:bone_meal", 9)]], &[&[]], &[&[]]),
        (642, &[&[("minecraft:music_disc_5", 1)]], &[&[("minecraft:disc_fragment_5", 9)]], &[&[]], &[&[]]),
        (643, &[&[("minecraft:breeze_rod", 1)]], &[&[("minecraft:wind_charge", 6)]], &[&[]], &[&[]]),
        (644, &[&[("minecraft:charcoal", 1)]], &[&[("mekanism:dust_charcoal", 1)]], &[&[]], &[&[]]),
        (645, &[&[("minecraft:chiseled_nether_bricks", 1)]], &[&[("minecraft:nether_bricks", 1)]], &[&[]], &[&[]]),
        (646, &[&[("minecraft:cinnabar_brick_slab", 1)]], &[&[("minecraft:polished_cinnabar_slab", 1)]], &[&[]], &[&[]]),
        (647, &[&[("minecraft:cinnabar_brick_stairs", 1)]], &[&[("minecraft:polished_cinnabar_stairs", 1)]], &[&[]], &[&[]]),
        (648, &[&[("minecraft:cinnabar_brick_wall", 1)]], &[&[("minecraft:polished_cinnabar_wall", 1)]], &[&[]], &[&[]]),
        (649, &[&[("minecraft:cinnabar_bricks", 1)]], &[&[("minecraft:polished_cinnabar", 1)]], &[&[]], &[&[]]),
        (650, &[&[("minecraft:chiseled_cinnabar", 1)]], &[&[("minecraft:cinnabar_bricks", 1)]], &[&[]], &[&[]]),
        (651, &[&[("minecraft:polished_cinnabar", 1)]], &[&[("minecraft:cinnabar", 1)]], &[&[]], &[&[]]),
        (652, &[&[("minecraft:polished_cinnabar_slab", 1)]], &[&[("minecraft:cinnabar_slab", 1)]], &[&[]], &[&[]]),
        (653, &[&[("minecraft:polished_cinnabar_stairs", 1)]], &[&[("minecraft:cinnabar_stairs", 1)]], &[&[]], &[&[]]),
        (654, &[&[("minecraft:polished_cinnabar_wall", 1)]], &[&[("minecraft:cinnabar_wall", 1)]], &[&[]], &[&[]]),
        (655, &[&[("minecraft:cobblestone", 1)]], &[&[("minecraft:gravel", 1)]], &[&[]], &[&[]]),
        (656, &[&[("minecraft:deepslate_brick_slab", 1)]], &[&[("minecraft:deepslate_tile_slab", 1)]], &[&[]], &[&[]]),
        (657, &[&[("minecraft:deepslate_brick_stairs", 1)]], &[&[("minecraft:deepslate_tile_stairs", 1)]], &[&[]], &[&[]]),
        (658, &[&[("minecraft:deepslate_brick_wall", 1)]], &[&[("minecraft:deepslate_tile_wall", 1)]], &[&[]], &[&[]]),
        (659, &[&[("minecraft:deepslate_bricks", 1)]], &[&[("minecraft:cracked_deepslate_bricks", 1)]], &[&[]], &[&[]]),
        (660, &[&[("minecraft:cracked_deepslate_bricks", 1)]], &[&[("minecraft:deepslate_tiles", 1)]], &[&[]], &[&[]]),
        (661, &[&[("minecraft:cracked_deepslate_tiles", 1)]], &[&[("minecraft:chiseled_deepslate", 1)]], &[&[]], &[&[]]),
        (662, &[&[("minecraft:chiseled_deepslate", 1)]], &[&[("minecraft:deepslate", 1)]], &[&[]], &[&[]]),
        (663, &[&[("minecraft:polished_deepslate_slab", 1)]], &[&[("minecraft:deepslate_brick_slab", 1)]], &[&[]], &[&[]]),
        (664, &[&[("minecraft:polished_deepslate_stairs", 1)]], &[&[("minecraft:deepslate_brick_stairs", 1)]], &[&[]], &[&[]]),
        (665, &[&[("minecraft:polished_deepslate", 1)]], &[&[("minecraft:deepslate_bricks", 1)]], &[&[]], &[&[]]),
        (666, &[&[("minecraft:polished_deepslate_wall", 1)]], &[&[("minecraft:deepslate_brick_wall", 1)]], &[&[]], &[&[]]),
        (667, &[&[("minecraft:deepslate_tile_slab", 1)]], &[&[("minecraft:cobbled_deepslate_slab", 1)]], &[&[]], &[&[]]),
        (668, &[&[("minecraft:deepslate_tile_stairs", 1)]], &[&[("minecraft:cobbled_deepslate_stairs", 1)]], &[&[]], &[&[]]),
        (669, &[&[("minecraft:deepslate_tiles", 1)]], &[&[("minecraft:cracked_deepslate_tiles", 1)]], &[&[]], &[&[]]),
        (670, &[&[("minecraft:deepslate_tile_wall", 1)]], &[&[("minecraft:cobbled_deepslate_wall", 1)]], &[&[]], &[&[]]),
        (671, &[&[("minecraft:deepslate", 1)]], &[&[("minecraft:cobbled_deepslate", 1)]], &[&[]], &[&[]]),
        (672, &[&[("minecraft:waxed_chiseled_copper", 1)]], &[&[("minecraft:chiseled_copper", 1)]], &[&[]], &[&[]]),
        (673, &[&[("minecraft:waxed_copper_bars", 1)]], &[&[("minecraft:copper_bars", 1)]], &[&[]], &[&[]]),
        (674, &[&[("minecraft:waxed_copper_block", 1)]], &[&[("minecraft:copper_block", 1)]], &[&[]], &[&[]]),
        (675, &[&[("minecraft:waxed_copper_bulb", 1)]], &[&[("minecraft:copper_bulb", 1)]], &[&[]], &[&[]]),
        (676, &[&[("minecraft:waxed_copper_chain", 1)]], &[&[("minecraft:copper_chain", 1)]], &[&[]], &[&[]]),
        (677, &[&[("minecraft:waxed_copper_chest", 1)]], &[&[("minecraft:copper_chest", 1)]], &[&[]], &[&[]]),
        (678, &[&[("minecraft:waxed_copper_door", 1)]], &[&[("minecraft:copper_door", 1)]], &[&[]], &[&[]]),
        (679, &[&[("minecraft:waxed_copper_golem_statue", 1)]], &[&[("minecraft:copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (680, &[&[("minecraft:waxed_copper_grate", 1)]], &[&[("minecraft:copper_grate", 1)]], &[&[]], &[&[]]),
        (681, &[&[("minecraft:waxed_copper_lantern", 1)]], &[&[("minecraft:copper_lantern", 1)]], &[&[]], &[&[]]),
        (682, &[&[("minecraft:waxed_copper_trapdoor", 1)]], &[&[("minecraft:copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (683, &[&[("minecraft:waxed_cut_copper", 1)]], &[&[("minecraft:cut_copper", 1)]], &[&[]], &[&[]]),
        (684, &[&[("minecraft:waxed_cut_copper_slab", 1)]], &[&[("minecraft:cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (685, &[&[("minecraft:waxed_cut_copper_stairs", 1)]], &[&[("minecraft:cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (686, &[&[("minecraft:waxed_exposed_chiseled_copper", 1)]], &[&[("minecraft:exposed_chiseled_copper", 1)]], &[&[]], &[&[]]),
        (687, &[&[("minecraft:waxed_exposed_copper", 1)]], &[&[("minecraft:exposed_copper", 1)]], &[&[]], &[&[]]),
        (688, &[&[("minecraft:waxed_exposed_copper_bars", 1)]], &[&[("minecraft:exposed_copper_bars", 1)]], &[&[]], &[&[]]),
        (689, &[&[("minecraft:waxed_exposed_copper_bulb", 1)]], &[&[("minecraft:exposed_copper_bulb", 1)]], &[&[]], &[&[]]),
        (690, &[&[("minecraft:waxed_exposed_copper_chain", 1)]], &[&[("minecraft:exposed_copper_chain", 1)]], &[&[]], &[&[]]),
        (691, &[&[("minecraft:waxed_exposed_copper_chest", 1)]], &[&[("minecraft:exposed_copper_chest", 1)]], &[&[]], &[&[]]),
        (692, &[&[("minecraft:waxed_exposed_copper_door", 1)]], &[&[("minecraft:exposed_copper_door", 1)]], &[&[]], &[&[]]),
        (693, &[&[("minecraft:waxed_exposed_copper_golem_statue", 1)]], &[&[("minecraft:exposed_copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (694, &[&[("minecraft:waxed_exposed_copper_grate", 1)]], &[&[("minecraft:exposed_copper_grate", 1)]], &[&[]], &[&[]]),
        (695, &[&[("minecraft:waxed_exposed_copper_lantern", 1)]], &[&[("minecraft:exposed_copper_lantern", 1)]], &[&[]], &[&[]]),
        (696, &[&[("minecraft:waxed_exposed_copper_trapdoor", 1)]], &[&[("minecraft:exposed_copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (697, &[&[("minecraft:waxed_exposed_cut_copper", 1)]], &[&[("minecraft:exposed_cut_copper", 1)]], &[&[]], &[&[]]),
        (698, &[&[("minecraft:waxed_exposed_cut_copper_slab", 1)]], &[&[("minecraft:exposed_cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (699, &[&[("minecraft:waxed_exposed_cut_copper_stairs", 1)]], &[&[("minecraft:exposed_cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (700, &[&[("minecraft:waxed_exposed_lightning_rod", 1)]], &[&[("minecraft:exposed_lightning_rod", 1)]], &[&[]], &[&[]]),
        (701, &[&[("minecraft:waxed_lightning_rod", 1)]], &[&[("minecraft:lightning_rod", 1)]], &[&[]], &[&[]]),
        (702, &[&[("minecraft:waxed_oxidized_chiseled_copper", 1)]], &[&[("minecraft:oxidized_chiseled_copper", 1)]], &[&[]], &[&[]]),
        (703, &[&[("minecraft:waxed_oxidized_copper", 1)]], &[&[("minecraft:oxidized_copper", 1)]], &[&[]], &[&[]]),
        (704, &[&[("minecraft:waxed_oxidized_copper_bars", 1)]], &[&[("minecraft:oxidized_copper_bars", 1)]], &[&[]], &[&[]]),
        (705, &[&[("minecraft:waxed_oxidized_copper_bulb", 1)]], &[&[("minecraft:oxidized_copper_bulb", 1)]], &[&[]], &[&[]]),
        (706, &[&[("minecraft:waxed_oxidized_copper_chain", 1)]], &[&[("minecraft:oxidized_copper_chain", 1)]], &[&[]], &[&[]]),
        (707, &[&[("minecraft:waxed_oxidized_copper_chest", 1)]], &[&[("minecraft:oxidized_copper_chest", 1)]], &[&[]], &[&[]]),
        (708, &[&[("minecraft:waxed_oxidized_copper_door", 1)]], &[&[("minecraft:oxidized_copper_door", 1)]], &[&[]], &[&[]]),
        (709, &[&[("minecraft:waxed_oxidized_copper_golem_statue", 1)]], &[&[("minecraft:oxidized_copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (710, &[&[("minecraft:waxed_oxidized_copper_grate", 1)]], &[&[("minecraft:oxidized_copper_grate", 1)]], &[&[]], &[&[]]),
        (711, &[&[("minecraft:waxed_oxidized_copper_lantern", 1)]], &[&[("minecraft:oxidized_copper_lantern", 1)]], &[&[]], &[&[]]),
        (712, &[&[("minecraft:waxed_oxidized_copper_trapdoor", 1)]], &[&[("minecraft:oxidized_copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (713, &[&[("minecraft:waxed_oxidized_cut_copper", 1)]], &[&[("minecraft:oxidized_cut_copper", 1)]], &[&[]], &[&[]]),
        (714, &[&[("minecraft:waxed_oxidized_cut_copper_slab", 1)]], &[&[("minecraft:oxidized_cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (715, &[&[("minecraft:waxed_oxidized_cut_copper_stairs", 1)]], &[&[("minecraft:oxidized_cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (716, &[&[("minecraft:waxed_oxidized_lightning_rod", 1)]], &[&[("minecraft:oxidized_lightning_rod", 1)]], &[&[]], &[&[]]),
        (717, &[&[("minecraft:waxed_weathered_chiseled_copper", 1)]], &[&[("minecraft:weathered_chiseled_copper", 1)]], &[&[]], &[&[]]),
        (718, &[&[("minecraft:waxed_weathered_copper", 1)]], &[&[("minecraft:weathered_copper", 1)]], &[&[]], &[&[]]),
        (719, &[&[("minecraft:waxed_weathered_copper_bars", 1)]], &[&[("minecraft:weathered_copper_bars", 1)]], &[&[]], &[&[]]),
        (720, &[&[("minecraft:waxed_weathered_copper_bulb", 1)]], &[&[("minecraft:weathered_copper_bulb", 1)]], &[&[]], &[&[]]),
        (721, &[&[("minecraft:waxed_weathered_copper_chain", 1)]], &[&[("minecraft:weathered_copper_chain", 1)]], &[&[]], &[&[]]),
        (722, &[&[("minecraft:waxed_weathered_copper_chest", 1)]], &[&[("minecraft:weathered_copper_chest", 1)]], &[&[]], &[&[]]),
        (723, &[&[("minecraft:waxed_weathered_copper_door", 1)]], &[&[("minecraft:weathered_copper_door", 1)]], &[&[]], &[&[]]),
        (724, &[&[("minecraft:waxed_weathered_copper_golem_statue", 1)]], &[&[("minecraft:weathered_copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (725, &[&[("minecraft:waxed_weathered_copper_grate", 1)]], &[&[("minecraft:weathered_copper_grate", 1)]], &[&[]], &[&[]]),
        (726, &[&[("minecraft:waxed_weathered_copper_lantern", 1)]], &[&[("minecraft:weathered_copper_lantern", 1)]], &[&[]], &[&[]]),
        (727, &[&[("minecraft:waxed_weathered_copper_trapdoor", 1)]], &[&[("minecraft:weathered_copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (728, &[&[("minecraft:waxed_weathered_cut_copper", 1)]], &[&[("minecraft:weathered_cut_copper", 1)]], &[&[]], &[&[]]),
        (729, &[&[("minecraft:waxed_weathered_cut_copper_slab", 1)]], &[&[("minecraft:weathered_cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (730, &[&[("minecraft:waxed_weathered_cut_copper_stairs", 1)]], &[&[("minecraft:weathered_cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (731, &[&[("minecraft:waxed_weathered_lightning_rod", 1)]], &[&[("minecraft:weathered_lightning_rod", 1)]], &[&[]], &[&[]]),
        (732, &[&[("minecraft:polished_diorite", 1)]], &[&[("minecraft:diorite", 1)]], &[&[]], &[&[]]),
        (733, &[&[("minecraft:polished_diorite_slab", 1)]], &[&[("minecraft:diorite_slab", 1)]], &[&[]], &[&[]]),
        (734, &[&[("minecraft:polished_diorite_stairs", 1)]], &[&[("minecraft:diorite_stairs", 1)]], &[&[]], &[&[]]),
        (735, &[&[("minecraft:dried_ghast", 1)]], &[&[("minecraft:ghast_tear", 3)]], &[&[]], &[&[]]),
        (736, &[&[("minecraft:flint", 1)]], &[&[("minecraft:gunpowder", 1)]], &[&[]], &[&[]]),
        (737, &[&[("minecraft:polished_granite", 1)]], &[&[("minecraft:granite", 1)]], &[&[]], &[&[]]),
        (738, &[&[("minecraft:polished_granite_slab", 1)]], &[&[("minecraft:granite_slab", 1)]], &[&[]], &[&[]]),
        (739, &[&[("minecraft:polished_granite_stairs", 1)]], &[&[("minecraft:granite_stairs", 1)]], &[&[]], &[&[]]),
        (740, &[&[("minecraft:gravel", 1)]], &[&[("minecraft:sand", 1)]], &[&[]], &[&[]]),
        (741, &[&[("minecraft:honeycomb_block", 1)]], &[&[("minecraft:honeycomb", 4)]], &[&[]], &[&[]]),
        (742, &[&[("minecraft:mud_bricks", 1)]], &[&[("minecraft:packed_mud", 1)]], &[&[]], &[&[]]),
        (743, &[&[("minecraft:nether_bricks", 1)]], &[&[("minecraft:cracked_nether_bricks", 1)]], &[&[]], &[&[]]),
        (744, &[&[("minecraft:obsidian", 1)]], &[&[("mekanism:dust_obsidian", 4)]], &[&[]], &[&[]]),
        (745, &[&[("minecraft:dripstone_block", 1)]], &[&[("minecraft:pointed_dripstone", 4)]], &[&[]], &[&[]]),
        (746, &[&[("minecraft:polished_basalt", 1), ("minecraft:smooth_basalt", 1)]], &[&[("minecraft:basalt", 1)]], &[&[]], &[&[]]),
        (747, &[&[("minecraft:prismarine", 1)]], &[&[("minecraft:prismarine_shard", 4)]], &[&[]], &[&[]]),
        (748, &[&[("minecraft:prismarine_bricks", 1)]], &[&[("minecraft:prismarine_shard", 9)]], &[&[]], &[&[]]),
        (749, &[&[("minecraft:prismarine_brick_slab", 2)]], &[&[("minecraft:prismarine_shard", 9)]], &[&[]], &[&[]]),
        (750, &[&[("minecraft:prismarine_brick_stairs", 1)]], &[&[("minecraft:prismarine_shard", 9)]], &[&[]], &[&[]]),
        (751, &[&[("minecraft:prismarine_slab", 1)]], &[&[("minecraft:prismarine_shard", 2)]], &[&[]], &[&[]]),
        (752, &[&[("minecraft:prismarine_stairs", 1)]], &[&[("minecraft:prismarine_shard", 4)]], &[&[]], &[&[]]),
        (753, &[&[("minecraft:prismarine_wall", 1)]], &[&[("minecraft:prismarine_shard", 4)]], &[&[]], &[&[]]),
        (754, &[&[("minecraft:purpur_pillar", 1)]], &[&[("minecraft:purpur_block", 1)]], &[&[]], &[&[]]),
        (755, &[&[("minecraft:quartz_bricks", 1)]], &[&[("minecraft:chiseled_quartz_block", 1)]], &[&[]], &[&[]]),
        (756, &[&[("minecraft:chiseled_quartz_block", 1)]], &[&[("minecraft:quartz_pillar", 1)]], &[&[]], &[&[]]),
        (757, &[&[("minecraft:quartz_pillar", 1)]], &[&[("minecraft:quartz_block", 1)]], &[&[]], &[&[]]),
        (758, &[&[("minecraft:quartz_slab", 1)]], &[&[("minecraft:smooth_quartz_slab", 1)]], &[&[]], &[&[]]),
        (759, &[&[("minecraft:smooth_quartz", 1)]], &[&[("minecraft:quartz_bricks", 1)]], &[&[]], &[&[]]),
        (760, &[&[("minecraft:quartz_stairs", 1)]], &[&[("minecraft:smooth_quartz_stairs", 1)]], &[&[]], &[&[]]),
        (761, &[&[("minecraft:quartz_block", 1)]], &[&[("minecraft:smooth_quartz", 1)]], &[&[]], &[&[]]),
        (762, &[&[("minecraft:red_sandstone", 1), ("minecraft:cut_red_sandstone", 1), ("minecraft:chiseled_red_sandstone", 1), ("minecraft:smooth_red_sandstone", 1)]], &[&[("minecraft:red_sand", 2)]], &[&[]], &[&[]]),
        (763, &[&[("minecraft:chiseled_resin_bricks", 1)]], &[&[("minecraft:resin_bricks", 1)]], &[&[]], &[&[]]),
        (764, &[&[("minecraft:resin_block", 1)]], &[&[("minecraft:resin_clump", 9)]], &[&[]], &[&[]]),
        (765, &[&[("minecraft:resin_bricks", 1)]], &[&[("minecraft:resin_block", 1)]], &[&[]], &[&[]]),
        (766, &[&[("minecraft:sandstone", 1), ("minecraft:cut_sandstone", 1), ("minecraft:chiseled_sandstone", 1), ("minecraft:smooth_sandstone", 1)]], &[&[("minecraft:sand", 2)]], &[&[]], &[&[]]),
        (767, &[&[("minecraft:soul_soil", 1)]], &[&[("minecraft:soul_sand", 1)]], &[&[]], &[&[]]),
        (768, &[&[("minecraft:stone_bricks", 1)]], &[&[("minecraft:cracked_stone_bricks", 1)]], &[&[]], &[&[]]),
        (769, &[&[("minecraft:chiseled_stone_bricks", 1)]], &[&[("minecraft:stone_bricks", 1)]], &[&[]], &[&[]]),
        (770, &[&[("minecraft:cracked_stone_bricks", 1)]], &[&[("minecraft:stone", 1)]], &[&[]], &[&[]]),
        (771, &[&[("minecraft:stone_slab", 1)]], &[&[("minecraft:cobblestone_slab", 1)]], &[&[]], &[&[]]),
        (772, &[&[("minecraft:stone_stairs", 1)]], &[&[("minecraft:cobblestone_stairs", 1)]], &[&[]], &[&[]]),
        (773, &[&[("minecraft:stone", 1)]], &[&[("minecraft:cobblestone", 1)]], &[&[]], &[&[]]),
        (774, &[&[("minecraft:white_cushion", 1), ("minecraft:orange_cushion", 1), ("minecraft:magenta_cushion", 1), ("minecraft:light_blue_cushion", 1), ("minecraft:yellow_cushion", 1), ("minecraft:lime_cushion", 1), ("minecraft:pink_cushion", 1), ("minecraft:gray_cushion", 1), ("minecraft:light_gray_cushion", 1), ("minecraft:cyan_cushion", 1), ("minecraft:purple_cushion", 1), ("minecraft:blue_cushion", 1), ("minecraft:brown_cushion", 1), ("minecraft:green_cushion", 1), ("minecraft:red_cushion", 1), ("minecraft:black_cushion", 1)]], &[&[("minecraft:string", 6)]], &[&[]], &[&[]]),
        (775, &[&[("minecraft:white_wool", 1), ("minecraft:orange_wool", 1), ("minecraft:magenta_wool", 1), ("minecraft:light_blue_wool", 1), ("minecraft:yellow_wool", 1), ("minecraft:lime_wool", 1), ("minecraft:pink_wool", 1), ("minecraft:gray_wool", 1), ("minecraft:light_gray_wool", 1), ("minecraft:cyan_wool", 1), ("minecraft:purple_wool", 1), ("minecraft:blue_wool", 1), ("minecraft:brown_wool", 1), ("minecraft:green_wool", 1), ("minecraft:red_wool", 1), ("minecraft:black_wool", 1)]], &[&[("minecraft:string", 4)]], &[&[]], &[&[]]),
        (776, &[&[("minecraft:white_wool_slab", 1), ("minecraft:orange_wool_slab", 1), ("minecraft:magenta_wool_slab", 1), ("minecraft:light_blue_wool_slab", 1), ("minecraft:yellow_wool_slab", 1), ("minecraft:lime_wool_slab", 1), ("minecraft:pink_wool_slab", 1), ("minecraft:gray_wool_slab", 1), ("minecraft:light_gray_wool_slab", 1), ("minecraft:cyan_wool_slab", 1), ("minecraft:purple_wool_slab", 1), ("minecraft:blue_wool_slab", 1), ("minecraft:brown_wool_slab", 1), ("minecraft:green_wool_slab", 1), ("minecraft:red_wool_slab", 1), ("minecraft:black_wool_slab", 1)]], &[&[("minecraft:string", 2)]], &[&[]], &[&[]]),
        (777, &[&[("minecraft:white_wool_stairs", 1), ("minecraft:orange_wool_stairs", 1), ("minecraft:magenta_wool_stairs", 1), ("minecraft:light_blue_wool_stairs", 1), ("minecraft:yellow_wool_stairs", 1), ("minecraft:lime_wool_stairs", 1), ("minecraft:pink_wool_stairs", 1), ("minecraft:gray_wool_stairs", 1), ("minecraft:light_gray_wool_stairs", 1), ("minecraft:cyan_wool_stairs", 1), ("minecraft:purple_wool_stairs", 1), ("minecraft:blue_wool_stairs", 1), ("minecraft:brown_wool_stairs", 1), ("minecraft:green_wool_stairs", 1), ("minecraft:red_wool_stairs", 1), ("minecraft:black_wool_stairs", 1)]], &[&[("minecraft:string", 6)]], &[&[]], &[&[]]),
        (778, &[&[("minecraft:sulfur_brick_slab", 1)]], &[&[("minecraft:polished_sulfur_slab", 1)]], &[&[]], &[&[]]),
        (779, &[&[("minecraft:sulfur_brick_stairs", 1)]], &[&[("minecraft:polished_sulfur_stairs", 1)]], &[&[]], &[&[]]),
        (780, &[&[("minecraft:sulfur_brick_wall", 1)]], &[&[("minecraft:polished_sulfur_wall", 1)]], &[&[]], &[&[]]),
        (781, &[&[("minecraft:sulfur_bricks", 1)]], &[&[("minecraft:polished_sulfur", 1)]], &[&[]], &[&[]]),
        (782, &[&[("minecraft:chiseled_sulfur", 1)]], &[&[("minecraft:sulfur_bricks", 1)]], &[&[]], &[&[]]),
        (783, &[&[("minecraft:polished_sulfur", 1)]], &[&[("minecraft:sulfur", 1)]], &[&[]], &[&[]]),
        (784, &[&[("minecraft:potent_sulfur", 1)]], &[&[("mekanism:dust_sulfur", 36)]], &[&[]], &[&[]]),
        (785, &[&[("minecraft:polished_sulfur_slab", 1)]], &[&[("minecraft:sulfur_slab", 1)]], &[&[]], &[&[]]),
        (786, &[&[("minecraft:sulfur_spike", 1)]], &[&[("mekanism:dust_sulfur", 1)]], &[&[]], &[&[]]),
        (787, &[&[("minecraft:polished_sulfur_stairs", 1)]], &[&[("minecraft:sulfur_stairs", 1)]], &[&[]], &[&[]]),
        (788, &[&[("minecraft:sulfur", 1)]], &[&[("mekanism:dust_sulfur", 4)]], &[&[]], &[&[]]),
        (789, &[&[("minecraft:polished_sulfur_wall", 1)]], &[&[("minecraft:sulfur_wall", 1)]], &[&[]], &[&[]]),
        (790, &[&[("minecraft:tuff_brick_slab", 1)]], &[&[("minecraft:polished_tuff_slab", 1)]], &[&[]], &[&[]]),
        (791, &[&[("minecraft:tuff_brick_stairs", 1)]], &[&[("minecraft:polished_tuff_stairs", 1)]], &[&[]], &[&[]]),
        (792, &[&[("minecraft:tuff_brick_wall", 1)]], &[&[("minecraft:polished_tuff_wall", 1)]], &[&[]], &[&[]]),
        (793, &[&[("minecraft:tuff_bricks", 1)]], &[&[("minecraft:polished_tuff", 1)]], &[&[]], &[&[]]),
        (794, &[&[("minecraft:polished_tuff", 1)]], &[&[("minecraft:tuff", 1)]], &[&[]], &[&[]]),
        (795, &[&[("minecraft:polished_tuff_slab", 1)]], &[&[("minecraft:tuff_slab", 1)]], &[&[]], &[&[]]),
        (796, &[&[("minecraft:polished_tuff_stairs", 1)]], &[&[("minecraft:tuff_stairs", 1)]], &[&[]], &[&[]]),
        (797, &[&[("minecraft:chiseled_tuff", 1)]], &[&[("minecraft:tuff_bricks", 1)]], &[&[]], &[&[]]),
        (798, &[&[("minecraft:polished_tuff_wall", 1)]], &[&[("minecraft:tuff_wall", 1)]], &[&[]], &[&[]]),
        (832, &[&[("minecraft:cyan_terracotta", 1)]], &[&[("minecraft:cyan_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (863, &[&[("minecraft:cobbled_deepslate", 1)]], &[&[("minecraft:deepslate", 1)]], &[&[]], &[&[]]),
        (914, &[&[("minecraft:deepslate_diamond_ore", 1)]], &[&[("minecraft:diamond", 1)]], &[&[]], &[&[]]),
        (915, &[&[("minecraft:diamond_ore", 1)]], &[&[("minecraft:diamond", 1)]], &[&[]], &[&[]]),
        (941, &[&[("minecraft:kelp", 1)]], &[&[("minecraft:dried_kelp", 1)]], &[&[]], &[&[]]),
        (1073, &[&[("minecraft:deepslate_emerald_ore", 1)]], &[&[("minecraft:emerald", 1)]], &[&[]], &[&[]]),
        (1074, &[&[("minecraft:emerald_ore", 1)]], &[&[("minecraft:emerald", 1)]], &[&[]], &[&[]]),
        (1100, &[&[("mekanism:dust_charcoal", 1)]], &[&[("minecraft:charcoal", 1)]], &[&[]], &[&[]]),
        (1101, &[&[("mekanism:sawdust", 8)]], &[&[("mekanism:dust_charcoal", 1)]], &[&[]], &[&[]]),
        (1102, &[&[("minecraft:clay", 1)]], &[&[("minecraft:clay_ball", 4)]], &[&[]], &[&[]]),
        (1103, &[&[("minecraft:andesite_slab", 1)]], &[&[("minecraft:polished_andesite_slab", 1)]], &[&[]], &[&[]]),
        (1104, &[&[("minecraft:andesite_stairs", 1)]], &[&[("minecraft:polished_andesite_stairs", 1)]], &[&[]], &[&[]]),
        (1105, &[&[("minecraft:andesite", 1)]], &[&[("minecraft:polished_andesite", 1)]], &[&[]], &[&[]]),
        (1106, &[&[("minecraft:basalt", 1), ("minecraft:smooth_basalt", 1)]], &[&[("minecraft:polished_basalt", 1)]], &[&[]], &[&[]]),
        (1107, &[&[("minecraft:polished_blackstone_bricks", 1)]], &[&[("minecraft:chiseled_polished_blackstone", 1)]], &[&[]], &[&[]]),
        (1108, &[&[("minecraft:cracked_polished_blackstone_bricks", 1)]], &[&[("minecraft:polished_blackstone_bricks", 1)]], &[&[]], &[&[]]),
        (1109, &[&[("minecraft:polished_blackstone", 1)]], &[&[("minecraft:cracked_polished_blackstone_bricks", 1)]], &[&[]], &[&[]]),
        (1110, &[&[("minecraft:cinnabar_bricks", 1)]], &[&[("minecraft:chiseled_cinnabar", 1)]], &[&[]], &[&[]]),
        (1111, &[&[("minecraft:polished_cinnabar_slab", 1)]], &[&[("minecraft:cinnabar_brick_slab", 1)]], &[&[]], &[&[]]),
        (1112, &[&[("minecraft:polished_cinnabar_stairs", 1)]], &[&[("minecraft:cinnabar_brick_stairs", 1)]], &[&[]], &[&[]]),
        (1113, &[&[("minecraft:polished_cinnabar", 1)]], &[&[("minecraft:cinnabar_bricks", 1)]], &[&[]], &[&[]]),
        (1114, &[&[("minecraft:polished_cinnabar_wall", 1)]], &[&[("minecraft:cinnabar_brick_wall", 1)]], &[&[]], &[&[]]),
        (1115, &[&[("minecraft:cinnabar_slab", 1)]], &[&[("minecraft:polished_cinnabar_slab", 1)]], &[&[]], &[&[]]),
        (1116, &[&[("minecraft:cinnabar_stairs", 1)]], &[&[("minecraft:polished_cinnabar_stairs", 1)]], &[&[]], &[&[]]),
        (1117, &[&[("minecraft:cinnabar", 1)]], &[&[("minecraft:polished_cinnabar", 1)]], &[&[]], &[&[]]),
        (1118, &[&[("minecraft:cinnabar_wall", 1)]], &[&[("minecraft:polished_cinnabar_wall", 1)]], &[&[]], &[&[]]),
        (1119, &[&[("minecraft:cracked_nether_bricks", 1)]], &[&[("minecraft:nether_bricks", 1)]], &[&[]], &[&[]]),
        (1120, &[&[("minecraft:chiseled_deepslate", 1)]], &[&[("minecraft:cracked_deepslate_tiles", 1)]], &[&[]], &[&[]]),
        (1121, &[&[("minecraft:cobbled_deepslate_slab", 1)]], &[&[("minecraft:polished_deepslate_slab", 1)]], &[&[]], &[&[]]),
        (1122, &[&[("minecraft:cobbled_deepslate_stairs", 1)]], &[&[("minecraft:polished_deepslate_stairs", 1)]], &[&[]], &[&[]]),
        (1123, &[&[("minecraft:cobbled_deepslate_wall", 1)]], &[&[("minecraft:polished_deepslate_wall", 1)]], &[&[]], &[&[]]),
        (1124, &[&[("minecraft:cracked_deepslate_bricks", 1)]], &[&[("minecraft:deepslate_bricks", 1)]], &[&[]], &[&[]]),
        (1125, &[&[("minecraft:cracked_deepslate_tiles", 1)]], &[&[("minecraft:deepslate_tiles", 1)]], &[&[]], &[&[]]),
        (1126, &[&[("minecraft:cobbled_deepslate", 1)]], &[&[("minecraft:deepslate", 1)]], &[&[]], &[&[]]),
        (1127, &[&[("minecraft:polished_deepslate", 1)]], &[&[("minecraft:chiseled_deepslate", 1)]], &[&[]], &[&[]]),
        (1128, &[&[("minecraft:polished_deepslate_slab", 1)]], &[&[("minecraft:deepslate_tile_slab", 1)]], &[&[]], &[&[]]),
        (1129, &[&[("minecraft:polished_deepslate_stairs", 1)]], &[&[("minecraft:deepslate_tile_stairs", 1)]], &[&[]], &[&[]]),
        (1130, &[&[("minecraft:polished_deepslate_wall", 1)]], &[&[("minecraft:deepslate_tile_wall", 1)]], &[&[]], &[&[]]),
        (1131, &[&[("minecraft:deepslate_tile_slab", 1)]], &[&[("minecraft:deepslate_brick_slab", 1)]], &[&[]], &[&[]]),
        (1132, &[&[("minecraft:deepslate_tile_stairs", 1)]], &[&[("minecraft:deepslate_brick_stairs", 1)]], &[&[]], &[&[]]),
        (1133, &[&[("minecraft:deepslate_tile_wall", 1)]], &[&[("minecraft:deepslate_brick_wall", 1)]], &[&[]], &[&[]]),
        (1134, &[&[("minecraft:deepslate_tiles", 1)]], &[&[("minecraft:cracked_deepslate_bricks", 1)]], &[&[]], &[&[]]),
        (1135, &[&[("minecraft:deepslate", 1)]], &[&[("minecraft:polished_deepslate", 1)]], &[&[]], &[&[]]),
        (1136, &[&[("minecraft:diorite_slab", 1)]], &[&[("minecraft:polished_diorite_slab", 1)]], &[&[]], &[&[]]),
        (1137, &[&[("minecraft:diorite_stairs", 1)]], &[&[("minecraft:polished_diorite_stairs", 1)]], &[&[]], &[&[]]),
        (1138, &[&[("minecraft:diorite", 1)]], &[&[("minecraft:polished_diorite", 1)]], &[&[]], &[&[]]),
        (1139, &[&[("minecraft:granite_slab", 1)]], &[&[("minecraft:polished_granite_slab", 1)]], &[&[]], &[&[]]),
        (1140, &[&[("minecraft:granite_stairs", 1)]], &[&[("minecraft:polished_granite_stairs", 1)]], &[&[]], &[&[]]),
        (1141, &[&[("minecraft:granite", 1)]], &[&[("minecraft:polished_granite", 1)]], &[&[]], &[&[]]),
        (1142, &[&[("minecraft:gravel", 1)]], &[&[("minecraft:flint", 1)]], &[&[]], &[&[]]),
        (1143, &[&[("minecraft:gunpowder", 1)]], &[&[("minecraft:flint", 1)]], &[&[]], &[&[]]),
        (1144, &[&[("minecraft:mossy_cobblestone", 1)]], &[&[("minecraft:cobblestone", 1)]], &[&[]], &[&[]]),
        (1145, &[&[("minecraft:mossy_cobblestone_slab", 1)]], &[&[("minecraft:cobblestone_slab", 1)]], &[&[]], &[&[]]),
        (1146, &[&[("minecraft:mossy_cobblestone_stairs", 1)]], &[&[("minecraft:cobblestone_stairs", 1)]], &[&[]], &[&[]]),
        (1147, &[&[("minecraft:mossy_cobblestone_wall", 1)]], &[&[("minecraft:cobblestone_wall", 1)]], &[&[]], &[&[]]),
        (1148, &[&[("minecraft:mossy_stone_brick_slab", 1)]], &[&[("minecraft:stone_brick_slab", 1)]], &[&[]], &[&[]]),
        (1149, &[&[("minecraft:mossy_stone_brick_stairs", 1)]], &[&[("minecraft:stone_brick_stairs", 1)]], &[&[]], &[&[]]),
        (1150, &[&[("minecraft:mossy_stone_brick_wall", 1)]], &[&[("minecraft:stone_brick_wall", 1)]], &[&[]], &[&[]]),
        (1151, &[&[("minecraft:mossy_stone_bricks", 1)]], &[&[("minecraft:stone_bricks", 1)]], &[&[]], &[&[]]),
        (1152, &[&[("minecraft:packed_mud", 1)]], &[&[("minecraft:mud_bricks", 1)]], &[&[]], &[&[]]),
        (1153, &[&[("minecraft:mud", 1)]], &[&[("minecraft:clay", 1)]], &[&[]], &[&[]]),
        (1154, &[&[("minecraft:nether_bricks", 1)]], &[&[("minecraft:chiseled_nether_bricks", 1)]], &[&[]], &[&[]]),
        (1155, &[&[("minecraft:purpur_block", 1)]], &[&[("minecraft:purpur_pillar", 1)]], &[&[]], &[&[]]),
        (1156, &[&[("minecraft:smooth_quartz", 1)]], &[&[("minecraft:quartz_block", 1)]], &[&[]], &[&[]]),
        (1157, &[&[("minecraft:smooth_quartz_slab", 1)]], &[&[("minecraft:quartz_slab", 1)]], &[&[]], &[&[]]),
        (1158, &[&[("minecraft:smooth_quartz_stairs", 1)]], &[&[("minecraft:quartz_stairs", 1)]], &[&[]], &[&[]]),
        (1159, &[&[("minecraft:quartz_block", 1), ("minecraft:quartz_bricks", 1), ("minecraft:chiseled_quartz_block", 1), ("minecraft:quartz_pillar", 1)]], &[&[("minecraft:quartz", 4)]], &[&[]], &[&[]]),
        (1160, &[&[("minecraft:resin_block", 1)]], &[&[("minecraft:resin_bricks", 1)]], &[&[]], &[&[]]),
        (1161, &[&[("minecraft:resin_bricks", 1)]], &[&[("minecraft:chiseled_resin_bricks", 1)]], &[&[]], &[&[]]),
        (1162, &[&[("minecraft:sand", 1), ("minecraft:red_sand", 1)]], &[&[("minecraft:gravel", 1)]], &[&[]], &[&[]]),
        (1163, &[&[("minecraft:soul_sand", 1)]], &[&[("minecraft:soul_soil", 1)]], &[&[]], &[&[]]),
        (1164, &[&[("minecraft:stone_bricks", 1)]], &[&[("minecraft:chiseled_stone_bricks", 1)]], &[&[]], &[&[]]),
        (1165, &[&[("minecraft:cracked_stone_bricks", 1)]], &[&[("minecraft:stone_bricks", 1)]], &[&[]], &[&[]]),
        (1166, &[&[("minecraft:stone", 1)]], &[&[("minecraft:cracked_stone_bricks", 1)]], &[&[]], &[&[]]),
        (1167, &[&[("minecraft:sulfur_bricks", 1)]], &[&[("minecraft:chiseled_sulfur", 1)]], &[&[]], &[&[]]),
        (1168, &[&[("mekanism:dust_sulfur", 1)]], &[&[("minecraft:sulfur_spike", 1)]], &[&[]], &[&[]]),
        (1169, &[&[("minecraft:polished_sulfur_slab", 1)]], &[&[("minecraft:sulfur_brick_slab", 1)]], &[&[]], &[&[]]),
        (1170, &[&[("minecraft:polished_sulfur_stairs", 1)]], &[&[("minecraft:sulfur_brick_stairs", 1)]], &[&[]], &[&[]]),
        (1171, &[&[("minecraft:polished_sulfur", 1)]], &[&[("minecraft:sulfur_bricks", 1)]], &[&[]], &[&[]]),
        (1172, &[&[("minecraft:polished_sulfur_wall", 1)]], &[&[("minecraft:sulfur_brick_wall", 1)]], &[&[]], &[&[]]),
        (1173, &[&[("minecraft:sulfur_slab", 1)]], &[&[("minecraft:polished_sulfur_slab", 1)]], &[&[]], &[&[]]),
        (1174, &[&[("minecraft:sulfur_spike", 1)]], &[&[("minecraft:gunpowder", 1)]], &[&[]], &[&[]]),
        (1175, &[&[("minecraft:sulfur_stairs", 1)]], &[&[("minecraft:polished_sulfur_stairs", 1)]], &[&[]], &[&[]]),
        (1176, &[&[("minecraft:sulfur", 1)]], &[&[("minecraft:polished_sulfur", 1)]], &[&[]], &[&[]]),
        (1177, &[&[("minecraft:sulfur_wall", 1)]], &[&[("minecraft:polished_sulfur_wall", 1)]], &[&[]], &[&[]]),
        (1178, &[&[("minecraft:tuff_bricks", 1)]], &[&[("minecraft:chiseled_tuff", 1)]], &[&[]], &[&[]]),
        (1179, &[&[("minecraft:polished_tuff_slab", 1)]], &[&[("minecraft:tuff_brick_slab", 1)]], &[&[]], &[&[]]),
        (1180, &[&[("minecraft:polished_tuff_stairs", 1)]], &[&[("minecraft:tuff_brick_stairs", 1)]], &[&[]], &[&[]]),
        (1181, &[&[("minecraft:polished_tuff", 1)]], &[&[("minecraft:tuff_bricks", 1)]], &[&[]], &[&[]]),
        (1182, &[&[("minecraft:polished_tuff_wall", 1)]], &[&[("minecraft:tuff_brick_wall", 1)]], &[&[]], &[&[]]),
        (1183, &[&[("minecraft:tuff_slab", 1)]], &[&[("minecraft:polished_tuff_slab", 1)]], &[&[]], &[&[]]),
        (1184, &[&[("minecraft:tuff_stairs", 1)]], &[&[("minecraft:polished_tuff_stairs", 1)]], &[&[]], &[&[]]),
        (1185, &[&[("minecraft:tuff", 1)]], &[&[("minecraft:polished_tuff", 1)]], &[&[]], &[&[]]),
        (1186, &[&[("minecraft:tuff_wall", 1)]], &[&[("minecraft:polished_tuff_wall", 1)]], &[&[]], &[&[]]),
        (1187, &[&[("minecraft:exposed_chiseled_copper", 1)]], &[&[("minecraft:chiseled_copper", 1)]], &[&[]], &[&[]]),
        (1188, &[&[("minecraft:exposed_copper_bars", 1)]], &[&[("minecraft:copper_bars", 1)]], &[&[]], &[&[]]),
        (1189, &[&[("minecraft:exposed_copper", 1)]], &[&[("minecraft:copper_block", 1)]], &[&[]], &[&[]]),
        (1190, &[&[("minecraft:exposed_copper_bulb", 1)]], &[&[("minecraft:copper_bulb", 1)]], &[&[]], &[&[]]),
        (1191, &[&[("minecraft:exposed_copper_chain", 1)]], &[&[("minecraft:copper_chain", 1)]], &[&[]], &[&[]]),
        (1192, &[&[("minecraft:exposed_copper_chest", 1)]], &[&[("minecraft:copper_chest", 1)]], &[&[]], &[&[]]),
        (1193, &[&[("minecraft:exposed_copper_door", 1)]], &[&[("minecraft:copper_door", 1)]], &[&[]], &[&[]]),
        (1194, &[&[("minecraft:exposed_copper_golem_statue", 1)]], &[&[("minecraft:copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (1195, &[&[("minecraft:exposed_copper_grate", 1)]], &[&[("minecraft:copper_grate", 1)]], &[&[]], &[&[]]),
        (1196, &[&[("minecraft:exposed_copper_lantern", 1)]], &[&[("minecraft:copper_lantern", 1)]], &[&[]], &[&[]]),
        (1197, &[&[("minecraft:exposed_copper_trapdoor", 1)]], &[&[("minecraft:copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (1198, &[&[("minecraft:exposed_cut_copper", 1)]], &[&[("minecraft:cut_copper", 1)]], &[&[]], &[&[]]),
        (1199, &[&[("minecraft:exposed_cut_copper_slab", 1)]], &[&[("minecraft:cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (1200, &[&[("minecraft:exposed_cut_copper_stairs", 1)]], &[&[("minecraft:cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (1201, &[&[("minecraft:weathered_chiseled_copper", 1)]], &[&[("minecraft:exposed_chiseled_copper", 1)]], &[&[]], &[&[]]),
        (1202, &[&[("minecraft:weathered_copper", 1)]], &[&[("minecraft:exposed_copper", 1)]], &[&[]], &[&[]]),
        (1203, &[&[("minecraft:weathered_copper_bars", 1)]], &[&[("minecraft:exposed_copper_bars", 1)]], &[&[]], &[&[]]),
        (1204, &[&[("minecraft:weathered_copper_bulb", 1)]], &[&[("minecraft:exposed_copper_bulb", 1)]], &[&[]], &[&[]]),
        (1205, &[&[("minecraft:weathered_copper_chain", 1)]], &[&[("minecraft:exposed_copper_chain", 1)]], &[&[]], &[&[]]),
        (1206, &[&[("minecraft:weathered_copper_chest", 1)]], &[&[("minecraft:exposed_copper_chest", 1)]], &[&[]], &[&[]]),
        (1207, &[&[("minecraft:weathered_copper_door", 1)]], &[&[("minecraft:exposed_copper_door", 1)]], &[&[]], &[&[]]),
        (1208, &[&[("minecraft:weathered_copper_golem_statue", 1)]], &[&[("minecraft:exposed_copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (1209, &[&[("minecraft:weathered_copper_grate", 1)]], &[&[("minecraft:exposed_copper_grate", 1)]], &[&[]], &[&[]]),
        (1210, &[&[("minecraft:weathered_copper_lantern", 1)]], &[&[("minecraft:exposed_copper_lantern", 1)]], &[&[]], &[&[]]),
        (1211, &[&[("minecraft:weathered_copper_trapdoor", 1)]], &[&[("minecraft:exposed_copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (1212, &[&[("minecraft:weathered_cut_copper", 1)]], &[&[("minecraft:exposed_cut_copper", 1)]], &[&[]], &[&[]]),
        (1213, &[&[("minecraft:weathered_cut_copper_slab", 1)]], &[&[("minecraft:exposed_cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (1214, &[&[("minecraft:weathered_cut_copper_stairs", 1)]], &[&[("minecraft:exposed_cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (1215, &[&[("minecraft:weathered_lightning_rod", 1)]], &[&[("minecraft:exposed_lightning_rod", 1)]], &[&[]], &[&[]]),
        (1216, &[&[("minecraft:exposed_lightning_rod", 1)]], &[&[("minecraft:lightning_rod", 1)]], &[&[]], &[&[]]),
        (1217, &[&[("minecraft:oxidized_chiseled_copper", 1)]], &[&[("minecraft:weathered_chiseled_copper", 1)]], &[&[]], &[&[]]),
        (1218, &[&[("minecraft:oxidized_copper", 1)]], &[&[("minecraft:weathered_copper", 1)]], &[&[]], &[&[]]),
        (1219, &[&[("minecraft:oxidized_copper_bars", 1)]], &[&[("minecraft:weathered_copper_bars", 1)]], &[&[]], &[&[]]),
        (1220, &[&[("minecraft:oxidized_copper_bulb", 1)]], &[&[("minecraft:weathered_copper_bulb", 1)]], &[&[]], &[&[]]),
        (1221, &[&[("minecraft:oxidized_copper_chain", 1)]], &[&[("minecraft:weathered_copper_chain", 1)]], &[&[]], &[&[]]),
        (1222, &[&[("minecraft:oxidized_copper_chest", 1)]], &[&[("minecraft:weathered_copper_chest", 1)]], &[&[]], &[&[]]),
        (1223, &[&[("minecraft:oxidized_copper_door", 1)]], &[&[("minecraft:weathered_copper_door", 1)]], &[&[]], &[&[]]),
        (1224, &[&[("minecraft:oxidized_copper_golem_statue", 1)]], &[&[("minecraft:weathered_copper_golem_statue", 1)]], &[&[]], &[&[]]),
        (1225, &[&[("minecraft:oxidized_copper_grate", 1)]], &[&[("minecraft:weathered_copper_grate", 1)]], &[&[]], &[&[]]),
        (1226, &[&[("minecraft:oxidized_copper_lantern", 1)]], &[&[("minecraft:weathered_copper_lantern", 1)]], &[&[]], &[&[]]),
        (1227, &[&[("minecraft:oxidized_copper_trapdoor", 1)]], &[&[("minecraft:weathered_copper_trapdoor", 1)]], &[&[]], &[&[]]),
        (1228, &[&[("minecraft:oxidized_cut_copper", 1)]], &[&[("minecraft:weathered_cut_copper", 1)]], &[&[]], &[&[]]),
        (1229, &[&[("minecraft:oxidized_cut_copper_slab", 1)]], &[&[("minecraft:weathered_cut_copper_slab", 1)]], &[&[]], &[&[]]),
        (1230, &[&[("minecraft:oxidized_cut_copper_stairs", 1)]], &[&[("minecraft:weathered_cut_copper_stairs", 1)]], &[&[]], &[&[]]),
        (1231, &[&[("minecraft:oxidized_lightning_rod", 1)]], &[&[("minecraft:weathered_lightning_rod", 1)]], &[&[]], &[&[]]),
        (1232, &[&[("minecraft:ink_sac", 1), ("minecraft:wither_rose", 1)]], &[&[("minecraft:black_dye", 2)]], &[&[]], &[&[]]),
        (1233, &[&[("minecraft:cornflower", 1), ("minecraft:lapis_lazuli", 1)]], &[&[("minecraft:blue_dye", 2)]], &[&[]], &[&[]]),
        (1234, &[&[("minecraft:cocoa_beans", 1)]], &[&[("minecraft:brown_dye", 2)]], &[&[]], &[&[]]),
        (1235, &[&[("minecraft:closed_eyeblossom", 1)]], &[&[("minecraft:gray_dye", 2)]], &[&[]], &[&[]]),
        (1236, &[&[("minecraft:cactus", 1)]], &[&[("minecraft:green_dye", 2)]], &[&[]], &[&[]]),
        (1237, &[&[("minecraft:pitcher_plant", 1)]], &[&[("minecraft:cyan_dye", 4)]], &[&[]], &[&[]]),
        (1238, &[&[("minecraft:lilac", 1)]], &[&[("minecraft:magenta_dye", 4)]], &[&[]], &[&[]]),
        (1239, &[&[("minecraft:peony", 1)]], &[&[("minecraft:pink_dye", 4)]], &[&[]], &[&[]]),
        (1240, &[&[("minecraft:rose_bush", 1)]], &[&[("minecraft:red_dye", 4)]], &[&[]], &[&[]]),
        (1241, &[&[("minecraft:sunflower", 1)]], &[&[("minecraft:yellow_dye", 4)]], &[&[]], &[&[]]),
        (1242, &[&[("minecraft:blue_orchid", 1)]], &[&[("minecraft:light_blue_dye", 2)]], &[&[]], &[&[]]),
        (1243, &[&[("minecraft:azure_bluet", 1), ("minecraft:oxeye_daisy", 1), ("minecraft:white_tulip", 1)]], &[&[("minecraft:light_gray_dye", 2)]], &[&[]], &[&[]]),
        (1244, &[&[("minecraft:sea_pickle", 1)]], &[&[("minecraft:lime_dye", 2)]], &[&[]], &[&[]]),
        (1245, &[&[("minecraft:open_eyeblossom", 1), ("minecraft:orange_tulip", 1), ("minecraft:torchflower", 1)]], &[&[("minecraft:orange_dye", 2)]], &[&[]], &[&[]]),
        (1246, &[&[("minecraft:allium", 1)]], &[&[("minecraft:magenta_dye", 2)]], &[&[]], &[&[]]),
        (1247, &[&[("minecraft:cactus_flower", 1), ("minecraft:pink_tulip", 1), ("minecraft:pink_petals", 1)]], &[&[("minecraft:pink_dye", 2)]], &[&[]], &[&[]]),
        (1248, &[&[("minecraft:beetroot", 1), ("minecraft:poppy", 1), ("minecraft:red_tulip", 1)]], &[&[("minecraft:red_dye", 2)]], &[&[]], &[&[]]),
        (1249, &[&[("minecraft:dandelion", 1), ("minecraft:golden_dandelion", 1), ("minecraft:wildflowers", 1)]], &[&[("minecraft:yellow_dye", 2)]], &[&[]], &[&[]]),
        (1250, &[&[("minecraft:bone_meal", 1), ("minecraft:lily_of_the_valley", 1)]], &[&[("minecraft:white_dye", 2)]], &[&[]], &[&[]]),
        (1251, &[&[("minecraft:coal", 1), ("minecraft:charcoal", 1)]], &[&[("mekanism:enriched_carbon", 1)]], &[&[]], &[&[]]),
        (1252, &[&[("minecraft:diamond", 1)]], &[&[("mekanism:enriched_diamond", 1)]], &[&[]], &[&[]]),
        (1253, &[&[("mekanism:dust_gold", 1)]], &[&[("mekanism:enriched_gold", 1)]], &[&[]], &[&[]]),
        (1254, &[&[("minecraft:redstone", 1)]], &[&[("mekanism:enriched_redstone", 1)]], &[&[]], &[&[]]),
        (1255, &[&[("mekanism:dust_refined_obsidian", 1)]], &[&[("mekanism:enriched_refined_obsidian", 1)]], &[&[]], &[&[]]),
        (1256, &[&[("mekanism:dust_tin", 1)]], &[&[("mekanism:enriched_tin", 1)]], &[&[]], &[&[]]),
        (1257, &[&[("minecraft:glowstone", 1)]], &[&[("minecraft:glowstone_dust", 4)]], &[&[]], &[&[]]),
        (1258, &[&[("mekanism:hdpe_pellet", 3)]], &[&[("mekanism:hdpe_sheet", 1)]], &[&[]], &[&[]]),
        (1259, &[&[("mekanism:block_salt", 1)]], &[&[("mekanism:salt", 4)]], &[&[]], &[&[]]),
        (1260, &[&[("minecraft:cobweb", 1)]], &[&[("minecraft:string", 9)]], &[&[]], &[&[]]),
        (1358, &[&[("minecraft:sand", 1), ("minecraft:red_sand", 1)]], &[&[("minecraft:glass", 1)]], &[&[]], &[&[]]),
        (1421, &[&[("minecraft:deepslate_gold_ore", 1)]], &[&[("minecraft:gold_ingot", 1)]], &[&[]], &[&[]]),
        (1422, &[&[("minecraft:gold_ore", 1)]], &[&[("minecraft:gold_ingot", 1)]], &[&[]], &[&[]]),
        (1423, &[&[("minecraft:nether_gold_ore", 1)]], &[&[("minecraft:gold_ingot", 1)]], &[&[]], &[&[]]),
        (1424, &[&[("minecraft:raw_gold", 1)]], &[&[("minecraft:gold_ingot", 1)]], &[&[]], &[&[]]),
        (1428, &[&[("mekanismtools:gold_paxel", 1)]], &[&[("minecraft:gold_nugget", 1)]], &[&[]], &[&[]]),
        (1429, &[&[("minecraft:golden_pickaxe", 1), ("minecraft:golden_shovel", 1), ("minecraft:golden_axe", 1), ("minecraft:golden_hoe", 1), ("minecraft:golden_sword", 1), ("minecraft:golden_spear", 1), ("minecraft:golden_helmet", 1), ("minecraft:golden_chestplate", 1), ("minecraft:golden_leggings", 1), ("minecraft:golden_boots", 1), ("minecraft:golden_horse_armor", 1), ("minecraft:golden_nautilus_armor", 1)]], &[&[("minecraft:gold_nugget", 1)]], &[&[]], &[&[]]),
        (1465, &[&[("minecraft:gray_terracotta", 1)]], &[&[("minecraft:gray_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1485, &[&[("minecraft:cactus", 1)]], &[&[("minecraft:green_dye", 1)]], &[&[]], &[&[]]),
        (1486, &[&[("minecraft:green_terracotta", 1)]], &[&[("minecraft:green_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1506, &[&[("mekanism:dust_gold", 4)], &[]], &[&[("mekanismgenerators:hohlraum", 1)]], &[&[], &[("mekanism:carbon", 10)]], &[&[]]),
        (1622, &[&[("minecraft:deepslate_iron_ore", 1)]], &[&[("minecraft:iron_ingot", 1)]], &[&[]], &[&[]]),
        (1623, &[&[("minecraft:iron_ore", 1)]], &[&[("minecraft:iron_ingot", 1)]], &[&[]], &[&[]]),
        (1624, &[&[("minecraft:raw_iron", 1)]], &[&[("minecraft:iron_ingot", 1)]], &[&[]], &[&[]]),
        (1629, &[&[("mekanismtools:iron_paxel", 1)]], &[&[("minecraft:iron_nugget", 1)]], &[&[]], &[&[]]),
        (1630, &[&[("minecraft:iron_pickaxe", 1), ("minecraft:iron_shovel", 1), ("minecraft:iron_axe", 1), ("minecraft:iron_hoe", 1), ("minecraft:iron_sword", 1), ("minecraft:iron_spear", 1), ("minecraft:iron_helmet", 1), ("minecraft:iron_chestplate", 1), ("minecraft:iron_leggings", 1), ("minecraft:iron_boots", 1), ("minecraft:iron_horse_armor", 1), ("minecraft:chainmail_helmet", 1), ("minecraft:chainmail_chestplate", 1), ("minecraft:chainmail_leggings", 1), ("minecraft:chainmail_boots", 1), ("minecraft:iron_nautilus_armor", 1)]], &[&[("minecraft:iron_nugget", 1)]], &[&[]], &[&[]]),
        (1679, &[&[("minecraft:deepslate_lapis_ore", 1)]], &[&[("minecraft:lapis_lazuli", 1)]], &[&[]], &[&[]]),
        (1680, &[&[("minecraft:lapis_ore", 1)]], &[&[("minecraft:lapis_lazuli", 1)]], &[&[]], &[&[]]),
        (1687, &[&[("minecraft:jungle_leaves", 1), ("minecraft:oak_leaves", 1), ("minecraft:spruce_leaves", 1), ("minecraft:pale_oak_leaves", 1), ("minecraft:dark_oak_leaves", 1), ("minecraft:acacia_leaves", 1), ("minecraft:birch_leaves", 1), ("minecraft:azalea_leaves", 1), ("minecraft:flowering_azalea_leaves", 1), ("minecraft:mangrove_leaves", 1), ("minecraft:cherry_leaves", 1), ("minecraft:red_poplar_leaves", 1), ("minecraft:orange_poplar_leaves", 1), ("minecraft:yellow_poplar_leaves", 1)]], &[&[("minecraft:leaf_litter", 1)]], &[&[]], &[&[]]),
        (1714, &[&[("minecraft:light_blue_terracotta", 1)]], &[&[("minecraft:light_blue_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1739, &[&[("minecraft:light_gray_terracotta", 1)]], &[&[("minecraft:light_gray_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1762, &[&[("minecraft:sea_pickle", 1)]], &[&[("minecraft:lime_dye", 1)]], &[&[]], &[&[]]),
        (1763, &[&[("minecraft:lime_terracotta", 1)]], &[&[("minecraft:lime_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1793, &[&[("minecraft:magenta_terracotta", 1)]], &[&[("minecraft:magenta_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1837, &[&[("mekanism:alloy_reinforced", 1)], &[]], &[&[("mekanism:alloy_atomic", 1)]], &[&[], &[("mekanism:refined_obsidian", 40)]], &[&[]]),
        (1838, &[&[("minecraft:copper_ingot", 1)], &[]], &[&[("mekanism:alloy_infused", 1)]], &[&[], &[("mekanism:redstone", 10)]], &[&[]]),
        (1839, &[&[("mekanism:alloy_infused", 1)], &[]], &[&[("mekanism:alloy_reinforced", 1)]], &[&[], &[("mekanism:diamond", 20)]], &[&[]]),
        (1840, &[&[("minecraft:blackstone", 1)], &[]], &[&[("minecraft:gilded_blackstone", 1)]], &[&[], &[("mekanism:gold", 100)]], &[&[]]),
        (1841, &[&[("minecraft:clay_ball", 1)], &[]], &[&[("minecraft:slime_ball", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1842, &[&[("minecraft:clay", 1)], &[]], &[&[("minecraft:slime_ball", 4)]], &[&[], &[("mekanism:bio", 40)]], &[&[]]),
        (1843, &[&[("minecraft:crimson_nylium", 1)], &[]], &[&[("minecraft:warped_nylium", 1)]], &[&[], &[("mekanism:fungi", 10)]], &[&[]]),
        (1844, &[&[("minecraft:dirt", 1)], &[]], &[&[("minecraft:mycelium", 1)]], &[&[], &[("mekanism:fungi", 10)]], &[&[]]),
        (1845, &[&[("minecraft:dirt", 1)], &[]], &[&[("minecraft:podzol", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1846, &[&[("minecraft:cobblestone", 1)], &[]], &[&[("minecraft:mossy_cobblestone", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1847, &[&[("minecraft:cobblestone_slab", 1)], &[]], &[&[("minecraft:mossy_cobblestone_slab", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1848, &[&[("minecraft:cobblestone_stairs", 1)], &[]], &[&[("minecraft:mossy_cobblestone_stairs", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1849, &[&[("minecraft:cobblestone_wall", 1)], &[]], &[&[("minecraft:mossy_cobblestone_wall", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1850, &[&[("minecraft:stone_bricks", 1)], &[]], &[&[("minecraft:mossy_stone_bricks", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1851, &[&[("minecraft:stone_brick_slab", 1)], &[]], &[&[("minecraft:mossy_stone_brick_slab", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1852, &[&[("minecraft:stone_brick_stairs", 1)], &[]], &[&[("minecraft:mossy_stone_brick_stairs", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1853, &[&[("minecraft:stone_brick_wall", 1)], &[]], &[&[("minecraft:mossy_stone_brick_wall", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1854, &[&[("minecraft:netherrack", 1)], &[]], &[&[("minecraft:crimson_nylium", 1)]], &[&[], &[("mekanism:fungi", 10)]], &[&[]]),
        (1855, &[&[("minecraft:sand", 1), ("minecraft:red_sand", 1)], &[]], &[&[("minecraft:dirt", 1)]], &[&[], &[("mekanism:bio", 10)]], &[&[]]),
        (1889, &[&[("minecraft:netherrack", 1)]], &[&[("minecraft:nether_brick", 1)]], &[&[]], &[&[]]),
        (1912, &[&[("minecraft:ancient_debris", 1)]], &[&[("minecraft:netherite_scrap", 1)]], &[&[]], &[&[]]),
        (1981, &[&[("minecraft:orange_terracotta", 1)]], &[&[("minecraft:orange_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (1998, &[&[("mekanismtools:osmium_helmet", 1), ("mekanismtools:osmium_chestplate", 1), ("mekanismtools:osmium_leggings", 1), ("mekanismtools:osmium_boots", 1), ("mekanismtools:osmium_horse_armor", 1), ("mekanismtools:osmium_nautilus_armor", 1), ("mekanismtools:osmium_axe", 1), ("mekanismtools:osmium_hoe", 1), ("mekanismtools:osmium_paxel", 1), ("mekanismtools:osmium_pickaxe", 1), ("mekanismtools:osmium_shield", 1), ("mekanismtools:osmium_shovel", 1), ("mekanismtools:osmium_sword", 1), ("mekanismtools:osmium_spear", 1)]], &[&[("mekanism:nugget_osmium", 1)]], &[&[]], &[&[]]),
        (2881, &[&[("minecraft:pink_terracotta", 1)]], &[&[("minecraft:pink_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (3417, &[&[("mekanismadditions:aqua_plastic", 1)]], &[&[("mekanismadditions:aqua_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3418, &[&[("mekanismadditions:black_plastic", 1)]], &[&[("mekanismadditions:black_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3419, &[&[("mekanismadditions:blue_plastic", 1)]], &[&[("mekanismadditions:blue_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3420, &[&[("mekanismadditions:brown_plastic", 1)]], &[&[("mekanismadditions:brown_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3421, &[&[("mekanismadditions:cyan_plastic", 1)]], &[&[("mekanismadditions:cyan_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3422, &[&[("mekanismadditions:dark_red_plastic", 1)]], &[&[("mekanismadditions:dark_red_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3423, &[&[("mekanismadditions:gray_plastic", 1)]], &[&[("mekanismadditions:gray_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3424, &[&[("mekanismadditions:green_plastic", 1)]], &[&[("mekanismadditions:green_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3425, &[&[("mekanismadditions:light_blue_plastic", 1)]], &[&[("mekanismadditions:light_blue_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3426, &[&[("mekanismadditions:light_gray_plastic", 1)]], &[&[("mekanismadditions:light_gray_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3427, &[&[("mekanismadditions:lime_plastic", 1)]], &[&[("mekanismadditions:lime_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3428, &[&[("mekanismadditions:magenta_plastic", 1)]], &[&[("mekanismadditions:magenta_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3429, &[&[("mekanismadditions:orange_plastic", 1)]], &[&[("mekanismadditions:orange_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3430, &[&[("mekanismadditions:pink_plastic", 1)]], &[&[("mekanismadditions:pink_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3431, &[&[("mekanismadditions:purple_plastic", 1)]], &[&[("mekanismadditions:purple_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3432, &[&[("mekanismadditions:red_plastic", 1)]], &[&[("mekanismadditions:red_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3433, &[&[("mekanismadditions:white_plastic", 1)]], &[&[("mekanismadditions:white_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3434, &[&[("mekanismadditions:yellow_plastic", 1)]], &[&[("mekanismadditions:yellow_slick_plastic", 1)]], &[&[]], &[&[]]),
        (3804, &[&[("minecraft:chorus_fruit", 1)]], &[&[("minecraft:popped_chorus_fruit", 1)]], &[&[]], &[&[]]),
        (3824, &[&[("mekanism:dust_copper", 3)], &[]], &[&[("mekanism:dust_bronze", 4)]], &[&[], &[("mekanism:tin", 10)]], &[&[]]),
        (3825, &[&[("mekanism:ingot_bronze", 1)]], &[&[("mekanism:dust_bronze", 1)]], &[&[]], &[&[]]),
        (3828, &[&[("mekanism:dust_bronze", 1)]], &[&[("mekanism:ingot_bronze", 1)]], &[&[]], &[&[]]),
        (3829, &[&[("minecraft:copper_ingot", 3)], &[]], &[&[("mekanism:ingot_bronze", 4)]], &[&[], &[("mekanism:tin", 10)]], &[&[]]),
        (3831, &[&[("mekanism:dust_coal", 1)]], &[&[("minecraft:coal", 1)]], &[&[]], &[&[]]),
        (3832, &[&[("minecraft:coal_ore", 1), ("minecraft:deepslate_coal_ore", 1)]], &[&[("minecraft:coal", 2)]], &[&[]], &[&[]]),
        (3834, &[&[("minecraft:coal", 1)]], &[&[("mekanism:dust_coal", 1)]], &[&[]], &[&[]]),
        (3841, &[&[("mekanism:clump_copper", 1)]], &[&[("mekanism:dirty_dust_copper", 1)]], &[&[]], &[&[]]),
        (3842, &[&[("mekanism:dirty_dust_copper", 1)]], &[&[("mekanism:dust_copper", 1)]], &[&[]], &[&[]]),
        (3843, &[&[("minecraft:copper_ingot", 1)]], &[&[("mekanism:dust_copper", 1)]], &[&[]], &[&[]]),
        (3844, &[&[("minecraft:copper_ore", 1), ("minecraft:deepslate_copper_ore", 1)]], &[&[("mekanism:dust_copper", 2)]], &[&[]], &[&[]]),
        (3845, &[&[("minecraft:raw_copper_block", 1)]], &[&[("mekanism:dust_copper", 12)]], &[&[]], &[&[]]),
        (3846, &[&[("minecraft:raw_copper", 3)]], &[&[("mekanism:dust_copper", 4)]], &[&[]], &[&[]]),
        (3848, &[&[("mekanism:dust_copper", 1)]], &[&[("minecraft:copper_ingot", 1)]], &[&[]], &[&[]]),
        (3859, &[&[("mekanism:dust_diamond", 1)]], &[&[("minecraft:diamond", 1)]], &[&[]], &[&[]]),
        (3860, &[&[("minecraft:diamond_ore", 1), ("minecraft:deepslate_diamond_ore", 1)]], &[&[("minecraft:diamond", 2)]], &[&[]], &[&[]]),
        (3862, &[&[("minecraft:diamond", 1)]], &[&[("mekanism:dust_diamond", 1)]], &[&[]], &[&[]]),
        (3864, &[&[("mekanism:dust_emerald", 1)]], &[&[("minecraft:emerald", 1)]], &[&[]], &[&[]]),
        (3865, &[&[("minecraft:emerald_ore", 1), ("minecraft:deepslate_emerald_ore", 1)]], &[&[("minecraft:emerald", 2)]], &[&[]], &[&[]]),
        (3867, &[&[("minecraft:emerald", 1)]], &[&[("mekanism:dust_emerald", 1)]], &[&[]], &[&[]]),
        (3871, &[&[("mekanism:deepslate_fluorite_ore", 1)]], &[&[("mekanism:fluorite_gem", 1)]], &[&[]], &[&[]]),
        (3872, &[&[("mekanism:dust_fluorite", 1)]], &[&[("mekanism:fluorite_gem", 1)]], &[&[]], &[&[]]),
        (3873, &[&[("mekanism:fluorite_ore", 1), ("mekanism:deepslate_fluorite_ore", 1)]], &[&[("mekanism:fluorite_gem", 6)]], &[&[]], &[&[]]),
        (3875, &[&[("mekanism:fluorite_ore", 1)]], &[&[("mekanism:fluorite_gem", 1)]], &[&[]], &[&[]]),
        (3877, &[&[("mekanism:fluorite_gem", 1)]], &[&[("mekanism:dust_fluorite", 1)]], &[&[]], &[&[]]),
        (3884, &[&[("mekanism:clump_gold", 1)]], &[&[("mekanism:dirty_dust_gold", 1)]], &[&[]], &[&[]]),
        (3885, &[&[("mekanism:dirty_dust_gold", 1)]], &[&[("mekanism:dust_gold", 1)]], &[&[]], &[&[]]),
        (3886, &[&[("minecraft:gold_ingot", 1)]], &[&[("mekanism:dust_gold", 1)]], &[&[]], &[&[]]),
        (3887, &[&[("minecraft:gold_ore", 1), ("minecraft:nether_gold_ore", 1), ("minecraft:deepslate_gold_ore", 1)]], &[&[("mekanism:dust_gold", 2)]], &[&[]], &[&[]]),
        (3888, &[&[("minecraft:raw_gold_block", 1)]], &[&[("mekanism:dust_gold", 12)]], &[&[]], &[&[]]),
        (3889, &[&[("minecraft:raw_gold", 3)]], &[&[("mekanism:dust_gold", 4)]], &[&[]], &[&[]]),
        (3891, &[&[("mekanism:dust_gold", 1)]], &[&[("minecraft:gold_ingot", 1)]], &[&[]], &[&[]]),
        (3908, &[&[("mekanism:clump_iron", 1)]], &[&[("mekanism:dirty_dust_iron", 1)]], &[&[]], &[&[]]),
        (3909, &[&[("mekanism:dirty_dust_iron", 1)]], &[&[("mekanism:dust_iron", 1)]], &[&[]], &[&[]]),
        (3910, &[&[("minecraft:iron_ingot", 1)]], &[&[("mekanism:dust_iron", 1)]], &[&[]], &[&[]]),
        (3911, &[&[("minecraft:iron_ore", 1), ("minecraft:deepslate_iron_ore", 1)]], &[&[("mekanism:dust_iron", 2)]], &[&[]], &[&[]]),
        (3912, &[&[("minecraft:raw_iron_block", 1)]], &[&[("mekanism:dust_iron", 12)]], &[&[]], &[&[]]),
        (3913, &[&[("minecraft:raw_iron", 3)]], &[&[("mekanism:dust_iron", 4)]], &[&[]], &[&[]]),
        (3914, &[&[("minecraft:iron_ingot", 1)], &[]], &[&[("mekanism:enriched_iron", 1)]], &[&[], &[("mekanism:carbon", 10)]], &[&[]]),
        (3915, &[&[("mekanism:dust_iron", 1)], &[]], &[&[("mekanism:enriched_iron", 1)]], &[&[], &[("mekanism:carbon", 10)]], &[&[]]),
        (3917, &[&[("mekanism:dust_iron", 1)]], &[&[("minecraft:iron_ingot", 1)]], &[&[]], &[&[]]),
        (3928, &[&[("mekanism:dust_lapis_lazuli", 1)]], &[&[("minecraft:lapis_lazuli", 1)]], &[&[]], &[&[]]),
        (3929, &[&[("minecraft:lapis_ore", 1), ("minecraft:deepslate_lapis_ore", 1)]], &[&[("minecraft:lapis_lazuli", 12)]], &[&[]], &[&[]]),
        (3931, &[&[("minecraft:lapis_lazuli", 1)]], &[&[("mekanism:dust_lapis_lazuli", 1)]], &[&[]], &[&[]]),
        (3944, &[&[("mekanism:clump_lead", 1)]], &[&[("mekanism:dirty_dust_lead", 1)]], &[&[]], &[&[]]),
        (3945, &[&[("mekanism:dirty_dust_lead", 1)]], &[&[("mekanism:dust_lead", 1)]], &[&[]], &[&[]]),
        (3946, &[&[("mekanism:ingot_lead", 1)]], &[&[("mekanism:dust_lead", 1)]], &[&[]], &[&[]]),
        (3947, &[&[("mekanism:lead_ore", 1), ("mekanism:deepslate_lead_ore", 1)]], &[&[("mekanism:dust_lead", 2)]], &[&[]], &[&[]]),
        (3948, &[&[("mekanism:block_raw_lead", 1)]], &[&[("mekanism:dust_lead", 12)]], &[&[]], &[&[]]),
        (3949, &[&[("mekanism:raw_lead", 3)]], &[&[("mekanism:dust_lead", 4)]], &[&[]], &[&[]]),
        (3952, &[&[("mekanism:deepslate_lead_ore", 1)]], &[&[("mekanism:ingot_lead", 1)]], &[&[]], &[&[]]),
        (3954, &[&[("mekanism:dust_lead", 1)]], &[&[("mekanism:ingot_lead", 1)]], &[&[]], &[&[]]),
        (3957, &[&[("mekanism:lead_ore", 1)]], &[&[("mekanism:ingot_lead", 1)]], &[&[]], &[&[]]),
        (3959, &[&[("mekanism:raw_lead", 1)]], &[&[("mekanism:ingot_lead", 1)]], &[&[]], &[&[]]),
        (3974, &[&[("minecraft:ancient_debris", 1)]], &[&[("mekanism:dirty_netherite_scrap", 3)]], &[&[]], &[&[]]),
        (3975, &[&[("minecraft:ancient_debris", 1)]], &[&[("minecraft:netherite_scrap", 2)]], &[&[]], &[&[]]),
        (3976, &[&[("mekanism:dirty_netherite_scrap", 1)]], &[&[("minecraft:netherite_scrap", 1)]], &[&[]], &[&[]]),
        (3979, &[&[("mekanism:dust_netherite", 1)]], &[&[("minecraft:netherite_ingot", 1)]], &[&[]], &[&[]]),
        (3980, &[&[("minecraft:netherite_ingot", 1)]], &[&[("mekanism:dust_netherite", 1)]], &[&[]], &[&[]]),
        (3981, &[&[("minecraft:netherite_scrap", 4)], &[]], &[&[("mekanism:dust_netherite", 1)]], &[&[], &[("mekanism:gold", 40)]], &[&[]]),
        (3987, &[&[("mekanism:clump_osmium", 1)]], &[&[("mekanism:dirty_dust_osmium", 1)]], &[&[]], &[&[]]),
        (3988, &[&[("mekanism:dirty_dust_osmium", 1)]], &[&[("mekanism:dust_osmium", 1)]], &[&[]], &[&[]]),
        (3989, &[&[("mekanism:ingot_osmium", 1)]], &[&[("mekanism:dust_osmium", 1)]], &[&[]], &[&[]]),
        (3990, &[&[("mekanism:osmium_ore", 1), ("mekanism:deepslate_osmium_ore", 1)]], &[&[("mekanism:dust_osmium", 2)]], &[&[]], &[&[]]),
        (3991, &[&[("mekanism:block_raw_osmium", 1)]], &[&[("mekanism:dust_osmium", 12)]], &[&[]], &[&[]]),
        (3992, &[&[("mekanism:raw_osmium", 3)]], &[&[("mekanism:dust_osmium", 4)]], &[&[]], &[&[]]),
        (3995, &[&[("mekanism:deepslate_osmium_ore", 1)]], &[&[("mekanism:ingot_osmium", 1)]], &[&[]], &[&[]]),
        (3997, &[&[("mekanism:dust_osmium", 1)]], &[&[("mekanism:ingot_osmium", 1)]], &[&[]], &[&[]]),
        (4000, &[&[("mekanism:osmium_ore", 1)]], &[&[("mekanism:ingot_osmium", 1)]], &[&[]], &[&[]]),
        (4002, &[&[("mekanism:raw_osmium", 1)]], &[&[("mekanism:ingot_osmium", 1)]], &[&[]], &[&[]]),
        (4017, &[&[("mekanism:dust_quartz", 1)]], &[&[("minecraft:quartz", 1)]], &[&[]], &[&[]]),
        (4018, &[&[("minecraft:nether_quartz_ore", 1)]], &[&[("minecraft:quartz", 2)]], &[&[]], &[&[]]),
        (4019, &[&[("minecraft:quartz", 1)]], &[&[("mekanism:dust_quartz", 1)]], &[&[]], &[&[]]),
        (4021, &[&[("minecraft:redstone_ore", 1), ("minecraft:deepslate_redstone_ore", 1)]], &[&[("minecraft:redstone", 12)]], &[&[]], &[&[]]),
        (4027, &[&[("mekanism:ingot_refined_glowstone", 1)]], &[&[("minecraft:glowstone_dust", 1)]], &[&[]], &[&[]]),
        (4028, &[&[("mekanism:ingot_refined_obsidian", 1)]], &[&[("mekanism:dust_refined_obsidian", 1)]], &[&[]], &[&[]]),
        (4029, &[&[("mekanism:dust_obsidian", 1)], &[]], &[&[("mekanism:dust_refined_obsidian", 1)]], &[&[], &[("mekanism:diamond", 10)]], &[&[]]),
        (4033, &[&[("mekanism:enriched_iron", 1)], &[]], &[&[("mekanism:dust_steel", 1)]], &[&[], &[("mekanism:carbon", 10)]], &[&[]]),
        (4036, &[&[("mekanism:dust_steel", 1)]], &[&[("mekanism:ingot_steel", 1)]], &[&[]], &[&[]]),
        (4038, &[&[("mekanism:ingot_steel", 1)]], &[&[("mekanism:dust_steel", 1)]], &[&[]], &[&[]]),
        (4044, &[&[("mekanism:clump_tin", 1)]], &[&[("mekanism:dirty_dust_tin", 1)]], &[&[]], &[&[]]),
        (4045, &[&[("mekanism:dirty_dust_tin", 1)]], &[&[("mekanism:dust_tin", 1)]], &[&[]], &[&[]]),
        (4046, &[&[("mekanism:ingot_tin", 1)]], &[&[("mekanism:dust_tin", 1)]], &[&[]], &[&[]]),
        (4047, &[&[("mekanism:tin_ore", 1), ("mekanism:deepslate_tin_ore", 1)]], &[&[("mekanism:dust_tin", 2)]], &[&[]], &[&[]]),
        (4048, &[&[("mekanism:block_raw_tin", 1)]], &[&[("mekanism:dust_tin", 12)]], &[&[]], &[&[]]),
        (4049, &[&[("mekanism:raw_tin", 3)]], &[&[("mekanism:dust_tin", 4)]], &[&[]], &[&[]]),
        (4052, &[&[("mekanism:deepslate_tin_ore", 1)]], &[&[("mekanism:ingot_tin", 1)]], &[&[]], &[&[]]),
        (4054, &[&[("mekanism:dust_tin", 1)]], &[&[("mekanism:ingot_tin", 1)]], &[&[]], &[&[]]),
        (4057, &[&[("mekanism:tin_ore", 1)]], &[&[("mekanism:ingot_tin", 1)]], &[&[]], &[&[]]),
        (4059, &[&[("mekanism:raw_tin", 1)]], &[&[("mekanism:ingot_tin", 1)]], &[&[]], &[&[]]),
        (4079, &[&[("mekanism:clump_uranium", 1)]], &[&[("mekanism:dirty_dust_uranium", 1)]], &[&[]], &[&[]]),
        (4080, &[&[("mekanism:dirty_dust_uranium", 1)]], &[&[("mekanism:dust_uranium", 1)]], &[&[]], &[&[]]),
        (4081, &[&[("mekanism:ingot_uranium", 1)]], &[&[("mekanism:dust_uranium", 1)]], &[&[]], &[&[]]),
        (4082, &[&[("mekanism:uranium_ore", 1), ("mekanism:deepslate_uranium_ore", 1)]], &[&[("mekanism:dust_uranium", 2)]], &[&[]], &[&[]]),
        (4083, &[&[("mekanism:block_raw_uranium", 1)]], &[&[("mekanism:dust_uranium", 12)]], &[&[]], &[&[]]),
        (4084, &[&[("mekanism:raw_uranium", 3)]], &[&[("mekanism:dust_uranium", 4)]], &[&[]], &[&[]]),
        (4090, &[&[("mekanism:deepslate_uranium_ore", 1)]], &[&[("mekanism:ingot_uranium", 1)]], &[&[]], &[&[]]),
        (4092, &[&[("mekanism:dust_uranium", 1)]], &[&[("mekanism:ingot_uranium", 1)]], &[&[]], &[&[]]),
        (4095, &[&[("mekanism:uranium_ore", 1)]], &[&[("mekanism:ingot_uranium", 1)]], &[&[]], &[&[]]),
        (4097, &[&[("mekanism:raw_uranium", 1)]], &[&[("mekanism:ingot_uranium", 1)]], &[&[]], &[&[]]),
        (4116, &[&[("mekanism:ingot_uranium", 1)]], &[&[("mekanism:yellow_cake_uranium", 2)]], &[&[]], &[&[]]),
        (4132, &[&[("minecraft:purple_terracotta", 1)]], &[&[("minecraft:purple_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (4158, &[&[("minecraft:nether_quartz_ore", 1)]], &[&[("minecraft:quartz", 1)]], &[&[]], &[&[]]),
        (4217, &[&[("minecraft:red_terracotta", 1)]], &[&[("minecraft:red_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (4244, &[&[("minecraft:deepslate_redstone_ore", 1)]], &[&[("minecraft:redstone", 1)]], &[&[]], &[&[]]),
        (4245, &[&[("minecraft:redstone_ore", 1)]], &[&[("minecraft:redstone", 1)]], &[&[]], &[&[]]),
        (4255, &[&[("mekanismtools:refined_glowstone_helmet", 1), ("mekanismtools:refined_glowstone_chestplate", 1), ("mekanismtools:refined_glowstone_leggings", 1), ("mekanismtools:refined_glowstone_boots", 1), ("mekanismtools:refined_glowstone_horse_armor", 1), ("mekanismtools:refined_glowstone_nautilus_armor", 1), ("mekanismtools:refined_glowstone_axe", 1), ("mekanismtools:refined_glowstone_hoe", 1), ("mekanismtools:refined_glowstone_paxel", 1), ("mekanismtools:refined_glowstone_pickaxe", 1), ("mekanismtools:refined_glowstone_shield", 1), ("mekanismtools:refined_glowstone_shovel", 1), ("mekanismtools:refined_glowstone_sword", 1), ("mekanismtools:refined_glowstone_spear", 1)]], &[&[("mekanism:nugget_refined_glowstone", 1)]], &[&[]], &[&[]]),
        (4271, &[&[("mekanismtools:refined_obsidian_helmet", 1), ("mekanismtools:refined_obsidian_chestplate", 1), ("mekanismtools:refined_obsidian_leggings", 1), ("mekanismtools:refined_obsidian_boots", 1), ("mekanismtools:refined_obsidian_horse_armor", 1), ("mekanismtools:refined_obsidian_nautilus_armor", 1), ("mekanismtools:refined_obsidian_axe", 1), ("mekanismtools:refined_obsidian_hoe", 1), ("mekanismtools:refined_obsidian_paxel", 1), ("mekanismtools:refined_obsidian_pickaxe", 1), ("mekanismtools:refined_obsidian_shield", 1), ("mekanismtools:refined_obsidian_shovel", 1), ("mekanismtools:refined_obsidian_sword", 1), ("mekanismtools:refined_obsidian_spear", 1)]], &[&[("mekanism:nugget_refined_obsidian", 1)]], &[&[]], &[&[]]),
        (4282, &[&[("minecraft:resin_clump", 1)]], &[&[("minecraft:resin_brick", 1)]], &[&[]], &[&[]]),
        (4530, &[&[("minecraft:basalt", 1)]], &[&[("minecraft:smooth_basalt", 1)]], &[&[]], &[&[]]),
        (4531, &[&[("minecraft:quartz_block", 1)]], &[&[("minecraft:smooth_quartz", 1)]], &[&[]], &[&[]]),
        (4536, &[&[("minecraft:red_sandstone", 1)]], &[&[("minecraft:smooth_red_sandstone", 1)]], &[&[]], &[&[]]),
        (4541, &[&[("minecraft:sandstone", 1)]], &[&[("minecraft:smooth_sandstone", 1)]], &[&[]], &[&[]]),
        (4546, &[&[("minecraft:stone", 1)]], &[&[("minecraft:smooth_stone", 1)]], &[&[]], &[&[]]),
        (4563, &[&[("minecraft:wet_sponge", 1)]], &[&[("minecraft:sponge", 1)]], &[&[]], &[&[]]),
        (4589, &[&[("mekanismtools:steel_helmet", 1), ("mekanismtools:steel_chestplate", 1), ("mekanismtools:steel_leggings", 1), ("mekanismtools:steel_boots", 1), ("mekanismtools:steel_horse_armor", 1), ("mekanismtools:steel_nautilus_armor", 1), ("mekanismtools:steel_axe", 1), ("mekanismtools:steel_hoe", 1), ("mekanismtools:steel_paxel", 1), ("mekanismtools:steel_pickaxe", 1), ("mekanismtools:steel_shield", 1), ("mekanismtools:steel_shovel", 1), ("mekanismtools:steel_sword", 1), ("mekanismtools:steel_spear", 1)]], &[&[("mekanism:nugget_steel", 1)]], &[&[]], &[&[]]),
        (4602, &[&[("minecraft:cobblestone", 1)]], &[&[("minecraft:stone", 1)]], &[&[]], &[&[]]),
        (4699, &[&[("minecraft:clay", 1)]], &[&[("minecraft:terracotta", 1)]], &[&[]], &[&[]]),
        (4940, &[&[("minecraft:white_terracotta", 1)]], &[&[("minecraft:white_glazed_terracotta", 1)]], &[&[]], &[&[]]),
        (4978, &[&[("minecraft:yellow_terracotta", 1)]], &[&[("minecraft:yellow_glazed_terracotta", 1)]], &[&[]], &[&[]]),
    ];
    /// Todas as receitas do jogo que têm display, na ordem do `RecipeManager` do original (`rastros/recipe_book_known_displays.jsonl`): `(receita, id do primeiro display, quantos)`. O `ClientRecipeBook` guarda os displays de todas as que o jogador conhece, e a tabela dele decide a ordem das coleções do livro.
    pub const DISPLAYS: &[machinery_basis_core::recipe_book_data::DisplayRow] = &[
        ("minecraft:acacia_boat", 0, 1),
        ("minecraft:acacia_button", 1, 1),
        ("minecraft:acacia_chest_boat", 2, 1),
        ("minecraft:acacia_door", 3, 1),
        ("minecraft:acacia_fence", 4, 1),
        ("minecraft:acacia_fence_gate", 5, 1),
        ("minecraft:acacia_hanging_sign", 6, 1),
        ("minecraft:acacia_planks", 7, 1),
        ("minecraft:acacia_pressure_plate", 8, 1),
        ("minecraft:acacia_shelf", 9, 1),
        ("minecraft:acacia_sign", 10, 1),
        ("minecraft:acacia_slab", 11, 1),
        ("minecraft:acacia_stairs", 12, 1),
        ("minecraft:acacia_trapdoor", 13, 1),
        ("minecraft:acacia_wood", 14, 1),
        ("mekanismgenerators:activating/tritium", 15, 1),
        ("minecraft:activator_rail", 16, 1),
        ("minecraft:amethyst_block", 17, 1),
        ("minecraft:andesite", 18, 1),
        ("minecraft:andesite_slab", 19, 1),
        ("minecraft:andesite_slab_from_andesite_stonecutting", 20, 1),
        ("minecraft:andesite_stairs", 21, 1),
        ("minecraft:andesite_stairs_from_andesite_stonecutting", 22, 1),
        ("minecraft:andesite_wall", 23, 1),
        ("minecraft:andesite_wall_from_andesite_stonecutting", 24, 1),
        ("mekanism:antiprotonic_nucleosynthesizer", 25, 1),
        ("minecraft:anvil", 26, 1),
        ("minecraft:armor_stand", 27, 1),
        ("minecraft:arrow", 28, 1),
        ("mekanism:atomic_disassembler", 29, 1),
        ("mekanism:attack_amplification_unit", 30, 1),
        ("minecraft:baked_potato", 31, 1),
        ("minecraft:baked_potato_from_campfire_cooking", 32, 1),
        ("minecraft:baked_potato_from_smoking", 33, 1),
        ("mekanismadditions:balloon/black", 34, 1),
        ("mekanismadditions:balloon/blue", 35, 1),
        ("mekanismadditions:balloon/brown", 36, 1),
        ("mekanismadditions:balloon/cyan", 37, 1),
        ("mekanismadditions:balloon/gray", 38, 1),
        ("mekanismadditions:balloon/green", 39, 1),
        ("mekanismadditions:balloon/light_blue", 40, 1),
        ("mekanismadditions:balloon/light_gray", 41, 1),
        ("mekanismadditions:balloon/lime", 42, 1),
        ("mekanismadditions:balloon/magenta", 43, 1),
        ("mekanismadditions:balloon/orange", 44, 1),
        ("mekanismadditions:balloon/pink", 45, 1),
        ("mekanismadditions:balloon/purple", 46, 1),
        ("mekanismadditions:balloon/recolor/black", 47, 1),
        ("mekanismadditions:balloon/recolor/blue", 48, 1),
        ("mekanismadditions:balloon/recolor/brown", 49, 1),
        ("mekanismadditions:balloon/recolor/cyan", 50, 1),
        ("mekanismadditions:balloon/recolor/gray", 51, 1),
        ("mekanismadditions:balloon/recolor/green", 52, 1),
        ("mekanismadditions:balloon/recolor/light_blue", 53, 1),
        ("mekanismadditions:balloon/recolor/light_gray", 54, 1),
        ("mekanismadditions:balloon/recolor/lime", 55, 1),
        ("mekanismadditions:balloon/recolor/magenta", 56, 1),
        ("mekanismadditions:balloon/recolor/orange", 57, 1),
        ("mekanismadditions:balloon/recolor/painting/aqua", 58, 1),
        ("mekanismadditions:balloon/recolor/painting/black", 59, 1),
        ("mekanismadditions:balloon/recolor/painting/blue", 60, 1),
        ("mekanismadditions:balloon/recolor/painting/brown", 61, 1),
        ("mekanismadditions:balloon/recolor/painting/cyan", 62, 1),
        ("mekanismadditions:balloon/recolor/painting/dark_red", 63, 1),
        ("mekanismadditions:balloon/recolor/painting/gray", 64, 1),
        ("mekanismadditions:balloon/recolor/painting/green", 65, 1),
        ("mekanismadditions:balloon/recolor/painting/light_blue", 66, 1),
        ("mekanismadditions:balloon/recolor/painting/light_gray", 67, 1),
        ("mekanismadditions:balloon/recolor/painting/lime", 68, 1),
        ("mekanismadditions:balloon/recolor/painting/magenta", 69, 1),
        ("mekanismadditions:balloon/recolor/painting/orange", 70, 1),
        ("mekanismadditions:balloon/recolor/painting/pink", 71, 1),
        ("mekanismadditions:balloon/recolor/painting/purple", 72, 1),
        ("mekanismadditions:balloon/recolor/painting/red", 73, 1),
        ("mekanismadditions:balloon/recolor/painting/white", 74, 1),
        ("mekanismadditions:balloon/recolor/painting/yellow", 75, 1),
        ("mekanismadditions:balloon/recolor/pink", 76, 1),
        ("mekanismadditions:balloon/recolor/purple", 77, 1),
        ("mekanismadditions:balloon/recolor/red", 78, 1),
        ("mekanismadditions:balloon/recolor/white", 79, 1),
        ("mekanismadditions:balloon/recolor/yellow", 80, 1),
        ("mekanismadditions:balloon/red", 81, 1),
        ("mekanismadditions:balloon/white", 82, 1),
        ("mekanismadditions:balloon/yellow", 83, 1),
        ("minecraft:bamboo_block", 84, 1),
        ("minecraft:bamboo_button", 85, 1),
        ("minecraft:bamboo_chest_raft", 86, 1),
        ("minecraft:bamboo_door", 87, 1),
        ("minecraft:bamboo_fence", 88, 1),
        ("minecraft:bamboo_fence_gate", 89, 1),
        ("minecraft:bamboo_hanging_sign", 90, 1),
        ("minecraft:bamboo_mosaic", 91, 1),
        ("minecraft:bamboo_mosaic_slab", 92, 1),
        ("minecraft:bamboo_mosaic_stairs", 93, 1),
        ("minecraft:bamboo_planks", 94, 1),
        ("minecraft:bamboo_pressure_plate", 95, 1),
        ("minecraft:bamboo_raft", 96, 1),
        ("minecraft:bamboo_shelf", 97, 1),
        ("minecraft:bamboo_sign", 98, 1),
        ("minecraft:bamboo_slab", 99, 1),
        ("minecraft:bamboo_stairs", 100, 1),
        ("minecraft:bamboo_trapdoor", 101, 1),
        ("minecraft:barrel", 102, 1),
        ("minecraft:beacon", 103, 1),
        ("minecraft:beehive", 104, 1),
        ("minecraft:beetroot_soup", 105, 1),
        ("mekanism:bin/advanced", 106, 1),
        ("mekanism:bin/basic", 107, 1),
        ("mekanism:bin/elite", 108, 1),
        ("mekanism:bin/ultimate", 109, 1),
        ("mekanism:bio_fuel", 110, 1),
        ("minecraft:birch_boat", 111, 1),
        ("minecraft:birch_button", 112, 1),
        ("minecraft:birch_chest_boat", 113, 1),
        ("minecraft:birch_door", 114, 1),
        ("minecraft:birch_fence", 115, 1),
        ("minecraft:birch_fence_gate", 116, 1),
        ("minecraft:birch_hanging_sign", 117, 1),
        ("minecraft:birch_planks", 118, 1),
        ("minecraft:birch_pressure_plate", 119, 1),
        ("minecraft:birch_shelf", 120, 1),
        ("minecraft:birch_sign", 121, 1),
        ("minecraft:birch_slab", 122, 1),
        ("minecraft:birch_stairs", 123, 1),
        ("minecraft:birch_trapdoor", 124, 1),
        ("minecraft:birch_wood", 125, 1),
        ("minecraft:black_banner", 126, 1),
        ("minecraft:black_bed", 127, 1),
        ("minecraft:black_bundle", 128, 1),
        ("minecraft:black_candle", 129, 1),
        ("minecraft:black_carpet", 130, 1),
        ("minecraft:black_concrete_powder", 131, 1),
        ("minecraft:black_concrete_slab", 132, 1),
        ("minecraft:black_concrete_slab_from_black_concrete_stonecutting", 133, 1),
        ("minecraft:black_concrete_stairs", 134, 1),
        ("minecraft:black_concrete_stairs_from_black_concrete_stonecutting", 135, 1),
        ("minecraft:black_cushion", 136, 1),
        ("minecraft:black_dye", 137, 1),
        ("minecraft:black_dye_from_wither_rose", 138, 1),
        ("minecraft:black_glazed_terracotta", 139, 1),
        ("minecraft:black_harness", 140, 1),
        ("minecraft:black_shulker_box", 141, 1),
        ("minecraft:black_stained_glass", 142, 1),
        ("minecraft:black_stained_glass_pane", 143, 1),
        ("minecraft:black_stained_glass_pane_from_glass_pane", 144, 1),
        ("minecraft:black_terracotta", 145, 1),
        ("minecraft:black_wool_slab", 146, 1),
        ("minecraft:black_wool_stairs", 147, 1),
        ("minecraft:blackstone_slab", 148, 1),
        ("minecraft:blackstone_slab_from_blackstone_stonecutting", 149, 1),
        ("minecraft:blackstone_stairs", 150, 1),
        ("minecraft:blackstone_stairs_from_blackstone_stonecutting", 151, 1),
        ("minecraft:blackstone_wall", 152, 1),
        ("minecraft:blackstone_wall_from_blackstone_stonecutting", 153, 1),
        ("minecraft:blast_furnace", 154, 1),
        ("mekanism:blasting_unit", 155, 1),
        ("minecraft:blaze_powder", 156, 1),
        ("minecraft:blue_banner", 157, 1),
        ("minecraft:blue_bed", 158, 1),
        ("minecraft:blue_bundle", 159, 1),
        ("minecraft:blue_candle", 160, 1),
        ("minecraft:blue_carpet", 161, 1),
        ("minecraft:blue_concrete_powder", 162, 1),
        ("minecraft:blue_concrete_slab", 163, 1),
        ("minecraft:blue_concrete_slab_from_blue_concrete_stonecutting", 164, 1),
        ("minecraft:blue_concrete_stairs", 165, 1),
        ("minecraft:blue_concrete_stairs_from_blue_concrete_stonecutting", 166, 1),
        ("minecraft:blue_cushion", 167, 1),
        ("minecraft:blue_dye", 168, 1),
        ("minecraft:blue_dye_from_cornflower", 169, 1),
        ("minecraft:blue_glazed_terracotta", 170, 1),
        ("minecraft:blue_harness", 171, 1),
        ("minecraft:blue_ice", 172, 1),
        ("minecraft:blue_shulker_box", 173, 1),
        ("minecraft:blue_stained_glass", 174, 1),
        ("minecraft:blue_stained_glass_pane", 175, 1),
        ("minecraft:blue_stained_glass_pane_from_glass_pane", 176, 1),
        ("minecraft:blue_terracotta", 177, 1),
        ("minecraft:blue_wool_slab", 178, 1),
        ("minecraft:blue_wool_stairs", 179, 1),
        ("mekanism:boiler_casing", 180, 1),
        ("mekanism:boiler_valve", 181, 1),
        ("minecraft:bolt_armor_trim_smithing_template", 182, 1),
        ("minecraft:bolt_armor_trim_smithing_template_smithing_trim", 183, 1),
        ("minecraft:bone_block", 184, 1),
        ("minecraft:bone_meal", 185, 1),
        ("minecraft:bone_meal_from_bone_block", 186, 1),
        ("minecraft:book", 187, 1),
        ("minecraft:bookshelf", 188, 1),
        ("minecraft:bordure_indented_banner_pattern", 189, 1),
        ("minecraft:bow", 190, 1),
        ("minecraft:bowl", 191, 1),
        ("minecraft:bread", 192, 1),
        ("minecraft:brewing_stand", 193, 1),
        ("minecraft:brick", 194, 1),
        ("minecraft:brick_slab", 195, 1),
        ("minecraft:brick_slab_from_bricks_stonecutting", 196, 1),
        ("minecraft:brick_stairs", 197, 1),
        ("minecraft:brick_stairs_from_bricks_stonecutting", 198, 1),
        ("minecraft:brick_wall", 199, 1),
        ("minecraft:brick_wall_from_bricks_stonecutting", 200, 1),
        ("minecraft:bricks", 201, 1),
        ("mekanismtools:bronze/armor/boots", 202, 1),
        ("mekanismtools:bronze/armor/chestplate", 203, 1),
        ("mekanismtools:bronze/armor/helmet", 204, 1),
        ("mekanismtools:bronze/armor/horse", 205, 1),
        ("mekanismtools:bronze/armor/leggings", 206, 1),
        ("mekanismtools:bronze/armor/nautilus", 207, 1),
        ("mekanismtools:bronze/nugget_from_blasting", 208, 1),
        ("mekanismtools:bronze/nugget_from_smelting", 209, 1),
        ("mekanismtools:bronze/shield", 210, 1),
        ("mekanismtools:bronze/tools/axe", 211, 1),
        ("mekanismtools:bronze/tools/hoe", 212, 1),
        ("mekanismtools:bronze/tools/paxel", 213, 1),
        ("mekanismtools:bronze/tools/pickaxe", 214, 1),
        ("mekanismtools:bronze/tools/shovel", 215, 1),
        ("mekanismtools:bronze/tools/spear", 216, 1),
        ("mekanismtools:bronze/tools/sword", 217, 1),
        ("minecraft:brown_banner", 218, 1),
        ("minecraft:brown_bed", 219, 1),
        ("minecraft:brown_bundle", 220, 1),
        ("minecraft:brown_candle", 221, 1),
        ("minecraft:brown_carpet", 222, 1),
        ("minecraft:brown_concrete_powder", 223, 1),
        ("minecraft:brown_concrete_slab", 224, 1),
        ("minecraft:brown_concrete_slab_from_brown_concrete_stonecutting", 225, 1),
        ("minecraft:brown_concrete_stairs", 226, 1),
        ("minecraft:brown_concrete_stairs_from_brown_concrete_stonecutting", 227, 1),
        ("minecraft:brown_cushion", 228, 1),
        ("minecraft:brown_dye", 229, 1),
        ("minecraft:brown_glazed_terracotta", 230, 1),
        ("minecraft:brown_harness", 231, 1),
        ("minecraft:brown_shulker_box", 232, 1),
        ("minecraft:brown_stained_glass", 233, 1),
        ("minecraft:brown_stained_glass_pane", 234, 1),
        ("minecraft:brown_stained_glass_pane_from_glass_pane", 235, 1),
        ("minecraft:brown_terracotta", 236, 1),
        ("minecraft:brown_wool_slab", 237, 1),
        ("minecraft:brown_wool_stairs", 238, 1),
        ("minecraft:brush", 239, 1),
        ("minecraft:bucket", 240, 1),
        ("minecraft:bundle", 241, 1),
        ("minecraft:cake", 242, 1),
        ("minecraft:calibrated_sculk_sensor", 243, 1),
        ("minecraft:campfire", 244, 1),
        ("minecraft:candle", 245, 1),
        ("mekanism:canteen", 246, 1),
        ("mekanism:cardboard_box", 247, 1),
        ("minecraft:carrot_on_a_stick", 248, 1),
        ("minecraft:cartography_table", 249, 1),
        ("minecraft:cauldron", 250, 1),
        ("mekanism:charcoal", 251, 1),
        ("minecraft:charcoal", 252, 1),
        ("mekanism:charge_distribution_unit", 253, 1),
        ("mekanism:chargepad", 254, 1),
        ("mekanism:chemical_conversion/bio/from_bio_fuel", 255, 1),
        ("mekanism:chemical_conversion/bio/from_bio_fuel_block", 256, 1),
        ("mekanism:chemical_conversion/carbon/from_charcoal", 257, 1),
        ("mekanism:chemical_conversion/carbon/from_charcoal_block", 258, 1),
        ("mekanism:chemical_conversion/carbon/from_coal", 259, 1),
        ("mekanism:chemical_conversion/carbon/from_coal_block", 260, 1),
        ("mekanism:chemical_conversion/carbon/from_enriched", 261, 1),
        ("mekanism:chemical_conversion/diamond/from_dust", 262, 1),
        ("mekanism:chemical_conversion/diamond/from_enriched", 263, 1),
        ("mekanism:chemical_conversion/flint_to_oxygen", 264, 1),
        ("mekanism:chemical_conversion/fungi/from_mushrooms", 265, 1),
        ("mekanism:chemical_conversion/gold/from_dust", 266, 1),
        ("mekanism:chemical_conversion/gold/from_enriched", 267, 1),
        ("mekanism:chemical_conversion/osmium_from_block", 268, 1),
        ("mekanism:chemical_conversion/osmium_from_ingot", 269, 1),
        ("mekanism:chemical_conversion/redstone/from_block", 270, 1),
        ("mekanism:chemical_conversion/redstone/from_dust", 271, 1),
        ("mekanism:chemical_conversion/redstone/from_enriched", 272, 1),
        ("mekanism:chemical_conversion/refined_obsidian/from_dust", 273, 1),
        ("mekanism:chemical_conversion/refined_obsidian/from_enriched", 274, 1),
        ("mekanism:chemical_conversion/salt_to_hydrogen_chloride", 275, 1),
        ("mekanism:chemical_conversion/sulfur_to_sulfuric_acid", 276, 1),
        ("mekanism:chemical_conversion/tin/from_dust", 277, 1),
        ("mekanism:chemical_conversion/tin/from_enriched", 278, 1),
        ("mekanism:chemical_crystallizer", 279, 1),
        ("mekanism:chemical_dissolution_chamber", 280, 1),
        ("mekanism:chemical_infuser", 281, 1),
        ("mekanismgenerators:chemical_infusing/fusion_fuel", 282, 1),
        ("mekanism:chemical_infusing/hydrogen_chloride", 283, 1),
        ("mekanism:chemical_infusing/sulfur_trioxide", 284, 1),
        ("mekanism:chemical_infusing/sulfuric_acid", 285, 1),
        ("mekanism:chemical_injection_chamber", 286, 1),
        ("mekanism:chemical_oxidizer", 287, 1),
        ("mekanism:chemical_tank/advanced", 288, 1),
        ("mekanism:chemical_tank/basic", 289, 1),
        ("mekanism:chemical_tank/elite", 290, 1),
        ("mekanism:chemical_tank/ultimate", 291, 1),
        ("mekanism:chemical_washer", 292, 1),
        ("minecraft:cherry_boat", 293, 1),
        ("minecraft:cherry_button", 294, 1),
        ("minecraft:cherry_chest_boat", 295, 1),
        ("minecraft:cherry_door", 296, 1),
        ("minecraft:cherry_fence", 297, 1),
        ("minecraft:cherry_fence_gate", 298, 1),
        ("minecraft:cherry_hanging_sign", 299, 1),
        ("minecraft:cherry_planks", 300, 1),
        ("minecraft:cherry_pressure_plate", 301, 1),
        ("minecraft:cherry_shelf", 302, 1),
        ("minecraft:cherry_sign", 303, 1),
        ("minecraft:cherry_slab", 304, 1),
        ("minecraft:cherry_stairs", 305, 1),
        ("minecraft:cherry_trapdoor", 306, 1),
        ("minecraft:cherry_wood", 307, 1),
        ("minecraft:chest", 308, 1),
        ("minecraft:chest_minecart", 309, 1),
        ("minecraft:chiseled_bookshelf", 310, 1),
        ("minecraft:chiseled_cinnabar", 311, 1),
        ("minecraft:chiseled_cinnabar_from_cinnabar_stonecutting", 312, 1),
        ("minecraft:chiseled_copper", 313, 1),
        ("minecraft:chiseled_copper_from_copper_block_stonecutting", 314, 1),
        ("minecraft:chiseled_copper_from_cut_copper_stonecutting", 315, 1),
        ("minecraft:chiseled_deepslate", 316, 1),
        ("minecraft:chiseled_deepslate_from_cobbled_deepslate_stonecutting", 317, 1),
        ("minecraft:chiseled_deepslate_from_deepslate_stonecutting", 318, 1),
        ("minecraft:chiseled_nether_bricks", 319, 1),
        ("minecraft:chiseled_nether_bricks_from_nether_bricks_stonecutting", 320, 1),
        ("minecraft:chiseled_polished_blackstone", 321, 1),
        ("minecraft:chiseled_polished_blackstone_from_blackstone_stonecutting", 322, 1),
        ("minecraft:chiseled_polished_blackstone_from_polished_blackstone_stonecutting", 323, 1),
        ("minecraft:chiseled_quartz_block", 324, 1),
        ("minecraft:chiseled_quartz_block_from_quartz_block_stonecutting", 325, 1),
        ("minecraft:chiseled_red_sandstone", 326, 1),
        ("minecraft:chiseled_red_sandstone_from_red_sandstone_stonecutting", 327, 1),
        ("minecraft:chiseled_resin_bricks", 328, 1),
        ("minecraft:chiseled_resin_bricks_from_resin_bricks_stonecutting", 329, 1),
        ("minecraft:chiseled_sandstone", 330, 1),
        ("minecraft:chiseled_sandstone_from_sandstone_stonecutting", 331, 1),
        ("minecraft:chiseled_stone_bricks", 332, 1),
        ("minecraft:chiseled_stone_bricks_from_stone_bricks_stonecutting", 333, 1),
        ("minecraft:chiseled_stone_bricks_from_stone_stonecutting", 334, 1),
        ("minecraft:chiseled_sulfur", 335, 1),
        ("minecraft:chiseled_sulfur_from_sulfur_stonecutting", 336, 1),
        ("minecraft:chiseled_tuff", 337, 1),
        ("minecraft:chiseled_tuff_bricks", 338, 1),
        ("minecraft:chiseled_tuff_bricks_from_polished_tuff_stonecutting", 339, 1),
        ("minecraft:chiseled_tuff_bricks_from_tuff_bricks_stonecutting", 340, 1),
        ("minecraft:chiseled_tuff_bricks_from_tuff_stonecutting", 341, 1),
        ("minecraft:chiseled_tuff_from_tuff_stonecutting", 342, 1),
        ("minecraft:cinnabar_brick_slab", 343, 1),
        ("minecraft:cinnabar_brick_slab_from_cinnabar_bricks_stonecutting", 344, 1),
        ("minecraft:cinnabar_brick_slab_from_cinnabar_stonecutting", 345, 1),
        ("minecraft:cinnabar_brick_slab_from_polished_cinnabar_stonecutting", 346, 1),
        ("minecraft:cinnabar_brick_stairs", 347, 1),
        ("minecraft:cinnabar_brick_stairs_from_cinnabar_bricks_stonecutting", 348, 1),
        ("minecraft:cinnabar_brick_stairs_from_cinnabar_stonecutting", 349, 1),
        ("minecraft:cinnabar_brick_stairs_from_polished_cinnabar_stonecutting", 350, 1),
        ("minecraft:cinnabar_brick_wall", 351, 1),
        ("minecraft:cinnabar_brick_wall_from_cinnabar_bricks_stonecutting", 352, 1),
        ("minecraft:cinnabar_brick_wall_from_cinnabar_stonecutting", 353, 1),
        ("minecraft:cinnabar_brick_wall_from_polished_cinnabar_stonecutting", 354, 1),
        ("minecraft:cinnabar_bricks", 355, 1),
        ("minecraft:cinnabar_bricks_from_cinnabar_stonecutting", 356, 1),
        ("minecraft:cinnabar_bricks_from_polished_cinnabar_stonecutting", 357, 1),
        ("minecraft:cinnabar_slab", 358, 1),
        ("minecraft:cinnabar_slab_from_cinnabar_stonecutting", 359, 1),
        ("minecraft:cinnabar_stairs", 360, 1),
        ("minecraft:cinnabar_stairs_from_cinnabar_stonecutting", 361, 1),
        ("minecraft:cinnabar_wall", 362, 1),
        ("minecraft:cinnabar_wall_from_cinnabar_stonecutting", 363, 1),
        ("minecraft:clay", 364, 1),
        ("minecraft:clock", 365, 1),
        ("minecraft:coal", 366, 1),
        ("minecraft:coal_block", 367, 1),
        ("minecraft:coal_from_blasting_coal_ore", 368, 1),
        ("minecraft:coal_from_blasting_deepslate_coal_ore", 369, 1),
        ("minecraft:coal_from_smelting_coal_ore", 370, 1),
        ("minecraft:coal_from_smelting_deepslate_coal_ore", 371, 1),
        ("minecraft:coarse_dirt", 372, 1),
        ("minecraft:coast_armor_trim_smithing_template", 373, 1),
        ("minecraft:coast_armor_trim_smithing_template_smithing_trim", 374, 1),
        ("minecraft:cobbled_deepslate_from_deepslate_stonecutting", 375, 1),
        ("minecraft:cobbled_deepslate_slab", 376, 1),
        ("minecraft:cobbled_deepslate_slab_from_cobbled_deepslate_stonecutting", 377, 1),
        ("minecraft:cobbled_deepslate_slab_from_deepslate_stonecutting", 378, 1),
        ("minecraft:cobbled_deepslate_stairs", 379, 1),
        ("minecraft:cobbled_deepslate_stairs_from_cobbled_deepslate_stonecutting", 380, 1),
        ("minecraft:cobbled_deepslate_stairs_from_deepslate_stonecutting", 381, 1),
        ("minecraft:cobbled_deepslate_wall", 382, 1),
        ("minecraft:cobbled_deepslate_wall_from_cobbled_deepslate_stonecutting", 383, 1),
        ("minecraft:cobbled_deepslate_wall_from_deepslate_stonecutting", 384, 1),
        ("minecraft:cobblestone_from_stone_stonecutting", 385, 1),
        ("minecraft:cobblestone_slab", 386, 1),
        ("minecraft:cobblestone_slab_from_cobblestone_stonecutting", 387, 1),
        ("minecraft:cobblestone_slab_from_stone_stonecutting", 388, 1),
        ("minecraft:cobblestone_stairs", 389, 1),
        ("minecraft:cobblestone_stairs_from_cobblestone_stonecutting", 390, 1),
        ("minecraft:cobblestone_stairs_from_stone_stonecutting", 391, 1),
        ("minecraft:cobblestone_wall", 392, 1),
        ("minecraft:cobblestone_wall_from_cobblestone_stonecutting", 393, 1),
        ("minecraft:cobblestone_wall_from_stone_stonecutting", 394, 1),
        ("mekanism:color_modulation_unit", 395, 1),
        ("mekanism:combiner", 396, 1),
        ("mekanism:combining/dye/black_to_light_gray", 397, 1),
        ("mekanism:combining/dye/cyan", 398, 1),
        ("mekanism:combining/dye/gray_to_light_gray", 399, 1),
        ("mekanism:combining/dye/light_blue", 400, 1),
        ("mekanism:combining/dye/lime", 401, 1),
        ("mekanism:combining/dye/magenta", 402, 1),
        ("mekanism:combining/dye/orange", 403, 1),
        ("mekanism:combining/dye/pink", 404, 1),
        ("mekanism:combining/dye/purple", 405, 1),
        ("mekanism:combining/glow/berries", 406, 1),
        ("mekanism:combining/glow/ink_sac", 407, 1),
        ("mekanism:combining/glow/item_frame", 408, 1),
        ("mekanism:combining/gravel", 409, 1),
        ("mekanism:combining/muddy_mangrove_roots", 410, 1),
        ("mekanism:combining/obsidian", 411, 1),
        ("mekanism:combining/packed_mud", 412, 1),
        ("mekanism:combining/rooted_dirt", 413, 1),
        ("mekanism:combining/wax/waxed_chiseled_copper", 414, 1),
        ("mekanism:combining/wax/waxed_copper_bars", 415, 1),
        ("mekanism:combining/wax/waxed_copper_block", 416, 1),
        ("mekanism:combining/wax/waxed_copper_bulb", 417, 1),
        ("mekanism:combining/wax/waxed_copper_chain", 418, 1),
        ("mekanism:combining/wax/waxed_copper_chest", 419, 1),
        ("mekanism:combining/wax/waxed_copper_door", 420, 1),
        ("mekanism:combining/wax/waxed_copper_golem_statue", 421, 1),
        ("mekanism:combining/wax/waxed_copper_grate", 422, 1),
        ("mekanism:combining/wax/waxed_copper_lantern", 423, 1),
        ("mekanism:combining/wax/waxed_copper_trapdoor", 424, 1),
        ("mekanism:combining/wax/waxed_cut_copper", 425, 1),
        ("mekanism:combining/wax/waxed_cut_copper_slab", 426, 1),
        ("mekanism:combining/wax/waxed_cut_copper_stairs", 427, 1),
        ("mekanism:combining/wax/waxed_exposed_chiseled_copper", 428, 1),
        ("mekanism:combining/wax/waxed_exposed_copper", 429, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_bars", 430, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_bulb", 431, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_chain", 432, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_chest", 433, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_door", 434, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_golem_statue", 435, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_grate", 436, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_lantern", 437, 1),
        ("mekanism:combining/wax/waxed_exposed_copper_trapdoor", 438, 1),
        ("mekanism:combining/wax/waxed_exposed_cut_copper", 439, 1),
        ("mekanism:combining/wax/waxed_exposed_cut_copper_slab", 440, 1),
        ("mekanism:combining/wax/waxed_exposed_cut_copper_stairs", 441, 1),
        ("mekanism:combining/wax/waxed_exposed_lightning_rod", 442, 1),
        ("mekanism:combining/wax/waxed_lightning_rod", 443, 1),
        ("mekanism:combining/wax/waxed_oxidized_chiseled_copper", 444, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper", 445, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_bars", 446, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_bulb", 447, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_chain", 448, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_chest", 449, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_door", 450, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_golem_statue", 451, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_grate", 452, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_lantern", 453, 1),
        ("mekanism:combining/wax/waxed_oxidized_copper_trapdoor", 454, 1),
        ("mekanism:combining/wax/waxed_oxidized_cut_copper", 455, 1),
        ("mekanism:combining/wax/waxed_oxidized_cut_copper_slab", 456, 1),
        ("mekanism:combining/wax/waxed_oxidized_cut_copper_stairs", 457, 1),
        ("mekanism:combining/wax/waxed_oxidized_lightning_rod", 458, 1),
        ("mekanism:combining/wax/waxed_weathered_chiseled_copper", 459, 1),
        ("mekanism:combining/wax/waxed_weathered_copper", 460, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_bars", 461, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_bulb", 462, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_chain", 463, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_chest", 464, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_door", 465, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_golem_statue", 466, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_grate", 467, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_lantern", 468, 1),
        ("mekanism:combining/wax/waxed_weathered_copper_trapdoor", 469, 1),
        ("mekanism:combining/wax/waxed_weathered_cut_copper", 470, 1),
        ("mekanism:combining/wax/waxed_weathered_cut_copper_slab", 471, 1),
        ("mekanism:combining/wax/waxed_weathered_cut_copper_stairs", 472, 1),
        ("mekanism:combining/wax/waxed_weathered_lightning_rod", 473, 1),
        ("minecraft:comparator", 474, 1),
        ("minecraft:compass", 475, 1),
        ("minecraft:composter", 476, 1),
        ("minecraft:conduit", 477, 1),
        ("mekanism:configuration_card", 478, 1),
        ("mekanism:configurator", 479, 1),
        ("mekanism:control_circuit/advanced", 480, 1),
        ("mekanism:control_circuit/basic", 481, 1),
        ("mekanism:control_circuit/elite", 482, 1),
        ("mekanism:control_circuit/infused_advanced", 483, 1),
        ("mekanism:control_circuit/infused_elite", 484, 1),
        ("mekanism:control_circuit/infused_ultimate", 485, 1),
        ("mekanism:control_circuit/ultimate", 486, 1),
        ("minecraft:cooked_beef", 487, 1),
        ("minecraft:cooked_beef_from_campfire_cooking", 488, 1),
        ("minecraft:cooked_beef_from_smoking", 489, 1),
        ("minecraft:cooked_chicken", 490, 1),
        ("minecraft:cooked_chicken_from_campfire_cooking", 491, 1),
        ("minecraft:cooked_chicken_from_smoking", 492, 1),
        ("minecraft:cooked_cod", 493, 1),
        ("minecraft:cooked_cod_from_campfire_cooking", 494, 1),
        ("minecraft:cooked_cod_from_smoking", 495, 1),
        ("minecraft:cooked_mutton", 496, 1),
        ("minecraft:cooked_mutton_from_campfire_cooking", 497, 1),
        ("minecraft:cooked_mutton_from_smoking", 498, 1),
        ("minecraft:cooked_porkchop", 499, 1),
        ("minecraft:cooked_porkchop_from_campfire_cooking", 500, 1),
        ("minecraft:cooked_porkchop_from_smoking", 501, 1),
        ("minecraft:cooked_rabbit", 502, 1),
        ("minecraft:cooked_rabbit_from_campfire_cooking", 503, 1),
        ("minecraft:cooked_rabbit_from_smoking", 504, 1),
        ("minecraft:cooked_salmon", 505, 1),
        ("minecraft:cooked_salmon_from_campfire_cooking", 506, 1),
        ("minecraft:cooked_salmon_from_smoking", 507, 1),
        ("minecraft:cookie", 508, 1),
        ("minecraft:copper_axe", 509, 1),
        ("minecraft:copper_bars", 510, 1),
        ("minecraft:copper_block", 511, 1),
        ("minecraft:copper_boots", 512, 1),
        ("minecraft:copper_bulb", 513, 1),
        ("minecraft:copper_chain", 514, 1),
        ("minecraft:copper_chest", 515, 1),
        ("minecraft:copper_chestplate", 516, 1),
        ("minecraft:copper_door", 517, 1),
        ("minecraft:copper_grate", 518, 1),
        ("minecraft:copper_grate_from_copper_block_stonecutting", 519, 1),
        ("minecraft:copper_helmet", 520, 1),
        ("minecraft:copper_hoe", 521, 1),
        ("minecraft:copper_ingot", 522, 1),
        ("minecraft:copper_ingot_from_blasting_copper_ore", 523, 1),
        ("minecraft:copper_ingot_from_blasting_deepslate_copper_ore", 524, 1),
        ("minecraft:copper_ingot_from_blasting_raw_copper", 525, 1),
        ("minecraft:copper_ingot_from_nuggets", 526, 1),
        ("minecraft:copper_ingot_from_smelting_copper_ore", 527, 1),
        ("minecraft:copper_ingot_from_smelting_deepslate_copper_ore", 528, 1),
        ("minecraft:copper_ingot_from_smelting_raw_copper", 529, 1),
        ("minecraft:copper_ingot_from_waxed_copper_block", 530, 1),
        ("minecraft:copper_lantern", 531, 1),
        ("minecraft:copper_leggings", 532, 1),
        ("minecraft:copper_nugget", 533, 1),
        ("mekanismtools:copper_nugget_from_blasting", 534, 1),
        ("minecraft:copper_nugget_from_blasting", 535, 1),
        ("mekanismtools:copper_nugget_from_smelting", 536, 1),
        ("minecraft:copper_nugget_from_smelting", 537, 1),
        ("mekanismtools:copper_paxel", 538, 1),
        ("minecraft:copper_pickaxe", 539, 1),
        ("minecraft:copper_shovel", 540, 1),
        ("minecraft:copper_spear", 541, 1),
        ("minecraft:copper_sword", 542, 1),
        ("minecraft:copper_torch", 543, 1),
        ("minecraft:copper_trapdoor", 544, 1),
        ("minecraft:cracked_deepslate_bricks", 545, 1),
        ("minecraft:cracked_deepslate_tiles", 546, 1),
        ("minecraft:cracked_nether_bricks", 547, 1),
        ("minecraft:cracked_polished_blackstone_bricks", 548, 1),
        ("minecraft:cracked_stone_bricks", 549, 1),
        ("minecraft:crafter", 550, 1),
        ("mekanism:crafting_formula", 551, 1),
        ("minecraft:crafting_table", 552, 1),
        ("minecraft:creaking_heart", 553, 1),
        ("minecraft:creeper_banner_pattern", 554, 1),
        ("minecraft:crimson_button", 555, 1),
        ("minecraft:crimson_door", 556, 1),
        ("minecraft:crimson_fence", 557, 1),
        ("minecraft:crimson_fence_gate", 558, 1),
        ("minecraft:crimson_hanging_sign", 559, 1),
        ("minecraft:crimson_hyphae", 560, 1),
        ("minecraft:crimson_planks", 561, 1),
        ("minecraft:crimson_pressure_plate", 562, 1),
        ("minecraft:crimson_shelf", 563, 1),
        ("minecraft:crimson_sign", 564, 1),
        ("minecraft:crimson_slab", 565, 1),
        ("minecraft:crimson_stairs", 566, 1),
        ("minecraft:crimson_trapdoor", 567, 1),
        ("minecraft:crossbow", 568, 1),
        ("mekanism:crusher", 569, 1),
        ("mekanism:crushing/andesite/from_polished", 570, 1),
        ("mekanism:crushing/andesite/slab_from_polished_slab", 571, 1),
        ("mekanism:crushing/andesite/stairs_from_polished_stairs", 572, 1),
        ("mekanism:crushing/biofuel/apple", 573, 1),
        ("mekanism:crushing/biofuel/baked_potato", 574, 1),
        ("mekanism:crushing/biofuel/bamboo", 575, 1),
        ("mekanism:crushing/biofuel/berries", 576, 1),
        ("mekanism:crushing/biofuel/big_dripleaf", 577, 1),
        ("mekanism:crushing/biofuel/bread", 578, 1),
        ("mekanism:crushing/biofuel/bush", 579, 1),
        ("mekanism:crushing/biofuel/cactus_flower", 580, 1),
        ("mekanism:crushing/biofuel/cake", 581, 1),
        ("mekanism:crushing/biofuel/carved_pumpkin", 582, 1),
        ("mekanism:crushing/biofuel/cookie", 583, 1),
        ("mekanism:crushing/biofuel/crimson_roots", 584, 1),
        ("mekanism:crushing/biofuel/crops", 585, 1),
        ("mekanism:crushing/biofuel/crops_tags", 586, 1),
        ("mekanism:crushing/biofuel/dried_kelp", 587, 1),
        ("mekanism:crushing/biofuel/dried_kelp_block", 588, 1),
        ("mekanism:crushing/biofuel/fern", 589, 1),
        ("mekanism:crushing/biofuel/firefly_bush", 590, 1),
        ("mekanism:crushing/biofuel/fungus", 591, 1),
        ("mekanism:crushing/biofuel/glow_lichen", 592, 1),
        ("mekanism:crushing/biofuel/hanging_roots", 593, 1),
        ("mekanism:crushing/biofuel/hay_block", 594, 1),
        ("mekanism:crushing/biofuel/kelp", 595, 1),
        ("mekanism:crushing/biofuel/large_fern", 596, 1),
        ("mekanism:crushing/biofuel/leaf_litter", 597, 1),
        ("mekanism:crushing/biofuel/leaves", 598, 1),
        ("mekanism:crushing/biofuel/lily_pad", 599, 1),
        ("mekanism:crushing/biofuel/mangrove_roots", 600, 1),
        ("mekanism:crushing/biofuel/melon", 601, 1),
        ("mekanism:crushing/biofuel/melon_slice", 602, 1),
        ("mekanism:crushing/biofuel/moss_block", 603, 1),
        ("mekanism:crushing/biofuel/moss_carpet", 604, 1),
        ("mekanism:crushing/biofuel/mushroom_blocks", 605, 1),
        ("mekanism:crushing/biofuel/mushrooms", 606, 1),
        ("mekanism:crushing/biofuel/nether_sprouts", 607, 1),
        ("mekanism:crushing/biofuel/nether_wart_block", 608, 1),
        ("mekanism:crushing/biofuel/pale_hanging_moss", 609, 1),
        ("mekanism:crushing/biofuel/pink_petals", 610, 1),
        ("mekanism:crushing/biofuel/pitcher_pod", 611, 1),
        ("mekanism:crushing/biofuel/pumpkin", 612, 1),
        ("mekanism:crushing/biofuel/pumpkin_pie", 613, 1),
        ("mekanism:crushing/biofuel/rotten_flesh", 614, 1),
        ("mekanism:crushing/biofuel/saplings", 615, 1),
        ("mekanism:crushing/biofuel/sea_pickle", 616, 1),
        ("mekanism:crushing/biofuel/seagrass", 617, 1),
        ("mekanism:crushing/biofuel/seeds", 618, 1),
        ("mekanism:crushing/biofuel/short_grass", 619, 1),
        ("mekanism:crushing/biofuel/shroomlight", 620, 1),
        ("mekanism:crushing/biofuel/small_dripleaf", 621, 1),
        ("mekanism:crushing/biofuel/small_flowers", 622, 1),
        ("mekanism:crushing/biofuel/spore_blossom", 623, 1),
        ("mekanism:crushing/biofuel/straw_bed", 624, 1),
        ("mekanism:crushing/biofuel/sugar_cane", 625, 1),
        ("mekanism:crushing/biofuel/tall_flowers", 626, 1),
        ("mekanism:crushing/biofuel/tall_grass", 627, 1),
        ("mekanism:crushing/biofuel/vines", 628, 1),
        ("mekanism:crushing/biofuel/warped_roots", 629, 1),
        ("mekanism:crushing/biofuel/warped_wart_block", 630, 1),
        ("mekanism:crushing/biofuel/wildflowers", 631, 1),
        ("mekanism:crushing/blackstone/bricks_to_cracked_bricks", 632, 1),
        ("mekanism:crushing/blackstone/chiseled_bricks_to_bricks", 633, 1),
        ("mekanism:crushing/blackstone/from_cracked_bricks", 634, 1),
        ("mekanism:crushing/blackstone/from_polished", 635, 1),
        ("mekanism:crushing/blackstone/polished_slabs_to_slabs", 636, 1),
        ("mekanism:crushing/blackstone/polished_stairs_to_stairs", 637, 1),
        ("mekanism:crushing/blackstone/polished_wall_to_wall", 638, 1),
        ("mekanism:crushing/blaze_rod", 639, 1),
        ("mekanism:crushing/bone", 640, 1),
        ("mekanism:crushing/bone_block", 641, 1),
        ("mekanism:crushing/break_disc_5", 642, 1),
        ("mekanism:crushing/breeze_rod", 643, 1),
        ("mekanism:crushing/charcoal_dust", 644, 1),
        ("mekanism:crushing/chiseled_nether_bricks_to_nether_bricks", 645, 1),
        ("mekanism:crushing/cinnabar/brick_slabs_to_polished", 646, 1),
        ("mekanism:crushing/cinnabar/brick_stairs_to_polished", 647, 1),
        ("mekanism:crushing/cinnabar/brick_wall_to_polished", 648, 1),
        ("mekanism:crushing/cinnabar/bricks_to_polished", 649, 1),
        ("mekanism:crushing/cinnabar/chiseled_to_brick", 650, 1),
        ("mekanism:crushing/cinnabar/from_polished", 651, 1),
        ("mekanism:crushing/cinnabar/slabs_from_polished", 652, 1),
        ("mekanism:crushing/cinnabar/stairs_from_polished", 653, 1),
        ("mekanism:crushing/cinnabar/wall_from_polished", 654, 1),
        ("mekanism:crushing/cobblestone_to_gravel", 655, 1),
        ("mekanism:crushing/deepslate/brick_slabs_to_tile", 656, 1),
        ("mekanism:crushing/deepslate/brick_stairs_to_tile", 657, 1),
        ("mekanism:crushing/deepslate/brick_wall_to_tile", 658, 1),
        ("mekanism:crushing/deepslate/bricks_to_cracked_bricks", 659, 1),
        ("mekanism:crushing/deepslate/cracked_bricks_to_tile", 660, 1),
        ("mekanism:crushing/deepslate/cracked_tile_to_chiseled", 661, 1),
        ("mekanism:crushing/deepslate/from_chiseled", 662, 1),
        ("mekanism:crushing/deepslate/polished_slabs_to_brick", 663, 1),
        ("mekanism:crushing/deepslate/polished_stairs_to_brick", 664, 1),
        ("mekanism:crushing/deepslate/polished_to_bricks", 665, 1),
        ("mekanism:crushing/deepslate/polished_wall_to_brick", 666, 1),
        ("mekanism:crushing/deepslate/tile_slabs_to_cobbled", 667, 1),
        ("mekanism:crushing/deepslate/tile_stairs_to_cobbled", 668, 1),
        ("mekanism:crushing/deepslate/tile_to_cracked_tile", 669, 1),
        ("mekanism:crushing/deepslate/tile_wall_to_cobbled", 670, 1),
        ("mekanism:crushing/deepslate/to_cobbled", 671, 1),
        ("mekanism:crushing/dewax/chiseled_copper", 672, 1),
        ("mekanism:crushing/dewax/copper_bars", 673, 1),
        ("mekanism:crushing/dewax/copper_block", 674, 1),
        ("mekanism:crushing/dewax/copper_bulb", 675, 1),
        ("mekanism:crushing/dewax/copper_chain", 676, 1),
        ("mekanism:crushing/dewax/copper_chest", 677, 1),
        ("mekanism:crushing/dewax/copper_door", 678, 1),
        ("mekanism:crushing/dewax/copper_golem_statue", 679, 1),
        ("mekanism:crushing/dewax/copper_grate", 680, 1),
        ("mekanism:crushing/dewax/copper_lantern", 681, 1),
        ("mekanism:crushing/dewax/copper_trapdoor", 682, 1),
        ("mekanism:crushing/dewax/cut_copper", 683, 1),
        ("mekanism:crushing/dewax/cut_copper_slab", 684, 1),
        ("mekanism:crushing/dewax/cut_copper_stairs", 685, 1),
        ("mekanism:crushing/dewax/exposed_chiseled_copper", 686, 1),
        ("mekanism:crushing/dewax/exposed_copper", 687, 1),
        ("mekanism:crushing/dewax/exposed_copper_bars", 688, 1),
        ("mekanism:crushing/dewax/exposed_copper_bulb", 689, 1),
        ("mekanism:crushing/dewax/exposed_copper_chain", 690, 1),
        ("mekanism:crushing/dewax/exposed_copper_chest", 691, 1),
        ("mekanism:crushing/dewax/exposed_copper_door", 692, 1),
        ("mekanism:crushing/dewax/exposed_copper_golem_statue", 693, 1),
        ("mekanism:crushing/dewax/exposed_copper_grate", 694, 1),
        ("mekanism:crushing/dewax/exposed_copper_lantern", 695, 1),
        ("mekanism:crushing/dewax/exposed_copper_trapdoor", 696, 1),
        ("mekanism:crushing/dewax/exposed_cut_copper", 697, 1),
        ("mekanism:crushing/dewax/exposed_cut_copper_slab", 698, 1),
        ("mekanism:crushing/dewax/exposed_cut_copper_stairs", 699, 1),
        ("mekanism:crushing/dewax/exposed_lightning_rod", 700, 1),
        ("mekanism:crushing/dewax/lightning_rod", 701, 1),
        ("mekanism:crushing/dewax/oxidized_chiseled_copper", 702, 1),
        ("mekanism:crushing/dewax/oxidized_copper", 703, 1),
        ("mekanism:crushing/dewax/oxidized_copper_bars", 704, 1),
        ("mekanism:crushing/dewax/oxidized_copper_bulb", 705, 1),
        ("mekanism:crushing/dewax/oxidized_copper_chain", 706, 1),
        ("mekanism:crushing/dewax/oxidized_copper_chest", 707, 1),
        ("mekanism:crushing/dewax/oxidized_copper_door", 708, 1),
        ("mekanism:crushing/dewax/oxidized_copper_golem_statue", 709, 1),
        ("mekanism:crushing/dewax/oxidized_copper_grate", 710, 1),
        ("mekanism:crushing/dewax/oxidized_copper_lantern", 711, 1),
        ("mekanism:crushing/dewax/oxidized_copper_trapdoor", 712, 1),
        ("mekanism:crushing/dewax/oxidized_cut_copper", 713, 1),
        ("mekanism:crushing/dewax/oxidized_cut_copper_slab", 714, 1),
        ("mekanism:crushing/dewax/oxidized_cut_copper_stairs", 715, 1),
        ("mekanism:crushing/dewax/oxidized_lightning_rod", 716, 1),
        ("mekanism:crushing/dewax/weathered_chiseled_copper", 717, 1),
        ("mekanism:crushing/dewax/weathered_copper", 718, 1),
        ("mekanism:crushing/dewax/weathered_copper_bars", 719, 1),
        ("mekanism:crushing/dewax/weathered_copper_bulb", 720, 1),
        ("mekanism:crushing/dewax/weathered_copper_chain", 721, 1),
        ("mekanism:crushing/dewax/weathered_copper_chest", 722, 1),
        ("mekanism:crushing/dewax/weathered_copper_door", 723, 1),
        ("mekanism:crushing/dewax/weathered_copper_golem_statue", 724, 1),
        ("mekanism:crushing/dewax/weathered_copper_grate", 725, 1),
        ("mekanism:crushing/dewax/weathered_copper_lantern", 726, 1),
        ("mekanism:crushing/dewax/weathered_copper_trapdoor", 727, 1),
        ("mekanism:crushing/dewax/weathered_cut_copper", 728, 1),
        ("mekanism:crushing/dewax/weathered_cut_copper_slab", 729, 1),
        ("mekanism:crushing/dewax/weathered_cut_copper_stairs", 730, 1),
        ("mekanism:crushing/dewax/weathered_lightning_rod", 731, 1),
        ("mekanism:crushing/diorite/from_polished", 732, 1),
        ("mekanism:crushing/diorite/slab_from_polished_slab", 733, 1),
        ("mekanism:crushing/diorite/stairs_from_polished_stairs", 734, 1),
        ("mekanism:crushing/dried_ghast", 735, 1),
        ("mekanism:crushing/flint_to_gunpowder", 736, 1),
        ("mekanism:crushing/granite/from_polished", 737, 1),
        ("mekanism:crushing/granite/slab_from_polished_slab", 738, 1),
        ("mekanism:crushing/granite/stairs_from_polished_stairs", 739, 1),
        ("mekanism:crushing/gravel_to_sand", 740, 1),
        ("mekanism:crushing/honeycomb_from_block", 741, 1),
        ("mekanism:crushing/mud_bricks_to_packed", 742, 1),
        ("mekanism:crushing/nether_bricks_to_cracked_nether_bricks", 743, 1),
        ("mekanism:crushing/obsidian_to_dust", 744, 1),
        ("mekanism:crushing/pointed_dripstone_from_block", 745, 1),
        ("mekanism:crushing/polished_or_smooth_basalt_to_basalt", 746, 1),
        ("mekanism:crushing/prismarine/shard_from_block", 747, 1),
        ("mekanism:crushing/prismarine/shard_from_brick", 748, 1),
        ("mekanism:crushing/prismarine/shard_from_brick_slabs", 749, 1),
        ("mekanism:crushing/prismarine/shard_from_brick_stairs", 750, 1),
        ("mekanism:crushing/prismarine/shard_from_slabs", 751, 1),
        ("mekanism:crushing/prismarine/shard_from_stairs", 752, 1),
        ("mekanism:crushing/prismarine/shard_from_wall", 753, 1),
        ("mekanism:crushing/purpur_block_from_pillar", 754, 1),
        ("mekanism:crushing/quartz/bricks_to_chiseled", 755, 1),
        ("mekanism:crushing/quartz/chiseled_to_pillar", 756, 1),
        ("mekanism:crushing/quartz/from_pillar", 757, 1),
        ("mekanism:crushing/quartz/slab_to_smooth_slab", 758, 1),
        ("mekanism:crushing/quartz/smooth_to_bricks", 759, 1),
        ("mekanism:crushing/quartz/stairs_to_smooth_stairs", 760, 1),
        ("mekanism:crushing/quartz/to_smooth_quartz", 761, 1),
        ("mekanism:crushing/red_sandstone_to_sand", 762, 1),
        ("mekanism:crushing/resin/chiseled_resin_bricks_to_bricks", 763, 1),
        ("mekanism:crushing/resin/clump_from_block", 764, 1),
        ("mekanism:crushing/resin/resin_bricks_to_block", 765, 1),
        ("mekanism:crushing/sandstone_to_sand", 766, 1),
        ("mekanism:crushing/soul_soil_to_soul_sand", 767, 1),
        ("mekanism:crushing/stone/bricks_to_cracked_bricks", 768, 1),
        ("mekanism:crushing/stone/chiseled_bricks_to_bricks", 769, 1),
        ("mekanism:crushing/stone/from_cracked_bricks", 770, 1),
        ("mekanism:crushing/stone/slabs_to_cobblestone_slabs", 771, 1),
        ("mekanism:crushing/stone/stairs_to_cobblestone_stairs", 772, 1),
        ("mekanism:crushing/stone/to_cobblestone", 773, 1),
        ("mekanism:crushing/string/from_cushions", 774, 1),
        ("mekanism:crushing/string/from_wool", 775, 1),
        ("mekanism:crushing/string/from_wool_slabs", 776, 1),
        ("mekanism:crushing/string/from_wool_stairs", 777, 1),
        ("mekanism:crushing/sulfur/brick_slabs_to_polished", 778, 1),
        ("mekanism:crushing/sulfur/brick_stairs_to_polished", 779, 1),
        ("mekanism:crushing/sulfur/brick_wall_to_polished", 780, 1),
        ("mekanism:crushing/sulfur/bricks_to_polished", 781, 1),
        ("mekanism:crushing/sulfur/chiseled_to_brick", 782, 1),
        ("mekanism:crushing/sulfur/from_polished", 783, 1),
        ("mekanism:crushing/sulfur/potent_to_dust", 784, 1),
        ("mekanism:crushing/sulfur/slabs_from_polished", 785, 1),
        ("mekanism:crushing/sulfur/spike_to_dust", 786, 1),
        ("mekanism:crushing/sulfur/stairs_from_polished", 787, 1),
        ("mekanism:crushing/sulfur/to_dust", 788, 1),
        ("mekanism:crushing/sulfur/wall_from_polished", 789, 1),
        ("mekanism:crushing/tuff/brick_slabs_to_polished", 790, 1),
        ("mekanism:crushing/tuff/brick_stairs_to_polished", 791, 1),
        ("mekanism:crushing/tuff/brick_wall_to_polished", 792, 1),
        ("mekanism:crushing/tuff/bricks_to_polished", 793, 1),
        ("mekanism:crushing/tuff/from_polished", 794, 1),
        ("mekanism:crushing/tuff/slabs_from_polished", 795, 1),
        ("mekanism:crushing/tuff/stairs_from_polished", 796, 1),
        ("mekanism:crushing/tuff/tuff_brick_conversion", 797, 1),
        ("mekanism:crushing/tuff/wall_from_polished", 798, 1),
        ("mekanism:crystallizing/lithium", 799, 1),
        ("mekanism:crystallizing/salt", 800, 1),
        ("minecraft:cut_copper", 801, 1),
        ("minecraft:cut_copper_from_copper_block_stonecutting", 802, 1),
        ("minecraft:cut_copper_slab", 803, 1),
        ("minecraft:cut_copper_slab_from_copper_block_stonecutting", 804, 1),
        ("minecraft:cut_copper_slab_from_cut_copper_stonecutting", 805, 1),
        ("minecraft:cut_copper_stairs", 806, 1),
        ("minecraft:cut_copper_stairs_from_copper_block_stonecutting", 807, 1),
        ("minecraft:cut_copper_stairs_from_cut_copper_stonecutting", 808, 1),
        ("minecraft:cut_red_sandstone", 809, 1),
        ("minecraft:cut_red_sandstone_from_red_sandstone_stonecutting", 810, 1),
        ("minecraft:cut_red_sandstone_slab", 811, 1),
        ("minecraft:cut_red_sandstone_slab_from_cut_red_sandstone_stonecutting", 812, 1),
        ("minecraft:cut_red_sandstone_slab_from_red_sandstone_stonecutting", 813, 1),
        ("minecraft:cut_sandstone", 814, 1),
        ("minecraft:cut_sandstone_from_sandstone_stonecutting", 815, 1),
        ("minecraft:cut_sandstone_slab", 816, 1),
        ("minecraft:cut_sandstone_slab_from_cut_sandstone_stonecutting", 817, 1),
        ("minecraft:cut_sandstone_slab_from_sandstone_stonecutting", 818, 1),
        ("minecraft:cyan_banner", 819, 1),
        ("minecraft:cyan_bed", 820, 1),
        ("minecraft:cyan_bundle", 821, 1),
        ("minecraft:cyan_candle", 822, 1),
        ("minecraft:cyan_carpet", 823, 1),
        ("minecraft:cyan_concrete_powder", 824, 1),
        ("minecraft:cyan_concrete_slab", 825, 1),
        ("minecraft:cyan_concrete_slab_from_cyan_concrete_stonecutting", 826, 1),
        ("minecraft:cyan_concrete_stairs", 827, 1),
        ("minecraft:cyan_concrete_stairs_from_cyan_concrete_stonecutting", 828, 1),
        ("minecraft:cyan_cushion", 829, 1),
        ("minecraft:cyan_dye", 830, 1),
        ("minecraft:cyan_dye_from_pitcher_plant", 831, 1),
        ("minecraft:cyan_glazed_terracotta", 832, 1),
        ("minecraft:cyan_harness", 833, 1),
        ("minecraft:cyan_shulker_box", 834, 1),
        ("minecraft:cyan_stained_glass", 835, 1),
        ("minecraft:cyan_stained_glass_pane", 836, 1),
        ("minecraft:cyan_stained_glass_pane_from_glass_pane", 837, 1),
        ("minecraft:cyan_terracotta", 838, 1),
        ("minecraft:cyan_wool_slab", 839, 1),
        ("minecraft:cyan_wool_stairs", 840, 1),
        ("minecraft:dark_oak_boat", 841, 1),
        ("minecraft:dark_oak_button", 842, 1),
        ("minecraft:dark_oak_chest_boat", 843, 1),
        ("minecraft:dark_oak_door", 844, 1),
        ("minecraft:dark_oak_fence", 845, 1),
        ("minecraft:dark_oak_fence_gate", 846, 1),
        ("minecraft:dark_oak_hanging_sign", 847, 1),
        ("minecraft:dark_oak_planks", 848, 1),
        ("minecraft:dark_oak_pressure_plate", 849, 1),
        ("minecraft:dark_oak_shelf", 850, 1),
        ("minecraft:dark_oak_sign", 851, 1),
        ("minecraft:dark_oak_slab", 852, 1),
        ("minecraft:dark_oak_stairs", 853, 1),
        ("minecraft:dark_oak_trapdoor", 854, 1),
        ("minecraft:dark_oak_wood", 855, 1),
        ("minecraft:dark_prismarine", 856, 1),
        ("minecraft:dark_prismarine_slab", 857, 1),
        ("minecraft:dark_prismarine_slab_from_dark_prismarine_stonecutting", 858, 1),
        ("minecraft:dark_prismarine_stairs", 859, 1),
        ("minecraft:dark_prismarine_stairs_from_dark_prismarine_stonecutting", 860, 1),
        ("minecraft:daylight_detector", 861, 1),
        ("minecraft:decorated_pot_simple", 862, 1),
        ("minecraft:deepslate", 863, 1),
        ("minecraft:deepslate_brick_slab", 864, 1),
        ("minecraft:deepslate_brick_slab_from_cobbled_deepslate_stonecutting", 865, 1),
        ("minecraft:deepslate_brick_slab_from_deepslate_bricks_stonecutting", 866, 1),
        ("minecraft:deepslate_brick_slab_from_deepslate_stonecutting", 867, 1),
        ("minecraft:deepslate_brick_slab_from_polished_deepslate_stonecutting", 868, 1),
        ("minecraft:deepslate_brick_stairs", 869, 1),
        ("minecraft:deepslate_brick_stairs_from_cobbled_deepslate_stonecutting", 870, 1),
        ("minecraft:deepslate_brick_stairs_from_deepslate_bricks_stonecutting", 871, 1),
        ("minecraft:deepslate_brick_stairs_from_deepslate_stonecutting", 872, 1),
        ("minecraft:deepslate_brick_stairs_from_polished_deepslate_stonecutting", 873, 1),
        ("minecraft:deepslate_brick_wall", 874, 1),
        ("minecraft:deepslate_brick_wall_from_cobbled_deepslate_stonecutting", 875, 1),
        ("minecraft:deepslate_brick_wall_from_deepslate_bricks_stonecutting", 876, 1),
        ("minecraft:deepslate_brick_wall_from_deepslate_stonecutting", 877, 1),
        ("minecraft:deepslate_brick_wall_from_polished_deepslate_stonecutting", 878, 1),
        ("minecraft:deepslate_bricks", 879, 1),
        ("minecraft:deepslate_bricks_from_cobbled_deepslate_stonecutting", 880, 1),
        ("minecraft:deepslate_bricks_from_deepslate_stonecutting", 881, 1),
        ("minecraft:deepslate_bricks_from_polished_deepslate_stonecutting", 882, 1),
        ("minecraft:deepslate_tile_slab", 883, 1),
        ("minecraft:deepslate_tile_slab_from_cobbled_deepslate_stonecutting", 884, 1),
        ("minecraft:deepslate_tile_slab_from_deepslate_bricks_stonecutting", 885, 1),
        ("minecraft:deepslate_tile_slab_from_deepslate_stonecutting", 886, 1),
        ("minecraft:deepslate_tile_slab_from_deepslate_tiles_stonecutting", 887, 1),
        ("minecraft:deepslate_tile_slab_from_polished_deepslate_stonecutting", 888, 1),
        ("minecraft:deepslate_tile_stairs", 889, 1),
        ("minecraft:deepslate_tile_stairs_from_cobbled_deepslate_stonecutting", 890, 1),
        ("minecraft:deepslate_tile_stairs_from_deepslate_bricks_stonecutting", 891, 1),
        ("minecraft:deepslate_tile_stairs_from_deepslate_stonecutting", 892, 1),
        ("minecraft:deepslate_tile_stairs_from_deepslate_tiles_stonecutting", 893, 1),
        ("minecraft:deepslate_tile_stairs_from_polished_deepslate_stonecutting", 894, 1),
        ("minecraft:deepslate_tile_wall", 895, 1),
        ("minecraft:deepslate_tile_wall_from_cobbled_deepslate_stonecutting", 896, 1),
        ("minecraft:deepslate_tile_wall_from_deepslate_bricks_stonecutting", 897, 1),
        ("minecraft:deepslate_tile_wall_from_deepslate_stonecutting", 898, 1),
        ("minecraft:deepslate_tile_wall_from_deepslate_tiles_stonecutting", 899, 1),
        ("minecraft:deepslate_tile_wall_from_polished_deepslate_stonecutting", 900, 1),
        ("minecraft:deepslate_tiles", 901, 1),
        ("minecraft:deepslate_tiles_from_cobbled_deepslate_stonecutting", 902, 1),
        ("minecraft:deepslate_tiles_from_deepslate_bricks_stonecutting", 903, 1),
        ("minecraft:deepslate_tiles_from_deepslate_stonecutting", 904, 1),
        ("minecraft:deepslate_tiles_from_polished_deepslate_stonecutting", 905, 1),
        ("minecraft:detector_rail", 906, 1),
        ("minecraft:diamond", 907, 1),
        ("minecraft:diamond_axe", 908, 1),
        ("minecraft:diamond_block", 909, 1),
        ("minecraft:diamond_boots", 910, 1),
        ("minecraft:diamond_chestplate", 911, 1),
        ("minecraft:diamond_from_blasting_deepslate_diamond_ore", 912, 1),
        ("minecraft:diamond_from_blasting_diamond_ore", 913, 1),
        ("minecraft:diamond_from_smelting_deepslate_diamond_ore", 914, 1),
        ("minecraft:diamond_from_smelting_diamond_ore", 915, 1),
        ("minecraft:diamond_helmet", 916, 1),
        ("minecraft:diamond_hoe", 917, 1),
        ("minecraft:diamond_leggings", 918, 1),
        ("mekanismtools:diamond_paxel", 919, 1),
        ("minecraft:diamond_pickaxe", 920, 1),
        ("minecraft:diamond_shovel", 921, 1),
        ("minecraft:diamond_spear", 922, 1),
        ("minecraft:diamond_sword", 923, 1),
        ("mekanism:dictionary", 924, 1),
        ("mekanism:digital_miner", 925, 1),
        ("mekanism:dimensional_stabilizer", 926, 1),
        ("minecraft:diorite", 927, 1),
        ("minecraft:diorite_slab", 928, 1),
        ("minecraft:diorite_slab_from_diorite_stonecutting", 929, 1),
        ("minecraft:diorite_stairs", 930, 1),
        ("minecraft:diorite_stairs_from_diorite_stonecutting", 931, 1),
        ("minecraft:diorite_wall", 932, 1),
        ("minecraft:diorite_wall_from_diorite_stonecutting", 933, 1),
        ("minecraft:dispenser", 934, 1),
        ("mekanism:dosimeter", 935, 1),
        ("mekanism:dosimeter_unit", 936, 1),
        ("minecraft:dried_ghast", 937, 1),
        ("minecraft:dried_kelp", 938, 1),
        ("minecraft:dried_kelp_block", 939, 1),
        ("minecraft:dried_kelp_from_campfire_cooking", 940, 1),
        ("minecraft:dried_kelp_from_smelting", 941, 1),
        ("minecraft:dried_kelp_from_smoking", 942, 1),
        ("minecraft:dripstone_block", 943, 1),
        ("minecraft:dropper", 944, 1),
        ("minecraft:dune_armor_trim_smithing_template", 945, 1),
        ("minecraft:dune_armor_trim_smithing_template_smithing_trim", 946, 1),
        ("mekanism:dye_base", 947, 1),
        ("minecraft:dye_black_bed", 948, 1),
        ("minecraft:dye_black_carpet", 949, 1),
        ("minecraft:dye_black_cushion", 950, 1),
        ("minecraft:dye_black_harness", 951, 1),
        ("minecraft:dye_black_wool", 952, 1),
        ("minecraft:dye_black_wool_slab", 953, 1),
        ("minecraft:dye_black_wool_stairs", 954, 1),
        ("minecraft:dye_blue_bed", 955, 1),
        ("minecraft:dye_blue_carpet", 956, 1),
        ("minecraft:dye_blue_cushion", 957, 1),
        ("minecraft:dye_blue_harness", 958, 1),
        ("minecraft:dye_blue_wool", 959, 1),
        ("minecraft:dye_blue_wool_slab", 960, 1),
        ("minecraft:dye_blue_wool_stairs", 961, 1),
        ("minecraft:dye_brown_bed", 962, 1),
        ("minecraft:dye_brown_carpet", 963, 1),
        ("minecraft:dye_brown_cushion", 964, 1),
        ("minecraft:dye_brown_harness", 965, 1),
        ("minecraft:dye_brown_wool", 966, 1),
        ("minecraft:dye_brown_wool_slab", 967, 1),
        ("minecraft:dye_brown_wool_stairs", 968, 1),
        ("minecraft:dye_cyan_bed", 969, 1),
        ("minecraft:dye_cyan_carpet", 970, 1),
        ("minecraft:dye_cyan_cushion", 971, 1),
        ("minecraft:dye_cyan_harness", 972, 1),
        ("minecraft:dye_cyan_wool", 973, 1),
        ("minecraft:dye_cyan_wool_slab", 974, 1),
        ("minecraft:dye_cyan_wool_stairs", 975, 1),
        ("minecraft:dye_gray_bed", 976, 1),
        ("minecraft:dye_gray_carpet", 977, 1),
        ("minecraft:dye_gray_cushion", 978, 1),
        ("minecraft:dye_gray_harness", 979, 1),
        ("minecraft:dye_gray_wool", 980, 1),
        ("minecraft:dye_gray_wool_slab", 981, 1),
        ("minecraft:dye_gray_wool_stairs", 982, 1),
        ("minecraft:dye_green_bed", 983, 1),
        ("minecraft:dye_green_carpet", 984, 1),
        ("minecraft:dye_green_cushion", 985, 1),
        ("minecraft:dye_green_harness", 986, 1),
        ("minecraft:dye_green_wool", 987, 1),
        ("minecraft:dye_green_wool_slab", 988, 1),
        ("minecraft:dye_green_wool_stairs", 989, 1),
        ("minecraft:dye_light_blue_bed", 990, 1),
        ("minecraft:dye_light_blue_carpet", 991, 1),
        ("minecraft:dye_light_blue_cushion", 992, 1),
        ("minecraft:dye_light_blue_harness", 993, 1),
        ("minecraft:dye_light_blue_wool", 994, 1),
        ("minecraft:dye_light_blue_wool_slab", 995, 1),
        ("minecraft:dye_light_blue_wool_stairs", 996, 1),
        ("minecraft:dye_light_gray_bed", 997, 1),
        ("minecraft:dye_light_gray_carpet", 998, 1),
        ("minecraft:dye_light_gray_cushion", 999, 1),
        ("minecraft:dye_light_gray_harness", 1000, 1),
        ("minecraft:dye_light_gray_wool", 1001, 1),
        ("minecraft:dye_light_gray_wool_slab", 1002, 1),
        ("minecraft:dye_light_gray_wool_stairs", 1003, 1),
        ("minecraft:dye_lime_bed", 1004, 1),
        ("minecraft:dye_lime_carpet", 1005, 1),
        ("minecraft:dye_lime_cushion", 1006, 1),
        ("minecraft:dye_lime_harness", 1007, 1),
        ("minecraft:dye_lime_wool", 1008, 1),
        ("minecraft:dye_lime_wool_slab", 1009, 1),
        ("minecraft:dye_lime_wool_stairs", 1010, 1),
        ("minecraft:dye_magenta_bed", 1011, 1),
        ("minecraft:dye_magenta_carpet", 1012, 1),
        ("minecraft:dye_magenta_cushion", 1013, 1),
        ("minecraft:dye_magenta_harness", 1014, 1),
        ("minecraft:dye_magenta_wool", 1015, 1),
        ("minecraft:dye_magenta_wool_slab", 1016, 1),
        ("minecraft:dye_magenta_wool_stairs", 1017, 1),
        ("minecraft:dye_orange_bed", 1018, 1),
        ("minecraft:dye_orange_carpet", 1019, 1),
        ("minecraft:dye_orange_cushion", 1020, 1),
        ("minecraft:dye_orange_harness", 1021, 1),
        ("minecraft:dye_orange_wool", 1022, 1),
        ("minecraft:dye_orange_wool_slab", 1023, 1),
        ("minecraft:dye_orange_wool_stairs", 1024, 1),
        ("minecraft:dye_pink_bed", 1025, 1),
        ("minecraft:dye_pink_carpet", 1026, 1),
        ("minecraft:dye_pink_cushion", 1027, 1),
        ("minecraft:dye_pink_harness", 1028, 1),
        ("minecraft:dye_pink_wool", 1029, 1),
        ("minecraft:dye_pink_wool_slab", 1030, 1),
        ("minecraft:dye_pink_wool_stairs", 1031, 1),
        ("minecraft:dye_purple_bed", 1032, 1),
        ("minecraft:dye_purple_carpet", 1033, 1),
        ("minecraft:dye_purple_cushion", 1034, 1),
        ("minecraft:dye_purple_harness", 1035, 1),
        ("minecraft:dye_purple_wool", 1036, 1),
        ("minecraft:dye_purple_wool_slab", 1037, 1),
        ("minecraft:dye_purple_wool_stairs", 1038, 1),
        ("minecraft:dye_red_bed", 1039, 1),
        ("minecraft:dye_red_carpet", 1040, 1),
        ("minecraft:dye_red_cushion", 1041, 1),
        ("minecraft:dye_red_harness", 1042, 1),
        ("minecraft:dye_red_wool", 1043, 1),
        ("minecraft:dye_red_wool_slab", 1044, 1),
        ("minecraft:dye_red_wool_stairs", 1045, 1),
        ("minecraft:dye_white_bed", 1046, 1),
        ("minecraft:dye_white_carpet", 1047, 1),
        ("minecraft:dye_white_cushion", 1048, 1),
        ("minecraft:dye_white_harness", 1049, 1),
        ("minecraft:dye_white_wool", 1050, 1),
        ("minecraft:dye_white_wool_slab", 1051, 1),
        ("minecraft:dye_white_wool_stairs", 1052, 1),
        ("minecraft:dye_yellow_bed", 1053, 1),
        ("minecraft:dye_yellow_carpet", 1054, 1),
        ("minecraft:dye_yellow_cushion", 1055, 1),
        ("minecraft:dye_yellow_harness", 1056, 1),
        ("minecraft:dye_yellow_wool", 1057, 1),
        ("minecraft:dye_yellow_wool_slab", 1058, 1),
        ("minecraft:dye_yellow_wool_stairs", 1059, 1),
        ("mekanism:dynamic_tank", 1060, 1),
        ("mekanism:dynamic_valve", 1061, 1),
        ("mekanism:electric_bow", 1062, 1),
        ("mekanism:electric_pump", 1063, 1),
        ("mekanism:electrolytic_breathing_unit", 1064, 1),
        ("mekanism:electrolytic_core", 1065, 1),
        ("mekanism:electrolytic_separator", 1066, 1),
        ("mekanismgenerators:electromagnetic_coil", 1067, 1),
        ("mekanism:elytra_unit", 1068, 1),
        ("minecraft:emerald", 1069, 1),
        ("minecraft:emerald_block", 1070, 1),
        ("minecraft:emerald_from_blasting_deepslate_emerald_ore", 1071, 1),
        ("minecraft:emerald_from_blasting_emerald_ore", 1072, 1),
        ("minecraft:emerald_from_smelting_deepslate_emerald_ore", 1073, 1),
        ("minecraft:emerald_from_smelting_emerald_ore", 1074, 1),
        ("minecraft:enchanting_table", 1075, 1),
        ("minecraft:end_crystal", 1076, 1),
        ("minecraft:end_rod", 1077, 1),
        ("minecraft:end_stone_brick_slab", 1078, 1),
        ("minecraft:end_stone_brick_slab_from_end_stone_bricks_stonecutting", 1079, 1),
        ("minecraft:end_stone_brick_slab_from_end_stone_stonecutting", 1080, 1),
        ("minecraft:end_stone_brick_stairs", 1081, 1),
        ("minecraft:end_stone_brick_stairs_from_end_stone_bricks_stonecutting", 1082, 1),
        ("minecraft:end_stone_brick_stairs_from_end_stone_stonecutting", 1083, 1),
        ("minecraft:end_stone_brick_wall", 1084, 1),
        ("minecraft:end_stone_brick_wall_from_end_stone_bricks_stonecutting", 1085, 1),
        ("minecraft:end_stone_brick_wall_from_end_stone_stonecutting", 1086, 1),
        ("minecraft:end_stone_bricks", 1087, 1),
        ("minecraft:end_stone_bricks_from_end_stone_stonecutting", 1088, 1),
        ("minecraft:ender_chest", 1089, 1),
        ("minecraft:ender_eye", 1090, 1),
        ("mekanism:energized_smelter", 1091, 1),
        ("mekanism:energy_conversion/redstone", 1092, 1),
        ("mekanism:energy_conversion/redstone_block", 1093, 1),
        ("mekanism:energy_cube/advanced", 1094, 1),
        ("mekanism:energy_cube/basic", 1095, 1),
        ("mekanism:energy_cube/elite", 1096, 1),
        ("mekanism:energy_cube/ultimate", 1097, 1),
        ("mekanism:energy_tablet", 1098, 1),
        ("mekanism:energy_unit", 1099, 1),
        ("mekanism:enriching/charcoal", 1100, 1),
        ("mekanism:enriching/charcoal_dust", 1101, 1),
        ("mekanism:enriching/clay_ball", 1102, 1),
        ("mekanism:enriching/conversion/andesite/slab_to_polished_slab", 1103, 1),
        ("mekanism:enriching/conversion/andesite/stairs_to_polished_stairs", 1104, 1),
        ("mekanism:enriching/conversion/andesite/to_polished", 1105, 1),
        ("mekanism:enriching/conversion/basalt_or_smooth_to_polished_basalt", 1106, 1),
        ("mekanism:enriching/conversion/blackstone/bricks_to_chiseled_bricks", 1107, 1),
        ("mekanism:enriching/conversion/blackstone/cracked_bricks_to_bricks", 1108, 1),
        ("mekanism:enriching/conversion/blackstone/to_cracked_bricks", 1109, 1),
        ("mekanism:enriching/conversion/cinnabar/brick_to_chiseled", 1110, 1),
        ("mekanism:enriching/conversion/cinnabar/polished_slabs_to_brick", 1111, 1),
        ("mekanism:enriching/conversion/cinnabar/polished_stairs_to_brick", 1112, 1),
        ("mekanism:enriching/conversion/cinnabar/polished_to_brick", 1113, 1),
        ("mekanism:enriching/conversion/cinnabar/polished_wall_to_brick", 1114, 1),
        ("mekanism:enriching/conversion/cinnabar/slabs_to_polished", 1115, 1),
        ("mekanism:enriching/conversion/cinnabar/stairs_to_polished", 1116, 1),
        ("mekanism:enriching/conversion/cinnabar/to_polished", 1117, 1),
        ("mekanism:enriching/conversion/cinnabar/wall_to_polished", 1118, 1),
        ("mekanism:enriching/conversion/cracked_nether_bricks_to_nether_bricks", 1119, 1),
        ("mekanism:enriching/conversion/deepslate/chiseled_to_cracked_tile", 1120, 1),
        ("mekanism:enriching/conversion/deepslate/cobbled_slabs_to_polished", 1121, 1),
        ("mekanism:enriching/conversion/deepslate/cobbled_stairs_to_polished", 1122, 1),
        ("mekanism:enriching/conversion/deepslate/cobbled_wall_to_polished", 1123, 1),
        ("mekanism:enriching/conversion/deepslate/cracked_bricks_to_bricks", 1124, 1),
        ("mekanism:enriching/conversion/deepslate/cracked_tile_to_tile", 1125, 1),
        ("mekanism:enriching/conversion/deepslate/from_cobbled", 1126, 1),
        ("mekanism:enriching/conversion/deepslate/polished_chiseled", 1127, 1),
        ("mekanism:enriching/conversion/deepslate/polished_slabs_to_tile", 1128, 1),
        ("mekanism:enriching/conversion/deepslate/polished_stairs_to_tile", 1129, 1),
        ("mekanism:enriching/conversion/deepslate/polished_wall_to_tile", 1130, 1),
        ("mekanism:enriching/conversion/deepslate/tile_slabs_to_brick", 1131, 1),
        ("mekanism:enriching/conversion/deepslate/tile_stairs_to_brick", 1132, 1),
        ("mekanism:enriching/conversion/deepslate/tile_wall_to_brick", 1133, 1),
        ("mekanism:enriching/conversion/deepslate/tiles_to_cracked_bricks", 1134, 1),
        ("mekanism:enriching/conversion/deepslate/to_polished", 1135, 1),
        ("mekanism:enriching/conversion/diorite/slab_to_polished_slab", 1136, 1),
        ("mekanism:enriching/conversion/diorite/stairs_to_polished_stairs", 1137, 1),
        ("mekanism:enriching/conversion/diorite/to_polished", 1138, 1),
        ("mekanism:enriching/conversion/granite/slab_to_polished_slab", 1139, 1),
        ("mekanism:enriching/conversion/granite/stairs_to_polished_stairs", 1140, 1),
        ("mekanism:enriching/conversion/granite/to_polished", 1141, 1),
        ("mekanism:enriching/conversion/gravel_to_flint", 1142, 1),
        ("mekanism:enriching/conversion/gunpowder_to_flint", 1143, 1),
        ("mekanism:enriching/conversion/mossy/cobblestone", 1144, 1),
        ("mekanism:enriching/conversion/mossy/cobblestone_slabs", 1145, 1),
        ("mekanism:enriching/conversion/mossy/cobblestone_stairs", 1146, 1),
        ("mekanism:enriching/conversion/mossy/cobblestone_walls", 1147, 1),
        ("mekanism:enriching/conversion/mossy/stone_brick_slabs", 1148, 1),
        ("mekanism:enriching/conversion/mossy/stone_brick_stairs", 1149, 1),
        ("mekanism:enriching/conversion/mossy/stone_brick_walls", 1150, 1),
        ("mekanism:enriching/conversion/mossy/stone_bricks", 1151, 1),
        ("mekanism:enriching/conversion/mud/packed_to_bricks", 1152, 1),
        ("mekanism:enriching/conversion/mud/to_clay", 1153, 1),
        ("mekanism:enriching/conversion/nether_bricks_to_chiseled_nether_bricks", 1154, 1),
        ("mekanism:enriching/conversion/purpur_pillar_from_block", 1155, 1),
        ("mekanism:enriching/conversion/quartz/from_smooth_quartz", 1156, 1),
        ("mekanism:enriching/conversion/quartz/smooth_slab_to_slab", 1157, 1),
        ("mekanism:enriching/conversion/quartz/smooth_stairs_to_stairs", 1158, 1),
        ("mekanism:enriching/conversion/quartz/to_item", 1159, 1),
        ("mekanism:enriching/conversion/resin/block_to_bricks", 1160, 1),
        ("mekanism:enriching/conversion/resin/bricks_to_chiseled", 1161, 1),
        ("mekanism:enriching/conversion/sand_to_gravel", 1162, 1),
        ("mekanism:enriching/conversion/soul_sand_to_soul_soil", 1163, 1),
        ("mekanism:enriching/conversion/stone/bricks_to_chiseled_bricks", 1164, 1),
        ("mekanism:enriching/conversion/stone/cracked_bricks_to_bricks", 1165, 1),
        ("mekanism:enriching/conversion/stone/to_cracked_bricks", 1166, 1),
        ("mekanism:enriching/conversion/sulfur/brick_to_chiseled", 1167, 1),
        ("mekanism:enriching/conversion/sulfur/dust_to_spike", 1168, 1),
        ("mekanism:enriching/conversion/sulfur/polished_slabs_to_brick", 1169, 1),
        ("mekanism:enriching/conversion/sulfur/polished_stairs_to_brick", 1170, 1),
        ("mekanism:enriching/conversion/sulfur/polished_to_brick", 1171, 1),
        ("mekanism:enriching/conversion/sulfur/polished_wall_to_brick", 1172, 1),
        ("mekanism:enriching/conversion/sulfur/slabs_to_polished", 1173, 1),
        ("mekanism:enriching/conversion/sulfur/spike_to_gunpowder", 1174, 1),
        ("mekanism:enriching/conversion/sulfur/stairs_to_polished", 1175, 1),
        ("mekanism:enriching/conversion/sulfur/to_polished", 1176, 1),
        ("mekanism:enriching/conversion/sulfur/wall_to_polished", 1177, 1),
        ("mekanism:enriching/conversion/tuff/brick_to_chiseled", 1178, 1),
        ("mekanism:enriching/conversion/tuff/polished_slabs_to_brick", 1179, 1),
        ("mekanism:enriching/conversion/tuff/polished_stairs_to_brick", 1180, 1),
        ("mekanism:enriching/conversion/tuff/polished_to_brick", 1181, 1),
        ("mekanism:enriching/conversion/tuff/polished_wall_to_brick", 1182, 1),
        ("mekanism:enriching/conversion/tuff/slabs_to_polished", 1183, 1),
        ("mekanism:enriching/conversion/tuff/stairs_to_polished", 1184, 1),
        ("mekanism:enriching/conversion/tuff/to_polished", 1185, 1),
        ("mekanism:enriching/conversion/tuff/wall_to_polished", 1186, 1),
        ("mekanism:enriching/deoxidizing/chiseled_copper", 1187, 1),
        ("mekanism:enriching/deoxidizing/copper_bars", 1188, 1),
        ("mekanism:enriching/deoxidizing/copper_block", 1189, 1),
        ("mekanism:enriching/deoxidizing/copper_bulb", 1190, 1),
        ("mekanism:enriching/deoxidizing/copper_chain", 1191, 1),
        ("mekanism:enriching/deoxidizing/copper_chest", 1192, 1),
        ("mekanism:enriching/deoxidizing/copper_door", 1193, 1),
        ("mekanism:enriching/deoxidizing/copper_golem_statue", 1194, 1),
        ("mekanism:enriching/deoxidizing/copper_grate", 1195, 1),
        ("mekanism:enriching/deoxidizing/copper_lantern", 1196, 1),
        ("mekanism:enriching/deoxidizing/copper_trapdoor", 1197, 1),
        ("mekanism:enriching/deoxidizing/cut_copper", 1198, 1),
        ("mekanism:enriching/deoxidizing/cut_copper_slab", 1199, 1),
        ("mekanism:enriching/deoxidizing/cut_copper_stairs", 1200, 1),
        ("mekanism:enriching/deoxidizing/exposed_chiseled_copper", 1201, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper", 1202, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_bars", 1203, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_bulb", 1204, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_chain", 1205, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_chest", 1206, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_door", 1207, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_golem_statue", 1208, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_grate", 1209, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_lantern", 1210, 1),
        ("mekanism:enriching/deoxidizing/exposed_copper_trapdoor", 1211, 1),
        ("mekanism:enriching/deoxidizing/exposed_cut_copper", 1212, 1),
        ("mekanism:enriching/deoxidizing/exposed_cut_copper_slab", 1213, 1),
        ("mekanism:enriching/deoxidizing/exposed_cut_copper_stairs", 1214, 1),
        ("mekanism:enriching/deoxidizing/exposed_lightning_rod", 1215, 1),
        ("mekanism:enriching/deoxidizing/lightning_rod", 1216, 1),
        ("mekanism:enriching/deoxidizing/weathered_chiseled_copper", 1217, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper", 1218, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_bars", 1219, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_bulb", 1220, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_chain", 1221, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_chest", 1222, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_door", 1223, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_golem_statue", 1224, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_grate", 1225, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_lantern", 1226, 1),
        ("mekanism:enriching/deoxidizing/weathered_copper_trapdoor", 1227, 1),
        ("mekanism:enriching/deoxidizing/weathered_cut_copper", 1228, 1),
        ("mekanism:enriching/deoxidizing/weathered_cut_copper_slab", 1229, 1),
        ("mekanism:enriching/deoxidizing/weathered_cut_copper_stairs", 1230, 1),
        ("mekanism:enriching/deoxidizing/weathered_lightning_rod", 1231, 1),
        ("mekanism:enriching/dye/black", 1232, 1),
        ("mekanism:enriching/dye/blue", 1233, 1),
        ("mekanism:enriching/dye/brown", 1234, 1),
        ("mekanism:enriching/dye/gray", 1235, 1),
        ("mekanism:enriching/dye/green", 1236, 1),
        ("mekanism:enriching/dye/large_cyan", 1237, 1),
        ("mekanism:enriching/dye/large_magenta", 1238, 1),
        ("mekanism:enriching/dye/large_pink", 1239, 1),
        ("mekanism:enriching/dye/large_red", 1240, 1),
        ("mekanism:enriching/dye/large_yellow", 1241, 1),
        ("mekanism:enriching/dye/light_blue", 1242, 1),
        ("mekanism:enriching/dye/light_gray", 1243, 1),
        ("mekanism:enriching/dye/lime", 1244, 1),
        ("mekanism:enriching/dye/orange", 1245, 1),
        ("mekanism:enriching/dye/small_magenta", 1246, 1),
        ("mekanism:enriching/dye/small_pink", 1247, 1),
        ("mekanism:enriching/dye/small_red", 1248, 1),
        ("mekanism:enriching/dye/small_yellow", 1249, 1),
        ("mekanism:enriching/dye/white", 1250, 1),
        ("mekanism:enriching/enriched/carbon", 1251, 1),
        ("mekanism:enriching/enriched/diamond", 1252, 1),
        ("mekanism:enriching/enriched/gold", 1253, 1),
        ("mekanism:enriching/enriched/redstone", 1254, 1),
        ("mekanism:enriching/enriched/refined_obsidian", 1255, 1),
        ("mekanism:enriching/enriched/tin", 1256, 1),
        ("mekanism:enriching/glowstone_dust", 1257, 1),
        ("mekanism:enriching/hdpe_sheet", 1258, 1),
        ("mekanism:enriching/salt", 1259, 1),
        ("mekanism:enriching/string", 1260, 1),
        ("mekanism:enrichment_chamber", 1261, 1),
        ("mekanism:evaporating/brine", 1262, 1),
        ("mekanism:evaporating/lithium", 1263, 1),
        ("mekanism:excavation_escalation_unit", 1264, 1),
        ("minecraft:exposed_chiseled_copper", 1265, 1),
        ("minecraft:exposed_chiseled_copper_from_exposed_copper_stonecutting", 1266, 1),
        ("minecraft:exposed_chiseled_copper_from_exposed_cut_copper_stonecutting", 1267, 1),
        ("minecraft:exposed_copper_bulb", 1268, 1),
        ("minecraft:exposed_copper_grate", 1269, 1),
        ("minecraft:exposed_copper_grate_from_exposed_copper_stonecutting", 1270, 1),
        ("minecraft:exposed_cut_copper", 1271, 1),
        ("minecraft:exposed_cut_copper_from_exposed_copper_stonecutting", 1272, 1),
        ("minecraft:exposed_cut_copper_slab", 1273, 1),
        ("minecraft:exposed_cut_copper_slab_from_exposed_copper_stonecutting", 1274, 1),
        ("minecraft:exposed_cut_copper_slab_from_exposed_cut_copper_stonecutting", 1275, 1),
        ("minecraft:exposed_cut_copper_stairs", 1276, 1),
        ("minecraft:exposed_cut_copper_stairs_from_exposed_copper_stonecutting", 1277, 1),
        ("minecraft:exposed_cut_copper_stairs_from_exposed_cut_copper_stonecutting", 1278, 1),
        ("minecraft:eye_armor_trim_smithing_template", 1279, 1),
        ("minecraft:eye_armor_trim_smithing_template_smithing_trim", 1280, 1),
        ("mekanism:factory/advanced/combining", 1281, 1),
        ("mekanism:factory/advanced/compressing", 1282, 1),
        ("mekanism:factory/advanced/crushing", 1283, 1),
        ("mekanism:factory/advanced/enriching", 1284, 1),
        ("mekanism:factory/advanced/infusing", 1285, 1),
        ("mekanism:factory/advanced/injecting", 1286, 1),
        ("mekanism:factory/advanced/purifying", 1287, 1),
        ("mekanism:factory/advanced/sawing", 1288, 1),
        ("mekanism:factory/advanced/smelting", 1289, 1),
        ("mekanism:factory/basic/combining", 1290, 1),
        ("mekanism:factory/basic/compressing", 1291, 1),
        ("mekanism:factory/basic/crushing", 1292, 1),
        ("mekanism:factory/basic/enriching", 1293, 1),
        ("mekanism:factory/basic/infusing", 1294, 1),
        ("mekanism:factory/basic/injecting", 1295, 1),
        ("mekanism:factory/basic/purifying", 1296, 1),
        ("mekanism:factory/basic/sawing", 1297, 1),
        ("mekanism:factory/basic/smelting", 1298, 1),
        ("mekanism:factory/elite/combining", 1299, 1),
        ("mekanism:factory/elite/compressing", 1300, 1),
        ("mekanism:factory/elite/crushing", 1301, 1),
        ("mekanism:factory/elite/enriching", 1302, 1),
        ("mekanism:factory/elite/infusing", 1303, 1),
        ("mekanism:factory/elite/injecting", 1304, 1),
        ("mekanism:factory/elite/purifying", 1305, 1),
        ("mekanism:factory/elite/sawing", 1306, 1),
        ("mekanism:factory/elite/smelting", 1307, 1),
        ("mekanism:factory/ultimate/combining", 1308, 1),
        ("mekanism:factory/ultimate/compressing", 1309, 1),
        ("mekanism:factory/ultimate/crushing", 1310, 1),
        ("mekanism:factory/ultimate/enriching", 1311, 1),
        ("mekanism:factory/ultimate/infusing", 1312, 1),
        ("mekanism:factory/ultimate/injecting", 1313, 1),
        ("mekanism:factory/ultimate/purifying", 1314, 1),
        ("mekanism:factory/ultimate/sawing", 1315, 1),
        ("mekanism:factory/ultimate/smelting", 1316, 1),
        ("mekanism:farming_unit", 1317, 1),
        ("minecraft:fermented_spider_eye", 1318, 1),
        ("minecraft:field_masoned_banner_pattern", 1319, 1),
        ("minecraft:fire_charge", 1320, 1),
        ("minecraft:firework_rocket_simple", 1321, 1),
        ("minecraft:fishing_rod", 1322, 1),
        ("mekanismgenerators:fission_reactor/casing", 1323, 1),
        ("mekanismgenerators:fission_reactor/control_rod_assembly", 1324, 1),
        ("mekanismgenerators:fission_reactor/fuel_assembly", 1325, 1),
        ("mekanismgenerators:fission_reactor/logic_adapter", 1326, 1),
        ("mekanismgenerators:fission_reactor/port", 1327, 1),
        ("mekanism:flamethrower", 1328, 1),
        ("minecraft:fletching_table", 1329, 1),
        ("minecraft:flint_and_steel", 1330, 1),
        ("minecraft:flow_armor_trim_smithing_template", 1331, 1),
        ("minecraft:flow_armor_trim_smithing_template_smithing_trim", 1332, 1),
        ("minecraft:flower_banner_pattern", 1333, 1),
        ("minecraft:flower_pot", 1334, 1),
        ("mekanism:fluid_tank/advanced", 1335, 1),
        ("mekanism:fluid_tank/basic", 1336, 1),
        ("mekanism:fluid_tank/elite", 1337, 1),
        ("mekanism:fluid_tank/ultimate", 1338, 1),
        ("mekanism:fluidic_plenisher", 1339, 1),
        ("mekanism:formulaic_assemblicator", 1340, 1),
        ("mekanism:fortune_unit", 1341, 1),
        ("mekanism:free_runners", 1342, 1),
        ("mekanism:free_runners_armored", 1343, 1),
        ("mekanism:frost_walker_unit", 1344, 1),
        ("mekanism:fuelwood_heater", 1345, 1),
        ("minecraft:furnace", 1346, 1),
        ("minecraft:furnace_minecart", 1347, 1),
        ("mekanism:gauge_dropper", 1348, 1),
        ("mekanism:geiger_counter", 1349, 1),
        ("mekanism:geiger_unit", 1350, 1),
        ("mekanismgenerators:generator/advanced_solar", 1351, 1),
        ("mekanismgenerators:generator/bio", 1352, 1),
        ("mekanismgenerators:generator/gas_burning", 1353, 1),
        ("mekanismgenerators:generator/heat", 1354, 1),
        ("mekanismgenerators:generator/solar", 1355, 1),
        ("mekanismgenerators:generator/wind", 1356, 1),
        ("mekanismgenerators:geothermal_generator_unit", 1357, 1),
        ("minecraft:glass", 1358, 1),
        ("minecraft:glass_bottle", 1359, 1),
        ("minecraft:glass_pane", 1360, 1),
        ("minecraft:glistering_melon_slice", 1361, 1),
        ("minecraft:glow_item_frame", 1362, 1),
        ("mekanismadditions:glow_panel/black", 1363, 1),
        ("mekanismadditions:glow_panel/blue", 1364, 1),
        ("mekanismadditions:glow_panel/brown", 1365, 1),
        ("mekanismadditions:glow_panel/cyan", 1366, 1),
        ("mekanismadditions:glow_panel/gray", 1367, 1),
        ("mekanismadditions:glow_panel/green", 1368, 1),
        ("mekanismadditions:glow_panel/light_blue", 1369, 1),
        ("mekanismadditions:glow_panel/light_gray", 1370, 1),
        ("mekanismadditions:glow_panel/lime", 1371, 1),
        ("mekanismadditions:glow_panel/magenta", 1372, 1),
        ("mekanismadditions:glow_panel/orange", 1373, 1),
        ("mekanismadditions:glow_panel/pink", 1374, 1),
        ("mekanismadditions:glow_panel/purple", 1375, 1),
        ("mekanismadditions:glow_panel/recolor/black", 1376, 1),
        ("mekanismadditions:glow_panel/recolor/blue", 1377, 1),
        ("mekanismadditions:glow_panel/recolor/brown", 1378, 1),
        ("mekanismadditions:glow_panel/recolor/cyan", 1379, 1),
        ("mekanismadditions:glow_panel/recolor/gray", 1380, 1),
        ("mekanismadditions:glow_panel/recolor/green", 1381, 1),
        ("mekanismadditions:glow_panel/recolor/light_blue", 1382, 1),
        ("mekanismadditions:glow_panel/recolor/light_gray", 1383, 1),
        ("mekanismadditions:glow_panel/recolor/lime", 1384, 1),
        ("mekanismadditions:glow_panel/recolor/magenta", 1385, 1),
        ("mekanismadditions:glow_panel/recolor/orange", 1386, 1),
        ("mekanismadditions:glow_panel/recolor/painting/aqua", 1387, 1),
        ("mekanismadditions:glow_panel/recolor/painting/black", 1388, 1),
        ("mekanismadditions:glow_panel/recolor/painting/blue", 1389, 1),
        ("mekanismadditions:glow_panel/recolor/painting/brown", 1390, 1),
        ("mekanismadditions:glow_panel/recolor/painting/cyan", 1391, 1),
        ("mekanismadditions:glow_panel/recolor/painting/dark_red", 1392, 1),
        ("mekanismadditions:glow_panel/recolor/painting/gray", 1393, 1),
        ("mekanismadditions:glow_panel/recolor/painting/green", 1394, 1),
        ("mekanismadditions:glow_panel/recolor/painting/light_blue", 1395, 1),
        ("mekanismadditions:glow_panel/recolor/painting/light_gray", 1396, 1),
        ("mekanismadditions:glow_panel/recolor/painting/lime", 1397, 1),
        ("mekanismadditions:glow_panel/recolor/painting/magenta", 1398, 1),
        ("mekanismadditions:glow_panel/recolor/painting/orange", 1399, 1),
        ("mekanismadditions:glow_panel/recolor/painting/pink", 1400, 1),
        ("mekanismadditions:glow_panel/recolor/painting/purple", 1401, 1),
        ("mekanismadditions:glow_panel/recolor/painting/red", 1402, 1),
        ("mekanismadditions:glow_panel/recolor/painting/white", 1403, 1),
        ("mekanismadditions:glow_panel/recolor/painting/yellow", 1404, 1),
        ("mekanismadditions:glow_panel/recolor/pink", 1405, 1),
        ("mekanismadditions:glow_panel/recolor/purple", 1406, 1),
        ("mekanismadditions:glow_panel/recolor/red", 1407, 1),
        ("mekanismadditions:glow_panel/recolor/white", 1408, 1),
        ("mekanismadditions:glow_panel/recolor/yellow", 1409, 1),
        ("mekanismadditions:glow_panel/red", 1410, 1),
        ("mekanismadditions:glow_panel/white", 1411, 1),
        ("mekanismadditions:glow_panel/yellow", 1412, 1),
        ("minecraft:glowstone", 1413, 1),
        ("minecraft:gold_block", 1414, 1),
        ("minecraft:gold_ingot_from_blasting_deepslate_gold_ore", 1415, 1),
        ("minecraft:gold_ingot_from_blasting_gold_ore", 1416, 1),
        ("minecraft:gold_ingot_from_blasting_nether_gold_ore", 1417, 1),
        ("minecraft:gold_ingot_from_blasting_raw_gold", 1418, 1),
        ("minecraft:gold_ingot_from_gold_block", 1419, 1),
        ("minecraft:gold_ingot_from_nuggets", 1420, 1),
        ("minecraft:gold_ingot_from_smelting_deepslate_gold_ore", 1421, 1),
        ("minecraft:gold_ingot_from_smelting_gold_ore", 1422, 1),
        ("minecraft:gold_ingot_from_smelting_nether_gold_ore", 1423, 1),
        ("minecraft:gold_ingot_from_smelting_raw_gold", 1424, 1),
        ("minecraft:gold_nugget", 1425, 1),
        ("mekanismtools:gold_nugget_from_blasting", 1426, 1),
        ("minecraft:gold_nugget_from_blasting", 1427, 1),
        ("mekanismtools:gold_nugget_from_smelting", 1428, 1),
        ("minecraft:gold_nugget_from_smelting", 1429, 1),
        ("mekanismtools:gold_paxel", 1430, 1),
        ("minecraft:golden_apple", 1431, 1),
        ("minecraft:golden_axe", 1432, 1),
        ("minecraft:golden_boots", 1433, 1),
        ("minecraft:golden_carrot", 1434, 1),
        ("minecraft:golden_chestplate", 1435, 1),
        ("minecraft:golden_dandelion", 1436, 1),
        ("minecraft:golden_helmet", 1437, 1),
        ("minecraft:golden_hoe", 1438, 1),
        ("minecraft:golden_leggings", 1439, 1),
        ("minecraft:golden_pickaxe", 1440, 1),
        ("minecraft:golden_shovel", 1441, 1),
        ("minecraft:golden_spear", 1442, 1),
        ("minecraft:golden_sword", 1443, 1),
        ("minecraft:granite", 1444, 1),
        ("minecraft:granite_slab", 1445, 1),
        ("minecraft:granite_slab_from_granite_stonecutting", 1446, 1),
        ("minecraft:granite_stairs", 1447, 1),
        ("minecraft:granite_stairs_from_granite_stonecutting", 1448, 1),
        ("minecraft:granite_wall", 1449, 1),
        ("minecraft:granite_wall_from_granite_stonecutting", 1450, 1),
        ("mekanism:gravitational_modulating_unit", 1451, 1),
        ("minecraft:gray_banner", 1452, 1),
        ("minecraft:gray_bed", 1453, 1),
        ("minecraft:gray_bundle", 1454, 1),
        ("minecraft:gray_candle", 1455, 1),
        ("minecraft:gray_carpet", 1456, 1),
        ("minecraft:gray_concrete_powder", 1457, 1),
        ("minecraft:gray_concrete_slab", 1458, 1),
        ("minecraft:gray_concrete_slab_from_gray_concrete_stonecutting", 1459, 1),
        ("minecraft:gray_concrete_stairs", 1460, 1),
        ("minecraft:gray_concrete_stairs_from_gray_concrete_stonecutting", 1461, 1),
        ("minecraft:gray_cushion", 1462, 1),
        ("minecraft:gray_dye", 1463, 1),
        ("minecraft:gray_dye_from_closed_eyeblossom", 1464, 1),
        ("minecraft:gray_glazed_terracotta", 1465, 1),
        ("minecraft:gray_harness", 1466, 1),
        ("minecraft:gray_shulker_box", 1467, 1),
        ("minecraft:gray_stained_glass", 1468, 1),
        ("minecraft:gray_stained_glass_pane", 1469, 1),
        ("minecraft:gray_stained_glass_pane_from_glass_pane", 1470, 1),
        ("minecraft:gray_terracotta", 1471, 1),
        ("minecraft:gray_wool_slab", 1472, 1),
        ("minecraft:gray_wool_stairs", 1473, 1),
        ("minecraft:green_banner", 1474, 1),
        ("minecraft:green_bed", 1475, 1),
        ("minecraft:green_bundle", 1476, 1),
        ("minecraft:green_candle", 1477, 1),
        ("minecraft:green_carpet", 1478, 1),
        ("minecraft:green_concrete_powder", 1479, 1),
        ("minecraft:green_concrete_slab", 1480, 1),
        ("minecraft:green_concrete_slab_from_green_concrete_stonecutting", 1481, 1),
        ("minecraft:green_concrete_stairs", 1482, 1),
        ("minecraft:green_concrete_stairs_from_green_concrete_stonecutting", 1483, 1),
        ("minecraft:green_cushion", 1484, 1),
        ("minecraft:green_dye", 1485, 1),
        ("minecraft:green_glazed_terracotta", 1486, 1),
        ("minecraft:green_harness", 1487, 1),
        ("minecraft:green_shulker_box", 1488, 1),
        ("minecraft:green_stained_glass", 1489, 1),
        ("minecraft:green_stained_glass_pane", 1490, 1),
        ("minecraft:green_stained_glass_pane_from_glass_pane", 1491, 1),
        ("minecraft:green_terracotta", 1492, 1),
        ("minecraft:green_wool_slab", 1493, 1),
        ("minecraft:green_wool_stairs", 1494, 1),
        ("minecraft:grindstone", 1495, 1),
        ("mekanism:gyroscopic_stabilization_unit", 1496, 1),
        ("minecraft:hay_block", 1497, 1),
        ("mekanism:hazmat_boots", 1498, 1),
        ("mekanism:hazmat_gown", 1499, 1),
        ("mekanism:hazmat_mask", 1500, 1),
        ("mekanism:hazmat_pants", 1501, 1),
        ("mekanism:hdpe_elytra", 1502, 1),
        ("mekanism:hdpe_rod", 1503, 1),
        ("mekanism:hdpe_stick", 1504, 1),
        ("minecraft:heavy_weighted_pressure_plate", 1505, 1),
        ("mekanismgenerators:hohlraum", 1506, 1),
        ("minecraft:honey_block", 1507, 1),
        ("minecraft:honey_bottle", 1508, 1),
        ("minecraft:honeycomb_block", 1509, 1),
        ("minecraft:hopper", 1510, 1),
        ("minecraft:hopper_minecart", 1511, 1),
        ("minecraft:host_armor_trim_smithing_template", 1512, 1),
        ("minecraft:host_armor_trim_smithing_template_smithing_trim", 1513, 1),
        ("mekanism:hydraulic_propulsion_unit", 1514, 1),
        ("mekanism:hydrostatic_repulsor_unit", 1515, 1),
        ("mekanism:induction/casing", 1516, 1),
        ("mekanism:induction/cell/advanced", 1517, 1),
        ("mekanism:induction/cell/basic", 1518, 1),
        ("mekanism:induction/cell/elite", 1519, 1),
        ("mekanism:induction/cell/ultimate", 1520, 1),
        ("mekanism:induction/port", 1521, 1),
        ("mekanism:induction/provider/advanced", 1522, 1),
        ("mekanism:induction/provider/basic", 1523, 1),
        ("mekanism:induction/provider/elite", 1524, 1),
        ("mekanism:induction/provider/ultimate", 1525, 1),
        ("mekanism:industrial_alarm", 1526, 1),
        ("mekanism:inhalation_purification_unit", 1527, 1),
        ("mekanism:injecting/brick_to_clay_ball", 1528, 1),
        ("mekanism:injecting/concrete/black", 1529, 1),
        ("mekanism:injecting/concrete/blue", 1530, 1),
        ("mekanism:injecting/concrete/brown", 1531, 1),
        ("mekanism:injecting/concrete/cyan", 1532, 1),
        ("mekanism:injecting/concrete/gray", 1533, 1),
        ("mekanism:injecting/concrete/green", 1534, 1),
        ("mekanism:injecting/concrete/light_blue", 1535, 1),
        ("mekanism:injecting/concrete/light_gray", 1536, 1),
        ("mekanism:injecting/concrete/lime", 1537, 1),
        ("mekanism:injecting/concrete/magenta", 1538, 1),
        ("mekanism:injecting/concrete/orange", 1539, 1),
        ("mekanism:injecting/concrete/pink", 1540, 1),
        ("mekanism:injecting/concrete/purple", 1541, 1),
        ("mekanism:injecting/concrete/red", 1542, 1),
        ("mekanism:injecting/concrete/white", 1543, 1),
        ("mekanism:injecting/concrete/yellow", 1544, 1),
        ("mekanism:injecting/coral/brain_coral", 1545, 1),
        ("mekanism:injecting/coral/brain_coral_block", 1546, 1),
        ("mekanism:injecting/coral/brain_coral_fan", 1547, 1),
        ("mekanism:injecting/coral/bubble_coral", 1548, 1),
        ("mekanism:injecting/coral/bubble_coral_block", 1549, 1),
        ("mekanism:injecting/coral/bubble_coral_fan", 1550, 1),
        ("mekanism:injecting/coral/fire_coral", 1551, 1),
        ("mekanism:injecting/coral/fire_coral_block", 1552, 1),
        ("mekanism:injecting/coral/fire_coral_fan", 1553, 1),
        ("mekanism:injecting/coral/horn_coral", 1554, 1),
        ("mekanism:injecting/coral/horn_coral_block", 1555, 1),
        ("mekanism:injecting/coral/horn_coral_fan", 1556, 1),
        ("mekanism:injecting/coral/tube_coral", 1557, 1),
        ("mekanism:injecting/coral/tube_coral_block", 1558, 1),
        ("mekanism:injecting/coral/tube_coral_fan", 1559, 1),
        ("mekanism:injecting/dirt_to_mud", 1560, 1),
        ("mekanism:injecting/gunpowder_to_sulfur", 1561, 1),
        ("mekanism:injecting/oxidizing/exposed_chiseled_copper", 1562, 1),
        ("mekanism:injecting/oxidizing/exposed_copper", 1563, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_bars", 1564, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_bulb", 1565, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_chain", 1566, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_chest", 1567, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_door", 1568, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_golem_statue", 1569, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_grate", 1570, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_lantern", 1571, 1),
        ("mekanism:injecting/oxidizing/exposed_copper_trapdoor", 1572, 1),
        ("mekanism:injecting/oxidizing/exposed_cut_copper", 1573, 1),
        ("mekanism:injecting/oxidizing/exposed_cut_copper_slab", 1574, 1),
        ("mekanism:injecting/oxidizing/exposed_cut_copper_stairs", 1575, 1),
        ("mekanism:injecting/oxidizing/exposed_lightning_rod", 1576, 1),
        ("mekanism:injecting/oxidizing/oxidized_chiseled_copper", 1577, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper", 1578, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_bars", 1579, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_bulb", 1580, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_chain", 1581, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_chest", 1582, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_door", 1583, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_golem_statue", 1584, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_grate", 1585, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_lantern", 1586, 1),
        ("mekanism:injecting/oxidizing/oxidized_copper_trapdoor", 1587, 1),
        ("mekanism:injecting/oxidizing/oxidized_cut_copper", 1588, 1),
        ("mekanism:injecting/oxidizing/oxidized_cut_copper_slab", 1589, 1),
        ("mekanism:injecting/oxidizing/oxidized_cut_copper_stairs", 1590, 1),
        ("mekanism:injecting/oxidizing/oxidized_lightning_rod", 1591, 1),
        ("mekanism:injecting/oxidizing/weathered_chiseled_copper", 1592, 1),
        ("mekanism:injecting/oxidizing/weathered_copper", 1593, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_bars", 1594, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_bulb", 1595, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_chain", 1596, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_chest", 1597, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_door", 1598, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_golem_statue", 1599, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_grate", 1600, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_lantern", 1601, 1),
        ("mekanism:injecting/oxidizing/weathered_copper_trapdoor", 1602, 1),
        ("mekanism:injecting/oxidizing/weathered_cut_copper", 1603, 1),
        ("mekanism:injecting/oxidizing/weathered_cut_copper_slab", 1604, 1),
        ("mekanism:injecting/oxidizing/weathered_cut_copper_stairs", 1605, 1),
        ("mekanism:injecting/oxidizing/weathered_lightning_rod", 1606, 1),
        ("mekanism:injecting/terracotta_to_clay", 1607, 1),
        ("minecraft:iron_axe", 1608, 1),
        ("minecraft:iron_bars", 1609, 1),
        ("minecraft:iron_block", 1610, 1),
        ("minecraft:iron_boots", 1611, 1),
        ("minecraft:iron_chain", 1612, 1),
        ("minecraft:iron_chestplate", 1613, 1),
        ("minecraft:iron_door", 1614, 1),
        ("minecraft:iron_helmet", 1615, 1),
        ("minecraft:iron_hoe", 1616, 1),
        ("minecraft:iron_ingot_from_blasting_deepslate_iron_ore", 1617, 1),
        ("minecraft:iron_ingot_from_blasting_iron_ore", 1618, 1),
        ("minecraft:iron_ingot_from_blasting_raw_iron", 1619, 1),
        ("minecraft:iron_ingot_from_iron_block", 1620, 1),
        ("minecraft:iron_ingot_from_nuggets", 1621, 1),
        ("minecraft:iron_ingot_from_smelting_deepslate_iron_ore", 1622, 1),
        ("minecraft:iron_ingot_from_smelting_iron_ore", 1623, 1),
        ("minecraft:iron_ingot_from_smelting_raw_iron", 1624, 1),
        ("minecraft:iron_leggings", 1625, 1),
        ("minecraft:iron_nugget", 1626, 1),
        ("mekanismtools:iron_nugget_from_blasting", 1627, 1),
        ("minecraft:iron_nugget_from_blasting", 1628, 1),
        ("mekanismtools:iron_nugget_from_smelting", 1629, 1),
        ("minecraft:iron_nugget_from_smelting", 1630, 1),
        ("mekanismtools:iron_paxel", 1631, 1),
        ("minecraft:iron_pickaxe", 1632, 1),
        ("minecraft:iron_shovel", 1633, 1),
        ("minecraft:iron_spear", 1634, 1),
        ("minecraft:iron_sword", 1635, 1),
        ("minecraft:iron_trapdoor", 1636, 1),
        ("mekanism:isotopic_centrifuge", 1637, 1),
        ("minecraft:item_frame", 1638, 1),
        ("minecraft:jack_o_lantern", 1639, 1),
        ("mekanism:jetpack", 1640, 1),
        ("mekanism:jetpack_armored", 1641, 1),
        ("mekanism:jetpack_unit", 1642, 1),
        ("minecraft:jukebox", 1643, 1),
        ("minecraft:jungle_boat", 1644, 1),
        ("minecraft:jungle_button", 1645, 1),
        ("minecraft:jungle_chest_boat", 1646, 1),
        ("minecraft:jungle_door", 1647, 1),
        ("minecraft:jungle_fence", 1648, 1),
        ("minecraft:jungle_fence_gate", 1649, 1),
        ("minecraft:jungle_hanging_sign", 1650, 1),
        ("minecraft:jungle_planks", 1651, 1),
        ("minecraft:jungle_pressure_plate", 1652, 1),
        ("minecraft:jungle_shelf", 1653, 1),
        ("minecraft:jungle_sign", 1654, 1),
        ("minecraft:jungle_slab", 1655, 1),
        ("minecraft:jungle_stairs", 1656, 1),
        ("minecraft:jungle_trapdoor", 1657, 1),
        ("minecraft:jungle_wood", 1658, 1),
        ("minecraft:ladder", 1659, 1),
        ("minecraft:lantern", 1660, 1),
        ("minecraft:lapis_block", 1661, 1),
        ("minecraft:lapis_lazuli", 1662, 1),
        ("mekanismtools:lapis_lazuli/armor/boots", 1663, 1),
        ("mekanismtools:lapis_lazuli/armor/chestplate", 1664, 1),
        ("mekanismtools:lapis_lazuli/armor/helmet", 1665, 1),
        ("mekanismtools:lapis_lazuli/armor/horse", 1666, 1),
        ("mekanismtools:lapis_lazuli/armor/leggings", 1667, 1),
        ("mekanismtools:lapis_lazuli/armor/nautilus", 1668, 1),
        ("mekanismtools:lapis_lazuli/shield", 1669, 1),
        ("mekanismtools:lapis_lazuli/tools/axe", 1670, 1),
        ("mekanismtools:lapis_lazuli/tools/hoe", 1671, 1),
        ("mekanismtools:lapis_lazuli/tools/paxel", 1672, 1),
        ("mekanismtools:lapis_lazuli/tools/pickaxe", 1673, 1),
        ("mekanismtools:lapis_lazuli/tools/shovel", 1674, 1),
        ("mekanismtools:lapis_lazuli/tools/spear", 1675, 1),
        ("mekanismtools:lapis_lazuli/tools/sword", 1676, 1),
        ("minecraft:lapis_lazuli_from_blasting_deepslate_lapis_ore", 1677, 1),
        ("minecraft:lapis_lazuli_from_blasting_lapis_ore", 1678, 1),
        ("minecraft:lapis_lazuli_from_smelting_deepslate_lapis_ore", 1679, 1),
        ("minecraft:lapis_lazuli_from_smelting_lapis_ore", 1680, 1),
        ("mekanism:laser", 1681, 1),
        ("mekanism:laser_amplifier", 1682, 1),
        ("mekanism:laser_dissipation_unit", 1683, 1),
        ("mekanismgenerators:laser_focus_matrix", 1684, 1),
        ("mekanism:laser_tractor_beam", 1685, 1),
        ("minecraft:lead", 1686, 1),
        ("minecraft:leaf_litter", 1687, 1),
        ("minecraft:leather", 1688, 1),
        ("minecraft:leather_boots", 1689, 1),
        ("minecraft:leather_boots_dyed", 1690, 1),
        ("minecraft:leather_chestplate", 1691, 1),
        ("minecraft:leather_chestplate_dyed", 1692, 1),
        ("minecraft:leather_helmet", 1693, 1),
        ("minecraft:leather_helmet_dyed", 1694, 1),
        ("minecraft:leather_horse_armor", 1695, 1),
        ("minecraft:leather_horse_armor_dyed", 1696, 1),
        ("minecraft:leather_leggings", 1697, 1),
        ("minecraft:leather_leggings_dyed", 1698, 1),
        ("minecraft:lectern", 1699, 1),
        ("minecraft:lever", 1700, 1),
        ("minecraft:light_blue_banner", 1701, 1),
        ("minecraft:light_blue_bed", 1702, 1),
        ("minecraft:light_blue_bundle", 1703, 1),
        ("minecraft:light_blue_candle", 1704, 1),
        ("minecraft:light_blue_carpet", 1705, 1),
        ("minecraft:light_blue_concrete_powder", 1706, 1),
        ("minecraft:light_blue_concrete_slab", 1707, 1),
        ("minecraft:light_blue_concrete_slab_from_light_blue_concrete_stonecutting", 1708, 1),
        ("minecraft:light_blue_concrete_stairs", 1709, 1),
        ("minecraft:light_blue_concrete_stairs_from_light_blue_concrete_stonecutting", 1710, 1),
        ("minecraft:light_blue_cushion", 1711, 1),
        ("minecraft:light_blue_dye_from_blue_orchid", 1712, 1),
        ("minecraft:light_blue_dye_from_blue_white_dye", 1713, 1),
        ("minecraft:light_blue_glazed_terracotta", 1714, 1),
        ("minecraft:light_blue_harness", 1715, 1),
        ("minecraft:light_blue_shulker_box", 1716, 1),
        ("minecraft:light_blue_stained_glass", 1717, 1),
        ("minecraft:light_blue_stained_glass_pane", 1718, 1),
        ("minecraft:light_blue_stained_glass_pane_from_glass_pane", 1719, 1),
        ("minecraft:light_blue_terracotta", 1720, 1),
        ("minecraft:light_blue_wool_slab", 1721, 1),
        ("minecraft:light_blue_wool_stairs", 1722, 1),
        ("minecraft:light_gray_banner", 1723, 1),
        ("minecraft:light_gray_bed", 1724, 1),
        ("minecraft:light_gray_bundle", 1725, 1),
        ("minecraft:light_gray_candle", 1726, 1),
        ("minecraft:light_gray_carpet", 1727, 1),
        ("minecraft:light_gray_concrete_powder", 1728, 1),
        ("minecraft:light_gray_concrete_slab", 1729, 1),
        ("minecraft:light_gray_concrete_slab_from_light_gray_concrete_stonecutting", 1730, 1),
        ("minecraft:light_gray_concrete_stairs", 1731, 1),
        ("minecraft:light_gray_concrete_stairs_from_light_gray_concrete_stonecutting", 1732, 1),
        ("minecraft:light_gray_cushion", 1733, 1),
        ("minecraft:light_gray_dye_from_azure_bluet", 1734, 1),
        ("minecraft:light_gray_dye_from_black_white_dye", 1735, 1),
        ("minecraft:light_gray_dye_from_gray_white_dye", 1736, 1),
        ("minecraft:light_gray_dye_from_oxeye_daisy", 1737, 1),
        ("minecraft:light_gray_dye_from_white_tulip", 1738, 1),
        ("minecraft:light_gray_glazed_terracotta", 1739, 1),
        ("minecraft:light_gray_harness", 1740, 1),
        ("minecraft:light_gray_shulker_box", 1741, 1),
        ("minecraft:light_gray_stained_glass", 1742, 1),
        ("minecraft:light_gray_stained_glass_pane", 1743, 1),
        ("minecraft:light_gray_stained_glass_pane_from_glass_pane", 1744, 1),
        ("minecraft:light_gray_terracotta", 1745, 1),
        ("minecraft:light_gray_wool_slab", 1746, 1),
        ("minecraft:light_gray_wool_stairs", 1747, 1),
        ("minecraft:light_weighted_pressure_plate", 1748, 1),
        ("minecraft:lightning_rod", 1749, 1),
        ("minecraft:lime_banner", 1750, 1),
        ("minecraft:lime_bed", 1751, 1),
        ("minecraft:lime_bundle", 1752, 1),
        ("minecraft:lime_candle", 1753, 1),
        ("minecraft:lime_carpet", 1754, 1),
        ("minecraft:lime_concrete_powder", 1755, 1),
        ("minecraft:lime_concrete_slab", 1756, 1),
        ("minecraft:lime_concrete_slab_from_lime_concrete_stonecutting", 1757, 1),
        ("minecraft:lime_concrete_stairs", 1758, 1),
        ("minecraft:lime_concrete_stairs_from_lime_concrete_stonecutting", 1759, 1),
        ("minecraft:lime_cushion", 1760, 1),
        ("minecraft:lime_dye", 1761, 1),
        ("minecraft:lime_dye_from_smelting", 1762, 1),
        ("minecraft:lime_glazed_terracotta", 1763, 1),
        ("minecraft:lime_harness", 1764, 1),
        ("minecraft:lime_shulker_box", 1765, 1),
        ("minecraft:lime_stained_glass", 1766, 1),
        ("minecraft:lime_stained_glass_pane", 1767, 1),
        ("minecraft:lime_stained_glass_pane_from_glass_pane", 1768, 1),
        ("minecraft:lime_terracotta", 1769, 1),
        ("minecraft:lime_wool_slab", 1770, 1),
        ("minecraft:lime_wool_stairs", 1771, 1),
        ("mekanism:locomotive_boosting_unit", 1772, 1),
        ("minecraft:lodestone", 1773, 1),
        ("mekanism:logistical_sorter", 1774, 1),
        ("minecraft:loom", 1775, 1),
        ("minecraft:mace", 1776, 1),
        ("minecraft:magenta_banner", 1777, 1),
        ("minecraft:magenta_bed", 1778, 1),
        ("minecraft:magenta_bundle", 1779, 1),
        ("minecraft:magenta_candle", 1780, 1),
        ("minecraft:magenta_carpet", 1781, 1),
        ("minecraft:magenta_concrete_powder", 1782, 1),
        ("minecraft:magenta_concrete_slab", 1783, 1),
        ("minecraft:magenta_concrete_slab_from_magenta_concrete_stonecutting", 1784, 1),
        ("minecraft:magenta_concrete_stairs", 1785, 1),
        ("minecraft:magenta_concrete_stairs_from_magenta_concrete_stonecutting", 1786, 1),
        ("minecraft:magenta_cushion", 1787, 1),
        ("minecraft:magenta_dye_from_allium", 1788, 1),
        ("minecraft:magenta_dye_from_blue_red_pink", 1789, 1),
        ("minecraft:magenta_dye_from_blue_red_white_dye", 1790, 1),
        ("minecraft:magenta_dye_from_lilac", 1791, 1),
        ("minecraft:magenta_dye_from_purple_and_pink", 1792, 1),
        ("minecraft:magenta_glazed_terracotta", 1793, 1),
        ("minecraft:magenta_harness", 1794, 1),
        ("minecraft:magenta_shulker_box", 1795, 1),
        ("minecraft:magenta_stained_glass", 1796, 1),
        ("minecraft:magenta_stained_glass_pane", 1797, 1),
        ("minecraft:magenta_stained_glass_pane_from_glass_pane", 1798, 1),
        ("minecraft:magenta_terracotta", 1799, 1),
        ("minecraft:magenta_wool_slab", 1800, 1),
        ("minecraft:magenta_wool_stairs", 1801, 1),
        ("minecraft:magma_block", 1802, 1),
        ("minecraft:magma_cream", 1803, 1),
        ("mekanism:magnetic_attraction_unit", 1804, 1),
        ("minecraft:mangrove_boat", 1805, 1),
        ("minecraft:mangrove_button", 1806, 1),
        ("minecraft:mangrove_chest_boat", 1807, 1),
        ("minecraft:mangrove_door", 1808, 1),
        ("minecraft:mangrove_fence", 1809, 1),
        ("minecraft:mangrove_fence_gate", 1810, 1),
        ("minecraft:mangrove_hanging_sign", 1811, 1),
        ("minecraft:mangrove_planks", 1812, 1),
        ("minecraft:mangrove_pressure_plate", 1813, 1),
        ("minecraft:mangrove_shelf", 1814, 1),
        ("minecraft:mangrove_sign", 1815, 1),
        ("minecraft:mangrove_slab", 1816, 1),
        ("minecraft:mangrove_stairs", 1817, 1),
        ("minecraft:mangrove_trapdoor", 1818, 1),
        ("minecraft:mangrove_wood", 1819, 1),
        ("minecraft:map", 1820, 1),
        ("minecraft:map_cloning", 1821, 8),
        ("mekanism:meka_tool", 1829, 1),
        ("mekanism:mekasuit_bodyarmor", 1830, 1),
        ("mekanism:mekasuit_boots", 1831, 1),
        ("mekanism:mekasuit_helmet", 1832, 1),
        ("mekanism:mekasuit_pants", 1833, 1),
        ("minecraft:melon", 1834, 1),
        ("minecraft:melon_seeds", 1835, 1),
        ("mekanism:metallurgic_infuser", 1836, 1),
        ("mekanism:metallurgic_infusing/alloy/atomic", 1837, 1),
        ("mekanism:metallurgic_infusing/alloy/infused", 1838, 1),
        ("mekanism:metallurgic_infusing/alloy/reinforced", 1839, 1),
        ("mekanism:metallurgic_infusing/blackstone_to_gilded_blackstone", 1840, 1),
        ("mekanism:metallurgic_infusing/clay_to_slime_ball", 1841, 1),
        ("mekanism:metallurgic_infusing/clay_to_slime_block", 1842, 1),
        ("mekanism:metallurgic_infusing/crimson_nylium_to_warped_nylium", 1843, 1),
        ("mekanism:metallurgic_infusing/dirt_to_mycelium", 1844, 1),
        ("mekanism:metallurgic_infusing/dirt_to_podzol", 1845, 1),
        ("mekanism:metallurgic_infusing/mossy/cobblestone", 1846, 1),
        ("mekanism:metallurgic_infusing/mossy/cobblestone_slab", 1847, 1),
        ("mekanism:metallurgic_infusing/mossy/cobblestone_stairs", 1848, 1),
        ("mekanism:metallurgic_infusing/mossy/cobblestone_wall", 1849, 1),
        ("mekanism:metallurgic_infusing/mossy/stone_brick", 1850, 1),
        ("mekanism:metallurgic_infusing/mossy/stone_brick_slab", 1851, 1),
        ("mekanism:metallurgic_infusing/mossy/stone_brick_stairs", 1852, 1),
        ("mekanism:metallurgic_infusing/mossy/stone_brick_wall", 1853, 1),
        ("mekanism:metallurgic_infusing/netherrack_to_crimson_nylium", 1854, 1),
        ("mekanism:metallurgic_infusing/sand_to_dirt", 1855, 1),
        ("minecraft:minecart", 1856, 1),
        ("mekanism:modification_station", 1857, 1),
        ("mekanism:module_base", 1858, 1),
        ("minecraft:mojang_banner_pattern", 1859, 1),
        ("minecraft:moss_carpet", 1860, 1),
        ("minecraft:mossy_cobblestone_from_moss_block", 1861, 1),
        ("minecraft:mossy_cobblestone_from_vine", 1862, 1),
        ("minecraft:mossy_cobblestone_slab", 1863, 1),
        ("minecraft:mossy_cobblestone_slab_from_mossy_cobblestone_stonecutting", 1864, 1),
        ("minecraft:mossy_cobblestone_stairs", 1865, 1),
        ("minecraft:mossy_cobblestone_stairs_from_mossy_cobblestone_stonecutting", 1866, 1),
        ("minecraft:mossy_cobblestone_wall", 1867, 1),
        ("minecraft:mossy_cobblestone_wall_from_mossy_cobblestone_stonecutting", 1868, 1),
        ("minecraft:mossy_stone_brick_slab", 1869, 1),
        ("minecraft:mossy_stone_brick_slab_from_mossy_stone_bricks_stonecutting", 1870, 1),
        ("minecraft:mossy_stone_brick_stairs", 1871, 1),
        ("minecraft:mossy_stone_brick_stairs_from_mossy_stone_bricks_stonecutting", 1872, 1),
        ("minecraft:mossy_stone_brick_wall", 1873, 1),
        ("minecraft:mossy_stone_brick_wall_from_mossy_stone_bricks_stonecutting", 1874, 1),
        ("minecraft:mossy_stone_bricks_from_moss_block", 1875, 1),
        ("minecraft:mossy_stone_bricks_from_vine", 1876, 1),
        ("mekanism:motorized_servo_unit", 1877, 1),
        ("minecraft:mud_brick_slab", 1878, 1),
        ("minecraft:mud_brick_slab_from_mud_bricks_stonecutting", 1879, 1),
        ("minecraft:mud_brick_stairs", 1880, 1),
        ("minecraft:mud_brick_stairs_from_mud_bricks_stonecutting", 1881, 1),
        ("minecraft:mud_brick_wall", 1882, 1),
        ("minecraft:mud_brick_wall_from_mud_bricks_stonecutting", 1883, 1),
        ("minecraft:mud_bricks", 1884, 1),
        ("minecraft:muddy_mangrove_roots", 1885, 1),
        ("minecraft:mushroom_stew", 1886, 1),
        ("minecraft:music_disc_5", 1887, 1),
        ("minecraft:name_tag", 1888, 1),
        ("minecraft:nether_brick", 1889, 1),
        ("minecraft:nether_brick_fence", 1890, 1),
        ("minecraft:nether_brick_slab", 1891, 1),
        ("minecraft:nether_brick_slab_from_nether_bricks_stonecutting", 1892, 1),
        ("minecraft:nether_brick_stairs", 1893, 1),
        ("minecraft:nether_brick_stairs_from_nether_bricks_stonecutting", 1894, 1),
        ("minecraft:nether_brick_wall", 1895, 1),
        ("minecraft:nether_brick_wall_from_nether_bricks_stonecutting", 1896, 1),
        ("minecraft:nether_bricks", 1897, 1),
        ("minecraft:nether_wart_block", 1898, 1),
        ("minecraft:netherite_axe_smithing", 1899, 1),
        ("minecraft:netherite_block", 1900, 1),
        ("minecraft:netherite_boots_smithing", 1901, 1),
        ("minecraft:netherite_chestplate_smithing", 1902, 1),
        ("minecraft:netherite_helmet_smithing", 1903, 1),
        ("minecraft:netherite_hoe_smithing", 1904, 1),
        ("minecraft:netherite_horse_armor_smithing", 1905, 1),
        ("minecraft:netherite_ingot", 1906, 1),
        ("minecraft:netherite_ingot_from_netherite_block", 1907, 1),
        ("minecraft:netherite_leggings_smithing", 1908, 1),
        ("minecraft:netherite_nautilus_armor_smithing", 1909, 1),
        ("mekanismtools:netherite_paxel", 1910, 1),
        ("minecraft:netherite_pickaxe_smithing", 1911, 1),
        ("minecraft:netherite_scrap", 1912, 1),
        ("minecraft:netherite_scrap_from_blasting", 1913, 1),
        ("minecraft:netherite_shovel_smithing", 1914, 1),
        ("minecraft:netherite_spear_smithing", 1915, 1),
        ("minecraft:netherite_sword_smithing", 1916, 1),
        ("minecraft:netherite_upgrade_smithing_template", 1917, 1),
        ("mekanism:network_reader", 1918, 1),
        ("minecraft:note_block", 1919, 1),
        ("mekanism:nucleosynthesizing/chorus_flower", 1920, 1),
        ("mekanism:nucleosynthesizing/crossbow", 1921, 1),
        ("mekanism:nucleosynthesizing/crying_obsidian", 1922, 1),
        ("mekanism:nucleosynthesizing/diamond", 1923, 1),
        ("mekanism:nucleosynthesizing/dragon_egg", 1924, 1),
        ("mekanism:nucleosynthesizing/echo_shard", 1925, 1),
        ("mekanism:nucleosynthesizing/emerald", 1926, 1),
        ("mekanism:nucleosynthesizing/enchanted_golden_apple", 1927, 1),
        ("mekanism:nucleosynthesizing/end_crystal", 1928, 1),
        ("mekanism:nucleosynthesizing/ender_chest", 1929, 1),
        ("mekanism:nucleosynthesizing/glowstone_block", 1930, 1),
        ("mekanism:nucleosynthesizing/heart_of_the_sea", 1931, 1),
        ("mekanism:nucleosynthesizing/iron", 1932, 1),
        ("mekanism:nucleosynthesizing/lapis_block", 1933, 1),
        ("mekanism:nucleosynthesizing/nautilus_shell", 1934, 1),
        ("mekanism:nucleosynthesizing/phantom_membrane", 1935, 1),
        ("mekanism:nucleosynthesizing/quartz_block", 1936, 1),
        ("mekanism:nucleosynthesizing/quartz_slab", 1937, 1),
        ("mekanism:nucleosynthesizing/quartz_stairs", 1938, 1),
        ("mekanism:nucleosynthesizing/redstone_block", 1939, 1),
        ("mekanism:nucleosynthesizing/respawn_anchor", 1940, 1),
        ("mekanism:nucleosynthesizing/trident", 1941, 1),
        ("mekanism:nucleosynthesizing/wither_skeleton_skull", 1942, 1),
        ("mekanism:nuggets/bronze", 1943, 1),
        ("mekanism:nuggets/refined_glowstone", 1944, 1),
        ("mekanism:nuggets/refined_obsidian", 1945, 1),
        ("mekanism:nuggets/steel", 1946, 1),
        ("mekanism:nutritional_injection_unit", 1947, 1),
        ("mekanism:nutritional_liquifier", 1948, 1),
        ("minecraft:oak_boat", 1949, 1),
        ("minecraft:oak_button", 1950, 1),
        ("minecraft:oak_chest_boat", 1951, 1),
        ("minecraft:oak_door", 1952, 1),
        ("minecraft:oak_fence", 1953, 1),
        ("minecraft:oak_fence_gate", 1954, 1),
        ("minecraft:oak_hanging_sign", 1955, 1),
        ("minecraft:oak_planks", 1956, 1),
        ("minecraft:oak_pressure_plate", 1957, 1),
        ("minecraft:oak_shelf", 1958, 1),
        ("minecraft:oak_sign", 1959, 1),
        ("minecraft:oak_slab", 1960, 1),
        ("minecraft:oak_stairs", 1961, 1),
        ("minecraft:oak_trapdoor", 1962, 1),
        ("minecraft:oak_wood", 1963, 1),
        ("minecraft:observer", 1964, 1),
        ("mekanismadditions:obsidian_tnt", 1965, 1),
        ("minecraft:orange_banner", 1966, 1),
        ("minecraft:orange_bed", 1967, 1),
        ("minecraft:orange_bundle", 1968, 1),
        ("minecraft:orange_candle", 1969, 1),
        ("minecraft:orange_carpet", 1970, 1),
        ("minecraft:orange_concrete_powder", 1971, 1),
        ("minecraft:orange_concrete_slab", 1972, 1),
        ("minecraft:orange_concrete_slab_from_orange_concrete_stonecutting", 1973, 1),
        ("minecraft:orange_concrete_stairs", 1974, 1),
        ("minecraft:orange_concrete_stairs_from_orange_concrete_stonecutting", 1975, 1),
        ("minecraft:orange_cushion", 1976, 1),
        ("minecraft:orange_dye_from_open_eyeblossom", 1977, 1),
        ("minecraft:orange_dye_from_orange_tulip", 1978, 1),
        ("minecraft:orange_dye_from_red_yellow", 1979, 1),
        ("minecraft:orange_dye_from_torchflower", 1980, 1),
        ("minecraft:orange_glazed_terracotta", 1981, 1),
        ("minecraft:orange_harness", 1982, 1),
        ("minecraft:orange_shulker_box", 1983, 1),
        ("minecraft:orange_stained_glass", 1984, 1),
        ("minecraft:orange_stained_glass_pane", 1985, 1),
        ("minecraft:orange_stained_glass_pane_from_glass_pane", 1986, 1),
        ("minecraft:orange_terracotta", 1987, 1),
        ("minecraft:orange_wool_slab", 1988, 1),
        ("minecraft:orange_wool_stairs", 1989, 1),
        ("mekanism:oredictionificator", 1990, 1),
        ("mekanismtools:osmium/armor/boots", 1991, 1),
        ("mekanismtools:osmium/armor/chestplate", 1992, 1),
        ("mekanismtools:osmium/armor/helmet", 1993, 1),
        ("mekanismtools:osmium/armor/horse", 1994, 1),
        ("mekanismtools:osmium/armor/leggings", 1995, 1),
        ("mekanismtools:osmium/armor/nautilus", 1996, 1),
        ("mekanismtools:osmium/nugget_from_blasting", 1997, 1),
        ("mekanismtools:osmium/nugget_from_smelting", 1998, 1),
        ("mekanismtools:osmium/shield", 1999, 1),
        ("mekanismtools:osmium/tools/axe", 2000, 1),
        ("mekanismtools:osmium/tools/hoe", 2001, 1),
        ("mekanismtools:osmium/tools/paxel", 2002, 1),
        ("mekanismtools:osmium/tools/pickaxe", 2003, 1),
        ("mekanismtools:osmium/tools/shovel", 2004, 1),
        ("mekanismtools:osmium/tools/spear", 2005, 1),
        ("mekanismtools:osmium/tools/sword", 2006, 1),
        ("mekanism:osmium_compressor", 2007, 1),
        ("minecraft:oxidized_chiseled_copper", 2008, 1),
        ("minecraft:oxidized_chiseled_copper_from_oxidized_copper_stonecutting", 2009, 1),
        ("minecraft:oxidized_chiseled_copper_from_oxidized_cut_copper_stonecutting", 2010, 1),
        ("minecraft:oxidized_copper_bulb", 2011, 1),
        ("minecraft:oxidized_copper_grate", 2012, 1),
        ("minecraft:oxidized_copper_grate_from_oxidized_copper_stonecutting", 2013, 1),
        ("minecraft:oxidized_cut_copper", 2014, 1),
        ("minecraft:oxidized_cut_copper_from_oxidized_copper_stonecutting", 2015, 1),
        ("minecraft:oxidized_cut_copper_slab", 2016, 1),
        ("minecraft:oxidized_cut_copper_slab_from_oxidized_copper_stonecutting", 2017, 1),
        ("minecraft:oxidized_cut_copper_slab_from_oxidized_cut_copper_stonecutting", 2018, 1),
        ("minecraft:oxidized_cut_copper_stairs", 2019, 1),
        ("minecraft:oxidized_cut_copper_stairs_from_oxidized_copper_stonecutting", 2020, 1),
        ("minecraft:oxidized_cut_copper_stairs_from_oxidized_cut_copper_stonecutting", 2021, 1),
        ("mekanism:oxidizing/bio/from_bio_fuel", 2022, 1),
        ("mekanism:oxidizing/bio/from_bio_fuel_block", 2023, 1),
        ("mekanism:oxidizing/brine", 2024, 1),
        ("mekanism:oxidizing/carbon/from_charcoal", 2025, 1),
        ("mekanism:oxidizing/carbon/from_charcoal_block", 2026, 1),
        ("mekanism:oxidizing/carbon/from_coal", 2027, 1),
        ("mekanism:oxidizing/carbon/from_coal_block", 2028, 1),
        ("mekanism:oxidizing/carbon/from_enriched", 2029, 1),
        ("mekanism:oxidizing/diamond/from_dust", 2030, 1),
        ("mekanism:oxidizing/diamond/from_enriched", 2031, 1),
        ("mekanism:oxidizing/fungi/from_mushrooms", 2032, 1),
        ("mekanism:oxidizing/gold/from_dust", 2033, 1),
        ("mekanism:oxidizing/gold/from_enriched", 2034, 1),
        ("mekanism:oxidizing/lithium", 2035, 1),
        ("mekanism:oxidizing/redstone/from_block", 2036, 1),
        ("mekanism:oxidizing/redstone/from_dust", 2037, 1),
        ("mekanism:oxidizing/redstone/from_enriched", 2038, 1),
        ("mekanism:oxidizing/refined_obsidian/from_dust", 2039, 1),
        ("mekanism:oxidizing/refined_obsidian/from_enriched", 2040, 1),
        ("mekanism:oxidizing/sulfur_dioxide", 2041, 1),
        ("mekanism:oxidizing/tin/from_dust", 2042, 1),
        ("mekanism:oxidizing/tin/from_enriched", 2043, 1),
        ("minecraft:packed_ice", 2044, 1),
        ("minecraft:packed_mud", 2045, 1),
        ("minecraft:painting", 2046, 1),
        ("mekanism:painting/banner/black", 2047, 1),
        ("mekanism:painting/banner/blue", 2048, 1),
        ("mekanism:painting/banner/brown", 2049, 1),
        ("mekanism:painting/banner/cyan", 2050, 1),
        ("mekanism:painting/banner/gray", 2051, 1),
        ("mekanism:painting/banner/green", 2052, 1),
        ("mekanism:painting/banner/light_blue", 2053, 1),
        ("mekanism:painting/banner/light_gray", 2054, 1),
        ("mekanism:painting/banner/lime", 2055, 1),
        ("mekanism:painting/banner/magenta", 2056, 1),
        ("mekanism:painting/banner/orange", 2057, 1),
        ("mekanism:painting/banner/pink", 2058, 1),
        ("mekanism:painting/banner/purple", 2059, 1),
        ("mekanism:painting/banner/red", 2060, 1),
        ("mekanism:painting/banner/white", 2061, 1),
        ("mekanism:painting/banner/yellow", 2062, 1),
        ("mekanism:painting/bed/black", 2063, 1),
        ("mekanism:painting/bed/blue", 2064, 1),
        ("mekanism:painting/bed/brown", 2065, 1),
        ("mekanism:painting/bed/cyan", 2066, 1),
        ("mekanism:painting/bed/gray", 2067, 1),
        ("mekanism:painting/bed/green", 2068, 1),
        ("mekanism:painting/bed/light_blue", 2069, 1),
        ("mekanism:painting/bed/light_gray", 2070, 1),
        ("mekanism:painting/bed/lime", 2071, 1),
        ("mekanism:painting/bed/magenta", 2072, 1),
        ("mekanism:painting/bed/orange", 2073, 1),
        ("mekanism:painting/bed/pink", 2074, 1),
        ("mekanism:painting/bed/purple", 2075, 1),
        ("mekanism:painting/bed/red", 2076, 1),
        ("mekanism:painting/bed/white", 2077, 1),
        ("mekanism:painting/bed/yellow", 2078, 1),
        ("mekanism:painting/candle/black", 2079, 1),
        ("mekanism:painting/candle/blue", 2080, 1),
        ("mekanism:painting/candle/brown", 2081, 1),
        ("mekanism:painting/candle/cyan", 2082, 1),
        ("mekanism:painting/candle/gray", 2083, 1),
        ("mekanism:painting/candle/green", 2084, 1),
        ("mekanism:painting/candle/light_blue", 2085, 1),
        ("mekanism:painting/candle/light_gray", 2086, 1),
        ("mekanism:painting/candle/lime", 2087, 1),
        ("mekanism:painting/candle/magenta", 2088, 1),
        ("mekanism:painting/candle/orange", 2089, 1),
        ("mekanism:painting/candle/pink", 2090, 1),
        ("mekanism:painting/candle/purple", 2091, 1),
        ("mekanism:painting/candle/red", 2092, 1),
        ("mekanism:painting/candle/white", 2093, 1),
        ("mekanism:painting/candle/yellow", 2094, 1),
        ("mekanism:painting/carpet/black", 2095, 1),
        ("mekanism:painting/carpet/blue", 2096, 1),
        ("mekanism:painting/carpet/brown", 2097, 1),
        ("mekanism:painting/carpet/cyan", 2098, 1),
        ("mekanism:painting/carpet/gray", 2099, 1),
        ("mekanism:painting/carpet/green", 2100, 1),
        ("mekanism:painting/carpet/light_blue", 2101, 1),
        ("mekanism:painting/carpet/light_gray", 2102, 1),
        ("mekanism:painting/carpet/lime", 2103, 1),
        ("mekanism:painting/carpet/magenta", 2104, 1),
        ("mekanism:painting/carpet/orange", 2105, 1),
        ("mekanism:painting/carpet/pink", 2106, 1),
        ("mekanism:painting/carpet/purple", 2107, 1),
        ("mekanism:painting/carpet/red", 2108, 1),
        ("mekanism:painting/carpet/white", 2109, 1),
        ("mekanism:painting/carpet/yellow", 2110, 1),
        ("mekanism:painting/concrete/black", 2111, 1),
        ("mekanism:painting/concrete/blue", 2112, 1),
        ("mekanism:painting/concrete/brown", 2113, 1),
        ("mekanism:painting/concrete/cyan", 2114, 1),
        ("mekanism:painting/concrete/gray", 2115, 1),
        ("mekanism:painting/concrete/green", 2116, 1),
        ("mekanism:painting/concrete/light_blue", 2117, 1),
        ("mekanism:painting/concrete/light_gray", 2118, 1),
        ("mekanism:painting/concrete/lime", 2119, 1),
        ("mekanism:painting/concrete/magenta", 2120, 1),
        ("mekanism:painting/concrete/orange", 2121, 1),
        ("mekanism:painting/concrete/pink", 2122, 1),
        ("mekanism:painting/concrete/purple", 2123, 1),
        ("mekanism:painting/concrete/red", 2124, 1),
        ("mekanism:painting/concrete/slabs/black", 2125, 1),
        ("mekanism:painting/concrete/slabs/blue", 2126, 1),
        ("mekanism:painting/concrete/slabs/brown", 2127, 1),
        ("mekanism:painting/concrete/slabs/cyan", 2128, 1),
        ("mekanism:painting/concrete/slabs/gray", 2129, 1),
        ("mekanism:painting/concrete/slabs/green", 2130, 1),
        ("mekanism:painting/concrete/slabs/light_blue", 2131, 1),
        ("mekanism:painting/concrete/slabs/light_gray", 2132, 1),
        ("mekanism:painting/concrete/slabs/lime", 2133, 1),
        ("mekanism:painting/concrete/slabs/magenta", 2134, 1),
        ("mekanism:painting/concrete/slabs/orange", 2135, 1),
        ("mekanism:painting/concrete/slabs/pink", 2136, 1),
        ("mekanism:painting/concrete/slabs/purple", 2137, 1),
        ("mekanism:painting/concrete/slabs/red", 2138, 1),
        ("mekanism:painting/concrete/slabs/white", 2139, 1),
        ("mekanism:painting/concrete/slabs/yellow", 2140, 1),
        ("mekanism:painting/concrete/stairs/black", 2141, 1),
        ("mekanism:painting/concrete/stairs/blue", 2142, 1),
        ("mekanism:painting/concrete/stairs/brown", 2143, 1),
        ("mekanism:painting/concrete/stairs/cyan", 2144, 1),
        ("mekanism:painting/concrete/stairs/gray", 2145, 1),
        ("mekanism:painting/concrete/stairs/green", 2146, 1),
        ("mekanism:painting/concrete/stairs/light_blue", 2147, 1),
        ("mekanism:painting/concrete/stairs/light_gray", 2148, 1),
        ("mekanism:painting/concrete/stairs/lime", 2149, 1),
        ("mekanism:painting/concrete/stairs/magenta", 2150, 1),
        ("mekanism:painting/concrete/stairs/orange", 2151, 1),
        ("mekanism:painting/concrete/stairs/pink", 2152, 1),
        ("mekanism:painting/concrete/stairs/purple", 2153, 1),
        ("mekanism:painting/concrete/stairs/red", 2154, 1),
        ("mekanism:painting/concrete/stairs/white", 2155, 1),
        ("mekanism:painting/concrete/stairs/yellow", 2156, 1),
        ("mekanism:painting/concrete/white", 2157, 1),
        ("mekanism:painting/concrete/yellow", 2158, 1),
        ("mekanism:painting/concrete_powder/black", 2159, 1),
        ("mekanism:painting/concrete_powder/blue", 2160, 1),
        ("mekanism:painting/concrete_powder/brown", 2161, 1),
        ("mekanism:painting/concrete_powder/cyan", 2162, 1),
        ("mekanism:painting/concrete_powder/gray", 2163, 1),
        ("mekanism:painting/concrete_powder/green", 2164, 1),
        ("mekanism:painting/concrete_powder/light_blue", 2165, 1),
        ("mekanism:painting/concrete_powder/light_gray", 2166, 1),
        ("mekanism:painting/concrete_powder/lime", 2167, 1),
        ("mekanism:painting/concrete_powder/magenta", 2168, 1),
        ("mekanism:painting/concrete_powder/orange", 2169, 1),
        ("mekanism:painting/concrete_powder/pink", 2170, 1),
        ("mekanism:painting/concrete_powder/purple", 2171, 1),
        ("mekanism:painting/concrete_powder/red", 2172, 1),
        ("mekanism:painting/concrete_powder/white", 2173, 1),
        ("mekanism:painting/concrete_powder/yellow", 2174, 1),
        ("mekanism:painting/cushion/black", 2175, 1),
        ("mekanism:painting/cushion/blue", 2176, 1),
        ("mekanism:painting/cushion/brown", 2177, 1),
        ("mekanism:painting/cushion/cyan", 2178, 1),
        ("mekanism:painting/cushion/gray", 2179, 1),
        ("mekanism:painting/cushion/green", 2180, 1),
        ("mekanism:painting/cushion/light_blue", 2181, 1),
        ("mekanism:painting/cushion/light_gray", 2182, 1),
        ("mekanism:painting/cushion/lime", 2183, 1),
        ("mekanism:painting/cushion/magenta", 2184, 1),
        ("mekanism:painting/cushion/orange", 2185, 1),
        ("mekanism:painting/cushion/pink", 2186, 1),
        ("mekanism:painting/cushion/purple", 2187, 1),
        ("mekanism:painting/cushion/red", 2188, 1),
        ("mekanism:painting/cushion/white", 2189, 1),
        ("mekanism:painting/cushion/yellow", 2190, 1),
        ("mekanism:painting/dye/black", 2191, 1),
        ("mekanism:painting/dye/blue", 2192, 1),
        ("mekanism:painting/dye/brown", 2193, 1),
        ("mekanism:painting/dye/cyan", 2194, 1),
        ("mekanism:painting/dye/gray", 2195, 1),
        ("mekanism:painting/dye/green", 2196, 1),
        ("mekanism:painting/dye/light_blue", 2197, 1),
        ("mekanism:painting/dye/light_gray", 2198, 1),
        ("mekanism:painting/dye/lime", 2199, 1),
        ("mekanism:painting/dye/magenta", 2200, 1),
        ("mekanism:painting/dye/orange", 2201, 1),
        ("mekanism:painting/dye/pink", 2202, 1),
        ("mekanism:painting/dye/purple", 2203, 1),
        ("mekanism:painting/dye/red", 2204, 1),
        ("mekanism:painting/dye/white", 2205, 1),
        ("mekanism:painting/dye/yellow", 2206, 1),
        ("mekanism:painting/glass/black", 2207, 1),
        ("mekanism:painting/glass/blue", 2208, 1),
        ("mekanism:painting/glass/brown", 2209, 1),
        ("mekanism:painting/glass/cyan", 2210, 1),
        ("mekanism:painting/glass/gray", 2211, 1),
        ("mekanism:painting/glass/green", 2212, 1),
        ("mekanism:painting/glass/light_blue", 2213, 1),
        ("mekanism:painting/glass/light_gray", 2214, 1),
        ("mekanism:painting/glass/lime", 2215, 1),
        ("mekanism:painting/glass/magenta", 2216, 1),
        ("mekanism:painting/glass/orange", 2217, 1),
        ("mekanism:painting/glass/pink", 2218, 1),
        ("mekanism:painting/glass/purple", 2219, 1),
        ("mekanism:painting/glass/red", 2220, 1),
        ("mekanism:painting/glass/white", 2221, 1),
        ("mekanism:painting/glass/yellow", 2222, 1),
        ("mekanism:painting/glass_pane/black", 2223, 1),
        ("mekanism:painting/glass_pane/blue", 2224, 1),
        ("mekanism:painting/glass_pane/brown", 2225, 1),
        ("mekanism:painting/glass_pane/cyan", 2226, 1),
        ("mekanism:painting/glass_pane/gray", 2227, 1),
        ("mekanism:painting/glass_pane/green", 2228, 1),
        ("mekanism:painting/glass_pane/light_blue", 2229, 1),
        ("mekanism:painting/glass_pane/light_gray", 2230, 1),
        ("mekanism:painting/glass_pane/lime", 2231, 1),
        ("mekanism:painting/glass_pane/magenta", 2232, 1),
        ("mekanism:painting/glass_pane/orange", 2233, 1),
        ("mekanism:painting/glass_pane/pink", 2234, 1),
        ("mekanism:painting/glass_pane/purple", 2235, 1),
        ("mekanism:painting/glass_pane/red", 2236, 1),
        ("mekanism:painting/glass_pane/white", 2237, 1),
        ("mekanism:painting/glass_pane/yellow", 2238, 1),
        ("mekanism:painting/harness/black", 2239, 1),
        ("mekanism:painting/harness/blue", 2240, 1),
        ("mekanism:painting/harness/brown", 2241, 1),
        ("mekanism:painting/harness/cyan", 2242, 1),
        ("mekanism:painting/harness/gray", 2243, 1),
        ("mekanism:painting/harness/green", 2244, 1),
        ("mekanism:painting/harness/light_blue", 2245, 1),
        ("mekanism:painting/harness/light_gray", 2246, 1),
        ("mekanism:painting/harness/lime", 2247, 1),
        ("mekanism:painting/harness/magenta", 2248, 1),
        ("mekanism:painting/harness/orange", 2249, 1),
        ("mekanism:painting/harness/pink", 2250, 1),
        ("mekanism:painting/harness/purple", 2251, 1),
        ("mekanism:painting/harness/red", 2252, 1),
        ("mekanism:painting/harness/white", 2253, 1),
        ("mekanism:painting/harness/yellow", 2254, 1),
        ("mekanism:painting/terracotta/black", 2255, 1),
        ("mekanism:painting/terracotta/blue", 2256, 1),
        ("mekanism:painting/terracotta/brown", 2257, 1),
        ("mekanism:painting/terracotta/cyan", 2258, 1),
        ("mekanism:painting/terracotta/glazed/black", 2259, 1),
        ("mekanism:painting/terracotta/glazed/blue", 2260, 1),
        ("mekanism:painting/terracotta/glazed/brown", 2261, 1),
        ("mekanism:painting/terracotta/glazed/cyan", 2262, 1),
        ("mekanism:painting/terracotta/glazed/gray", 2263, 1),
        ("mekanism:painting/terracotta/glazed/green", 2264, 1),
        ("mekanism:painting/terracotta/glazed/light_blue", 2265, 1),
        ("mekanism:painting/terracotta/glazed/light_gray", 2266, 1),
        ("mekanism:painting/terracotta/glazed/lime", 2267, 1),
        ("mekanism:painting/terracotta/glazed/magenta", 2268, 1),
        ("mekanism:painting/terracotta/glazed/orange", 2269, 1),
        ("mekanism:painting/terracotta/glazed/pink", 2270, 1),
        ("mekanism:painting/terracotta/glazed/purple", 2271, 1),
        ("mekanism:painting/terracotta/glazed/red", 2272, 1),
        ("mekanism:painting/terracotta/glazed/white", 2273, 1),
        ("mekanism:painting/terracotta/glazed/yellow", 2274, 1),
        ("mekanism:painting/terracotta/gray", 2275, 1),
        ("mekanism:painting/terracotta/green", 2276, 1),
        ("mekanism:painting/terracotta/light_blue", 2277, 1),
        ("mekanism:painting/terracotta/light_gray", 2278, 1),
        ("mekanism:painting/terracotta/lime", 2279, 1),
        ("mekanism:painting/terracotta/magenta", 2280, 1),
        ("mekanism:painting/terracotta/orange", 2281, 1),
        ("mekanism:painting/terracotta/pink", 2282, 1),
        ("mekanism:painting/terracotta/purple", 2283, 1),
        ("mekanism:painting/terracotta/red", 2284, 1),
        ("mekanism:painting/terracotta/white", 2285, 1),
        ("mekanism:painting/terracotta/yellow", 2286, 1),
        ("mekanism:painting/wool/black", 2287, 1),
        ("mekanism:painting/wool/blue", 2288, 1),
        ("mekanism:painting/wool/brown", 2289, 1),
        ("mekanism:painting/wool/cyan", 2290, 1),
        ("mekanism:painting/wool/gray", 2291, 1),
        ("mekanism:painting/wool/green", 2292, 1),
        ("mekanism:painting/wool/light_blue", 2293, 1),
        ("mekanism:painting/wool/light_gray", 2294, 1),
        ("mekanism:painting/wool/lime", 2295, 1),
        ("mekanism:painting/wool/magenta", 2296, 1),
        ("mekanism:painting/wool/orange", 2297, 1),
        ("mekanism:painting/wool/pink", 2298, 1),
        ("mekanism:painting/wool/purple", 2299, 1),
        ("mekanism:painting/wool/red", 2300, 1),
        ("mekanism:painting/wool/slabs/black", 2301, 1),
        ("mekanism:painting/wool/slabs/blue", 2302, 1),
        ("mekanism:painting/wool/slabs/brown", 2303, 1),
        ("mekanism:painting/wool/slabs/cyan", 2304, 1),
        ("mekanism:painting/wool/slabs/gray", 2305, 1),
        ("mekanism:painting/wool/slabs/green", 2306, 1),
        ("mekanism:painting/wool/slabs/light_blue", 2307, 1),
        ("mekanism:painting/wool/slabs/light_gray", 2308, 1),
        ("mekanism:painting/wool/slabs/lime", 2309, 1),
        ("mekanism:painting/wool/slabs/magenta", 2310, 1),
        ("mekanism:painting/wool/slabs/orange", 2311, 1),
        ("mekanism:painting/wool/slabs/pink", 2312, 1),
        ("mekanism:painting/wool/slabs/purple", 2313, 1),
        ("mekanism:painting/wool/slabs/red", 2314, 1),
        ("mekanism:painting/wool/slabs/white", 2315, 1),
        ("mekanism:painting/wool/slabs/yellow", 2316, 1),
        ("mekanism:painting/wool/stairs/black", 2317, 1),
        ("mekanism:painting/wool/stairs/blue", 2318, 1),
        ("mekanism:painting/wool/stairs/brown", 2319, 1),
        ("mekanism:painting/wool/stairs/cyan", 2320, 1),
        ("mekanism:painting/wool/stairs/gray", 2321, 1),
        ("mekanism:painting/wool/stairs/green", 2322, 1),
        ("mekanism:painting/wool/stairs/light_blue", 2323, 1),
        ("mekanism:painting/wool/stairs/light_gray", 2324, 1),
        ("mekanism:painting/wool/stairs/lime", 2325, 1),
        ("mekanism:painting/wool/stairs/magenta", 2326, 1),
        ("mekanism:painting/wool/stairs/orange", 2327, 1),
        ("mekanism:painting/wool/stairs/pink", 2328, 1),
        ("mekanism:painting/wool/stairs/purple", 2329, 1),
        ("mekanism:painting/wool/stairs/red", 2330, 1),
        ("mekanism:painting/wool/stairs/white", 2331, 1),
        ("mekanism:painting/wool/stairs/yellow", 2332, 1),
        ("mekanism:painting/wool/white", 2333, 1),
        ("mekanism:painting/wool/yellow", 2334, 1),
        ("mekanism:painting_machine", 2335, 1),
        ("minecraft:pale_moss_carpet", 2336, 1),
        ("minecraft:pale_oak_boat", 2337, 1),
        ("minecraft:pale_oak_button", 2338, 1),
        ("minecraft:pale_oak_chest_boat", 2339, 1),
        ("minecraft:pale_oak_door", 2340, 1),
        ("minecraft:pale_oak_fence", 2341, 1),
        ("minecraft:pale_oak_fence_gate", 2342, 1),
        ("minecraft:pale_oak_hanging_sign", 2343, 1),
        ("minecraft:pale_oak_planks", 2344, 1),
        ("minecraft:pale_oak_pressure_plate", 2345, 1),
        ("minecraft:pale_oak_shelf", 2346, 1),
        ("minecraft:pale_oak_sign", 2347, 1),
        ("minecraft:pale_oak_slab", 2348, 1),
        ("minecraft:pale_oak_stairs", 2349, 1),
        ("minecraft:pale_oak_trapdoor", 2350, 1),
        ("minecraft:pale_oak_wood", 2351, 1),
        ("mekanism:paper", 2352, 1),
        ("minecraft:paper", 2353, 1),
        ("mekanism:personal_barrel", 2354, 1),
        ("mekanism:personal_chest", 2355, 1),
        ("mekanism:pigment_extracting/banner/black", 2356, 1),
        ("mekanism:pigment_extracting/banner/blue", 2357, 1),
        ("mekanism:pigment_extracting/banner/brown", 2358, 1),
        ("mekanism:pigment_extracting/banner/cyan", 2359, 1),
        ("mekanism:pigment_extracting/banner/gray", 2360, 1),
        ("mekanism:pigment_extracting/banner/green", 2361, 1),
        ("mekanism:pigment_extracting/banner/light_blue", 2362, 1),
        ("mekanism:pigment_extracting/banner/light_gray", 2363, 1),
        ("mekanism:pigment_extracting/banner/lime", 2364, 1),
        ("mekanism:pigment_extracting/banner/magenta", 2365, 1),
        ("mekanism:pigment_extracting/banner/orange", 2366, 1),
        ("mekanism:pigment_extracting/banner/pink", 2367, 1),
        ("mekanism:pigment_extracting/banner/purple", 2368, 1),
        ("mekanism:pigment_extracting/banner/red", 2369, 1),
        ("mekanism:pigment_extracting/banner/white", 2370, 1),
        ("mekanism:pigment_extracting/banner/yellow", 2371, 1),
        ("mekanism:pigment_extracting/candle/black", 2372, 1),
        ("mekanism:pigment_extracting/candle/blue", 2373, 1),
        ("mekanism:pigment_extracting/candle/brown", 2374, 1),
        ("mekanism:pigment_extracting/candle/cyan", 2375, 1),
        ("mekanism:pigment_extracting/candle/gray", 2376, 1),
        ("mekanism:pigment_extracting/candle/green", 2377, 1),
        ("mekanism:pigment_extracting/candle/light_blue", 2378, 1),
        ("mekanism:pigment_extracting/candle/light_gray", 2379, 1),
        ("mekanism:pigment_extracting/candle/lime", 2380, 1),
        ("mekanism:pigment_extracting/candle/magenta", 2381, 1),
        ("mekanism:pigment_extracting/candle/orange", 2382, 1),
        ("mekanism:pigment_extracting/candle/pink", 2383, 1),
        ("mekanism:pigment_extracting/candle/purple", 2384, 1),
        ("mekanism:pigment_extracting/candle/red", 2385, 1),
        ("mekanism:pigment_extracting/candle/white", 2386, 1),
        ("mekanism:pigment_extracting/candle/yellow", 2387, 1),
        ("mekanism:pigment_extracting/carpet/black", 2388, 1),
        ("mekanism:pigment_extracting/carpet/blue", 2389, 1),
        ("mekanism:pigment_extracting/carpet/brown", 2390, 1),
        ("mekanism:pigment_extracting/carpet/cyan", 2391, 1),
        ("mekanism:pigment_extracting/carpet/gray", 2392, 1),
        ("mekanism:pigment_extracting/carpet/green", 2393, 1),
        ("mekanism:pigment_extracting/carpet/light_blue", 2394, 1),
        ("mekanism:pigment_extracting/carpet/light_gray", 2395, 1),
        ("mekanism:pigment_extracting/carpet/lime", 2396, 1),
        ("mekanism:pigment_extracting/carpet/magenta", 2397, 1),
        ("mekanism:pigment_extracting/carpet/orange", 2398, 1),
        ("mekanism:pigment_extracting/carpet/pink", 2399, 1),
        ("mekanism:pigment_extracting/carpet/purple", 2400, 1),
        ("mekanism:pigment_extracting/carpet/red", 2401, 1),
        ("mekanism:pigment_extracting/carpet/white", 2402, 1),
        ("mekanism:pigment_extracting/carpet/yellow", 2403, 1),
        ("mekanism:pigment_extracting/concrete/black", 2404, 1),
        ("mekanism:pigment_extracting/concrete/blue", 2405, 1),
        ("mekanism:pigment_extracting/concrete/brown", 2406, 1),
        ("mekanism:pigment_extracting/concrete/cyan", 2407, 1),
        ("mekanism:pigment_extracting/concrete/gray", 2408, 1),
        ("mekanism:pigment_extracting/concrete/green", 2409, 1),
        ("mekanism:pigment_extracting/concrete/light_blue", 2410, 1),
        ("mekanism:pigment_extracting/concrete/light_gray", 2411, 1),
        ("mekanism:pigment_extracting/concrete/lime", 2412, 1),
        ("mekanism:pigment_extracting/concrete/magenta", 2413, 1),
        ("mekanism:pigment_extracting/concrete/orange", 2414, 1),
        ("mekanism:pigment_extracting/concrete/pink", 2415, 1),
        ("mekanism:pigment_extracting/concrete/purple", 2416, 1),
        ("mekanism:pigment_extracting/concrete/red", 2417, 1),
        ("mekanism:pigment_extracting/concrete/slabs/black", 2418, 1),
        ("mekanism:pigment_extracting/concrete/slabs/blue", 2419, 1),
        ("mekanism:pigment_extracting/concrete/slabs/brown", 2420, 1),
        ("mekanism:pigment_extracting/concrete/slabs/cyan", 2421, 1),
        ("mekanism:pigment_extracting/concrete/slabs/gray", 2422, 1),
        ("mekanism:pigment_extracting/concrete/slabs/green", 2423, 1),
        ("mekanism:pigment_extracting/concrete/slabs/light_blue", 2424, 1),
        ("mekanism:pigment_extracting/concrete/slabs/light_gray", 2425, 1),
        ("mekanism:pigment_extracting/concrete/slabs/lime", 2426, 1),
        ("mekanism:pigment_extracting/concrete/slabs/magenta", 2427, 1),
        ("mekanism:pigment_extracting/concrete/slabs/orange", 2428, 1),
        ("mekanism:pigment_extracting/concrete/slabs/pink", 2429, 1),
        ("mekanism:pigment_extracting/concrete/slabs/purple", 2430, 1),
        ("mekanism:pigment_extracting/concrete/slabs/red", 2431, 1),
        ("mekanism:pigment_extracting/concrete/slabs/white", 2432, 1),
        ("mekanism:pigment_extracting/concrete/slabs/yellow", 2433, 1),
        ("mekanism:pigment_extracting/concrete/stairs/black", 2434, 1),
        ("mekanism:pigment_extracting/concrete/stairs/blue", 2435, 1),
        ("mekanism:pigment_extracting/concrete/stairs/brown", 2436, 1),
        ("mekanism:pigment_extracting/concrete/stairs/cyan", 2437, 1),
        ("mekanism:pigment_extracting/concrete/stairs/gray", 2438, 1),
        ("mekanism:pigment_extracting/concrete/stairs/green", 2439, 1),
        ("mekanism:pigment_extracting/concrete/stairs/light_blue", 2440, 1),
        ("mekanism:pigment_extracting/concrete/stairs/light_gray", 2441, 1),
        ("mekanism:pigment_extracting/concrete/stairs/lime", 2442, 1),
        ("mekanism:pigment_extracting/concrete/stairs/magenta", 2443, 1),
        ("mekanism:pigment_extracting/concrete/stairs/orange", 2444, 1),
        ("mekanism:pigment_extracting/concrete/stairs/pink", 2445, 1),
        ("mekanism:pigment_extracting/concrete/stairs/purple", 2446, 1),
        ("mekanism:pigment_extracting/concrete/stairs/red", 2447, 1),
        ("mekanism:pigment_extracting/concrete/stairs/white", 2448, 1),
        ("mekanism:pigment_extracting/concrete/stairs/yellow", 2449, 1),
        ("mekanism:pigment_extracting/concrete/white", 2450, 1),
        ("mekanism:pigment_extracting/concrete/yellow", 2451, 1),
        ("mekanism:pigment_extracting/concrete_powder/black", 2452, 1),
        ("mekanism:pigment_extracting/concrete_powder/blue", 2453, 1),
        ("mekanism:pigment_extracting/concrete_powder/brown", 2454, 1),
        ("mekanism:pigment_extracting/concrete_powder/cyan", 2455, 1),
        ("mekanism:pigment_extracting/concrete_powder/gray", 2456, 1),
        ("mekanism:pigment_extracting/concrete_powder/green", 2457, 1),
        ("mekanism:pigment_extracting/concrete_powder/light_blue", 2458, 1),
        ("mekanism:pigment_extracting/concrete_powder/light_gray", 2459, 1),
        ("mekanism:pigment_extracting/concrete_powder/lime", 2460, 1),
        ("mekanism:pigment_extracting/concrete_powder/magenta", 2461, 1),
        ("mekanism:pigment_extracting/concrete_powder/orange", 2462, 1),
        ("mekanism:pigment_extracting/concrete_powder/pink", 2463, 1),
        ("mekanism:pigment_extracting/concrete_powder/purple", 2464, 1),
        ("mekanism:pigment_extracting/concrete_powder/red", 2465, 1),
        ("mekanism:pigment_extracting/concrete_powder/white", 2466, 1),
        ("mekanism:pigment_extracting/concrete_powder/yellow", 2467, 1),
        ("mekanism:pigment_extracting/cushion/black", 2468, 1),
        ("mekanism:pigment_extracting/cushion/blue", 2469, 1),
        ("mekanism:pigment_extracting/cushion/brown", 2470, 1),
        ("mekanism:pigment_extracting/cushion/cyan", 2471, 1),
        ("mekanism:pigment_extracting/cushion/gray", 2472, 1),
        ("mekanism:pigment_extracting/cushion/green", 2473, 1),
        ("mekanism:pigment_extracting/cushion/light_blue", 2474, 1),
        ("mekanism:pigment_extracting/cushion/light_gray", 2475, 1),
        ("mekanism:pigment_extracting/cushion/lime", 2476, 1),
        ("mekanism:pigment_extracting/cushion/magenta", 2477, 1),
        ("mekanism:pigment_extracting/cushion/orange", 2478, 1),
        ("mekanism:pigment_extracting/cushion/pink", 2479, 1),
        ("mekanism:pigment_extracting/cushion/purple", 2480, 1),
        ("mekanism:pigment_extracting/cushion/red", 2481, 1),
        ("mekanism:pigment_extracting/cushion/white", 2482, 1),
        ("mekanism:pigment_extracting/cushion/yellow", 2483, 1),
        ("mekanism:pigment_extracting/dye/black", 2484, 1),
        ("mekanism:pigment_extracting/dye/blue", 2485, 1),
        ("mekanism:pigment_extracting/dye/brown", 2486, 1),
        ("mekanism:pigment_extracting/dye/cyan", 2487, 1),
        ("mekanism:pigment_extracting/dye/gray", 2488, 1),
        ("mekanism:pigment_extracting/dye/green", 2489, 1),
        ("mekanism:pigment_extracting/dye/light_blue", 2490, 1),
        ("mekanism:pigment_extracting/dye/light_gray", 2491, 1),
        ("mekanism:pigment_extracting/dye/lime", 2492, 1),
        ("mekanism:pigment_extracting/dye/magenta", 2493, 1),
        ("mekanism:pigment_extracting/dye/orange", 2494, 1),
        ("mekanism:pigment_extracting/dye/pink", 2495, 1),
        ("mekanism:pigment_extracting/dye/purple", 2496, 1),
        ("mekanism:pigment_extracting/dye/red", 2497, 1),
        ("mekanism:pigment_extracting/dye/white", 2498, 1),
        ("mekanism:pigment_extracting/dye/yellow", 2499, 1),
        ("mekanism:pigment_extracting/flower/black", 2500, 1),
        ("mekanism:pigment_extracting/flower/blue", 2501, 1),
        ("mekanism:pigment_extracting/flower/brown", 2502, 1),
        ("mekanism:pigment_extracting/flower/gray", 2503, 1),
        ("mekanism:pigment_extracting/flower/green", 2504, 1),
        ("mekanism:pigment_extracting/flower/large_cyan", 2505, 1),
        ("mekanism:pigment_extracting/flower/large_magenta", 2506, 1),
        ("mekanism:pigment_extracting/flower/large_pink", 2507, 1),
        ("mekanism:pigment_extracting/flower/large_red", 2508, 1),
        ("mekanism:pigment_extracting/flower/large_yellow", 2509, 1),
        ("mekanism:pigment_extracting/flower/light_blue", 2510, 1),
        ("mekanism:pigment_extracting/flower/light_gray", 2511, 1),
        ("mekanism:pigment_extracting/flower/lime", 2512, 1),
        ("mekanism:pigment_extracting/flower/orange", 2513, 1),
        ("mekanism:pigment_extracting/flower/small_magenta", 2514, 1),
        ("mekanism:pigment_extracting/flower/small_pink", 2515, 1),
        ("mekanism:pigment_extracting/flower/small_red", 2516, 1),
        ("mekanism:pigment_extracting/flower/small_yellow", 2517, 1),
        ("mekanism:pigment_extracting/flower/white", 2518, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/aqua", 2519, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/black", 2520, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/blue", 2521, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/brown", 2522, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/cyan", 2523, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/dark_red", 2524, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/gray", 2525, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/green", 2526, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/light_blue", 2527, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/light_gray", 2528, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/lime", 2529, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/magenta", 2530, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/orange", 2531, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/pink", 2532, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/purple", 2533, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/red", 2534, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/white", 2535, 1),
        ("mekanismadditions:pigment_extracting/plastic/block/yellow", 2536, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/aqua", 2537, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/black", 2538, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/blue", 2539, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/brown", 2540, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/cyan", 2541, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/dark_red", 2542, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/gray", 2543, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/green", 2544, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/light_blue", 2545, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/light_gray", 2546, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/lime", 2547, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/magenta", 2548, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/orange", 2549, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/pink", 2550, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/purple", 2551, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/red", 2552, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/white", 2553, 1),
        ("mekanismadditions:pigment_extracting/plastic/glow/yellow", 2554, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/aqua", 2555, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/black", 2556, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/blue", 2557, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/brown", 2558, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/cyan", 2559, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/dark_red", 2560, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/gray", 2561, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/green", 2562, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/light_blue", 2563, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/light_gray", 2564, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/lime", 2565, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/magenta", 2566, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/orange", 2567, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/pink", 2568, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/purple", 2569, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/red", 2570, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/white", 2571, 1),
        ("mekanismadditions:pigment_extracting/plastic/reinforced/yellow", 2572, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/aqua", 2573, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/black", 2574, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/blue", 2575, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/brown", 2576, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/cyan", 2577, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/dark_red", 2578, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/gray", 2579, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/green", 2580, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/light_blue", 2581, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/light_gray", 2582, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/lime", 2583, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/magenta", 2584, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/orange", 2585, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/pink", 2586, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/purple", 2587, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/red", 2588, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/white", 2589, 1),
        ("mekanismadditions:pigment_extracting/plastic/road/yellow", 2590, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/aqua", 2591, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/black", 2592, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/blue", 2593, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/brown", 2594, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/cyan", 2595, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/dark_red", 2596, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/aqua", 2597, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/black", 2598, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/blue", 2599, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/brown", 2600, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/cyan", 2601, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/dark_red", 2602, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/gray", 2603, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/green", 2604, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/light_blue", 2605, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/light_gray", 2606, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/lime", 2607, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/magenta", 2608, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/orange", 2609, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/pink", 2610, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/purple", 2611, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/red", 2612, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/white", 2613, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/glow/yellow", 2614, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/gray", 2615, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/green", 2616, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/light_blue", 2617, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/light_gray", 2618, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/lime", 2619, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/magenta", 2620, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/orange", 2621, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/pink", 2622, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/purple", 2623, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/red", 2624, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentaqua", 2625, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentblack", 2626, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentblue", 2627, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentbrown", 2628, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentcyan", 2629, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentdark_red", 2630, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentgray", 2631, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentgreen", 2632, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentlight_blue", 2633, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentlight_gray", 2634, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentlime", 2635, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentmagenta", 2636, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentorange", 2637, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentpink", 2638, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentpurple", 2639, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentred", 2640, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentwhite", 2641, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/transparentyellow", 2642, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/white", 2643, 1),
        ("mekanismadditions:pigment_extracting/plastic/slab/yellow", 2644, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/aqua", 2645, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/black", 2646, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/blue", 2647, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/brown", 2648, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/cyan", 2649, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/dark_red", 2650, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/gray", 2651, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/green", 2652, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/light_blue", 2653, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/light_gray", 2654, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/lime", 2655, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/magenta", 2656, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/orange", 2657, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/pink", 2658, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/purple", 2659, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/red", 2660, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/white", 2661, 1),
        ("mekanismadditions:pigment_extracting/plastic/slick/yellow", 2662, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/aqua", 2663, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/black", 2664, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/blue", 2665, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/brown", 2666, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/cyan", 2667, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/dark_red", 2668, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/aqua", 2669, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/black", 2670, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/blue", 2671, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/brown", 2672, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/cyan", 2673, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/dark_red", 2674, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/gray", 2675, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/green", 2676, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/light_blue", 2677, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/light_gray", 2678, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/lime", 2679, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/magenta", 2680, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/orange", 2681, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/pink", 2682, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/purple", 2683, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/red", 2684, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/white", 2685, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/glow/yellow", 2686, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/gray", 2687, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/green", 2688, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/light_blue", 2689, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/light_gray", 2690, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/lime", 2691, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/magenta", 2692, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/orange", 2693, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/pink", 2694, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/purple", 2695, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/red", 2696, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentaqua", 2697, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentblack", 2698, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentblue", 2699, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentbrown", 2700, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentcyan", 2701, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentdark_red", 2702, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentgray", 2703, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentgreen", 2704, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentlight_blue", 2705, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentlight_gray", 2706, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentlime", 2707, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentmagenta", 2708, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentorange", 2709, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentpink", 2710, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentpurple", 2711, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentred", 2712, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentwhite", 2713, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/transparentyellow", 2714, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/white", 2715, 1),
        ("mekanismadditions:pigment_extracting/plastic/stairs/yellow", 2716, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/aqua", 2717, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/black", 2718, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/blue", 2719, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/brown", 2720, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/cyan", 2721, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/dark_red", 2722, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/gray", 2723, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/green", 2724, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/light_blue", 2725, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/light_gray", 2726, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/lime", 2727, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/magenta", 2728, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/orange", 2729, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/pink", 2730, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/purple", 2731, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/red", 2732, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/white", 2733, 1),
        ("mekanismadditions:pigment_extracting/plastic/transparent/yellow", 2734, 1),
        ("mekanism:pigment_extracting/stained_glass/black", 2735, 1),
        ("mekanism:pigment_extracting/stained_glass/blue", 2736, 1),
        ("mekanism:pigment_extracting/stained_glass/brown", 2737, 1),
        ("mekanism:pigment_extracting/stained_glass/cyan", 2738, 1),
        ("mekanism:pigment_extracting/stained_glass/gray", 2739, 1),
        ("mekanism:pigment_extracting/stained_glass/green", 2740, 1),
        ("mekanism:pigment_extracting/stained_glass/light_blue", 2741, 1),
        ("mekanism:pigment_extracting/stained_glass/light_gray", 2742, 1),
        ("mekanism:pigment_extracting/stained_glass/lime", 2743, 1),
        ("mekanism:pigment_extracting/stained_glass/magenta", 2744, 1),
        ("mekanism:pigment_extracting/stained_glass/orange", 2745, 1),
        ("mekanism:pigment_extracting/stained_glass/pink", 2746, 1),
        ("mekanism:pigment_extracting/stained_glass/purple", 2747, 1),
        ("mekanism:pigment_extracting/stained_glass/red", 2748, 1),
        ("mekanism:pigment_extracting/stained_glass/white", 2749, 1),
        ("mekanism:pigment_extracting/stained_glass/yellow", 2750, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/black", 2751, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/blue", 2752, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/brown", 2753, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/cyan", 2754, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/gray", 2755, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/green", 2756, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/light_blue", 2757, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/light_gray", 2758, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/lime", 2759, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/magenta", 2760, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/orange", 2761, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/pink", 2762, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/purple", 2763, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/red", 2764, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/white", 2765, 1),
        ("mekanism:pigment_extracting/stained_glass_pane/yellow", 2766, 1),
        ("mekanism:pigment_extracting/terracotta/black", 2767, 1),
        ("mekanism:pigment_extracting/terracotta/blue", 2768, 1),
        ("mekanism:pigment_extracting/terracotta/brown", 2769, 1),
        ("mekanism:pigment_extracting/terracotta/cyan", 2770, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/black", 2771, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/blue", 2772, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/brown", 2773, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/cyan", 2774, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/gray", 2775, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/green", 2776, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/light_blue", 2777, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/light_gray", 2778, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/lime", 2779, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/magenta", 2780, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/orange", 2781, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/pink", 2782, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/purple", 2783, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/red", 2784, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/white", 2785, 1),
        ("mekanism:pigment_extracting/terracotta/glazed/yellow", 2786, 1),
        ("mekanism:pigment_extracting/terracotta/gray", 2787, 1),
        ("mekanism:pigment_extracting/terracotta/green", 2788, 1),
        ("mekanism:pigment_extracting/terracotta/light_blue", 2789, 1),
        ("mekanism:pigment_extracting/terracotta/light_gray", 2790, 1),
        ("mekanism:pigment_extracting/terracotta/lime", 2791, 1),
        ("mekanism:pigment_extracting/terracotta/magenta", 2792, 1),
        ("mekanism:pigment_extracting/terracotta/orange", 2793, 1),
        ("mekanism:pigment_extracting/terracotta/pink", 2794, 1),
        ("mekanism:pigment_extracting/terracotta/purple", 2795, 1),
        ("mekanism:pigment_extracting/terracotta/red", 2796, 1),
        ("mekanism:pigment_extracting/terracotta/white", 2797, 1),
        ("mekanism:pigment_extracting/terracotta/yellow", 2798, 1),
        ("mekanism:pigment_extracting/wool/black", 2799, 1),
        ("mekanism:pigment_extracting/wool/blue", 2800, 1),
        ("mekanism:pigment_extracting/wool/brown", 2801, 1),
        ("mekanism:pigment_extracting/wool/cyan", 2802, 1),
        ("mekanism:pigment_extracting/wool/gray", 2803, 1),
        ("mekanism:pigment_extracting/wool/green", 2804, 1),
        ("mekanism:pigment_extracting/wool/light_blue", 2805, 1),
        ("mekanism:pigment_extracting/wool/light_gray", 2806, 1),
        ("mekanism:pigment_extracting/wool/lime", 2807, 1),
        ("mekanism:pigment_extracting/wool/magenta", 2808, 1),
        ("mekanism:pigment_extracting/wool/orange", 2809, 1),
        ("mekanism:pigment_extracting/wool/pink", 2810, 1),
        ("mekanism:pigment_extracting/wool/purple", 2811, 1),
        ("mekanism:pigment_extracting/wool/red", 2812, 1),
        ("mekanism:pigment_extracting/wool/slabs/black", 2813, 1),
        ("mekanism:pigment_extracting/wool/slabs/blue", 2814, 1),
        ("mekanism:pigment_extracting/wool/slabs/brown", 2815, 1),
        ("mekanism:pigment_extracting/wool/slabs/cyan", 2816, 1),
        ("mekanism:pigment_extracting/wool/slabs/gray", 2817, 1),
        ("mekanism:pigment_extracting/wool/slabs/green", 2818, 1),
        ("mekanism:pigment_extracting/wool/slabs/light_blue", 2819, 1),
        ("mekanism:pigment_extracting/wool/slabs/light_gray", 2820, 1),
        ("mekanism:pigment_extracting/wool/slabs/lime", 2821, 1),
        ("mekanism:pigment_extracting/wool/slabs/magenta", 2822, 1),
        ("mekanism:pigment_extracting/wool/slabs/orange", 2823, 1),
        ("mekanism:pigment_extracting/wool/slabs/pink", 2824, 1),
        ("mekanism:pigment_extracting/wool/slabs/purple", 2825, 1),
        ("mekanism:pigment_extracting/wool/slabs/red", 2826, 1),
        ("mekanism:pigment_extracting/wool/slabs/white", 2827, 1),
        ("mekanism:pigment_extracting/wool/slabs/yellow", 2828, 1),
        ("mekanism:pigment_extracting/wool/stairs/black", 2829, 1),
        ("mekanism:pigment_extracting/wool/stairs/blue", 2830, 1),
        ("mekanism:pigment_extracting/wool/stairs/brown", 2831, 1),
        ("mekanism:pigment_extracting/wool/stairs/cyan", 2832, 1),
        ("mekanism:pigment_extracting/wool/stairs/gray", 2833, 1),
        ("mekanism:pigment_extracting/wool/stairs/green", 2834, 1),
        ("mekanism:pigment_extracting/wool/stairs/light_blue", 2835, 1),
        ("mekanism:pigment_extracting/wool/stairs/light_gray", 2836, 1),
        ("mekanism:pigment_extracting/wool/stairs/lime", 2837, 1),
        ("mekanism:pigment_extracting/wool/stairs/magenta", 2838, 1),
        ("mekanism:pigment_extracting/wool/stairs/orange", 2839, 1),
        ("mekanism:pigment_extracting/wool/stairs/pink", 2840, 1),
        ("mekanism:pigment_extracting/wool/stairs/purple", 2841, 1),
        ("mekanism:pigment_extracting/wool/stairs/red", 2842, 1),
        ("mekanism:pigment_extracting/wool/stairs/white", 2843, 1),
        ("mekanism:pigment_extracting/wool/stairs/yellow", 2844, 1),
        ("mekanism:pigment_extracting/wool/white", 2845, 1),
        ("mekanism:pigment_extracting/wool/yellow", 2846, 1),
        ("mekanism:pigment_extractor", 2847, 1),
        ("mekanism:pigment_mixer", 2848, 1),
        ("mekanism:pigment_mixing/aqua_yellow_to_lime", 2849, 1),
        ("mekanism:pigment_mixing/black_red_to_dark_red", 2850, 1),
        ("mekanism:pigment_mixing/black_white_to_gray", 2851, 1),
        ("mekanism:pigment_mixing/blue_green_to_cyan", 2852, 1),
        ("mekanism:pigment_mixing/blue_red_to_purple", 2853, 1),
        ("mekanism:pigment_mixing/blue_white_to_light_blue", 2854, 1),
        ("mekanism:pigment_mixing/blue_yellow_to_green", 2855, 1),
        ("mekanism:pigment_mixing/cyan_white_to_aqua", 2856, 1),
        ("mekanism:pigment_mixing/gray_red_to_dark_red", 2857, 1),
        ("mekanism:pigment_mixing/gray_white_to_light_gray", 2858, 1),
        ("mekanism:pigment_mixing/green_white_to_lime", 2859, 1),
        ("mekanism:pigment_mixing/light_blue_lime_to_aqua", 2860, 1),
        ("mekanism:pigment_mixing/light_blue_red_to_magenta", 2861, 1),
        ("mekanism:pigment_mixing/purple_pink_to_magenta", 2862, 1),
        ("mekanism:pigment_mixing/red_white_to_pink", 2863, 1),
        ("mekanism:pigment_mixing/red_yellow_to_orange", 2864, 1),
        ("minecraft:pink_banner", 2865, 1),
        ("minecraft:pink_bed", 2866, 1),
        ("minecraft:pink_bundle", 2867, 1),
        ("minecraft:pink_candle", 2868, 1),
        ("minecraft:pink_carpet", 2869, 1),
        ("minecraft:pink_concrete_powder", 2870, 1),
        ("minecraft:pink_concrete_slab", 2871, 1),
        ("minecraft:pink_concrete_slab_from_pink_concrete_stonecutting", 2872, 1),
        ("minecraft:pink_concrete_stairs", 2873, 1),
        ("minecraft:pink_concrete_stairs_from_pink_concrete_stonecutting", 2874, 1),
        ("minecraft:pink_cushion", 2875, 1),
        ("minecraft:pink_dye_from_cactus_flower", 2876, 1),
        ("minecraft:pink_dye_from_peony", 2877, 1),
        ("minecraft:pink_dye_from_pink_petals", 2878, 1),
        ("minecraft:pink_dye_from_pink_tulip", 2879, 1),
        ("minecraft:pink_dye_from_red_white_dye", 2880, 1),
        ("minecraft:pink_glazed_terracotta", 2881, 1),
        ("minecraft:pink_harness", 2882, 1),
        ("minecraft:pink_shulker_box", 2883, 1),
        ("minecraft:pink_stained_glass", 2884, 1),
        ("minecraft:pink_stained_glass_pane", 2885, 1),
        ("minecraft:pink_stained_glass_pane_from_glass_pane", 2886, 1),
        ("minecraft:pink_terracotta", 2887, 1),
        ("minecraft:pink_wool_slab", 2888, 1),
        ("minecraft:pink_wool_stairs", 2889, 1),
        ("minecraft:piston", 2890, 1),
        ("mekanismadditions:plastic/block/black", 2891, 1),
        ("mekanismadditions:plastic/block/blue", 2892, 1),
        ("mekanismadditions:plastic/block/brown", 2893, 1),
        ("mekanismadditions:plastic/block/cyan", 2894, 1),
        ("mekanismadditions:plastic/block/gray", 2895, 1),
        ("mekanismadditions:plastic/block/green", 2896, 1),
        ("mekanismadditions:plastic/block/light_blue", 2897, 1),
        ("mekanismadditions:plastic/block/light_gray", 2898, 1),
        ("mekanismadditions:plastic/block/lime", 2899, 1),
        ("mekanismadditions:plastic/block/magenta", 2900, 1),
        ("mekanismadditions:plastic/block/orange", 2901, 1),
        ("mekanismadditions:plastic/block/pink", 2902, 1),
        ("mekanismadditions:plastic/block/purple", 2903, 1),
        ("mekanismadditions:plastic/block/recolor/black", 2904, 1),
        ("mekanismadditions:plastic/block/recolor/blue", 2905, 1),
        ("mekanismadditions:plastic/block/recolor/brown", 2906, 1),
        ("mekanismadditions:plastic/block/recolor/cyan", 2907, 1),
        ("mekanismadditions:plastic/block/recolor/gray", 2908, 1),
        ("mekanismadditions:plastic/block/recolor/green", 2909, 1),
        ("mekanismadditions:plastic/block/recolor/light_blue", 2910, 1),
        ("mekanismadditions:plastic/block/recolor/light_gray", 2911, 1),
        ("mekanismadditions:plastic/block/recolor/lime", 2912, 1),
        ("mekanismadditions:plastic/block/recolor/magenta", 2913, 1),
        ("mekanismadditions:plastic/block/recolor/orange", 2914, 1),
        ("mekanismadditions:plastic/block/recolor/painting/aqua", 2915, 1),
        ("mekanismadditions:plastic/block/recolor/painting/black", 2916, 1),
        ("mekanismadditions:plastic/block/recolor/painting/blue", 2917, 1),
        ("mekanismadditions:plastic/block/recolor/painting/brown", 2918, 1),
        ("mekanismadditions:plastic/block/recolor/painting/cyan", 2919, 1),
        ("mekanismadditions:plastic/block/recolor/painting/dark_red", 2920, 1),
        ("mekanismadditions:plastic/block/recolor/painting/gray", 2921, 1),
        ("mekanismadditions:plastic/block/recolor/painting/green", 2922, 1),
        ("mekanismadditions:plastic/block/recolor/painting/light_blue", 2923, 1),
        ("mekanismadditions:plastic/block/recolor/painting/light_gray", 2924, 1),
        ("mekanismadditions:plastic/block/recolor/painting/lime", 2925, 1),
        ("mekanismadditions:plastic/block/recolor/painting/magenta", 2926, 1),
        ("mekanismadditions:plastic/block/recolor/painting/orange", 2927, 1),
        ("mekanismadditions:plastic/block/recolor/painting/pink", 2928, 1),
        ("mekanismadditions:plastic/block/recolor/painting/purple", 2929, 1),
        ("mekanismadditions:plastic/block/recolor/painting/red", 2930, 1),
        ("mekanismadditions:plastic/block/recolor/painting/white", 2931, 1),
        ("mekanismadditions:plastic/block/recolor/painting/yellow", 2932, 1),
        ("mekanismadditions:plastic/block/recolor/pink", 2933, 1),
        ("mekanismadditions:plastic/block/recolor/purple", 2934, 1),
        ("mekanismadditions:plastic/block/recolor/red", 2935, 1),
        ("mekanismadditions:plastic/block/recolor/white", 2936, 1),
        ("mekanismadditions:plastic/block/recolor/yellow", 2937, 1),
        ("mekanismadditions:plastic/block/red", 2938, 1),
        ("mekanismadditions:plastic/block/white", 2939, 1),
        ("mekanismadditions:plastic/block/yellow", 2940, 1),
        ("mekanismadditions:plastic/fence/aqua", 2941, 1),
        ("mekanismadditions:plastic/fence/black", 2942, 1),
        ("mekanismadditions:plastic/fence/blue", 2943, 1),
        ("mekanismadditions:plastic/fence/brown", 2944, 1),
        ("mekanismadditions:plastic/fence/cyan", 2945, 1),
        ("mekanismadditions:plastic/fence/dark_red", 2946, 1),
        ("mekanismadditions:plastic/fence/gray", 2947, 1),
        ("mekanismadditions:plastic/fence/green", 2948, 1),
        ("mekanismadditions:plastic/fence/light_blue", 2949, 1),
        ("mekanismadditions:plastic/fence/light_gray", 2950, 1),
        ("mekanismadditions:plastic/fence/lime", 2951, 1),
        ("mekanismadditions:plastic/fence/magenta", 2952, 1),
        ("mekanismadditions:plastic/fence/orange", 2953, 1),
        ("mekanismadditions:plastic/fence/pink", 2954, 1),
        ("mekanismadditions:plastic/fence/purple", 2955, 1),
        ("mekanismadditions:plastic/fence/recolor/black", 2956, 1),
        ("mekanismadditions:plastic/fence/recolor/blue", 2957, 1),
        ("mekanismadditions:plastic/fence/recolor/brown", 2958, 1),
        ("mekanismadditions:plastic/fence/recolor/cyan", 2959, 1),
        ("mekanismadditions:plastic/fence/recolor/gray", 2960, 1),
        ("mekanismadditions:plastic/fence/recolor/green", 2961, 1),
        ("mekanismadditions:plastic/fence/recolor/light_blue", 2962, 1),
        ("mekanismadditions:plastic/fence/recolor/light_gray", 2963, 1),
        ("mekanismadditions:plastic/fence/recolor/lime", 2964, 1),
        ("mekanismadditions:plastic/fence/recolor/magenta", 2965, 1),
        ("mekanismadditions:plastic/fence/recolor/orange", 2966, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/aqua", 2967, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/black", 2968, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/blue", 2969, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/brown", 2970, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/cyan", 2971, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/dark_red", 2972, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/gray", 2973, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/green", 2974, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/light_blue", 2975, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/light_gray", 2976, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/lime", 2977, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/magenta", 2978, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/orange", 2979, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/pink", 2980, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/purple", 2981, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/red", 2982, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/white", 2983, 1),
        ("mekanismadditions:plastic/fence/recolor/painting/yellow", 2984, 1),
        ("mekanismadditions:plastic/fence/recolor/pink", 2985, 1),
        ("mekanismadditions:plastic/fence/recolor/purple", 2986, 1),
        ("mekanismadditions:plastic/fence/recolor/red", 2987, 1),
        ("mekanismadditions:plastic/fence/recolor/white", 2988, 1),
        ("mekanismadditions:plastic/fence/recolor/yellow", 2989, 1),
        ("mekanismadditions:plastic/fence/red", 2990, 1),
        ("mekanismadditions:plastic/fence/white", 2991, 1),
        ("mekanismadditions:plastic/fence/yellow", 2992, 1),
        ("mekanismadditions:plastic/fence_gate/aqua", 2993, 1),
        ("mekanismadditions:plastic/fence_gate/black", 2994, 1),
        ("mekanismadditions:plastic/fence_gate/blue", 2995, 1),
        ("mekanismadditions:plastic/fence_gate/brown", 2996, 1),
        ("mekanismadditions:plastic/fence_gate/cyan", 2997, 1),
        ("mekanismadditions:plastic/fence_gate/dark_red", 2998, 1),
        ("mekanismadditions:plastic/fence_gate/gray", 2999, 1),
        ("mekanismadditions:plastic/fence_gate/green", 3000, 1),
        ("mekanismadditions:plastic/fence_gate/light_blue", 3001, 1),
        ("mekanismadditions:plastic/fence_gate/light_gray", 3002, 1),
        ("mekanismadditions:plastic/fence_gate/lime", 3003, 1),
        ("mekanismadditions:plastic/fence_gate/magenta", 3004, 1),
        ("mekanismadditions:plastic/fence_gate/orange", 3005, 1),
        ("mekanismadditions:plastic/fence_gate/pink", 3006, 1),
        ("mekanismadditions:plastic/fence_gate/purple", 3007, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/black", 3008, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/blue", 3009, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/brown", 3010, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/cyan", 3011, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/gray", 3012, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/green", 3013, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/light_blue", 3014, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/light_gray", 3015, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/lime", 3016, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/magenta", 3017, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/orange", 3018, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/aqua", 3019, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/black", 3020, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/blue", 3021, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/brown", 3022, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/cyan", 3023, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/dark_red", 3024, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/gray", 3025, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/green", 3026, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/light_blue", 3027, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/light_gray", 3028, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/lime", 3029, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/magenta", 3030, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/orange", 3031, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/pink", 3032, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/purple", 3033, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/red", 3034, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/white", 3035, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/painting/yellow", 3036, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/pink", 3037, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/purple", 3038, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/red", 3039, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/white", 3040, 1),
        ("mekanismadditions:plastic/fence_gate/recolor/yellow", 3041, 1),
        ("mekanismadditions:plastic/fence_gate/red", 3042, 1),
        ("mekanismadditions:plastic/fence_gate/white", 3043, 1),
        ("mekanismadditions:plastic/fence_gate/yellow", 3044, 1),
        ("mekanismadditions:plastic/glow/aqua", 3045, 1),
        ("mekanismadditions:plastic/glow/black", 3046, 1),
        ("mekanismadditions:plastic/glow/blue", 3047, 1),
        ("mekanismadditions:plastic/glow/brown", 3048, 1),
        ("mekanismadditions:plastic/glow/cyan", 3049, 1),
        ("mekanismadditions:plastic/glow/dark_red", 3050, 1),
        ("mekanismadditions:plastic/glow/gray", 3051, 1),
        ("mekanismadditions:plastic/glow/green", 3052, 1),
        ("mekanismadditions:plastic/glow/light_blue", 3053, 1),
        ("mekanismadditions:plastic/glow/light_gray", 3054, 1),
        ("mekanismadditions:plastic/glow/lime", 3055, 1),
        ("mekanismadditions:plastic/glow/magenta", 3056, 1),
        ("mekanismadditions:plastic/glow/orange", 3057, 1),
        ("mekanismadditions:plastic/glow/pink", 3058, 1),
        ("mekanismadditions:plastic/glow/purple", 3059, 1),
        ("mekanismadditions:plastic/glow/recolor/black", 3060, 1),
        ("mekanismadditions:plastic/glow/recolor/blue", 3061, 1),
        ("mekanismadditions:plastic/glow/recolor/brown", 3062, 1),
        ("mekanismadditions:plastic/glow/recolor/cyan", 3063, 1),
        ("mekanismadditions:plastic/glow/recolor/gray", 3064, 1),
        ("mekanismadditions:plastic/glow/recolor/green", 3065, 1),
        ("mekanismadditions:plastic/glow/recolor/light_blue", 3066, 1),
        ("mekanismadditions:plastic/glow/recolor/light_gray", 3067, 1),
        ("mekanismadditions:plastic/glow/recolor/lime", 3068, 1),
        ("mekanismadditions:plastic/glow/recolor/magenta", 3069, 1),
        ("mekanismadditions:plastic/glow/recolor/orange", 3070, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/aqua", 3071, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/black", 3072, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/blue", 3073, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/brown", 3074, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/cyan", 3075, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/dark_red", 3076, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/gray", 3077, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/green", 3078, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/light_blue", 3079, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/light_gray", 3080, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/lime", 3081, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/magenta", 3082, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/orange", 3083, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/pink", 3084, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/purple", 3085, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/red", 3086, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/white", 3087, 1),
        ("mekanismadditions:plastic/glow/recolor/painting/yellow", 3088, 1),
        ("mekanismadditions:plastic/glow/recolor/pink", 3089, 1),
        ("mekanismadditions:plastic/glow/recolor/purple", 3090, 1),
        ("mekanismadditions:plastic/glow/recolor/red", 3091, 1),
        ("mekanismadditions:plastic/glow/recolor/white", 3092, 1),
        ("mekanismadditions:plastic/glow/recolor/yellow", 3093, 1),
        ("mekanismadditions:plastic/glow/red", 3094, 1),
        ("mekanismadditions:plastic/glow/white", 3095, 1),
        ("mekanismadditions:plastic/glow/yellow", 3096, 1),
        ("mekanismadditions:plastic/reinforced/aqua", 3097, 1),
        ("mekanismadditions:plastic/reinforced/black", 3098, 1),
        ("mekanismadditions:plastic/reinforced/blue", 3099, 1),
        ("mekanismadditions:plastic/reinforced/brown", 3100, 1),
        ("mekanismadditions:plastic/reinforced/cyan", 3101, 1),
        ("mekanismadditions:plastic/reinforced/dark_red", 3102, 1),
        ("mekanismadditions:plastic/reinforced/gray", 3103, 1),
        ("mekanismadditions:plastic/reinforced/green", 3104, 1),
        ("mekanismadditions:plastic/reinforced/light_blue", 3105, 1),
        ("mekanismadditions:plastic/reinforced/light_gray", 3106, 1),
        ("mekanismadditions:plastic/reinforced/lime", 3107, 1),
        ("mekanismadditions:plastic/reinforced/magenta", 3108, 1),
        ("mekanismadditions:plastic/reinforced/orange", 3109, 1),
        ("mekanismadditions:plastic/reinforced/pink", 3110, 1),
        ("mekanismadditions:plastic/reinforced/purple", 3111, 1),
        ("mekanismadditions:plastic/reinforced/recolor/black", 3112, 1),
        ("mekanismadditions:plastic/reinforced/recolor/blue", 3113, 1),
        ("mekanismadditions:plastic/reinforced/recolor/brown", 3114, 1),
        ("mekanismadditions:plastic/reinforced/recolor/cyan", 3115, 1),
        ("mekanismadditions:plastic/reinforced/recolor/gray", 3116, 1),
        ("mekanismadditions:plastic/reinforced/recolor/green", 3117, 1),
        ("mekanismadditions:plastic/reinforced/recolor/light_blue", 3118, 1),
        ("mekanismadditions:plastic/reinforced/recolor/light_gray", 3119, 1),
        ("mekanismadditions:plastic/reinforced/recolor/lime", 3120, 1),
        ("mekanismadditions:plastic/reinforced/recolor/magenta", 3121, 1),
        ("mekanismadditions:plastic/reinforced/recolor/orange", 3122, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/aqua", 3123, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/black", 3124, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/blue", 3125, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/brown", 3126, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/cyan", 3127, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/dark_red", 3128, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/gray", 3129, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/green", 3130, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/light_blue", 3131, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/light_gray", 3132, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/lime", 3133, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/magenta", 3134, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/orange", 3135, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/pink", 3136, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/purple", 3137, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/red", 3138, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/white", 3139, 1),
        ("mekanismadditions:plastic/reinforced/recolor/painting/yellow", 3140, 1),
        ("mekanismadditions:plastic/reinforced/recolor/pink", 3141, 1),
        ("mekanismadditions:plastic/reinforced/recolor/purple", 3142, 1),
        ("mekanismadditions:plastic/reinforced/recolor/red", 3143, 1),
        ("mekanismadditions:plastic/reinforced/recolor/white", 3144, 1),
        ("mekanismadditions:plastic/reinforced/recolor/yellow", 3145, 1),
        ("mekanismadditions:plastic/reinforced/red", 3146, 1),
        ("mekanismadditions:plastic/reinforced/white", 3147, 1),
        ("mekanismadditions:plastic/reinforced/yellow", 3148, 1),
        ("mekanismadditions:plastic/road/aqua", 3149, 1),
        ("mekanismadditions:plastic/road/black", 3150, 1),
        ("mekanismadditions:plastic/road/blue", 3151, 1),
        ("mekanismadditions:plastic/road/brown", 3152, 1),
        ("mekanismadditions:plastic/road/cyan", 3153, 1),
        ("mekanismadditions:plastic/road/dark_red", 3154, 1),
        ("mekanismadditions:plastic/road/gray", 3155, 1),
        ("mekanismadditions:plastic/road/green", 3156, 1),
        ("mekanismadditions:plastic/road/light_blue", 3157, 1),
        ("mekanismadditions:plastic/road/light_gray", 3158, 1),
        ("mekanismadditions:plastic/road/lime", 3159, 1),
        ("mekanismadditions:plastic/road/magenta", 3160, 1),
        ("mekanismadditions:plastic/road/orange", 3161, 1),
        ("mekanismadditions:plastic/road/pink", 3162, 1),
        ("mekanismadditions:plastic/road/purple", 3163, 1),
        ("mekanismadditions:plastic/road/recolor/black", 3164, 1),
        ("mekanismadditions:plastic/road/recolor/blue", 3165, 1),
        ("mekanismadditions:plastic/road/recolor/brown", 3166, 1),
        ("mekanismadditions:plastic/road/recolor/cyan", 3167, 1),
        ("mekanismadditions:plastic/road/recolor/gray", 3168, 1),
        ("mekanismadditions:plastic/road/recolor/green", 3169, 1),
        ("mekanismadditions:plastic/road/recolor/light_blue", 3170, 1),
        ("mekanismadditions:plastic/road/recolor/light_gray", 3171, 1),
        ("mekanismadditions:plastic/road/recolor/lime", 3172, 1),
        ("mekanismadditions:plastic/road/recolor/magenta", 3173, 1),
        ("mekanismadditions:plastic/road/recolor/orange", 3174, 1),
        ("mekanismadditions:plastic/road/recolor/painting/aqua", 3175, 1),
        ("mekanismadditions:plastic/road/recolor/painting/black", 3176, 1),
        ("mekanismadditions:plastic/road/recolor/painting/blue", 3177, 1),
        ("mekanismadditions:plastic/road/recolor/painting/brown", 3178, 1),
        ("mekanismadditions:plastic/road/recolor/painting/cyan", 3179, 1),
        ("mekanismadditions:plastic/road/recolor/painting/dark_red", 3180, 1),
        ("mekanismadditions:plastic/road/recolor/painting/gray", 3181, 1),
        ("mekanismadditions:plastic/road/recolor/painting/green", 3182, 1),
        ("mekanismadditions:plastic/road/recolor/painting/light_blue", 3183, 1),
        ("mekanismadditions:plastic/road/recolor/painting/light_gray", 3184, 1),
        ("mekanismadditions:plastic/road/recolor/painting/lime", 3185, 1),
        ("mekanismadditions:plastic/road/recolor/painting/magenta", 3186, 1),
        ("mekanismadditions:plastic/road/recolor/painting/orange", 3187, 1),
        ("mekanismadditions:plastic/road/recolor/painting/pink", 3188, 1),
        ("mekanismadditions:plastic/road/recolor/painting/purple", 3189, 1),
        ("mekanismadditions:plastic/road/recolor/painting/red", 3190, 1),
        ("mekanismadditions:plastic/road/recolor/painting/white", 3191, 1),
        ("mekanismadditions:plastic/road/recolor/painting/yellow", 3192, 1),
        ("mekanismadditions:plastic/road/recolor/pink", 3193, 1),
        ("mekanismadditions:plastic/road/recolor/purple", 3194, 1),
        ("mekanismadditions:plastic/road/recolor/red", 3195, 1),
        ("mekanismadditions:plastic/road/recolor/white", 3196, 1),
        ("mekanismadditions:plastic/road/recolor/yellow", 3197, 1),
        ("mekanismadditions:plastic/road/red", 3198, 1),
        ("mekanismadditions:plastic/road/white", 3199, 1),
        ("mekanismadditions:plastic/road/yellow", 3200, 1),
        ("mekanismadditions:plastic/slab/aqua", 3201, 1),
        ("mekanismadditions:plastic/slab/black", 3202, 1),
        ("mekanismadditions:plastic/slab/blue", 3203, 1),
        ("mekanismadditions:plastic/slab/brown", 3204, 1),
        ("mekanismadditions:plastic/slab/cyan", 3205, 1),
        ("mekanismadditions:plastic/slab/dark_red", 3206, 1),
        ("mekanismadditions:plastic/slab/glow/aqua", 3207, 1),
        ("mekanismadditions:plastic/slab/glow/black", 3208, 1),
        ("mekanismadditions:plastic/slab/glow/blue", 3209, 1),
        ("mekanismadditions:plastic/slab/glow/brown", 3210, 1),
        ("mekanismadditions:plastic/slab/glow/cyan", 3211, 1),
        ("mekanismadditions:plastic/slab/glow/dark_red", 3212, 1),
        ("mekanismadditions:plastic/slab/glow/gray", 3213, 1),
        ("mekanismadditions:plastic/slab/glow/green", 3214, 1),
        ("mekanismadditions:plastic/slab/glow/light_blue", 3215, 1),
        ("mekanismadditions:plastic/slab/glow/light_gray", 3216, 1),
        ("mekanismadditions:plastic/slab/glow/lime", 3217, 1),
        ("mekanismadditions:plastic/slab/glow/magenta", 3218, 1),
        ("mekanismadditions:plastic/slab/glow/orange", 3219, 1),
        ("mekanismadditions:plastic/slab/glow/pink", 3220, 1),
        ("mekanismadditions:plastic/slab/glow/purple", 3221, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/black", 3222, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/blue", 3223, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/brown", 3224, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/cyan", 3225, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/gray", 3226, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/green", 3227, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/light_blue", 3228, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/light_gray", 3229, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/lime", 3230, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/magenta", 3231, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/orange", 3232, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/aqua", 3233, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/black", 3234, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/blue", 3235, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/brown", 3236, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/cyan", 3237, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/dark_red", 3238, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/gray", 3239, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/green", 3240, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/light_blue", 3241, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/light_gray", 3242, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/lime", 3243, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/magenta", 3244, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/orange", 3245, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/pink", 3246, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/purple", 3247, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/red", 3248, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/white", 3249, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/painting/yellow", 3250, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/pink", 3251, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/purple", 3252, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/red", 3253, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/white", 3254, 1),
        ("mekanismadditions:plastic/slab/glow/recolor/yellow", 3255, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/aqua", 3256, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/black", 3257, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/blue", 3258, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/brown", 3259, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/cyan", 3260, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/dark_red", 3261, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/gray", 3262, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/green", 3263, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/light_blue", 3264, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/light_gray", 3265, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/lime", 3266, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/magenta", 3267, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/orange", 3268, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/pink", 3269, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/purple", 3270, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/red", 3271, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/white", 3272, 1),
        ("mekanismadditions:plastic/slab/glow/recombination/yellow", 3273, 1),
        ("mekanismadditions:plastic/slab/glow/red", 3274, 1),
        ("mekanismadditions:plastic/slab/glow/white", 3275, 1),
        ("mekanismadditions:plastic/slab/glow/yellow", 3276, 1),
        ("mekanismadditions:plastic/slab/gray", 3277, 1),
        ("mekanismadditions:plastic/slab/green", 3278, 1),
        ("mekanismadditions:plastic/slab/light_blue", 3279, 1),
        ("mekanismadditions:plastic/slab/light_gray", 3280, 1),
        ("mekanismadditions:plastic/slab/lime", 3281, 1),
        ("mekanismadditions:plastic/slab/magenta", 3282, 1),
        ("mekanismadditions:plastic/slab/orange", 3283, 1),
        ("mekanismadditions:plastic/slab/pink", 3284, 1),
        ("mekanismadditions:plastic/slab/purple", 3285, 1),
        ("mekanismadditions:plastic/slab/recolor/black", 3286, 1),
        ("mekanismadditions:plastic/slab/recolor/blue", 3287, 1),
        ("mekanismadditions:plastic/slab/recolor/brown", 3288, 1),
        ("mekanismadditions:plastic/slab/recolor/cyan", 3289, 1),
        ("mekanismadditions:plastic/slab/recolor/gray", 3290, 1),
        ("mekanismadditions:plastic/slab/recolor/green", 3291, 1),
        ("mekanismadditions:plastic/slab/recolor/light_blue", 3292, 1),
        ("mekanismadditions:plastic/slab/recolor/light_gray", 3293, 1),
        ("mekanismadditions:plastic/slab/recolor/lime", 3294, 1),
        ("mekanismadditions:plastic/slab/recolor/magenta", 3295, 1),
        ("mekanismadditions:plastic/slab/recolor/orange", 3296, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/aqua", 3297, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/black", 3298, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/blue", 3299, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/brown", 3300, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/cyan", 3301, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/dark_red", 3302, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/gray", 3303, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/green", 3304, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/light_blue", 3305, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/light_gray", 3306, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/lime", 3307, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/magenta", 3308, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/orange", 3309, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/pink", 3310, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/purple", 3311, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/red", 3312, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/white", 3313, 1),
        ("mekanismadditions:plastic/slab/recolor/painting/yellow", 3314, 1),
        ("mekanismadditions:plastic/slab/recolor/pink", 3315, 1),
        ("mekanismadditions:plastic/slab/recolor/purple", 3316, 1),
        ("mekanismadditions:plastic/slab/recolor/red", 3317, 1),
        ("mekanismadditions:plastic/slab/recolor/white", 3318, 1),
        ("mekanismadditions:plastic/slab/recolor/yellow", 3319, 1),
        ("mekanismadditions:plastic/slab/recombination/aqua", 3320, 1),
        ("mekanismadditions:plastic/slab/recombination/black", 3321, 1),
        ("mekanismadditions:plastic/slab/recombination/blue", 3322, 1),
        ("mekanismadditions:plastic/slab/recombination/brown", 3323, 1),
        ("mekanismadditions:plastic/slab/recombination/cyan", 3324, 1),
        ("mekanismadditions:plastic/slab/recombination/dark_red", 3325, 1),
        ("mekanismadditions:plastic/slab/recombination/gray", 3326, 1),
        ("mekanismadditions:plastic/slab/recombination/green", 3327, 1),
        ("mekanismadditions:plastic/slab/recombination/light_blue", 3328, 1),
        ("mekanismadditions:plastic/slab/recombination/light_gray", 3329, 1),
        ("mekanismadditions:plastic/slab/recombination/lime", 3330, 1),
        ("mekanismadditions:plastic/slab/recombination/magenta", 3331, 1),
        ("mekanismadditions:plastic/slab/recombination/orange", 3332, 1),
        ("mekanismadditions:plastic/slab/recombination/pink", 3333, 1),
        ("mekanismadditions:plastic/slab/recombination/purple", 3334, 1),
        ("mekanismadditions:plastic/slab/recombination/red", 3335, 1),
        ("mekanismadditions:plastic/slab/recombination/white", 3336, 1),
        ("mekanismadditions:plastic/slab/recombination/yellow", 3337, 1),
        ("mekanismadditions:plastic/slab/red", 3338, 1),
        ("mekanismadditions:plastic/slab/transparent/aqua", 3339, 1),
        ("mekanismadditions:plastic/slab/transparent/black", 3340, 1),
        ("mekanismadditions:plastic/slab/transparent/blue", 3341, 1),
        ("mekanismadditions:plastic/slab/transparent/brown", 3342, 1),
        ("mekanismadditions:plastic/slab/transparent/cyan", 3343, 1),
        ("mekanismadditions:plastic/slab/transparent/dark_red", 3344, 1),
        ("mekanismadditions:plastic/slab/transparent/gray", 3345, 1),
        ("mekanismadditions:plastic/slab/transparent/green", 3346, 1),
        ("mekanismadditions:plastic/slab/transparent/light_blue", 3347, 1),
        ("mekanismadditions:plastic/slab/transparent/light_gray", 3348, 1),
        ("mekanismadditions:plastic/slab/transparent/lime", 3349, 1),
        ("mekanismadditions:plastic/slab/transparent/magenta", 3350, 1),
        ("mekanismadditions:plastic/slab/transparent/orange", 3351, 1),
        ("mekanismadditions:plastic/slab/transparent/pink", 3352, 1),
        ("mekanismadditions:plastic/slab/transparent/purple", 3353, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/black", 3354, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/blue", 3355, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/brown", 3356, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/cyan", 3357, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/gray", 3358, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/green", 3359, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/light_blue", 3360, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/light_gray", 3361, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/lime", 3362, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/magenta", 3363, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/orange", 3364, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/aqua", 3365, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/black", 3366, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/blue", 3367, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/brown", 3368, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/cyan", 3369, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/dark_red", 3370, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/gray", 3371, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/green", 3372, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/light_blue", 3373, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/light_gray", 3374, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/lime", 3375, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/magenta", 3376, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/orange", 3377, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/pink", 3378, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/purple", 3379, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/red", 3380, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/white", 3381, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/painting/yellow", 3382, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/pink", 3383, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/purple", 3384, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/red", 3385, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/white", 3386, 1),
        ("mekanismadditions:plastic/slab/transparent/recolor/yellow", 3387, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/aqua", 3388, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/black", 3389, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/blue", 3390, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/brown", 3391, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/cyan", 3392, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/dark_red", 3393, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/gray", 3394, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/green", 3395, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/light_blue", 3396, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/light_gray", 3397, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/lime", 3398, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/magenta", 3399, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/orange", 3400, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/pink", 3401, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/purple", 3402, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/red", 3403, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/white", 3404, 1),
        ("mekanismadditions:plastic/slab/transparent/recombination/yellow", 3405, 1),
        ("mekanismadditions:plastic/slab/transparent/red", 3406, 1),
        ("mekanismadditions:plastic/slab/transparent/white", 3407, 1),
        ("mekanismadditions:plastic/slab/transparent/yellow", 3408, 1),
        ("mekanismadditions:plastic/slab/white", 3409, 1),
        ("mekanismadditions:plastic/slab/yellow", 3410, 1),
        ("mekanismadditions:plastic/slick/aqua", 3411, 1),
        ("mekanismadditions:plastic/slick/black", 3412, 1),
        ("mekanismadditions:plastic/slick/blue", 3413, 1),
        ("mekanismadditions:plastic/slick/brown", 3414, 1),
        ("mekanismadditions:plastic/slick/cyan", 3415, 1),
        ("mekanismadditions:plastic/slick/dark_red", 3416, 1),
        ("mekanismadditions:plastic/slick/enriching/aqua", 3417, 1),
        ("mekanismadditions:plastic/slick/enriching/black", 3418, 1),
        ("mekanismadditions:plastic/slick/enriching/blue", 3419, 1),
        ("mekanismadditions:plastic/slick/enriching/brown", 3420, 1),
        ("mekanismadditions:plastic/slick/enriching/cyan", 3421, 1),
        ("mekanismadditions:plastic/slick/enriching/dark_red", 3422, 1),
        ("mekanismadditions:plastic/slick/enriching/gray", 3423, 1),
        ("mekanismadditions:plastic/slick/enriching/green", 3424, 1),
        ("mekanismadditions:plastic/slick/enriching/light_blue", 3425, 1),
        ("mekanismadditions:plastic/slick/enriching/light_gray", 3426, 1),
        ("mekanismadditions:plastic/slick/enriching/lime", 3427, 1),
        ("mekanismadditions:plastic/slick/enriching/magenta", 3428, 1),
        ("mekanismadditions:plastic/slick/enriching/orange", 3429, 1),
        ("mekanismadditions:plastic/slick/enriching/pink", 3430, 1),
        ("mekanismadditions:plastic/slick/enriching/purple", 3431, 1),
        ("mekanismadditions:plastic/slick/enriching/red", 3432, 1),
        ("mekanismadditions:plastic/slick/enriching/white", 3433, 1),
        ("mekanismadditions:plastic/slick/enriching/yellow", 3434, 1),
        ("mekanismadditions:plastic/slick/gray", 3435, 1),
        ("mekanismadditions:plastic/slick/green", 3436, 1),
        ("mekanismadditions:plastic/slick/light_blue", 3437, 1),
        ("mekanismadditions:plastic/slick/light_gray", 3438, 1),
        ("mekanismadditions:plastic/slick/lime", 3439, 1),
        ("mekanismadditions:plastic/slick/magenta", 3440, 1),
        ("mekanismadditions:plastic/slick/orange", 3441, 1),
        ("mekanismadditions:plastic/slick/pink", 3442, 1),
        ("mekanismadditions:plastic/slick/purple", 3443, 1),
        ("mekanismadditions:plastic/slick/recolor/black", 3444, 1),
        ("mekanismadditions:plastic/slick/recolor/blue", 3445, 1),
        ("mekanismadditions:plastic/slick/recolor/brown", 3446, 1),
        ("mekanismadditions:plastic/slick/recolor/cyan", 3447, 1),
        ("mekanismadditions:plastic/slick/recolor/gray", 3448, 1),
        ("mekanismadditions:plastic/slick/recolor/green", 3449, 1),
        ("mekanismadditions:plastic/slick/recolor/light_blue", 3450, 1),
        ("mekanismadditions:plastic/slick/recolor/light_gray", 3451, 1),
        ("mekanismadditions:plastic/slick/recolor/lime", 3452, 1),
        ("mekanismadditions:plastic/slick/recolor/magenta", 3453, 1),
        ("mekanismadditions:plastic/slick/recolor/orange", 3454, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/aqua", 3455, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/black", 3456, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/blue", 3457, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/brown", 3458, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/cyan", 3459, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/dark_red", 3460, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/gray", 3461, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/green", 3462, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/light_blue", 3463, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/light_gray", 3464, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/lime", 3465, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/magenta", 3466, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/orange", 3467, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/pink", 3468, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/purple", 3469, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/red", 3470, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/white", 3471, 1),
        ("mekanismadditions:plastic/slick/recolor/painting/yellow", 3472, 1),
        ("mekanismadditions:plastic/slick/recolor/pink", 3473, 1),
        ("mekanismadditions:plastic/slick/recolor/purple", 3474, 1),
        ("mekanismadditions:plastic/slick/recolor/red", 3475, 1),
        ("mekanismadditions:plastic/slick/recolor/white", 3476, 1),
        ("mekanismadditions:plastic/slick/recolor/yellow", 3477, 1),
        ("mekanismadditions:plastic/slick/red", 3478, 1),
        ("mekanismadditions:plastic/slick/white", 3479, 1),
        ("mekanismadditions:plastic/slick/yellow", 3480, 1),
        ("mekanismadditions:plastic/stairs/aqua", 3481, 1),
        ("mekanismadditions:plastic/stairs/black", 3482, 1),
        ("mekanismadditions:plastic/stairs/blue", 3483, 1),
        ("mekanismadditions:plastic/stairs/brown", 3484, 1),
        ("mekanismadditions:plastic/stairs/cyan", 3485, 1),
        ("mekanismadditions:plastic/stairs/dark_red", 3486, 1),
        ("mekanismadditions:plastic/stairs/glow/aqua", 3487, 1),
        ("mekanismadditions:plastic/stairs/glow/black", 3488, 1),
        ("mekanismadditions:plastic/stairs/glow/blue", 3489, 1),
        ("mekanismadditions:plastic/stairs/glow/brown", 3490, 1),
        ("mekanismadditions:plastic/stairs/glow/cyan", 3491, 1),
        ("mekanismadditions:plastic/stairs/glow/dark_red", 3492, 1),
        ("mekanismadditions:plastic/stairs/glow/gray", 3493, 1),
        ("mekanismadditions:plastic/stairs/glow/green", 3494, 1),
        ("mekanismadditions:plastic/stairs/glow/light_blue", 3495, 1),
        ("mekanismadditions:plastic/stairs/glow/light_gray", 3496, 1),
        ("mekanismadditions:plastic/stairs/glow/lime", 3497, 1),
        ("mekanismadditions:plastic/stairs/glow/magenta", 3498, 1),
        ("mekanismadditions:plastic/stairs/glow/orange", 3499, 1),
        ("mekanismadditions:plastic/stairs/glow/pink", 3500, 1),
        ("mekanismadditions:plastic/stairs/glow/purple", 3501, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/black", 3502, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/blue", 3503, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/brown", 3504, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/cyan", 3505, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/gray", 3506, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/green", 3507, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/light_blue", 3508, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/light_gray", 3509, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/lime", 3510, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/magenta", 3511, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/orange", 3512, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/aqua", 3513, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/black", 3514, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/blue", 3515, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/brown", 3516, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/cyan", 3517, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/dark_red", 3518, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/gray", 3519, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/green", 3520, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/light_blue", 3521, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/light_gray", 3522, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/lime", 3523, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/magenta", 3524, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/orange", 3525, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/pink", 3526, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/purple", 3527, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/red", 3528, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/white", 3529, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/painting/yellow", 3530, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/pink", 3531, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/purple", 3532, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/red", 3533, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/white", 3534, 1),
        ("mekanismadditions:plastic/stairs/glow/recolor/yellow", 3535, 1),
        ("mekanismadditions:plastic/stairs/glow/red", 3536, 1),
        ("mekanismadditions:plastic/stairs/glow/white", 3537, 1),
        ("mekanismadditions:plastic/stairs/glow/yellow", 3538, 1),
        ("mekanismadditions:plastic/stairs/gray", 3539, 1),
        ("mekanismadditions:plastic/stairs/green", 3540, 1),
        ("mekanismadditions:plastic/stairs/light_blue", 3541, 1),
        ("mekanismadditions:plastic/stairs/light_gray", 3542, 1),
        ("mekanismadditions:plastic/stairs/lime", 3543, 1),
        ("mekanismadditions:plastic/stairs/magenta", 3544, 1),
        ("mekanismadditions:plastic/stairs/orange", 3545, 1),
        ("mekanismadditions:plastic/stairs/pink", 3546, 1),
        ("mekanismadditions:plastic/stairs/purple", 3547, 1),
        ("mekanismadditions:plastic/stairs/recolor/black", 3548, 1),
        ("mekanismadditions:plastic/stairs/recolor/blue", 3549, 1),
        ("mekanismadditions:plastic/stairs/recolor/brown", 3550, 1),
        ("mekanismadditions:plastic/stairs/recolor/cyan", 3551, 1),
        ("mekanismadditions:plastic/stairs/recolor/gray", 3552, 1),
        ("mekanismadditions:plastic/stairs/recolor/green", 3553, 1),
        ("mekanismadditions:plastic/stairs/recolor/light_blue", 3554, 1),
        ("mekanismadditions:plastic/stairs/recolor/light_gray", 3555, 1),
        ("mekanismadditions:plastic/stairs/recolor/lime", 3556, 1),
        ("mekanismadditions:plastic/stairs/recolor/magenta", 3557, 1),
        ("mekanismadditions:plastic/stairs/recolor/orange", 3558, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/aqua", 3559, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/black", 3560, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/blue", 3561, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/brown", 3562, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/cyan", 3563, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/dark_red", 3564, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/gray", 3565, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/green", 3566, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/light_blue", 3567, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/light_gray", 3568, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/lime", 3569, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/magenta", 3570, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/orange", 3571, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/pink", 3572, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/purple", 3573, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/red", 3574, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/white", 3575, 1),
        ("mekanismadditions:plastic/stairs/recolor/painting/yellow", 3576, 1),
        ("mekanismadditions:plastic/stairs/recolor/pink", 3577, 1),
        ("mekanismadditions:plastic/stairs/recolor/purple", 3578, 1),
        ("mekanismadditions:plastic/stairs/recolor/red", 3579, 1),
        ("mekanismadditions:plastic/stairs/recolor/white", 3580, 1),
        ("mekanismadditions:plastic/stairs/recolor/yellow", 3581, 1),
        ("mekanismadditions:plastic/stairs/red", 3582, 1),
        ("mekanismadditions:plastic/stairs/transparent/aqua", 3583, 1),
        ("mekanismadditions:plastic/stairs/transparent/black", 3584, 1),
        ("mekanismadditions:plastic/stairs/transparent/blue", 3585, 1),
        ("mekanismadditions:plastic/stairs/transparent/brown", 3586, 1),
        ("mekanismadditions:plastic/stairs/transparent/cyan", 3587, 1),
        ("mekanismadditions:plastic/stairs/transparent/dark_red", 3588, 1),
        ("mekanismadditions:plastic/stairs/transparent/gray", 3589, 1),
        ("mekanismadditions:plastic/stairs/transparent/green", 3590, 1),
        ("mekanismadditions:plastic/stairs/transparent/light_blue", 3591, 1),
        ("mekanismadditions:plastic/stairs/transparent/light_gray", 3592, 1),
        ("mekanismadditions:plastic/stairs/transparent/lime", 3593, 1),
        ("mekanismadditions:plastic/stairs/transparent/magenta", 3594, 1),
        ("mekanismadditions:plastic/stairs/transparent/orange", 3595, 1),
        ("mekanismadditions:plastic/stairs/transparent/pink", 3596, 1),
        ("mekanismadditions:plastic/stairs/transparent/purple", 3597, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/black", 3598, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/blue", 3599, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/brown", 3600, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/cyan", 3601, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/gray", 3602, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/green", 3603, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/light_blue", 3604, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/light_gray", 3605, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/lime", 3606, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/magenta", 3607, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/orange", 3608, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/aqua", 3609, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/black", 3610, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/blue", 3611, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/brown", 3612, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/cyan", 3613, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/dark_red", 3614, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/gray", 3615, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/green", 3616, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/light_blue", 3617, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/light_gray", 3618, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/lime", 3619, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/magenta", 3620, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/orange", 3621, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/pink", 3622, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/purple", 3623, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/red", 3624, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/white", 3625, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/painting/yellow", 3626, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/pink", 3627, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/purple", 3628, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/red", 3629, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/white", 3630, 1),
        ("mekanismadditions:plastic/stairs/transparent/recolor/yellow", 3631, 1),
        ("mekanismadditions:plastic/stairs/transparent/red", 3632, 1),
        ("mekanismadditions:plastic/stairs/transparent/white", 3633, 1),
        ("mekanismadditions:plastic/stairs/transparent/yellow", 3634, 1),
        ("mekanismadditions:plastic/stairs/white", 3635, 1),
        ("mekanismadditions:plastic/stairs/yellow", 3636, 1),
        ("mekanismadditions:plastic/transparent/black", 3637, 1),
        ("mekanismadditions:plastic/transparent/blue", 3638, 1),
        ("mekanismadditions:plastic/transparent/brown", 3639, 1),
        ("mekanismadditions:plastic/transparent/cyan", 3640, 1),
        ("mekanismadditions:plastic/transparent/gray", 3641, 1),
        ("mekanismadditions:plastic/transparent/green", 3642, 1),
        ("mekanismadditions:plastic/transparent/light_blue", 3643, 1),
        ("mekanismadditions:plastic/transparent/light_gray", 3644, 1),
        ("mekanismadditions:plastic/transparent/lime", 3645, 1),
        ("mekanismadditions:plastic/transparent/magenta", 3646, 1),
        ("mekanismadditions:plastic/transparent/orange", 3647, 1),
        ("mekanismadditions:plastic/transparent/pink", 3648, 1),
        ("mekanismadditions:plastic/transparent/purple", 3649, 1),
        ("mekanismadditions:plastic/transparent/recolor/black", 3650, 1),
        ("mekanismadditions:plastic/transparent/recolor/blue", 3651, 1),
        ("mekanismadditions:plastic/transparent/recolor/brown", 3652, 1),
        ("mekanismadditions:plastic/transparent/recolor/cyan", 3653, 1),
        ("mekanismadditions:plastic/transparent/recolor/gray", 3654, 1),
        ("mekanismadditions:plastic/transparent/recolor/green", 3655, 1),
        ("mekanismadditions:plastic/transparent/recolor/light_blue", 3656, 1),
        ("mekanismadditions:plastic/transparent/recolor/light_gray", 3657, 1),
        ("mekanismadditions:plastic/transparent/recolor/lime", 3658, 1),
        ("mekanismadditions:plastic/transparent/recolor/magenta", 3659, 1),
        ("mekanismadditions:plastic/transparent/recolor/orange", 3660, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/aqua", 3661, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/black", 3662, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/blue", 3663, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/brown", 3664, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/cyan", 3665, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/dark_red", 3666, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/gray", 3667, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/green", 3668, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/light_blue", 3669, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/light_gray", 3670, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/lime", 3671, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/magenta", 3672, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/orange", 3673, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/pink", 3674, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/purple", 3675, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/red", 3676, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/white", 3677, 1),
        ("mekanismadditions:plastic/transparent/recolor/painting/yellow", 3678, 1),
        ("mekanismadditions:plastic/transparent/recolor/pink", 3679, 1),
        ("mekanismadditions:plastic/transparent/recolor/purple", 3680, 1),
        ("mekanismadditions:plastic/transparent/recolor/red", 3681, 1),
        ("mekanismadditions:plastic/transparent/recolor/white", 3682, 1),
        ("mekanismadditions:plastic/transparent/recolor/yellow", 3683, 1),
        ("mekanismadditions:plastic/transparent/red", 3684, 1),
        ("mekanismadditions:plastic/transparent/white", 3685, 1),
        ("mekanismadditions:plastic/transparent/yellow", 3686, 1),
        ("minecraft:polished_andesite", 3687, 1),
        ("minecraft:polished_andesite_from_andesite_stonecutting", 3688, 1),
        ("minecraft:polished_andesite_slab", 3689, 1),
        ("minecraft:polished_andesite_slab_from_andesite_stonecutting", 3690, 1),
        ("minecraft:polished_andesite_slab_from_polished_andesite_stonecutting", 3691, 1),
        ("minecraft:polished_andesite_stairs", 3692, 1),
        ("minecraft:polished_andesite_stairs_from_andesite_stonecutting", 3693, 1),
        ("minecraft:polished_andesite_stairs_from_polished_andesite_stonecutting", 3694, 1),
        ("minecraft:polished_basalt", 3695, 1),
        ("minecraft:polished_basalt_from_basalt_stonecutting", 3696, 1),
        ("minecraft:polished_blackstone", 3697, 1),
        ("minecraft:polished_blackstone_brick_slab", 3698, 1),
        ("minecraft:polished_blackstone_brick_slab_from_blackstone_stonecutting", 3699, 1),
        ("minecraft:polished_blackstone_brick_slab_from_polished_blackstone_bricks_stonecutting", 3700, 1),
        ("minecraft:polished_blackstone_brick_slab_from_polished_blackstone_stonecutting", 3701, 1),
        ("minecraft:polished_blackstone_brick_stairs", 3702, 1),
        ("minecraft:polished_blackstone_brick_stairs_from_blackstone_stonecutting", 3703, 1),
        ("minecraft:polished_blackstone_brick_stairs_from_polished_blackstone_bricks_stonecutting", 3704, 1),
        ("minecraft:polished_blackstone_brick_stairs_from_polished_blackstone_stonecutting", 3705, 1),
        ("minecraft:polished_blackstone_brick_wall", 3706, 1),
        ("minecraft:polished_blackstone_brick_wall_from_blackstone_stonecutting", 3707, 1),
        ("minecraft:polished_blackstone_brick_wall_from_polished_blackstone_bricks_stonecutting", 3708, 1),
        ("minecraft:polished_blackstone_brick_wall_from_polished_blackstone_stonecutting", 3709, 1),
        ("minecraft:polished_blackstone_bricks", 3710, 1),
        ("minecraft:polished_blackstone_bricks_from_blackstone_stonecutting", 3711, 1),
        ("minecraft:polished_blackstone_bricks_from_polished_blackstone_stonecutting", 3712, 1),
        ("minecraft:polished_blackstone_button", 3713, 1),
        ("minecraft:polished_blackstone_from_blackstone_stonecutting", 3714, 1),
        ("minecraft:polished_blackstone_pressure_plate", 3715, 1),
        ("minecraft:polished_blackstone_slab", 3716, 1),
        ("minecraft:polished_blackstone_slab_from_blackstone_stonecutting", 3717, 1),
        ("minecraft:polished_blackstone_slab_from_polished_blackstone_stonecutting", 3718, 1),
        ("minecraft:polished_blackstone_stairs", 3719, 1),
        ("minecraft:polished_blackstone_stairs_from_blackstone_stonecutting", 3720, 1),
        ("minecraft:polished_blackstone_stairs_from_polished_blackstone_stonecutting", 3721, 1),
        ("minecraft:polished_blackstone_wall", 3722, 1),
        ("minecraft:polished_blackstone_wall_from_blackstone_stonecutting", 3723, 1),
        ("minecraft:polished_blackstone_wall_from_polished_blackstone_stonecutting", 3724, 1),
        ("minecraft:polished_cinnabar", 3725, 1),
        ("minecraft:polished_cinnabar_from_cinnabar_stonecutting", 3726, 1),
        ("minecraft:polished_cinnabar_slab", 3727, 1),
        ("minecraft:polished_cinnabar_slab_from_cinnabar_stonecutting", 3728, 1),
        ("minecraft:polished_cinnabar_slab_from_polished_cinnabar_stonecutting", 3729, 1),
        ("minecraft:polished_cinnabar_stairs", 3730, 1),
        ("minecraft:polished_cinnabar_stairs_from_cinnabar_stonecutting", 3731, 1),
        ("minecraft:polished_cinnabar_stairs_from_polished_cinnabar_stonecutting", 3732, 1),
        ("minecraft:polished_cinnabar_wall", 3733, 1),
        ("minecraft:polished_cinnabar_wall_from_cinnabar_stonecutting", 3734, 1),
        ("minecraft:polished_cinnabar_wall_from_polished_cinnabar_stonecutting", 3735, 1),
        ("minecraft:polished_deepslate", 3736, 1),
        ("minecraft:polished_deepslate_from_cobbled_deepslate_stonecutting", 3737, 1),
        ("minecraft:polished_deepslate_from_deepslate_stonecutting", 3738, 1),
        ("minecraft:polished_deepslate_slab", 3739, 1),
        ("minecraft:polished_deepslate_slab_from_cobbled_deepslate_stonecutting", 3740, 1),
        ("minecraft:polished_deepslate_slab_from_deepslate_stonecutting", 3741, 1),
        ("minecraft:polished_deepslate_slab_from_polished_deepslate_stonecutting", 3742, 1),
        ("minecraft:polished_deepslate_stairs", 3743, 1),
        ("minecraft:polished_deepslate_stairs_from_cobbled_deepslate_stonecutting", 3744, 1),
        ("minecraft:polished_deepslate_stairs_from_deepslate_stonecutting", 3745, 1),
        ("minecraft:polished_deepslate_stairs_from_polished_deepslate_stonecutting", 3746, 1),
        ("minecraft:polished_deepslate_wall", 3747, 1),
        ("minecraft:polished_deepslate_wall_from_cobbled_deepslate_stonecutting", 3748, 1),
        ("minecraft:polished_deepslate_wall_from_deepslate_stonecutting", 3749, 1),
        ("minecraft:polished_deepslate_wall_from_polished_deepslate_stonecutting", 3750, 1),
        ("minecraft:polished_diorite", 3751, 1),
        ("minecraft:polished_diorite_from_diorite_stonecutting", 3752, 1),
        ("minecraft:polished_diorite_slab", 3753, 1),
        ("minecraft:polished_diorite_slab_from_diorite_stonecutting", 3754, 1),
        ("minecraft:polished_diorite_slab_from_polished_diorite_stonecutting", 3755, 1),
        ("minecraft:polished_diorite_stairs", 3756, 1),
        ("minecraft:polished_diorite_stairs_from_diorite_stonecutting", 3757, 1),
        ("minecraft:polished_diorite_stairs_from_polished_diorite_stonecutting", 3758, 1),
        ("minecraft:polished_granite", 3759, 1),
        ("minecraft:polished_granite_from_granite_stonecutting", 3760, 1),
        ("minecraft:polished_granite_slab", 3761, 1),
        ("minecraft:polished_granite_slab_from_granite_stonecutting", 3762, 1),
        ("minecraft:polished_granite_slab_from_polished_granite_stonecutting", 3763, 1),
        ("minecraft:polished_granite_stairs", 3764, 1),
        ("minecraft:polished_granite_stairs_from_granite_stonecutting", 3765, 1),
        ("minecraft:polished_granite_stairs_from_polished_granite_stonecutting", 3766, 1),
        ("minecraft:polished_sulfur", 3767, 1),
        ("minecraft:polished_sulfur_from_sulfur_stonecutting", 3768, 1),
        ("minecraft:polished_sulfur_slab", 3769, 1),
        ("minecraft:polished_sulfur_slab_from_polished_sulfur_stonecutting", 3770, 1),
        ("minecraft:polished_sulfur_slab_from_sulfur_stonecutting", 3771, 1),
        ("minecraft:polished_sulfur_stairs", 3772, 1),
        ("minecraft:polished_sulfur_stairs_from_polished_sulfur_stonecutting", 3773, 1),
        ("minecraft:polished_sulfur_stairs_from_sulfur_stonecutting", 3774, 1),
        ("minecraft:polished_sulfur_wall", 3775, 1),
        ("minecraft:polished_sulfur_wall_from_polished_sulfur_stonecutting", 3776, 1),
        ("minecraft:polished_sulfur_wall_from_sulfur_stonecutting", 3777, 1),
        ("minecraft:polished_tuff", 3778, 1),
        ("minecraft:polished_tuff_from_tuff_stonecutting", 3779, 1),
        ("minecraft:polished_tuff_slab", 3780, 1),
        ("minecraft:polished_tuff_slab_from_polished_tuff_stonecutting", 3781, 1),
        ("minecraft:polished_tuff_slab_from_tuff_stonecutting", 3782, 1),
        ("minecraft:polished_tuff_stairs", 3783, 1),
        ("minecraft:polished_tuff_stairs_from_polished_tuff_stonecutting", 3784, 1),
        ("minecraft:polished_tuff_stairs_from_tuff_stonecutting", 3785, 1),
        ("minecraft:polished_tuff_wall", 3786, 1),
        ("minecraft:polished_tuff_wall_from_polished_tuff_stonecutting", 3787, 1),
        ("minecraft:polished_tuff_wall_from_tuff_stonecutting", 3788, 1),
        ("minecraft:poplar_boat", 3789, 1),
        ("minecraft:poplar_button", 3790, 1),
        ("minecraft:poplar_chest_boat", 3791, 1),
        ("minecraft:poplar_door", 3792, 1),
        ("minecraft:poplar_fence", 3793, 1),
        ("minecraft:poplar_fence_gate", 3794, 1),
        ("minecraft:poplar_hanging_sign", 3795, 1),
        ("minecraft:poplar_planks", 3796, 1),
        ("minecraft:poplar_pressure_plate", 3797, 1),
        ("minecraft:poplar_shelf", 3798, 1),
        ("minecraft:poplar_sign", 3799, 1),
        ("minecraft:poplar_slab", 3800, 1),
        ("minecraft:poplar_stairs", 3801, 1),
        ("minecraft:poplar_trapdoor", 3802, 1),
        ("minecraft:poplar_wood", 3803, 1),
        ("minecraft:popped_chorus_fruit", 3804, 1),
        ("mekanism:portable_qio_dashboard", 3805, 1),
        ("mekanism:portable_teleporter", 3806, 1),
        ("minecraft:potent_sulfur", 3807, 1),
        ("minecraft:powered_rail", 3808, 1),
        ("mekanism:precision_sawmill", 3809, 1),
        ("mekanism:pressure_disperser", 3810, 1),
        ("mekanism:pressurized_reaction_chamber", 3811, 1),
        ("minecraft:prismarine", 3812, 1),
        ("minecraft:prismarine_brick_slab", 3813, 1),
        ("minecraft:prismarine_brick_slab_from_prismarine_bricks_stonecutting", 3814, 1),
        ("minecraft:prismarine_brick_stairs", 3815, 1),
        ("minecraft:prismarine_brick_stairs_from_prismarine_bricks_stonecutting", 3816, 1),
        ("minecraft:prismarine_bricks", 3817, 1),
        ("minecraft:prismarine_slab", 3818, 1),
        ("minecraft:prismarine_slab_from_prismarine_stonecutting", 3819, 1),
        ("minecraft:prismarine_stairs", 3820, 1),
        ("minecraft:prismarine_stairs_from_prismarine_stonecutting", 3821, 1),
        ("minecraft:prismarine_wall", 3822, 1),
        ("minecraft:prismarine_wall_from_prismarine_stonecutting", 3823, 1),
        ("mekanism:processing/bronze/dust/from_infusing", 3824, 1),
        ("mekanism:processing/bronze/dust/from_ingot", 3825, 1),
        ("mekanism:processing/bronze/ingot/from_block", 3826, 1),
        ("mekanism:processing/bronze/ingot/from_dust_blasting", 3827, 1),
        ("mekanism:processing/bronze/ingot/from_dust_smelting", 3828, 1),
        ("mekanism:processing/bronze/ingot/from_infusing", 3829, 1),
        ("mekanism:processing/bronze/ingot/from_nuggets", 3830, 1),
        ("mekanism:processing/coal/from_dust", 3831, 1),
        ("mekanism:processing/coal/from_ore", 3832, 1),
        ("mekanism:processing/coal/to_deepslate_ore", 3833, 1),
        ("mekanism:processing/coal/to_dust", 3834, 1),
        ("mekanism:processing/coal/to_ore", 3835, 1),
        ("mekanism:processing/copper/clump/from_ore", 3836, 1),
        ("mekanism:processing/copper/clump/from_raw_block", 3837, 1),
        ("mekanism:processing/copper/clump/from_raw_ore", 3838, 1),
        ("mekanism:processing/copper/clump/from_shard", 3839, 1),
        ("mekanism:processing/copper/crystal/from_slurry", 3840, 1),
        ("mekanism:processing/copper/dirty_dust/from_clump", 3841, 1),
        ("mekanism:processing/copper/dust/from_dirty_dust", 3842, 1),
        ("mekanism:processing/copper/dust/from_ingot", 3843, 1),
        ("mekanism:processing/copper/dust/from_ore", 3844, 1),
        ("mekanism:processing/copper/dust/from_raw_block", 3845, 1),
        ("mekanism:processing/copper/dust/from_raw_ore", 3846, 1),
        ("mekanism:processing/copper/ingot/from_dust_blasting", 3847, 1),
        ("mekanism:processing/copper/ingot/from_dust_smelting", 3848, 1),
        ("mekanism:processing/copper/ore/deepslate_from_raw", 3849, 1),
        ("mekanism:processing/copper/ore/from_raw", 3850, 1),
        ("mekanism:processing/copper/shard/from_crystal", 3851, 1),
        ("mekanism:processing/copper/shard/from_ore", 3852, 1),
        ("mekanism:processing/copper/shard/from_raw_block", 3853, 1),
        ("mekanism:processing/copper/shard/from_raw_ore", 3854, 1),
        ("mekanism:processing/copper/slurry/clean", 3855, 1),
        ("mekanism:processing/copper/slurry/dirty/from_ore", 3856, 1),
        ("mekanism:processing/copper/slurry/dirty/from_raw_block", 3857, 1),
        ("mekanism:processing/copper/slurry/dirty/from_raw_ore", 3858, 1),
        ("mekanism:processing/diamond/from_dust", 3859, 1),
        ("mekanism:processing/diamond/from_ore", 3860, 1),
        ("mekanism:processing/diamond/to_deepslate_ore", 3861, 1),
        ("mekanism:processing/diamond/to_dust", 3862, 1),
        ("mekanism:processing/diamond/to_ore", 3863, 1),
        ("mekanism:processing/emerald/from_dust", 3864, 1),
        ("mekanism:processing/emerald/from_ore", 3865, 1),
        ("mekanism:processing/emerald/to_deepslate_ore", 3866, 1),
        ("mekanism:processing/emerald/to_dust", 3867, 1),
        ("mekanism:processing/emerald/to_ore", 3868, 1),
        ("mekanism:processing/fluorite/from_block", 3869, 1),
        ("mekanism:processing/fluorite/from_deepslate_ore_blasting", 3870, 1),
        ("mekanism:processing/fluorite/from_deepslate_ore_smelting", 3871, 1),
        ("mekanism:processing/fluorite/from_dust", 3872, 1),
        ("mekanism:processing/fluorite/from_ore", 3873, 1),
        ("mekanism:processing/fluorite/from_ore_blasting", 3874, 1),
        ("mekanism:processing/fluorite/from_ore_smelting", 3875, 1),
        ("mekanism:processing/fluorite/to_deepslate_ore", 3876, 1),
        ("mekanism:processing/fluorite/to_dust", 3877, 1),
        ("mekanism:processing/fluorite/to_ore", 3878, 1),
        ("mekanism:processing/gold/clump/from_ore", 3879, 1),
        ("mekanism:processing/gold/clump/from_raw_block", 3880, 1),
        ("mekanism:processing/gold/clump/from_raw_ore", 3881, 1),
        ("mekanism:processing/gold/clump/from_shard", 3882, 1),
        ("mekanism:processing/gold/crystal/from_slurry", 3883, 1),
        ("mekanism:processing/gold/dirty_dust/from_clump", 3884, 1),
        ("mekanism:processing/gold/dust/from_dirty_dust", 3885, 1),
        ("mekanism:processing/gold/dust/from_ingot", 3886, 1),
        ("mekanism:processing/gold/dust/from_ore", 3887, 1),
        ("mekanism:processing/gold/dust/from_raw_block", 3888, 1),
        ("mekanism:processing/gold/dust/from_raw_ore", 3889, 1),
        ("mekanism:processing/gold/ingot/from_dust_blasting", 3890, 1),
        ("mekanism:processing/gold/ingot/from_dust_smelting", 3891, 1),
        ("mekanism:processing/gold/ore/deepslate_from_raw", 3892, 1),
        ("mekanism:processing/gold/ore/from_raw", 3893, 1),
        ("mekanism:processing/gold/ore/nether_from_raw", 3894, 1),
        ("mekanism:processing/gold/shard/from_crystal", 3895, 1),
        ("mekanism:processing/gold/shard/from_ore", 3896, 1),
        ("mekanism:processing/gold/shard/from_raw_block", 3897, 1),
        ("mekanism:processing/gold/shard/from_raw_ore", 3898, 1),
        ("mekanism:processing/gold/slurry/clean", 3899, 1),
        ("mekanism:processing/gold/slurry/dirty/from_ore", 3900, 1),
        ("mekanism:processing/gold/slurry/dirty/from_raw_block", 3901, 1),
        ("mekanism:processing/gold/slurry/dirty/from_raw_ore", 3902, 1),
        ("mekanism:processing/iron/clump/from_ore", 3903, 1),
        ("mekanism:processing/iron/clump/from_raw_block", 3904, 1),
        ("mekanism:processing/iron/clump/from_raw_ore", 3905, 1),
        ("mekanism:processing/iron/clump/from_shard", 3906, 1),
        ("mekanism:processing/iron/crystal/from_slurry", 3907, 1),
        ("mekanism:processing/iron/dirty_dust/from_clump", 3908, 1),
        ("mekanism:processing/iron/dust/from_dirty_dust", 3909, 1),
        ("mekanism:processing/iron/dust/from_ingot", 3910, 1),
        ("mekanism:processing/iron/dust/from_ore", 3911, 1),
        ("mekanism:processing/iron/dust/from_raw_block", 3912, 1),
        ("mekanism:processing/iron/dust/from_raw_ore", 3913, 1),
        ("mekanism:processing/iron/enriched", 3914, 1),
        ("mekanism:processing/iron/enriched_dust", 3915, 1),
        ("mekanism:processing/iron/ingot/from_dust_blasting", 3916, 1),
        ("mekanism:processing/iron/ingot/from_dust_smelting", 3917, 1),
        ("mekanism:processing/iron/ore/deepslate_from_raw", 3918, 1),
        ("mekanism:processing/iron/ore/from_raw", 3919, 1),
        ("mekanism:processing/iron/shard/from_crystal", 3920, 1),
        ("mekanism:processing/iron/shard/from_ore", 3921, 1),
        ("mekanism:processing/iron/shard/from_raw_block", 3922, 1),
        ("mekanism:processing/iron/shard/from_raw_ore", 3923, 1),
        ("mekanism:processing/iron/slurry/clean", 3924, 1),
        ("mekanism:processing/iron/slurry/dirty/from_ore", 3925, 1),
        ("mekanism:processing/iron/slurry/dirty/from_raw_block", 3926, 1),
        ("mekanism:processing/iron/slurry/dirty/from_raw_ore", 3927, 1),
        ("mekanism:processing/lapis_lazuli/from_dust", 3928, 1),
        ("mekanism:processing/lapis_lazuli/from_ore", 3929, 1),
        ("mekanism:processing/lapis_lazuli/to_deepslate_ore", 3930, 1),
        ("mekanism:processing/lapis_lazuli/to_dust", 3931, 1),
        ("mekanism:processing/lapis_lazuli/to_ore", 3932, 1),
        ("mekanism:processing/lategame/antimatter/from_pellet", 3933, 1),
        ("mekanism:processing/lategame/antimatter_pellet/from_gas", 3934, 1),
        ("mekanism:processing/lategame/plutonium", 3935, 1),
        ("mekanism:processing/lategame/plutonium_pellet/from_reaction", 3936, 1),
        ("mekanism:processing/lategame/polonium", 3937, 1),
        ("mekanism:processing/lategame/polonium_pellet/from_reaction", 3938, 1),
        ("mekanism:processing/lead/clump/from_ore", 3939, 1),
        ("mekanism:processing/lead/clump/from_raw_block", 3940, 1),
        ("mekanism:processing/lead/clump/from_raw_ore", 3941, 1),
        ("mekanism:processing/lead/clump/from_shard", 3942, 1),
        ("mekanism:processing/lead/crystal/from_slurry", 3943, 1),
        ("mekanism:processing/lead/dirty_dust/from_clump", 3944, 1),
        ("mekanism:processing/lead/dust/from_dirty_dust", 3945, 1),
        ("mekanism:processing/lead/dust/from_ingot", 3946, 1),
        ("mekanism:processing/lead/dust/from_ore", 3947, 1),
        ("mekanism:processing/lead/dust/from_raw_block", 3948, 1),
        ("mekanism:processing/lead/dust/from_raw_ore", 3949, 1),
        ("mekanism:processing/lead/ingot/from_block", 3950, 1),
        ("mekanism:processing/lead/ingot/from_deepslate_ore_blasting", 3951, 1),
        ("mekanism:processing/lead/ingot/from_deepslate_ore_smelting", 3952, 1),
        ("mekanism:processing/lead/ingot/from_dust_blasting", 3953, 1),
        ("mekanism:processing/lead/ingot/from_dust_smelting", 3954, 1),
        ("mekanism:processing/lead/ingot/from_nuggets", 3955, 1),
        ("mekanism:processing/lead/ingot/from_ore_blasting", 3956, 1),
        ("mekanism:processing/lead/ingot/from_ore_smelting", 3957, 1),
        ("mekanism:processing/lead/ingot/from_raw_blasting", 3958, 1),
        ("mekanism:processing/lead/ingot/from_raw_smelting", 3959, 1),
        ("mekanism:processing/lead/nugget/from_ingot", 3960, 1),
        ("mekanism:processing/lead/ore/deepslate_from_raw", 3961, 1),
        ("mekanism:processing/lead/ore/from_raw", 3962, 1),
        ("mekanism:processing/lead/raw/from_raw_block", 3963, 1),
        ("mekanism:processing/lead/raw_storage_blocks/from_raw", 3964, 1),
        ("mekanism:processing/lead/shard/from_crystal", 3965, 1),
        ("mekanism:processing/lead/shard/from_ore", 3966, 1),
        ("mekanism:processing/lead/shard/from_raw_block", 3967, 1),
        ("mekanism:processing/lead/shard/from_raw_ore", 3968, 1),
        ("mekanism:processing/lead/slurry/clean", 3969, 1),
        ("mekanism:processing/lead/slurry/dirty/from_ore", 3970, 1),
        ("mekanism:processing/lead/slurry/dirty/from_raw_block", 3971, 1),
        ("mekanism:processing/lead/slurry/dirty/from_raw_ore", 3972, 1),
        ("mekanism:processing/lead/storage_blocks/from_ingots", 3973, 1),
        ("mekanism:processing/netherite/ancient_debris_to_dirty_scrap", 3974, 1),
        ("mekanism:processing/netherite/ancient_debris_to_scrap", 3975, 1),
        ("mekanism:processing/netherite/dirty_scrap_to_scrap", 3976, 1),
        ("mekanism:processing/netherite/dust_to_ancient_debris", 3977, 1),
        ("mekanism:processing/netherite/ingot_from_dust_blasting", 3978, 1),
        ("mekanism:processing/netherite/ingot_from_dust_smelting", 3979, 1),
        ("mekanism:processing/netherite/ingot_to_dust", 3980, 1),
        ("mekanism:processing/netherite/scrap_to_dust", 3981, 1),
        ("mekanism:processing/osmium/clump/from_ore", 3982, 1),
        ("mekanism:processing/osmium/clump/from_raw_block", 3983, 1),
        ("mekanism:processing/osmium/clump/from_raw_ore", 3984, 1),
        ("mekanism:processing/osmium/clump/from_shard", 3985, 1),
        ("mekanism:processing/osmium/crystal/from_slurry", 3986, 1),
        ("mekanism:processing/osmium/dirty_dust/from_clump", 3987, 1),
        ("mekanism:processing/osmium/dust/from_dirty_dust", 3988, 1),
        ("mekanism:processing/osmium/dust/from_ingot", 3989, 1),
        ("mekanism:processing/osmium/dust/from_ore", 3990, 1),
        ("mekanism:processing/osmium/dust/from_raw_block", 3991, 1),
        ("mekanism:processing/osmium/dust/from_raw_ore", 3992, 1),
        ("mekanism:processing/osmium/ingot/from_block", 3993, 1),
        ("mekanism:processing/osmium/ingot/from_deepslate_ore_blasting", 3994, 1),
        ("mekanism:processing/osmium/ingot/from_deepslate_ore_smelting", 3995, 1),
        ("mekanism:processing/osmium/ingot/from_dust_blasting", 3996, 1),
        ("mekanism:processing/osmium/ingot/from_dust_smelting", 3997, 1),
        ("mekanism:processing/osmium/ingot/from_nuggets", 3998, 1),
        ("mekanism:processing/osmium/ingot/from_ore_blasting", 3999, 1),
        ("mekanism:processing/osmium/ingot/from_ore_smelting", 4000, 1),
        ("mekanism:processing/osmium/ingot/from_raw_blasting", 4001, 1),
        ("mekanism:processing/osmium/ingot/from_raw_smelting", 4002, 1),
        ("mekanism:processing/osmium/nugget/from_ingot", 4003, 1),
        ("mekanism:processing/osmium/ore/deepslate_from_raw", 4004, 1),
        ("mekanism:processing/osmium/ore/from_raw", 4005, 1),
        ("mekanism:processing/osmium/raw/from_raw_block", 4006, 1),
        ("mekanism:processing/osmium/raw_storage_blocks/from_raw", 4007, 1),
        ("mekanism:processing/osmium/shard/from_crystal", 4008, 1),
        ("mekanism:processing/osmium/shard/from_ore", 4009, 1),
        ("mekanism:processing/osmium/shard/from_raw_block", 4010, 1),
        ("mekanism:processing/osmium/shard/from_raw_ore", 4011, 1),
        ("mekanism:processing/osmium/slurry/clean", 4012, 1),
        ("mekanism:processing/osmium/slurry/dirty/from_ore", 4013, 1),
        ("mekanism:processing/osmium/slurry/dirty/from_raw_block", 4014, 1),
        ("mekanism:processing/osmium/slurry/dirty/from_raw_ore", 4015, 1),
        ("mekanism:processing/osmium/storage_blocks/from_ingots", 4016, 1),
        ("mekanism:processing/quartz/from_dust", 4017, 1),
        ("mekanism:processing/quartz/from_ore", 4018, 1),
        ("mekanism:processing/quartz/to_dust", 4019, 1),
        ("mekanism:processing/quartz/to_ore", 4020, 1),
        ("mekanism:processing/redstone/from_ore", 4021, 1),
        ("mekanism:processing/redstone/to_deepslate_ore", 4022, 1),
        ("mekanism:processing/redstone/to_ore", 4023, 1),
        ("mekanism:processing/refined_glowstone/ingot/from_block", 4024, 1),
        ("mekanism:processing/refined_glowstone/ingot/from_dust", 4025, 1),
        ("mekanism:processing/refined_glowstone/ingot/from_nuggets", 4026, 1),
        ("mekanism:processing/refined_glowstone/ingot_to_dust", 4027, 1),
        ("mekanism:processing/refined_obsidian/dust/from_ingot", 4028, 1),
        ("mekanism:processing/refined_obsidian/dust/from_obsidian_dust", 4029, 1),
        ("mekanism:processing/refined_obsidian/ingot/from_block", 4030, 1),
        ("mekanism:processing/refined_obsidian/ingot/from_dust", 4031, 1),
        ("mekanism:processing/refined_obsidian/ingot/from_nuggets", 4032, 1),
        ("mekanism:processing/steel/enriched_iron_to_dust", 4033, 1),
        ("mekanism:processing/steel/ingot/from_block", 4034, 1),
        ("mekanism:processing/steel/ingot/from_dust_blasting", 4035, 1),
        ("mekanism:processing/steel/ingot/from_dust_smelting", 4036, 1),
        ("mekanism:processing/steel/ingot/from_nuggets", 4037, 1),
        ("mekanism:processing/steel/ingot_to_dust", 4038, 1),
        ("mekanism:processing/tin/clump/from_ore", 4039, 1),
        ("mekanism:processing/tin/clump/from_raw_block", 4040, 1),
        ("mekanism:processing/tin/clump/from_raw_ore", 4041, 1),
        ("mekanism:processing/tin/clump/from_shard", 4042, 1),
        ("mekanism:processing/tin/crystal/from_slurry", 4043, 1),
        ("mekanism:processing/tin/dirty_dust/from_clump", 4044, 1),
        ("mekanism:processing/tin/dust/from_dirty_dust", 4045, 1),
        ("mekanism:processing/tin/dust/from_ingot", 4046, 1),
        ("mekanism:processing/tin/dust/from_ore", 4047, 1),
        ("mekanism:processing/tin/dust/from_raw_block", 4048, 1),
        ("mekanism:processing/tin/dust/from_raw_ore", 4049, 1),
        ("mekanism:processing/tin/ingot/from_block", 4050, 1),
        ("mekanism:processing/tin/ingot/from_deepslate_ore_blasting", 4051, 1),
        ("mekanism:processing/tin/ingot/from_deepslate_ore_smelting", 4052, 1),
        ("mekanism:processing/tin/ingot/from_dust_blasting", 4053, 1),
        ("mekanism:processing/tin/ingot/from_dust_smelting", 4054, 1),
        ("mekanism:processing/tin/ingot/from_nuggets", 4055, 1),
        ("mekanism:processing/tin/ingot/from_ore_blasting", 4056, 1),
        ("mekanism:processing/tin/ingot/from_ore_smelting", 4057, 1),
        ("mekanism:processing/tin/ingot/from_raw_blasting", 4058, 1),
        ("mekanism:processing/tin/ingot/from_raw_smelting", 4059, 1),
        ("mekanism:processing/tin/nugget/from_ingot", 4060, 1),
        ("mekanism:processing/tin/ore/deepslate_from_raw", 4061, 1),
        ("mekanism:processing/tin/ore/from_raw", 4062, 1),
        ("mekanism:processing/tin/raw/from_raw_block", 4063, 1),
        ("mekanism:processing/tin/raw_storage_blocks/from_raw", 4064, 1),
        ("mekanism:processing/tin/shard/from_crystal", 4065, 1),
        ("mekanism:processing/tin/shard/from_ore", 4066, 1),
        ("mekanism:processing/tin/shard/from_raw_block", 4067, 1),
        ("mekanism:processing/tin/shard/from_raw_ore", 4068, 1),
        ("mekanism:processing/tin/slurry/clean", 4069, 1),
        ("mekanism:processing/tin/slurry/dirty/from_ore", 4070, 1),
        ("mekanism:processing/tin/slurry/dirty/from_raw_block", 4071, 1),
        ("mekanism:processing/tin/slurry/dirty/from_raw_ore", 4072, 1),
        ("mekanism:processing/tin/storage_blocks/from_ingots", 4073, 1),
        ("mekanism:processing/uranium/clump/from_ore", 4074, 1),
        ("mekanism:processing/uranium/clump/from_raw_block", 4075, 1),
        ("mekanism:processing/uranium/clump/from_raw_ore", 4076, 1),
        ("mekanism:processing/uranium/clump/from_shard", 4077, 1),
        ("mekanism:processing/uranium/crystal/from_slurry", 4078, 1),
        ("mekanism:processing/uranium/dirty_dust/from_clump", 4079, 1),
        ("mekanism:processing/uranium/dust/from_dirty_dust", 4080, 1),
        ("mekanism:processing/uranium/dust/from_ingot", 4081, 1),
        ("mekanism:processing/uranium/dust/from_ore", 4082, 1),
        ("mekanism:processing/uranium/dust/from_raw_block", 4083, 1),
        ("mekanism:processing/uranium/dust/from_raw_ore", 4084, 1),
        ("mekanism:processing/uranium/fissile_fuel", 4085, 1),
        ("mekanism:processing/uranium/hydrofluoric_acid", 4086, 1),
        ("mekanism:processing/uranium/hydrofluoric_acid_from_block", 4087, 1),
        ("mekanism:processing/uranium/ingot/from_block", 4088, 1),
        ("mekanism:processing/uranium/ingot/from_deepslate_ore_blasting", 4089, 1),
        ("mekanism:processing/uranium/ingot/from_deepslate_ore_smelting", 4090, 1),
        ("mekanism:processing/uranium/ingot/from_dust_blasting", 4091, 1),
        ("mekanism:processing/uranium/ingot/from_dust_smelting", 4092, 1),
        ("mekanism:processing/uranium/ingot/from_nuggets", 4093, 1),
        ("mekanism:processing/uranium/ingot/from_ore_blasting", 4094, 1),
        ("mekanism:processing/uranium/ingot/from_ore_smelting", 4095, 1),
        ("mekanism:processing/uranium/ingot/from_raw_blasting", 4096, 1),
        ("mekanism:processing/uranium/ingot/from_raw_smelting", 4097, 1),
        ("mekanism:processing/uranium/nugget/from_ingot", 4098, 1),
        ("mekanism:processing/uranium/ore/deepslate_from_raw", 4099, 1),
        ("mekanism:processing/uranium/ore/from_raw", 4100, 1),
        ("mekanism:processing/uranium/raw/from_raw_block", 4101, 1),
        ("mekanism:processing/uranium/raw_storage_blocks/from_raw", 4102, 1),
        ("mekanism:processing/uranium/reprocessing/from_plutonium", 4103, 1),
        ("mekanism:processing/uranium/reprocessing/to_fuel", 4104, 1),
        ("mekanism:processing/uranium/shard/from_crystal", 4105, 1),
        ("mekanism:processing/uranium/shard/from_ore", 4106, 1),
        ("mekanism:processing/uranium/shard/from_raw_block", 4107, 1),
        ("mekanism:processing/uranium/shard/from_raw_ore", 4108, 1),
        ("mekanism:processing/uranium/slurry/clean", 4109, 1),
        ("mekanism:processing/uranium/slurry/dirty/from_ore", 4110, 1),
        ("mekanism:processing/uranium/slurry/dirty/from_raw_block", 4111, 1),
        ("mekanism:processing/uranium/slurry/dirty/from_raw_ore", 4112, 1),
        ("mekanism:processing/uranium/storage_blocks/from_ingots", 4113, 1),
        ("mekanism:processing/uranium/sulfuric_acid", 4114, 1),
        ("mekanism:processing/uranium/uranium_oxide", 4115, 1),
        ("mekanism:processing/uranium/yellow_cake_uranium", 4116, 1),
        ("minecraft:pumpkin_pie", 4117, 1),
        ("minecraft:pumpkin_seeds", 4118, 1),
        ("mekanism:purification_chamber", 4119, 1),
        ("minecraft:purple_banner", 4120, 1),
        ("minecraft:purple_bed", 4121, 1),
        ("minecraft:purple_bundle", 4122, 1),
        ("minecraft:purple_candle", 4123, 1),
        ("minecraft:purple_carpet", 4124, 1),
        ("minecraft:purple_concrete_powder", 4125, 1),
        ("minecraft:purple_concrete_slab", 4126, 1),
        ("minecraft:purple_concrete_slab_from_purple_concrete_stonecutting", 4127, 1),
        ("minecraft:purple_concrete_stairs", 4128, 1),
        ("minecraft:purple_concrete_stairs_from_purple_concrete_stonecutting", 4129, 1),
        ("minecraft:purple_cushion", 4130, 1),
        ("minecraft:purple_dye", 4131, 1),
        ("minecraft:purple_glazed_terracotta", 4132, 1),
        ("minecraft:purple_harness", 4133, 1),
        ("minecraft:purple_shulker_box", 4134, 1),
        ("minecraft:purple_stained_glass", 4135, 1),
        ("minecraft:purple_stained_glass_pane", 4136, 1),
        ("minecraft:purple_stained_glass_pane_from_glass_pane", 4137, 1),
        ("minecraft:purple_terracotta", 4138, 1),
        ("minecraft:purple_wool_slab", 4139, 1),
        ("minecraft:purple_wool_stairs", 4140, 1),
        ("minecraft:purpur_block", 4141, 1),
        ("minecraft:purpur_pillar", 4142, 1),
        ("minecraft:purpur_pillar_from_purpur_block_stonecutting", 4143, 1),
        ("minecraft:purpur_slab", 4144, 1),
        ("minecraft:purpur_slab_from_purpur_block_stonecutting", 4145, 1),
        ("minecraft:purpur_stairs", 4146, 1),
        ("minecraft:purpur_stairs_from_purpur_block_stonecutting", 4147, 1),
        ("mekanism:qio_dashboard", 4148, 1),
        ("mekanism:qio_drive_array", 4149, 1),
        ("mekanism:qio_drive_base", 4150, 1),
        ("mekanism:qio_drive_hyper_dense", 4151, 1),
        ("mekanism:qio_drive_supermassive", 4152, 1),
        ("mekanism:qio_drive_time_dilating", 4153, 1),
        ("mekanism:qio_exporter", 4154, 1),
        ("mekanism:qio_importer", 4155, 1),
        ("mekanism:qio_redstone_adapter", 4156, 1),
        ("mekanism:quantum_entangloporter", 4157, 1),
        ("minecraft:quartz", 4158, 1),
        ("minecraft:quartz_block", 4159, 1),
        ("minecraft:quartz_bricks", 4160, 1),
        ("minecraft:quartz_bricks_from_quartz_block_stonecutting", 4161, 1),
        ("minecraft:quartz_from_blasting", 4162, 1),
        ("minecraft:quartz_pillar", 4163, 1),
        ("minecraft:quartz_pillar_from_quartz_block_stonecutting", 4164, 1),
        ("minecraft:quartz_slab", 4165, 1),
        ("minecraft:quartz_slab_from_quartz_block_stonecutting", 4166, 1),
        ("minecraft:quartz_stairs", 4167, 1),
        ("minecraft:quartz_stairs_from_quartz_block_stonecutting", 4168, 1),
        ("minecraft:rabbit_stew_from_brown_mushroom", 4169, 1),
        ("minecraft:rabbit_stew_from_red_mushroom", 4170, 1),
        ("minecraft:rabbit_stew_from_shelf_mushroom", 4171, 1),
        ("mekanism:radiation_shielding_unit", 4172, 1),
        ("mekanism:radioactive_waste_barrel", 4173, 1),
        ("minecraft:rail", 4174, 1),
        ("mekanism:rails", 4175, 1),
        ("minecraft:raiser_armor_trim_smithing_template", 4176, 1),
        ("minecraft:raiser_armor_trim_smithing_template_smithing_trim", 4177, 1),
        ("minecraft:raw_copper", 4178, 1),
        ("minecraft:raw_copper_block", 4179, 1),
        ("minecraft:raw_gold", 4180, 1),
        ("minecraft:raw_gold_block", 4181, 1),
        ("minecraft:raw_iron", 4182, 1),
        ("minecraft:raw_iron_block", 4183, 1),
        ("mekanism:reaction/coal_gasification/blocks_coals", 4184, 1),
        ("mekanism:reaction/coal_gasification/coals", 4185, 1),
        ("mekanism:reaction/coal_gasification/dusts_coals", 4186, 1),
        ("mekanism:reaction/substrate/ethene_oxygen", 4187, 1),
        ("mekanism:reaction/substrate/water_ethene", 4188, 1),
        ("mekanism:reaction/substrate/water_hydrogen", 4189, 1),
        ("mekanism:reaction/wood_gasification/dusts_wood", 4190, 1),
        ("mekanism:reaction/wood_gasification/logs", 4191, 1),
        ("mekanism:reaction/wood_gasification/planks", 4192, 1),
        ("mekanism:reaction/wood_gasification/rods_wooden", 4193, 1),
        ("mekanism:reaction/wood_gasification/wooden_slabs", 4194, 1),
        ("mekanism:reaction/wood_gasification/wooden_stairs", 4195, 1),
        ("mekanismgenerators:reactor/controller", 4196, 1),
        ("mekanismgenerators:reactor/frame", 4197, 1),
        ("mekanismgenerators:reactor/glass", 4198, 1),
        ("mekanismgenerators:reactor/logic_adapter", 4199, 1),
        ("mekanismgenerators:reactor/port", 4200, 1),
        ("minecraft:recovery_compass", 4201, 1),
        ("minecraft:red_banner", 4202, 1),
        ("minecraft:red_bed", 4203, 1),
        ("minecraft:red_bundle", 4204, 1),
        ("minecraft:red_candle", 4205, 1),
        ("minecraft:red_carpet", 4206, 1),
        ("minecraft:red_concrete_powder", 4207, 1),
        ("minecraft:red_concrete_slab", 4208, 1),
        ("minecraft:red_concrete_slab_from_red_concrete_stonecutting", 4209, 1),
        ("minecraft:red_concrete_stairs", 4210, 1),
        ("minecraft:red_concrete_stairs_from_red_concrete_stonecutting", 4211, 1),
        ("minecraft:red_cushion", 4212, 1),
        ("minecraft:red_dye_from_beetroot", 4213, 1),
        ("minecraft:red_dye_from_poppy", 4214, 1),
        ("minecraft:red_dye_from_rose_bush", 4215, 1),
        ("minecraft:red_dye_from_tulip", 4216, 1),
        ("minecraft:red_glazed_terracotta", 4217, 1),
        ("minecraft:red_harness", 4218, 1),
        ("minecraft:red_nether_brick_slab", 4219, 1),
        ("minecraft:red_nether_brick_slab_from_red_nether_bricks_stonecutting", 4220, 1),
        ("minecraft:red_nether_brick_stairs", 4221, 1),
        ("minecraft:red_nether_brick_stairs_from_red_nether_bricks_stonecutting", 4222, 1),
        ("minecraft:red_nether_brick_wall", 4223, 1),
        ("minecraft:red_nether_brick_wall_from_red_nether_bricks_stonecutting", 4224, 1),
        ("minecraft:red_nether_bricks", 4225, 1),
        ("minecraft:red_sandstone", 4226, 1),
        ("minecraft:red_sandstone_slab", 4227, 1),
        ("minecraft:red_sandstone_slab_from_red_sandstone_stonecutting", 4228, 1),
        ("minecraft:red_sandstone_stairs", 4229, 1),
        ("minecraft:red_sandstone_stairs_from_red_sandstone_stonecutting", 4230, 1),
        ("minecraft:red_sandstone_wall", 4231, 1),
        ("minecraft:red_sandstone_wall_from_red_sandstone_stonecutting", 4232, 1),
        ("minecraft:red_shulker_box", 4233, 1),
        ("minecraft:red_stained_glass", 4234, 1),
        ("minecraft:red_stained_glass_pane", 4235, 1),
        ("minecraft:red_stained_glass_pane_from_glass_pane", 4236, 1),
        ("minecraft:red_terracotta", 4237, 1),
        ("minecraft:red_wool_slab", 4238, 1),
        ("minecraft:red_wool_stairs", 4239, 1),
        ("minecraft:redstone", 4240, 1),
        ("minecraft:redstone_block", 4241, 1),
        ("minecraft:redstone_from_blasting_deepslate_redstone_ore", 4242, 1),
        ("minecraft:redstone_from_blasting_redstone_ore", 4243, 1),
        ("minecraft:redstone_from_smelting_deepslate_redstone_ore", 4244, 1),
        ("minecraft:redstone_from_smelting_redstone_ore", 4245, 1),
        ("minecraft:redstone_lamp", 4246, 1),
        ("minecraft:redstone_torch", 4247, 1),
        ("mekanismtools:refined_glowstone/armor/boots", 4248, 1),
        ("mekanismtools:refined_glowstone/armor/chestplate", 4249, 1),
        ("mekanismtools:refined_glowstone/armor/helmet", 4250, 1),
        ("mekanismtools:refined_glowstone/armor/horse", 4251, 1),
        ("mekanismtools:refined_glowstone/armor/leggings", 4252, 1),
        ("mekanismtools:refined_glowstone/armor/nautilus", 4253, 1),
        ("mekanismtools:refined_glowstone/nugget_from_blasting", 4254, 1),
        ("mekanismtools:refined_glowstone/nugget_from_smelting", 4255, 1),
        ("mekanismtools:refined_glowstone/shield", 4256, 1),
        ("mekanismtools:refined_glowstone/tools/axe", 4257, 1),
        ("mekanismtools:refined_glowstone/tools/hoe", 4258, 1),
        ("mekanismtools:refined_glowstone/tools/paxel", 4259, 1),
        ("mekanismtools:refined_glowstone/tools/pickaxe", 4260, 1),
        ("mekanismtools:refined_glowstone/tools/shovel", 4261, 1),
        ("mekanismtools:refined_glowstone/tools/spear", 4262, 1),
        ("mekanismtools:refined_glowstone/tools/sword", 4263, 1),
        ("mekanismtools:refined_obsidian/armor/boots", 4264, 1),
        ("mekanismtools:refined_obsidian/armor/chestplate", 4265, 1),
        ("mekanismtools:refined_obsidian/armor/helmet", 4266, 1),
        ("mekanismtools:refined_obsidian/armor/horse", 4267, 1),
        ("mekanismtools:refined_obsidian/armor/leggings", 4268, 1),
        ("mekanismtools:refined_obsidian/armor/nautilus", 4269, 1),
        ("mekanismtools:refined_obsidian/nugget_from_blasting", 4270, 1),
        ("mekanismtools:refined_obsidian/nugget_from_smelting", 4271, 1),
        ("mekanismtools:refined_obsidian/shield", 4272, 1),
        ("mekanismtools:refined_obsidian/tools/axe", 4273, 1),
        ("mekanismtools:refined_obsidian/tools/hoe", 4274, 1),
        ("mekanismtools:refined_obsidian/tools/paxel", 4275, 1),
        ("mekanismtools:refined_obsidian/tools/pickaxe", 4276, 1),
        ("mekanismtools:refined_obsidian/tools/shovel", 4277, 1),
        ("mekanismtools:refined_obsidian/tools/spear", 4278, 1),
        ("mekanismtools:refined_obsidian/tools/sword", 4279, 1),
        ("minecraft:repeater", 4280, 1),
        ("minecraft:resin_block", 4281, 1),
        ("minecraft:resin_brick", 4282, 1),
        ("minecraft:resin_brick_slab", 4283, 1),
        ("minecraft:resin_brick_slab_from_resin_bricks_stonecutting", 4284, 1),
        ("minecraft:resin_brick_stairs", 4285, 1),
        ("minecraft:resin_brick_stairs_from_resin_bricks_stonecutting", 4286, 1),
        ("minecraft:resin_brick_wall", 4287, 1),
        ("minecraft:resin_brick_wall_from_resin_bricks_stonecutting", 4288, 1),
        ("minecraft:resin_bricks", 4289, 1),
        ("minecraft:resin_clump", 4290, 1),
        ("mekanism:resistive_heater", 4291, 1),
        ("minecraft:respawn_anchor", 4292, 1),
        ("minecraft:rib_armor_trim_smithing_template", 4293, 1),
        ("minecraft:rib_armor_trim_smithing_template_smithing_trim", 4294, 1),
        ("mekanism:robit", 4295, 1),
        ("mekanism:rotary/brine", 4296, 2),
        ("mekanism:rotary/chlorine", 4298, 2),
        ("mekanismgenerators:rotary/deuterium", 4300, 2),
        ("mekanism:rotary/ethene", 4302, 2),
        ("mekanismgenerators:rotary/fusion_fuel", 4304, 2),
        ("mekanism:rotary/hydrofluoric_acid", 4306, 2),
        ("mekanism:rotary/hydrogen", 4308, 2),
        ("mekanism:rotary/hydrogen_chloride", 4310, 2),
        ("mekanism:rotary/lithium", 4312, 2),
        ("mekanism:rotary/oxygen", 4314, 2),
        ("mekanism:rotary/sodium", 4316, 2),
        ("mekanism:rotary/steam", 4318, 2),
        ("mekanism:rotary/sulfur_dioxide", 4320, 2),
        ("mekanism:rotary/sulfur_trioxide", 4322, 2),
        ("mekanism:rotary/sulfuric_acid", 4324, 2),
        ("mekanism:rotary/superheated_sodium", 4326, 2),
        ("mekanismgenerators:rotary/tritium", 4328, 2),
        ("mekanism:rotary/uranium_hexafluoride", 4330, 2),
        ("mekanism:rotary/uranium_oxide", 4332, 2),
        ("mekanism:rotary/water_vapor", 4334, 2),
        ("mekanism:rotary_condensentrator", 4336, 1),
        ("mekanismgenerators:rotational_complex", 4337, 1),
        ("minecraft:saddle", 4338, 1),
        ("minecraft:sandstone", 4339, 1),
        ("minecraft:sandstone_slab", 4340, 1),
        ("minecraft:sandstone_slab_from_sandstone_stonecutting", 4341, 1),
        ("minecraft:sandstone_stairs", 4342, 1),
        ("minecraft:sandstone_stairs_from_sandstone_stonecutting", 4343, 1),
        ("minecraft:sandstone_wall", 4344, 1),
        ("minecraft:sandstone_wall_from_sandstone_stonecutting", 4345, 1),
        ("mekanismgenerators:saturating_condenser", 4346, 1),
        ("mekanism:sawing/bamboo_block", 4347, 1),
        ("mekanism:sawing/barrel", 4348, 1),
        ("mekanism:sawing/bed/black", 4349, 1),
        ("mekanism:sawing/bed/blue", 4350, 1),
        ("mekanism:sawing/bed/brown", 4351, 1),
        ("mekanism:sawing/bed/cyan", 4352, 1),
        ("mekanism:sawing/bed/gray", 4353, 1),
        ("mekanism:sawing/bed/green", 4354, 1),
        ("mekanism:sawing/bed/light_blue", 4355, 1),
        ("mekanism:sawing/bed/light_gray", 4356, 1),
        ("mekanism:sawing/bed/lime", 4357, 1),
        ("mekanism:sawing/bed/magenta", 4358, 1),
        ("mekanism:sawing/bed/orange", 4359, 1),
        ("mekanism:sawing/bed/pink", 4360, 1),
        ("mekanism:sawing/bed/purple", 4361, 1),
        ("mekanism:sawing/bed/red", 4362, 1),
        ("mekanism:sawing/bed/straw", 4363, 1),
        ("mekanism:sawing/bed/white", 4364, 1),
        ("mekanism:sawing/bed/yellow", 4365, 1),
        ("mekanism:sawing/boat/acacia", 4366, 1),
        ("mekanism:sawing/boat/bamboo", 4367, 1),
        ("mekanism:sawing/boat/birch", 4368, 1),
        ("mekanism:sawing/boat/cherry", 4369, 1),
        ("mekanism:sawing/boat/dark_oak", 4370, 1),
        ("mekanism:sawing/boat/jungle", 4371, 1),
        ("mekanism:sawing/boat/mangrove", 4372, 1),
        ("mekanism:sawing/boat/oak", 4373, 1),
        ("mekanism:sawing/boat/pale_oak", 4374, 1),
        ("mekanism:sawing/boat/poplar", 4375, 1),
        ("mekanism:sawing/boat/spruce", 4376, 1),
        ("mekanism:sawing/bookshelf", 4377, 1),
        ("mekanism:sawing/button", 4378, 1),
        ("mekanism:sawing/chest", 4379, 1),
        ("mekanism:sawing/chest_boat/acacia", 4380, 1),
        ("mekanism:sawing/chest_boat/bamboo", 4381, 1),
        ("mekanism:sawing/chest_boat/birch", 4382, 1),
        ("mekanism:sawing/chest_boat/cherry", 4383, 1),
        ("mekanism:sawing/chest_boat/dark_oak", 4384, 1),
        ("mekanism:sawing/chest_boat/jungle", 4385, 1),
        ("mekanism:sawing/chest_boat/mangrove", 4386, 1),
        ("mekanism:sawing/chest_boat/oak", 4387, 1),
        ("mekanism:sawing/chest_boat/pale_oak", 4388, 1),
        ("mekanism:sawing/chest_boat/poplar", 4389, 1),
        ("mekanism:sawing/chest_boat/spruce", 4390, 1),
        ("mekanism:sawing/chiseled_bookshelf", 4391, 1),
        ("mekanism:sawing/composter", 4392, 1),
        ("mekanism:sawing/crafting_table", 4393, 1),
        ("mekanism:sawing/creaking_heart", 4394, 1),
        ("mekanism:sawing/door/acacia", 4395, 1),
        ("mekanism:sawing/door/bamboo", 4396, 1),
        ("mekanism:sawing/door/birch", 4397, 1),
        ("mekanism:sawing/door/cherry", 4398, 1),
        ("mekanism:sawing/door/crimson", 4399, 1),
        ("mekanism:sawing/door/dark_oak", 4400, 1),
        ("mekanism:sawing/door/jungle", 4401, 1),
        ("mekanism:sawing/door/mangrove", 4402, 1),
        ("mekanism:sawing/door/oak", 4403, 1),
        ("mekanism:sawing/door/pale_oak", 4404, 1),
        ("mekanism:sawing/door/poplar", 4405, 1),
        ("mekanism:sawing/door/spruce", 4406, 1),
        ("mekanism:sawing/door/warped", 4407, 1),
        ("mekanism:sawing/dried_ghast", 4408, 1),
        ("mekanism:sawing/fence_gate/acacia", 4409, 1),
        ("mekanism:sawing/fence_gate/bamboo", 4410, 1),
        ("mekanism:sawing/fence_gate/birch", 4411, 1),
        ("mekanism:sawing/fence_gate/cherry", 4412, 1),
        ("mekanism:sawing/fence_gate/crimson", 4413, 1),
        ("mekanism:sawing/fence_gate/dark_oak", 4414, 1),
        ("mekanism:sawing/fence_gate/jungle", 4415, 1),
        ("mekanism:sawing/fence_gate/mangrove", 4416, 1),
        ("mekanism:sawing/fence_gate/oak", 4417, 1),
        ("mekanism:sawing/fence_gate/pale_oak", 4418, 1),
        ("mekanism:sawing/fence_gate/poplar", 4419, 1),
        ("mekanism:sawing/fence_gate/spruce", 4420, 1),
        ("mekanism:sawing/fence_gate/warped", 4421, 1),
        ("mekanism:sawing/fences", 4422, 1),
        ("mekanism:sawing/hanging_sign/acacia", 4423, 1),
        ("mekanism:sawing/hanging_sign/bamboo", 4424, 1),
        ("mekanism:sawing/hanging_sign/birch", 4425, 1),
        ("mekanism:sawing/hanging_sign/cherry", 4426, 1),
        ("mekanism:sawing/hanging_sign/crimson", 4427, 1),
        ("mekanism:sawing/hanging_sign/dark_oak", 4428, 1),
        ("mekanism:sawing/hanging_sign/jungle", 4429, 1),
        ("mekanism:sawing/hanging_sign/mangrove", 4430, 1),
        ("mekanism:sawing/hanging_sign/oak", 4431, 1),
        ("mekanism:sawing/hanging_sign/pale_oak", 4432, 1),
        ("mekanism:sawing/hanging_sign/poplar", 4433, 1),
        ("mekanism:sawing/hanging_sign/spruce", 4434, 1),
        ("mekanism:sawing/hanging_sign/warped", 4435, 1),
        ("mekanism:sawing/item_frame", 4436, 1),
        ("mekanism:sawing/jukebox", 4437, 1),
        ("mekanism:sawing/ladder", 4438, 1),
        ("mekanism:sawing/lectern", 4439, 1),
        ("mekanism:sawing/log/acacia", 4440, 1),
        ("mekanism:sawing/log/birch", 4441, 1),
        ("mekanism:sawing/log/cherry", 4442, 1),
        ("mekanism:sawing/log/crimson", 4443, 1),
        ("mekanism:sawing/log/dark_oak", 4444, 1),
        ("mekanism:sawing/log/jungle", 4445, 1),
        ("mekanism:sawing/log/mangrove", 4446, 1),
        ("mekanism:sawing/log/oak", 4447, 1),
        ("mekanism:sawing/log/pale_oak", 4448, 1),
        ("mekanism:sawing/log/poplar", 4449, 1),
        ("mekanism:sawing/log/spruce", 4450, 1),
        ("mekanism:sawing/log/warped", 4451, 1),
        ("mekanism:sawing/melon", 4452, 1),
        ("mekanism:sawing/note_block", 4453, 1),
        ("mekanism:sawing/planks", 4454, 1),
        ("mekanism:sawing/pressure_plate/acacia", 4455, 1),
        ("mekanism:sawing/pressure_plate/bamboo", 4456, 1),
        ("mekanism:sawing/pressure_plate/birch", 4457, 1),
        ("mekanism:sawing/pressure_plate/cherry", 4458, 1),
        ("mekanism:sawing/pressure_plate/crimson", 4459, 1),
        ("mekanism:sawing/pressure_plate/dark_oak", 4460, 1),
        ("mekanism:sawing/pressure_plate/jungle", 4461, 1),
        ("mekanism:sawing/pressure_plate/mangrove", 4462, 1),
        ("mekanism:sawing/pressure_plate/oak", 4463, 1),
        ("mekanism:sawing/pressure_plate/pale_oak", 4464, 1),
        ("mekanism:sawing/pressure_plate/poplar", 4465, 1),
        ("mekanism:sawing/pressure_plate/spruce", 4466, 1),
        ("mekanism:sawing/pressure_plate/warped", 4467, 1),
        ("mekanism:sawing/pumpkin", 4468, 1),
        ("mekanism:sawing/redstone_torch", 4469, 1),
        ("mekanism:sawing/saddle", 4470, 1),
        ("mekanism:sawing/shelf/acacia", 4471, 1),
        ("mekanism:sawing/shelf/bamboo", 4472, 1),
        ("mekanism:sawing/shelf/birch", 4473, 1),
        ("mekanism:sawing/shelf/cherry", 4474, 1),
        ("mekanism:sawing/shelf/crimson", 4475, 1),
        ("mekanism:sawing/shelf/dark_oak", 4476, 1),
        ("mekanism:sawing/shelf/jungle", 4477, 1),
        ("mekanism:sawing/shelf/mangrove", 4478, 1),
        ("mekanism:sawing/shelf/oak", 4479, 1),
        ("mekanism:sawing/shelf/pale_oak", 4480, 1),
        ("mekanism:sawing/shelf/poplar", 4481, 1),
        ("mekanism:sawing/shelf/spruce", 4482, 1),
        ("mekanism:sawing/shelf/warped", 4483, 1),
        ("mekanism:sawing/sign", 4484, 1),
        ("mekanism:sawing/slabs", 4485, 1),
        ("mekanism:sawing/soul_torch", 4486, 1),
        ("mekanism:sawing/stairs", 4487, 1),
        ("mekanism:sawing/stick", 4488, 1),
        ("mekanism:sawing/torch", 4489, 1),
        ("mekanism:sawing/trapdoor/acacia", 4490, 1),
        ("mekanism:sawing/trapdoor/bamboo", 4491, 1),
        ("mekanism:sawing/trapdoor/birch", 4492, 1),
        ("mekanism:sawing/trapdoor/cherry", 4493, 1),
        ("mekanism:sawing/trapdoor/crimson", 4494, 1),
        ("mekanism:sawing/trapdoor/dark_oak", 4495, 1),
        ("mekanism:sawing/trapdoor/jungle", 4496, 1),
        ("mekanism:sawing/trapdoor/mangrove", 4497, 1),
        ("mekanism:sawing/trapdoor/oak", 4498, 1),
        ("mekanism:sawing/trapdoor/pale_oak", 4499, 1),
        ("mekanism:sawing/trapdoor/poplar", 4500, 1),
        ("mekanism:sawing/trapdoor/spruce", 4501, 1),
        ("mekanism:sawing/trapdoor/warped", 4502, 1),
        ("mekanism:sawing/trapped_chest", 4503, 1),
        ("minecraft:scaffolding", 4504, 1),
        ("mekanism:scuba_mask", 4505, 1),
        ("mekanism:scuba_tank", 4506, 1),
        ("minecraft:sea_lantern", 4507, 1),
        ("mekanism:security_desk", 4508, 1),
        ("mekanism:seismic_reader", 4509, 1),
        ("mekanism:seismic_vibrator", 4510, 1),
        ("minecraft:sentry_armor_trim_smithing_template", 4511, 1),
        ("minecraft:sentry_armor_trim_smithing_template_smithing_trim", 4512, 1),
        ("mekanism:separator/brine", 4513, 1),
        ("mekanismgenerators:separator/heavy_water", 4514, 1),
        ("mekanism:separator/water", 4515, 1),
        ("minecraft:shaper_armor_trim_smithing_template", 4516, 1),
        ("minecraft:shaper_armor_trim_smithing_template_smithing_trim", 4517, 1),
        ("mekanism:shearing_unit", 4518, 1),
        ("minecraft:shears", 4519, 1),
        ("minecraft:shield", 4520, 1),
        ("minecraft:shulker_box", 4521, 1),
        ("minecraft:silence_armor_trim_smithing_template", 4522, 1),
        ("minecraft:silence_armor_trim_smithing_template_smithing_trim", 4523, 1),
        ("mekanism:silk_touch_unit", 4524, 1),
        ("minecraft:skull_banner_pattern", 4525, 1),
        ("minecraft:slime_ball", 4526, 1),
        ("minecraft:slime_block", 4527, 1),
        ("minecraft:smithing_table", 4528, 1),
        ("minecraft:smoker", 4529, 1),
        ("minecraft:smooth_basalt", 4530, 1),
        ("minecraft:smooth_quartz", 4531, 1),
        ("minecraft:smooth_quartz_slab", 4532, 1),
        ("minecraft:smooth_quartz_slab_from_smooth_quartz_stonecutting", 4533, 1),
        ("minecraft:smooth_quartz_stairs", 4534, 1),
        ("minecraft:smooth_quartz_stairs_from_smooth_quartz_stonecutting", 4535, 1),
        ("minecraft:smooth_red_sandstone", 4536, 1),
        ("minecraft:smooth_red_sandstone_slab", 4537, 1),
        ("minecraft:smooth_red_sandstone_slab_from_smooth_red_sandstone_stonecutting", 4538, 1),
        ("minecraft:smooth_red_sandstone_stairs", 4539, 1),
        ("minecraft:smooth_red_sandstone_stairs_from_smooth_red_sandstone_stonecutting", 4540, 1),
        ("minecraft:smooth_sandstone", 4541, 1),
        ("minecraft:smooth_sandstone_slab", 4542, 1),
        ("minecraft:smooth_sandstone_slab_from_smooth_sandstone_stonecutting", 4543, 1),
        ("minecraft:smooth_sandstone_stairs", 4544, 1),
        ("minecraft:smooth_sandstone_stairs_from_smooth_sandstone_stonecutting", 4545, 1),
        ("minecraft:smooth_stone", 4546, 1),
        ("minecraft:smooth_stone_slab", 4547, 1),
        ("minecraft:smooth_stone_slab_from_smooth_stone_stonecutting", 4548, 1),
        ("minecraft:snout_armor_trim_smithing_template", 4549, 1),
        ("minecraft:snout_armor_trim_smithing_template_smithing_trim", 4550, 1),
        ("minecraft:snow", 4551, 1),
        ("minecraft:snow_block", 4552, 1),
        ("mekanism:solar_neutron_activator", 4553, 1),
        ("mekanismgenerators:solar_panel", 4554, 1),
        ("mekanismgenerators:solar_recharging_unit", 4555, 1),
        ("minecraft:soul_campfire", 4556, 1),
        ("minecraft:soul_lantern", 4557, 1),
        ("mekanism:soul_surfer_unit", 4558, 1),
        ("minecraft:soul_torch", 4559, 1),
        ("minecraft:spectral_arrow", 4560, 1),
        ("minecraft:spire_armor_trim_smithing_template", 4561, 1),
        ("minecraft:spire_armor_trim_smithing_template_smithing_trim", 4562, 1),
        ("minecraft:sponge", 4563, 1),
        ("minecraft:spruce_boat", 4564, 1),
        ("minecraft:spruce_button", 4565, 1),
        ("minecraft:spruce_chest_boat", 4566, 1),
        ("minecraft:spruce_door", 4567, 1),
        ("minecraft:spruce_fence", 4568, 1),
        ("minecraft:spruce_fence_gate", 4569, 1),
        ("minecraft:spruce_hanging_sign", 4570, 1),
        ("minecraft:spruce_planks", 4571, 1),
        ("minecraft:spruce_pressure_plate", 4572, 1),
        ("minecraft:spruce_shelf", 4573, 1),
        ("minecraft:spruce_sign", 4574, 1),
        ("minecraft:spruce_slab", 4575, 1),
        ("minecraft:spruce_stairs", 4576, 1),
        ("minecraft:spruce_trapdoor", 4577, 1),
        ("minecraft:spruce_wood", 4578, 1),
        ("mekanism:sps_casing", 4579, 1),
        ("mekanism:sps_port", 4580, 1),
        ("minecraft:spyglass", 4581, 1),
        ("mekanismtools:steel/armor/boots", 4582, 1),
        ("mekanismtools:steel/armor/chestplate", 4583, 1),
        ("mekanismtools:steel/armor/helmet", 4584, 1),
        ("mekanismtools:steel/armor/horse", 4585, 1),
        ("mekanismtools:steel/armor/leggings", 4586, 1),
        ("mekanismtools:steel/armor/nautilus", 4587, 1),
        ("mekanismtools:steel/nugget_from_blasting", 4588, 1),
        ("mekanismtools:steel/nugget_from_smelting", 4589, 1),
        ("mekanismtools:steel/shield", 4590, 1),
        ("mekanismtools:steel/tools/axe", 4591, 1),
        ("mekanismtools:steel/tools/hoe", 4592, 1),
        ("mekanismtools:steel/tools/paxel", 4593, 1),
        ("mekanismtools:steel/tools/pickaxe", 4594, 1),
        ("mekanismtools:steel/tools/shovel", 4595, 1),
        ("mekanismtools:steel/tools/spear", 4596, 1),
        ("mekanismtools:steel/tools/sword", 4597, 1),
        ("mekanism:steel_casing", 4598, 1),
        ("minecraft:stick", 4599, 1),
        ("minecraft:stick_from_bamboo_item", 4600, 1),
        ("minecraft:sticky_piston", 4601, 1),
        ("minecraft:stone", 4602, 1),
        ("minecraft:stone_axe", 4603, 1),
        ("minecraft:stone_brick_slab", 4604, 1),
        ("minecraft:stone_brick_slab_from_stone_bricks_stonecutting", 4605, 1),
        ("minecraft:stone_brick_slab_from_stone_stonecutting", 4606, 1),
        ("minecraft:stone_brick_stairs", 4607, 1),
        ("minecraft:stone_brick_stairs_from_stone_bricks_stonecutting", 4608, 1),
        ("minecraft:stone_brick_stairs_from_stone_stonecutting", 4609, 1),
        ("minecraft:stone_brick_wall", 4610, 1),
        ("minecraft:stone_brick_wall_from_stone_bricks_stonecutting", 4611, 1),
        ("minecraft:stone_brick_wall_from_stone_stonecutting", 4612, 1),
        ("minecraft:stone_bricks", 4613, 1),
        ("minecraft:stone_bricks_from_stone_stonecutting", 4614, 1),
        ("minecraft:stone_button", 4615, 1),
        ("minecraft:stone_hoe", 4616, 1),
        ("mekanismtools:stone_paxel", 4617, 1),
        ("minecraft:stone_pickaxe", 4618, 1),
        ("minecraft:stone_pressure_plate", 4619, 1),
        ("minecraft:stone_shovel", 4620, 1),
        ("minecraft:stone_slab", 4621, 1),
        ("minecraft:stone_slab_from_stone_stonecutting", 4622, 1),
        ("minecraft:stone_spear", 4623, 1),
        ("minecraft:stone_stairs", 4624, 1),
        ("minecraft:stone_stairs_from_stone_stonecutting", 4625, 1),
        ("minecraft:stone_sword", 4626, 1),
        ("minecraft:stonecutter", 4627, 1),
        ("mekanism:storage_blocks/bio_fuel", 4628, 1),
        ("mekanism:storage_blocks/bronze", 4629, 1),
        ("mekanism:storage_blocks/charcoal", 4630, 1),
        ("mekanism:storage_blocks/fluorite", 4631, 1),
        ("mekanism:storage_blocks/refined_glowstone", 4632, 1),
        ("mekanism:storage_blocks/refined_obsidian", 4633, 1),
        ("mekanism:storage_blocks/salt", 4634, 1),
        ("mekanism:storage_blocks/steel", 4635, 1),
        ("minecraft:straw_bed", 4636, 1),
        ("minecraft:stripped_acacia_wood", 4637, 1),
        ("minecraft:stripped_birch_wood", 4638, 1),
        ("minecraft:stripped_cherry_wood", 4639, 1),
        ("minecraft:stripped_crimson_hyphae", 4640, 1),
        ("minecraft:stripped_dark_oak_wood", 4641, 1),
        ("minecraft:stripped_jungle_wood", 4642, 1),
        ("minecraft:stripped_mangrove_wood", 4643, 1),
        ("minecraft:stripped_oak_wood", 4644, 1),
        ("minecraft:stripped_pale_oak_wood", 4645, 1),
        ("minecraft:stripped_poplar_wood", 4646, 1),
        ("minecraft:stripped_spruce_wood", 4647, 1),
        ("minecraft:stripped_warped_hyphae", 4648, 1),
        ("mekanism:structural_glass", 4649, 1),
        ("minecraft:sugar_from_honey_bottle", 4650, 1),
        ("minecraft:sugar_from_sugar_cane", 4651, 1),
        ("minecraft:sulfur_brick_slab", 4652, 1),
        ("minecraft:sulfur_brick_slab_from_polished_sulfur_stonecutting", 4653, 1),
        ("minecraft:sulfur_brick_slab_from_sulfur_bricks_stonecutting", 4654, 1),
        ("minecraft:sulfur_brick_slab_from_sulfur_stonecutting", 4655, 1),
        ("minecraft:sulfur_brick_stairs", 4656, 1),
        ("minecraft:sulfur_brick_stairs_from_polished_sulfur_stonecutting", 4657, 1),
        ("minecraft:sulfur_brick_stairs_from_sulfur_bricks_stonecutting", 4658, 1),
        ("minecraft:sulfur_brick_stairs_from_sulfur_stonecutting", 4659, 1),
        ("minecraft:sulfur_brick_wall", 4660, 1),
        ("minecraft:sulfur_brick_wall_from_polished_sulfur_stonecutting", 4661, 1),
        ("minecraft:sulfur_brick_wall_from_sulfur_bricks_stonecutting", 4662, 1),
        ("minecraft:sulfur_brick_wall_from_sulfur_stonecutting", 4663, 1),
        ("minecraft:sulfur_bricks", 4664, 1),
        ("minecraft:sulfur_bricks_from_polished_sulfur_stonecutting", 4665, 1),
        ("minecraft:sulfur_bricks_from_sulfur_stonecutting", 4666, 1),
        ("mekanism:sulfur_dye", 4667, 1),
        ("minecraft:sulfur_from_sulfur_spikes", 4668, 1),
        ("minecraft:sulfur_slab", 4669, 1),
        ("minecraft:sulfur_slab_from_sulfur_stonecutting", 4670, 1),
        ("minecraft:sulfur_stairs", 4671, 1),
        ("minecraft:sulfur_stairs_from_sulfur_stonecutting", 4672, 1),
        ("minecraft:sulfur_wall", 4673, 1),
        ("minecraft:sulfur_wall_from_sulfur_stonecutting", 4674, 1),
        ("mekanism:supercharged_coil", 4675, 1),
        ("mekanism:superheating_element", 4676, 1),
        ("minecraft:suspicious_stew_from_allium", 4677, 1),
        ("minecraft:suspicious_stew_from_azure_bluet", 4678, 1),
        ("minecraft:suspicious_stew_from_blue_orchid", 4679, 1),
        ("minecraft:suspicious_stew_from_closed_eyeblossom", 4680, 1),
        ("minecraft:suspicious_stew_from_cornflower", 4681, 1),
        ("minecraft:suspicious_stew_from_dandelion", 4682, 1),
        ("minecraft:suspicious_stew_from_golden_dandelion", 4683, 1),
        ("minecraft:suspicious_stew_from_lily_of_the_valley", 4684, 1),
        ("minecraft:suspicious_stew_from_open_eyeblossom", 4685, 1),
        ("minecraft:suspicious_stew_from_orange_tulip", 4686, 1),
        ("minecraft:suspicious_stew_from_oxeye_daisy", 4687, 1),
        ("minecraft:suspicious_stew_from_pink_tulip", 4688, 1),
        ("minecraft:suspicious_stew_from_poppy", 4689, 1),
        ("minecraft:suspicious_stew_from_red_tulip", 4690, 1),
        ("minecraft:suspicious_stew_from_torchflower", 4691, 1),
        ("minecraft:suspicious_stew_from_white_tulip", 4692, 1),
        ("minecraft:suspicious_stew_from_wither_rose", 4693, 1),
        ("minecraft:target", 4694, 1),
        ("mekanism:teleportation_core", 4695, 1),
        ("mekanism:teleportation_unit", 4696, 1),
        ("mekanism:teleporter", 4697, 1),
        ("mekanism:teleporter_frame", 4698, 1),
        ("minecraft:terracotta", 4699, 1),
        ("mekanism:thermal_evaporation/block", 4700, 1),
        ("mekanism:thermal_evaporation/controller", 4701, 1),
        ("mekanism:thermal_evaporation/valve", 4702, 1),
        ("minecraft:tide_armor_trim_smithing_template", 4703, 1),
        ("minecraft:tide_armor_trim_smithing_template_smithing_trim", 4704, 1),
        ("mekanism:tier_installer/advanced", 4705, 1),
        ("mekanism:tier_installer/basic", 4706, 1),
        ("mekanism:tier_installer/elite", 4707, 1),
        ("mekanism:tier_installer/ultimate", 4708, 1),
        ("minecraft:tinted_glass", 4709, 1),
        ("minecraft:tipped_arrow", 4710, 1),
        ("minecraft:tnt", 4711, 1),
        ("minecraft:tnt_minecart", 4712, 1),
        ("minecraft:torch", 4713, 1),
        ("mekanism:transmitter/diversion_transporter", 4714, 1),
        ("mekanism:transmitter/logistical_transporter/advanced", 4715, 1),
        ("mekanism:transmitter/logistical_transporter/basic", 4716, 1),
        ("mekanism:transmitter/logistical_transporter/elite", 4717, 1),
        ("mekanism:transmitter/logistical_transporter/ultimate", 4718, 1),
        ("mekanism:transmitter/mechanical_pipe/advanced", 4719, 1),
        ("mekanism:transmitter/mechanical_pipe/basic", 4720, 1),
        ("mekanism:transmitter/mechanical_pipe/elite", 4721, 1),
        ("mekanism:transmitter/mechanical_pipe/ultimate", 4722, 1),
        ("mekanism:transmitter/pressurized_tube/advanced", 4723, 1),
        ("mekanism:transmitter/pressurized_tube/basic", 4724, 1),
        ("mekanism:transmitter/pressurized_tube/elite", 4725, 1),
        ("mekanism:transmitter/pressurized_tube/ultimate", 4726, 1),
        ("mekanism:transmitter/restrictive_transporter", 4727, 1),
        ("mekanism:transmitter/thermodynamic_conductor/advanced", 4728, 1),
        ("mekanism:transmitter/thermodynamic_conductor/basic", 4729, 1),
        ("mekanism:transmitter/thermodynamic_conductor/elite", 4730, 1),
        ("mekanism:transmitter/thermodynamic_conductor/ultimate", 4731, 1),
        ("mekanism:transmitter/universal_cable/advanced", 4732, 1),
        ("mekanism:transmitter/universal_cable/basic", 4733, 1),
        ("mekanism:transmitter/universal_cable/elite", 4734, 1),
        ("mekanism:transmitter/universal_cable/ultimate", 4735, 1),
        ("minecraft:trapped_chest", 4736, 1),
        ("minecraft:tripwire_hook", 4737, 1),
        ("minecraft:tuff_brick_slab", 4738, 1),
        ("minecraft:tuff_brick_slab_from_polished_tuff_stonecutting", 4739, 1),
        ("minecraft:tuff_brick_slab_from_tuff_bricks_stonecutting", 4740, 1),
        ("minecraft:tuff_brick_slab_from_tuff_stonecutting", 4741, 1),
        ("minecraft:tuff_brick_stairs", 4742, 1),
        ("minecraft:tuff_brick_stairs_from_polished_tuff_stonecutting", 4743, 1),
        ("minecraft:tuff_brick_stairs_from_tuff_bricks_stonecutting", 4744, 1),
        ("minecraft:tuff_brick_stairs_from_tuff_stonecutting", 4745, 1),
        ("minecraft:tuff_brick_wall", 4746, 1),
        ("minecraft:tuff_brick_wall_from_polished_tuff_stonecutting", 4747, 1),
        ("minecraft:tuff_brick_wall_from_tuff_bricks_stonecutting", 4748, 1),
        ("minecraft:tuff_brick_wall_from_tuff_stonecutting", 4749, 1),
        ("minecraft:tuff_bricks", 4750, 1),
        ("minecraft:tuff_bricks_from_polished_tuff_stonecutting", 4751, 1),
        ("minecraft:tuff_bricks_from_tuff_stonecutting", 4752, 1),
        ("minecraft:tuff_slab", 4753, 1),
        ("minecraft:tuff_slab_from_tuff_stonecutting", 4754, 1),
        ("minecraft:tuff_stairs", 4755, 1),
        ("minecraft:tuff_stairs_from_tuff_stonecutting", 4756, 1),
        ("minecraft:tuff_wall", 4757, 1),
        ("minecraft:tuff_wall_from_tuff_stonecutting", 4758, 1),
        ("mekanismgenerators:turbine/blade", 4759, 1),
        ("mekanismgenerators:turbine/casing", 4760, 1),
        ("mekanismgenerators:turbine/rotor", 4761, 1),
        ("mekanismgenerators:turbine/valve", 4762, 1),
        ("mekanismgenerators:turbine/vent", 4763, 1),
        ("minecraft:turtle_helmet", 4764, 1),
        ("mekanism:upgrade/anchor", 4765, 1),
        ("mekanism:upgrade/chemical", 4766, 1),
        ("mekanism:upgrade/energy", 4767, 1),
        ("mekanism:upgrade/filter", 4768, 1),
        ("mekanism:upgrade/muffling", 4769, 1),
        ("mekanism:upgrade/speed", 4770, 1),
        ("mekanism:upgrade/stone_generator", 4771, 1),
        ("mekanism:upgrade/xp_containment", 4772, 1),
        ("mekanism:vein_mining_unit", 4773, 1),
        ("minecraft:vex_armor_trim_smithing_template", 4774, 1),
        ("minecraft:vex_armor_trim_smithing_template_smithing_trim", 4775, 1),
        ("mekanism:vision_enhancement_unit", 4776, 1),
        ("mekanismadditions:walkie_talkie", 4777, 1),
        ("minecraft:ward_armor_trim_smithing_template", 4778, 1),
        ("minecraft:ward_armor_trim_smithing_template_smithing_trim", 4779, 1),
        ("minecraft:warped_button", 4780, 1),
        ("minecraft:warped_door", 4781, 1),
        ("minecraft:warped_fence", 4782, 1),
        ("minecraft:warped_fence_gate", 4783, 1),
        ("minecraft:warped_fungus_on_a_stick", 4784, 1),
        ("minecraft:warped_hanging_sign", 4785, 1),
        ("minecraft:warped_hyphae", 4786, 1),
        ("minecraft:warped_planks", 4787, 1),
        ("minecraft:warped_pressure_plate", 4788, 1),
        ("minecraft:warped_shelf", 4789, 1),
        ("minecraft:warped_sign", 4790, 1),
        ("minecraft:warped_slab", 4791, 1),
        ("minecraft:warped_stairs", 4792, 1),
        ("minecraft:warped_trapdoor", 4793, 1),
        ("minecraft:waxed_chiseled_copper", 4794, 1),
        ("minecraft:waxed_chiseled_copper_from_honeycomb", 4795, 1),
        ("minecraft:waxed_chiseled_copper_from_waxed_copper_block_stonecutting", 4796, 1),
        ("minecraft:waxed_chiseled_copper_from_waxed_cut_copper_stonecutting", 4797, 1),
        ("minecraft:waxed_copper_bars_from_honeycomb", 4798, 1),
        ("minecraft:waxed_copper_block_from_honeycomb", 4799, 1),
        ("minecraft:waxed_copper_bulb", 4800, 1),
        ("minecraft:waxed_copper_bulb_from_honeycomb", 4801, 1),
        ("minecraft:waxed_copper_chain_from_honeycomb", 4802, 1),
        ("minecraft:waxed_copper_chest_from_honeycomb", 4803, 1),
        ("minecraft:waxed_copper_door_from_honeycomb", 4804, 1),
        ("minecraft:waxed_copper_golem_statue_from_honeycomb", 4805, 1),
        ("minecraft:waxed_copper_grate", 4806, 1),
        ("minecraft:waxed_copper_grate_from_honeycomb", 4807, 1),
        ("minecraft:waxed_copper_grate_from_waxed_copper_block_stonecutting", 4808, 1),
        ("minecraft:waxed_copper_lantern_from_honeycomb", 4809, 1),
        ("minecraft:waxed_copper_trapdoor_from_honeycomb", 4810, 1),
        ("minecraft:waxed_cut_copper", 4811, 1),
        ("minecraft:waxed_cut_copper_from_honeycomb", 4812, 1),
        ("minecraft:waxed_cut_copper_from_waxed_copper_block_stonecutting", 4813, 1),
        ("minecraft:waxed_cut_copper_slab", 4814, 1),
        ("minecraft:waxed_cut_copper_slab_from_honeycomb", 4815, 1),
        ("minecraft:waxed_cut_copper_slab_from_waxed_copper_block_stonecutting", 4816, 1),
        ("minecraft:waxed_cut_copper_slab_from_waxed_cut_copper_stonecutting", 4817, 1),
        ("minecraft:waxed_cut_copper_stairs", 4818, 1),
        ("minecraft:waxed_cut_copper_stairs_from_honeycomb", 4819, 1),
        ("minecraft:waxed_cut_copper_stairs_from_waxed_copper_block_stonecutting", 4820, 1),
        ("minecraft:waxed_cut_copper_stairs_from_waxed_cut_copper_stonecutting", 4821, 1),
        ("minecraft:waxed_exposed_chiseled_copper", 4822, 1),
        ("minecraft:waxed_exposed_chiseled_copper_from_honeycomb", 4823, 1),
        ("minecraft:waxed_exposed_chiseled_copper_from_waxed_exposed_copper_stonecutting", 4824, 1),
        ("minecraft:waxed_exposed_chiseled_copper_from_waxed_exposed_cut_copper_stonecutting", 4825, 1),
        ("minecraft:waxed_exposed_copper_bars_from_honeycomb", 4826, 1),
        ("minecraft:waxed_exposed_copper_bulb", 4827, 1),
        ("minecraft:waxed_exposed_copper_bulb_from_honeycomb", 4828, 1),
        ("minecraft:waxed_exposed_copper_chain_from_honeycomb", 4829, 1),
        ("minecraft:waxed_exposed_copper_chest_from_honeycomb", 4830, 1),
        ("minecraft:waxed_exposed_copper_door_from_honeycomb", 4831, 1),
        ("minecraft:waxed_exposed_copper_from_honeycomb", 4832, 1),
        ("minecraft:waxed_exposed_copper_golem_statue_from_honeycomb", 4833, 1),
        ("minecraft:waxed_exposed_copper_grate", 4834, 1),
        ("minecraft:waxed_exposed_copper_grate_from_honeycomb", 4835, 1),
        ("minecraft:waxed_exposed_copper_grate_from_waxed_exposed_copper_stonecutting", 4836, 1),
        ("minecraft:waxed_exposed_copper_lantern_from_honeycomb", 4837, 1),
        ("minecraft:waxed_exposed_copper_trapdoor_from_honeycomb", 4838, 1),
        ("minecraft:waxed_exposed_cut_copper", 4839, 1),
        ("minecraft:waxed_exposed_cut_copper_from_honeycomb", 4840, 1),
        ("minecraft:waxed_exposed_cut_copper_from_waxed_exposed_copper_stonecutting", 4841, 1),
        ("minecraft:waxed_exposed_cut_copper_slab", 4842, 1),
        ("minecraft:waxed_exposed_cut_copper_slab_from_honeycomb", 4843, 1),
        ("minecraft:waxed_exposed_cut_copper_slab_from_waxed_exposed_copper_stonecutting", 4844, 1),
        ("minecraft:waxed_exposed_cut_copper_slab_from_waxed_exposed_cut_copper_stonecutting", 4845, 1),
        ("minecraft:waxed_exposed_cut_copper_stairs", 4846, 1),
        ("minecraft:waxed_exposed_cut_copper_stairs_from_honeycomb", 4847, 1),
        ("minecraft:waxed_exposed_cut_copper_stairs_from_waxed_exposed_copper_stonecutting", 4848, 1),
        ("minecraft:waxed_exposed_cut_copper_stairs_from_waxed_exposed_cut_copper_stonecutting", 4849, 1),
        ("minecraft:waxed_exposed_lightning_rod_from_honeycomb", 4850, 1),
        ("minecraft:waxed_lightning_rod_from_honeycomb", 4851, 1),
        ("minecraft:waxed_oxidized_chiseled_copper", 4852, 1),
        ("minecraft:waxed_oxidized_chiseled_copper_from_honeycomb", 4853, 1),
        ("minecraft:waxed_oxidized_chiseled_copper_from_waxed_oxidized_copper_stonecutting", 4854, 1),
        ("minecraft:waxed_oxidized_chiseled_copper_from_waxed_oxidized_cut_copper_stonecutting", 4855, 1),
        ("minecraft:waxed_oxidized_copper_bars_from_honeycomb", 4856, 1),
        ("minecraft:waxed_oxidized_copper_bulb", 4857, 1),
        ("minecraft:waxed_oxidized_copper_bulb_from_honeycomb", 4858, 1),
        ("minecraft:waxed_oxidized_copper_chain_from_honeycomb", 4859, 1),
        ("minecraft:waxed_oxidized_copper_chest_from_honeycomb", 4860, 1),
        ("minecraft:waxed_oxidized_copper_door_from_honeycomb", 4861, 1),
        ("minecraft:waxed_oxidized_copper_from_honeycomb", 4862, 1),
        ("minecraft:waxed_oxidized_copper_golem_statue_from_honeycomb", 4863, 1),
        ("minecraft:waxed_oxidized_copper_grate", 4864, 1),
        ("minecraft:waxed_oxidized_copper_grate_from_honeycomb", 4865, 1),
        ("minecraft:waxed_oxidized_copper_grate_from_waxed_oxidized_copper_stonecutting", 4866, 1),
        ("minecraft:waxed_oxidized_copper_lantern_from_honeycomb", 4867, 1),
        ("minecraft:waxed_oxidized_copper_trapdoor_from_honeycomb", 4868, 1),
        ("minecraft:waxed_oxidized_cut_copper", 4869, 1),
        ("minecraft:waxed_oxidized_cut_copper_from_honeycomb", 4870, 1),
        ("minecraft:waxed_oxidized_cut_copper_from_waxed_oxidized_copper_stonecutting", 4871, 1),
        ("minecraft:waxed_oxidized_cut_copper_slab", 4872, 1),
        ("minecraft:waxed_oxidized_cut_copper_slab_from_honeycomb", 4873, 1),
        ("minecraft:waxed_oxidized_cut_copper_slab_from_waxed_oxidized_copper_stonecutting", 4874, 1),
        ("minecraft:waxed_oxidized_cut_copper_slab_from_waxed_oxidized_cut_copper_stonecutting", 4875, 1),
        ("minecraft:waxed_oxidized_cut_copper_stairs", 4876, 1),
        ("minecraft:waxed_oxidized_cut_copper_stairs_from_honeycomb", 4877, 1),
        ("minecraft:waxed_oxidized_cut_copper_stairs_from_waxed_oxidized_copper_stonecutting", 4878, 1),
        ("minecraft:waxed_oxidized_cut_copper_stairs_from_waxed_oxidized_cut_copper_stonecutting", 4879, 1),
        ("minecraft:waxed_oxidized_lightning_rod_from_honeycomb", 4880, 1),
        ("minecraft:waxed_weathered_chiseled_copper", 4881, 1),
        ("minecraft:waxed_weathered_chiseled_copper_from_honeycomb", 4882, 1),
        ("minecraft:waxed_weathered_chiseled_copper_from_waxed_weathered_copper_stonecutting", 4883, 1),
        ("minecraft:waxed_weathered_chiseled_copper_from_waxed_weathered_cut_copper_stonecutting", 4884, 1),
        ("minecraft:waxed_weathered_copper_bars_from_honeycomb", 4885, 1),
        ("minecraft:waxed_weathered_copper_bulb", 4886, 1),
        ("minecraft:waxed_weathered_copper_bulb_from_honeycomb", 4887, 1),
        ("minecraft:waxed_weathered_copper_chain_from_honeycomb", 4888, 1),
        ("minecraft:waxed_weathered_copper_chest_from_honeycomb", 4889, 1),
        ("minecraft:waxed_weathered_copper_door_from_honeycomb", 4890, 1),
        ("minecraft:waxed_weathered_copper_from_honeycomb", 4891, 1),
        ("minecraft:waxed_weathered_copper_golem_statue_from_honeycomb", 4892, 1),
        ("minecraft:waxed_weathered_copper_grate", 4893, 1),
        ("minecraft:waxed_weathered_copper_grate_from_honeycomb", 4894, 1),
        ("minecraft:waxed_weathered_copper_grate_from_waxed_weathered_copper_stonecutting", 4895, 1),
        ("minecraft:waxed_weathered_copper_lantern_from_honeycomb", 4896, 1),
        ("minecraft:waxed_weathered_copper_trapdoor_from_honeycomb", 4897, 1),
        ("minecraft:waxed_weathered_cut_copper", 4898, 1),
        ("minecraft:waxed_weathered_cut_copper_from_honeycomb", 4899, 1),
        ("minecraft:waxed_weathered_cut_copper_from_waxed_weathered_copper_stonecutting", 4900, 1),
        ("minecraft:waxed_weathered_cut_copper_slab", 4901, 1),
        ("minecraft:waxed_weathered_cut_copper_slab_from_honeycomb", 4902, 1),
        ("minecraft:waxed_weathered_cut_copper_slab_from_waxed_weathered_copper_stonecutting", 4903, 1),
        ("minecraft:waxed_weathered_cut_copper_slab_from_waxed_weathered_cut_copper_stonecutting", 4904, 1),
        ("minecraft:waxed_weathered_cut_copper_stairs", 4905, 1),
        ("minecraft:waxed_weathered_cut_copper_stairs_from_honeycomb", 4906, 1),
        ("minecraft:waxed_weathered_cut_copper_stairs_from_waxed_weathered_copper_stonecutting", 4907, 1),
        ("minecraft:waxed_weathered_cut_copper_stairs_from_waxed_weathered_cut_copper_stonecutting", 4908, 1),
        ("minecraft:waxed_weathered_lightning_rod_from_honeycomb", 4909, 1),
        ("minecraft:wayfinder_armor_trim_smithing_template", 4910, 1),
        ("minecraft:wayfinder_armor_trim_smithing_template_smithing_trim", 4911, 1),
        ("minecraft:weathered_chiseled_copper", 4912, 1),
        ("minecraft:weathered_chiseled_copper_from_weathered_copper_stonecutting", 4913, 1),
        ("minecraft:weathered_chiseled_copper_from_weathered_cut_copper_stonecutting", 4914, 1),
        ("minecraft:weathered_copper_bulb", 4915, 1),
        ("minecraft:weathered_copper_grate", 4916, 1),
        ("minecraft:weathered_copper_grate_from_weathered_copper_stonecutting", 4917, 1),
        ("minecraft:weathered_cut_copper", 4918, 1),
        ("minecraft:weathered_cut_copper_from_weathered_copper_stonecutting", 4919, 1),
        ("minecraft:weathered_cut_copper_slab", 4920, 1),
        ("minecraft:weathered_cut_copper_slab_from_weathered_copper_stonecutting", 4921, 1),
        ("minecraft:weathered_cut_copper_slab_from_weathered_cut_copper_stonecutting", 4922, 1),
        ("minecraft:weathered_cut_copper_stairs", 4923, 1),
        ("minecraft:weathered_cut_copper_stairs_from_weathered_copper_stonecutting", 4924, 1),
        ("minecraft:weathered_cut_copper_stairs_from_weathered_cut_copper_stonecutting", 4925, 1),
        ("minecraft:wheat", 4926, 1),
        ("minecraft:white_banner", 4927, 1),
        ("minecraft:white_bed", 4928, 1),
        ("minecraft:white_bundle", 4929, 1),
        ("minecraft:white_candle", 4930, 1),
        ("minecraft:white_carpet", 4931, 1),
        ("minecraft:white_concrete_powder", 4932, 1),
        ("minecraft:white_concrete_slab", 4933, 1),
        ("minecraft:white_concrete_slab_from_white_concrete_stonecutting", 4934, 1),
        ("minecraft:white_concrete_stairs", 4935, 1),
        ("minecraft:white_concrete_stairs_from_white_concrete_stonecutting", 4936, 1),
        ("minecraft:white_cushion", 4937, 1),
        ("minecraft:white_dye", 4938, 1),
        ("minecraft:white_dye_from_lily_of_the_valley", 4939, 1),
        ("minecraft:white_glazed_terracotta", 4940, 1),
        ("minecraft:white_harness", 4941, 1),
        ("minecraft:white_shulker_box", 4942, 1),
        ("minecraft:white_stained_glass", 4943, 1),
        ("minecraft:white_stained_glass_pane", 4944, 1),
        ("minecraft:white_stained_glass_pane_from_glass_pane", 4945, 1),
        ("minecraft:white_terracotta", 4946, 1),
        ("minecraft:white_wool_from_string", 4947, 1),
        ("minecraft:white_wool_slab", 4948, 1),
        ("minecraft:white_wool_stairs", 4949, 1),
        ("minecraft:wild_armor_trim_smithing_template", 4950, 1),
        ("minecraft:wild_armor_trim_smithing_template_smithing_trim", 4951, 1),
        ("minecraft:wind_charge", 4952, 1),
        ("minecraft:wolf_armor", 4953, 1),
        ("minecraft:wolf_armor_dyed", 4954, 1),
        ("mekanismtools:wood_paxel", 4955, 1),
        ("minecraft:wooden_axe", 4956, 1),
        ("minecraft:wooden_hoe", 4957, 1),
        ("minecraft:wooden_pickaxe", 4958, 1),
        ("minecraft:wooden_shovel", 4959, 1),
        ("minecraft:wooden_spear", 4960, 1),
        ("minecraft:wooden_sword", 4961, 1),
        ("minecraft:writable_book", 4962, 1),
        ("minecraft:yellow_banner", 4963, 1),
        ("minecraft:yellow_bed", 4964, 1),
        ("minecraft:yellow_bundle", 4965, 1),
        ("minecraft:yellow_candle", 4966, 1),
        ("minecraft:yellow_carpet", 4967, 1),
        ("minecraft:yellow_concrete_powder", 4968, 1),
        ("minecraft:yellow_concrete_slab", 4969, 1),
        ("minecraft:yellow_concrete_slab_from_yellow_concrete_stonecutting", 4970, 1),
        ("minecraft:yellow_concrete_stairs", 4971, 1),
        ("minecraft:yellow_concrete_stairs_from_yellow_concrete_stonecutting", 4972, 1),
        ("minecraft:yellow_cushion", 4973, 1),
        ("minecraft:yellow_dye_from_dandelion", 4974, 1),
        ("minecraft:yellow_dye_from_golden_dandelion", 4975, 1),
        ("minecraft:yellow_dye_from_sunflower", 4976, 1),
        ("minecraft:yellow_dye_from_wildflowers", 4977, 1),
        ("minecraft:yellow_glazed_terracotta", 4978, 1),
        ("minecraft:yellow_harness", 4979, 1),
        ("minecraft:yellow_shulker_box", 4980, 1),
        ("minecraft:yellow_stained_glass", 4981, 1),
        ("minecraft:yellow_stained_glass_pane", 4982, 1),
        ("minecraft:yellow_stained_glass_pane_from_glass_pane", 4983, 1),
        ("minecraft:yellow_terracotta", 4984, 1),
        ("minecraft:yellow_wool_slab", 4985, 1),
        ("minecraft:yellow_wool_stairs", 4986, 1),
    ];
}
// recipe-book-rows:fim

/// Os livros de receitas das quatro máquinas (`MekanismRecipeBookTypes`, um por tipo de receita, compartilhado entre as máquinas do mesmo tipo):
/// o tipo, as abas depois da bússola (`getPrimaryIcon` e `getSecondaryIcon` de cada `*BookCategory`; as do forno são as do vanilla) e as linhas.
pub mod recipe_books {
    use super::recipe_book_rows::{DISPLAYS, GHOSTS, REQUIREMENTS, ROWS};
    use machinery_basis_core::recipe_book_view::{BookData, BookTab};

    const fn tab(
        category: &'static str,
        primary: &'static str,
        secondary: Option<&'static str>,
    ) -> BookTab {
        BookTab {
            category,
            primary,
            secondary,
        }
    }

    pub static ENRICHING: BookData = BookData {
        book_type: "enriching",
        settings_type: "MEKANISM_ENRICHING",
        filter_name: "gui.mekanism.recipe_book.toggle_recipes.enriching",
        tabs: &[
            tab(
                "mekanism:enriching_building",
                "minecraft:polished_blackstone_bricks",
                None,
            ),
            tab(
                "mekanism:enriching_efficiency",
                "mekanism:enriched_diamond",
                Some("mekanism:dust_iron"),
            ),
            tab(
                "mekanism:enriching_decorative",
                "minecraft:magenta_dye",
                None,
            ),
            tab(
                "mekanism:enriching_misc",
                "minecraft:flint",
                Some("mekanism:hdpe_sheet"),
            ),
        ],
        rows: ROWS,
        requirements: REQUIREMENTS,
        ghosts: GHOSTS,
        displays: DISPLAYS,
    };

    pub static CRUSHING: BookData = BookData {
        book_type: "crushing",
        settings_type: "MEKANISM_CRUSHING",
        filter_name: "gui.mekanism.recipe_book.toggle_recipes.crushing",
        tabs: &[
            tab(
                "mekanism:crushing_building",
                "minecraft:cracked_stone_bricks",
                None,
            ),
            tab(
                "mekanism:crushing_efficiency",
                "mekanism:dirty_dust_copper",
                None,
            ),
            tab(
                "mekanism:crushing_misc",
                "minecraft:prismarine_shard",
                Some("mekanism:dust_emerald"),
            ),
        ],
        rows: ROWS,
        requirements: REQUIREMENTS,
        ghosts: GHOSTS,
        displays: DISPLAYS,
    };

    pub static METALLURGIC_INFUSING: BookData = BookData {
        book_type: "metallurgic_infusing",
        settings_type: "MEKANISM_METALLURGIC_INFUSING",
        filter_name: "gui.mekanism.recipe_book.toggle_recipes.metallurgic_infusing",
        tabs: &[
            tab(
                "mekanism:metallurgic_infusing_building",
                "minecraft:mycelium",
                None,
            ),
            tab(
                "mekanism:metallurgic_infusing_misc",
                "mekanism:enriched_iron",
                Some("mekanism:basic_control_circuit"),
            ),
        ],
        rows: ROWS,
        requirements: REQUIREMENTS,
        ghosts: GHOSTS,
        displays: DISPLAYS,
    };

    /// O Energized Smelter mostra os displays do `minecraft:smelting` (o tipo `mekanism:smelting` não tem display nenhum).
    pub static SMELTING: BookData = BookData {
        book_type: "smelting",
        settings_type: "MEKANISM_SMELTING",
        filter_name: "gui.recipebook.toggleRecipes.smeltable",
        tabs: &[
            tab("minecraft:furnace_food", "minecraft:porkchop", None),
            tab("minecraft:furnace_blocks", "minecraft:stone", None),
            tab(
                "minecraft:furnace_misc",
                "minecraft:lava_bucket",
                Some("minecraft:emerald"),
            ),
        ],
        rows: ROWS,
        requirements: REQUIREMENTS,
        ghosts: GHOSTS,
        displays: DISPLAYS,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_enter_by_the_front_the_left_and_the_top() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up] {
            assert_eq!(
                item_slots(true, front, face).inputs,
                [INPUT_SLOT],
                "{face:?}"
            );
        }
    }

    #[test]
    fn the_base_feeds_the_infusion_slot() {
        assert_eq!(
            item_slots(true, Face::East, Face::Down).inputs,
            [INFUSION_SLOT]
        );
    }

    #[test]
    fn the_right_gives_only_the_output_and_the_back_only_the_energy_slot() {
        for front in [Face::North, Face::South, Face::East, Face::West] {
            let right = item_slots(true, front, front.right_of());
            assert!(right.inputs.is_empty());
            assert_eq!(right.outputs, [OUTPUT_SLOT]);
            assert_eq!(
                item_slots(true, front, front.opposite()).inputs,
                [ENERGY_SLOT]
            );
            assert_eq!(
                item_slots(true, front, front.opposite()).outputs,
                [ENERGY_SLOT]
            );
            assert!(!item_slots(true, front, front).inputs.is_empty());
        }
    }

    #[test]
    fn an_electric_machine_also_takes_by_the_base() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up, Face::Down] {
            let slots = item_slots(false, front, face);
            assert_eq!(slots.inputs, [ELECTRIC_INPUT_SLOT], "{face:?}");
        }
        assert!(item_slots(false, front, front.right_of()).inputs.is_empty());
        assert_eq!(
            item_slots(false, front, front.opposite()).inputs,
            [ELECTRIC_ENERGY_SLOT]
        );
    }

    #[test]
    fn a_machine_placed_without_the_item_reaches_nothing() {
        for infuser in [false, true] {
            let spec = if infuser {
                infuser_side_config()
            } else {
                electric_side_config()
            };
            let mut config = TileComponentConfig::new(&spec.types);
            (spec.setup)(&mut config);
            for face in Face::ALL {
                assert_eq!(
                    config.item_face(Face::North, face, &[0, 1, 2, 3]),
                    FaceSlots::default()
                );
            }
        }
    }

    #[test]
    fn the_screen_item_config_is_the_one_the_component_builds() {
        for (infuser, screen) in [(false, &ELECTRIC_ITEM_CONFIG), (true, &INFUSER_ITEM_CONFIG)] {
            let config = item_placed_config(infuser);
            let items = config.config(TransmissionType::Item).unwrap();
            for (data_type, slots) in screen.entries {
                let info = items.slot_info_of(*data_type).unwrap();
                assert_eq!(&info.containers[..], *slots, "{data_type:?}");
            }
            assert_eq!(items.supported_data_types().len() - 1, screen.entries.len());
        }
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
