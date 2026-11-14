//! A forma real de um transmissor (`BlockTransmitter.getRealShape`: a de seleção sem o Configurator, a de colisão e a visual) para TODA combinação de tipos de conexão dos seis lados,
//! o pequeno e o grande (4^6 = 4096 cada), contra o original: `rastros/transmitter_shape_all.jsonl` (`testes/TransmitterShapeAllTraceTest.java`, `VoxelShapeUtils.batchCombine` do centro
//! e dos braços de `getSideForType`). Cada linha traz a ocupação da forma numa grade de 16 por 16 por 16 pixels, o número de arestas do `forAllEdges` e o hash delas, na ordem do jogo;
//! as combinações só `NORMAL` e `NONE` trazem também as caixas e as arestas inteiras (decisão 209).
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::voxel_edges::union_edges;
use mekanism_rules::transmitter_shape;
use serde_json::Value;
use std::path::{Path, PathBuf};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn trace() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/transmitter_shape_all.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", Path::new(&path).display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Os tipos dos seis lados, na ordem de `Direction` (baixo, cima, norte, sul, oeste, leste): `N` normal, `P` push, `L` pull, `X` nenhum.
fn kinds(letters: &str) -> [ConnectionType; 6] {
    let mut kinds = [ConnectionType::None; 6];
    for (side, letter) in letters.chars().enumerate() {
        kinds[side] = match letter {
            'N' => ConnectionType::Normal,
            'P' => ConnectionType::Push,
            'L' => ConnectionType::Pull,
            _ => ConnectionType::None,
        };
    }
    kinds
}

fn block(large: bool) -> &'static str {
    if large {
        "mekanism:basic_mechanical_pipe"
    } else {
        "mekanism:basic_universal_cable"
    }
}

/// A ocupação da grade de 16 pixels (a célula `i` tem o centro em `(i % 16 + 0,5, i / 16 % 16 + 0,5, i / 256 + 0,5) / 16`, dentro de uma caixa com os limites incluídos), em FNV-1a de
/// 64 bits dos bytes de 8 células, a conta do rastro. Todas as caixas são de pixels inteiros, então o centro da célula `i` está na caixa de `de` a `ate` pixels quando `de <= i < ate`.
fn occupancy(boxes: &[[f64; 6]]) -> u64 {
    let pixels: Vec<[usize; 6]> = boxes
        .iter()
        .map(|bounds| {
            bounds.map(|value| {
                let scaled = value * 16.0;
                assert_eq!(scaled, scaled.round(), "uma caixa fora da grade de pixels");
                scaled as usize
            })
        })
        .collect();
    let mut hash = FNV_OFFSET;
    for cell in (0..4096usize).step_by(8) {
        let mut bits = 0u64;
        for bit in 0..8 {
            let index = cell + bit;
            let point = [index % 16, index / 16 % 16, index / 256];
            let inside = pixels
                .iter()
                .any(|b| (0..3).all(|axis| b[axis] <= point[axis] && point[axis] < b[axis + 3]));
            bits |= u64::from(inside) << bit;
        }
        hash = (hash ^ bits).wrapping_mul(FNV_PRIME);
    }
    hash
}

fn edge_hash(edges: &[[f64; 6]]) -> u64 {
    let mut hash = FNV_OFFSET;
    for edge in edges {
        for value in edge {
            hash = (hash ^ value.to_bits()).wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

fn six(value: &Value) -> [f64; 6] {
    std::array::from_fn(|index| value[index].as_f64().unwrap())
}

#[test]
fn the_real_shape_of_every_combination_is_the_original() {
    let lines = trace();
    assert_eq!(
        lines.len(),
        8192,
        "4096 combinações do pequeno e 4096 do grande"
    );
    let mut detailed = 0;
    for line in &lines {
        let large = line["large"].as_u64().unwrap() == 1;
        let letters = line["kinds"].as_str().unwrap();
        let piece = transmitter_shape(block(large), &kinds(letters)).unwrap();
        let at = format!("{} {letters}", if large { "grande" } else { "pequeno" });
        assert_eq!(
            format!("{:x}", occupancy(&piece.boxes)),
            line["occupancy"].as_str().unwrap(),
            "a ocupação de {at}"
        );
        let edges = union_edges(&piece.boxes);
        assert_eq!(
            edges.len() as u64,
            line["edges"].as_u64().unwrap(),
            "o número de arestas de {at}"
        );
        assert_eq!(
            format!("{:x}", edge_hash(&edges)),
            line["edge_hash"].as_str().unwrap(),
            "as arestas de {at}, na ordem do jogo"
        );
        if let Some(list) = line.get("edge_list") {
            detailed += 1;
            let expected: Vec<[f64; 6]> = list.as_array().unwrap().iter().map(six).collect();
            assert_eq!(edges, expected, "as arestas inteiras de {at}");
            let boxes: Vec<[f64; 6]> = line["box_list"]
                .as_array()
                .unwrap()
                .iter()
                .map(six)
                .collect();
            assert_eq!(
                occupancy(&boxes),
                occupancy(&piece.boxes),
                "as caixas inteiras de {at}"
            );
        }
    }
    assert!(
        detailed >= 2 * 64,
        "as combinações com as listas inteiras: {detailed}"
    );
}

#[test]
fn a_transmitter_without_arms_is_its_center() {
    for (large, low, high) in [(false, 5.0, 11.0), (true, 4.0, 12.0)] {
        let piece = transmitter_shape(block(large), &[ConnectionType::None; 6]).unwrap();
        assert_eq!(
            piece.boxes,
            vec![[low, low, low, high, high, high].map(|pixels| pixels / 16.0)]
        );
        assert!(piece.outline_edges().is_none());
    }
}

#[test]
fn only_a_transmitter_has_a_shape() {
    assert!(transmitter_shape("mekanism:steel_casing", &[ConnectionType::Normal; 6]).is_none());
}
