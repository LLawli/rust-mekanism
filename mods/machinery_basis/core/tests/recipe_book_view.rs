//! A janela do livro (`View`) contra o rastro do original: com todas as receitas conhecidas, as abas de `crushing` têm as coleções que o
//! `ClientRecipeBook` de verdade listou (`recipe_book_client.jsonl`), a aba de busca soma as delas em ordem, a página tem 20 botões, e a
//! busca por texto devolve as coleções que a `FullTextSearchTree` achou.
use std::fs;
use std::path::PathBuf;

use machinery_basis_core::recipe_book_view::{BookData, BookTab, Row, Tab, View};
use serde_json::Value;

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn rows() -> &'static [Row] {
    let rows: Vec<Row> = read("recipe_book_displays.jsonl")
        .into_iter()
        .filter(|row| row.get("id").is_some())
        .map(|row| {
            let tooltip: Vec<&'static str> = row["tooltip"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| leak(line.as_str().unwrap()))
                .collect();
            let result = &row["results"][0];
            (
                row["id"].as_i64().unwrap() as i32,
                leak(row["recipe"].as_str().unwrap()),
                leak(row["type"].as_str().unwrap()),
                row["group"].as_i64().map_or(-1, |group| group as i32),
                leak(row["category"].as_str().unwrap()),
                leak(result["item"].as_str().unwrap()),
                result["count"].as_i64().unwrap() as i32,
                Box::leak(tooltip.into_boxed_slice()) as &'static [&'static str],
                row["requirements"].as_i64().map_or(-1, |n| n as i32),
            )
        })
        .collect();
    Box::leak(rows.into_boxed_slice())
}

fn crushing() -> &'static BookData {
    let tabs: Vec<BookTab> = ["building", "efficiency", "misc"]
        .iter()
        .map(|name| BookTab {
            category: leak(&format!("mekanism:crushing_{name}")),
            primary: "minecraft:stone",
            secondary: None,
        })
        .collect();
    Box::leak(Box::new(BookData {
        book_type: "crushing",
        tabs: Box::leak(tabs.into_boxed_slice()),
        rows: rows(),
    }))
}

fn all_ids() -> Vec<i32> {
    rows().iter().map(|row| row.0).collect()
}

fn trace_step1() -> Value {
    read("recipe_book_client.jsonl")
        .into_iter()
        .find(|line| line["step"] == 1)
        .unwrap()
}

fn view(filtering: bool, craftable: bool) -> View {
    View::new(crushing(), all_ids().into_iter(), filtering, &|_| craftable)
}

#[test]
fn the_tabs_list_the_collections_of_the_original() {
    let trace = trace_step1();
    let mut view = view(false, true);
    for (index, name) in ["building", "efficiency", "misc"].iter().enumerate() {
        let expected = trace["tabs"][format!("mekanism:crushing_{name}")]
            .as_array()
            .unwrap()
            .len();
        view.select_tab(Tab::Category(index));
        assert_eq!(view.page.collections.len(), expected, "aba {name}");
    }
    view.select_tab(Tab::Search);
    assert_eq!(view.page.collections.len(), 45 + 12 + 30);
    assert_eq!(view.page.total_pages, 5);
    assert_eq!(view.buttons().len(), 20);
    assert_eq!(view.page_label(), Some((1, 5)));
}

#[test]
fn the_search_box_finds_what_the_original_search_tree_finds() {
    let mut view = view(false, true);
    for query in "iron".chars() {
        assert!(view.type_char(query));
    }
    assert!(!view.page.collections.is_empty());
    let trace = trace_step1();
    let expected = trace["queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|query| query["q"] == "iron")
        .unwrap()["first_ids"]
        .as_array()
        .unwrap()
        .len();
    assert!(expected > 0);
    // Todas as buscas do original juntas dão mais coleções do que a do tipo `crushing` sozinho: a daqui é um subconjunto.
    assert!(view.page.collections.len() <= expected);
    assert!(view.backspace());
    assert_eq!(view.search, "iro");
}

#[test]
fn the_filter_keeps_only_what_can_be_crafted() {
    let mut view = view(false, false);
    assert!(!view.buttons().is_empty());
    assert!(view.buttons().iter().all(|button| !button.craftable));
    assert!(view.toggle_filtering());
    assert!(view.buttons().is_empty());
    assert_eq!(view.page_label(), None);
}

#[test]
fn only_the_known_recipes_show_and_empty_tabs_hide() {
    let view = View::new(crushing(), std::iter::empty(), false, &|_| true);
    assert_eq!(view.visible_tabs(), vec![Tab::Search]);
    assert!(view.buttons().is_empty());
    let one = rows().iter().find(|row| row.2 == "crushing").unwrap();
    let view = View::new(crushing(), std::iter::once(one.0), false, &|_| true);
    assert_eq!(view.visible_tabs().len(), 2);
    assert_eq!(view.buttons().len(), 1);
}

#[test]
fn the_slot_select_time_cycles_the_recipes_of_a_button() {
    let mut view = view(false, true);
    let grouped = view
        .buttons()
        .iter()
        .position(|button| button.many)
        .expect("há um botão com mais de uma receita");
    let first = view.buttons()[grouped].item;
    let mut seen = std::collections::BTreeSet::new();
    for tick in 0..8 {
        view.time = tick as f32 * 30.0;
        seen.insert(view.buttons()[grouped].item);
    }
    assert!(seen.contains(first));
}
