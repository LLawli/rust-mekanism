//! A colocação da receita pelo livro, contra o original (`dev/referencia/testes/RecipePlaceTraceTest.java`, que chama
//! `MekanismContainer.handlePlacement` nas quatro máquinas): em cada cenário de `recipe_place_<máquina>.jsonl`, a resposta
//! (`NOTHING`, `PLACE_GHOST_RECIPE` ou a `ClassCastException` do Energized Smelter), o inventário do jogador, os slots da máquina e
//! o que caiu no chão são os do Java, com o `hashCode` de cada `ItemResource` que aquela execução deu (a ordem do `HashMap` do
//! `RecipeIngredientResolver` sai dele). A tabela `GHOSTS` é o que os rastros dizem que cada display mostra.
use machinery_basis_core::recipe_book_view::BookData;
use machinery_basis_core::recipe_place::{Action, Containers, MAIN_SLOTS, Rules, Stack, State, item_of, place_recipe};
use mekanism_rules::recipe_book_rows::{GHOSTS, REQUIREMENTS};
use mekanism_rules::recipe_books::{CRUSHING, ENRICHING, METALLURGIC_INFUSING, SMELTING};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn trace(name: &str) -> Vec<Value> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros").join(name);
    std::fs::read_to_string(file).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// As regras dos slots com o que o rastro deu: o slot 0 é a entrada (aceita o que a máquina tem receita, e os cenários só põem ali
/// itens assim) e o 1 a saída, de onde só se tira.
struct Traced {
    hashes: BTreeMap<String, i32>,
}

impl Rules for Traced {
    fn max_stack(&self, resource: &str) -> i32 {
        if item_of(resource) == "minecraft:diamond_sword" { 1 } else { 64 }
    }
    fn valid(&self, slot: usize, _: &str) -> bool {
        slot == 0
    }
    fn can_insert(&self, slot: usize, _: &str) -> bool {
        slot == 0
    }
    fn can_extract(&self, _: usize, _: &str) -> bool {
        true
    }
    fn hash(&self, resource: &str) -> i32 {
        // O rastro grava o hash dos itens do inventário (os que entram no `HashMap`). O do item que estava no slot só vira chave da
        // tabela do que volta ao jogador, que com um slot de entrada tem uma chave só: a ordem não depende dele.
        self.hashes.get(resource).copied().unwrap_or(0)
    }
}

fn stack(value: &Value) -> Stack {
    let name = value[0].as_str().unwrap();
    (!name.is_empty()).then(|| (name.to_owned(), value[1].as_i64().unwrap() as i32))
}

fn state(value: &Value, selected: usize) -> (State, Option<(String, i64)>) {
    let mut main: Vec<Stack> = vec![None; MAIN_SLOTS];
    let mut offhand = None;
    for entry in value["inv"].as_array().unwrap() {
        let slot = entry[0].as_u64().unwrap() as usize;
        let found = Some((entry[1].as_str().unwrap().to_owned(), entry[2].as_i64().unwrap() as i32));
        match slot {
            40 => offhand = found,
            _ => main[slot] = found,
        }
    }
    let tank = value["tank"].as_array().and_then(|tank| {
        let name = tank[0].as_str().unwrap();
        (!name.is_empty()).then(|| (name.to_owned(), tank[1].as_i64().unwrap()))
    });
    (State { main, offhand, selected, slots: vec![stack(&value["input"]), stack(&value["output"])], dropped: Vec::new() }, tank)
}

