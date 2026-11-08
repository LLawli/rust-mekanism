//! `CONTENT_GAPS` (decisão 13): as receitas que ficam incompletas só porque o port ainda não tem o conteúdo delas. O Horizon dá `Incomplete recipe detected`
//! (e a mensagem do `IncompleteRecipeScanner`) a toda receita de tipo do Mekanism cujo ingrediente é só de tags sem item nenhum, e hoje são 40; o original, com o
//! conteúdo inteiro, não tem nenhuma. A lista do mod tem de ser exatamente o que os arquivos do conteúdo dão, com a regra do Horizon: um ingrediente de item
//! (id, tag, lista ou `neoforge:or`) que não resolve nenhum item e só tem tags é incompleto, e um que cita um item desconhecido é erro do arquivo (`Couldn't parse`).
//! Conteúdo novo que tira uma receita da lista (ou põe uma nela) falha este teste: ajuste `CONTENT_GAPS`.
mod common;

use common::load_of_type;
use mekanism_rules::CONTENT_GAPS;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Os tipos de receita do Mekanism que o port lê, e onde cada um guarda o ingrediente de item.
const TYPES: [(&str, &[&str]); 5] = [
    ("mekanism:enriching", &["input", "ingredient"]),
    ("mekanism:crushing", &["input", "ingredient"]),
    (
        "mekanism:metallurgic_infusing",
        &["item_input", "ingredient"],
    ),
    ("mekanism:chemical_conversion", &["input", "ingredient"]),
    ("mekanism:energy_conversion", &["input", "ingredient"]),
];

fn data(namespace: &str) -> PathBuf {
    let owner = if namespace == "mekanismgenerators" {
        "mekanismgenerators"
    } else {
        "mekanism"
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(owner)
        .join("content/data")
        .join(namespace)
}

/// Se o item existe: o vanilla existe, e os do Mekanism são os que o conteúdo declara (item ou bloco com item).
fn known_item(id: &str) -> bool {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    if namespace == "minecraft" {
        return true;
    }
    ["item", "block"].iter().any(|kind| {
        data(namespace)
            .join("horizon")
            .join(kind)
            .join(format!("{path}.json"))
            .exists()
    })
}

/// Os itens de uma entrada de ingrediente (id, `#tag`), e se ela é uma tag.
fn entry_items(entry: &str, items: &mut BTreeSet<String>) {
    let Some(tag) = entry.strip_prefix('#') else {
        if known_item(entry) {
            items.insert(entry.to_owned());
        }
        return;
    };
    let (namespace, path) = tag.split_once(':').unwrap();
    // As tags do vanilla o jogo as dá, e o teste as trata como não vazias.
    if namespace == "minecraft" {
        items.insert(format!("#{tag}"));
        return;
    }
    let file = data(namespace)
        .join("tags/item")
        .join(format!("{path}.json"));
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let json: Value = serde_json::from_str(&text).unwrap();
    for value in json["values"].as_array().unwrap() {
        entry_items(value.as_str().unwrap(), items);
    }
}

/// As folhas de um ingrediente: ids e tags, achatando listas e `neoforge:or`.
fn leaves<'a>(ingredient: &'a Value, out: &mut Vec<&'a str>) {
    match ingredient {
        Value::String(entry) => out.push(entry),
        Value::Array(list) => list.iter().for_each(|child| leaves(child, out)),
        Value::Object(object) => {
            if let Some(values) = object.get("values") {
                leaves(values, out);
            }
        }
        other => panic!("ingrediente {other}"),
    }
}

/// O que o Horizon faz com um ingrediente: `Some(true)` incompleto (sem item, só tags), `Some(false)` erro do arquivo (sem item e com um id desconhecido), `None` completo.
fn classify(ingredient: &Value) -> Option<bool> {
    let mut entries = Vec::new();
    leaves(ingredient, &mut entries);
    let mut items = BTreeSet::new();
    entries
        .iter()
        .for_each(|entry| entry_items(entry, &mut items));
    if !items.is_empty() {
        return None;
    }
    Some(entries.iter().all(|entry| entry.starts_with('#')))
}

#[test]
fn the_content_gaps_are_what_the_content_files_give() {
    let mut incomplete = BTreeSet::new();
    let mut seen = 0;
    for (kind, path) in TYPES {
        for (id, json) in load_of_type(kind) {
            seen += 1;
            let mut ingredient = &json;
            for key in path {
                ingredient = &ingredient[*key];
            }
            if classify(ingredient) == Some(true) {
                incomplete.insert(id);
            }
        }
    }
    assert!(seen >= 150, "receitas lidas: {seen}");
    let listed: BTreeSet<String> = CONTENT_GAPS.iter().map(|id| (*id).to_owned()).collect();
    assert_eq!(incomplete, listed, "CONTENT_GAPS não bate com o conteúdo");
    assert_eq!(
        CONTENT_GAPS.len(),
        listed.len(),
        "id repetido em CONTENT_GAPS"
    );
}

#[test]
fn nothing_the_port_already_has_is_in_the_list() {
    // As receitas que o port leva a sério (ferro, carbono, osmium, aço) não podem estar na lista.
    for id in [
        "mekanism:processing/iron/enriched",
        "mekanism:processing/iron/enriched_dust",
        "mekanism:enriching/enriched/carbon",
    ] {
        assert!(!CONTENT_GAPS.contains(&id), "{id}");
    }
}
