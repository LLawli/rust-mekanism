//! Repete os rastros `chamber_name` (o nome de uma câmara colocada por um item com nome e por um sem) e `chamber_name_styled` (o mesmo
//! com o componente inteiro: cor, negrito e um filho de outra cor). O componente é o JSON do codec do jogo (`ComponentSerialization`).
use machinery_basis_core::name::TileName;
use serde_json::{Value, json};
use std::path::Path;

/// O nome do bloco e a descrição do contêiner, como a linguagem do jogo do rastro os dá.
fn block_name() -> Value {
    json!("Enrichment Chamber")
}

fn container_description() -> Value {
    json!("Enrichment Chamber")
}

fn lines(file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(file);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn check(name: &TileName<Value>, expected: &Value) {
    let custom = name.custom_name().cloned();
    assert_eq!(expected["name"], *name.name(&block_name()));
    assert_eq!(
        expected["display_name"],
        *name.display_name(&container_description(), &block_name())
    );
    assert_eq!(expected["custom_name"], json!(custom));
    assert_eq!(expected["nameable"], name.is_nameable());
    assert_eq!(expected["saved_name"], name.saved().is_some());
}

#[test]
fn a_chamber_placed_from_a_named_item_and_from_a_plain_one() {
    let lines = lines("chamber_name.jsonl");
    assert_eq!(lines.len(), 2);
    // O item com nome: `applyImplicitComponents` com o `custom_name` dele.
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some(json!("Minha Camara")));
    check(&named, &lines[0]);
    // O item sem nome.
    let mut plain = TileName::new(true);
    plain.apply_implicit_components(None);
    check(&plain, &lines[1]);
}

#[test]
fn what_is_saved_comes_back_on_load() {
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some(json!("Minha Camara")));
    let mut reloaded = TileName::new(true);
    reloaded.load(named.saved().cloned());
    assert_eq!(reloaded, named);
}

/// `Component.literal("Camara Colorida").withStyle(red, bold)`, e com um filho `" Azul"` em `0x3355FF` e itálico, como o codec os escreve.
fn styled_items() -> [Value; 2] {
    [
        json!({"text": "Camara Colorida", "color": "red", "bold": true}),
        json!({"text": "Camara Colorida", "color": "red", "bold": true, "extra": [{"text": " Azul", "color": "#3355FF", "italic": true}]}),
    ]
}

/// O que o tile mostra, grava e devolve ao item com o componente inteiro: cada campo do rastro é o JSON do mesmo componente que entrou.
#[test]
fn a_styled_name_is_kept_whole_in_the_tile_the_title_and_the_dropped_item() {
    let lines = lines("chamber_name_styled.jsonl");
    assert_eq!(lines.len(), 2);
    for (item, line) in styled_items().into_iter().zip(&lines) {
        // A entrada que o teste do Java monta é a mesma que o port recebe do item.
        assert_eq!(line["custom_name"], item);
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some(item.clone()));
        assert_eq!(line["name"], *name.name(&block_name()));
        assert_eq!(
            line["display_name"],
            *name.display_name(&container_description(), &block_name())
        );
        assert_eq!(json!(name.custom_name()), line["custom_name"]);
        // O título é o texto do componente: o que o jogo mede e centra.
        assert_eq!(line["drops"], json!([name.item_name()]));
        // O que foi gravado volta intacto no carregamento.
        let mut reloaded = TileName::new(true);
        reloaded.load(name.saved().cloned());
        assert_eq!(reloaded, name);
    }
}

/// O Heat Generator também aceita nome (`nameable = hasGui() && !AttributeGui.hasCustomName()`, e ele não tem nome de tela próprio): o
/// rastro `heat_generator_name_styled` tem o mesmo componente, e sem nome o título é `container.mekanismgenerators.heat_generator`.
#[test]
fn a_heat_generator_is_nameable_and_keeps_the_component_whole() {
    let lines = lines("heat_generator_name_styled.jsonl");
    assert_eq!(lines.len(), 2);
    let block = json!({"translate": "block.mekanismgenerators.heat_generator"});
    let description = json!({"translate": "container.mekanismgenerators.heat_generator"});
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some(styled_items()[1].clone()));
    assert_eq!(lines[0]["custom_name"], json!(named.custom_name()));
    assert_eq!(lines[0]["name"], *named.name(&block));
    assert_eq!(
        lines[0]["display_name"],
        *named.display_name(&description, &block)
    );
    assert_eq!(lines[0]["drops"], json!([named.item_name()]));
    let mut plain = TileName::<serde_json::Value>::new(true);
    plain.apply_implicit_components(None);
    assert_eq!(lines[1]["name"], *plain.name(&block));
    assert_eq!(
        lines[1]["display_name"],
        *plain.display_name(&description, &block)
    );
    assert_eq!(lines[1]["custom_name"], json!(null));
}

/// O item que a explosão solta leva o nome do tile, como o de uma quebra (`chamber_explosion`).
#[test]
fn the_item_dropped_by_an_explosion_carries_the_name() {
    let lines = lines("chamber_explosion.jsonl");
    let broken = lines
        .iter()
        .find(|line| line.get("broken").is_some())
        .unwrap();
    let mut named = TileName::new(true);
    named.apply_implicit_components(Some(styled_items()[0].clone()));
    assert_eq!(broken["drops"][0]["item"], "mekanism:enrichment_chamber");
    assert_eq!(broken["drops"][0]["custom_name"], json!(named.item_name()));
}
