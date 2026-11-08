//! O módulo `components-ingredient` da `machinery_basis` (`mods/machinery_basis/components_ingredient.rs`), que o launcher enxerta em `crafting.rs` do
//! Horizon, compilado aqui como é: ele só usa `std` e `serde_json`. Resolve o `items` de um `neoforge:components` (id, `#tag`, lista, `neoforge:or`) para o hook
//! `ingredient`, que não recebe o `Items`.
#[allow(dead_code)]
mod machinery_components {
    include!("../../components_ingredient.rs");
}

use serde_json::{Map, Value, json};
use std::collections::HashMap;

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn tags() -> HashMap<String, Vec<String>> {
    HashMap::from([
        (
            "minecraft:coals".to_owned(),
            vec!["minecraft:coal".to_owned(), "minecraft:charcoal".to_owned()],
        ),
        (
            "c:rods".to_owned(),
            vec!["minecraft:stick".to_owned(), "minecraft:coal".to_owned()],
        ),
    ])
}

fn resolved(data: &Value, tags: &HashMap<String, Vec<String>>) -> Vec<String> {
    let data = object(data.clone());
    machinery_components::stash(&data, tags);
    let mut items = Vec::new();
    machinery_components::resolve("neoforge:components", &data, &mut items);
    items
}

#[test]
fn ids_lists_tags_and_unions_become_one_list_without_repeats() {
    let kind = "neoforge:ingredient_type";
    assert_eq!(
        resolved(
            &json!({kind: "neoforge:components", "items": "minecraft:stick"}),
            &tags()
        ),
        ["minecraft:stick"]
    );
    assert_eq!(
        resolved(
            &json!({kind: "neoforge:components", "items": ["minecraft:bone", "minecraft:stick"]}),
            &tags()
        ),
        ["minecraft:bone", "minecraft:stick"]
    );
    assert_eq!(
        resolved(
            &json!({kind: "neoforge:components", "items": "#minecraft:coals"}),
            &tags()
        ),
        ["minecraft:coal", "minecraft:charcoal"]
    );
    // A união de `HolderSet`s se achata, e o que a tag repete do que já está na lista não entra de novo.
    let union = json!({kind: "neoforge:components", "items": {"type": "neoforge:or", "values": ["#minecraft:coals", ["#c:rods"], "minecraft:bone"]}});
    assert_eq!(
        resolved(&union, &tags()),
        [
            "minecraft:coal",
            "minecraft:charcoal",
            "minecraft:stick",
            "minecraft:bone"
        ]
    );
}

#[test]
fn a_missing_tag_leaves_the_list_empty_and_the_file_fails_like_in_the_game() {
    let data = json!({"type": "neoforge:components", "items": "#mektest:nothing"});
    assert!(resolved(&data, &tags()).is_empty());
}

#[test]
fn other_kinds_are_left_alone_and_the_tags_are_not_kept() {
    let mut items = vec!["minecraft:keep".to_owned()];
    let data = object(json!({"type": "mymod:aspect", "items": "#minecraft:coals"}));
    machinery_components::stash(&data, &tags());
    machinery_components::resolve("mymod:aspect", &data, &mut items);
    assert_eq!(items, ["minecraft:keep"]);
    // Sem `#tag` nada se guarda, e depois de um `resolve` a cópia vai embora: a tag do objeto seguinte não acha a do anterior.
    let first = json!({"type": "neoforge:components", "items": "#minecraft:coals"});
    assert!(!resolved(&first, &tags()).is_empty());
    assert!(resolved(&first, &HashMap::new()).is_empty());
}
