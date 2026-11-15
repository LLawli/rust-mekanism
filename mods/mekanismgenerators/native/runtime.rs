// Regras do Heat Generator, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o
// módulo crate::game::mekanismgenerators_rules. Segue TileEntityHeatGenerator.
use machinery_basis_core::active_fx::{FxParticle, Particle, face_position};
use machinery_basis_core::client_config::{
    Entry, Loaded, Lookup, Section, Setting, Spec, Value as ConfigScalar,
};
use machinery_basis_core::container_sync::{
    Kind, Mirror, StackValue, Tracked, TrackedSource, Value,
};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::machine_sync::upgrade_map;
use machinery_basis_core::settings::ConfigValue;
use machinery_basis_core::slot_view::{ContainerSlotType, SlotOverlay, SlotSpec};
use machinery_basis_core::temperature::{self, TemperatureUnit};
use machinery_basis_core::tile::TileBase;
use machinery_basis_core::tile_trackers::{
    EnergyTrackers, TileTrackers, position_of, source, upgrade_window_trackers,
};
use machinery_basis_core::units::{energy_fraction, stored_mb};
use std::sync::LazyLock;

/// Os itens do Mekanism Generators com o componente `MekanismDataComponents.DETAILS` (o rastro `upgrade_history_details.jsonl`, conferido por `details_items_trace`).
pub const DETAILS_ITEMS: [&str; 8] = [
    "mekanismgenerators:heat_generator",
    "mekanismgenerators:solar_generator",
    "mekanismgenerators:gas_burning_generator",
    "mekanismgenerators:bio_generator",
    "mekanismgenerators:advanced_solar_generator",
    "mekanismgenerators:wind_generator",
    "mekanismgenerators:fission_reactor_logic_adapter",
    "mekanismgenerators:fusion_reactor_logic_adapter",
];

/// Se o item tem o componente `DETAILS` (a dica de detalhes e o esconde-detalhes do tooltip).
pub fn has_details(item: &str) -> bool {
    DETAILS_ITEMS.contains(&item)
}

pub const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";

/// `GeneratorsBlockTypes.java:69`: `SMOKE` e `FLAME` em `(rand * 0,6 - 0,3, rand * 6/16, -0,52)` relativo ao centro do bloco, na face de trás (o
/// `facing` gira a posição). O Heat Generator não é um `Machine`, então não tem as partículas de `Machine.java:23`.
pub static HEAT_GENERATOR_FX: [FxParticle; 2] = [
    FxParticle {
        particle: Particle::Smoke,
        position: |next| face_position(-0.52, next),
    },
    FxParticle {
        particle: Particle::Flame,
        position: |next| face_position(-0.52, next),
    },
];
pub const MENU: &str = "mekanismgenerators:heat_generator";
pub const LAVA: &str = "minecraft:lava";

pub const HEAT_CAPACITY: f64 = 10.0;
pub const INVERSE_CONDUCTION: f64 = 5.0;
pub const INVERSE_INSULATION: f64 = 100.0;
/// A face de baixo usa a isolação padrão: é por ela que o calor sairia para um vizinho.
pub const INSULATION_BY_FACE: [f64; 6] = [0.0, 100.0, 100.0, 100.0, 100.0, 100.0];
const THERMAL_EFFICIENCY: f64 = 0.5;

/// Valores padrão de `GeneratorsConfig` e `GeneratorsStorageConfig` (`heatGeneration`, `heatGenerationLava`, `heatGenerationNether`,
/// `heatGenerationFluidRate`, `tankCapacity` e `heatGenerator`): o que vale sem arquivo, e o que os testes dos rastros usam.
pub const HEAT_GENERATION: f64 = 100.0;
pub const HEAT_GENERATION_LAVA: f64 = 7.0;
pub const HEAT_GENERATION_NETHER: f64 = 10.0;
pub const FLUID_RATE: u64 = 100;
pub const TANK_CAPACITY: u64 = 1_000;
pub const ENERGY_CAPACITY: i32 = 240;
/// As seis faces mais a lava dentro do bloco, o Nether e a queima.
pub const MAX_PRODUCTION: i32 = 7 * 7 + 10 + 100;
pub const BUCKET_VOLUME: u64 = 1_000;
/// `blockDeactivationDelay`, o padrão de `GeneralConfig` do Mekanism: quantos ticks o bloco aceso espera para apagar depois que o
/// gerador para (`TileEntityMekanism.setActive`). O Heat Generator não troca o `delaySupplier`. O jogo lê o valor do arquivo
/// (`machinery_basis_core::settings`); este é o dos testes.
pub const BLOCK_DEACTIVATION_DELAY: i32 = 60;

// ---------------------------------------------------------------------------------------------------------------------------
// Os arquivos de configuração do Generators (`MekanismGeneratorsConfig`): `generators` e `generator-storage`, os dois `Type.SYNCED`, registrados por
// `MekanismConfigHelper.registerConfig` na pasta do Mekanism (`Mekanism/<arquivo>.toml`, `Mekanism.MOD_NAME`). Só as chaves do Heat Generator.

pub const GENERATORS_CONFIG_FILE: &str = "Mekanism/generators.toml";
pub const GENERATOR_STORAGE_CONFIG_FILE: &str = "Mekanism/generator-storage.toml";

/// `heatGenerationFluidRate` é `define(.., 100, value -> value instanceof Integer i && i > 0 && i <= heatTankCapacity.getOrDefault())`
/// (`GeneratorsConfig.java:130`): sem faixa, e o que passa da capacidade do tanque que o validador vê volta ao padrão.
fn fluid_rate_fits_the_tank(rate: i32, lookup: Lookup<'_>) -> bool {
    let Some(ConfigScalar::Long(tank)) = lookup("heat_generator", "tankCapacity") else {
        return false;
    };
    rate > 0 && i64::from(rate) <= tank
}

