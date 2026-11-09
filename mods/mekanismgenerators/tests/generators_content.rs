//! Os arquivos, o texto, as tags e as receitas do conteúdo do Generators sem função: o estado de bloco, a definição do item, os modelos, as texturas e a tabela de drop são os
//! do original (o hash de `rastros/generators_datagen.json` cobre os bytes), nada que eles citam falta, o nome e a descrição são os da datagen, cada bloco e item está nas tags
//! em que o original o pôs (`Holder.tags()` em jogo, `rastros/blocos_generators.jsonl`) e cada receita da datagen que ficou completa existe no port.
#[path = "common/generators.rs"]
mod generators;

use generators::{
    ITEMS, blocos, content, datagen, declared, digest, itens, json, mekanism_content, scope,
};
use machinery_basis_core::placement::Rule;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const NS: &str = "mekanismgenerators";

/// Os modelos que um blockstate, uma definição de item ou um modelo cita (`model`, `parent`) e as texturas de um modelo.
fn references(value: &Value, models: &mut BTreeSet<String>, textures: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                match (key.as_str(), child) {
                    ("model" | "parent" | "lit" | "unlit", Value::String(name))
                        if name.contains(':') =>
                    {
                        models.insert(name.clone());
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

fn asset_file(namespace: &str, relative: &str) -> std::path::PathBuf {
    let root = if namespace == NS {
        content()
    } else {
        mekanism_content()
    };
    root.join("assets").join(namespace).join(relative)
}

#[test]
fn the_files_of_every_block_and_item_are_the_originals_and_nothing_they_cite_is_missing() {
    let datagen = datagen();
    let manifest = datagen["files"].as_object().unwrap();
    let mut models = BTreeSet::new();
    let mut textures = BTreeSet::new();
    let mut ids: Vec<(String, &str)> = scope().into_iter().map(|id| (id, "block")).collect();
    ids.extend(ITEMS.iter().map(|id| ((*id).to_owned(), "item")));
    for (id, kind) in &ids {
        let mut files = vec![format!("assets/{NS}/items/{id}.json")];
        if kind == &"block" {
            files.push(format!("assets/{NS}/blockstates/{id}.json"));
            files.push(format!("data/{NS}/loot_table/blocks/{id}.json"));
        }
        for relative in files {
            assert!(content().join(&relative).is_file(), "falta {relative}");
            assert_eq!(
                manifest.get(&relative).and_then(Value::as_str),
                Some(digest(&content().join(&relative)).as_str()),
                "{relative} não é o do original"
            );
            if relative.starts_with("assets/") {
                references(
                    &json(&content().join(&relative)),
                    &mut models,
                    &mut textures,
                );
            }
        }
    }
    // O que só o renderer do bloco de entidade lê (Fase 6): o modelo das pás do Wind Generator (`RenderWindGenerator`) e a textura da turbina (`ModelTurbine`).
    models.insert("mekanismgenerators:block/wind_generator_blades".to_owned());
    let turbine = content().join(format!("assets/{NS}/render/turbine.png"));
    assert_eq!(
        manifest
            .get(&format!("assets/{NS}/render/turbine.png"))
            .and_then(Value::as_str),
        Some(digest(&turbine).as_str())
    );
    // O fecho dos modelos pelos pais, e as texturas de cada um (e o `.obj` e o `.mtl` de um modelo `neoforge:obj`).
    let mut pending: Vec<String> = models.iter().cloned().collect();
    let mut done = BTreeSet::new();
    let mut objs = BTreeSet::new();
    while let Some(name) = pending.pop() {
        let (namespace, path) = name.split_once(':').unwrap();
        if !matches!(namespace, "mekanism" | NS) || !done.insert(name.clone()) {
            continue;
        }
        if path.ends_with(".obj") {
            objs.insert((namespace.to_owned(), path.to_owned()));
            continue;
        }
        let file = asset_file(namespace, &format!("models/{path}.json"));
        assert!(file.is_file(), "falta o modelo {name}");
        let model = json(&file);
        let mut more = BTreeSet::new();
        references(&model, &mut more, &mut textures);
        pending.extend(more);
        if namespace == NS {
            let relative = format!("assets/{NS}/models/{path}.json");
            assert_eq!(
                manifest.get(&relative).and_then(Value::as_str),
                Some(digest(&file).as_str()),
                "{relative} não é o do original"
            );
        }
    }
    assert_eq!(done.len(), 35);
    assert!(
        datagen.get("derived").is_none(),
        "a opção B da decisão 31 saiu: nenhum arquivo é derivado"
    );
    for (namespace, path) in &objs {
        // `models/block/x.obj` está em `assets/<ns>/models/block/x.obj`; o `.mtl` ao lado.
        for file in [path.clone(), path.replace(".obj", ".mtl")] {
            let found = asset_file(namespace, &file);
            assert!(found.is_file(), "falta {}", found.display());
            if namespace != NS {
                continue;
            }
            let relative = format!("assets/{NS}/{file}");
            // O `.obj` e o `.mtl` são a cópia do original: o `transform` do modelo (a translação do `wind_generator`) quem aplica é a injeção `model_transform_*`.
            assert_eq!(
                manifest.get(&relative).and_then(Value::as_str),
                Some(digest(&found).as_str()),
                "{file}"
            );
        }
    }
    // O modelo diz a translação, como o original (o Horizon a aplica pela injeção).
    let model = json(&content().join(format!("assets/{NS}/models/block/wind_generator.json")));
    assert_eq!(
        model["children"]["main"]["transform"]["translation"],
        serde_json::json!([0.5, 0, 0.5])
    );
    assert_eq!(objs.len(), 2, "o wind_generator cita dois .obj");
    for texture in &textures {
        let (namespace, path) = texture.split_once(':').unwrap();
        if !matches!(namespace, "mekanism" | NS) {
            continue;
        }
        let file = asset_file(namespace, &format!("textures/{path}.png"));
        assert!(file.is_file(), "falta a textura {texture}");
        if namespace == NS {
            let relative = format!("assets/{NS}/textures/{path}.png");
            assert_eq!(
                manifest.get(&relative).and_then(Value::as_str),
                Some(digest(&file).as_str()),
                "{relative}"
            );
        }
    }
}

/// O nome do bloco e do item, a chave `description.*` (a do componente `mekanism:description` do item do original) e o título da aba são os da datagen.
#[test]
fn the_names_and_descriptions_are_the_datagens() {
    let datagen = datagen();
    let original = datagen["lang"].as_object().unwrap();
    let lang = json(&content().join(format!("assets/{NS}/lang/en_us.json")));
    let rows: BTreeMap<String, Value> = blocos()
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), row))
        .collect();
    assert_eq!(rows.len(), 24);
    for (full, row) in &rows {
        let id = full.strip_prefix("mekanismgenerators:").unwrap();
        let name = format!("block.{NS}.{id}");
        assert_eq!(lang[&name], original[&name], "{name}");
        let description: Vec<&str> = row["item"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|c| c.starts_with("mekanism:description="))
            .collect();
        assert_eq!(description.len(), 1, "{id}");
        let key = description[0]
            .split("key='")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .unwrap();
        assert_eq!(key, format!("description.{NS}.{id}"));
        assert_eq!(lang[key], original[key], "{key}");
    }
    for item in ITEMS {
        let name = format!("item.{NS}.{item}");
        assert_eq!(lang[&name], original[&name], "{name}");
    }
    let title = format!("constants.{NS}.mod_name");
    assert_eq!(lang[&title], "Mekanism: Generators");
    assert_eq!(lang[&title], original[&title]);
}

