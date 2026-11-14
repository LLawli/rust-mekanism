//! Os arquivos, o texto e as tags dos blocos de máquina, dos blocos avulsos com tile e dos de armazenamento, indução, multibloco e transmissores: o estado de bloco, a definição do item, os modelos, as texturas e a tabela de drop são os
//! do original (o hash de `rastros/craft_datagen.json` cobre os bytes), nada que eles citam falta, o nome e a descrição são os da datagen, e cada bloco e item está nas tags
//! em que o original o pôs (`BlockStateBase.is`/`Holder.tags()` em jogo, `rastros/blocos.jsonl`).
mod common;

use common::craft::{TIERS5, blocos, cables, content, json, rastro, structure_scope, transmitters};
use mekanism_rules::{transmitter_icon_sprite, transmitter_models};
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
    out.extend(structure_scope());
    out
}

fn energy_cubes() -> Vec<String> {
    TIERS5.map(|tier| format!("{tier}_energy_cube")).to_vec()
}

fn fluid_tanks() -> Vec<String> {
    TIERS5.map(|tier| format!("{tier}_fluid_tank")).to_vec()
}

/// Os arquivos que o port escreve em vez de copiar (decisão 26): o blockstate dos transmissores (e as partes que ele junta) e o blockstate e o item do cubo de energia e do tanque de fluido
/// (o vidro tingido de cada tier), e os modelos e as texturas que eles citam. O item, o modelo, o OBJ e o `.mtl` do transmissor são os do original.
fn generated(relative: &str) -> bool {
    let id = relative
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".json")
        .trim_end_matches(".png");
    let in_set = |ids: Vec<String>| ids.iter().any(|candidate| candidate == id);
    (relative.starts_with("assets/mekanism/blockstates/")
        && (in_set(transmitters()) || in_set(energy_cubes()) || in_set(fluid_tanks())))
        || (relative.starts_with("assets/mekanism/items/")
            && (in_set(energy_cubes()) || in_set(fluid_tanks())))
        || relative.starts_with("assets/mekanism/models/block/transmitter_part/")
        || relative.starts_with("assets/mekanism/models/block/fluid_tank/")
        || (relative.starts_with("assets/mekanism/textures/block/models/fluid_tank_side_")
            && TIERS5.iter().any(|tier| id.ends_with(&format!("_{tier}"))))
        || (relative.starts_with("assets/mekanism/models/block/energy_cube/")
            && (id.ends_with("_static") || id.ends_with("_item")))
        || (relative.starts_with("assets/mekanism/models/item/") && id.ends_with("_port"))
}

/// O que um modelo `neoforge:obj` cita em `model` e `mtl_override`: arquivos, não modelos.
const OBJ_SUFFIXES: [&str; 3] = [".obj", ".obj.mek", ".mtl"];

