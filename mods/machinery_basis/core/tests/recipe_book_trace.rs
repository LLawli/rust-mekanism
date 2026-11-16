//! O livro de receitas contra o original: `RecipeBookDisplaysTraceTest.java` (`dev/referencia/rastros/recipe_book_displays.jsonl` e
//! `recipe_book_client.jsonl`) grava os displays que o servidor manda das receitas de `enriching`, `crushing`, `smelting` (as do vanilla
//! que o Energized Smelter usa) e `metallurgic_infusing`, e o que o `ClientRecipeBook` e a `FullTextSearchTree` de verdade fazem com elas
//! quando o jogador conhece todas (`step` 1) ou uma a cada três (`step` 3): as coleções de cada aba, na ordem, e as coleções que cada
//! busca acha. O teste monta o mesmo livro com a lógica da lib e compara.
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use machinery_basis_core::recipe_book::{ClientBook, Entry};
use serde_json::Value;

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros").join(name);
    fs::read_to_string(path).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// As linhas de tooltip que a busca lê e os ids do resultado, por id de display.
struct Row {
    entry: Entry,
    names: Vec<String>,
    ids: Vec<(String, String)>,
}

fn displays() -> Vec<Row> {
    read("recipe_book_displays.jsonl")
        .into_iter()
        .filter(|row| row.get("id").is_some())
        .map(|row| Row {
            entry: Entry {
                id: row["id"].as_i64().unwrap() as i32,
                category: row["category"].as_str().unwrap().to_owned(),
                group: row["group"].as_i64().map(|group| group as i32),
            },
            names: row["tooltip"].as_array().unwrap().iter().map(|line| line.as_str().unwrap().to_lowercase()).collect(),
            ids: row["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|result| {
                    let (namespace, path) = result["item"].as_str().unwrap().split_once(':').unwrap();
                    (namespace.to_lowercase(), path.to_lowercase())
                })
                .collect(),
        })
        .collect()
}

/// O livro do jogador que conhece uma receita a cada `step`, com os textos que a busca lê por coleção.
fn build(rows: &[Row], step: usize) -> ClientBook {
    let mut book = ClientBook::default();
    for (index, row) in rows.iter().enumerate() {
        if index % step == 0 {
            book.add(row.entry.clone());
        }
    }
    book.rebuild_collections(&[]);
    let by_id: HashMap<i32, &Row> = rows.iter().map(|row| (row.entry.id, row)).collect();
    for index in 0..book.collection_count() {
        let (names, ids): (Vec<_>, Vec<_>) = book.collection(index).entries.iter().map(|entry| (&by_id[&entry.id].names, &by_id[&entry.id].ids)).unzip();
        let collection = book.collection_mut(index);
        collection.names = names.into_iter().flatten().cloned().collect();
        collection.ids = ids.into_iter().flatten().cloned().collect();
    }
    book
}

#[test]
fn the_collections_of_each_tab_come_in_the_order_of_the_original() {
    let rows = displays();
    assert!(rows.len() > 600, "o rastro tem {} displays", rows.len());
    let mut compared = 0;
    for line in read("recipe_book_client.jsonl") {
        assert!(line.get("error").is_none(), "o rastro falhou: {line}");
        let step = line["step"].as_u64().unwrap() as usize;
        let book = build(&rows, step);
        for (category, lists) in line["tabs"].as_object().unwrap() {
            let ours: Vec<Vec<i64>> = book
                .tab(category)
                .iter()
                .map(|&index| book.collection(index).entries.iter().map(|entry| i64::from(entry.id)).collect())
                .collect();
            let theirs: Vec<Vec<i64>> = lists.as_array().unwrap().iter().map(|list| list.as_array().unwrap().iter().map(|id| id.as_i64().unwrap()).collect()).collect();
            assert_eq!(ours, theirs, "step {step}, aba {category}");
            compared += theirs.len();
        }
        assert!(line["known"].as_u64().unwrap() > 200);
    }
    assert!(compared > 400, "só {compared} coleções comparadas");
}

#[test]
fn the_search_finds_the_same_collections_as_the_original() {
    let rows = displays();
    let mut compared = 0;
    for line in read("recipe_book_client.jsonl") {
        let step = line["step"].as_u64().unwrap() as usize;
        let book = build(&rows, step);
        for query in line["queries"].as_array().unwrap() {
            let text = query["q"].as_str().unwrap();
            let theirs: BTreeSet<i64> = query["first_ids"].as_array().unwrap().iter().map(|id| id.as_i64().unwrap()).collect();
            let ours: BTreeSet<i64> = (0..book.collection_count())
                .filter(|&index| book.collection(index).matches_search(&text.to_lowercase()))
                .map(|index| i64::from(book.collection(index).entries[0].id))
                .collect();
            assert_eq!(ours, theirs, "step {step}, busca {text:?}");
            compared += theirs.len();
        }
    }
    assert!(compared > 500, "só {compared} resultados comparados");
}
