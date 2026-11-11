//! A tela do cubo de energia (`GuiEnergyCube`), sem o jogo: os dados do menu, o nível do medidor, os textos dos tooltips e as molduras dos slots contra o Java.
use machinery_basis_core::container_sync::{Kind, Mirror};
use machinery_basis_core::slot_view::{SlotOverlay, SlotType, slot_type};
use machinery_basis_core::tile_trackers::source;
use mekanism_rules::{
    CUBE_GAUGE, CUBE_ITEM_CONFIG, CUBE_SLOTS_SPEC, CubeScreenData, cube_energy_tab_tooltip,
    cube_gauge_level, cube_gauge_tooltip, cube_trackers,
};
use std::path::{Path, PathBuf};

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism")
}

fn translate(key: &str) -> String {
    match key {
        "energy.mekanism.forge.short" => "FE",
        "generic.mekanism.fraction" => "%1$s/%2$s",
        "gui.mekanism.empty" => "Empty",
        "gui.mekanism.infinite" => "Infinite",
        "gui.mekanism.unit" => "Unit: %1$s",
        "gui.mekanism.max_output" => "Max Output: %1$s/t",
        "matrix.mekanism.input_rate" => "Input: %1$s/t",
        other => other,
    }
    .to_owned()
}

/// `TileEntityMekanism.addContainerTrackers` num tile com segurança, redstone e um `LastEnergyTracker` sem upgrades: a frequência e o modo de segurança, os três da redstone, o último
/// recebido e a energia, nesta ordem.
#[test]
fn the_menu_data_follows_the_tile_order() {
    let list = cube_trackers();
    let sources: Vec<u16> = list.iter().map(|tracked| tracked.source).collect();
    assert_eq!(
        sources,
        [
            source::SECURITY_FREQUENCY,
            source::SECURITY_MODE,
            source::CONTROL_TYPE,
            source::POWERED,
            source::WAS_POWERED,
            source::LAST_ENERGY_RECEIVED,
            source::ENERGY,
        ]
    );
    assert_eq!(list[5].kind, Kind::Long);
    assert_eq!(list[6].kind, Kind::Long);
}

#[test]
fn an_empty_mirror_reads_zero() {
    assert_eq!(
        CubeScreenData::read(&Mirror::new(&cube_trackers())),
        CubeScreenData::default()
    );
}

/// `GuiEnergyGauge.getScaledLevel`: zero vazio, o máximo no `Long.MAX_VALUE`, e no meio `max(1, (int) ((height - 2) * divideToLevel))`.
#[test]
fn the_gauge_level_follows_get_scaled_level() {
    let height = CUBE_GAUGE[3] as i32;
    assert_eq!(height, 50);
    assert_eq!(cube_gauge_level(0, 4_000_000, height), 0);
    assert_eq!(cube_gauge_level(1, 4_000_000_000, height), 1);
    assert_eq!(cube_gauge_level(2_000_000, 4_000_000, height), 24);
    assert_eq!(cube_gauge_level(4_000_000, 4_000_000, height), 48);
    assert_eq!(cube_gauge_level(i64::MAX, i64::MAX, height), 48);
    assert_eq!(cube_gauge_level(5, i64::MAX, height), 1);
}

/// `EnergyDisplay.getTextComponent` e `GuiEnergyGauge.getTooltipText`.
#[test]
fn the_gauge_tooltip_follows_the_energy_display() {
    assert_eq!(cube_gauge_tooltip(0, 4_000_000, &translate), "Empty");
    assert_eq!(
        cube_gauge_tooltip(2_000_000, 4_000_000, &translate),
        "2 MFE/4 MFE"
    );
    assert_eq!(
        cube_gauge_tooltip(1_500, 4_000_000, &translate),
        "1.5 kFE/4 MFE"
    );
    assert_eq!(
        cube_gauge_tooltip(i64::MAX, i64::MAX, &translate),
        "Infinite"
    );
    assert_eq!(cube_gauge_tooltip(7, i64::MAX, &translate), "7 FE/Infinite");
}

/// `GuiEnergyTab` do cubo: o último recebido, a saída máxima do tier e a unidade.
#[test]
fn the_energy_tab_lists_the_input_rate_and_the_output() {
    assert_eq!(
        cube_energy_tab_tooltip(1_500, 8_000, &translate),
        ["Input: 1.5 kFE/t", "Max Output: 8 kFE/t", "Unit: FE"]
    );
}

/// `getInitialInventory`: o de descarga (17, 35) com `MINUS` e o de carga (143, 35) com `PLUS`; a moldura vem do papel na configuração de itens
/// (`setupIOConfig(ITEM, chargeSlot, dischargeSlot, true)`: a carga é a entrada e a descarga a saída).
#[test]
fn the_slots_follow_get_initial_inventory() {
    assert_eq!(CUBE_SLOTS_SPEC[0].item, [17.0, 35.0]);
    assert_eq!(CUBE_SLOTS_SPEC[0].overlay, Some(SlotOverlay::Minus));
    assert_eq!(CUBE_SLOTS_SPEC[1].item, [143.0, 35.0]);
    assert_eq!(CUBE_SLOTS_SPEC[1].overlay, Some(SlotOverlay::Plus));
    let frame = |index: usize| {
        slot_type(
            Some(&CUBE_ITEM_CONFIG),
            index,
            CUBE_SLOTS_SPEC[index].container,
        )
    };
    assert_eq!(frame(0), Some(SlotType::Output));
    assert_eq!(frame(1), Some(SlotType::Input));
}

/// Os arquivos que a tela desenha existem no conteúdo: os overlays `plus` e `minus`, o sprite da energia (animado) e o suporte da armadura à direita.
#[test]
fn the_assets_the_screen_draws_are_in_the_content() {
    for path in [
        "textures/gui/sprites/slot/overlay/plus.png",
        "textures/gui/sprites/slot/overlay/minus.png",
        "textures/mek_liquid/energy.png",
        "textures/mek_liquid/energy.png.mcmeta",
        "textures/gui/sprites/holder_right.png",
        "textures/gui/sprites/gauge/overlay/wide.png",
    ] {
        assert!(content().join(path).is_file(), "falta {path}");
    }
}

/// O texto das telas do cubo vem do idioma do Mekanism: os cinco títulos e as duas linhas da aba de energia.
#[test]
fn the_screen_text_is_in_the_language_file() {
    let lang = std::fs::read_to_string(content().join("lang/en_us.json")).unwrap();
    for key in [
        "container.mekanism.basic_energy_cube",
        "container.mekanism.advanced_energy_cube",
        "container.mekanism.elite_energy_cube",
        "container.mekanism.ultimate_energy_cube",
        "container.mekanism.creative_energy_cube",
        "matrix.mekanism.input_rate",
        "gui.mekanism.max_output",
    ] {
        assert!(lang.contains(&format!("\"{key}\"")), "falta {key}");
    }
}
