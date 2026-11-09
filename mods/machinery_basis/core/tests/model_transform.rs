//! O módulo `model-transform` da `machinery_basis` (`mods/machinery_basis/model_transform.rs`), que o launcher enxerta em `math.rs` do `horizon-assets`, compilado aqui como
//! é: ele só usa `std` e `serde_json`. É o `transform` de modelo do NeoForge (`TransformationHelper.Deserializer`, `composeRootTransformIntoModelState`), conferido contra o
//! original: `rastros/model_transform.jsonl` (`testes/ModelTransformTraceTest.java`, as classes do jogo no `runGameTestServer`) traz, para cada JSON e cada rotação de blockstate,
//! a matriz que o NeoForge lê, o estado composto, a posição de pontos de amostra como o `FaceBakery` os põe e a direção de cada face. Os dois modelos do original que usam o
//! `transform` (o `wind_generator` e o `advanced_solar_generator`) são lidos do conteúdo do port.
#[allow(dead_code)]
mod machinery_model_transform {
    include!("../../model_transform.rs");
}

use machinery_model_transform as transform;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn rows(value: &Value) -> transform::Rows {
    std::array::from_fn(|r| std::array::from_fn(|c| value[r][c].as_f64().unwrap() as f32))
}

fn trace() -> Vec<Value> {
    let path = root().join("dev/referencia/rastros/model_transform.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance * (1.0 + b.abs())
}

/// A rotação de blockstate do rastro é uma permutação com sinais: a que o Horizon guarda (`BlockModelTransform::matrix_rows`).
fn blockstate(parent: &Value) -> transform::Rows {
    let java = rows(parent);
    for row in &java {
        for value in row {
            assert!(
                [-1.0, 0.0, 1.0].contains(value),
                "a rotação do blockstate não é exata: {parent}"
            );
        }
    }
    java
}

#[test]
fn the_trace_has_every_case_and_every_parent() {
    let rows = trace();
    let errors = rows.iter().filter(|r| r["error"] == true).count();
    assert_eq!(errors, 6, "os JSON que o NeoForge recusa");
    assert_eq!(
        rows.len(),
        6 + 14 * 6,
        "14 transformações em 6 rotações de blockstate"
    );
}

#[test]
fn the_json_reads_to_the_matrix_the_original_reads() {
    let mut checked = 0;
    for row in trace().iter().filter(|r| r["error"] != true) {
        let json = row["json"].as_str().unwrap();
        let value: Value = serde_json::from_str(json).unwrap();
        let ours = transform::parse(&value).unwrap_or_else(|e| panic!("{json}: {e}"));
        let java = rows(&row["root"]);
        // Só translação, escala e matriz dão a mesma conta bit a bit; as rotações trazem o `sin` e o `cos` do JOML (aqui o `f64`), a menos de 1e-6.
        let exact = value
            .as_object()
            .is_none_or(|m| !m.keys().any(|k| k.contains("rotation")));
        for r in 0..3 {
            for c in 0..4 {
                if exact {
                    assert_eq!(
                        ours[r][c].to_bits(),
                        java[r][c].to_bits(),
                        "{json} [{r}][{c}]: {} contra {}",
                        ours[r][c],
                        java[r][c]
                    );
                } else {
                    assert!(
                        close(ours[r][c], java[r][c], 1e-6),
                        "{json} [{r}][{c}]: {} contra {}",
                        ours[r][c],
                        java[r][c]
                    );
                }
            }
        }
        let entered = transform::enter(&encode_for_test(&ours));
        assert_eq!(
            entered.is_none(),
            row["identity"] == true,
            "{json}: identidade"
        );
        if entered.is_some() {
            transform::leave();
        }
        checked += 1;
    }
    assert_eq!(checked, 14 * 6);
}

/// O que `slot_sprite` guarda para uma matriz, para pôr `enter` à prova sem o JSON.
fn encode_for_test(m: &transform::Rows) -> String {
    let mut out = String::from("neoforge_transform:7f");
    for row in m {
        for value in row {
            out.push_str(&format!("_{:08x}", value.to_bits()));
        }
    }
    out
}

#[test]
fn what_the_original_refuses_is_refused() {
    let mut refused = 0;
    for row in trace().iter().filter(|r| r["error"] == true) {
        let json = row["json"].as_str().unwrap();
        let value: Value = serde_json::from_str(json).unwrap();
        assert!(transform::parse(&value).is_err(), "{json}");
        refused += 1;
    }
    assert_eq!(refused, 6);
}

#[test]
fn the_baked_vertices_and_directions_are_the_originals() {
    let tolerance = 2e-5;
    let mut points = 0;
    let mut directions = 0;
    for row in trace().iter().filter(|r| r["error"] != true) {
        let json = row["json"].as_str().unwrap();
        let value: Value = serde_json::from_str(json).unwrap();
        let root = transform::parse(&value).unwrap();
        let r = blockstate(&row["parent"]);
        // O que `bake_into` faz: o Horizon gira pelo blockstate, e a injeção move pelo modelo; a identidade não entra (o NeoForge devolve o estado como está).
        let placement = if root == transform::IDENTITY {
            transform::IDENTITY
        } else {
            transform::placement(&r, &transform::apply_origin(&root, [-0.5; 3]))
        };
        for point in row["points"].as_array().unwrap() {
            let p: Vec<f32> = point
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            let rotated = transform::position(&r, [p[0], p[1], p[2]]);
            let got = transform::position(&placement, rotated);
            for axis in 0..3 {
                assert!(
                    close(got[axis], p[3 + axis], tolerance),
                    "{json} y/{}: {:?} -> {:?}, o original {:?}",
                    row["parent"],
                    &p[..3],
                    got,
                    &p[3..]
                );
            }
            points += 1;
        }
        let names = ["down", "up", "north", "south", "west", "east"];
        for (index, expected) in row["directions"].as_array().unwrap().iter().enumerate() {
            let rotated = transform::rotate_direction(&r, index);
            let got = transform::rotate_direction(&placement, rotated);
            let expected = names
                .iter()
                .position(|n| Some(*n) == expected.as_str())
                .unwrap();
            // Num empate (45 graus) o ruído do ponto flutuante decide; o resto tem de ser a direção do original.
            let steps = [
                [0.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, -1.0],
                [0.0, 0.0, 1.0],
                [-1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
            ];
            let vector: [f32; 3] = std::array::from_fn(|axis| {
                (0..3).map(|k| placement[axis][k] * steps[rotated][k]).sum()
            });
            let dot = |d: usize| {
                vector[0] * steps[d][0] + vector[1] * steps[d][1] + vector[2] * steps[d][2]
            };
            assert!(
                got == expected || close(dot(got), dot(expected), 1e-4),
                "{json}: direção {index} deu {got}, o original {expected}"
            );
            directions += 1;
        }
    }
    assert_eq!(points, 14 * 6 * 10);
    assert_eq!(directions, 14 * 6 * 6);
}

#[test]
fn a_translation_only_matrix_moves_and_keeps_the_directions() {
    let m = transform::parse(&json!({"translation": [0, 1, 0]})).unwrap();
    let placement = transform::placement(
        &transform::IDENTITY,
        &transform::apply_origin(&m, [-0.5; 3]),
    );
    assert!(transform::moves_only(&placement));
    assert_eq!(
        transform::position(&placement, [0.5, 0.1, 0.5]),
        [0.5, 1.1, 0.5]
    );
    let rotated = transform::parse(&json!({"rotation": [0, 90, 0]})).unwrap();
    assert!(!transform::moves_only(&transform::apply_origin(
        &rotated, [-0.5; 3]
    )));
}

#[test]
fn the_models_of_the_original_that_use_the_transform_read_as_the_original_reads_them() {
    let models =
        root().join("mods/mekanismgenerators/content/assets/mekanismgenerators/models/block");
    // `advanced_solar_generator`: o modelo inteiro sobe um bloco ("the model is at Y-1 to fit inside vanilla size restrictions").
    let solar: Value = serde_json::from_str(
        &std::fs::read_to_string(models.join("advanced_solar_generator.json")).unwrap(),
    )
    .unwrap();
    let m = transform::parse(&solar["transform"]).unwrap();
    let placement = transform::placement(
        &transform::IDENTITY,
        &transform::apply_origin(&m, [-0.5; 3]),
    );
    assert_eq!(
        transform::position(&placement, [0.375, 0.25, 0.625]),
        [0.375, 1.25, 0.625]
    );
    // `wind_generator`: o `.obj` do filho `main` está centrado na origem e a translação o põe no bloco ("so that the item rotates appropriately on the ground").
    let wind: Value =
        serde_json::from_str(&std::fs::read_to_string(models.join("wind_generator.json")).unwrap())
            .unwrap();
    assert_eq!(wind["loader"], "neoforge:composite");
    assert!(wind["transform"].is_null(), "só o filho leva o transform");
    let m = transform::parse(&wind["children"]["main"]["transform"]).unwrap();
    let placement = transform::placement(
        &transform::IDENTITY,
        &transform::apply_origin(&m, [-0.5; 3]),
    );
    assert_eq!(
        transform::position(&placement, [-0.5, 0.0, -0.5]),
        [0.0, 0.0, 0.0]
    );
    assert_eq!(
        transform::position(&placement, [0.5, 1.0, 0.5]),
        [1.0, 1.0, 1.0]
    );
    // Com o blockstate girado (`y: 90`, a leste) a torre continua no centro do bloco: a translação gira com o modelo, em volta do centro.
    let turned: transform::Rows = [
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
    ];
    let placement = transform::placement(&turned, &transform::apply_origin(&m, [-0.5; 3]));
    let rotated = transform::position(&turned, [0.0, 0.5, 0.0]);
    assert_eq!(
        transform::position(&placement, rotated),
        transform::position(&turned, [0.5, 0.5, 0.5])
    );
}

#[test]
fn the_slot_carries_the_matrix_and_a_unique_id() {
    let text = r#"{"parent":"block/block","transform":{"translation":[0.5,0,0.5]},"textures":{}}"#;
    assert!(transform::mentions(text));
    assert!(!transform::mentions(r#"{"parent":"block/cube"}"#));
    let first = transform::slot_sprite(text).unwrap().unwrap();
    let second = transform::slot_sprite(text).unwrap().unwrap();
    assert_ne!(first, second, "cada modelo decodificado leva o seu id");
    let (id, m) = transform::decode_sprite(&first).unwrap();
    assert_eq!(
        m,
        transform::parse(&json!({"translation": [0.5, 0, 0.5]})).unwrap()
    );
    assert_ne!(id, transform::decode_sprite(&second).unwrap().0);
    assert_eq!(
        transform::slot_sprite(r#"{"textures":{"transform":"x"}}"#),
        Ok(None)
    );
    assert!(transform::slot_sprite(r#"{"transform":"foo"}"#).is_err());
    assert!(transform::decode_sprite("minecraft:block/stone").is_none());
    assert!(transform::decode_sprite("neoforge_transform:1_00").is_none());
}

#[test]
fn a_child_that_only_inherits_the_composites_transform_does_not_apply_it_again() {
    let composite = transform::slot_sprite(r#"{"transform":{"translation":[1,0,0]}}"#)
        .unwrap()
        .unwrap();
    let own = transform::slot_sprite(r#"{"transform":{"translation":[0,0,1]}}"#)
        .unwrap()
        .unwrap();
    let identity = transform::slot_sprite(r#"{"transform":"identity"}"#)
        .unwrap()
        .unwrap();
    assert!(transform::enter(&composite).is_some());
    // O filho que herdou o espaço do `composite` traz o mesmo id: nada a aplicar.
    assert!(transform::enter(&composite).is_none());
    // O filho com o próprio `transform` aplica o seu por dentro do do `composite`.
    assert!(transform::enter(&own).is_some());
    assert!(transform::enter(&own).is_none());
    transform::leave();
    assert!(
        transform::enter(&composite).is_none(),
        "de volta ao `composite`"
    );
    // A identidade não aplica nada nem entra na pilha.
    assert!(transform::enter(&identity).is_none());
    transform::leave();
    assert!(
        transform::enter(&composite).is_some(),
        "fora do `composite`, a pilha está limpa"
    );
    transform::leave();
}

#[test]
fn the_reentry_mark_lets_the_original_body_through_once() {
    assert!(!transform::bake_reentered(7));
    transform::bake_mark(7);
    assert!(!transform::bake_reentered(8));
    assert!(transform::bake_reentered(7));
    assert!(!transform::bake_reentered(7));
}

#[test]
fn the_direction_search_is_javas_get_approximate_nearest() {
    assert_eq!(
        transform::nearest([0.0, 0.0, 0.0]),
        2,
        "sem produto positivo, norte"
    );
    assert_eq!(transform::nearest([0.0, -3.0, 1.0]), 0);
    assert_eq!(
        transform::nearest([1.0, 1.0, 0.0]),
        1,
        "empate: a primeira da ordem de `Direction.values()`"
    );
    assert_eq!(transform::nearest([0.2, 0.1, 0.3]), 3);
    assert_eq!(transform::nearest([-2.0, 0.0, 0.0]), 4);
}