/// `GeneratorsConfig` (`:115-130`), a seção `heat_generator`, com o texto de `GeneratorsConfigTranslations`. `heatGenerationLava` vai até
/// `Integer.MAX_VALUE / (EnumUtils.DIRECTIONS.length + 1)`, para as sete faces não estourarem o `int`.
pub const GENERATORS_CONFIG: Spec = Spec {
    sections: &[Section {
        name: "heat_generator",
        comment: "Settings for configuring Heat Generators",
        entries: &[
            Entry {
                key: "heatGeneration",
                comment: "Amount of energy the Heat Generator produces per tick. heatGeneration + heatGenerationLava * lavaSides + heatGenerationNether. Note: lavaSides is how many sides are adjacent to lava, this includes the block itself if it is lava logged allowing for a max of 7 \"sides\".",
                setting: Setting::Int {
                    default: 100,
                    min: 0,
                    max: i32::MAX,
                },
            },
            Entry {
                key: "heatGenerationLava",
                comment: "Multiplier of effectiveness of Lava that is adjacent to the Heat Generator.",
                setting: Setting::Int {
                    default: 7,
                    min: 0,
                    max: i32::MAX / 7,
                },
            },
            Entry {
                key: "heatGenerationNether",
                comment: "Add this amount to the energy produced by a heat generator if it is in an 'ultrawarm' dimension, in vanilla this is just the Nether.",
                setting: Setting::Int {
                    default: 10,
                    min: 0,
                    max: i32::MAX,
                },
            },
            Entry {
                key: "tankCapacity",
                comment: "The capacity in mB of the fluid tank in the Heat Generator.",
                setting: Setting::Long {
                    default: 1_000,
                    min: 1,
                    max: i64::MAX,
                },
            },
            Entry {
                key: "heatGenerationFluidRate",
                comment: "The amount of lava in mB that gets consumed to transfer heatGeneration Joules to the Heat Generator.",
                setting: Setting::IntWhere {
                    default: 100,
                    valid: fluid_rate_fits_the_tank,
                },
            },
        ],
    }],
};

/// `GeneratorsStorageConfig` (`:25-26`): `heatGenerator`, `CachedLongValue.definedMin(.., 12 * TICKS_PER_SECOND, 1)`, com o texto de
/// `TranslationPreset.ENERGY_STORAGE`. É a capacidade do buffer de energia (`withEnergyStorage`).
pub const GENERATOR_STORAGE_CONFIG: Spec = Spec {
    sections: &[Section {
        name: "",
        comment: "",
        entries: &[Entry {
            key: "heatGenerator",
            comment: "Base energy capacity of: Heat Generator",
            setting: Setting::Long {
                default: 240,
                min: 1,
                max: i64::MAX,
            },
        }],
    }],
};

/// O que o Heat Generator lê da configuração, de uma vez. O original lê cada valor no uso (`generators.heatGeneration.get()` no tick, o tanque com um
/// fornecedor): o glue refaz isto do valor de agora a cada tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeatConfig {
    pub heat_generation: i32,
    pub heat_generation_lava: i32,
    pub heat_generation_nether: i32,
    pub tank_capacity: i64,
    pub fluid_rate: i32,
    pub energy_capacity: i64,
}

impl HeatConfig {
    pub const DEFAULT: Self = Self {
        heat_generation: 100,
        heat_generation_lava: 7,
        heat_generation_nether: 10,
        tank_capacity: 1_000,
        fluid_rate: 100,
        energy_capacity: 240,
    };

    /// Os valores dos dois arquivos lidos.
    pub fn of(generators: &Loaded, storage: &Loaded) -> Self {
        let d = Self::DEFAULT;
        let int = |key: &str, default: i32| {
            generators
                .int(&GENERATORS_CONFIG, "heat_generator", key)
                .unwrap_or(default)
        };
        Self {
            heat_generation: int("heatGeneration", d.heat_generation),
            heat_generation_lava: int("heatGenerationLava", d.heat_generation_lava),
            heat_generation_nether: int("heatGenerationNether", d.heat_generation_nether),
            tank_capacity: generators
                .long(&GENERATORS_CONFIG, "heat_generator", "tankCapacity")
                .unwrap_or(d.tank_capacity),
            fluid_rate: int("heatGenerationFluidRate", d.fluid_rate),
            energy_capacity: storage
                .long(&GENERATOR_STORAGE_CONFIG, "", "heatGenerator")
                .unwrap_or(d.energy_capacity),
        }
    }

    /// `MAX_PRODUCTION` (`TileEntityHeatGenerator.java:41-45`): `heatGenerationLava * 7`, mais `heatGenerationNether` e `heatGeneration`, cada soma truncada
    /// em `Integer.MAX_VALUE` (`MathUtils.addClamped`).
    pub fn max_production(&self) -> i32 {
        self.heat_generation_lava
            .saturating_mul(7)
            .saturating_add(self.heat_generation_nether)
            .saturating_add(self.heat_generation)
    }

    /// A capacidade do tanque em mB (o `defineInRange` dela começa em 1).
    pub fn tank(&self) -> u64 {
        self.tank_capacity.max(1) as u64
    }

    /// A capacidade do buffer de energia como o Horizon a guarda (`i32`): o que passa disso fica em `i32::MAX`.
    pub fn energy_capacity_int(&self) -> i32 {
        i32::try_from(self.energy_capacity).unwrap_or(i32::MAX)
    }
}

/// Os slots de `TileEntityHeatGenerator.getInitialInventory`, na ordem dos índices: o do combustível (`FluidInventorySlot`, `EXTRA` no
/// contêiner) e o de energia (`EnergyInventorySlot`, com o raio por cima). A posição é a do item. O gerador não tem configuração de
/// lados de itens, então a moldura vem do tipo no contêiner.
pub const SLOTS_SPEC: [SlotSpec; 2] = [
    SlotSpec {
        item: [17.0, 35.0],
        container: ContainerSlotType::Extra,
        overlay: None,
    },
    SlotSpec {
        item: [143.0, 35.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Power),
    },
];

/// O `gui.scaling` dos sprites do Mekanism que a tela usa, como os `.png.mcmeta` do original dizem (um teste confere contra os
/// arquivos): o painel `base`, a moldura `gauge/normal` e o mosaico de `bar/vertical_power`. As molduras e o painel são os mesmos
/// que as telas das máquinas usam.
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
/// As chaves de idioma do Mekanism que os tooltips usam (`MekanismLang.GENERIC_FRACTION`, `GENERIC_STORED_MB`, `EMPTY` e
/// `ENERGY_FORGE`) e o nome da lava (`FluidType.getDescription`).
pub const FRACTION_LANG: &str = "generic.mekanism.fraction";
pub const STORED_MB_LANG: &str = "generic.mekanism.stored.mb";
pub const EMPTY_LANG: &str = "gui.mekanism.empty";
pub const ENERGY_UNIT_LANG: &str = "energy.mekanism.forge.short";
/// O componente que guarda a energia do item (`MekanismDataComponents.ATTACHED_ENERGY`, registrado pelo Mekanism) e a linha `STORED_ENERGY` do tooltip.
pub const ENERGY_COMPONENT: &str = "mekanism:energy";
pub const STORED_ENERGY_LANG: &str = "tooltip.mekanism.stored_energy";
pub const INFINITE_LANG: &str = "gui.mekanism.infinite";
/// `EnumColor.BRIGHT_GREEN` e `EnumColor.GRAY`.
pub const STORED_ENERGY_COLOR: u32 = 0x75_FF_89;
pub const STORED_ENERGY_ARG_COLOR: u32 = 0xCF_CF_CF;
pub const LAVA_NAME_LANG: &str = "block.minecraft.lava";

