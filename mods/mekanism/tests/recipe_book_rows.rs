//! A tabela dos displays do livro (`recipe_book_rows`, gerada do rastro `recipe_book_displays.jsonl` do original) contra o conteúdo
//! do port: cada receita do Mekanism na tabela tem o arquivo, a categoria do livro que o arquivo dá (`category`, `misc` sem o
//! campo), o item e a quantidade do resultado, e o grupo (`group`) é o mesmo número em todas as receitas que dizem o mesmo texto.
//! O id e o número do grupo são do original (os de todas as receitas do jogo, que o Horizon não numera); o resto o port deriva.
mod common;

use common::craft::{content, json};
use mekanism_rules::recipe_book_rows::ROWS;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn file_of(recipe: &str) -> Option<PathBuf> {
    let (namespace, path) = recipe.split_once(':')?;
    let file = content()
        .join("../..")
        .join(namespace)
        .join("content/data")
        .join(namespace)
        .join("recipe")
        .join(format!("{path}.json"));
    file.is_file().then_some(file)
}

/// A categoria do livro de uma receita do Mekanism: `mekanism:<tipo>_<category>`, com `misc` sem o campo (`BookInfo`).
fn category_of(kind: &str, json: &Value) -> String {
    format!(
        "mekanism:{kind}_{}",
        json["category"].as_str().unwrap_or("misc")
    )
}

fn rows_of_content() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    i32,
    &'static str,
    i32,
    Value,
)> {
    ROWS.iter()
        .filter(|row| row.2 != "smelting")
        .filter_map(|row| {
            file_of(row.1).map(|file| (row.1, row.2, row.4, row.3, row.5, row.6, json(&file)))
        })
        .collect()
}

#[test]
fn the_table_is_the_trace_of_the_original() {
    let trace = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/rastros/recipe_book_displays.jsonl"),
    )
    .unwrap();
    let lines: Vec<Value> = trace
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("id").is_some())
        .collect();
    assert_eq!(lines.len(), ROWS.len());
    assert_eq!(ROWS.len(), 614);
    let ids: BTreeSet<i32> = ROWS.iter().map(|row| row.0).collect();
    assert_eq!(ids.len(), ROWS.len(), "um id por display");
    // A mesma contagem por tipo que o relatório da 1.ª rodada mediu: 223, 255 e 31 (mais 105 do cozimento do vanilla).
    let count = |kind: &str| ROWS.iter().filter(|row| row.2 == kind).count();
    assert_eq!(
        (
            count("enriching"),
            count("crushing"),
            count("metallurgic_infusing"),
            count("smelting")
        ),
        (223, 255, 31, 105)
    );
}

#[test]
fn the_content_gives_the_category_the_result_and_the_group_of_the_original() {
    let rows = rows_of_content();
    // Só as 18 do Mekanism Additions ficam sem arquivo (o mod não é do port): as outras 491 das quatro máquinas têm.
    assert_eq!(rows.len(), 614 - 105 - 18);
    for (recipe, kind, category, _, result, count, json) in &rows {
        assert_eq!(category_of(kind, json), *category, "categoria de {recipe}");
        assert_eq!(json["output"]["id"], *result, "resultado de {recipe}");
        assert_eq!(
            json["output"]["count"].as_i64().unwrap_or(1),
            i64::from(*count),
            "quantidade de {recipe}"
        );
    }
}

#[test]
fn a_group_text_is_one_group_number() {
    let mut by_text: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
    let mut by_number: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    for (recipe, _, _, group, _, _, json) in rows_of_content() {
        match json["group"].as_str() {
            Some(text) => {
                assert!(group >= 0, "{recipe} tem group e a tabela não");
                by_text.entry(text.to_owned()).or_default().insert(group);
                by_number.entry(group).or_default().insert(text.to_owned());
            }
            None => assert_eq!(group, -1, "{recipe} não tem group e a tabela tem"),
        }
    }
    assert!(by_text.values().all(|numbers| numbers.len() == 1));
    assert!(by_number.values().all(|texts| texts.len() == 1));
    assert!(by_text.len() > 50);
}

/// A tabela de todos os displays do jogo (`DISPLAYS`, de `recipe_book_known_displays.jsonl`): os ids são seguidos, do zero aos 4.987
/// do original, cada linha do livro das máquinas está nela com a mesma receita, e as contagens por mod são as medidas (decisão 269).
#[test]
fn the_table_of_every_display_of_the_game_has_the_numbering_of_the_original() {
    use mekanism_rules::recipe_book_rows::DISPLAYS;
    let mut next = 0;
    let mut by_mod: BTreeMap<&str, i32> = BTreeMap::new();
    for &(recipe, first, count) in DISPLAYS {
        assert_eq!(first, next, "{recipe}");
        assert!(count > 0, "{recipe}");
        next += i32::from(count);
        *by_mod.entry(recipe.split_once(':').unwrap().0).or_default() += i32::from(count);
    }
    assert_eq!(next, 4987);
    assert_eq!(
        by_mod,
        BTreeMap::from([
            ("mekanism", 1982),
            ("mekanismadditions", 1114),
            ("mekanismgenerators", 38),
            ("mekanismtools", 107),
            ("minecraft", 1746),
        ])
    );
    let displays = machinery_basis_core::recipe_book_data::Displays(DISPLAYS);
    let by_id: BTreeMap<i32, &str> = displays.ids().collect();
    for row in ROWS {
        assert_eq!(by_id[&row.0], row.1, "display {}", row.0);
    }
    // O que o port tem hoje do Mekanism e do Generators (o resto são máquinas ainda não portadas): com o vanilla, menos que os 3.072
    // que levam a tabela do `HashMap` a 8.192; por isso o `/recipe give @s *` conta a tabela inteira do original (decisão 269).
    let ported: i32 = DISPLAYS
        .iter()
        .filter(|row| !row.0.starts_with("minecraft:") && file_of(row.0).is_some())
        .map(|row| i32::from(row.2))
        .sum();
    assert!(ported > 500, "{ported}");
    assert!(1746 + ported < 3072, "{ported}");
}
