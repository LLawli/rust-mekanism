//! A regra de corte de face entre o vidro estrutural, o vidro do reator e a matriz de foco do laser (`native/glass_skip_table.rs`, que o módulo `glass-skip` põe em `block_render.rs`) contra o
//! `skipRendering` do original: `rastros/blocos_oclusao_pular.jsonl` (`testes/OclusaoTraceTest.java`) lista todo par de blocos (estado padrão) do Mekanism e do Generators em que ele dá verdadeiro.
//! A tabela guarda o estado padrão de cada bloco em estáticos, então tudo roda num teste só.
#[path = "../native/glass_skip_table.rs"]
mod glass;

use serde_json::Value;
use std::collections::BTreeSet;

const STRUCTURAL: &str = "mekanism:structural_glass";
const REACTOR: &str = "mekanismgenerators:reactor_glass";
const MATRIX: &str = "mekanismgenerators:laser_focus_matrix";

fn rastro() -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/blocos_oclusao_pular.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_glass_blocks_skip_each_other_the_way_the_original_does() {
    let rows = rastro();
    let (counts, pairs): (Vec<&Value>, Vec<&Value>) =
        rows.iter().partition(|row| row.get("pairs").is_some());
    assert_eq!(counts.len(), 1);
    // Todo par examinado: o original tem pelo menos 180 blocos, então o teste não passa com um rastro vazio.
    assert!(counts[0]["blocks"].as_u64().unwrap() >= 180);
    assert_eq!(
        counts[0]["pairs"].as_u64().unwrap(),
        counts[0]["blocks"].as_u64().unwrap().pow(2)
    );
    let skipped: BTreeSet<(&str, &str)> = pairs
        .iter()
        .map(|row| (row["own"].as_str().unwrap(), row["other"].as_str().unwrap()))
        .collect();
    // O original só pula a face entre os três vidros e entre cada bloco e ele mesmo, se for do mesmo tipo de vidro (nenhum outro bloco tem `skipRendering`).
    assert!(
        skipped
            .iter()
            .all(|(a, b)| [STRUCTURAL, REACTOR, MATRIX].contains(a)
                && [STRUCTURAL, REACTOR, MATRIX].contains(b)),
        "{skipped:?}"
    );
    let ids = [
        (STRUCTURAL, 1),
        (REACTOR, 2),
        (MATRIX, 3),
        ("minecraft:stone", 4),
    ];
    let id = |name: &str| ids.iter().find(|(n, _)| *n == name).unwrap().1;
    glass::record(id(STRUCTURAL), id(REACTOR), id(MATRIX));
    for (a, _) in ids {
        for (b, _) in ids {
            let want = skipped.contains(&(a, b));
            let got = glass::skips(id(a), id(b));
            if [STRUCTURAL, REACTOR, MATRIX].contains(&a) {
                assert_eq!(got, Some(want), "{a} contra {b}");
            } else {
                assert_eq!(got, None, "{a} contra {b}: a regra do Horizon segue");
                assert!(!want, "{a} contra {b}");
            }
        }
    }
    // O registro sem os blocos do Generators (só o Mekanism): nenhum estado casa por engano.
    glass::record(id(STRUCTURAL), glass::MISSING, glass::MISSING);
    assert_eq!(glass::skips(id(STRUCTURAL), id(STRUCTURAL)), Some(true));
    assert_eq!(glass::skips(id(STRUCTURAL), id(REACTOR)), Some(false));
    assert_eq!(glass::skips(glass::MISSING, glass::MISSING), None);
}
