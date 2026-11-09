//! Nenhuma receita dos tipos do Mekanism que o port lê fica incompleta só porque falta conteúdo (decisão 13, fechada em 2026-11-09). O Horizon dá
//! `Incomplete recipe detected` (e a mensagem do `IncompleteRecipeScanner`) a toda receita cujo ingrediente é só de tags sem item nenhum, e o original, com o
//! conteúdo inteiro, não tem nenhuma. Antes dos itens de craft e de recurso eram 40 (a lista `CONTENT_GAPS`, de estanho, urânio, chumbo, fluorita, bronze, os pós
//! sujos e os blocos de bio fuel e de carvão vegetal); com eles são 0, e a lista saiu junto. Este teste fica de guarda: um ingrediente de receita (id, tag, lista
//! ou `neoforge:or`) que não resolve nenhum item e só tem tags é incompleto, e um que cita um item desconhecido é erro do arquivo (`Couldn't parse`). Receita nova
//! ou tag esvaziada que deixe uma incompleta falha aqui, em vez de aparecer só no chat do jogador.
mod common;

use common::load_of_type;
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
fn no_recipe_of_the_types_the_port_reads_is_incomplete() {
    let mut incomplete = BTreeSet::new();
    let mut unknown_item = BTreeSet::new();
    let mut seen = 0;
    for (kind, path) in TYPES {
        for (id, json) in load_of_type(kind) {
            seen += 1;
            let mut ingredient = &json;
            for key in path {
                ingredient = &ingredient[*key];
            }
            match classify(ingredient) {
                Some(true) => {
                    incomplete.insert(id);
                }
                Some(false) => {
                    unknown_item.insert(id);
                }
                None => {}
            }
        }
    }
    assert!(seen >= 150, "receitas lidas: {seen}");
    assert!(
        incomplete.is_empty(),
        "receitas incompletas (a mensagem sairia no chat): {incomplete:?}"
    );
    assert!(
        unknown_item.is_empty(),
        "receitas com item desconhecido (o Horizon as recusa): {unknown_item:?}"
    );
}

/// Os 40 ids de `CONTENT_GAPS` (a lista antes do conteúdo de craft e de recurso): nenhum ficou incompleto nem desconhecido, e todos têm o arquivo.
#[test]
fn the_forty_old_gaps_are_complete_now() {
    const OLD_GAPS: [&str; 40] = [
        "mekanism:chemical_conversion/bio/from_bio_fuel_block",
        "mekanism:chemical_conversion/carbon/from_charcoal_block",
        "mekanism:chemical_conversion/tin/from_dust",
        "mekanism:chemical_conversion/tin/from_enriched",
        "mekanism:enriching/charcoal_dust",
        "mekanism:enriching/enriched/tin",
        "mekanism:processing/bronze/dust/from_ingot",
        "mekanism:processing/copper/dirty_dust/from_clump",
        "mekanism:processing/copper/dust/from_dirty_dust",
        "mekanism:processing/fluorite/from_dust",
        "mekanism:processing/fluorite/from_ore",
        "mekanism:processing/fluorite/to_dust",
        "mekanism:processing/gold/dirty_dust/from_clump",
        "mekanism:processing/gold/dust/from_dirty_dust",
        "mekanism:processing/iron/dirty_dust/from_clump",
        "mekanism:processing/iron/dust/from_dirty_dust",
        "mekanism:processing/lead/dirty_dust/from_clump",
        "mekanism:processing/lead/dust/from_dirty_dust",
        "mekanism:processing/lead/dust/from_ingot",
        "mekanism:processing/lead/dust/from_ore",
        "mekanism:processing/lead/dust/from_raw_block",
        "mekanism:processing/lead/dust/from_raw_ore",
        "mekanism:processing/osmium/dirty_dust/from_clump",
        "mekanism:processing/osmium/dust/from_dirty_dust",
        "mekanism:processing/osmium/dust/from_raw_block",
        "mekanism:processing/refined_glowstone/ingot_to_dust",
        "mekanism:processing/refined_obsidian/dust/from_ingot",
        "mekanism:processing/tin/dirty_dust/from_clump",
        "mekanism:processing/tin/dust/from_dirty_dust",
        "mekanism:processing/tin/dust/from_ingot",
        "mekanism:processing/tin/dust/from_ore",
        "mekanism:processing/tin/dust/from_raw_block",
        "mekanism:processing/tin/dust/from_raw_ore",
        "mekanism:processing/uranium/dirty_dust/from_clump",
        "mekanism:processing/uranium/dust/from_dirty_dust",
        "mekanism:processing/uranium/dust/from_ingot",
        "mekanism:processing/uranium/dust/from_ore",
        "mekanism:processing/uranium/dust/from_raw_block",
        "mekanism:processing/uranium/dust/from_raw_ore",
        "mekanism:processing/uranium/yellow_cake_uranium",
    ];
    let mut loaded: BTreeSet<String> = BTreeSet::new();
    for (kind, path) in TYPES {
        for (id, json) in load_of_type(kind) {
            let mut ingredient = &json;
            for key in path {
                ingredient = &ingredient[*key];
            }
            assert!(classify(ingredient).is_none(), "{id}");
            loaded.insert(id);
        }
    }
    for id in OLD_GAPS {
        assert!(loaded.contains(id), "falta a receita {id}");
    }
}
