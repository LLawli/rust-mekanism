//! Os arquivos, o texto e as tags dos blocos de máquina e dos blocos avulsos com tile: o estado de bloco, a definição do item, os modelos, as texturas e a tabela de drop são os
//! do original (o hash de `rastros/craft_datagen.json` cobre os bytes), nada que eles citam falta, o nome e a descrição são os da datagen, e cada bloco e item está nas tags
//! em que o original o pôs (`BlockStateBase.is`/`Holder.tags()` em jogo, `rastros/blocos.jsonl`).
mod common;

use common::craft::{blocos, content, json, rastro};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MACHINES: &str = "antiprotonic_nucleosynthesizer chemical_crystallizer chemical_dissolution_chamber chemical_infuser chemical_injection_chamber chemical_oxidizer \
chemical_washer combiner digital_miner dimensional_stabilizer electric_pump electrolytic_separator fluidic_plenisher formulaic_assemblicator fuelwood_heater \
isotopic_centrifuge laser laser_amplifier laser_tractor_beam logistical_sorter modification_station nutritional_liquifier oredictionificator osmium_compressor \
painting_machine pigment_extractor pigment_mixer precision_sawmill pressurized_reaction_chamber purification_chamber qio_dashboard qio_drive_array qio_exporter \
qio_importer qio_redstone_adapter quantum_entangloporter resistive_heater rotary_condensentrator security_desk seismic_vibrator solar_neutron_activator teleporter \
chargepad industrial_alarm radioactive_waste_barrel personal_barrel personal_chest cardboard_box";

fn scope() -> Vec<String> {
    let mut out: Vec<String> = MACHINES.split_whitespace().map(str::to_owned).collect();
    for tier in ["basic", "advanced", "elite", "ultimate"] {
        for kind in [
            "combining",
            "compressing",
            "crushing",
            "enriching",
            "infusing",
            "injecting",
            "purifying",
            "sawing",
            "smelting",
        ] {
            out.push(format!("{tier}_{kind}_factory"));
        }
    }
    out
}

fn manifest() -> BTreeSet<String> {
    rastro("craft_datagen.json")
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

/// Os modelos que um blockstate, uma definição de item ou um modelo cita (`model`, `parent`, `holidays`) e as texturas de um modelo.
fn references(value: &Value, models: &mut BTreeSet<String>, textures: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                match (key.as_str(), child) {
                    ("model" | "parent", Value::String(name)) if name.contains(':') => {
                        models.insert(name.clone());
                    }
                    ("holidays", Value::Object(holidays)) => {
                        models.extend(holidays.values().filter_map(|v| v.as_str().map(str::to_owned)));
                    }
                    ("textures", Value::Object(map)) => {
                        textures.extend(
                            map.values()
                                .filter_map(Value::as_str)
                                .filter(|t| !t.starts_with('#'))
                                .map(str::to_owned),
                        );
                    }
                    _ => {}
                }
                references(child, models, textures);
            }
        }
        Value::Array(list) => list.iter().for_each(|child| references(child, models, textures)),
        _ => {}
    }
}

#[test]
fn the_files_of_every_block_are_the_originals_and_nothing_they_cite_is_missing() {
    let files = manifest();
    let assets = content().join("assets/mekanism");
    let mut seen = BTreeSet::new();
    let mut models = BTreeSet::new();
    let mut textures = BTreeSet::new();
    for id in scope() {
        for relative in [
            format!("assets/mekanism/blockstates/{id}.json"),
            format!("assets/mekanism/items/{id}.json"),
            format!("data/mekanism/loot_table/blocks/{id}.json"),
        ] {
            assert!(content().join(&relative).is_file(), "falta {relative}");
            assert!(files.contains(&relative), "{relative} não é o do original");
            seen.insert(relative);
        }
        for relative in [
            format!("blockstates/{id}.json"),
            format!("items/{id}.json"),
        ] {
            references(&json(&assets.join(relative)), &mut models, &mut textures);
        }
    }
    // O fecho dos modelos pelos pais, e as texturas de cada um.
    let mut pending: Vec<String> = models.iter().cloned().collect();
    let mut done = BTreeSet::new();
    while let Some(name) = pending.pop() {
        let (namespace, path) = name.split_once(':').unwrap();
        if namespace != "mekanism" || !done.insert(name.clone()) {
            continue;
        }
        let relative = format!("assets/mekanism/models/{path}.json");
        assert!(content().join(&relative).is_file(), "falta o modelo {name}");
        assert!(files.contains(&relative), "{relative} não é o do original");
        let mut more = BTreeSet::new();
        references(&json(&content().join(&relative)), &mut more, &mut textures);
        pending.extend(more);
    }
    for texture in &textures {
        let Some(path) = texture.strip_prefix("mekanism:") else {
            continue;
        };
        let relative = format!("assets/mekanism/textures/{path}.png");
        assert!(content().join(&relative).is_file(), "falta a textura {texture}");
        assert!(files.contains(&relative), "{relative} não é a do original");
        let animation = content().join(format!("{relative}.mcmeta"));
        if animation.is_file() {
            assert!(files.contains(&format!("{relative}.mcmeta")));
        }
    }
    assert!(done.len() >= 150, "{} modelos", done.len());
    assert!(textures.len() >= 200, "{} texturas", textures.len());
}

