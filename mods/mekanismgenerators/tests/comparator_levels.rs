//! Repete, tick a tick, o rastro `comparator_levels_heat_generator.jsonl` do Heat Generator original (`dev/referencia/testes/ComparatorTraceTest.java`): o
//! comparador lê só o tanque de lava (`TileEntityHeatGenerator.getRedstoneLevel` -> `getRedstoneSignalFromContainer`, `:233-235`), o nível é guardado no
//! fim do tick seguinte à mudança (`TileEntityMekanism.tickServer`, `:645-652`) e só quando o tanque mudou (`makesComparatorDirty`, `:237-239`).
use machinery_basis_core::containers::redstone_signal_of_container;
use machinery_basis_core::tile::RedstoneControl;
use mekanismgenerators_rules::{State, TANK_CAPACITY, finish_tick};
use serde_json::Value;
use std::path::Path;

fn trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/comparator_levels_heat_generator.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O nível que o Java lê do tanque na hora (`getRedstoneLevel`).
fn live(state: &State) -> i64 {
    i64::from(redstone_signal_of_container(state.lava, TANK_CAPACITY))
}

fn replay() -> Vec<String> {
    let lines = trace();
    let mut state = State::new(20.0);
    let mut seen = None;
    let mut wrong = Vec::new();
    for line in lines.iter().filter(|line| line.get("ops").is_some()) {
        let t = line["t"].as_u64().unwrap();
        // O tick do tile (os block entities) vem antes das operações do passo do gameTest.
        let lava_before = state.lava;
        finish_tick(&mut state, lava_before, &mut seen);
        for op in line["ops"].as_array().unwrap() {
            let text = op["op"].as_str().unwrap();
            let words: Vec<&str> = text.split(' ').collect();
            match words[0] {
                "put" => state = State::new(20.0),
                "mode" => state
                    .tile
                    .set_control_type(RedstoneControl::from_serialized_name(
                        &words[1].to_lowercase(),
                    )),
                "lava" => state.lava = words[1].parse().unwrap(),
                "slots" | "mark" => {}
                other => panic!("operação desconhecida {other:?}"),
            }
        }
        let want = &line["rows"][0];
        if want.is_null() {
            continue;
        }
        let got = (live(&state), i64::from(state.tile.current_redstone_level()));
        let expected = (
            want["level"].as_i64().unwrap(),
            want["current"].as_i64().unwrap(),
        );
        if got != expected {
            wrong.push(format!(
                "t={t}: (nível, guardado) {got:?}, o rastro diz {expected:?}"
            ));
        }
    }
    wrong
}

#[test]
fn the_comparator_of_the_heat_generator_reads_only_the_lava_tank() {
    let wrong = replay();
    assert!(
        wrong.is_empty(),
        "{} diferenças\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_lava_levels_match_the_marks_of_the_original() {
    let marks: Vec<Value> = trace()
        .into_iter()
        .filter(|line| line.get("mark").is_some())
        .collect();
    assert!(marks.len() >= 8, "marcas: {}", marks.len());
    for mark in marks {
        let lava = mark["lava"].as_u64().unwrap();
        assert_eq!(
            mark["lava_capacity"].as_u64().unwrap(),
            TANK_CAPACITY,
            "{}",
            mark["mark"]
        );
        assert_eq!(
            i64::from(redstone_signal_of_container(lava, TANK_CAPACITY)),
            mark["level"].as_i64().unwrap(),
            "{}: lava {lava}",
            mark["mark"]
        );
    }
}