/// O tooltip de `GuiVerticalPowerBar`: a energia sobre a capacidade, em FE.
pub fn energy_tooltip(energy: u64, capacity: u64, translate: &dyn Fn(&str) -> String) -> String {
    energy_fraction(
        energy,
        capacity,
        &translate(ENERGY_UNIT_LANG),
        &translate(FRACTION_LANG),
    )
}

/// `GuiEnergyTab`: o elemento de textura em (-26, 137), 26 por 26, sem clique, e o sprite que o Mekanism guarda.
pub const ENERGY_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 137.0, 26.0, 26.0],
    };
pub const ENERGY_TAB_SPRITE: &str = "mekanism:gui/sprites/tab/energy_info";
/// `GeneratorsLang.PRODUCING_AMOUNT` e `MekanismLang.UNIT`.
pub const PRODUCING_LANG: &str = "gui.mekanismgenerators.producing.amount";
pub const UNIT_LANG: &str = "gui.mekanism.unit";

/// O tooltip de `GuiEnergyTab` do Heat Generator (`tile.getProductionRate()`): o que produz por tick e a unidade.
pub fn energy_tab_tooltip(production: u64, translate: &dyn Fn(&str) -> String) -> Vec<String> {
    machinery_basis_core::units::energy_info_lines(
        &[(&translate(PRODUCING_LANG), production)],
        &translate(ENERGY_UNIT_LANG),
        &translate(UNIT_LANG),
    )
}

/// `GuiHeatTab`: o elemento de textura em (-26, 109), 26 por 26. O sprite muda com a unidade de temperatura da configuração local.
pub const HEAT_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 109.0, 26.0, 26.0],
    };
/// `GeneratorsContainerTypes.java:29`: o Heat Generator registra `.armorSideBar(-20, 11, 0)` (o `x`, o `y` e o `offhandOffset` de
/// `ContainerBuilder.armorSideBar`), e `MekanismContainer.addArmorSlots` põe os quatro slots de armadura e o da mão a partir dali.
pub const ARMOR_BAR_X: i32 = -20;
pub const ARMOR_BAR_Y: i32 = 11;
pub const ARMOR_BAR_OFFHAND_OFFSET: i32 = 0;
/// `GuiHeatGenerator.java:35`: `GuiSideHolder.create(this, -26, 6, 98, true, true, SpecialColors.TAB_ARMOR_SLOTS)`, o suporte de 26 por 98 atrás
/// dos cinco slots, à esquerda (`holder_left`). A cor é o pixel 21 de `colormap/gui_objects.png` (o 22º `register()` de `SpecialColors`), branco.
pub const ARMOR_HOLDER: machinery_basis_core::gui::Rect = [-26.0, 6.0, 26.0, 98.0];
pub const ARMOR_HOLDER_SPRITE: &str = "mekanism:gui/sprites/holder_left";
pub const ARMOR_HOLDER_TINT: u32 = 0xFF_FF_FF_FF;

/// O que a tela do gerador declara à `machinery_basis` do tipo de menu: a barra de armadura. A aba de upgrades é da lib.
pub fn armor_bar_declaration() -> machinery_basis_core::armor_bar::Declaration {
    machinery_basis_core::armor_bar::Declaration {
        x: ARMOR_BAR_X,
        y: ARMOR_BAR_Y,
        offhand_offset: ARMOR_BAR_OFFHAND_OFFSET,
    }
}

/// Os elementos da tela que ficam fora da imagem de 176 por 166 e recebem o mouse (`GuiMekanism.hasClickedOutside`): a aba de calor e a de
/// energia, ambas sempre ativas e visíveis.
pub fn screen_elements() -> Vec<machinery_basis_core::screen_elements::Element> {
    [HEAT_TAB.rect, ENERGY_TAB.rect]
        .map(machinery_basis_core::screen_elements::Element::always)
        .to_vec()
}

/// `GuiHeatTab.getResource`: `Mekanism.rl("tab/heat_info_" + type.getTabName())`, no atlas de sprites da tela.
pub fn heat_tab_sprite(unit: TemperatureUnit) -> String {
    format!("mekanism:gui/sprites/tab/heat_info_{}", unit.tab_name())
}
/// O arquivo da configuração local do Mekanism (`MekanismConfig.local`, `Mekanism/local.toml`), na pasta do original (`config/Mekanism/`, como o `client.toml`).
pub const LOCAL_CONFIG_NAMESPACE: &str = "Mekanism";
/// `MekanismLang.TEMPERATURE`, `TRANSFERRED_RATE` e `DISSIPATED_RATE`: os modelos das três linhas da aba de calor.
pub const TEMPERATURE_LANG: &str = "temperature.mekanism.short";
pub const TRANSFERRED_RATE_LANG: &str = "gui.mekanism.transferred_rate";
pub const DISSIPATED_RATE_LANG: &str = "gui.mekanism.dissipated";
/// `TemperatureUnit.getTranslationKey`: o nome curto que `MekanismLang.UNIT` escreve (`K`, `C`, `R`, `F`, `STP`).
pub fn unit_lang(unit: TemperatureUnit) -> &'static str {
    match unit {
        TemperatureUnit::Kelvin => "temperature.mekanism.kelvin.short",
        TemperatureUnit::Celsius => "temperature.mekanism.celsius.short",
        TemperatureUnit::Rankine => "temperature.mekanism.rankine.short",
        TemperatureUnit::Fahrenheit => "temperature.mekanism.fahrenheit.short",
        TemperatureUnit::Ambient => "temperature.mekanism.ambient.short",
    }
}

/// O que `GuiHeatGenerator` põe no `infoHandler` de `GuiHeatTab`: a temperatura do tile (`getTemperature`, o calor sobre a capacidade) e
/// as duas perdas do último tick, em kelvin, escritas na unidade da configuração local (`MekanismUtils.getTemperatureDisplay` com
/// `TemperatureUnit.KELVIN`): a temperatura desloca o zero da escala (`shift`), as perdas são diferenças e não.
pub fn heat_tab_tooltip(
    heat: f64,
    transfer_loss: f64,
    environment_loss: f64,
    local: TemperatureUnit,
    translate: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let show = |value: f64, shift: bool| {
        temperature::local_display(value, TemperatureUnit::Kelvin, local, shift)
    };
    let (temperature_template, transferred_template, dissipated_template) = (
        translate(TEMPERATURE_LANG),
        translate(TRANSFERRED_RATE_LANG),
        translate(DISSIPATED_RATE_LANG),
    );
    temperature::heat_info_lines(
        &[
            (&temperature_template, show(heat / HEAT_CAPACITY, true)),
            (&transferred_template, show(transfer_loss, false)),
            (&dissipated_template, show(environment_loss, false)),
        ],
        &translate(unit_lang(local)),
        &translate(UNIT_LANG),
    )
}

