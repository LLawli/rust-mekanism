// Regras do Heat Generator, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o
// módulo crate::game::mekanismgenerators_rules. Segue TileEntityHeatGenerator.
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::slot_view::{ContainerSlotType, SlotOverlay, SlotSpec};
use machinery_basis_core::units::{energy_fraction, stored_mb};

pub const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
pub const MENU: &str = "mekanismgenerators:heat_generator";
pub const LAVA: &str = "minecraft:lava";

pub const HEAT_CAPACITY: f64 = 10.0;
pub const INVERSE_CONDUCTION: f64 = 5.0;
pub const INVERSE_INSULATION: f64 = 100.0;
/// A face de baixo usa a isolação padrão: é por ela que o calor sairia para um vizinho.
pub const INSULATION_BY_FACE: [f64; 6] = [0.0, 100.0, 100.0, 100.0, 100.0, 100.0];
const THERMAL_EFFICIENCY: f64 = 0.5;

/// Valores padrão de `GeneratorsConfig` e `GeneratorsStorageConfig`.
pub const HEAT_GENERATION: f64 = 100.0;
pub const HEAT_GENERATION_LAVA: f64 = 7.0;
pub const HEAT_GENERATION_NETHER: f64 = 10.0;
pub const FLUID_RATE: u64 = 100;
pub const TANK_CAPACITY: u64 = 1_000;
pub const ENERGY_CAPACITY: i32 = 240;
/// As seis faces mais a lava dentro do bloco, o Nether e a queima.
pub const MAX_PRODUCTION: i32 = 7 * 7 + 10 + 100;
pub const BUCKET_VOLUME: u64 = 1_000;

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
    pub ambient: f64,
}

/// O que aconteceu no tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Produced {
    pub active: bool,
    pub lava_used: u64,
    pub energy: i32,
}

pub fn new_capacitor(ambient: f64) -> Capacitor {
    Capacitor::new(
        HEAT_CAPACITY,
        INVERSE_CONDUCTION,
        INVERSE_INSULATION,
        ambient,
    )
}

fn boost(around: &Surroundings) -> f64 {
    let lava = HEAT_GENERATION_LAVA * f64::from(around.lava_faces);
    if around.in_nether {
        lava + HEAT_GENERATION_NETHER
    } else {
        lava
    }
}

/// Um tick. `lava` é o que há no tanque, e a energia sai sem o limite do buffer.
pub fn tick(heat: &mut Capacitor, around: &Surroundings, lava: u64, energy_full: bool) -> Produced {
    heat.handle(boost(around));
    let active = lava >= FLUID_RATE && !energy_full;
    if active {
        heat.handle(HEAT_GENERATION);
    }
    let temperature = heat.temperature();
    let carnot = 1.0 - around.ambient.min(temperature) / around.ambient.max(temperature);
    let heat_lost = THERMAL_EFFICIENCY * (temperature - around.ambient);
    heat.handle(-heat_lost);
    let energy = ((heat_lost.abs() * carnot) as i32).min(MAX_PRODUCTION);
    heat.lose_to_environment(around.ambient, INSULATION_BY_FACE);
    Produced {
        active,
        lava_used: if active { FLUID_RATE } else { 0 },
        energy,
    }
}

/// Quanto de lava um item de `burn_time` rende, em proporção a um balde de lava.
pub fn lava_from_fuel(burn_time: i32, lava_bucket_burn_time: i32) -> u64 {
    if burn_time <= 0 || lava_bucket_burn_time <= 0 {
        return 0;
    }
    BUCKET_VOLUME * burn_time as u64 / lava_bucket_burn_time as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use machinery_basis_core::heat::AMBIENT_TEMP;

    const PLAINS: Surroundings = Surroundings {
        lava_faces: 0,
        in_nether: false,
        ambient: AMBIENT_TEMP,
    };

    fn run(ticks: u32, around: &Surroundings, lava: u64) -> (Capacitor, Produced) {
        let mut heat = new_capacitor(AMBIENT_TEMP);
        let mut last = Produced::default();
        for _ in 0..ticks {
            last = tick(&mut heat, around, lava, false);
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
        let produced = tick(&mut heat, &PLAINS, TANK_CAPACITY, true);
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
}
