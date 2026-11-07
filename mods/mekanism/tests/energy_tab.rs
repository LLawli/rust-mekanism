//! A aba de energia (`GuiEnergyTab`) sem o jogo: o retângulo, o tooltip com os textos do idioma do mod, e o slot que leva o consumo ao cliente.
use mekanism_rules::{
    ELECTRIC_DATA, ENERGY_TAB, ENERGY_TAB_SPRITE, INFUSER_DATA, ScreenData, energy_tab_tooltip,
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

#[test]
fn the_consumption_rate_travels_in_its_own_two_slots() {
    for layout in [&ELECTRIC_DATA, &INFUSER_DATA] {
        let data = ScreenData {
            energy_per_tick: 70_000,
            ..ScreenData::default()
        };
        let written = data.write(layout);
        let wire: Vec<i32> = written.iter().map(|&v| i32::from(v as i16)).collect();
        assert_eq!(ScreenData::read(layout, &wire).energy_per_tick, 70_000);
        let differing: Vec<usize> = (0..layout.len)
            .filter(|&i| written[i] != ScreenData::default().write(layout)[i])
            .collect();
        assert_eq!(
            differing,
            vec![layout.energy_per_tick, layout.energy_per_tick + 1]
        );
    }
}
