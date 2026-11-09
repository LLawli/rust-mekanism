//! As tags `c:` da datagen do original que o port tem ficam como o original as escreveu, e nenhuma tag `c:` do conteúdo cita o
//! que o Horizon descartaria: ele larga em silêncio a entrada que não é item nem bloco do conteúdo e a `#tag` sem arquivo, e
//! uma tag que fica vazia faz a receita sumir sem aviso.
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Tags de `data/c/tags` da datagen do Mekanism (`src/datagen/generated/mekanism`) que o port copia, byte a byte.
const COPIED_FROM_DATAGEN: [(&str, &str); 6] = [
    (
        "block/ores/osmium.json",
        "{\n  \"values\": [\n    \"mekanism:osmium_ore\",\n    \"mekanism:deepslate_osmium_ore\"\n  ]\n}",
    ),
    (
        "item/alloys.json",
        "{\n  \"values\": [\n    \"#c:alloys/advanced\",\n    \"#c:alloys/elite\",\n    \"#c:alloys/ultimate\"\n  ]\n}",
    ),
    (
        "item/circuits.json",
        "{\n  \"values\": [\n    \"#c:circuits/basic\",\n    \"#c:circuits/advanced\",\n    \"#c:circuits/elite\",\n    \"#c:circuits/ultimate\"\n  ]\n}",
    ),
    (
        "item/circuits/advanced.json",
        "{\n  \"values\": [\n    \"mekanism:advanced_control_circuit\"\n  ]\n}",
    ),
    (
        "item/circuits/elite.json",
        "{\n  \"values\": [\n    \"mekanism:elite_control_circuit\"\n  ]\n}",
    ),
    (
        "item/circuits/ultimate.json",
        "{\n  \"values\": [\n    \"mekanism:ultimate_control_circuit\"\n  ]\n}",
    ),
];

fn mods() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn c_tags() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/c/tags")
}

fn files_in(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_in(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Os ids `namespace:nome` declarados em `data/<namespace>/horizon/<kind>/` de um mod de conteúdo.
fn declared(mod_dir: &str, namespace: &str, kind: &str, out: &mut BTreeSet<String>) {
    let dir = mods()
        .join(mod_dir)
        .join("content/data")
        .join(namespace)
        .join("horizon")
        .join(kind);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap();
        out.insert(format!("{namespace}:{name}"));
    }
}

fn values(path: &Path) -> Vec<String> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["values"]
        .as_array()
        .unwrap_or_else(|| panic!("{}: sem values", path.display()))
        .iter()
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other["id"].as_str().unwrap().to_owned(),
        })
        .collect()
}

#[test]
fn the_copied_tags_are_the_datagen_files_of_the_original() {
    for (path, original) in COPIED_FROM_DATAGEN {
        let file = c_tags().join(path);
        let ours = std::fs::read_to_string(&file)
            .unwrap_or_else(|_| panic!("falta a tag {path}, que o original gera"));
        assert_eq!(ours, original, "{path} difere do arquivo do original");
    }
}

#[test]
fn every_c_tag_names_only_what_the_content_has() {
    let mut blocks = BTreeSet::new();
    let mut items = BTreeSet::new();
    for (mod_dir, namespace) in [
        ("mekanism", "mekanism"),
        ("mekanismgenerators", "mekanismgenerators"),
    ] {
        declared(mod_dir, namespace, "block", &mut blocks);
        declared(mod_dir, namespace, "block", &mut items);
        declared(mod_dir, namespace, "item", &mut items);
    }
    let mut files = Vec::new();
    files_in(&c_tags(), &mut files);
    assert!(files.len() >= 179, "só {} tags em data/c/tags", files.len());
    for file in files {
        let relative = file.strip_prefix(c_tags()).unwrap();
        let kind = relative.components().next().unwrap().as_os_str();
        let kind = kind.to_str().unwrap();
        let entries = values(&file);
        assert!(!entries.is_empty(), "{}: tag vazia", relative.display());
        for entry in entries {
            let known = if let Some(tag) = entry.strip_prefix('#') {
                let (namespace, path) = tag.split_once(':').unwrap();
                match namespace {
                    "minecraft" => true,
                    "c" => c_tags().join(kind).join(format!("{path}.json")).is_file(),
                    other => mods()
                        .join("mekanism/content/data")
                        .join(other)
                        .join("tags")
                        .join(kind)
                        .join(format!("{path}.json"))
                        .is_file(),
                }
            } else if entry.starts_with("minecraft:") {
                true
            } else {
                match kind {
                    "block" => blocks.contains(&entry),
                    "item" => items.contains(&entry),
                    other => panic!(
                        "{}: o Horizon não valida tags de {other}",
                        relative.display()
                    ),
                }
            };
            assert!(
                known,
                "{}: {entry} não existe no conteúdo",
                relative.display()
            );
        }
    }
}