const ADAPTED: &str = "assets/mekanism/blockstates/qio_redstone_adapter.json";

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
                    ("model" | "parent" | "lit" | "unlit", Value::String(name))
                        if name.contains(':')
                            && !OBJ_SUFFIXES.iter().any(|s| name.ends_with(s)) =>
                    {
                        models.insert(name.clone());
                    }
                    ("holidays", Value::Object(holidays)) => {
                        models.extend(
                            holidays
                                .values()
                                .filter_map(|v| v.as_str().map(str::to_owned)),
                        );
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
        Value::Array(list) => list
            .iter()
            .for_each(|child| references(child, models, textures)),
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
        // O `bounding_block` é só bloco, sem item nem tabela de drop (a datagen não tem nenhuma).
        let mut wanted = vec![format!("assets/mekanism/blockstates/{id}.json")];
        if id != "bounding_block" {
            wanted.push(format!("assets/mekanism/items/{id}.json"));
            wanted.push(format!("data/mekanism/loot_table/blocks/{id}.json"));
        } else {
            for relative in [
                "assets/mekanism/items/bounding_block.json",
                "data/mekanism/loot_table/blocks/bounding_block.json",
            ] {
                assert!(!content().join(relative).exists(), "{relative}");
            }
        }
        for relative in wanted {
            assert!(content().join(&relative).is_file(), "falta {relative}");
            // O blockstate do QIO Redstone Adapter é o do original com `model` nas variantes especiais (teste abaixo); o que o port escreve tem o seu teste.
            assert!(
                files.contains(&relative) || relative == ADAPTED || generated(&relative),
                "{relative} não é o do original"
            );
            seen.insert(relative);
        }
        for relative in [format!("blockstates/{id}.json"), format!("items/{id}.json")] {
            if id == "bounding_block" && relative.starts_with("items") {
                continue;
            }
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
        assert!(
            files.contains(&relative) || generated(&relative),
            "{relative} não é o do original"
        );
        let mut more = BTreeSet::new();
        references(&json(&content().join(&relative)), &mut more, &mut textures);
        pending.extend(more);
    }
    for texture in &textures {
        let Some(path) = texture.strip_prefix("mekanism:") else {
            continue;
        };
        let relative = format!("assets/mekanism/textures/{path}.png");
        assert!(
            content().join(&relative).is_file(),
            "falta a textura {texture}"
        );
        assert!(
            files.contains(&relative) || generated(&relative),
            "{relative} não é a do original"
        );
        let animation = content().join(format!("{relative}.mcmeta"));
        if animation.is_file() {
            assert!(files.contains(&format!("{relative}.mcmeta")));
        }
    }
    assert!(done.len() >= 250, "{} modelos", done.len());
    assert!(textures.len() >= 300, "{} texturas", textures.len());
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
        // A chave `description.*` está no componente `mekanism:description` do item do original (o Cardboard Box e o bloco de apoio não têm).
        let components = rows[&format!("mekanism:{id}")]["item"]["components"]
            .as_array()
            .unwrap();
        let description: Vec<&str> = components
            .iter()
            .filter_map(Value::as_str)
            .filter(|c| c.starts_with("mekanism:description="))
            .collect();
        if id == "cardboard_box" || id == "bounding_block" {
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
        let entry = entry
            .as_str()
            .map_or_else(|| entry["id"].as_str().unwrap(), |s| s);
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
        let root = content()
            .join("data")
            .join(namespace)
            .join("tags")
            .join(kind);
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
    let datagen: BTreeSet<&str> = datagen
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
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
                assert!(
                    lists,
                    "{kind} {full}: o original o põe em #{tag} e o port não"
                );
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

/// O `qio_redstone_adapter` do original tem, no estado ativo, uma variante `mekanism:special/qio_redstone_adapter` sem `model` (`QIORedstoneAdapterModel` escolhe `lit` ou
/// `unlit` pelo dado do modelo da block entity, `EMITTING`), e o Horizon recusa o blockstate inteiro por isso. O port põe `model` com o modelo `unlit`, o que o adaptador
/// sem block entity (que nunca emite) mostra; as chaves do original ficam, e a variante apagada (`active=false`) é a do original.
#[test]
fn the_redstone_adapter_blockstate_is_the_original_plus_the_unlit_model() {
    let state = json(&content().join(ADAPTED));
    let variants = state["variants"].as_object().unwrap();
    assert_eq!(variants.len(), 12);
    let (mut special, mut plain) = (0, 0);
    for (key, variant) in variants {
        if let Some(kind) = variant.get("type") {
            assert_eq!(kind, "mekanism:special/qio_redstone_adapter", "{key}");
            assert!(key.starts_with("active=true"), "{key}");
            assert_eq!(variant["model"], variant["unlit"], "{key}");
            assert_eq!(
                variant["unlit"],
                "mekanism:block/qio_redstone_adapter_unlit"
            );
            assert_eq!(variant["lit"], "mekanism:block/qio_redstone_adapter_lit");
            special += 1;
        } else {
            assert!(key.starts_with("active=false"), "{key}");
            assert_eq!(
                variant["model"],
                "mekanism:block/qio_redstone_adapter_offline"
            );
            plain += 1;
        }
    }
    assert_eq!((special, plain), (6, 6));
}

/// O mesmo JSON com todo número como `f64`: `5` e `5.0` são o mesmo pixel, e o Python escreve um ou outro conforme a conta.
fn numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(list) => Value::Array(list.iter().map(numbers).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), numbers(v))).collect())
        }
        other => other.clone(),
    }
}

