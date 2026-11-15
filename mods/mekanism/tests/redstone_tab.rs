//! A aba de controle de redstone (`GuiRedstoneControlTab`): os sprites copiados do original byte a byte (a soma FNV-1a de cada arquivo, tirada do `mekanism-ref`), as chaves de língua
//! e a posição nas três telas. O que a aba faz (o ciclo de modos, o efeito no tick, o NBT e o item, a recusa) está em `redstone_trace.rs`, contra o rastro do Java.
use machinery_basis_core::redstone_tab;
use machinery_basis_core::tile::RedstoneControl;
use std::path::PathBuf;

fn sprites() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/textures/gui/sprites")
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Cada arquivo que a aba usa é o do original: `src/main/resources/assets/mekanism/textures/gui/sprites` do `mekanism-ref`. A soma e o tamanho vêm de lá.
#[test]
fn the_sprites_of_the_tab_are_the_ones_of_the_original_byte_for_byte() {
    let expected = [
        ("redstone_control/disabled.png", 177, 0xe0d3_5e2e_7a5b_0868),
        ("redstone_control/high.png", 156, 0x2b5a_6f85_c937_312c),
        ("redstone_control/low.png", 177, 0x9f92_013a_69d4_9020),
        ("redstone_control/pulse.png", 148, 0xc69d_7e9d_b9b8_362e),
        (
            "redstone_control/pulse.png.mcmeta",
            121,
            0xd7dd_b1a6_8d4d_6d47,
        ),
        ("holder_right.png", 128, 0xbae0_8f40_b35a_d540),
        ("warning.png", 110, 0x668a_64a3_e2da_b264),
    ];
    for (name, size, sum) in expected {
        let bytes = std::fs::read(sprites().join(name)).unwrap_or_else(|_| panic!("sem {name}"));
        assert_eq!((bytes.len(), fnv(&bytes)), (size, sum), "{name}");
    }
}

/// O código nomeia cada modo pelo sprite que existe.
#[test]
fn every_mode_names_a_sprite_that_exists() {
    for mode in [
        RedstoneControl::Disabled,
        RedstoneControl::High,
        RedstoneControl::Low,
        RedstoneControl::Pulse,
    ] {
        let name = redstone_tab::overlay(mode)
            .strip_prefix("mekanism:")
            .unwrap();
        assert!(sprites().join(format!("{name}.png")).is_file(), "{name}");
    }
}

fn english(key: &str) -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    json[key]
        .as_str()
        .unwrap_or_else(|| panic!("sem {key}"))
        .to_owned()
}

/// `MekanismLang.REDSTONE_CONTROL_*` do `en_us.json` que a datagen do original escreve.
#[test]
fn the_tooltip_of_each_mode_is_the_text_of_the_original() {
    let texts = [
        (RedstoneControl::Disabled, "Redstone Detection: IGNORED"),
        (RedstoneControl::High, "Redstone Detection: NORMAL"),
        (RedstoneControl::Low, "Redstone Detection: INVERTED"),
        (RedstoneControl::Pulse, "Redstone Detection: PULSE"),
    ];
    for (mode, text) in texts {
        assert_eq!(english(redstone_tab::tooltip_key(mode)), text);
    }
}

/// `GuiRedstoneControlTab`: o suporte em (176, 137), o botão em (179, 141), a cor `0xFFC9071F`, igual nas máquinas, no gerador e no cubo.
#[test]
fn the_tab_sits_at_the_same_place_in_every_screen() {
    assert_eq!(mekanism_rules::SCREEN_IMAGE_WIDTH, 176);
    assert_eq!(
        redstone_tab::tab(mekanism_rules::SCREEN_IMAGE_WIDTH).holder(),
        [176.0, 137.0, 26.0, 26.0]
    );
    assert_eq!(redstone_tab::tab(176).button(), [179.0, 141.0, 18.0, 18.0]);
    assert_eq!(redstone_tab::TAB_TINT, 0xFFC9_071F);
}
