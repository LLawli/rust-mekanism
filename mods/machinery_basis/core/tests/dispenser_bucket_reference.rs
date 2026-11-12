//! O dispenser com um balde contra um bloco com `fluid_logged`, contra o original: `dev/referencia/rastros/dispenser_bucket.jsonl` (`testes/DispenserBucketTraceTest.java`) grava, para
//! seis blocos (o Heat Generator, o cubo de energia, a Enrichment Chamber, o cabo, o cano e a pedra), com o bloco vazio, com lava e com água dentro, o que o dispenser faz com o balde
//! de lava, o de água, o vazio, o de peixe e o de neve em pó: o que sobra no dispenser e o `fluid_logged` do bloco depois. Os valores que o bloco declara saem do conteúdo.

use machinery_basis_core::fluid_logged::{self, Dispensed};
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn rastro() -> Vec<Value> {
    let path = root().join("dev/referencia/rastros/dispenser_bucket.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Os valores de `fluid_logged` que o bloco do port declara, ou `None` quando ele não tem a propriedade.
fn declared(target: &str) -> Option<Vec<String>> {
    let (mod_id, block) = match target {
        "heat_generator" => ("mekanismgenerators", target),
        "stone" => return None,
        _ => ("mekanism", target),
    };
    let path = root().join(format!(
        "mods/{mod_id}/content/data/{mod_id}/horizon/block/{block}.json"
    ));
    let json: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).unwrap();
    let property = json.get("state_properties")?.get("fluid_logged")?;
    Some(
        property["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    )
}

fn contents(row: &Value) -> Vec<(String, u64)> {
    row["dispenser"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s[0].as_str().unwrap().to_owned(), s[1].as_u64().unwrap()))
        .collect()
}

/// O que cada balde faz contra cada bloco, em 90 casos, é o que o original faz: o fluido entra no bloco vazio que o declara e o balde volta vazio, o balde vazio tira o fluido de dentro
/// e o cheio vira no dispenser, e o resto solta o item sem mexer no bloco.
#[test]
fn a_dispenser_does_with_each_bucket_what_the_original_does() {
    let rows = rastro();
    assert_eq!(rows.len(), 90);
    let (mut filled, mut picked, mut dropped, mut passed) = (0, 0, 0, 0);
    for row in &rows {
        let target = row["target"].as_str().unwrap();
        let pre = row["pre"].as_str().unwrap();
        let item = row["item"].as_str().unwrap();
        let case = format!("{target} {pre} {item}");
        let declared = declared(target);
        let id = format!("minecraft:{item}");
        let current = declared.as_ref().map(|_| pre);
        if let Some(values) = &declared {
            assert!(
                values.iter().any(|v| v == pre),
                "{case}: o bloco não declara {pre}: {values:?}"
            );
        }
        let outcome = fluid_logged::dispense_bucket(&id, current, |value| {
            declared
                .as_ref()
                .is_some_and(|values| values.iter().any(|v| v == value))
        });
        let logged = row["logged"].as_str().unwrap();
        let held = contents(row);
        match outcome {
            Dispensed::Pass => {
                passed += 1;
                assert_eq!(
                    logged, "-",
                    "{case}: um bloco com a propriedade não pode ficar para o Horizon"
                );
            }
            Dispensed::Fill { value } => {
                filled += 1;
                assert_eq!(logged, value, "{case}");
                assert_eq!(held, vec![("bucket".to_owned(), 1)], "{case}");
            }
            Dispensed::Pickup { value, bucket } => {
                picked += 1;
                assert_eq!(logged, value, "{case}");
                let name = bucket.trim_start_matches("minecraft:");
                assert_eq!(
                    held,
                    vec![("bucket".to_owned(), 1), (name.to_owned(), 1)],
                    "{case}"
                );
            }
            Dispensed::Drop => {
                dropped += 1;
                assert_eq!(logged, pre, "{case}: o bloco não muda");
                let left = if item == "bucket" {
                    vec![("bucket".to_owned(), 1)]
                } else {
                    vec![]
                };
                assert_eq!(held, left, "{case}");
            }
        }
    }
    // Pedra e Enrichment Chamber não têm a propriedade (30 casos). Cada um dos outros quatro blocos enche com os baldes de lava, de água e de peixe quando vazio (3), tira o fluido
    // com o balde vazio quando tem lava ou água (2) e solta o item nos 10 casos restantes.
    assert_eq!((filled, picked, dropped, passed), (12, 8, 40, 30));
}

/// Só o balde de água e o de lava (e os de peixe, que guardam água) esvaziam num bloco com `fluid_logged`; o de neve em pó e o do cubo de enxofre não.
#[test]
fn only_a_bucket_with_water_or_lava_fills_a_block() {
    let all = |_: &str| true;
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:lava_bucket", Some("empty"), all),
        Dispensed::Fill { value: "lava" }
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:water_bucket", Some("empty"), all),
        Dispensed::Fill { value: "water" }
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:cod_bucket", Some("empty"), all),
        Dispensed::Fill { value: "water" }
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:powder_snow_bucket", Some("empty"), all),
        Dispensed::Drop
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:sulfur_cube_bucket", Some("empty"), all),
        Dispensed::Drop
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:stick", Some("empty"), all),
        Dispensed::Pass
    );
    assert_eq!(
        fluid_logged::dispense_bucket("mekanism:heavy_water_bucket", Some("empty"), all),
        Dispensed::Pass
    );
}

/// Um bloco sem a propriedade fica para o dispenser do Horizon, com qualquer balde.
#[test]
fn a_block_without_fluid_logged_is_left_to_the_dispenser_of_the_game() {
    for item in [
        "minecraft:lava_bucket",
        "minecraft:bucket",
        "minecraft:cod_bucket",
    ] {
        assert_eq!(
            fluid_logged::dispense_bucket(item, None, |_| true),
            Dispensed::Pass
        );
    }
}

/// `IStateFluidLoggable.isValidFluid`: um bloco que não declara o valor do fluido (o Heat Generator antigo, só `empty` e `lava`) não recebe o balde de água, que o dispenser solta.
#[test]
fn a_block_that_does_not_declare_the_fluid_gets_the_bucket_dropped() {
    let lava_only = |value: &str| value == "lava";
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:water_bucket", Some("empty"), lava_only),
        Dispensed::Drop
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:cod_bucket", Some("empty"), lava_only),
        Dispensed::Drop
    );
    assert_eq!(
        fluid_logged::dispense_bucket("minecraft:lava_bucket", Some("empty"), lava_only),
        Dispensed::Fill { value: "lava" }
    );
}