/// O clique na aba (`GuiHeatTab.onClick`): sobre o retângulo dela, e fora de uma janela por cima (`blocked`, a janela de upgrades
/// aberta sobre o ponto), o botão esquerdo avança a unidade da configuração local e o direito volta. Devolve a unidade nova quando a
/// aba tomou o clique, e `None` quando ele segue para o resto da tela.
pub fn heat_tab_click(
    unit: TemperatureUnit,
    at: [f32; 2],
    button: i32,
    blocked: bool,
) -> Option<TemperatureUnit> {
    if !HEAT_TAB.is_mouse_over(at, blocked) {
        return None;
    }
    temperature::rotate(unit, button)
}

/// O tooltip de `GuiFluidGauge` (`GuiTankGauge.getContentsTooltips`): `Empty`, ou `Lava: <mB> mB`.
pub fn lava_tooltip(amount: u64, translate: &dyn Fn(&str) -> String) -> String {
    if amount == 0 {
        return translate(EMPTY_LANG);
    }
    stored_mb(
        &translate(STORED_MB_LANG),
        &translate(LAVA_NAME_LANG),
        amount,
    )
}

/// A lava no medidor: o sprite do jogo (`MekanismRenderer.getFluidTexture(lava, STILL)`), do tamanho de um quadro.
pub const LAVA_SPRITE: &str = "block/lava_still";
pub const LAVA_FRAME: [f32; 2] = [16.0, 16.0];

/// O que o gerador sabe do mundo neste tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surroundings {
    /// Lados com lava, fonte ou corrente. A lava dentro do bloco conta como mais um.
    pub lava_faces: u32,
    pub in_nether: bool,
    /// O ar do bloco, que o `simulate` do gerador usa para a eficiência de Carnot.
    pub ambient: f64,
    /// O ar de cada face, na ordem das direções do jogo (baixo, cima, norte, sul, oeste, leste): a perda para o ambiente usa o do bloco
    /// vizinho (`getAmbientTemperature(side)`).
    pub face_ambient: [f64; 6],
}

/// O que aconteceu no tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Produced {
    pub active: bool,
    pub lava_used: u64,
    pub energy: i32,
    /// `lastTransferLoss`, o `adjacentTransfer` de `simulate`: a temperatura que passou para o vizinho de baixo (um condutor termodinâmico, `condutor-jogo`). Zero sem vizinho com a
    /// capability `HEAT` voltada para cima.
    pub transfer_loss: f64,
    /// `lastEnvironmentLoss`, o `environmentTransfer` de `simulate`: a temperatura que o ar levou, só das faces mais quentes que ele.
    pub environment_loss: f64,
}

pub fn new_capacitor(ambient: f64) -> Capacitor {
    Capacitor::new(
        HEAT_CAPACITY,
        INVERSE_CONDUCTION,
        INVERSE_INSULATION,
        ambient,
    )
}

/// `getBoost` (`TileEntityHeatGenerator.java:155-182`): `heatGenerationLava` por lado com lava (nada, se ele for zero) e, no Nether, `heatGenerationNether`
/// somado com `addClamped`.
fn boost(config: &HeatConfig, around: &Surroundings) -> f64 {
    let lava = if config.heat_generation_lava == 0 {
        0
    } else {
        config
            .heat_generation_lava
            .saturating_mul(i32::try_from(around.lava_faces).unwrap_or(i32::MAX))
    };
    let boost = if around.in_nether {
        lava.saturating_add(config.heat_generation_nether)
    } else {
        lava
    };
    f64::from(boost)
}

/// Um tick. `lava` é o que há no tanque, e a energia sai sem o limite do buffer. `can_function` é o `canFunction()` do tile
/// (`TileEntityHeatGenerator.java:140`): o controle de redstone e o `redstone` que o último aviso dos vizinhos deixou; sem ele a lava não
/// queima, mas o calor guardado segue gerando.
pub fn tick(
    config: &HeatConfig,
    heat: &mut Capacitor,
    around: &Surroundings,
    lava: u64,
    energy_full: bool,
    can_function: bool,
) -> Produced {
    tick_with(
        config,
        heat,
        around,
        lava,
        energy_full,
        can_function,
        &mut |_| 0.0,
    )
}

/// [`tick`] com o vizinho de baixo: `adjacent` é o `simulateAdjacent` do lado `DOWN` (`TileEntityHeatGenerator.getAdjacent` só entrega por baixo), chamado com o capacitor depois da perda
/// do `simulate` do gerador e antes da perda para o ar (`ITileHeatHandler.simulate`: primeiro `simulateAdjacent`, depois `simulateEnvironment`), e devolve a temperatura que passou.
#[allow(clippy::too_many_arguments)]
pub fn tick_with(
    config: &HeatConfig,
    heat: &mut Capacitor,
    around: &Surroundings,
    lava: u64,
    energy_full: bool,
    can_function: bool,
    adjacent: &mut dyn FnMut(&mut Capacitor) -> f64,
) -> Produced {
    heat.handle(boost(config, around));
    let fluid_rate = u64::try_from(config.fluid_rate).unwrap_or(0);
    let active = can_function && lava >= fluid_rate && !energy_full;
    if active {
        heat.handle(f64::from(config.heat_generation));
    }
    let temperature = heat.temperature();
    let carnot = 1.0 - around.ambient.min(temperature) / around.ambient.max(temperature);
    let heat_lost = THERMAL_EFFICIENCY * (temperature - around.ambient);
    heat.handle(-heat_lost);
    let energy = ((heat_lost.abs() * carnot) as i32).min(config.max_production());
    let transfer_loss = adjacent(heat);
    let environment_loss = heat.lose_to_faces(around.face_ambient, INSULATION_BY_FACE);
    Produced {
        active,
        lava_used: if active { fluid_rate } else { 0 },
        energy,
        transfer_loss,
        environment_loss,
    }
}

/// O que o Heat Generator guarda e muda a cada tick: o calor, a lava do tanque e a energia do buffer, mais o que todo tile do Mekanism
/// guarda (`currentActive`, `updateDelay` e o estado `lit` do bloco).
#[derive(Clone, Debug)]
pub struct State {
    pub heat: Capacitor,
    pub lava: u64,
    pub energy: i32,
    pub tile: TileBase,
    /// A configuração de agora (`MekanismGeneratorsConfig`).
    pub config: HeatConfig,
}

