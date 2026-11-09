//! O que os testes do conteúdo do Generators partilham (frente `generators-conteudo`): os caminhos do conteúdo, os rastros do original em jogo e o hash que
//! `dev/referencia/generators_conteudo.py` grava em `rastros/generators_datagen.json`.
#![allow(dead_code)]
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

pub fn mekanism_content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../mekanism/content")
}

pub fn rastro_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name)
}

fn lines(name: &str) -> Vec<Value> {
    let path = rastro_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Os 24 blocos do Mekanism Generators do original em jogo, na ordem do registro de `GeneratorsBlocks`: `rastros/blocos_generators.jsonl`
/// (`testes/BlocosTraceTest.java`, o método `generators`), `blocos_generators_formas.jsonl` (a tabela de formas de cada bloco) e `blocos_generators_colocacao.jsonl` (o
/// `facing` ao pôr). Um estado herda de `base` o que não difere do estado padrão.
pub fn blocos() -> Vec<Value> {
    let shapes: BTreeMap<String, Value> = lines("blocos_generators_formas.jsonl")
        .into_iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["shapes"].clone(),
            )
        })
        .collect();
    let placement: BTreeMap<String, Value> = lines("blocos_generators_colocacao.jsonl")
        .into_iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["placement"].clone(),
            )
        })
        .collect();
    lines("blocos_generators.jsonl")
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

/// Os itens do Generators que não são de bloco, do original em jogo (`blocos_generators_itens.jsonl`), por id sem namespace.
pub fn itens() -> BTreeMap<String, Value> {
    lines("blocos_generators_itens.jsonl")
        .into_iter()
        .map(|row| {
            (
                row["id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("mekanismgenerators:")
                    .unwrap()
                    .to_owned(),
                row,
            )
        })
        .collect()
}

/// `rastros/generators_datagen.json`: o hash dos arquivos copiados, as tags e as receitas da datagen, o texto, a ordem do registro e a aba criativa.
pub fn datagen() -> Value {
    json(&rastro_path("generators_datagen.json"))
}

pub fn json(path: &Path) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// FNV-1a de 64 bits e o tamanho, como `generators_conteudo.py` (`fnv`).
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

/// Os ids declarados em `data/mekanismgenerators/horizon/<kind>/`.
pub fn declared(kind: &str) -> BTreeSet<String> {
    std::fs::read_dir(content().join("data/mekanismgenerators/horizon").join(kind))
        .unwrap()
        .map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".json").unwrap().to_owned()
        })
        .collect()
}

/// Os 24 blocos do registro, e os 23 que esta frente trouxe (o `heat_generator` já existia, com o gabarito do forno).
pub fn all_blocks() -> Vec<String> {
    blocos()
        .iter()
        .map(|row| {
            row["id"]
                .as_str()
                .unwrap()
                .strip_prefix("mekanismgenerators:")
                .unwrap()
                .to_owned()
        })
        .collect()
}

pub fn scope() -> Vec<String> {
    all_blocks()
        .into_iter()
        .filter(|id| id != "heat_generator")
        .collect()
}

pub const ITEMS: [&str; 3] = ["solar_panel", "hohlraum", "turbine_blade"];

/// Os arquivos de uma pasta, recursivamente, com o caminho relativo a ela.
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