/// O transmissor do original é uma variante `mekanism:special/transmitter` que o Horizon recusa. O blockstate do port é um `multipart` com todas as partes, e o que o original mostra sem o dado da block entity (a parte `NONE` de
/// cada lado) é o que vale por padrão; o dado por instância entra pela injeção `transmitter_model_state`: um modelo por lado e por estado visual do `TransmitterModelData`
/// (`VisualConnectionStatus`: `NORMAL`, `PUSH`, `PULL`, `NONE` e as duas partes do trecho reto, estas só onde o `hideContiguousJoin` do blockstate está ligado), cada um com `visibility` das
/// 24 partes do OBJ. A parte do trecho reto troca as texturas de centro pela do braço (`NoneSegmentRemap`) e a girada ainda leva a rotação de `makeIconStatusTransform`. O item, o modelo do
/// corpo, o OBJ e o `.mtl` são cópias.
#[test]
fn the_transmitters_draw_the_obj_of_the_original_part_by_part() {
    let icons = rastro("transmitter_model_parts.json");
    let rastro = rastro("estrutura_modelos.json");
    let files = manifest();
    let sides = ["down", "up", "north", "south", "west", "east"];
    let statuses = ["NONE", "NORMAL", "PUSH", "PULL"];
    // `VisualConnectionStatus.VALUES` e a parte do OBJ que cada um mostra.
    let visual = [
        ("normal", "NORMAL"),
        ("push", "PUSH"),
        ("pull", "PULL"),
        ("none", "NONE"),
        ("none_contiguous", "NONE"),
        ("none_contiguous_rotated", "NONE"),
    ];
    let mut ids = transmitters();
    ids.extend(cables());
    assert_eq!(ids.len(), 22);
    let mut hiding = 0;
    for id in ids {
        let want = &rastro["transmitters"][&id];
        let hide = want["hide_contiguous_join"].as_bool().unwrap();
        hiding += usize::from(hide);
        assert!(
            files.contains(&format!("assets/mekanism/items/{id}.json")),
            "{id}: o item é o do original"
        );
        // O blockstate é um `multipart` com todas as partes, lado e depois estado visual (`lado * partes por lado + estado`, o índice que a injeção `transmitter_model_state` escolhe): só as
        // `NONE` valem por padrão, o que o original mostra sem o dado; as outras levam uma condição que nenhum estado cumpre.
        let state = json(&content().join(format!("assets/mekanism/blockstates/{id}.json")));
        let parts = state["multipart"].as_array().unwrap();
        let per_side = if hide { 6 } else { 4 };
        let glass = want["glass"].as_str();
        assert_eq!(parts.len(), sides.len() * per_side * (1 + usize::from(glass.is_some())), "{id}");
        for (i, side) in sides.iter().enumerate() {
            for (j, (name, _)) in visual.iter().take(per_side).enumerate() {
                let part = &parts[i * per_side + j];
                assert_eq!(
                    part["apply"],
                    serde_json::json!({"model": format!("mekanism:block/transmitter_part/{id}/{side}_{name}")}),
                    "{id}"
                );
                if *name == "none" {
                    assert!(part.get("when").is_none(), "{id} {side}");
                } else {
                    assert_eq!(
                        part["when"],
                        serde_json::json!({"AND": [{*side: "true"}, {*side: "false"}]}),
                        "{id} {side}_{name}"
                    );
                }
            }
        }
        let mut models = 0;
        for side in sides {
            for (name, part) in visual {
                let contiguous = name.starts_with("none_contiguous");
                let path = format!(
                    "assets/mekanism/models/block/transmitter_part/{id}/{side}_{name}.json"
                );
                if contiguous && !hide {
                    assert!(
                        !content().join(&path).exists(),
                        "{id}: sem o `hideContiguousJoin` o modelo não tem {path}"
                    );
                    continue;
                }
                models += 1;
                let model = json(&content().join(&path));
                assert_eq!(model["parent"], want["model"], "{id} {side}_{name}");
                let visibility = model["visibility"].as_object().unwrap();
                assert_eq!(
                    visibility.len(),
                    sides.len() * statuses.len(),
                    "{id} {side}_{name}"
                );
                for (group, shown) in visibility {
                    assert_eq!(
                        shown.as_bool().unwrap(),
                        *group == format!("{side}{part}"),
                        "{id} {side}_{name} {group}"
                    );
                }
                // O trecho reto mostra o centro com a textura do braço (`#center_<lado>` vira `#side`); a girada gira em volta do centro do bloco, como `makeIconStatusTransform`.
                let keys: Vec<&str> = model
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect();
                match name {
                    "none_contiguous" => {
                        assert_eq!(keys, ["parent", "textures", "visibility"], "{id} {side}")
                    }
                    "none_contiguous_rotated" => {
                        assert_eq!(
                            keys,
                            ["parent", "textures", "transform", "visibility"],
                            "{id} {side}"
                        )
                    }
                    _ => assert_eq!(keys, ["parent", "visibility"], "{id} {side}_{name}"),
                }
                if contiguous {
                    let slots = ["down", "up", "north", "south", "east", "west"]
                        .map(|slot| format!("center_{slot}"));
                    let textures = model["textures"].as_object().unwrap();
                    assert_eq!(textures.len(), slots.len(), "{id} {side}_{name}");
                    for slot in slots {
                        assert_eq!(textures[&slot], "#side", "{id} {side}_{name} {slot}");
                    }
                }
                if name == "none_contiguous_rotated" {
                    let icon = icons["icon_transforms"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|icon| icon["side"] == side)
                        .unwrap();
                    assert_eq!(
                        model["transform"],
                        serde_json::json!({"rotation": icon["quaternion"], "origin": "center"}),
                        "{id} {side}"
                    );
                }
            }
        }
        assert_eq!(models, if hide { 36 } else { 24 }, "{id}");
        // O vidro (`glass` do blockstate do original, só no transportador logístico): as mesmas partes depois das do corpo, cada uma com o vidro por pai, as texturas de centro do vidro
        // trocadas pela do braço (`#side_glass`, `NoneSegmentRemap`) no trecho reto e a mesma rotação da girada; tudo o que o blockstate cita existe.
        if let Some(glass) = glass {
            assert!(hide, "{id}");
            for (i, side) in sides.iter().enumerate() {
                for (j, (name, _)) in visual.iter().enumerate() {
                    let entry = &parts[sides.len() * per_side + i * per_side + j];
                    assert_eq!(
                        entry["apply"],
                        serde_json::json!({"model": format!("mekanism:block/transmitter_part/{id}/glass_{side}_{name}")}),
                        "{id}"
                    );
                    assert_eq!(entry["when"], serde_json::json!({"AND": [{*side: "true"}, {*side: "false"}]}), "{id} glass_{side}_{name}");
                    let model = json(&content().join(format!("assets/mekanism/models/block/transmitter_part/{id}/glass_{side}_{name}.json")));
                    assert_eq!(model["parent"], glass, "{id} glass_{side}_{name}");
                    let part = visual[j].1;
                    for (group, shown) in model["visibility"].as_object().unwrap() {
                        assert_eq!(shown.as_bool().unwrap(), *group == format!("{side}{part}"), "{id} glass_{side}_{name} {group}");
                    }
                    if name.starts_with("none_contiguous") {
                        let textures = model["textures"].as_object().unwrap();
                        assert_eq!(textures.len(), 6, "{id}");
                        for slot in ["down", "up", "north", "south", "east", "west"] {
                            assert_eq!(textures[&format!("center_glass_{slot}")], "#side_glass", "{id} {side}_{name}");
                        }
                    } else {
                        assert!(model.get("textures").is_none(), "{id} glass_{side}_{name}");
                    }
                    assert_eq!(model.get("transform").is_some(), *name == "none_contiguous_rotated", "{id} glass_{side}_{name}");
                }
            }
            let model = json(&content().join("assets/mekanism/models/block/transmitter/large/logistical_transporter/transporter_glass.json"));
            assert_eq!(model["loader"], "neoforge:obj");
            for file in [
                "assets/mekanism/models/block/transmitter_glass_large.mtl",
                "assets/mekanism/models/block/transporter_box.json",
                "assets/mekanism/textures/block/transporter_box.png",
                "assets/mekanism/textures/block/models/multipart/logistical_transporter_glass_colored.png",
                "assets/mekanism/textures/block/models/multipart/logistical_transporter_vertical_glass_colored.png",
                "assets/mekanism/textures/block/models/multipart/opaque/logistical_transporter_glass_colored.png",
                "assets/mekanism/textures/block/models/multipart/opaque/logistical_transporter_vertical_glass_colored.png",
            ] {
                assert!(files.contains(file), "{file}");
            }
        }
        // O corpo herda um modelo `neoforge:obj` do original; o OBJ tem as 24 partes e o `.mtl` que ele nomeia está ao lado.
        let mut model = want["model"].as_str().unwrap().to_owned();
        let obj = loop {
            let path = format!(
                "assets/mekanism/models/{}.json",
                model.strip_prefix("mekanism:").unwrap()
            );
            assert!(files.contains(&path), "{path}");
            let data = json(&content().join(&path));
            if data["loader"] == "neoforge:obj" {
                break data;
            }
            model = data["parent"].as_str().unwrap().to_owned();
        };
        let obj_path = format!(
            "assets/mekanism/{}",
            obj["model"]
                .as_str()
                .unwrap()
                .strip_prefix("mekanism:")
                .unwrap()
        );
        assert!(files.contains(&obj_path), "{obj_path}");
        let text = std::fs::read_to_string(content().join(&obj_path)).unwrap();
        let groups: BTreeSet<&str> = text.lines().filter_map(|l| l.strip_prefix("o ")).collect();
        let expected: BTreeSet<String> = sides
            .iter()
            .flat_map(|s| statuses.iter().map(move |t| format!("{s}{t}")))
            .collect();
        assert_eq!(
            groups,
            expected.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            "{id}"
        );
        let mtl = text
            .lines()
            .find_map(|l| l.strip_prefix("mtllib "))
            .unwrap();
        let mtl_path = obj_path.rsplit_once('/').unwrap().0.to_owned() + "/" + mtl;
        assert!(files.contains(&mtl_path), "{mtl_path}");
    }
    // Só o desviador desliga o `hideContiguousJoin`.
    assert_eq!(hiding, 21);
}