impl State {
    /// Um gerador novo com a configuração padrão (os testes dos rastros); o jogo troca `config` pelo valor lido do arquivo antes de cada tick.
    pub fn new(ambient: f64) -> Self {
        Self {
            heat: new_capacitor(ambient),
            lava: 0,
            energy: 0,
            tile: TileBase::new(ConfigValue::Live(
                machinery_basis_core::settings::block_deactivation_delay,
            )),
            config: HeatConfig::DEFAULT,
        }
    }
}

/// Um tick do tile (`tickServer` e `onUpdateServer`, depois do slot de combustível): conta o atraso do bloco aceso, queima a lava,
/// guarda a energia no buffer, liga ou desliga o tile (`setActive`) e devolve o que entrou no buffer (`producingEnergy`: a diferença
/// da energia antes e depois, que o limite do buffer corta).
pub fn step(state: &mut State, around: &Surroundings) -> Produced {
    step_with(state, around, &mut |_| 0.0)
}

/// [`step`] com o vizinho de baixo (veja [`tick_with`]).
pub fn step_with(
    state: &mut State,
    around: &Surroundings,
    adjacent: &mut dyn FnMut(&mut Capacitor) -> f64,
) -> Produced {
    state.tile.tick_delay();
    let capacity = state.config.energy_capacity_int();
    let full = state.energy >= capacity;
    let can_function = state.tile.is_redstone_activated();
    let mut produced = tick_with(
        &state.config,
        &mut state.heat,
        around,
        state.lava,
        full,
        can_function,
        adjacent,
    );
    state.lava -= produced.lava_used;
    let filled = state.energy.saturating_add(produced.energy).min(capacity);
    produced.energy = filled - state.energy;
    state.energy = filled;
    state.tile.set_active(produced.active);
    produced
}

/// `TileEntityGenerator.onUpdateServer`, que o `TileEntityHeatGenerator.onUpdateServer` chama antes de tudo (`super.onUpdateServer`): com `canFunction()`, `EnergyUtils.emit(outputCaches,
/// energyContainer, null)` entrega a energia do buffer ao único vizinho que o gerador tem como destino, o da frente (`ENERGY_SIDES` é só `RelativeSide.FRONT`), e tira do buffer o que
/// o vizinho aceitou. Devolve o que saiu.
pub fn emit_energy(
    state: &mut State,
    neighbours: &dyn machinery_basis_core::energy_cube::EnergyNeighbours,
    front: machinery_basis_core::sides::Face,
) -> i32 {
    if !state.tile.is_redstone_activated() {
        return 0;
    }
    machinery_basis_core::energy_cube::emit_stored(neighbours, &mut state.energy, &[front])
}

/// O `onUpdateServer` inteiro do ponto de vista da energia: a entrega ([`emit_energy`]) vem antes de [`step`], então o que sai é a energia que o buffer guardava no fim do tick anterior, e a
/// deste tick só sai no seguinte.
pub fn step_emitting(
    state: &mut State,
    around: &Surroundings,
    neighbours: &dyn machinery_basis_core::energy_cube::EnergyNeighbours,
    front: machinery_basis_core::sides::Face,
) -> Produced {
    emit_energy(state, neighbours, front);
    step(state, around)
}

/// O fim de `TileEntityMekanism.tickServer` do Heat Generator (`:645-655`): o comparador relê o tanque de lava, mas só se alguma coisa o marcou
/// (`makesComparatorDirty` é só o tanque de fluido, `TileEntityHeatGenerator.java:236-238`), e o nível novo é o do tanque
/// (`ContainerType.FLUID.getRedstoneSignalFromContainer`). `lava_before` é a lava do começo do tick, antes de o combustível entrar; `lava_seen` é a que o
/// tanque tinha quando o comparador leu da última vez (`None` num tile novo, que nasce sem a marca: só uma mudança dentro do tick a acende). Devolve se o
/// nível mudou, ou seja, se `notifyComparatorChange` avisa os vizinhos.
pub fn finish_tick(state: &mut State, lava_before: u64, lava_seen: &mut Option<u64>) -> bool {
    if state.lava != lava_seen.unwrap_or(lava_before) {
        state.tile.set_changed();
    }
    *lava_seen = Some(state.lava);
    let lava = state.lava;
    let tank = state.config.tank();
    state
        .tile
        .finish_tick(|| machinery_basis_core::containers::redstone_signal_of_container(lava, tank))
}

/// O `MEKTRACE` de um tick: os campos de `dev/referencia/rastros/heat_generator_*` que a regra calcula, com os mesmos nomes. `now` é o
/// tempo do jogo, `fuel` o item do slot de combustível (caminho e quantidade) e `state` o que o tick deixou.
pub fn trace_line(
    now: i64,
    state: &State,
    around: &Surroundings,
    produced: &Produced,
    fuel: (&str, u64),
) -> String {
    let faces: Vec<String> = around.face_ambient.iter().map(f64::to_string).collect();
    format!(
        "{{\"t\":{now},\"heat\":{},\"temperature\":{},\"ambient\":{},\"ambient_faces\":[{}],\"lava_faces\":{},\"energy\":{},\"capacity\":{},\"lava\":{},\"produced\":{},\"transfer_loss\":{:?},\"environment_loss\":{:?},\"active\":{},\"block_active\":{},\"update_delay\":{},\"fuel\":[\"{}\",{}]}}",
        state.heat.heat,
        state.heat.temperature(),
        around.ambient,
        faces.join(","),
        around.lava_faces,
        state.energy,
        state.config.energy_capacity_int(),
        state.lava,
        produced.energy,
        produced.transfer_loss,
        produced.environment_loss,
        state.tile.active(),
        state.tile.block_active(),
        state.tile.update_delay(),
        fuel.0,
        fuel.1,
    )
}

/// O que há no slot de combustível.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuelItem {
    /// Um balde de lava, que o slot esvazia no tanque.
    LavaBucket,
    /// Um item que queima por `burn_time` ticks, convertido em lava em proporção ao balde de lava.
    Solid {
        burn_time: i32,
        lava_bucket_burn_time: i32,
    },
}

/// `FluidFuelInventorySlot.fillOrBurn`: quanta lava o item põe no tanque, que tem `room` livre. `None` deixa o item no slot.
pub fn fuel_to_tank(item: FuelItem, room: u64) -> Option<u64> {
    if room == 0 {
        return None;
    }
    match item {
        FuelItem::LavaBucket => (room >= BUCKET_VOLUME).then_some(BUCKET_VOLUME),
        FuelItem::Solid {
            burn_time,
            lava_bucket_burn_time,
        } => {
            let fuel = lava_from_fuel(burn_time, lava_bucket_burn_time);
            (fuel > 0 && fuel <= room).then_some(fuel)
        }
    }
}

