//! O tipo de receita `mekanism:mek_data` (`MekanismShapedRecipe`): as 86 receitas da datagen (85 do Mekanism e a do vento do Generators) estão no conteúdo, com o formato
//! da `crafting_shaped`, citam só o que existe, e a injeção `mek_data_alias` as põe na mesa. O que a `assemble` copia das entradas vem do rastro do original
//! (`mek_data_assemble.jsonl`, `testes/MekDataTraceTest.java`).
mod common;

use common::craft::{content, declared, json, rastro};
use machinery_basis_core::recipe_data::{apply_energy, merge_energy};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[path = "common/crafting_stub.rs"]
mod items;

/// O módulo `mek_data_energy.rs`, como mora em `crafting.rs` (`mekanism_mek_data`): o `horizon_world::nbt::Tag` que ele importa é o do brinquedo.
#[allow(dead_code)]
mod mekanism_mek_data {
    pub mod horizon_world {
        pub mod nbt {
            pub use crate::items::Tag;
        }
    }
    include!("../native/mek_data_energy.rs");
}

#[allow(clippy::ptr_arg)]
mod alias {
    use super::mekanism_mek_data;
    use serde_json::Value;
    include!("../native/mek_data_alias.rs");
    pub fn run(sources: &mut Vec<(String, Value)>) {
        mekanism_mek_data_alias(sources)
    }
}

fn generators_content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../mekanismgenerators/content")
}

fn generators_items() -> BTreeSet<String> {
    ["item", "block"]
        .into_iter()
        .flat_map(|kind| {
            std::fs::read_dir(
                generators_content()
                    .join("data/mekanismgenerators/horizon")
                    .join(kind),
            )
            .unwrap()
        })
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .map(|name| name.strip_suffix(".json").unwrap().to_owned())
        .collect()
}

