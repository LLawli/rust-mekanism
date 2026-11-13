//! Os vértices da sobreposição do Configurator contra o original: `dev/referencia/rastros/desviador_sobreposicao.jsonl`
//! (`testes/DesviadorSobreposicaoRastro.java`, o `render` de `ConfiguratorOverlayHandler` com o `PoseStack`, o `FaceInfo` e o JOML reais do Minecraft 26.3). Cada linha é uma
//! face, um `yRot` da câmera e se é a do desviador (`scale(0.5F)` e `translate(0.5, 0.5, 0.5)`); o teste confere os quatro vértices (posição e uv) em float, a rotação das faces
//! de cima e de baixo incluída, e a cor do desviador (`ARGB.white(0.8F)`).
use machinery_basis_core::face_overlay::{diversion_vertices, vertices};
use machinery_basis_core::sides::Face;
use serde_json::Value;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/desviador_sobreposicao.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::ALL
        .into_iter()
        .find(|face| face.name() == name)
        .unwrap_or_else(|| panic!("face {name}"))
}

/// A diferença máxima aceita por coordenada: o JOML multiplica a matriz inteira em float (`m00 * x + m10 * y + m20 * z + m30`), e a lib faz a conta por eixo, então um ou dois
/// ulps de um valor perto de 1 podem sobrar.
const TOLERANCE: f32 = 2.0e-7;

#[test]
fn the_vertices_of_every_face_and_camera_are_the_originals() {
    let mut checked = 0;
    let mut worst = 0.0_f32;
    for row in rows().iter().filter(|row| row.get("meta").is_none()) {
        let which = face(row["face"].as_str().unwrap());
        let y_rot = row["y_rot"].as_f64().unwrap();
        let mine = if row["diversion"].as_bool().unwrap() {
            diversion_vertices(which, y_rot)
        } else {
            vertices(which, y_rot)
        };
        let theirs = row["vertices"].as_array().unwrap();
        assert_eq!(theirs.len(), 4);
        for (index, vertex) in theirs.iter().enumerate() {
            for axis in 0..3 {
                let expected = vertex["p"][axis].as_f64().unwrap() as f32;
                let diff = (mine[index].position[axis] - expected).abs();
                worst = worst.max(diff);
                assert!(
                    diff <= TOLERANCE,
                    "{row}: vértice {index} eixo {axis}: {} contra {expected}",
                    mine[index].position[axis]
                );
            }
            for axis in 0..2 {
                assert_eq!(
                    mine[index].uv[axis],
                    vertex["uv"][axis].as_f64().unwrap() as f32,
                    "{row}: uv {index}"
                );
            }
        }
        checked += 1;
    }
    // 6 faces, 11 câmeras, os dois desenhos.
    assert_eq!(checked, 6 * 11 * 2);
    eprintln!("maior diferença contra o original: {worst:e}");
}

#[test]
fn the_diversion_color_is_white_at_eighty_percent() {
    let meta = &rows()[0];
    let argb = meta["meta"]
        .as_str()
        .unwrap()
        .rsplit("diversion_argb=")
        .next()
        .unwrap()
        .parse::<i32>()
        .unwrap() as u32;
    assert_eq!(argb, 0xCCFF_FFFF);
}