/// O cubo de energia: o `EnergyCubeModel` junta o quadro e, por lado, o LED apagado (sem block entity todo lado é `INACTIVE`); o item (`RenderEnergyCubeItem`) usa a configuração de
/// lados do item (`FRONT_OUT_EJECT`: entrada em todo lado e saída na frente; `ALL_OUTPUT` no criativo): a porta em todo lado e o LED aceso (as mesmas caixas com o uv deslocado e a luz em 15) onde sai. O blockstate gira o modelo como o original.
#[test]
fn the_energy_cubes_are_drawn_with_the_frame_the_leds_and_the_ports_of_the_original() {
    let rastro = rastro("estrutura_modelos.json");
    let cube = &rastro["energy_cube"];
    let sides = ["front", "left", "right", "back", "top", "bottom"];
    let shift: Vec<f64> = cube["leds"]["lit_uv_shift"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    for tier in TIERS5 {
        let block = format!("{tier}_energy_cube");
        let mut textures = cube["textures"].as_object().unwrap().clone();
        textures.extend(cube["tiers"][tier]["textures"].as_object().unwrap().clone());
        let mut resting: Vec<Value> = cube["frame"].as_array().unwrap().clone();
        let mut carried = resting.clone();
        for side in sides {
            resting.extend(cube["leds"][side].as_array().unwrap().clone());
            // O item: `SIDE_CONFIG` tem `INPUT` em todo lado e `OUTPUT` na frente (`ALL_OUTPUT` no criativo): a porta em todos, o LED aceso só nos que saem.
            if side == "front" || tier == "creative" {
                for led in cube["leds"][side].as_array().unwrap() {
                    let mut lit = led.clone();
                    for face in lit["faces"].as_object_mut().unwrap().values_mut() {
                        if let Some(uv) = face.get_mut("uv") {
                            let values: Vec<f64> = uv
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_f64().unwrap())
                                .collect();
                            *uv = serde_json::json!([
                                values[0] + shift[0],
                                values[1] + shift[1],
                                values[2] + shift[0],
                                values[3] + shift[1]
                            ]);
                        }
                    }
                    lit["light_emission"] = 15.into();
                    carried.push(lit);
                }
            } else {
                carried.extend(cube["leds"][side].as_array().unwrap().clone());
            }
            carried.extend(cube["ports"][side].as_array().unwrap().clone());
        }
        let models = content().join("assets/mekanism/models");
        let at_rest = json(&models.join(format!("block/energy_cube/{tier}_static.json")));
        assert_eq!(
            numbers(&at_rest["elements"]),
            numbers(&Value::Array(resting)),
            "{block}"
        );
        assert_eq!(
            at_rest["textures"],
            Value::Object(textures.clone()),
            "{block}"
        );
        let held = json(&models.join(format!("block/energy_cube/{tier}_item.json")));
        assert_eq!(
            numbers(&held["elements"]),
            numbers(&Value::Array(carried)),
            "{block}"
        );
        assert_eq!(held["textures"], Value::Object(textures), "{block}");
        // O item: o modelo da mão com o `display` do modelo base do original.
        let item = json(&models.join(format!("item/{block}_port.json")));
        assert_eq!(
            item["parent"],
            format!("mekanism:block/energy_cube/{tier}_item")
        );
        assert_eq!(
            numbers(&item["display"]),
            numbers(&cube["display"][tier]),
            "{block}"
        );
        let definition = json(&content().join(format!("assets/mekanism/items/{block}.json")));
        assert_eq!(definition["model"]["type"], "minecraft:model");
        assert_eq!(
            definition["model"]["model"],
            format!("mekanism:item/{block}_port")
        );
        // O blockstate: o modelo parado girado para cada `facing`, na ordem alfabética do original.
        let state = json(&content().join(format!("assets/mekanism/blockstates/{block}.json")));
        let variants = state["variants"].as_object().unwrap();
        let keys: Vec<&str> = variants.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "facing=down",
                "facing=east",
                "facing=north",
                "facing=south",
                "facing=up",
                "facing=west"
            ]
        );
        let rotations = [
            ("down", 0, 90),
            ("east", 90, 0),
            ("north", 0, 0),
            ("south", 180, 0),
            ("up", 0, 270),
            ("west", 270, 0),
        ];
        for (facing, y, x) in rotations {
            let variant = &variants[&format!("facing={facing}")];
            assert_eq!(
                variant["model"],
                format!("mekanism:block/energy_cube/{tier}_static")
            );
            assert_eq!(
                variant.get("y").and_then(Value::as_i64).unwrap_or(0),
                y,
                "{block} {facing}"
            );
            assert_eq!(
                variant.get("x").and_then(Value::as_i64).unwrap_or(0),
                x,
                "{block} {facing}"
            );
        }
    }
}

