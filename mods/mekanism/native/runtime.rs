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
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::resource::Resource;
use machinery_basis_core::settings::{self, ConfigValue, MachineSettings};
use machinery_basis_core::side_config::{
    AttachedSideConfig, LightConfigInfo, RelativeSide, TileComponentConfig, TransmissionType,
};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
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
    ],
};

/// O arquivo de cliente que o Horizon escreve quando ele não existe: a seção `sounds` e as posições das janelas que o port abre (`window.upgrade`), no
/// texto do NightConfig. O original escreve as seis seções e as onze janelas; o port, só o que lê (decisão 12, revista pela 14).
pub fn client_default_text() -> String {
    use machinery_basis_core::window::WindowPositions;
    let windows = WindowPositions::defaults_for(&[UPGRADE_WINDOW]).to_toml(&WINDOW_TYPES);
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

/// `GeneralConfig`: `blockDeactivationDelay` (`:126-127`, `defineInRange(.., 3 * TICKS_PER_SECOND, 0, TICKS_PER_MINUTE)`) e `maxUpgradeMultiplier` (`:133-134`,
/// `defineInRange(.., 10, 1, Integer.MAX_VALUE)`), as duas de fora de qualquer seção. O resto do arquivo entra com a fase que o lê.
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

/// `GeneralConfig.blockDeactivationDelay` e `maxUpgradeMultiplier` de um arquivo lido.
pub fn machine_settings(loaded: &ClientLoaded) -> MachineSettings {
    let defaults = MachineSettings::DEFAULT;
    MachineSettings {
        block_deactivation_delay: loaded
            .int(&GENERAL_CONFIG, "", "blockDeactivationDelay")
            .unwrap_or(defaults.block_deactivation_delay),
        max_upgrade_multiplier: loaded
            .int(&GENERAL_CONFIG, "", "maxUpgradeMultiplier")
            .unwrap_or(defaults.max_upgrade_multiplier),
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

/// O que os arquivos de servidor dizem às mecânicas portadas do Mekanism, de uma vez.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub usage: Usage,
    pub cables: [i64; 4],
    pub cubes: [i64; 5],
    pub cells: [i64; 4],
    pub gear: Gear,
}

impl ServerConfig {
    pub const DEFAULT: Self = Self {
        usage: Usage::DEFAULT,
        cables: DEFAULT_CABLE_CAPACITIES,
        cubes: DEFAULT_CUBE_CAPACITIES,
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
    let usage = 0.005 * (capacity as f64);
    let clamped = if usage >= f64::from(i32::MAX) {
        i32::MAX
    } else {
        usage as i32
    };
    clamped.max(1)
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
            infuser_side_config(),
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

/// Para um hopper, de uma máquina posta por um item.
pub fn item_sides(infuser: bool, front: Face) -> Sides {
    Sides::of_slots(&|face| item_slots(infuser, front, face))
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

/// A cor do nome de um item (`ItemRegistryObject.setNameColor`, `ItemDeferredRegister`: o componente `item_name` com a cor do tier ou do `EnumColor`): os quatro
/// circuitos, os quatro discos do QIO e os quatro instaladores de tier (`BaseTier.getTextColor`, `BaseTier.java:22-25`; o instalador pela cor do tier de destino,
/// `registerInstaller`, `MekanismItems.java:299-302`), os três pellets (`EnumColor.PURPLE`, `GRAY` e `INDIGO`, `EnumColor.java`) e o Configurator (`EnumColor.AQUA`, `MekanismItems.java:93-94`).
/// O Horizon lê o `item_name` só do dado de cada pilha, não do componente padrão do item, então a cor vai pelo hook do tooltip (`glue::tooltip`).
pub fn name_color(item: &str) -> Option<u32> {
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

/// `BlockSmallTransmitter` e `BlockLargeTransmitter`: o centro (`box(5..11)` e `box(4..12)`) em coordenadas do bloco, pelo id; `None` se o bloco não é um transmissor.
pub fn transmitter_center(block: &str) -> Option<[f64; 6]> {
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
        Some([
            5.0 / 16.0,
            5.0 / 16.0,
            5.0 / 16.0,
            11.0 / 16.0,
            11.0 / 16.0,
            11.0 / 16.0,
        ])
    } else if large {
        Some([
            4.0 / 16.0,
            4.0 / 16.0,
            4.0 / 16.0,
            12.0 / 16.0,
            12.0 / 16.0,
            12.0 / 16.0,
        ])
    } else {
        None
    }
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

/// `MultipartUtils.collisionRayTrace` sobre `getCollisionBoxes`: a caixa mais perto que o raio atravessa (a primeira em empate), e o centro se nenhuma
/// (`BlockTransmitter.getShape`: "if we failed to figure it out somehow, just fall back to the center").
pub fn select_box(
    boxes: &[[f64; 6]],
    origin: [f64; 3],
    end: [f64; 3],
    center: [f64; 6],
) -> [f64; 6] {
    let mut best: Option<(f64, [f64; 6])> = None;
    for bounds in boxes {
        if let Some(distance) = clip_box(origin, end, bounds)
            && best.is_none_or(|(nearest, _)| distance < nearest)
        {
            best = Some((distance, *bounds));
        }
    }
    best.map_or(center, |(_, bounds)| bounds)
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
            assert!(item_sides(true, front).accepts_input(front));
        }
    }

    #[test]
    fn a_hopper_below_the_infuser_gets_the_infusion_slot_and_no_other_face_can_be_below() {
        let sides = item_sides(true, Face::North);
        assert!(sides.gives_output(Face::Down));
        assert_eq!(
            item_slots(true, Face::North, Face::Down).outputs,
            [INFUSION_SLOT]
        );
        // O back (`ENERGY`) também entrega, mas um hopper só puxa do bloco de cima: ele não está embaixo de nenhuma face lateral.
        assert!(sides.gives_output(Face::South));
        assert!(!sides.gives_output(Face::North));
        assert!(!sides.gives_output(Face::East));
        assert!(!sides.gives_output(Face::Up));
    }

    #[test]
    fn a_hopper_below_an_electric_machine_gets_nothing() {
        let sides = item_sides(false, Face::North);
        assert!(!sides.gives_output(Face::Down));
        assert!(sides.gives_output(Face::South));
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