/// Os três itens simples: o item é o gabarito do ferro, a pilha e a raridade são as do original (o Hohlraum empilha 1), a definição do item e o modelo existem.
#[test]
fn the_three_plain_items_are_registered_as_the_original_does() {
    let rastro = itens();
    for item in ITEMS {
        let row = &rastro[item];
        let declaration = json(&content().join(format!("data/{NS}/horizon/item/{item}.json")));
        assert_eq!(declaration["template"], "minecraft:iron_nugget", "{item}");
        assert_eq!(row["rarity"], "COMMON", "{item}");
        assert_eq!(
            declaration["max_stack_size"].as_u64().unwrap_or(64),
            row["max_stack"].as_u64().unwrap(),
            "{item}: pilha"
        );
        let model = json(&content().join(format!("assets/{NS}/items/{item}.json")));
        assert_eq!(
            model["model"]["model"],
            format!("mekanismgenerators:item/{item}")
        );
        assert!(
            content()
                .join(format!("assets/{NS}/models/item/{item}.json"))
                .is_file()
        );
        assert!(
            content()
                .join(format!("assets/{NS}/textures/item/{item}.png"))
                .is_file()
        );
    }
    assert_eq!(rastro["hohlraum"]["max_stack"], 1);
    // Só os sete itens que não são de bloco existem no registro: os três e os quatro baldes.
    assert_eq!(rastro.len(), 7);
    assert_eq!(
        declared("item").len(),
        ITEMS.len() + 4,
        "os itens declarados: os três e os quatro baldes"
    );
}

