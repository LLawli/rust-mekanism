//! O contorno do Configurator num braço de transmissor (`BlockTransmitter.getShape` devolve a forma da peça, e o jogo desenha as arestas dela): o braço `PUSH` e o `PULL` são três
//! caixas empilhadas (`BlockSmallTransmitter.SIDES_PUSH` e `SIDES_PULL`, e as do `BlockLargeTransmitter`), e as arestas são as da união, com os degraus (decisão 166, H179). Contra o
//! original: `rastros/transmitter_shape_edges.jsonl` (`testes/TransmitterShapeTraceTest.java`, `VoxelShape.forAllEdges` de cada braço).
use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::connection::ConnectionType;
use mekanism_rules::transmitter_pieces;
use serde_json::Value;
use std::path::{Path, PathBuf};

type Edge = [f64; 6];

fn trace() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/transmitter_shape_edges.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", Path::new(&path).display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn six(value: &Value) -> Edge {
    std::array::from_fn(|index| value[index].as_f64().unwrap())
}

fn face(name: &str) -> Face {
    match name {
        "down" => Face::Down,
        "up" => Face::Up,
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        _ => Face::East,
    }
}

fn kind(name: &str) -> ConnectionType {
    match name {
        "PUSH" => ConnectionType::Push,
        "PULL" => ConnectionType::Pull,
        _ => ConnectionType::Normal,
    }
}

fn sorted(mut edges: Vec<Edge>) -> Vec<Edge> {
    edges.sort_by(|a, b| {
        a.iter()
            .zip(b)
            .map(|(x, y)| x.total_cmp(y))
            .find(|order| order.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    edges
}

/// As 12 arestas de uma caixa, na ordem de `cube_edges()` do Horizon escaladas por ela (o contorno de uma peça de uma caixa só).
fn box_edges(bounds: [f64; 6]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for axis in 0..3 {
        for (u, v) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let mut from = [0usize; 3];
            from[(axis + 1) % 3] = u;
            from[(axis + 2) % 3] = v;
            let mut to = from;
            to[axis] = 1;
            let point = |end: [usize; 3]| -> [f64; 3] {
                std::array::from_fn(|index| bounds[index + 3 * end[index]])
            };
            let (a, b) = (point(from), point(to));
            edges.push([a[0], a[1], a[2], b[0], b[1], b[2]]);
        }
    }
    edges
}

/// As arestas da peça como o laço do jogo as desenha: o que o RETURN de `cube_edges` devolve (as armadas, em fração) escalado pela caixa do `context_outline`.
fn drawn(piece: &mekanism_rules::TransmitterPiece) -> Vec<Edge> {
    let bounds = piece.bounds();
    match piece.outline_edges() {
        Some(fractions) => fractions
            .iter()
            .map(|edge| {
                std::array::from_fn(|index| {
                    let axis = index % 3;
                    bounds[axis] + edge[index] * (bounds[axis + 3] - bounds[axis])
                })
            })
            .collect(),
        None => box_edges(bounds),
    }
}

fn close(a: &[Edge], b: &[Edge]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() < 1e-12))
}

#[test]
fn the_outline_of_every_arm_is_the_edges_the_original_draws() {
    let mut arms = 0;
    for row in trace() {
        let Some(side) = row["side"].as_str() else {
            continue;
        };
        let block = if row["large"].as_bool().unwrap() {
            "mekanism:basic_mechanical_pipe"
        } else {
            "mekanism:basic_universal_cable"
        };
        let mut kinds = [ConnectionType::None; 6];
        kinds[face(side) as usize] = kind(row["type"].as_str().unwrap());
        let piece = &transmitter_pieces(block, &kinds).unwrap()[0];
        let want: Vec<Edge> = row["edges"].as_array().unwrap().iter().map(six).collect();
        let got = drawn(piece);
        assert!(close(&sorted(got), &sorted(want)), "{row}");
        arms += 1;
    }
    assert_eq!(arms, 2 * 3 * 6);
}

#[test]
fn only_push_and_pull_arms_draw_their_own_edges() {
    // O braço normal e o centro são uma caixa só: o cubo unitário escalado pela caixa já é o contorno deles.
    for row in trace() {
        let Some(side) = row["side"].as_str() else {
            continue;
        };
        let mut kinds = [ConnectionType::None; 6];
        kinds[face(side) as usize] = kind(row["type"].as_str().unwrap());
        let piece = &transmitter_pieces("mekanism:basic_universal_cable", &kinds).unwrap()[0];
        assert_eq!(
            piece.outline_edges().is_some(),
            row["type"] != "NORMAL",
            "{row}"
        );
    }
    let center =
        transmitter_pieces("mekanism:basic_universal_cable", &[ConnectionType::None; 6]).unwrap();
    assert_eq!(center.last().unwrap().outline_edges(), None);
}

#[test]
fn the_pull_outline_is_larger_than_the_box_that_contains_it_by_the_steps() {
    // Um `PULL` pequeno para o leste: as três caixas têm meias larguras 3, 2 e 4 pixels, então o contorno da união tem arestas dentro da caixa que as contém (os degraus).
    let mut kinds = [ConnectionType::None; 6];
    kinds[Face::East as usize] = ConnectionType::Pull;
    let piece = &transmitter_pieces("mekanism:basic_universal_cable", &kinds).unwrap()[0];
    let edges = drawn(piece);
    assert_eq!(edges.len(), 36);
    let bounds = piece.bounds();
    assert!(
        edges
            .iter()
            .any(|edge| edge[0] > bounds[0] && edge[0] < bounds[3])
    );
}