fn lines(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O arquivo da receita no port: `mekanism/recipe/x.json` é do Mekanism, `mekanismgenerators/recipe/x.json` do Generators.
fn recipe_file(relative: &str) -> PathBuf {
    match relative.split_once('/').unwrap() {
        ("mekanism", rest) => content().join("data/mekanism").join(rest),
        ("mekanismgenerators", rest) => generators_content()
            .join("data/mekanismgenerators")
            .join(rest),
        _ => panic!("{relative}"),
    }
}

fn tag_members(tag: &str, vanilla_tags: &BTreeSet<String>, vanilla: &BTreeSet<String>) -> usize {
    let (namespace, rest) = tag.split_once(':').unwrap();
    let file = content()
        .join("data")
        .join(namespace)
        .join("tags/item")
        .join(format!("{rest}.json"));
    if !file.is_file() {
        return usize::from(vanilla_tags.contains(tag));
    }
    json(&file)["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().map_or_else(|| v["id"].as_str().unwrap(), |s| s))
        .map(|entry| match entry.strip_prefix('#') {
            Some(nested) => tag_members(nested, vanilla_tags, vanilla),
            None if entry.starts_with("minecraft:") => usize::from(vanilla.contains(entry)),
            None => {
                let id = entry.strip_prefix("mekanism:").unwrap();
                usize::from(declared("item").contains(id) || declared("block").contains(id))
            }
        })
        .sum()
}

#[test]
fn every_recipe_of_the_original_is_in_the_content_as_the_original_wrote_it() {
    let recipes = rastro("mek_data_recipes.json");
    let recipes = recipes.as_object().unwrap();
    assert_eq!(recipes.len(), 86);
    let own: BTreeSet<String> = declared("item")
        .into_iter()
        .chain(declared("block"))
        .map(|id| format!("mekanism:{id}"))
        .collect();
    let generators: BTreeSet<String> = generators_items()
        .into_iter()
        .map(|id| format!("mekanismgenerators:{id}"))
        .collect();
    let vanilla = rastro("horizon_vanilla_ids.json");
    let set = |key: &str| -> BTreeSet<String> {
        vanilla[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    let (vanilla_items, vanilla_tags) = (set("items"), set("item_tags"));
    for (relative, original) in recipes {
        let ours = json(&recipe_file(relative));
        assert_eq!(ours["type"], "mekanism:mek_data", "{relative}");
        assert_eq!(ours["result"]["id"], original["result"], "{relative}");
        assert_eq!(ours["pattern"], original["pattern"], "{relative}");
        assert_eq!(
            ours.get("group").and_then(Value::as_str),
            original["group"].as_str(),
            "{relative}"
        );
        let result = original["result"].as_str().unwrap();
        assert!(
            own.contains(result) || generators.contains(result),
            "{relative}: {result}"
        );
        let width = original["pattern"][0].as_str().unwrap().len();
        let mut used = BTreeSet::new();
        for row in original["pattern"].as_array().unwrap() {
            let row = row.as_str().unwrap();
            assert_eq!(row.len(), width, "{relative}");
            used.extend(row.chars().filter(|c| *c != ' ').map(String::from));
        }
        let keys: BTreeSet<String> = ours["key"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, used, "{relative}");
        for entry in original["refs"].as_array().unwrap() {
            let entry = entry.as_str().unwrap();
            if let Some(tag) = entry.strip_prefix('#') {
                assert!(
                    tag_members(tag, &vanilla_tags, &vanilla_items) > 0,
                    "{relative}: a tag {tag} está vazia"
                );
            } else if entry.starts_with("minecraft:") {
                assert!(vanilla_items.contains(entry), "{relative}: {entry}");
            } else {
                assert!(
                    own.contains(entry) || generators.contains(entry),
                    "{relative}: {entry}"
                );
            }
        }
    }
}

/// O rastro do original lista as mesmas 86 receitas da classe `MekanismShapedRecipe`, com o serializador `mekanism:mek_data`, e a `assemble` de uma grade vazia já dá o item do resultado.
#[test]
fn the_original_lists_the_same_recipes() {
    let trace = lines("mek_data_recipes.jsonl");
    let totals = trace.last().unwrap();
    assert_eq!(totals["total"], 86);
    let recipes = rastro("mek_data_recipes.json");
    let mut ids = BTreeSet::new();
    for line in &trace[1..trace.len() - 1] {
        assert_eq!(line["serializer"], "mekanism:mek_data");
        let id = line["id"].as_str().unwrap();
        let (namespace, path) = id.split_once(':').unwrap();
        let relative = format!("{namespace}/recipe/{path}.json");
        assert_eq!(
            line["empty_input_assembles_to"], recipes[&relative]["result"],
            "{id}"
        );
        ids.insert(relative);
    }
    assert_eq!(
        ids,
        recipes
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn the_alias_makes_each_recipe_a_shaped_table_recipe_and_touches_nothing_else() {
    let recipes = rastro("mek_data_recipes.json");
    let mut sources: Vec<(String, Value)> = recipes
        .as_object()
        .unwrap()
        .keys()
        .map(|relative| (relative.clone(), json(&recipe_file(relative))))
        .collect();
    let before = sources.clone();
    sources.push((
        "minecraft:plain".into(),
        serde_json::json!({"type": "minecraft:crafting_shaped", "x": 1}),
    ));
    sources.push((
        "mekanism:enriching/x".into(),
        serde_json::json!({"type": "mekanism:enriching"}),
    ));
    sources.push(("mekanism:odd".into(), serde_json::json!("not an object")));
    alias::run(&mut sources);
    assert_eq!(sources.len(), 86 + 3);
    for ((_, after), (_, original)) in sources.iter().zip(&before) {
        assert_eq!(after["type"], "minecraft:crafting_shaped");
        let mut want = original.clone();
        want["type"] = "minecraft:crafting_shaped".into();
        assert_eq!(after, &want);
    }
    assert_eq!(
        sources[86].1,
        serde_json::json!({"type": "minecraft:crafting_shaped", "x": 1})
    );
    assert_eq!(sources[87].1["type"], "mekanism:enriching");
    assert_eq!(sources[88].1, Value::from("not an object"));
}

/// O que a `assemble` do original copia (`mek_data_assemble.jsonl`): a energia somada e limitada à capacidade da saída, e do resto só o dono; a configuração de lados e o
/// ejetor da entrada não vão para o resultado.
#[test]
fn the_energy_is_the_sum_clamped_and_the_side_configuration_is_not_copied() {
    let trace = lines("mek_data_assemble.jsonl");
    let (mut with_energy, mut clamped, mut configured) = (0, 0, 0);
    for line in &trace[1..] {
        let name = line["case"].as_str().unwrap();
        let energies: Vec<i64> = line["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["energy"].as_i64().unwrap())
            .collect();
        let capacity = line["result_capacity"].as_i64().unwrap();
        let total = merge_energy(&energies).unwrap_or(0);
        let result = &line["result"];
        assert_eq!(
            result["energy"].as_i64().unwrap(),
            apply_energy(total, capacity, 0),
            "{name}"
        );
        let patch: Vec<&str> = result["patch"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            patch.contains(&"mekanism:energy"),
            total != 0 && capacity > 0,
            "{name}"
        );
        assert!(
            patch
                .iter()
                .all(|key| ["mekanism:energy", "mekanism:owner"].contains(key)),
            "{name}: {patch:?}"
        );
        if total > 0 {
            with_energy += 1;
        }
        if total > capacity {
            clamped += 1;
        }
        let carried = line["inputs"].as_array().unwrap().iter().any(|i| {
            i["patch"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "mekanism:ejector")
        });
        if carried {
            configured += 1;
        }
    }
    assert!(
        with_energy >= 5 && clamped >= 1 && configured >= 2,
        "{with_energy} {clamped} {configured}"
    );
}
