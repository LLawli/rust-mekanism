//! As tags e as receitas de mesa e de forno do original que citam os itens e blocos de craft e de recurso, e o worldgen deles (`OreType`, `WorldConfig`), contra os
//! rastros que `dev/referencia/craft_rastros.py` tirou da datagen do original (`craft_tags.json`, `craft_recipes.json`, `craft_worldgen.json`).
mod common;

use common::craft::{content, declared, digest, files_in, json, rastro};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Os itens e blocos que o port declara, com o namespace.
fn own_ids() -> BTreeSet<String> {
    declared("item").into_iter().chain(declared("block")).map(|id| format!("mekanism:{id}")).collect()
}

fn values(path: &std::path::Path) -> Vec<String> {
    json(path)["values"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
}

/// As três tags do vanilla que o original lista por inteiro com blocos que o port ainda não tem: o port traz só as entradas que existem (as da lista do original, na
/// ordem dela, e as que ele já tinha achatadas de tags aninhadas, como as máquinas e os cabos em `#mekanism:factory_supported` e `#mekanism:transmitters`).
const PARTIAL: [&str; 3] = [
    "minecraft/tags/block/mineable/pickaxe.json",
    "minecraft/tags/block/blocks_motion_no_leaves.json",
    "minecraft/tags/block/guarded_by_piglins.json",
];

const FLATTENED: [&str; 8] = [
    "mekanism:enrichment_chamber", "mekanism:crusher", "mekanism:metallurgic_infuser", "mekanism:energized_smelter", "mekanism:basic_universal_cable",
    "mekanism:advanced_universal_cable", "mekanism:elite_universal_cable", "mekanism:ultimate_universal_cable",
];

/// Os itens e blocos que o port tem em `ids` e uma tag do original lista: conferidos um a um.
#[test]
fn every_tag_lists_the_ids_the_original_lists() {
    let own = own_ids();
    let tags = rastro("craft_tags.json");
    let mut checked = 0;
    for (relative, original) in tags.as_object().unwrap() {
        let original: Vec<String> = original.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
        let path = content().join("data").join(relative);
        if PARTIAL.contains(&relative.as_str()) {
            let ours = values(&path);
            for id in own.iter().filter(|id| original.contains(id)) {
                assert!(ours.contains(id), "{relative}: falta {id}");
            }
            // O que o original não lista direto e o port tem: as máquinas e os cabos, que o original põe por tags aninhadas (`#mekanism:factory_supported` e
            // `#mekanism:transmitters`) e o port achata na tag do vanilla.
            for entry in ours.iter().filter(|entry| !original.contains(entry)) {
                assert!(relative.ends_with("mineable/pickaxe.json") && FLATTENED.contains(&entry.as_str()), "{relative}: {entry} não está na tag do original");
            }
            checked += 1;
            continue;
        }
        // Tag que cita o que nenhuma frente deste mod tem ainda (cabos aninhados, baldes, barris): fica de fora inteira, sem editar a tag do original.
        let cites_missing = original.iter().any(|entry| {
            if let Some(tag) = entry.strip_prefix('#') {
                let (namespace, rest) = tag.split_once(':').unwrap();
                let kind = if relative.contains("/tags/block/") { "block" } else { "item" };
                namespace == "mekanism" && !content().join("data/mekanism/tags").join(kind).join(format!("{rest}.json")).is_file()
            } else {
                entry.starts_with("mekanism:") && !own.contains(entry)
            }
        });
        if cites_missing {
            assert!(!path.is_file(), "{relative} cita o que o port não tem e não devia estar aqui");
            continue;
        }
        assert!(path.is_file(), "falta a tag {relative}");
        assert_eq!(values(&path), original, "{relative}");
        checked += 1;
    }
    assert!(checked >= 140, "{checked}");
}

/// As receitas de mesa e de forno da datagen: toda a que ficou completa (cada item e tag que cita existe no port ou no vanilla, e o resultado não tem componente
/// que o port não registra) está no port; a que cita o que falta fica de fora, e nenhuma receita do port cita o que o Horizon descartaria em silêncio.
#[test]
fn every_complete_crafting_and_smelting_recipe_is_in_the_content() {
    let recipes = rastro("craft_recipes.json");
    let vanilla = rastro("horizon_vanilla_ids.json");
    let vanilla_items: BTreeSet<&str> = vanilla["items"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let vanilla_tags: BTreeSet<&str> = vanilla["item_tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let own = own_ids();
    // Os componentes de resultado que o port registra (`command_components`/`component_types`): só o tipo de upgrade.
    let known_components = ["mekanism:upgrade_type"];
    let exists = |entry: &str| -> bool {
        if let Some(tag) = entry.strip_prefix('#') {
            let (namespace, rest) = tag.split_once(':').unwrap();
            match namespace {
                "minecraft" => vanilla_tags.contains(entry.trim_start_matches('#')),
                "c" => content().join("data/c/tags/item").join(format!("{rest}.json")).is_file(),
                "mekanism" => content().join("data/mekanism/tags/item").join(format!("{rest}.json")).is_file(),
                other => panic!("tag de {other}"),
            }
        } else if entry.starts_with("minecraft:") {
            vanilla_items.contains(entry)
        } else {
            own.contains(entry)
        }
    };
    let (mut present, mut absent) = (0, Vec::new());
    for (relative, recipe) in recipes.as_object().unwrap() {
        let complete = recipe["refs"].as_array().unwrap().iter().all(|r| exists(r.as_str().unwrap()))
            && exists(recipe["result"].as_str().unwrap())
            && recipe["result_components"].as_array().unwrap().iter().all(|c| known_components.contains(&c.as_str().unwrap()));
        let path = content().join("data").join(relative);
        if complete {
            assert!(path.is_file(), "a receita {relative} ficou completa e não está no port");
            present += 1;
        } else {
            assert!(!path.is_file(), "{relative} cita o que falta e está no port");
            absent.push(relative.as_str());
        }
    }
    assert!(present >= 120, "{present}");
    assert!(absent.len() >= 100, "{}", absent.len());
    // Cada uma é a do original (o hash do rastro de arquivos cobre os bytes; aqui o tipo e o resultado).
    for (relative, recipe) in recipes.as_object().unwrap() {
        let path = content().join("data").join(relative);
        if path.is_file() {
            let ours = json(&path);
            assert_eq!(ours["type"], recipe["type"], "{relative}");
            let result = if ours["result"].is_object() { ours["result"]["id"].clone() } else { ours["result"].clone() };
            assert_eq!(result, recipe["result"], "{relative}");
        }
    }
}

/// O Metallurgic Infuser, o Enrichment Chamber, o Crusher e o Energized Smelter: cada receita dos tipos do Mekanism que o port lê é a do original (mesmo arquivo).
#[test]
fn the_machine_recipes_are_the_originals() {
    let manifest = rastro("craft_datagen.json");
    let manifest = manifest.as_object().unwrap();
    for kind in ["enriching", "crushing", "metallurgic_infusing", "chemical_conversion", "energy_conversion"] {
        let folder = content().join("data/mekanism/recipe").join(kind);
        let files = files_in(&folder);
        assert!(!files.is_empty(), "{kind}");
        for (relative, path) in files {
            let key = format!("data/mekanism/recipe/{kind}/{relative}");
            assert_eq!(manifest.get(&key).and_then(Value::as_str), Some(digest(&path).as_str()), "{key}");
        }
    }
}

/// `OreType` (`OreType.java:18-47`): por minério, as veias com nome, quantidade por chunk, chance de descarte ao ar, tamanho, forma, limites e platô.
struct Vein {
    name: &'static str,
    per_chunk: u64,
    discard: f64,
    size: u64,
    shape: &'static str,
    min: (&'static str, i64),
    max: (&'static str, i64),
    plateau: u64,
}

const fn vein(name: &'static str, per_chunk: u64, discard: f64, size: u64, shape: &'static str, min: (&'static str, i64), max: (&'static str, i64), plateau: u64) -> Vein {
    Vein { name, per_chunk, discard, size, shape, min, max, plateau }
}

fn ores() -> Vec<(&'static str, Vec<Vein>)> {
    vec![
        ("tin", vec![
            vein("small", 14, 0.0, 4, "trapezoid", ("absolute", -20), ("absolute", 94), 0),
            vein("large", 12, 0.0, 9, "trapezoid", ("absolute", -32), ("absolute", 72), 0),
        ]),
        ("osmium", vec![
            vein("upper", 65, 0.0, 7, "trapezoid", ("absolute", 72), ("below_top", -24), 8),
            vein("middle", 6, 0.0, 9, "trapezoid", ("absolute", -32), ("absolute", 56), 0),
            vein("small", 8, 0.0, 4, "uniform", ("above_bottom", 0), ("absolute", 64), 0),
        ]),
        ("uranium", vec![
            vein("small", 4, 0.0, 4, "trapezoid", ("above_bottom", 0), ("absolute", 8), 0),
            vein("buried", 7, 0.75, 9, "trapezoid", ("above_bottom", -24), ("above_bottom", 56), 16),
        ]),
        ("fluorite", vec![
            vein("normal", 5, 0.0, 5, "uniform", ("above_bottom", 0), ("absolute", 23), 0),
            vein("buried", 3, 1.0, 13, "trapezoid", ("above_bottom", 0), ("absolute", 4), 0),
        ]),
        ("lead", vec![vein("normal", 8, 0.25, 9, "trapezoid", ("above_bottom", -24), ("absolute", 64), 0)]),
    ]
}

fn worldgen(relative: &str) -> Value {
    json(&content().join("data/mekanism").join(relative))
}

/// A veia no Horizon: o `mekanism:ore` vira `minecraft:ore` com o tamanho e o descarte da configuração, e os `targets` são os do original; o `mekanism:disableable`
/// sai (a geração e a retrogeração ligadas por padrão, sem a retrogeração), a quantidade e a altura fixas nos valores padrão (`WorldConfig`).
#[test]
fn the_ore_features_are_the_originals_with_the_default_world_config() {
    let original = rastro("craft_worldgen.json");
    let mut count = 0;
    for (ore, veins) in ores() {
        for v in &veins {
            let name = format!("ore_{ore}_{}", v.name);
            let feature = &original[format!("worldgen/feature/{name}.json")];
            let placed = &original[format!("worldgen/placed_feature/{name}.json")];
            assert_eq!(feature["type"], "mekanism:ore", "{name}");
            assert_eq!(feature["ore_type"], json!({"type": ore, "index": veins.iter().position(|x| x.name == v.name).unwrap()}), "{name}");
            assert_eq!(placed["placement"][0]["type"], "mekanism:disableable", "{name}");
            let ours = worldgen(&format!("worldgen/configured_feature/{name}.json"));
            assert_eq!(ours["type"], "minecraft:ore", "{name}");
            assert_eq!(ours["size"], v.size, "{name}: tamanho");
            assert_eq!(ours["discard_chance_on_air_exposure"].as_f64(), Some(v.discard), "{name}: descarte");
            assert_eq!(ours["targets"], feature["targets"], "{name}: alvos");
            let anchor = |(kind, value): (&str, i64)| json!({ kind: value });
            let mut height = json!({"type": format!("minecraft:{}", v.shape), "min_inclusive": anchor(v.min), "max_inclusive": anchor(v.max)});
            if v.plateau > 0 {
                height["plateau"] = json!(v.plateau);
            }
            let expected = json!({
                "feature": format!("mekanism:{name}"),
                "placement": [
                    {"type": "minecraft:count", "count": v.per_chunk},
                    {"type": "minecraft:in_square"},
                    {"type": "minecraft:height_range", "height": height},
                    {"type": "minecraft:biome"},
                ],
            });
            assert_eq!(worldgen(&format!("worldgen/placed_feature/{name}.json")), expected, "{name}");
            count += 1;
        }
        // O biome modifier do minério (`mekanism:<ore>`): as veias dele, na ordem de `OreType`, no passo dos minérios, no overworld (`#mekanism:spawn_ores`).
        let modifier = worldgen(&format!("neoforge/biome_modifier/{ore}.json"));
        let features: Vec<String> = veins.iter().map(|v| format!("mekanism:ore_{ore}_{}", v.name)).collect();
        // O original escreve uma lista, ou o id sozinho quando a lista tem um só (`HolderSet.direct` de um elemento).
        let listed = if features.len() == 1 { json!(features[0]) } else { json!(features) };
        assert_eq!(modifier["features"], listed, "{ore}");
        assert_eq!(modifier["step"], "underground_ores", "{ore}");
        assert_eq!(modifier["type"], "neoforge:add_features", "{ore}");
        assert_eq!(modifier["biomes"], "#minecraft:is_overworld", "{ore}");
        let from_original = &original[format!("neoforge/biome_modifier/{ore}.json")];
        assert_eq!(from_original["features"], listed, "{ore}: ordem do original");
        assert_eq!(from_original["biomes"], "#mekanism:spawn_ores", "{ore}");
    }
    assert_eq!(count, 11 - 1);
}

/// O sal (`ResizableDiskFeature`, `SaltConfig(2 por chunk, raio de 2 a 3, meia-altura 1)`): o disco do vanilla, no fundo da água, onde há terra ou argila.
#[test]
fn the_salt_disk_is_the_original_with_the_default_world_config() {
    let original = rastro("craft_worldgen.json");
    let feature = &original["worldgen/feature/salt.json"];
    assert_eq!(feature["type"], "mekanism:disk");
    let ours = worldgen("worldgen/configured_feature/salt.json");
    assert_eq!(ours["type"], "minecraft:disk");
    assert_eq!(ours["half_height"], 1);
    assert_eq!(ours["radius"], json!({"type": "minecraft:uniform", "min_inclusive": 2, "max_inclusive": 3}));
    assert_eq!(ours["state_provider"], feature["state_provider"]);
    assert_eq!(ours["state_provider"], json!({"id": "mekanism:block_salt"}));
    assert_eq!(ours["target"], feature["target"]);
    let placed = &original["worldgen/placed_feature/salt.json"];
    let mut kept: Vec<Value> = placed["placement"].as_array().unwrap().clone();
    // `mekanism:disableable` sai e a quantidade vira o `perChunk` (2); a altura é a do fundo do mundo (`OCEAN_FLOOR_WG`, a geração normal).
    assert_eq!(kept.remove(0)["type"], "mekanism:disableable");
    kept[0]["count"] = json!(2);
    assert_eq!(worldgen("worldgen/placed_feature/salt.json")["placement"], json!(kept));
    assert_eq!(worldgen("worldgen/placed_feature/salt.json")["feature"], "mekanism:salt");
    let modifier = worldgen("neoforge/biome_modifier/salt.json");
    assert_eq!(modifier["features"], "mekanism:salt");
    assert_eq!(modifier["step"], "underground_ores");
    assert_eq!(modifier["biomes"], "#minecraft:is_overworld");
}