/// Quanto de lava um item de `burn_time` rende, em proporção a um balde de lava.
pub fn lava_from_fuel(burn_time: i32, lava_bucket_burn_time: i32) -> u64 {
    if burn_time <= 0 || lava_bucket_burn_time <= 0 {
        return 0;
    }
    BUCKET_VOLUME * burn_time as u64 / lava_bucket_burn_time as u64
}

// ---------------------------------------------------------------------------------------------------------------------------
// Upgrades (`TileComponentUpgrade`) e a janela deles. O gerador só aceita o de silêncio (`AttributeUpgradeSupport.MUFFLING_ONLY`). O
// item, o componente que guarda o tipo, os tipos e as tags são do Mekanism (o Generators usa os dados dele, como os sprites e o idioma),
// então os ids ficam aqui e não na `machinery_basis`.
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::upgrade_window::UpgradeSync;
use machinery_basis_core::window::{SelectedWindow, WindowType};
use std::rc::Rc;

/// `MekanismItems.UPGRADE` e o componente `MekanismDataComponents.UPGRADE_TYPE`, que guarda o tipo no item.
pub const UPGRADE_ITEM: &str = "mekanism:upgrade";
pub const UPGRADE_TYPE_COMPONENT: &str = "mekanism:upgrade_type";
/// `UpgradeIds.MUFFLING`, o único tipo que o gerador aceita.
pub const MUFFLING_UPGRADE: &str = "mekanism:muffling";
/// `MekanismTags.Upgrades.MUFFLING_ONLY`: o que `AttributeUpgradeSupport.MUFFLING_ONLY` aceita.
pub const MUFFLING_ONLY: &str = "mekanism:upgrade_support/muffling_only";

/// Os dados de upgrade do gerador: os tipos, o que a tag `muffling_only` aceita, e o item que os carrega, com o limite de pilha dele.
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
        MUFFLING_ONLY,
    ))
}

/// Os dois slots virtuais da janela de upgrades vêm depois dos do gerador (`TileComponentUpgrade`: entrada e saída).
pub const UPGRADE_SLOT: usize = SLOTS_SPEC.len();
pub const UPGRADE_OUTPUT_SLOT: usize = UPGRADE_SLOT + 1;
/// Quantos slots o contêiner do gerador declara, os virtuais inclusos.
pub const MENU_SLOTS: usize = UPGRADE_SLOT + 2;

/// Os slots do gerador no menu: o de combustível e o de energia, na ordem de `getInitialInventory`.
pub const FUEL_SLOT: usize = 0;
pub const ENERGY_SLOT: usize = 1;

/// O slot de itens que `TileEntityHeatGenerator.getInitialInventory` põe em cada lado (`MekContainerHelper.forSide`, um
/// `BasicContainerHolder`, sem configuração de lados: `TileEntityGenerator` não herda `TileEntityConfigurableMachine`): o combustível na
/// frente, na esquerda, nas costas, no alto e na base, e o slot de energia só na direita. É por instância porque os lados são relativos
/// à frente do bloco.
pub fn item_slot_of(
    front: machinery_basis_core::sides::Face,
    face: machinery_basis_core::sides::Face,
) -> usize {
    use machinery_basis_core::side_config::RelativeSide;
    match RelativeSide::from_directions(front, face) {
        RelativeSide::Right => ENERGY_SLOT,
        _ => FUEL_SLOT,
    }
}

/// As faces de item do Heat Generator por instância (H2, decisão 94): cada face alcança o slot de [`item_slot_of`] e deixa entrar e sair (o `BasicContainerHolder` não tem
/// `ISlotInfo` com `canInput` e `canOutput`); quem decide é a regra do slot.
pub fn item_instance(
    front: machinery_basis_core::sides::Face,
) -> machinery_basis_core::item_instance::InstanceItems {
    use machinery_basis_core::item_instance::{FaceItems, InstanceItems};
    let mut items = InstanceItems::default();
    for face in machinery_basis_core::sides::Face::ALL {
        items.faces[face as usize] = FaceItems {
            slots: vec![item_slot_of(front, face)],
            can_input: true,
            can_output: true,
        };
    }
    items
}

/// `isValidForInsertion(item, EXTERNAL)` do slot de combustível (`FluidFuelInventorySlot.forFuel`, `FluidFuelInventorySlot.java:56-60`): o item tem `COOKING_FUEL` ou enche o
/// tanque de lava (`canFill`: um balde de lava).
pub fn fuel_slot_insertable(cooking_fuel: bool, fills_tank: bool) -> bool {
    cooking_fuel || fills_tank
}

/// `isValidForExtraction(item, EXTERNAL)` do mesmo slot (`FluidFuelInventorySlot.java:47-54`): sai o que não é combustível e não enche o tanque, o que sobra quando "algo deu
/// muito errado", por exemplo depois de um `/reload`.
pub fn fuel_slot_extractable(cooking_fuel: bool, fills_tank: bool) -> bool {
    !cooking_fuel && !fills_tank
}

/// A capability de energia do gerador (`TileEntityGenerator.ENERGY_SIDES`, `BasicSingleHolder`): só a frente.
pub fn has_energy_capability(
    front: machinery_basis_core::sides::Face,
    face: machinery_basis_core::sides::Face,
) -> bool {
    face == front
}

/// Os slots do contêiner do menu, na ordem em que o Java os adiciona (`inventoryContainerSlots`): os dois virtuais da janela de upgrades
/// vêm ANTES dos do gerador (`MekanismTileContainer.addSlots`: "add them before the main inventory to make sure they take priority in
/// targeting"), e a prioridade do shift-clique vem daí. No Horizon eles ficam no fim, para não deslocar o NBT de mundos antigos.
pub const CONTAINER_ORDER: [usize; 4] = [UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT, FUEL_SLOT, ENERGY_SLOT];

/// `ITransactionalSlot.exists(selectedWindow)`: um slot virtual só existe com a janela dele escolhida por quem clica.
pub fn slot_exists(slot: usize, selected: Option<SelectedWindow>) -> bool {
    if slot == UPGRADE_SLOT || slot == UPGRADE_OUTPUT_SLOT {
        return SelectedWindow::exists(SelectedWindow::new(UPGRADE_WINDOW, 0), selected);
    }
    true
}

/// `isValidForInsertion(.., MANUAL)` do slot virtual de entrada (`UpgradeInventorySlot.input`): o item de upgrade com um tipo da tag
/// do gerador. `kind` é o tipo que o item carrega no componente. A saída de upgrades não aceita nada de quem clica.
pub fn accepts_upgrade(data: &UpgradeData, item: &str, kind: Option<&str>) -> bool {
    item == UPGRADE_ITEM && kind.is_some_and(|kind| data.supports(kind))
}