/// `c:crops/melon` é do NeoForge (`Tags.Items.CROPS_MELON`, em `data/c/tags/item/crops/melon.json` do jar `neoforge-26.3.0.100`) e
/// lista só `minecraft:melon`, o bloco. Com a fatia nela, `crushing/biofuel/melon` e `crushing/biofuel/melon_slice` casavam a mesma
/// entrada no Crusher, e o port dava 6 de bio fuel em vez de 1: o original não acha receita para a fatia além da dela
/// (`dev/referencia/rastros/crusher_melon.jsonl`).
#[test]
fn the_melon_crop_tag_is_the_melon_block_and_not_the_slice() {
    let found = values(&c_tags().join("item/crops/melon.json"));
    assert_eq!(found, ["minecraft:melon"]);
}

/// As entradas de um ingrediente de receita (`"id"`, `"#tag"`, lista, ou `neoforge:or` com `values`), com as tags do conteúdo
/// resolvidas. Tag que o conteúdo não tem (as do vanilla) não dá entrada nenhuma.
fn ingredient_items(ingredient: &Value, out: &mut BTreeSet<String>) {
    match ingredient {
        Value::String(id) => match id.strip_prefix('#') {
            Some(tag) => {
                let (namespace, path) = tag.split_once(':').unwrap();
                let file = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("content/data")
                    .join(namespace)
                    .join("tags/item")
                    .join(format!("{path}.json"));
                if file.exists() {
                    for entry in values(&file) {
                        ingredient_items(&Value::String(entry), out);
                    }
                }
            }
            None => {
                out.insert(id.clone());
            }
        },
        Value::Array(list) => list.iter().for_each(|child| ingredient_items(child, out)),
        Value::Object(object) => {
            if let Some(children) = object.get("values") {
                ingredient_items(children, out);
            }
            if let Some(id) = object.get("item") {
                ingredient_items(id, out);
            }
        }
        _ => {}
    }
}

/// Os ids das receitas de `crushing` do conteúdo (o caminho depois de `recipe/`, sem `.json`) cuja entrada casa `item`.
fn crushing_recipes_matching(item: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/recipe");
    let mut files = Vec::new();
    files_in(&root.join("crushing"), &mut files);
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let recipe: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let mut items = BTreeSet::new();
        ingredient_items(&recipe["input"]["ingredient"], &mut items);
        if items.contains(item) {
            let id = file.strip_prefix(&root).unwrap().with_extension("");
            found.push(id.to_str().unwrap().to_owned());
        }
    }
    found
}

/// O original (`SlotsTraceTest.crusherMelon`) acha uma só receita de `crushing` para `minecraft:melon_slice`, e é a da própria fatia.
#[test]
fn the_crusher_has_one_recipe_for_a_melon_slice_as_the_original_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/crusher_melon.jsonl");
    let line = std::fs::read_to_string(path).unwrap();
    let trace: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
    let original: Vec<String> = trace["matching"]
        .as_array()
        .unwrap()
        .iter()
        .map(|found| found["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(original, ["crushing/biofuel/melon_slice"]);
    assert_eq!(crushing_recipes_matching("minecraft:melon_slice"), original);
}

/// As tags `c:` da datagen que citam os itens e blocos de craft e de recurso (`craft_data.rs` as confere uma a uma contra o rastro do original) e as três do NeoForge que
/// as receitas completas pedem (`c:ender_pearls`, `c:buckets/water` e `c:rods/wooden`, do jar `neoforge-26.3.0.100`) existem.
#[test]
fn the_tags_the_craft_recipes_need_exist() {
    for path in [
        "item/ender_pearls.json",
        "item/buckets/water.json",
        "item/rods/wooden.json",
        "item/storage_blocks/steel.json",
        "item/ingots/tin.json",
        "item/nuggets/refined_glowstone.json",
        "item/dusts/salt.json",
        "item/fuels/block/bio.json",
        "block/ores/uranium.json",
        "block/storage_blocks/raw_tin.json",
    ] {
        assert!(c_tags().join(path).is_file(), "falta data/c/tags/{path}");
    }
    assert_eq!(values(&c_tags().join("item/ender_pearls.json")), ["minecraft:ender_pearl"]);
    assert_eq!(values(&c_tags().join("item/buckets/water.json")), ["minecraft:water_bucket"]);
    assert_eq!(values(&c_tags().join("item/rods/wooden.json")), ["minecraft:stick"]);
}
