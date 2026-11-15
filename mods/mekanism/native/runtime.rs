// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::active_fx::{FxParticle, Particle, REDSTONE, face_position};
use machinery_basis_core::cached_recipe::ItemToItemCache;
use machinery_basis_core::chemical::{ChemicalResource, ConversionCache, can_fill_or_convert_with};
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

/// A aba de xp das telas das máquinas.
pub fn xp_tab() -> machinery_basis_core::side_tab::XpTab {
    machinery_basis_core::side_tab::XpTab::new(SCREEN_IMAGE_WIDTH, XP_TAB_Y, true)
}

/// Os elementos que a tela de uma máquina tem fora da imagem de 176 por 166 e que `GuiMekanism.findElementOver` acha (os que a tela do port
/// desenha; o livro de receitas do original ainda não existe aqui):
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
    format!("description.mekanism.{name}")
}

/// `ItemRegistryObject.getNameColor` (`:91-104`): o item de um bloco com tier (`Attribute.getBaseTier`) leva a cor do tier no nome (`BaseTier.getTextColor`: Basic `0x5FFFB8`, Advanced
/// `0xFF806A`, Elite `0x4BF8FF`, Ultimate `0xF787FF`, Creative `0x585858`). Aqui os blocos com tier que têm `DETAILS`: as 36 fábricas e os 5 cubos de energia.
pub fn tier_block_color(item: &str) -> Option<u32> {
    let name = item.strip_prefix("mekanism:")?;
    if !(name.ends_with("_factory") || name.ends_with("_energy_cube")) {
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
}

impl Chat {
    fn new(translate: &str, with: Vec<Chat>, color: Option<u32>) -> Chat {
        Chat {
            translate: translate.to_owned(),
            with,
            color,
        }
    }

    /// `translate` sem argumentos, na cor `color` (o `ordinal` de `EnumColor`).
    fn plain(translate: &str, color: Option<u8>) -> Chat {
        Chat::new(translate, Vec::new(), color.map(enum_color_rgb))
    }

    /// O JSON de componente, como o Java o grava.
    pub fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({ "translate": self.translate });
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
