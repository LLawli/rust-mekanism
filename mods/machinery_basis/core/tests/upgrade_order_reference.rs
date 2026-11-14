//! A ordem das linhas de upgrade no tooltip do item (decisão 225) contra o original: `dev/referencia/rastros/item_upgrade_order_<n>.jsonl` (`testes/UpgradeOrderTraceTest.java`, duas
//! execuções do `runGameTestServer`) grava, para 64 amostras de upgrades instalados em ordens diferentes, o hash de identidade de cada `Holder<Upgrade>` da execução, a ordem do mapa do
//! tile, a do item, a do que o cliente decodifica e as linhas do tooltip. A mesma instalação sai em ordens diferentes nas duas execuções (os hashes são outros) e igual dentro de uma.
use machinery_basis_core::upgrade_order::stages;
use serde_json::Value;
use std::path::PathBuf;

fn rows(run: u32) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/item_upgrade_order_{run}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}

/// O nome que o tooltip mostra (`upgrade.mekanism.<id>` da datagen) de cada tipo.
fn display(id: &str) -> &'static str {
    match id {
        "speed" => "Speed",
        "energy" => "Energy",
        "muffling" => "Muffling",
        "filter" => "Filter",
        "chemical" => "Chemical",
        "anchor" => "Anchor",
        "stone_generator" => "Stone Generator",
        "xp_containment" => "Xp Containment",
        other => panic!("{other}"),
    }
}

/// As três ordens que o modelo dá para a instalação do rastro, com os hashes dele, como nomes.
fn modelled(row: &Value) -> [Vec<String>; 3] {
    let installed = names(&row["installed"]);
    let hashes: Vec<i32> = installed
        .iter()
        .map(|id| row["hashes"][id.as_str()].as_i64().unwrap() as i32)
        .collect();
    stages(&hashes).map(|order| {
        order
            .into_iter()
            .map(|index| installed[index].clone())
            .collect()
    })
}

#[test]
fn the_tables_of_the_original_give_the_order_of_the_tile_the_item_and_the_decoded_map() {
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            let [tile, item, decoded] = modelled(row);
            assert_eq!(
                tile,
                names(&row["tile"]),
                "execução {run} amostra {n}: o mapa do tile"
            );
            assert_eq!(
                item,
                names(&row["item"]),
                "execução {run} amostra {n}: o mapa do item"
            );
            assert_eq!(
                decoded,
                names(&row["decoded"]),
                "execução {run} amostra {n}: o que o cliente decodifica"
            );
        }
    }
}

#[test]
fn the_tooltip_lists_the_upgrades_in_the_order_of_the_item_map() {
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            let lines: Vec<String> = names(&row["tooltip"]);
            let expected: Vec<String> = names(&row["item"])
                .iter()
                .map(|id| display(id).to_owned())
                .collect();
            let seen: Vec<String> = lines
                .iter()
                .map(|line| {
                    line.trim_start_matches("- ")
                        .split(": x")
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect();
            assert_eq!(seen, expected, "execução {run} amostra {n}");
        }
    }
}

/// O que o original mostra: dentro de uma execução a ordem é fixa (o mesmo conjunto instalado em ordens diferentes dá a mesma ordem do item quando nada colide), e de uma execução
/// para outra ela muda.
#[test]
fn the_order_changes_from_one_run_to_the_next() {
    let (first, second) = (rows(1), rows(2));
    let differing = first
        .iter()
        .zip(&second)
        .filter(|(a, b)| a["item"] != b["item"])
        .count();
    assert!(
        differing > first.len() / 4,
        "só {differing} de {} amostras mudaram de uma execução para a outra",
        first.len()
    );
    // O hash de um `Holder` não muda dentro da execução: o mesmo valor em todas as amostras.
    for run in [&first, &second] {
        assert!(run.iter().all(|row| row["hashes"] == run[0]["hashes"]));
    }
    assert_ne!(first[0]["hashes"], second[0]["hashes"]);
}