/// As fontes que o Heat Generator acrescenta às da estrutura do tile (`TileEntityHeatGenerator.addContainerTrackers`).
pub const SOURCE_PRODUCTION_RATE: u16 = source::MOD;
pub const SOURCE_TRANSFER_LOSS: u16 = source::MOD + 1;
pub const SOURCE_ENVIRONMENT_LOSS: u16 = source::MOD + 2;

/// Os dados que o menu do Heat Generator sincroniza, na ordem de `addContainerTrackers`: a base do tile (a lava é o tanque de fluido,
/// o calor e a capacidade vêm do capacitor, e a energia é um `BasicEnergyContainer`, que não é ajustável) e depois `getProductionRate`,
/// `getLastTransferLoss` e `getLastEnvironmentLoss`. O rastro `heat_generator_trackers` do Java confere a ordem, a classe e o valor.
pub fn heat_generator_trackers() -> Vec<Tracked> {
    let mut list = TileTrackers {
        security: true,
        redstone: true,
        fluid_tanks: 1,
        heat: true,
        energy: Some(EnergyTrackers {
            last_energy: true,
            adjustable: false,
        }),
        ..TileTrackers::default()
    }
    .trackers();
    list.extend([
        Tracked::new(SOURCE_PRODUCTION_RATE, Kind::Int),
        Tracked::new(SOURCE_TRANSFER_LOSS, Kind::Double),
        Tracked::new(SOURCE_ENVIRONMENT_LOSS, Kind::Double),
    ]);
    list
}

/// O que a janela de upgrades acrescenta (`TileComponentUpgrade.getSpecificSyncableData`).
pub fn upgrade_window_data() -> [Tracked; 2] {
    upgrade_window_trackers()
}

/// O Heat Generator visto pelo `broadcastChanges`: o objeto vivo (calor, lava, energia e o estado de redstone do tile), o que o último
/// tick produziu (`producingEnergy`, `lastTransferLoss` e `lastEnvironmentLoss` ficam nos campos do tile até o tick seguinte) e o
/// `upgradeTicks` e o mapa de upgrades.
pub struct HeatGeneratorSync<'a> {
    pub state: &'a State,
    pub last: &'a Produced,
    pub upgrades: &'a UpgradeSync,
    /// A segurança do tile (`TileComponentSecurity`): o dono e o modo que o menu leva.
    pub security: machinery_basis_core::security::SecuritySync,
}

impl TrackedSource for HeatGeneratorSync<'_> {
    fn value(&self, id: u16) -> Value {
        let redstone = self.state.tile.redstone_sync();
        match id {
            source::SECURITY_FREQUENCY => {
                let (hash, bytes) = self.security.frequency();
                Value::Codec { hash, bytes }
            }
            source::SECURITY_MODE => Value::Enum(self.security.mode()),
            source::CONTROL_TYPE => Value::Enum(redstone.control_type.ordinal()),
            source::POWERED => Value::Boolean(redstone.powered),
            source::WAS_POWERED => Value::Boolean(redstone.was_powered),
            source::FLUID_TANK => Value::Stack(StackValue {
                resource: if self.state.lava > 0 { LAVA } else { "" }.to_owned(),
                amount: self.state.lava,
            }),
            source::HEAT => Value::Double(self.state.heat.heat),
            source::HEAT_CAPACITY => Value::Double(self.state.heat.capacity),
            // O buffer só aceita energia de dentro (`BasicEnergyContainer.output`): nada entra pela capability.
            source::LAST_ENERGY_RECEIVED => Value::Long(0),
            source::ENERGY => Value::Long(i64::from(self.state.energy)),
            SOURCE_PRODUCTION_RATE => Value::Int(self.last.energy),
            SOURCE_TRANSFER_LOSS => Value::Double(self.last.transfer_loss),
            SOURCE_ENVIRONMENT_LOSS => Value::Double(self.last.environment_loss),
            source::UPGRADE_TICKS => Value::Int(self.upgrades.ticks),
            source::UPGRADE_MAP => upgrade_map(self.upgrades),
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

/// Onde cada dado da tela fica na lista do menu (`trackedData`), tirada da própria lista de `addContainerTrackers`: a posição que o servidor
/// mandou no `PacketUpdateContainer` e o cliente lê do espelho.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenIndexes {
    /// O tanque de lava (`SyncableLargeResourceStack`) e a energia.
    pub lava: usize,
    pub energy: usize,
    /// `getProductionRate`, `getLastTransferLoss` e `getLastEnvironmentLoss`.
    pub production: usize,
    pub transfer_loss: usize,
    pub environment_loss: usize,
    /// O calor do capacitor (`getHeat`).
    pub heat: usize,
}

impl ScreenIndexes {
    pub fn of(list: &[Tracked]) -> Self {
        let at = |source| position_of(list, source);
        Self {
            lava: at(source::FLUID_TANK),
            energy: at(source::ENERGY),
            production: at(SOURCE_PRODUCTION_RATE),
            transfer_loss: at(SOURCE_TRANSFER_LOSS),
            environment_loss: at(SOURCE_ENVIRONMENT_LOSS),
            heat: at(source::HEAT),
        }
    }
}

static INDEXES: LazyLock<ScreenIndexes> =
    LazyLock::new(|| ScreenIndexes::of(&heat_generator_trackers()));

static SECURITY_INDEXES: LazyLock<(usize, usize)> = LazyLock::new(|| {
    let list = heat_generator_trackers();
    (
        position_of(&list, source::SECURITY_FREQUENCY),
        position_of(&list, source::SECURITY_MODE),
    )
});

/// A segurança que o cliente lê do menu do Heat Generator (`GuiSecurityTab`).
pub fn security(mirror: &Mirror) -> machinery_basis_core::security::SecurityView {
    let (frequency, mode) = *SECURITY_INDEXES;
    machinery_basis_core::security::SecurityView::read(mirror, frequency, mode)
}

/// O que o cliente lê do espelho do menu do Heat Generator.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenData {
    pub energy: u32,
    pub lava: u64,
    pub production: u32,
    pub heat: f64,
    pub transfer_loss: f64,
    pub environment_loss: f64,
}

