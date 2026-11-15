//! Os itens do Mekanism com o componente `DETAILS` contra o rastro do original (`dev/referencia/rastros/upgrade_history_details.jsonl`, `UpgradeHistoryTraceTest.detailsItems`:
//! para cada item registrado, se ele tem `DETAILS` e `DESCRIPTION` e as linhas do tooltip do servidor, onde os detalhes aparecem sempre).
use mekanism_rules::{DETAILS_ITEMS, description_key, has_details};
use serde_json::Value;
use std::path::PathBuf;

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn trace() -> Vec<Value> {
    let text = std::fs::read_to_string(root(
        "../../dev/referencia/rastros/upgrade_history_details.jsonl",
    ))
    .expect("rastro");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON"))
        .collect()
}

fn lang() -> Value {
    let text =
        std::fs::read_to_string(root("content/assets/mekanism/lang/en_us.json")).expect("lang");
    serde_json::from_str(&text).expect("JSON do lang")
}

fn of_mekanism(row: &Value) -> bool {
    row["item"].as_str().unwrap().starts_with("mekanism:")
}

#[test]
fn the_details_items_are_the_ones_of_the_original() {
    let mut expected: Vec<&str> = trace()
        .iter()
        .filter(|row| of_mekanism(row) && row["details"] == true)
        .map(|row| row["item"].as_str().unwrap().to_owned())
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect();
    let mut have: Vec<&str> = DETAILS_ITEMS.to_vec();
    expected.sort_unstable();
    have.sort_unstable();
    assert_eq!(have, expected);
    assert_eq!(DETAILS_ITEMS.len(), 91);
}

#[test]
fn only_the_details_items_hide_the_details() {
    for row in trace().iter().filter(|row| of_mekanism(row)) {
        let item = row["item"].as_str().unwrap();
        assert_eq!(has_details(item), row["details"] == true, "{item}");
    }
}

/// Todo item com `DETAILS` tem `DESCRIPTION`, e a chave que o port usa dá, no `en_us.json`, o texto da descrição que o tooltip do original traz (a linha própria, antes das que outros
/// componentes ainda acrescentem depois).
#[test]
fn the_description_key_gives_the_text_the_original_shows() {
    let lang = lang();
    for row in trace()
        .iter()
        .filter(|row| of_mekanism(row) && row["details"] == true)
    {
        let item = row["item"].as_str().unwrap();
        assert_eq!(row["description"], true, "{item}");
        let key = description_key(item);
        let text = lang[&key]
            .as_str()
            .unwrap_or_else(|| panic!("{item}: falta {key} no lang"));
        let lines: Vec<&str> = row["tooltip"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap())
            .collect();
        assert!(
            lines.contains(&text),
            "{item}: {key} = {text:?} não está em {lines:?}"
        );
    }
}

/// O nome do item de um bloco com tier leva a cor do tier (`ItemRegistryObject.getNameColor`): as fábricas e os cubos de energia do rastro, e só eles entre os itens com `DETAILS`.
#[test]
fn the_tiered_blocks_with_details_get_the_color_of_the_tier() {
    use mekanism_rules::{name_color, tier_block_color};
    let mut tiered = 0;
    for row in trace()
        .iter()
        .filter(|row| of_mekanism(row) && row["details"] == true)
    {
        let item = row["item"].as_str().unwrap();
        let expected = item.ends_with("_factory") || item.ends_with("_energy_cube");
        assert_eq!(tier_block_color(item).is_some(), expected, "{item}");
        if expected {
            tiered += 1;
            assert!(name_color(item).is_some(), "{item}");
        }
    }
    assert_eq!(tiered, 41);
    assert_eq!(name_color("mekanism:basic_energy_cube"), Some(0x5FFFB8));
    assert_eq!(
        name_color("mekanism:ultimate_infusing_factory"),
        Some(0xF787FF)
    );
    assert_eq!(name_color("mekanism:creative_energy_cube"), Some(0x585858));
}