fn machine(name: &str, book: &BookData, expected: usize) {
    let cases = trace(&format!("recipe_place_{name}.jsonl"));
    assert_eq!(cases.len(), expected, "{name}");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for case in &cases {
        let label = format!("{name} caso {}", case["case"]);
        let (mut now, tank) = state(&case["before"], case["selected"].as_u64().unwrap() as usize);
        let rules = Traced { hashes: serde_json::from_value(case["hashes"].clone()).unwrap() };
        let recipe = book
            .placement(case["display"].as_i64().unwrap() as i32, tank.as_ref().map(|(name, amount)| (name.as_str(), *amount)))
            .unwrap_or_else(|| panic!("{label}: sem a receita"));
        let action = place_recipe(
            &mut now,
            &rules,
            &recipe,
            Containers { inputs: &[0], outputs: &[1] },
            case["max"].as_bool().unwrap(),
            case["creative"].as_bool().unwrap(),
        );
        let named = match action {
            Action::Nothing => "NOTHING",
            Action::PlaceGhostRecipe => "PLACE_GHOST_RECIPE",
            Action::ClassCast => "error:java.lang.ClassCastException",
        };
        assert_eq!(case["action"], named, "{label}");
        let (after, _) = state(&case["after"], now.selected);
        assert_eq!(now.main, after.main, "{label}: o inventário");
        assert_eq!(now.offhand, after.offhand, "{label}: a outra mão");
        assert_eq!(now.slots, after.slots, "{label}: os slots");
        let dropped: Vec<(String, i32)> =
            case["dropped"].as_array().unwrap().iter().map(|drop| (drop[0].as_str().unwrap().to_owned(), drop[1].as_i64().unwrap() as i32)).collect();
        assert_eq!(now.dropped, dropped, "{label}: o que caiu");
        *seen.entry(named.to_owned()).or_default() += 1;
    }
    // O rastro cobre os dois desfechos (ou só a exceção, no Energized Smelter).
    if name == "energized_smelter" {
        assert_eq!(seen.len(), 1);
    } else {
        assert_eq!(seen.len(), 2, "{name}: {seen:?}");
    }
}

#[test]
fn the_enrichment_chamber_places_what_the_original_places() {
    machine("enrichment_chamber", &ENRICHING, 50);
}

#[test]
fn the_crusher_places_what_the_original_places() {
    machine("crusher", &CRUSHING, 14);
}

#[test]
fn the_energized_smelter_refuses_every_recipe_as_the_original_throws() {
    machine("energized_smelter", &SMELTING, 5);
}

#[test]
fn the_metallurgic_infuser_places_what_the_original_places() {
    machine("metallurgic_infuser", &METALLURGIC_INFUSING, 19);
}

fn shown(value: &Value) -> Vec<Vec<(String, i32)>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one.as_array().unwrap().iter().map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].as_i64().unwrap() as i32)).collect())
        .collect()
}

fn ours(lists: &[&[(&str, i32)]]) -> Vec<Vec<(String, i32)>> {
    lists.iter().map(|one| one.iter().map(|(name, count)| ((*name).to_owned(), *count)).collect()).collect()
}

#[test]
fn the_ghost_table_is_what_the_original_displays_resolve_to() {
    let items = trace("recipe_book_requirements.jsonl");
    let info = trace("recipe_place_info.jsonl");
    assert_eq!(items.len(), GHOSTS.len());
    assert_eq!(info.len(), GHOSTS.len());
    assert_eq!(REQUIREMENTS.len(), GHOSTS.len());
    for ((line, extra), ghost) in items.iter().zip(&info).zip(GHOSTS) {
        assert_eq!(line["id"], ghost.0);
        assert_eq!(extra["id"], ghost.0);
        assert_eq!(shown(&line["ing"]), ours(ghost.1), "{}", line["recipe"]);
        assert_eq!(shown(&line["out"]), ours(ghost.2), "{}", line["recipe"]);
        assert_eq!(shown(&extra["chem"]), ours(ghost.3), "{}", line["recipe"]);
        assert_eq!(shown(&extra["chem_out"]), ours(ghost.4), "{}", line["recipe"]);
        // Nenhuma receita dos quatro tipos é impossível de colocar (`placementInfo().isImpossibleToPlace()`).
        assert_eq!(extra["impossible"], false);
        // Só as do forno do vanilla não são `MekanismRecipe` (o `ClassCastException` de `handlePlacement`).
        let foreign = REQUIREMENTS.iter().find(|row| row.0 == ghost.0).unwrap().2.is_none();
        assert_eq!(extra["class"] == "SmeltingRecipe", foreign, "{}", line["recipe"]);
    }
    assert!(GHOSTS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(METALLURGIC_INFUSING.ghost(1838).is_some());
}
