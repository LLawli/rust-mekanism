//! Os itens do Mekanism Generators com o componente `DETAILS` contra o rastro do original (`dev/referencia/rastros/upgrade_history_details.jsonl`).
use mekanismgenerators_rules::{DETAILS_ITEMS, has_details};
use serde_json::Value;
use std::path::PathBuf;

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn rows() -> Vec<Value> {
    let text = std::fs::read_to_string(root(
        "../../dev/referencia/rastros/upgrade_history_details.jsonl",
    ))
    .expect("rastro");
    text.lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("linha JSON"))
        .filter(|row| {
            row["item"]
                .as_str()
                .unwrap()
                .starts_with("mekanismgenerators:")
        })
        .collect()
}

#[test]
fn the_details_items_are_the_ones_of_the_original() {
    for row in rows() {
        let item = row["item"].as_str().unwrap();
        assert_eq!(has_details(item), row["details"] == true, "{item}");
    }
    assert_eq!(
        rows().iter().filter(|row| row["details"] == true).count(),
        DETAILS_ITEMS.len()
    );
}

/// A chave `description.mekanismgenerators.<nome>` do `en_us.json` dá o texto da descrição que o tooltip do original traz.
#[test]
fn the_description_of_each_item_is_in_the_tooltip_of_the_original() {
    let text = std::fs::read_to_string(root("content/assets/mekanismgenerators/lang/en_us.json"))
        .expect("lang");
    let lang: Value = serde_json::from_str(&text).unwrap();
    for row in rows().iter().filter(|row| row["details"] == true) {
        let item = row["item"].as_str().unwrap();
        let key = format!("description.{}", item.replace(':', "."));
        let line = lang[&key]
            .as_str()
            .unwrap_or_else(|| panic!("{item}: falta {key}"));
        let lines: Vec<&str> = row["tooltip"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        assert!(
            lines.contains(&line),
            "{item}: {line:?} não está em {lines:?}"
        );
    }
}