/// O tanque de fluido de cada tier: o blockstate e os dois modelos (parado e ativo) usam o vidro já multiplicado pela cor do tier (`ClientRegistration.registerBlockColorHandlers`, a cor de
/// `colormap/tiers.png`), que o Horizon não tinge; o item sem fluido (`RenderFluidTankItem` desenha o modelo do estado padrão) é o modelo parado com o `display` do modelo base do original.
#[test]
fn the_fluid_tanks_use_the_glass_of_their_tier_and_the_items_show_the_block_model() {
    let rastro = rastro("estrutura_modelos.json");
    let colors = rastro["tier_colors"].as_object().unwrap();
    assert_eq!(colors.len(), 5);
    let mut seen = std::collections::BTreeSet::new();
    for tier in TIERS5 {
        // As cores dos cinco tiers são distintas: um tier sem cor própria deixaria o vidro de dois tanques igual.
        assert!(seen.insert(colors[tier].to_string()), "{tier}");
        let block = format!("{tier}_fluid_tank");
        let definition = json(&content().join(format!("assets/mekanism/items/{block}.json")));
        assert_eq!(definition["model"]["type"], "minecraft:model", "{block}");
        assert_eq!(
            definition["model"]["model"],
            format!("mekanism:item/{block}_port")
        );
        let item = json(&content().join(format!("assets/mekanism/models/item/{block}_port.json")));
        assert_eq!(
            item["parent"],
            format!("mekanism:block/fluid_tank/{tier}"),
            "{block}"
        );
        assert_eq!(
            numbers(&item["display"]),
            numbers(&rastro["fluid_tank_display"][tier]),
            "{block}"
        );
        let state = json(&content().join(format!("assets/mekanism/blockstates/{block}.json")));
        assert_eq!(
            state["variants"]["active=false"]["model"],
            format!("mekanism:block/fluid_tank/{tier}")
        );
        assert_eq!(
            state["variants"]["active=true"]["model"],
            format!("mekanism:block/fluid_tank/{tier}_active")
        );
        for (suffix, parent, glass) in [
            ("", "mekanism:block/fluid_tank", "off"),
            ("_active", "mekanism:block/fluid_tank_active", "on"),
        ] {
            let model = json(&content().join(format!(
                "assets/mekanism/models/block/fluid_tank/{tier}{suffix}.json"
            )));
            assert_eq!(model["parent"], parent, "{block}");
            let texture = format!("mekanism:block/models/fluid_tank_side_{glass}_{tier}");
            assert_eq!(model["textures"]["glass"], texture, "{block}");
            // A textura do tier existe e tem o tamanho da do original (o IHDR do PNG: largura e altura nos bytes 16 a 24); os pixels são conferidos por `test_blocos_conteudo.py`.
            let baked = std::fs::read(content().join(format!(
                "assets/mekanism/textures/block/models/fluid_tank_side_{glass}_{tier}.png"
            )))
            .unwrap();
            let original = std::fs::read(content().join(format!(
                "assets/mekanism/textures/block/models/fluid_tank_side_{glass}.png"
            )))
            .unwrap();
            assert_eq!(baked[16..24], original[16..24], "{block}");
            assert_ne!(
                baked, original,
                "{block}: a textura do tier é a do original multiplicada pela cor"
            );
        }
    }
}