/// Os ids de uma tag do port, juntando o Generators e o Mekanism (o Horizon junta as listas de mesmo caminho), com as tags aninhadas resolvidas.
fn members(kind: &str, namespace: &str, path: &str, out: &mut BTreeSet<String>, depth: u8) {
    for root in [content(), mekanism_content()] {
        let file = root
            .join("data")
            .join(namespace)
            .join("tags")
            .join(kind)
            .join(format!("{path}.json"));
        if depth > 8 || !file.is_file() {
            continue;
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
}

fn tag_files(root: &Path, kind: &str) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for namespace in ["c", "minecraft", "mekanism", NS] {
        let base = root.join("data").join(namespace).join("tags").join(kind);
        if !base.is_dir() {
            continue;
        }
        for (relative, _) in generators::files_in(&base) {
            out.insert((
                namespace.to_owned(),
                relative.trim_end_matches(".json").to_owned(),
            ));
        }
    }
    out
}

/// Os ids de uma tag da datagen, com as tags aninhadas que a datagen também escreve resolvidas (`#mekanism:structures/common` está nela; as do vanilla e do NeoForge, não).
fn datagen_members(tags: &serde_json::Map<String, Value>, key: &str, out: &mut BTreeSet<String>) {
    let Some(tag) = tags.get(key) else {
        return;
    };
    // O que o port ainda não tem (`missing`: o que é de outra frente) não conta.
    let missing: BTreeSet<&str> = tag["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for value in tag["values"].as_array().unwrap() {
        let value = value.as_str().unwrap();
        if missing.contains(value) {
            continue;
        }
        if let Some(nested) = value.strip_prefix('#') {
            let (namespace, path) = nested.split_once(':').unwrap();
            let kind = key.split('/').nth(2).unwrap();
            datagen_members(tags, &format!("{namespace}/tags/{kind}/{path}.json"), out);
        } else {
            out.insert(value.to_owned());
        }
    }
}

/// Cada tag que a datagen escreve para um bloco ou item do Generators é uma tag do original para ele (em jogo, `Holder.tags()`) e lista o bloco no port; e toda tag do port que
/// lista o bloco é uma tag do original para ele. Vale para as tags de bloco e de item, para os 24 blocos (o `heat_generator` também: o pickaxe e o `blocks_motion_no_leaves` são
/// dele). As tags que o vanilla deriva das propriedades do bloco (`blocks_motion_no_leaves` dos outros 18) não passam pela datagen e o Horizon as dá sozinho.
#[test]
fn every_block_and_item_is_in_the_tags_the_original_puts_it_in() {
    let datagen = datagen();
    let tags = datagen["tags"].as_object().unwrap();
    let rows: BTreeMap<String, Value> = blocos()
        .into_iter()
        .map(|row| (row["id"].as_str().unwrap().to_owned(), row))
        .collect();
    let mut checked = 0;
    for kind in ["block", "item"] {
        let mut port: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for root in [content(), mekanism_content()] {
            for (namespace, path) in tag_files(&root, kind) {
                let mut ids = BTreeSet::new();
                members(kind, &namespace, &path, &mut ids, 0);
                port.insert(format!("{namespace}:{path}"), ids);
            }
        }
        for (full, row) in &rows {
            let original: BTreeSet<String> = if kind == "block" {
                row["tags"].as_array().unwrap()
            } else {
                row["item"]["tags"].as_array().unwrap()
            }
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
            // As tags `mekanism:` que a datagen do Generators acrescenta não são do Generators escrever (`pending`).
            for key in tags.keys().filter(|key| {
                key.split('/').nth(2) == Some(kind) && tags[*key].get("pending").is_none()
            }) {
                let mut ids = BTreeSet::new();
                datagen_members(tags, key, &mut ids);
                if !ids.contains(full) {
                    continue;
                }
                let namespace = key.split('/').next().unwrap();
                let path = key
                    .strip_prefix(&format!("{namespace}/tags/{kind}/"))
                    .unwrap()
                    .trim_end_matches(".json");
                let tag = format!("{namespace}:{path}");
                assert!(
                    original.contains(&tag),
                    "{kind} {full}: a datagen o põe em #{tag} e o original não"
                );
                let lists = port.get(&tag).is_some_and(|ids| ids.contains(full));
                assert!(
                    lists,
                    "{kind} {full}: o original o põe em #{tag} e o port não"
                );
                checked += 1;
            }
            for (tag, ids) in &port {
                if ids.contains(full) {
                    assert!(
                        original.contains(tag),
                        "{kind} {full}: o port o põe em #{tag} e o original não"
                    );
                }
            }
        }
    }
    assert!(checked >= 80, "{checked}");
}

/// Cada tag da datagen que cita o Generators está no port com os ids que o port tem, e o que falta é só o que a datagen lista de outra frente
/// (nada mais: `mekanism:pressure_disperser` e `#mekanism:structures/common` entraram com a frente `blocos-estrutura`).
#[test]
fn the_tags_hold_what_the_datagen_lists_except_what_is_not_ported_yet() {
    let datagen = datagen();
    let mut missing = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for (key, tag) in datagen["tags"].as_object().unwrap() {
        let kind = key.split('/').nth(2).unwrap();
        let namespace = key.split('/').next().unwrap();
        let path = key
            .strip_prefix(&format!("{namespace}/tags/{kind}/"))
            .unwrap()
            .trim_end_matches(".json");
        let file = content().join("data").join(key);
        if tag.get("pending").is_some() {
            // O Horizon não deixa um mod acrescentar a tag de outro (`a mod adds files in its own namespaces`): fica para o arquivo do Mekanism.
            assert_eq!(namespace, "mekanism", "{key}");
            assert!(!file.exists(), "{key}");
            // Elas vivem no arquivo do Mekanism, com as entradas do Generators como opcionais (`required: false`: sem o Generators somem, com ele entram), que o gerador do Mekanism
            // escreve (`blocos_conteudo.py`, `generators_optional_tags`).
            let theirs = json(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../mekanism/content/data")
                    .join(key),
            );
            let optional: Vec<(String, bool)> = theirs["values"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.is_object())
                .map(|v| {
                    (
                        v["id"].as_str().unwrap().to_owned(),
                        v["required"].as_bool().unwrap(),
                    )
                })
                .collect();
            let want: Vec<(String, bool)> = tag["missing"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v.as_str().unwrap().to_owned(), false))
                .collect();
            assert_eq!(optional, want, "{key}");
            pending.insert(key.clone());
            continue;
        }
        assert!(file.is_file(), "falta a tag {key}");
        let ours: Vec<String> = json(&file)["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let want: Vec<String> = tag["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let absent: Vec<String> = tag["missing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        for value in &absent {
            missing.insert(value.clone());
            assert!(!ours.contains(value), "{key}: {value}");
        }
        if namespace == "minecraft" {
            // O Horizon junta as listas do vanilla achatadas: o arquivo do Generators tem só os ids do Generators (as tags aninhadas resolvidas), e o do Mekanism os dele.
            let mut flat = BTreeSet::new();
            datagen_members(datagen["tags"].as_object().unwrap(), key, &mut flat);
            let flat: BTreeSet<String> = flat
                .into_iter()
                .filter(|v| v.starts_with("mekanismgenerators:"))
                .collect();
            assert_eq!(ours.iter().cloned().collect::<BTreeSet<_>>(), flat, "{key}");
            assert_eq!(ours.len(), flat.len(), "{key}: repetido");
            continue;
        }
        let expected: Vec<&String> = want.iter().filter(|v| !absent.contains(v)).collect();
        assert_eq!(ours.iter().collect::<Vec<_>>(), expected, "{key}");
        let _ = path;
    }
    // O dispersor de pressão e `#mekanism:structures/common` entraram com a frente `blocos-estrutura`: nada falta.
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(pending.len(), 4, "{pending:?}");
}

/// As tags `c:` do NeoForge que as receitas citam (`#c:bars`) são as do jar, byte a byte (o hash do rastro).
#[test]
fn the_neoforge_tags_are_the_jars() {
    let datagen = datagen();
    let tags = datagen["neoforge_tags"].as_object().unwrap();
    assert_eq!(tags.len(), 3);
    for (relative, hash) in tags {
        assert_eq!(
            hash.as_str().unwrap(),
            digest(&content().join(relative)),
            "{relative}"
        );
    }
}

/// Cada receita da datagen que ficou completa com o conteúdo do port existe no port, no mesmo caminho (o id da receita é o do original), igual ao original; as que não, são as
/// duas que citam o que o port não tem (as unidades de módulo do Mekanism). A do conjunto de combustível da fissão e a do controlador do reator ficaram completas com o tanque de químico e o dispersor de pressão da frente `blocos-estrutura`.
#[test]
fn every_complete_recipe_of_the_datagen_is_ported() {
    let datagen = datagen();
    let recipes = datagen["recipes"].as_object().unwrap();
    let manifest = datagen["files"].as_object().unwrap();
    let mut incomplete = Vec::new();
    let mut ported = 0;
    for (relative, info) in recipes {
        let file = content().join("data").join(relative);
        if info["complete"] == true {
            assert!(file.is_file(), "falta a receita {relative}");
            assert_eq!(
                manifest
                    .get(&format!("data/{relative}"))
                    .and_then(Value::as_str),
                Some(digest(&file).as_str()),
                "{relative}"
            );
            ported += 1;
        } else {
            assert!(!file.exists(), "{relative} está incompleta e existe");
            incomplete.push(relative.clone());
        }
    }
    assert_eq!(ported, 26);
    assert_eq!(
        incomplete,
        vec![
            "mekanismgenerators/recipe/geothermal_generator_unit.json",
            "mekanismgenerators/recipe/solar_recharging_unit.json",
        ]
    );
    // A receita de infusão do hohlraum é de um tipo do Mekanism e tem o item do Generators como resultado.
    let hohlraum = json(&content().join("data/mekanismgenerators/recipe/hohlraum.json"));
    assert_eq!(hohlraum["type"], "mekanism:metallurgic_infusing");
    assert_eq!(hohlraum["output"]["id"], "mekanismgenerators:hohlraum");
}

/// O `facing` que a regra de colocação dá é o que o original dá a cada guinada (de -720 a 720 graus), a cada inclinação e a cada face clicada, e o `fluid_logged` o que ele dá
/// com água, lava e ar no lugar (`rastros/blocos_generators_colocacao.jsonl`).
#[test]
fn the_placement_is_the_one_the_original_gives() {
    let mut facing_blocks = 0;
    let mut logged_blocks = 0;
    let mut points = 0;
    for row in blocos() {
        let id = row["id"].as_str().unwrap();
        let name = id.strip_prefix("mekanismgenerators:").unwrap();
        let placement = row["placement"].as_object().unwrap();
        let file = content().join(format!("data/{NS}/machinery_basis/placement/{name}.json"));
        if name == "heat_generator" {
            // O gabarito do forno se vira sozinho (Fase 5).
            assert!(!file.exists());
            continue;
        }
        if placement.is_empty() {
            assert!(
                !file.exists(),
                "{name}: tem regra e o original não escolhe nada"
            );
            continue;
        }
        let rule = Rule::parse(&json(&file));
        if placement.contains_key("yaw") {
            let facing = rule
                .facing
                .as_ref()
                .unwrap_or_else(|| panic!("{id}: sem facing"));
            let from = placement["yaw_from"].as_i64().unwrap();
            let step = placement["yaw_step"].as_i64().unwrap();
            for (index, want) in placement["yaw"].as_array().unwrap().iter().enumerate() {
                let yaw = (from + step * index as i64) as f32;
                assert_eq!(
                    facing.direction(yaw, 0.0, "up"),
                    want.as_str(),
                    "{id}: guinada {yaw}"
                );
                points += 1;
            }
            for (index, want) in placement["pitch"].as_array().unwrap().iter().enumerate() {
                let pitch = (placement["pitch_from"].as_i64().unwrap() + index as i64) as f32;
                assert_eq!(
                    facing.direction(0.0, pitch, "up"),
                    want.as_str(),
                    "{id}: inclinação {pitch}"
                );
                points += 1;
            }
            let faces = ["down", "up", "north", "south", "west", "east"];
            for (face, want) in faces.iter().zip(placement["face"].as_array().unwrap()) {
                assert_eq!(
                    facing.direction(0.0, 0.0, face),
                    want.as_str(),
                    "{id}: face {face}"
                );
                points += 1;
            }
            facing_blocks += 1;
        } else {
            assert!(rule.facing.is_none(), "{id}");
        }
        if let Some(fluids) = placement.get("fluid").and_then(Value::as_object) {
            assert_eq!(rule.fluid_logged.as_deref(), Some("fluid_logged"), "{id}");
            for (fluid, want) in fluids {
                assert_eq!(
                    machinery_basis_core::placement::fluid_logged_value(fluid),
                    want.as_str().unwrap(),
                    "{id}: {fluid}"
                );
            }
            logged_blocks += 1;
        } else {
            assert!(rule.fluid_logged.is_none(), "{id}");
        }
    }
    // Os seis geradores têm `facing`; os seis menos o heat (fora), cinco com `facing` e `fluid_logged`, mais as três peças com `fluid_logged` só.
    assert_eq!(facing_blocks, 5);
    assert_eq!(logged_blocks, 8);
    assert!(points >= 5 * 500, "{points}");
}
