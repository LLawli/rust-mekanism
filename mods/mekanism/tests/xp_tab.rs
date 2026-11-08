//! A aba de xp (`GuiXpContainmentTab`) contra o que o cliente oficial dá: onde ela fica, e se aparece, em cada estado da cena
//! (`xp_tab.jsonl`, do log da `CenaTelas`). Mais o que o port decide com a mesma regra: o clique, o foco e o tooltip.
mod common;

use common::Live;
use machinery_basis_core::side_tab::{ButtonLook, XpTab};
use mekanism_rules::{
    ELECTRIC_SCREEN, INFUSER_SCREEN, ScreenData, XP_TAB_ICON, XP_TAB_TINT, XP_TAB_TOOLTIP,
    XP_TAB_Y, xp_tab,
};
use serde_json::Value;
use std::path::Path;

fn trace() -> Vec<Value> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/xp_tab.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Se o tile da cena tem o upgrade quando a tela abre: `hasXpContainment()`. `instalando` põe o upgrade no slot e roda 10 ticks, antes
/// dos 20 que a instalação leva; `instalado` roda 30.
fn has_containment(case: &str) -> bool {
    !(case.ends_with("_sem")
        || case.ends_with("_hover_sem")
        || case.contains("_nome_")
        || case.ends_with("_instalando"))
}

#[test]
fn the_tab_sits_where_the_original_puts_it_and_shows_with_the_upgrade() {
    let lines = trace();
    assert!(lines.len() >= 20, "{} casos", lines.len());
    let tab = xp_tab();
    for line in &lines {
        let case = line["case"].as_str().unwrap();
        let rect: Vec<f32> = ["x", "y", "w", "h"]
            .iter()
            .map(|key| line[*key].as_f64().unwrap() as f32)
            .collect();
        assert_eq!(tab.tab.holder().to_vec(), rect, "{case}");
        assert_eq!(
            XpTab::visible(has_containment(case)),
            line["visible"],
            "{case}"
        );
    }
}

/// O botão de dentro, o tooltip e o ícone são os de `GuiXpContainmentTab`.
#[test]
fn the_button_the_tooltip_and_the_look_are_the_ones_of_the_class() {
    let tab = xp_tab();
    assert_eq!(XP_TAB_Y, 90);
    assert_eq!(tab.tab.button(), [179.0, 94.0, 18.0, 18.0]);
    assert_eq!(XP_TAB_TINT, 0xFF80FF20);
    assert_eq!(XP_TAB_ICON, "mekanism:gui/sprites/tab/xp_containment");
    assert_eq!(XP_TAB_TOOLTIP, "tab.mekanism.xp_containment");
    // O mouse sobre o botão dá foco; fora dele não.
    assert_eq!(
        ButtonLook::of(true, tab.hovered(true, [188.0, 103.0], false)),
        ButtonLook::Focus
    );
    assert_eq!(
        ButtonLook::of(true, tab.hovered(true, [200.0, 115.0], false)),
        ButtonLook::Normal
    );
    // Atrás da janela de upgrades, sem foco.
    assert_eq!(
        ButtonLook::of(true, tab.hovered(true, [183.0, 100.0], true)),
        ButtonLook::Normal
    );
}

/// O cliente só sabe do upgrade pelo `SyncableBoolean` de `hasXpContainment` que o servidor manda: sem ele a aba não existe.
#[test]
fn the_tab_follows_the_synced_flag() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        let with = Live {
            xp_containment: true,
            ..Live::default()
        }
        .mirror(spec);
        let without = Live::default().mirror(spec);
        assert!(XpTab::visible(ScreenData::read(spec, &with).xp_containment));
        assert!(!XpTab::visible(
            ScreenData::read(spec, &without).xp_containment
        ));
    }
}
