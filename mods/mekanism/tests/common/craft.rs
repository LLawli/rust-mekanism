//! O que os testes do conteúdo de craft e de recurso partilham: os caminhos do conteúdo, os rastros do original e o hash que `dev/referencia/craft_rastros.py` grava.
#![allow(dead_code)]
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

pub fn rastro(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap()
}

/// Os blocos do Mekanism original em jogo, na ordem do registro: `rastros/blocos.jsonl` (`testes/BlocosTraceTest.java`: propriedades, estados, tags, item),
/// `blocos_formas.jsonl` (a tabela de formas de cada bloco) e `blocos_colocacao.jsonl` (o `facing` e o `fluid_logged` ao pôr). Um estado herda de `base` o que não difere do
/// estado padrão.
pub fn blocos() -> Vec<Value> {
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
    let shapes: std::collections::BTreeMap<String, Value> = lines("blocos_formas.jsonl")
        .into_iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["shapes"].clone(),
            )
        })
        .collect();
    let placement: std::collections::BTreeMap<String, Value> = lines("blocos_colocacao.jsonl")
        .into_iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["placement"].clone(),
            )
        })
        .collect();
    lines("blocos.jsonl")
        .into_iter()
        .map(|mut row| {
            let id = row["id"].as_str().unwrap().to_owned();
            row["shapes"] = shapes[&id].clone();
            row["placement"] = placement[&id].clone();
            let base = row["base"].as_object().unwrap().clone();
            for state in row["states"].as_array_mut().unwrap() {
                let state = state.as_object_mut().unwrap();
                for (key, value) in &base {
                    state.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            row
        })
        .collect()
}

pub fn json(path: &Path) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// FNV-1a de 64 bits e o tamanho, como `craft_rastros.py` (`fnv`).
pub fn fnv(data: &[u8]) -> String {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in data {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{hash:016x}:{}", data.len())
}

/// O hash de um arquivo do conteúdo: o JSON pelo texto canônico (chaves em ordem, sem espaço), o resto pelos bytes.
pub fn digest(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let value: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        fnv(serde_json::to_string(&value).unwrap().as_bytes())
    } else {
        fnv(&bytes)
    }
}

/// Os ids declarados em `data/mekanism/horizon/<kind>/`.
pub fn declared(kind: &str) -> BTreeSet<String> {
    std::fs::read_dir(content().join("data/mekanism/horizon").join(kind))
        .unwrap()
        .map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".json").unwrap().to_owned()
        })
        .collect()
}

/// Os arquivos `.json` de uma pasta, recursivamente, com o caminho relativo a ela.
pub fn files_in(root: &Path) -> Vec<(String, PathBuf)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                    path,
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// Os itens da frente `itens-equipamento` (`MekanismItems`: ferramentas, armaduras, leitores, instaladores, o Robit), declarados sem função; em ordem alfabética.
pub const EQUIPMENT_ITEMS: [&str; 37] = [
    "advanced_tier_installer",
    "atomic_disassembler",
    "basic_tier_installer",
    "canteen",
    "configuration_card",
    "configurator",
    "crafting_formula",
    "dictionary",
    "dosimeter",
    "electric_bow",
    "elite_tier_installer",
    "energy_tablet",
    "flamethrower",
    "free_runners",
    "free_runners_armored",
    "gauge_dropper",
    "geiger_counter",
    "hazmat_boots",
    "hazmat_gown",
    "hazmat_mask",
    "hazmat_pants",
    "hdpe_elytra",
    "jetpack",
    "jetpack_armored",
    "meka_tool",
    "mekasuit_bodyarmor",
    "mekasuit_boots",
    "mekasuit_helmet",
    "mekasuit_pants",
    "network_reader",
    "portable_qio_dashboard",
    "portable_teleporter",
    "robit",
    "scuba_mask",
    "scuba_tank",
    "seismic_reader",
    "ultimate_tier_installer",
];

pub fn assets() -> PathBuf {
    content().join("assets/mekanism")
}

/// O arquivo de modelo (`mekanism:item/x` → `models/item/x.json`) e o que ele cita, seguindo o `parent` e os `children` do `mekanism:`: as texturas (`.png`), e, num modelo
/// `neoforge:obj`, o `.obj` e o `.mtl` dele.
pub fn check_model(reference: &str, seen: &mut BTreeSet<String>) {
    let Some(path) = reference.strip_prefix("mekanism:") else {
        return;
    };
    if !seen.insert(path.to_owned()) {
        return;
    }
    let file = assets().join("models").join(format!("{path}.json"));
    assert!(file.is_file(), "falta o modelo {reference}");
    let model = json(&file);
    if let Some(parent) = model["parent"].as_str() {
        check_model(parent, seen);
    }
    let mut textures: Vec<&Value> = model["textures"]
        .as_object()
        .into_iter()
        .flat_map(|textures| textures.values())
        .collect();
    for child in model["children"]
        .as_object()
        .into_iter()
        .flat_map(|children| children.values())
    {
        if let Some(parent) = child["parent"].as_str() {
            check_model(parent, seen);
        }
        textures.extend(
            child["textures"]
                .as_object()
                .into_iter()
                .flat_map(|textures| textures.values()),
        );
    }
    for texture in textures.into_iter().filter_map(Value::as_str) {
        if let Some(path) = texture.strip_prefix("mekanism:") {
            assert!(
                assets()
                    .join("textures")
                    .join(format!("{path}.png"))
                    .is_file(),
                "falta a textura {texture} (modelo {reference})"
            );
        }
    }
    if model["loader"] == "neoforge:obj" {
        let obj = model["model"]
            .as_str()
            .and_then(|m| m.strip_prefix("mekanism:"))
            .unwrap_or_else(|| panic!("{reference}: modelo obj sem arquivo"));
        for file in [obj.to_owned(), obj.replace(".obj", ".mtl")] {
            assert!(assets().join(&file).is_file(), "falta {file} ({reference})");
        }
    }
}
