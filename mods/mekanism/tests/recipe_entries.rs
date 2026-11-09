//! O que o Horizon descarta em silêncio: ele larga a entrada de um ingrediente que não é item nem bloco do conteúdo e a `#tag` sem arquivo (a receita fica sem a
//! entrada, ou vazia e incompleta), e recusa a receita cujo resultado é um item desconhecido. Com os itens de craft e de recurso no port, cada entrada e cada resultado
//! de cada receita do conteúdo (mesa de trabalho, forno e os tipos do Mekanism que o port lê) tem de existir: um item do vanilla (`rastros/horizon_vanilla_ids.json`,
//! tirado do jogo), um item ou bloco do mod, ou uma tag com arquivo e com algum membro.
mod common;

use common::craft::{content, declared, files_in, json, rastro};
use serde_json::Value;
use std::collections::BTreeSet;

struct Known {
    vanilla_items: BTreeSet<String>,
    vanilla_tags: BTreeSet<String>,
    own: BTreeSet<String>,
}

fn known() -> Known {
    let vanilla = rastro("horizon_vanilla_ids.json");
    let strings = |key: &str| -> BTreeSet<String> {
        vanilla[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    Known {
        vanilla_items: strings("items"),
        vanilla_tags: strings("item_tags"),
        own: declared("item")
            .into_iter()
            .chain(declared("block"))
            .map(|id| format!("mekanism:{id}"))
            .collect(),
    }
}

impl Known {
    fn item(&self, id: &str) -> bool {
        if id.starts_with("minecraft:") {
            self.vanilla_items.contains(id)
        } else {
            self.own.contains(id)
        }
    }

    /// Se a tag de item existe e resolve algum item (por entradas diretas ou por tags aninhadas que resolvem).
    fn tag(&self, tag: &str, depth: usize) -> bool {
        let (namespace, path) = tag.split_once(':').unwrap();
        if namespace == "minecraft" {
            return self.vanilla_tags.contains(tag);
        }
        let file = content()
            .join("data")
            .join(namespace)
            .join("tags/item")
            .join(format!("{path}.json"));
        if !file.is_file() || depth > 8 {
            return false;
        }
        json(&file)["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                let entry = entry
                    .as_str()
                    .unwrap_or_else(|| entry["id"].as_str().unwrap());
                match entry.strip_prefix('#') {
                    Some(nested) => self.tag(nested, depth + 1),
                    None => self.item(entry),
                }
            })
    }
}

/// Os ids e as `#tags` de um ingrediente: um id, uma lista, `neoforge:or`/`values`, `item` ou `tag`.
fn ingredient(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(entry) => out.push(entry.clone()),
        Value::Array(list) => list.iter().for_each(|child| ingredient(child, out)),
        Value::Object(map) => {
            if let Some(values) = map.get("values") {
                ingredient(values, out);
            } else if let Some(item) = map.get("item") {
                ingredient(item, out);
            } else if let Some(tag) = map.get("tag").and_then(Value::as_str) {
                out.push(format!("#{tag}"));
            }
        }
        _ => {}
    }
}

/// Os ingredientes e os resultados de item de uma receita, por tipo.
fn references(recipe: &Value) -> (Vec<String>, Vec<String>) {
    let (mut inputs, mut outputs) = (Vec::new(), Vec::new());
    let result = |value: &Value, outputs: &mut Vec<String>| {
        outputs.push(
            value
                .as_str()
                .map_or_else(|| value["id"].as_str().unwrap().to_owned(), str::to_owned),
        );
    };
    match recipe["type"].as_str().unwrap() {
        "minecraft:crafting_shaped" => {
            recipe["key"]
                .as_object()
                .unwrap()
                .values()
                .for_each(|v| ingredient(v, &mut inputs));
            result(&recipe["result"], &mut outputs);
        }
        "minecraft:crafting_shapeless" => {
            ingredient(&recipe["ingredients"], &mut inputs);
            result(&recipe["result"], &mut outputs);
        }
        "minecraft:smelting"
        | "minecraft:blasting"
        | "minecraft:smoking"
        | "minecraft:campfire_cooking" => {
            ingredient(&recipe["ingredient"], &mut inputs);
            result(&recipe["result"], &mut outputs);
        }
        "mekanism:enriching" | "mekanism:crushing" => {
            ingredient(&recipe["input"]["ingredient"], &mut inputs);
            result(&recipe["output"], &mut outputs);
        }
        "mekanism:metallurgic_infusing" => {
            ingredient(&recipe["item_input"]["ingredient"], &mut inputs);
            result(&recipe["output"], &mut outputs);
        }
        "mekanism:chemical_conversion" | "mekanism:energy_conversion" => {
            ingredient(&recipe["input"]["ingredient"], &mut inputs)
        }
        other => panic!("tipo de receita {other} sem regra neste teste"),
    }
    (inputs, outputs)
}

#[test]
fn no_recipe_of_the_content_cites_what_the_horizon_would_drop() {
    let known = known();
    let root = content().join("data/mekanism/recipe");
    let mut checked = 0;
    let mut problems = Vec::new();
    for (relative, path) in files_in(&root) {
        if !relative.ends_with(".json") {
            continue;
        }
        let recipe = json(&path);
        let Some(kind) = recipe["type"].as_str() else {
            continue;
        };
        // As receitas de outras máquinas do original (injeção, purificação...) não são lidas pelo port.
        let supported = kind.starts_with("minecraft:")
            || [
                "enriching",
                "crushing",
                "metallurgic_infusing",
                "chemical_conversion",
                "energy_conversion",
            ]
            .iter()
            .any(|t| kind == format!("mekanism:{t}"));
        if !supported {
            continue;
        }
        let (inputs, outputs) = references(&recipe);
        for entry in inputs {
            let ok = match entry.strip_prefix('#') {
                Some(tag) => known.tag(tag, 0),
                None => known.item(&entry),
            };
            if !ok {
                problems.push(format!("{relative}: a entrada {entry} o Horizon descarta"));
            }
        }
        for id in outputs {
            if !known.item(&id) {
                problems.push(format!("{relative}: o resultado {id} é desconhecido"));
            }
        }
        checked += 1;
    }
    assert!(checked >= 600, "receitas conferidas: {checked}");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A regra de verdade: uma entrada desconhecida é pega (o teste não passa por vazio).
#[test]
fn the_check_catches_an_unknown_entry() {
    let known = known();
    assert!(known.item("minecraft:iron_ingot"));
    assert!(!known.item("minecraft:not_an_item"));
    assert!(!known.item("mekanism:not_an_item"));
    assert!(known.item("mekanism:ingot_tin"));
    assert!(known.tag("c:ingots/tin", 0));
    assert!(known.tag("minecraft:planks", 0));
    assert!(!known.tag("c:ingots/not_a_metal", 0));
    assert!(!known.tag("mekanism:not_a_tag", 0));
}
