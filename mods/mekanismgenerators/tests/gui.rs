//! A tela do Heat Generator sem o jogo: os valores de escala contra os arquivos do Mekanism, as molduras dos slots e os sprites que
//! o código cita.
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::slot_view::{SlotType, slot_type};
use mekanismgenerators_rules::{
    BAR_FRAME_SLICE, EMPTY_LANG, ENERGY_UNIT_LANG, FRACTION_LANG, GAUGE_FRAME_SLICE,
    LAVA_NAME_LANG, PANEL_SLICE, POWER_BAR_TILE, SLOTS_SPEC, STORED_MB_LANG, energy_tooltip,
    lava_tooltip,
};
use std::path::{Path, PathBuf};

fn sprites() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/assets/mekanism/textures/gui/sprites")
}

fn mcmeta(sprite: &str) -> String {
    std::fs::read_to_string(sprites().join(format!("{sprite}.png.mcmeta"))).unwrap()
}

/// O mosaico de 2 por 2 de `bar/vertical_power`, lido do `.mcmeta` sem depender de um parser de JSON a mais.
fn tile_of(text: &str) -> [f32; 2] {
    let number = |key: &str| {
        let at = text.find(&format!("\"{key}\"")).unwrap();
        let rest = &text[at..];
        let digits: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse::<f32>().unwrap()
    };
    assert!(text.contains("\"tile\""));
    [number("width"), number("height")]
}

#[test]
fn the_scaling_in_the_code_is_what_the_mcmeta_files_say() {
    for (sprite, expected) in [
        ("base", PANEL_SLICE),
        ("bar/base", BAR_FRAME_SLICE),
        ("gauge/normal", GAUGE_FRAME_SLICE),
    ] {
        assert_eq!(
            NineSlice::from_mcmeta(&mcmeta(sprite)).unwrap(),
            expected,
            "{sprite}"
        );
    }
    assert_eq!(tile_of(&mcmeta("bar/vertical_power")), POWER_BAR_TILE);
}

/// Sem configuração de lados de itens, o gerador escolhe a moldura pelo tipo do slot: o do combustível é normal, o de energia verde.
#[test]
fn the_slot_frames_come_from_the_container_type() {
    let frames: Vec<_> = SLOTS_SPEC
        .iter()
        .enumerate()
        .map(|(index, spec)| slot_type(None, index, spec.container).unwrap())
        .collect();
    assert_eq!(frames, [SlotType::Normal, SlotType::Power]);
    assert_eq!(SLOTS_SPEC[0].overlay, None);
    assert_eq!(
        SLOTS_SPEC[1].overlay.map(|overlay| overlay.name()),
        Some("power")
    );
}

/// Os slots de item ficam onde o Java os põe: 17,35 e 143,35, e o Horizon desenha o item ali, com a moldura um pixel antes.
#[test]
fn the_item_slots_are_where_the_java_puts_them() {
    assert_eq!(SLOTS_SPEC[0].item, [17.0, 35.0]);
    assert_eq!(SLOTS_SPEC[1].item, [143.0, 35.0]);
}

#[test]
fn every_sprite_the_screen_names_exists() {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("native/glue.rs"))
            .unwrap();
    let mut named = Vec::new();
    let mut rest = source.as_str();
    while let Some(at) = rest.find("\"mekanism:gui/sprites/") {
        let tail = &rest[at + 1..];
        let end = tail.find('"').unwrap();
        named.push(tail[..end].to_owned());
        rest = &tail[end..];
    }
    assert!(named.len() >= 8, "{named:?}");
    for sprite in &named {
        let relative = sprite.strip_prefix("mekanism:gui/sprites/").unwrap();
        assert!(
            sprites().join(format!("{relative}.png")).is_file(),
            "{sprite}"
        );
    }
}

/// O idioma que os tooltips leem: as chaves do Mekanism (que o Generators usa) e o nome vanilla da lava.
fn english(key: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/assets/mekanism/lang/en_us.json");
    let text = std::fs::read_to_string(path).unwrap();
    let find = |wanted: &str| {
        let at = text
            .find(&format!("\"{wanted}\": \""))
            .map(|at| at + wanted.len() + 5);
        at.map(|at| text[at..].split('"').next().unwrap().to_owned())
    };
    match key {
        LAVA_NAME_LANG => "Lava".to_owned(),
        _ => find(key).unwrap_or_else(|| panic!("{key}")),
    }
}

#[test]
fn the_tooltips_use_the_lang_keys_of_the_original() {
    assert_eq!(english(FRACTION_LANG), "%1$s/%2$s");
    assert_eq!(english(STORED_MB_LANG), "%1$s: %2$s mB");
    assert_eq!(english(EMPTY_LANG), "Empty");
    assert_eq!(english(ENERGY_UNIT_LANG), "FE");
}

#[test]
fn the_bar_tooltip_is_energy_over_capacity_in_fe() {
    assert_eq!(energy_tooltip(0, 240, &english), "0 FE/240 FE");
    assert_eq!(energy_tooltip(120, 240, &english), "120 FE/240 FE");
}

#[test]
fn the_gauge_tooltip_names_the_lava_or_says_empty() {
    assert_eq!(lava_tooltip(0, &english), "Empty");
    assert_eq!(lava_tooltip(1_000, &english), "Lava: 1,000 mB");
    assert_eq!(lava_tooltip(250, &english), "Lava: 250 mB");
}
