//! O módulo `obj-material` que entra em `crates/horizon-assets/src/obj.rs` (`mods/machinery_basis/obj_material.rs`): o `Kd` e o `neoforge_TintIndex` de um `.mtl`, que o Horizon não lê.
//! O arquivo é código só de `std`, então o teste o compila como está. A conta é a do NeoForge, lida no código dele (`ObjMaterialLibrary`, `ObjGeometry.parseVector4` e `makeQuad`,
//! `VertexConsumer.setColor(float, float, float, float)` e `ARGB.color`); os `.mtl` são os do original que o conteúdo do port traz, byte a byte.
#[path = "../../obj_material.rs"]
mod material;

use material::{NO_TINT, Properties, color, decode, encode, key, properties};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn mtl(name: &str) -> String {
    let path = root()
        .join("mods/mekanism/content/assets/mekanism/models/block")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `Kd 0.8 0.8 0.8` vira `(int)(0.8F * 255.0F)` = 204 em cada canal, com o alfa 1 do `parseVector4` (255): o `0xFFCCCCCC` que o spike mediu no original.
#[test]
fn kd_of_the_transmitter_materials_is_the_gray_the_original_multiplies() {
    for (file, count) in [("transmitter_small.mtl", 9), ("transmitter_large.mtl", 9)] {
        let found = properties(&mtl(file));
        assert_eq!(found.len(), count, "{file}");
        for (name, (kd, tint)) in &found {
            assert_eq!(*kd, Some(0xFFCC_CCCCu32 as i32), "{file} {name}");
            assert_eq!(*tint, NO_TINT, "{file} {name}");
        }
    }
    assert_eq!(color([0.8, 0.8, 0.8, 1.0]), 0xFFCC_CCCCu32 as i32);
}

/// O alfa vem do quarto número de `Kd` (`parseVector4` com 4 números), os canais que faltam valem 0, e o que passa de 1 entra mascarado em 8 bits (`ARGB.color`).
#[test]
fn the_color_follows_parse_vector4_and_argb() {
    let one = |line: &str| {
        properties(&format!("newmtl m\n{line}\n"))
            .pop()
            .map(|(_, p)| p.0.unwrap())
    };
    assert_eq!(one("Kd 0.5"), Some(0xFF7F_0000u32 as i32));
    assert_eq!(one("Kd 0.5 0.25"), Some(0xFF7F_3F00u32 as i32));
    assert_eq!(one("Kd 0.5 0.25 0.1"), Some(0xFF7F_3F19u32 as i32));
    assert_eq!(one("Kd 0.5 0.25 0.1 0.5"), Some(0x7F7F_3F19u32 as i32));
    assert_eq!(one("Kd 0 0 0"), Some(0xFF00_0000u32 as i32));
    assert_eq!(color([1.0, 0.0, 0.0, 1.0]), 0xFFFF_0000u32 as i32);
    assert_eq!(color([2.0, 0.0, 0.0, 1.0]), 0xFFFE_0000u32 as i32);
    assert_eq!(color([0.0, 2.0, 0.0, 1.0]), 0xFF00_FE00u32 as i32);
}

/// O vidro do transportador e o conteúdo têm `neoforge_TintIndex 0`; `forge_TintIndex` vale igual; a última linha vence e um `newmtl` repetido recomeça o material.
#[test]
fn the_tint_index_comes_from_neoforge_tint_index() {
    // O primeiro material de `transmitter_glass_large.mtl` do original (o vidro do transportador logístico, que só se desenha com cor), como ele o escreve.
    let glass = properties(
        "newmtl None\nKa 0.000000 0.000000 0.000000\nneoforge_TintIndex 0\nKd 0.8 0.8 0.8\nd 1\nmap_Kd #side_glass\n",
    );
    assert_eq!(
        glass,
        vec![("None".to_owned(), (Some(0xFFCC_CCCCu32 as i32), 0))]
    );
    // O conteúdo (`transmitter_contents.mtl`) escreve o índice depois do `d`.
    let contents = properties(
        "newmtl Contents\nKa 0.000000 0.000000 0.000000\nKd 0.8 0.8 0.8\nd 1\nneoforge_TintIndex 0\nmap_Kd #contents\n",
    );
    assert_eq!(
        contents,
        glass
            .iter()
            .map(|(_, p)| ("Contents".to_owned(), *p))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        properties("newmtl a\nforge_TintIndex 2\n"),
        vec![("a".to_owned(), (None, 2))]
    );
    let text = "newmtl a\nforge_TintIndex 2\nnewmtl b\nneoforge_TintIndex 1\nneoforge_TintIndex 3\nKd 0.2 0.2 0.2\nnewmtl a\nKd 1 1 1\nnewmtl c\nmap_Kd #x\n";
    let found = properties(text);
    assert_eq!(
        found,
        vec![("b".to_owned(), (Some(0xFF33_3333u32 as i32), 3))]
    );
}

/// `Kd 1 1 1` é a cor que o quad já tem: o material sem `Kd` útil nem índice de tinta não ganha nada, e o modelo que não traz `Kd` (o MekaSuit, o Wind Generator) não muda.
#[test]
fn a_material_with_nothing_to_apply_is_left_out() {
    assert!(properties("newmtl a\nKd 1 1 1\nmap_Kd #x\nnewmtl b\nKd 1.0 1.0 1.0 1.0\n").is_empty());
    assert!(properties("Kd 0.5 0.5 0.5\nneoforge_TintIndex 0\n").is_empty());
    for file in [
        "mods/mekanism/content/assets/mekanism/models/entity/mekasuit.mtl",
        "mods/mekanism/content/assets/mekanism/models/entity/mekatool_left.mtl",
        "mods/mekanism/content/assets/mekanism/models/entity/mekatool_right.mtl",
        "mods/mekanismgenerators/content/assets/mekanismgenerators/models/block/wind_generator.mtl",
        "mods/mekanismgenerators/content/assets/mekanismgenerators/models/block/wind_generator_blades.mtl",
    ] {
        let text = std::fs::read_to_string(root().join(file)).unwrap();
        assert!(properties(&text).is_empty(), "{file}");
    }
}

/// A entrada extra do mapa de `parse_mtl` (chave e valor) volta inteira, e um valor que não é de um material extra não decodifica.
#[test]
fn the_extra_entry_round_trips() {
    for properties in [
        (Some(0xFFCC_CCCCu32 as i32), NO_TINT),
        (Some(0x0000_0001), 0),
        (None, 5),
        (Some(i32::MIN), 12),
    ] {
        let properties: Properties = properties;
        assert_eq!(decode(&encode(properties)), Some(properties));
    }
    assert_eq!(encode((Some(0xFFCC_CCCCu32 as i32), -1)), "ffcccccc|-1");
    assert_eq!(encode((None, 0)), "|0");
    assert_eq!(decode("side"), None);
    assert_eq!(decode("zz|1"), None);
    assert_eq!(decode("ffffffff|x"), None);
    assert!(key("None").starts_with(material::PREFIX));
    assert_ne!(key("None"), "None");
}