/// A tabela que o jogo usa para pedir as partes (`transmitter_models`) é a dos 22 transmissores: o prefixo dos modelos que o port escreve, o `hideContiguousJoin` do blockstate do original e o
/// dado do desviador (`TileEntityDiversionTransporter.initModelData`).
#[test]
fn the_model_table_of_the_game_lists_the_22_transmitters_with_what_the_original_says() {
    let rastro = rastro("estrutura_modelos.json");
    let table = transmitter_models();
    let mut ids = transmitters();
    ids.extend(cables());
    assert_eq!(table.len(), 22);
    assert_eq!(table.len(), ids.len());
    for id in ids {
        let row = table
            .iter()
            .find(|row| row.block == format!("mekanism:{id}"))
            .unwrap_or_else(|| panic!("{id}"));
        assert_eq!(row.prefix, format!("mekanism:block/transmitter_part/{id}"));
        assert_eq!(
            row.hide_contiguous_join,
            rastro["transmitters"][&id]["hide_contiguous_join"]
                .as_bool()
                .unwrap(),
            "{id}"
        );
        assert_eq!(row.diversion, id == "diversion_transporter", "{id}");
        // O vidro é o `glass` do blockstate do original: só os quatro transportadores logísticos.
        assert_eq!(row.glass, !rastro["transmitters"][&id]["glass"].is_null(), "{id}");
    }
}