#[test]
fn every_block_has_the_name_and_the_description_of_the_datagen() {
    let lang = json(&content().join("assets/mekanism/lang/en_us.json"));
    let original = rastro("craft_lang.json");
    let rows: BTreeMap<String, Value> = blocos()
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), row))
        .collect();
    for id in scope() {
        let name = format!("block.mekanism.{id}");
        assert!(lang.get(&name).is_some(), "falta {name}");
        assert_eq!(lang[&name], original[&name], "{name}");
        // A chave `description.*` está no componente `mekanism:description` do item do original (o Cardboard Box não tem).
        let components = rows[&format!("mekanism:{id}")]["item"]["components"]
            .as_array()
            .unwrap();
        let description: Vec<&str> = components
            .iter()
            .filter_map(Value::as_str)
            .filter(|c| c.starts_with("mekanism:description="))
            .collect();
        if id == "cardboard_box" {
            assert!(description.is_empty());
            continue;
        }
        assert_eq!(description.len(), 1, "{id}");
        let key = description[0]
            .split("key='")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .unwrap();
        assert!(lang.get(key).is_some(), "{id}: falta {key}");
        assert_eq!(lang[key], original[key], "{key}");
    }
}

/// Os ids de uma tag do port com as tags aninhadas do port resolvidas.
fn members(kind: &str, namespace: &str, path: &str, out: &mut BTreeSet<String>, depth: u8) {
    let file = content()
        .join("data")
        .join(namespace)
        .join("tags")
        .join(kind)
        .join(format!("{path}.json"));
    if depth > 8 || !file.is_file() {
        return;
    }
    for entry in json(&file)["values"].as_array().unwrap() {
        let entry = entry.as_str().map_or_else(|| entry["id"].as_str().unwrap(), |s| s);
        if let Some(tag) = entry.strip_prefix('#') {
            let (namespace, path) = tag.split_once(':').unwrap();
            members(kind, namespace, path, out, depth + 1);
        } else {
            out.insert(entry.to_owned());
        }
    }
}

fn tag_files(kind: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for namespace in ["c", "minecraft", "mekanism"] {
        let root = content().join("data").join(namespace).join("tags").join(kind);
        if !root.is_dir() {
            continue;
        }
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .trim_end_matches(".json")
                            .to_owned(),
                    );
                }
            }
        }
        let mut paths = Vec::new();
        walk(&root, &root, &mut paths);
        out.extend(paths.into_iter().map(|p| (namespace.to_owned(), p)));
    }
    out
}

/// O que o original em jogo diz de cada tag (`Holder.tags()`) e o que o port diz, nos dois sentidos: toda tag que o original põe no bloco e que a datagen escreve (e não o
/// vanilla nem o NeoForge por conta própria) lista o bloco no port, e toda tag do port que lista o bloco é uma tag do original para ele. Vale para as tags de bloco e de item.
#[test]
fn every_block_and_item_is_in_the_tags_the_original_puts_it_in() {
    let datagen = rastro("craft_tags.json");
    let datagen: BTreeSet<&str> = datagen.as_object().unwrap().keys().map(String::as_str).collect();
    let rows: BTreeMap<String, Value> = blocos()
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), row))
        .collect();
    let mut checked = 0;
    for (kind, original_tags) in [("block", "tags"), ("item", "tags")] {
        let mut port: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (namespace, path) in tag_files(kind) {
            let mut ids = BTreeSet::new();
            members(kind, &namespace, &path, &mut ids, 0);
            port.insert(format!("{namespace}:{path}"), ids);
        }
        for id in scope() {
            let full = format!("mekanism:{id}");
            let row = &rows[&full];
            let original: BTreeSet<String> = if kind == "block" {
                row[original_tags].as_array().unwrap()
            } else {
                row["item"][original_tags].as_array().unwrap()
            }
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
            for tag in &original {
                let (namespace, path) = tag.split_once(':').unwrap();
                let key = format!("{namespace}/tags/{kind}/{path}.json");
                if !datagen.contains(key.as_str()) {
                    continue;
                }
                let lists = port.get(tag).is_some_and(|ids| ids.contains(&full));
                assert!(lists, "{kind} {full}: o original o põe em #{tag} e o port não");
                checked += 1;
            }
            for (tag, ids) in &port {
                if ids.contains(&full) {
                    assert!(
                        original.contains(tag),
                        "{kind} {full}: o port o põe em #{tag} e o original não"
                    );
                }
            }
        }
    }
    assert!(checked >= 300, "{checked}");
}
