//! Os dados ficam nos caminhos do original, com os arquivos dele, para datapacks do Mekanism valerem aqui.
use mekanism_rules::{CHEMICAL_FOLDER, EMPTY_CHEMICAL, chemical_resource};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Quantos químicos o original tem, sem contar o vazio.
const CHEMICALS: usize = 64;

fn chemical_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanism")
        .join(CHEMICAL_FOLDER)
}

fn chemical_file(name: &str) -> Value {
    let path = chemical_folder().join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_chemical_of_the_original_is_read() {
    let mut read = 0;
    for entry in std::fs::read_dir(chemical_folder()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let id = format!("mekanism:{name}");
        if id == EMPTY_CHEMICAL {
            continue;
        }
        assert!(
            chemical_resource(&id, &chemical_file(&name)).is_some(),
            "{id}"
        );
        read += 1;
    }
    assert_eq!(read, CHEMICALS);
}

#[test]
fn the_empty_chemical_is_not_something_a_tank_holds() {
    assert_eq!(
        chemical_resource(EMPTY_CHEMICAL, &chemical_file("empty")),
        None
    );
}

#[test]
fn colors_and_kinds_come_from_the_original_format() {
    for (name, kind, color) in [
        ("bio", "infusion", 0x5A_46_30),
        ("carbon", "infusion", 0x2C_2C_2C),
        ("diamond", "infusion", 0x6C_ED_D8),
        ("fungi", "infusion", 0x74_65_6A),
        ("gold", "infusion", 0xF2_CD_67),
        ("redstone", "infusion", 0xB3_05_05),
        ("refined_obsidian", "infusion", 0x7C_00_ED),
        ("tin", "infusion", 0xCC_CC_D9),
        ("red", "chemical", 0xFF_38_3C),
    ] {
        let resource =
            chemical_resource(&format!("mekanism:{name}"), &chemical_file(name)).unwrap();
        assert_eq!(
            (resource.kind.as_str(), resource.color),
            (kind, color),
            "{name}"
        );
    }
}
