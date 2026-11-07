//! Repete o rastro `chamber_name`: o nome de uma câmara colocada por um item com nome e por um sem.
use machinery_basis_core::name::TileName;
use serde_json::Value;
use std::path::Path;

/// O nome do bloco e a descrição do contêiner, como a linguagem do jogo do rastro os dá.
const BLOCK_NAME: &str = "Enrichment Chamber";
const CONTAINER_DESCRIPTION: &str = "Enrichment Chamber";

fn lines() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/chamber_name.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn check(name: &TileName, expected: &Value) {
    let custom = name.custom_name().map(str::to_owned);
    assert_eq!(expected["name"], name.name(BLOCK_NAME));
    assert_eq!(
        expected["display_name"],
        name.display_name(CONTAINER_DESCRIPTION, BLOCK_NAME)
    );
    assert_eq!(expected["custom_name"], serde_json::json!(custom));
    assert_eq!(expected["nameable"], name.is_nameable());
    assert_eq!(expected["saved_name"], name.saved().is_some());
}

#[test]
fn a_chamber_placed_from_a_named_item_and_from_a_plain_one() {
    let lines = lines();
    assert_eq!(lines.len(), 2);
    // O item com nome: `applyImplicitComponents` com o `custom_name` dele.
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some("Minha Camara".to_owned()));
    check(&named, &lines[0]);
    // O item sem nome.
    let mut plain = TileName::new(true);
    plain.apply_implicit_components(None);
    check(&plain, &lines[1]);
}

#[test]
fn what_is_saved_comes_back_on_load() {
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some("Minha Camara".to_owned()));
    let mut reloaded = TileName::new(true);
    reloaded.load(named.saved().map(str::to_owned));
    assert_eq!(reloaded, named);
}
