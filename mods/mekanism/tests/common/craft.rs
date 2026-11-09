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