/// `TransmitterTypeDecorator.registerDecorators` (`ClientRegistration.registerItemDecorations`): os 12 itens dos tubos pressurizados, dos condutores termodinâmicos e dos cabos universais
/// (os quatro tiers de cada) levam o sprite `transmitter_icon/<id>`; os canos, os transportadores e o resto não. Os 12 PNG são os do original (o hash de `craft_datagen.json`).
#[test]
fn the_transmitter_items_of_the_original_carry_their_type_icon() {
    let decorated = [
        "basic_pressurized_tube", "advanced_pressurized_tube", "elite_pressurized_tube", "ultimate_pressurized_tube",
        "basic_thermodynamic_conductor", "advanced_thermodynamic_conductor", "elite_thermodynamic_conductor", "ultimate_thermodynamic_conductor",
        "basic_universal_cable", "advanced_universal_cable", "elite_universal_cable", "ultimate_universal_cable",
    ];
    let files = manifest();
    for id in decorated {
        assert_eq!(transmitter_icon_sprite(&format!("mekanism:{id}")), Some(format!("mekanism:gui/sprites/transmitter_icon/{id}")), "{id}");
        let path = format!("assets/mekanism/textures/gui/sprites/transmitter_icon/{id}.png");
        assert!(content().join(&path).is_file(), "{path}");
        assert!(files.contains(&path), "{path} não é o do original");
    }
    let icons = std::fs::read_dir(content().join("assets/mekanism/textures/gui/sprites/transmitter_icon")).unwrap().count();
    assert_eq!(icons, decorated.len());
    for other in transmitters().into_iter().chain(cables()).filter(|id| !decorated.contains(&id.as_str())) {
        assert_eq!(transmitter_icon_sprite(&format!("mekanism:{other}")), None, "{other}");
    }
    assert_eq!(transmitter_icon_sprite("minecraft:stick"), None);
    assert_eq!(transmitter_icon_sprite("mekanism:basic_energy_cube"), None);
}