impl ScreenData {
    /// O que o cliente lê. Antes dos primeiros valores do servidor o espelho está vazio, e o que falta vale o padrão do dado.
    pub fn read(mirror: &Mirror) -> Self {
        let indexes = &*INDEXES;
        let unsigned = |value: i64| u32::try_from(value.clamp(0, i64::from(u32::MAX))).unwrap_or(0);
        Self {
            energy: unsigned(mirror.long(indexes.energy)),
            lava: mirror.stack(indexes.lava).amount,
            production: unsigned(i64::from(mirror.int(indexes.production))),
            heat: mirror.double(indexes.heat),
            transfer_loss: mirror.double(indexes.transfer_loss),
            environment_loss: mirror.double(indexes.environment_loss),
        }
    }
}

/// `WindowType`, na ordem do enum do Java. `CRAFTING` aceita `MAX_CRAFTING_WINDOWS` (3) janelas. É a tabela do Mekanism: o gerador abre a
/// mesma janela.
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

#[cfg(test)]
mod tests {
    use super::*;
    use machinery_basis_core::heat::AMBIENT_TEMP;

    const PLAINS: Surroundings = Surroundings {
        lava_faces: 0,
        in_nether: false,
        ambient: AMBIENT_TEMP,
        face_ambient: [AMBIENT_TEMP; 6],
    };

    fn run(ticks: u32, around: &Surroundings, lava: u64) -> (Capacitor, Produced) {
        let mut heat = new_capacitor(AMBIENT_TEMP);
        let mut last = Produced::default();
        for _ in 0..ticks {
            last = tick(&HeatConfig::DEFAULT, &mut heat, around, lava, false, true);
        }
        (heat, last)
    }

    /// Valores calculados à parte, em Python, seguindo as linhas do Java.
    fn assert_matches(ticks: u32, around: &Surroundings, lava: u64, temperature: f64, energy: i32) {
        let (heat, produced) = run(ticks, around, lava);
        assert!(
            (heat.temperature() - temperature).abs() < 0.001,
            "{ticks} ticks: {}",
            heat.temperature()
        );
        assert_eq!(produced.energy, energy, "{ticks} ticks");
    }

    #[test]
    fn matches_the_reference_curve() {
        assert_matches(1, &PLAINS, TANK_CAPACITY, 309.4944, 0);
        assert_matches(2, &PLAINS, TANK_CAPACITY, 318.5086, 0);
        assert_matches(10, &PLAINS, TANK_CAPACITY, 376.0101, 8);
        assert_matches(100, &PLAINS, TANK_CAPACITY, 486.7183, 38);
        assert_matches(3_000, &PLAINS, TANK_CAPACITY, 487.7657, 39);
    }

    #[test]
    fn surrounded_by_lava_it_produces_with_no_fuel() {
        let around = Surroundings {
            lava_faces: 7,
            ..PLAINS
        };
        assert_matches(3_000, &around, 0, 392.0052, 11);
        assert!(!run(3_000, &around, 0).1.active);
    }

    #[test]
    fn one_lava_face_warms_it_without_producing() {
        let around = Surroundings {
            lava_faces: 1,
            ..PLAINS
        };
        assert_matches(3_000, &around, 0, 313.1436, 0);
    }

    #[test]
    fn stays_cold_without_lava() {
        let (heat, produced) = run(1, &PLAINS, 0);
        assert_eq!(produced, Produced::default());
        assert_eq!(heat.temperature(), AMBIENT_TEMP);
    }

    #[test]
    fn stops_burning_when_full() {
        let mut heat = new_capacitor(AMBIENT_TEMP);
        let produced = tick(
            &HeatConfig::DEFAULT,
            &mut heat,
            &PLAINS,
            TANK_CAPACITY,
            true,
            true,
        );
        assert!(!produced.active);
        assert_eq!(produced.lava_used, 0);
    }

    #[test]
    fn less_than_a_tick_of_lava_does_not_burn() {
        assert!(!run(1, &PLAINS, FLUID_RATE - 1).1.active);
    }

    #[test]
    fn the_nether_adds_heat() {
        let nether = Surroundings {
            in_nether: true,
            ..PLAINS
        };
        assert!(run(1, &nether, 0).0.temperature() > AMBIENT_TEMP);
    }

    #[test]
    fn fuel_converts_by_burn_time() {
        assert_eq!(lava_from_fuel(1_600, 20_000), 80);
        assert_eq!(lava_from_fuel(20_000, 20_000), 1_000);
        assert_eq!(lava_from_fuel(0, 20_000), 0);
    }

    #[test]
    fn the_block_stays_lit_for_the_deactivation_delay() {
        let mut state = State::new(AMBIENT_TEMP);
        state.lava = FLUID_RATE;
        step(&mut state, &PLAINS);
        assert!(
            state.tile.active() && state.tile.block_active(),
            "liga na hora"
        );
        // Sem atraso armado, o primeiro desligamento apaga o bloco na hora e arma o atraso.
        step(&mut state, &PLAINS);
        assert!(!state.tile.active() && !state.tile.block_active());
        assert_eq!(state.tile.update_delay(), BLOCK_DEACTIVATION_DELAY);
        // Ligar de novo é instantâneo, e o atraso armado segura o desligamento seguinte.
        state.lava = FLUID_RATE;
        step(&mut state, &PLAINS);
        assert!(state.tile.block_active());
        step(&mut state, &PLAINS);
        assert!(!state.tile.active());
        assert_eq!(state.tile.update_delay(), BLOCK_DEACTIVATION_DELAY);
        for _ in 1..BLOCK_DEACTIVATION_DELAY {
            assert!(state.tile.block_active());
            step(&mut state, &PLAINS);
        }
        assert!(state.tile.block_active());
        step(&mut state, &PLAINS);
        assert!(!state.tile.block_active());
    }

    #[test]
    fn the_fuel_slot_takes_what_burns_or_fills_the_tank_and_lets_out_only_the_rest() {
        // `FluidFuelInventorySlot.forFuel`: entra o combustível e o que enche o tanque; sai só o que não é nem um nem outro.
        for (fuel, fills) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(fuel_slot_insertable(fuel, fills), fuel || fills);
            assert_eq!(fuel_slot_extractable(fuel, fills), !fuel && !fills);
        }
    }

    #[test]
    fn every_face_reaches_the_fuel_slot_except_the_right_which_reaches_the_energy_slot() {
        use machinery_basis_core::sides::Face;
        for front in [Face::North, Face::South, Face::East, Face::West] {
            let items = item_instance(front);
            for face in Face::ALL {
                let want = if face == front.right_of() {
                    ENERGY_SLOT
                } else {
                    FUEL_SLOT
                };
                assert_eq!(
                    items.face_slots()[face as usize],
                    vec![want],
                    "{front:?} {face:?}"
                );
                assert!(items.accepts_through(face, want) && items.releases_through(face, want));
            }
            assert_eq!(items.inputs(), vec![FUEL_SLOT, ENERGY_SLOT]);
        }
    }
}
