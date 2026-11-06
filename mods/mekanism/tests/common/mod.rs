//! O que os testes de rastro partilham: os dados de upgrade lidos dos mesmos arquivos que o jogo lê.
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use mekanism_rules::upgrade_data as build_upgrade_data;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A pilha do item de upgrade, que não tem `max_stack_size` próprio.
const UPGRADE_STACK_SIZE: u32 = 64;
const NAMESPACE: &str = "mekanism";

fn content(folder: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanism")
        .join(folder)
}

/// Os `.json` de uma pasta de dados, cada um com o id que o jogo lhe dá (`FileToIdConverter`): `<ns>:<caminho sem .json>`.
fn read_folder(root: &Path, folder: &Path, files: &mut Vec<(String, Value)>) {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_folder(root, &path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let relative = path.strip_prefix(root).unwrap().with_extension("");
            let id = format!("{NAMESPACE}:{}", relative.to_str().unwrap());
            let text = std::fs::read_to_string(&path).unwrap();
            files.push((id, serde_json::from_str(&text).unwrap()));
        }
    }
}

fn load(folder: &str) -> Vec<(String, Value)> {
    let root = content(folder);
    let mut files = Vec::new();
    read_folder(&root, &root, &mut files);
    files
}

/// Os dados de upgrade das máquinas portadas: os tipos e as tags que o mod entrega, e nada escrito à mão.
pub fn upgrade_data() -> Rc<UpgradeData> {
    let (types, errors) = UpgradeTypes::load(&load(mekanism_rules::UPGRADE_FOLDER));
    assert!(errors.is_empty(), "tipos recusados: {errors:?}");
    assert!(types.len() >= 4, "faltam tipos: {}", types.len());
    let tags = UpgradeTags::load(&load(mekanism_rules::UPGRADE_TAGS_FOLDER));
    build_upgrade_data(&types, &tags, UPGRADE_STACK_SIZE)
}
