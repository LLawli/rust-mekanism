//! O que as telas desenham, sem o jogo: as molduras dos slots, os sprites e a escala deles contra os arquivos do original, os
//! tooltips das barras e os ícones de químico contra os dados.
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::slot_view::SlotType;
use mekanism_rules::{
    BAR_FRAME_SLICE, CHEMICAL_FOLDER, CHEMICAL_ICONS, ELECTRIC_SCREEN, EMPTY_CHEMICAL,
    GAUGE_FRAME_SLICE, INFUSER_SCREEN, PANEL_SLICE, POWER_BAR_TILE, chemical_icon,
    chemical_icon_slot, chemical_resource, chemical_tooltip, energy_tooltip,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

fn sprites() -> PathBuf {
    content().join("assets/mekanism/textures/gui/sprites")
}

/// Largura e altura de um PNG, do cabeçalho `IHDR`.
fn png_size(path: &Path) -> [u32; 2] {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{}", path.display());
    let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    [word(16), word(20)]
}

fn mcmeta(path: &Path) -> String {
    std::fs::read_to_string(path.with_extension("png.mcmeta")).unwrap()
}

/// O `gui.scaling` que o código usa é o que o `.png.mcmeta` copiado do original diz, e o sprite tem o tamanho dele.
#[test]
fn the_scaling_in_the_code_is_what_the_mcmeta_files_say() {
    for (sprite, expected) in [
        ("base", PANEL_SLICE),
        ("bar/base", BAR_FRAME_SLICE),
        ("gauge/normal", GAUGE_FRAME_SLICE),
    ] {
        let path = sprites().join(format!("{sprite}.png"));
        let slice = NineSlice::from_mcmeta(&mcmeta(&path)).unwrap();
        assert_eq!(slice, expected, "{sprite}");
        let [width, height] = png_size(&path);
        assert_eq!([width as f32, height as f32], expected.size, "{sprite}");
    }
}

#[test]
fn the_power_bar_tiles_its_sprite_at_the_size_the_mcmeta_gives() {
    let path = sprites().join("bar/vertical_power.png");
    let meta: Value = serde_json::from_str(&mcmeta(&path)).unwrap();
    let scaling = &meta["gui"]["scaling"];
    assert_eq!(scaling["type"], "tile");
    assert_eq!(
        [
            scaling["width"].as_f64().unwrap() as f32,
            scaling["height"].as_f64().unwrap() as f32
        ],
        POWER_BAR_TILE
    );
    let [width, height] = png_size(&path);
    assert_eq!([width as f32, height as f32], POWER_BAR_TILE);
}

#[test]
fn the_slot_sprites_that_have_a_scaling_draw_at_their_own_size() {
    // `slot/output` e `slot/warning` são nove partes de 18 por 18, e a moldura mede 18: um pedaço só.
    for sprite in ["slot/output", "slot/warning"] {
        let path = sprites().join(format!("{sprite}.png"));
        let slice = NineSlice::from_mcmeta(&mcmeta(&path)).unwrap();
        assert_eq!(slice.size, [18.0, 18.0], "{sprite}");
        assert_eq!(png_size(&path), [18, 18], "{sprite}");
    }
}

fn chemical_folder() -> PathBuf {
    content().join("data/mekanism").join(CHEMICAL_FOLDER)
}

/// Todo químico de infusão do original tem o sprite na lista, e cada sprite da lista existe na pasta de GUI.
#[test]
fn every_infusion_icon_is_known_and_shipped() {
    let mut infusions = 0;
    for entry in std::fs::read_dir(chemical_folder()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let id = format!("mekanism:{name}");
        if id == EMPTY_CHEMICAL {
            continue;
        }
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let resource = chemical_resource(&id, &json).unwrap();
        if resource.kind != "infusion" {
            continue;
        }
        infusions += 1;
        let icon = resource.icon.as_deref();
        let slot = chemical_icon_slot(icon);
        assert_ne!(slot, 0, "{id}: sprite fora da lista, {icon:?}");
        assert_eq!(chemical_icon(slot).unwrap(), icon.unwrap());
        // O `tint` é o do arquivo (`#aarrggbb`), com o alfa.
        assert_eq!(resource.tint >> 24, 0xFF, "{id}");
    }
    assert_eq!(infusions, 8);
    // Cada sprite da lista existe no caminho do original, e o atlas de blocos (`blocks.json`, o arquivo do original) lista a pasta.
    for icon in CHEMICAL_ICONS {
        let relative = icon.strip_prefix("mekanism:").unwrap();
        let path = content().join("assets/mekanism/textures").join(relative);
        assert!(path.with_extension("png").is_file(), "{icon}");
        assert!(path.with_extension("png.mcmeta").is_file(), "{icon}");
    }
    let atlas =
        std::fs::read_to_string(content().join("assets/minecraft/atlases/blocks.json")).unwrap();
    assert!(atlas.contains("\"source\": \"mek_chemical\""));
    assert_eq!(chemical_icon_slot(None), 0);
    assert_eq!(chemical_icon(0), None);
    assert_eq!(chemical_icon(CHEMICAL_ICONS.len() as u32 + 1), None);
}

#[test]
fn the_infusion_tint_is_the_one_of_the_file_not_the_color_representation() {
    // `bio` tem `tint` branco e `color_representation` marrom: a barra usa o `tint`, e é o sprite que tem a cor.
    let bio: Value =
        serde_json::from_str(&std::fs::read_to_string(chemical_folder().join("bio.json")).unwrap())
            .unwrap();
    let bio = chemical_resource("mekanism:bio", &bio).unwrap();
    assert_eq!(bio.tint, 0xFF_FF_FF_FF);
    assert_eq!(bio.color, 0x5A_46_30);
    let carbon: Value = serde_json::from_str(
        &std::fs::read_to_string(chemical_folder().join("carbon.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        chemical_resource("mekanism:carbon", &carbon).unwrap().tint,
        0xFF_2C_2C_2C
    );
}

/// As molduras que `GuiMekanism.addSlots` escolhe: o papel de cada slot na configuração de lados.
#[test]
fn the_slot_frames_follow_the_side_configuration() {
    let frames = |spec: &mekanism_rules::ScreenSpec| -> Vec<SlotType> {
        (0..spec.slots.len())
            .map(|slot| spec.slot_type(slot).unwrap())
            .collect()
    };
    // Entrada vermelha, saída azul, energia verde.
    assert_eq!(
        frames(&ELECTRIC_SCREEN),
        [SlotType::Input, SlotType::Output, SlotType::Power]
    );
    // O Metallurgic Infuser acrescenta o slot extra, o do químico, amarelo.
    assert_eq!(
        frames(&INFUSER_SCREEN),
        [
            SlotType::Extra,
            SlotType::Input,
            SlotType::Output,
            SlotType::Power
        ]
    );
    let names: Vec<_> = frames(&INFUSER_SCREEN)
        .iter()
        .map(|slot| slot.name())
        .collect();
    assert_eq!(names, ["extra", "input", "output", "power"]);
}

#[test]
fn only_the_energy_slot_has_the_overlay() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        let overlays: Vec<_> = spec
            .slots
            .iter()
            .map(|slot| slot.overlay.map(|overlay| overlay.name()))
            .collect();
        assert_eq!(overlays.iter().flatten().count(), 1);
        assert_eq!(overlays.last().unwrap(), &Some("power"));
    }
}

#[test]
fn the_slot_positions_are_the_ones_of_the_item_slots() {
    let item = |spec: &mekanism_rules::ScreenSpec| {
        spec.slots.iter().map(|slot| slot.item).collect::<Vec<_>>()
    };
    assert_eq!(
        item(&ELECTRIC_SCREEN),
        [[64.0, 17.0], [116.0, 35.0], [64.0, 53.0]]
    );
    assert_eq!(
        item(&INFUSER_SCREEN),
        [[17.0, 35.0], [51.0, 43.0], [109.0, 43.0], [143.0, 35.0]]
    );
}

fn english(key: &str) -> String {
    let path = content().join("assets/mekanism/lang/en_us.json");
    let lang: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    lang[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

#[test]
fn the_energy_tooltip_is_energy_over_capacity_in_fe() {
    assert_eq!(energy_tooltip(0, 32, &english), "0 FE/32 FE");
    assert_eq!(energy_tooltip(1_500, 8_000, &english), "1.5 kFE/8 kFE");
    assert_eq!(
        energy_tooltip(256_000_000, 256_000_000, &english),
        "256 MFE/256 MFE"
    );
    // Sem capacidade, só a energia.
    assert_eq!(energy_tooltip(500, 0, &english), "500 FE");
}

#[test]
fn the_chemical_tooltip_is_the_name_and_the_amount_or_empty() {
    assert_eq!(chemical_tooltip(None, 0, &english), "Empty");
    assert_eq!(
        chemical_tooltip(Some("Biomass"), 1_000, &english),
        "Biomass: 1,000 mB"
    );
    assert_eq!(chemical_tooltip(Some("Tin"), 7, &english), "Tin: 7 mB");
    // O nome sai da chave `chemical.<ns>.<nome>` do id.
    assert_eq!(english("chemical.mekanism.bio"), "Biomass");
    assert_eq!(
        english("chemical.mekanism.refined_obsidian"),
        "Refined Obsidian"
    );
}

/// Todo sprite de GUI que o código das telas cita tem o arquivo, e o conjunto citado não encolhe sem ninguém notar.
#[test]
fn every_sprite_the_screens_name_exists() {
    let mut named = std::collections::BTreeSet::new();
    for glue in [
        "mods/mekanism/native/glue.rs",
        "mods/mekanismgenerators/native/glue.rs",
        "mods/machinery_basis/glue.rs",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(glue);
        let source = std::fs::read_to_string(path).unwrap();
        let mut rest = source.as_str();
        while let Some(at) = rest.find("\"mekanism:gui/sprites/") {
            let tail = &rest[at + 1..];
            let end = tail.find('"').unwrap();
            named.insert(tail[..end].to_owned());
            rest = &tail[end..];
        }
    }
    assert!(named.len() >= 20, "{named:?}");
    for sprite in &named {
        let relative = sprite.strip_prefix("mekanism:gui/").unwrap();
        let path = content()
            .join("assets/mekanism/textures/gui")
            .join(relative);
        assert!(path.with_extension("png").is_file(), "{sprite}");
    }
}
