//! `BlockEnergyCube.getShape` do cubo posto, contra o rastro `energy_cube_shapes.jsonl` do original (`EnergyCubeShapeTraceTest`): para cada um dos seis `facing` e cada uma das 64
//! combinações de lados relativos habilitados, o conjunto de pontos da seleção e da colisão tem de ser o mesmo. O original funde as caixas (`VoxelShape.toAabbs`) e a regra do port
//! as deixa soltas, então a comparação é por pontos, nas células da grade formada por todas as fronteiras dos dois lados.
use machinery_basis_core::side_config::RelativeSide;
use machinery_basis_core::sides::Face;
use mekanism_rules::cube_shape;
use serde_json::Value;
use std::path::Path;

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap()
}

fn boxes(value: &Value) -> Vec<[f64; 6]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let b = b.as_array().unwrap();
            std::array::from_fn(|i| b[i].as_f64().unwrap())
        })
        .collect()
}

fn same_points(a: &[[f64; 6]], b: &[[f64; 6]]) -> bool {
    let cuts = |axis: usize| {
        let mut c: Vec<f64> = a
            .iter()
            .chain(b)
            .flat_map(|x| [x[axis], x[axis + 3]])
            .collect();
        c.sort_by(|p, q| p.partial_cmp(q).unwrap());
        c.dedup_by(|p, q| (*p - *q).abs() < 1e-7);
        c
    };
    let (xs, ys, zs) = (cuts(0), cuts(1), cuts(2));
    let inside = |set: &[[f64; 6]], p: [f64; 3]| {
        set.iter()
            .any(|x| (0..3).all(|i| p[i] > x[i] + 1e-7 && p[i] < x[i + 3] - 1e-7))
    };
    for ix in 0..xs.len() - 1 {
        for iy in 0..ys.len() - 1 {
            for iz in 0..zs.len() - 1 {
                let p = [
                    (xs[ix] + xs[ix + 1]) / 2.0,
                    (ys[iy] + ys[iy + 1]) / 2.0,
                    (zs[iz] + zs[iz + 1]) / 2.0,
                ];
                if inside(a, p) != inside(b, p) {
                    return false;
                }
            }
        }
    }
    true
}

#[test]
fn the_placed_shape_follows_the_original_for_every_facing_and_side_set() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/energy_cube_shapes.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let mut cases = 0;
    for line in text.lines().skip(1) {
        let case: Value = serde_json::from_str(line).unwrap();
        let facing = face(case["facing"].as_str().unwrap());
        let mask = case["mask"].as_u64().unwrap();
        let enabled = cube_shape::absolute_enabled(facing, |side| {
            mask & (1
                << RelativeSide::VALUES
                    .iter()
                    .position(|s| *s == side)
                    .unwrap())
                != 0
        });
        let ours = cube_shape::boxes(facing, enabled);
        assert!(
            same_points(&ours, &boxes(&case["outline"])),
            "seleção {facing:?} máscara {mask}"
        );
        assert!(
            same_points(&ours, &boxes(&case["collision"])),
            "colisão {facing:?} máscara {mask}"
        );
        cases += 1;
    }
    assert_eq!(cases, 384);
}
