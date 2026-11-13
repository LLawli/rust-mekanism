//! As arestas do contorno de uma união de caixas (`machinery_basis_core::voxel_edges`, `VoxelShape.forAllEdges`), contra o original: `rastros/transmitter_shape_edges.jsonl`
//! (`testes/TransmitterShapeTraceTest.java`, as formas dos braços dos transmissores do Mekanism no `runGameTestServer`) traz, para cada braço (pequeno e grande, `NORMAL`, `PUSH` e
//! `PULL`, nos seis lados) e para o centro, as caixas de `toAabbs` e as arestas de `forAllEdges`, na ordem do jogo.
use machinery_basis_core::voxel_edges::{Edge, union_edges, union_edges_in};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn six(value: &Value) -> [f64; 6] {
    std::array::from_fn(|index| value[index].as_f64().unwrap())
}

fn trace() -> Vec<Value> {
    let path = root().join("dev/referencia/rastros/transmitter_shape_edges.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
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

#[test]
fn the_trace_has_every_arm_shape() {
    // 2 tamanhos, 1 centro + 3 tipos por 6 lados.
    assert_eq!(trace().len(), 2 * (1 + 3 * 6));
}

#[test]
fn every_shape_of_the_original_gives_the_same_edges_in_the_same_order() {
    for row in trace() {
        let boxes: Vec<[f64; 6]> = row["boxes"].as_array().unwrap().iter().map(six).collect();
        let want: Vec<Edge> = row["edges"].as_array().unwrap().iter().map(six).collect();
        assert_eq!(union_edges(&boxes), want, "{}", row);
    }
}

#[test]
fn the_push_and_pull_arms_are_not_the_box_that_contains_them() {
    // O contorno de um `PULL` tem os degraus das três caixas: mais arestas que a caixa que as contém (12).
    let mut checked = 0;
    for row in trace() {
        if row["type"] == "PUSH" || row["type"] == "PULL" {
            let edges = row["edges"].as_array().unwrap().len();
            assert!(edges > 12, "{row}");
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * 2 * 6);
}

#[test]
fn one_box_is_the_twelve_edges_of_the_cube() {
    let edges = union_edges(&[[0.25, 0.5, 0.125, 0.75, 1.0, 0.375]]);
    assert_eq!(edges.len(), 12);
    let want = sorted(vec![
        [0.25, 0.5, 0.125, 0.25, 0.5, 0.375],
        [0.25, 1.0, 0.125, 0.25, 1.0, 0.375],
        [0.75, 0.5, 0.125, 0.75, 0.5, 0.375],
        [0.75, 1.0, 0.125, 0.75, 1.0, 0.375],
        [0.25, 0.5, 0.125, 0.25, 1.0, 0.125],
        [0.75, 0.5, 0.125, 0.75, 1.0, 0.125],
        [0.25, 0.5, 0.375, 0.25, 1.0, 0.375],
        [0.75, 0.5, 0.375, 0.75, 1.0, 0.375],
        [0.25, 0.5, 0.125, 0.75, 0.5, 0.125],
        [0.25, 0.5, 0.375, 0.75, 0.5, 0.375],
        [0.25, 1.0, 0.125, 0.75, 1.0, 0.125],
        [0.25, 1.0, 0.375, 0.75, 1.0, 0.375],
    ]);
    assert_eq!(sorted(edges), want);
}

#[test]
fn an_l_shaped_prism_has_eighteen_edges_and_the_inner_corner() {
    // Dois blocos de um L no plano xy: 6 vértices no contorno, 6 arestas de cima, 6 de baixo e 6 de lado.
    let edges = union_edges(&[
        [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
        [0.0, 0.5, 0.0, 0.5, 1.0, 1.0],
    ]);
    assert_eq!(edges.len(), 18);
    // A aresta do canto de dentro (x 0,5, y 0,5) anda em z de 0 a 1.
    assert!(edges.contains(&[0.5, 0.5, 0.0, 0.5, 0.5, 1.0]));
    // E o plano de y = 0,5 que divide as duas caixas não corta a aresta vertical de x = 0 (um segmento só, de 0 a 1).
    assert!(edges.contains(&[0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
}

#[test]
fn two_boxes_that_touch_only_at_an_edge_keep_the_diagonal_cells() {
    // Duas células em cantos opostos (`fullCount == 2` com `oddCount` par): a reta do canto comum é aresta.
    let edges = union_edges(&[
        [0.0, 0.0, 0.0, 0.5, 0.5, 1.0],
        [0.5, 0.5, 0.0, 1.0, 1.0, 1.0],
    ]);
    assert!(edges.contains(&[0.5, 0.5, 0.0, 0.5, 0.5, 1.0]));
    // 12 + 12 arestas, a do canto comum conta uma vez e as quatro que se encontram nele (duas retas por face, x e y, de cima e de baixo) se juntam duas a duas: 24 - 1 - 4.
    assert_eq!(edges.len(), 19);
}

#[test]
fn no_boxes_have_no_edges() {
    assert!(union_edges(&[]).is_empty());
}

#[test]
fn the_edges_in_a_box_are_fractions_of_it() {
    let boxes = [
        [0.25, 0.0, 0.25, 0.75, 0.25, 0.75],
        [0.375, 0.25, 0.375, 0.625, 0.5, 0.625],
    ];
    let bounds = [0.25, 0.0, 0.25, 0.75, 0.5, 0.75];
    let fractions = union_edges_in(&boxes, &bounds);
    let world = union_edges(&boxes);
    assert_eq!(fractions.len(), world.len());
    for (fraction, edge) in fractions.iter().zip(&world) {
        for index in 0..6 {
            let axis = index % 3;
            let back = bounds[axis] + fraction[index] * (bounds[axis + 3] - bounds[axis]);
            assert!((back - edge[index]).abs() < 1e-12, "{fraction:?} {edge:?}");
        }
    }
    // Os extremos da caixa que contém tudo são 0 e 1.
    assert!(
        fractions
            .iter()
            .flatten()
            .all(|value| (0.0..=1.0).contains(value))
    );
    assert!(
        fractions.iter().any(|edge| edge[0] == 0.0) && fractions.iter().any(|edge| edge[3] == 1.0)
    );
}
