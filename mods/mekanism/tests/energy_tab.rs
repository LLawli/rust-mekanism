//! A aba de energia (`GuiEnergyTab`) sem o jogo: o retângulo, o tooltip com os textos do idioma do mod, e o dado que leva o consumo ao cliente.
mod common;

use common::Live;
use mekanism_rules::{
    ELECTRIC_SCREEN, ENERGY_TAB, ENERGY_TAB_SPRITE, INFUSER_SCREEN, ScreenData, energy_tab_tooltip,
};
use serde_json::Value;
use std::path::Path;

fn lang() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn tooltip(active: bool, per_tick: u32, energy: u64, capacity: u64) -> Vec<String> {
    let lang = lang();
    energy_tab_tooltip(active, per_tick, energy, capacity, &|key: &str| {
        lang[key].as_str().unwrap_or(key).to_owned()
    })
}

#[test]
fn the_tab_is_the_texture_element_at_minus_26_137() {
    assert_eq!(ENERGY_TAB.rect, [-26.0, 137.0, 26.0, 26.0]);
    assert_eq!(ENERGY_TAB_SPRITE, "mekanism:gui/sprites/tab/energy_info");
    assert!(ENERGY_TAB.is_mouse_over([-26.0, 137.0], false));
    assert!(!ENERGY_TAB.is_mouse_over([-26.0, 163.0], false));
    assert!(!ENERGY_TAB.is_mouse_over([0.0, 140.0], false));
}

/// `Using: <consumo>/t` só com a máquina ativa, `Needed: <capacidade - energia>`, e `Unit: FE`.
#[test]
fn the_tooltip_says_what_it_uses_what_it_needs_and_the_unit() {
    assert_eq!(
        tooltip(true, 50, 0, 1_000),
        ["Using: 50 FE/t", "Needed: 1 kFE", "Unit: FE"]
    );
    assert_eq!(
        tooltip(false, 50, 250, 1_000),
        ["Using: 0 FE/t", "Needed: 750 FE", "Unit: FE"]
    );
    // Cheia, não falta nada; com energia acima da capacidade também não (`max(0, ...)`).
    assert_eq!(tooltip(true, 1_500, 1_000, 1_000)[1], "Needed: 0 FE");
    assert_eq!(tooltip(true, 1_500, 2_000, 1_000)[1], "Needed: 0 FE");
    assert_eq!(tooltip(true, 1_500, 0, 1_000)[0], "Using: 1.5 kFE/t");
}

/// O consumo (`getEnergyPerTick`, um `SyncableInt`) chega ao cliente inteiro, sem corte de tamanho: valores acima de
/// 65535 e o maior inteiro voltam iguais, e nenhum outro dado da tela muda.
#[test]
fn the_consumption_rate_arrives_whole_and_moves_nothing_else() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        for per_tick in [0, 1, 70_000, i32::MAX] {
            let live = Live {
                energy_per_tick: per_tick,
                ..Live::default()
            };
            let mirror = live.mirror(spec);
            assert_eq!(mirror.int((spec.indexes)().energy_per_tick), per_tick);
            assert_eq!(
                ScreenData::read(spec, &mirror),
                ScreenData {
                    energy_per_tick: per_tick as u32,
                    ..ScreenData::default()
                }
            );
        }
    }
}
