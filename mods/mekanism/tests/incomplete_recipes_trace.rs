//! `IncompleteRecipeScanner`: a mensagem de receita incompleta e quem a recebe. O rastro `incomplete_recipes` (`RestosTraceTest.incompleteRecipes`) grava o
//! componente `RECIPE_WARNING` como o `ComponentSerialization` o escreve, o texto dele, as receitas que a varredura acha incompletas (só a do data pack
//! de teste, de uma tag que existe e não tem membro) e, em cada evento, quantas mensagens cada jogador recebeu.
use mekanism_rules::{IncompleteScanner, recipe_warning_json};
use serde_json::Value;
use std::path::Path;

fn trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/incomplete_recipes.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn case<'a>(lines: &'a [Value], name: &str) -> &'a Value {
    lines
        .iter()
        .find(|line| line["case"] == name)
        .unwrap_or_else(|| panic!("falta o caso {name}"))
}

fn lang() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// O texto do `translatable` como o jogo o monta: `%1$s` e `%2$s` pelos argumentos, que também são `translatable`.
fn text(component: &Value, lang: &Value) -> String {
    let key = component["translate"].as_str().unwrap();
    let template = lang[key]
        .as_str()
        .unwrap_or_else(|| panic!("falta {key} no lang"));
    let args: Vec<String> = component["with"]
        .as_array()
        .map(|with| with.iter().map(|arg| text(arg, lang)).collect())
        .unwrap_or_default();
    let mut out = template.to_owned();
    for (index, arg) in args.iter().enumerate() {
        out = out.replace(&format!("%{}$s", index + 1), arg);
    }
    out
}

#[test]
fn the_message_is_the_component_the_original_builds() {
    let lines = trace();
    let component = case(&lines, "component");
    assert_eq!(&recipe_warning_json(), &component["json"], "o componente");
    assert_eq!(
        text(&recipe_warning_json(), &lang()),
        component["string"].as_str().unwrap(),
        "o texto"
    );
}

#[test]
fn the_scan_finds_only_the_recipe_with_an_empty_tag() {
    let lines = trace();
    let incomplete = case(&lines, "scan")["incomplete"].as_array().unwrap();
    assert_eq!(incomplete.len(), 1, "{incomplete:?}");
    assert!(
        incomplete[0]
            .as_str()
            .unwrap()
            .ends_with("incomplete_empty_tag")
    );
}

/// Quem recebe em cada evento: o `ServerStartedEvent` ninguém, a entrada de um jogador ele mesmo se a última varredura achou, e o `/reload` todos.
#[test]
fn the_scanner_messages_the_players_the_original_does() {
    let lines = trace();
    let found_by_the_scan = !case(&lines, "scan")["incomplete"]
        .as_array()
        .unwrap()
        .is_empty();
    assert!(found_by_the_scan);

    // `ServerStartedEvent`: refaz a varredura; ninguém é avisado (o rastro tinha um jogador online e ele não recebeu nada).
    let mut scanner = IncompleteScanner::new();
    scanner.server_started(found_by_the_scan);
    let started = case(&lines, "server_started");
    assert_eq!(scanner.found(), started["found"].as_bool().unwrap());
    assert_eq!(started["messages_to_online"], 0);

    // A entrada de um jogador, com a varredura positiva: recebe, uma vez.
    let join = case(&lines, "join_found");
    assert_eq!(
        u32::from(scanner.player_joined()),
        join["messages"].as_u64().unwrap() as u32
    );
    assert_eq!(scanner.found(), join["found"].as_bool().unwrap());

    // Com a varredura negativa não recebe, e a entrada não varre: a varredura segue negativa.
    let mut negative = IncompleteScanner::new();
    let none = case(&lines, "join_not_found");
    assert_eq!(
        u32::from(negative.player_joined()),
        none["messages"].as_u64().unwrap() as u32
    );
    assert_eq!(negative.found(), none["found"].as_bool().unwrap());

    // `/reload`: a varredura é refeita (positiva) e a mensagem vai a todos os jogadores do servidor, a varredura estando negativa antes.
    let reload = case(&lines, "reload");
    let everyone = negative.reloaded(found_by_the_scan);
    let players = reload["players"].as_u64().unwrap() as usize;
    let delivered: Vec<u64> = reload["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|count| count.as_u64().unwrap())
        .collect();
    assert_eq!(delivered.len(), players);
    assert!(
        delivered.iter().all(|&count| count == u64::from(everyone)),
        "{delivered:?}"
    );
    assert_eq!(negative.found(), reload["found"].as_bool().unwrap());

    // O `/reload` sem receita incompleta não manda nada, e a varredura volta a negativa.
    assert!(!negative.reloaded(false));
    assert!(!negative.found());
}

/// Decisão 13 (fechada em 2026-11-09): com o conteúdo de craft e de recurso completo, qualquer receita incompleta conta, como no original. A varredura do rastro
/// (a receita do data pack de teste, de uma tag que existe e está vazia) acha uma, e uma varredura sem nenhuma não levanta a mensagem.
#[test]
fn any_incomplete_recipe_raises_the_message() {
    let lines = trace();
    let from_the_original = case(&lines, "scan")["incomplete"].as_array().unwrap();
    assert!(!from_the_original.is_empty());
    let mut found = IncompleteScanner::new();
    found.server_started(!from_the_original.is_empty());
    assert!(found.found());
    let mut none = IncompleteScanner::new();
    none.server_started(false);
    assert!(!none.found());
}
