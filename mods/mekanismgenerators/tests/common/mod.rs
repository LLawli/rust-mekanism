//! O que os testes de rastro do Heat Generator partilham: os dados de upgrade lidos dos mesmos arquivos que o jogo lê. O Generators usa
//! os tipos e as tags do Mekanism (`data/mekanism/mekanism/upgrade` e `data/mekanism/tags/mekanism/upgrade`), então vêm do conteúdo dele.
#![allow(dead_code)]
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A pilha do item de upgrade, que não tem `max_stack_size` próprio.
const UPGRADE_STACK_SIZE: u32 = 64;
const NAMESPACE: &str = "mekanism";
/// `Upgrade.DIRECT_CODEC`: a pasta dos tipos, e a das tags de suporte.
const UPGRADE_FOLDER: &str = "mekanism/upgrade";
const UPGRADE_TAGS_FOLDER: &str = "tags/mekanism/upgrade";

fn content(folder: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/data/mekanism")
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

/// Os dados de upgrade do Heat Generator: os tipos e as tags que o Mekanism entrega, e nada escrito à mão.
pub fn upgrade_data() -> Rc<UpgradeData> {
    let (types, errors) = UpgradeTypes::load(&load(UPGRADE_FOLDER));
    assert!(errors.is_empty(), "tipos recusados: {errors:?}");
    assert!(types.len() >= 4, "faltam tipos: {}", types.len());
    let tags = UpgradeTags::load(&load(UPGRADE_TAGS_FOLDER));
    mekanismgenerators_rules::upgrade_data(&types, &tags, UPGRADE_STACK_SIZE)
}

/// Quantos tipos de upgrade o Mekanism declara.
pub fn declared_types() -> usize {
    UpgradeTypes::load(&load(UPGRADE_FOLDER)).0.len()
}
