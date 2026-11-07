//! Repete o `InventoryContainerSlot.mayPlace` que o Mekanism original respondeu em cada clique do cursor
//! (`dev/referencia/rastros/slot_click_<máquina>.jsonl`, escrito por `SlotClickTraceTest.java`): o menu de verdade
//! (`MekanismTileContainer.clicked`), com a máquina posta por um jogador, um item de uma lista que as regras separam, e a mão, o
//! slot e o tanque em cada estado. Cada linha traz `may_place` e o que o clique esquerdo, o direito, a tecla numérica e o arrasto
//! deixaram no slot, no cursor e na barra.
//!
//! Dois fatos do rastro sustentam o teste:
//! - `may_place` de um (estado, slot, item) é o mesmo com o slot vazio, com outro item e com o mesmo item, e com cada clique: a
//!   segunda condição do `mayPlace` (tirar o que está lá, `isCurrentValidForExtraction(MANUAL)`) nunca recusa nas cinco máquinas;
//! - o resultado de cada clique é uma função do `may_place` e do que há no slot: o motor de menu do jogo faz o resto (`tests/slot_click_model`
//!   mostra isso para o Horizon, que usa o motor dele).
//!
//! Receitas: a Enrichment Chamber, o Crusher e o Metallurgic Infuser leem os arquivos do conteúdo (`recipe/enriching`, `crushing`,
//! `metallurgic_infusing` e `chemical_conversion`), que têm as tags `c:` do port. O Energized Smelter usa `recipe_type = smelting`
//! do jogo (as receitas de cozimento do vanilla), que um teste sem o jogo não tem: as da lista são as três que o Java aceitou.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalResource};
use machinery_basis_core::containers::ItemResource;
use mekanism_rules::{Held, Indexes, SlotIndexes, infusing_cache_of, slot_accepts};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const STACK: u32 = 64;
/// Os itens da sonda do Java, as máquinas e os estados que ela varre: um rastro menor é um rastro cortado.
const ITEMS: usize = 15;
const ELECTRIC_ROWS: usize = ITEMS * 3 * 8;
const INFUSER_ROWS: usize = ITEMS * 4 * 8 * 6;
const HEAT_ROWS: usize = ITEMS * 2 * 8 * 2;

fn rows(machine: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("slot_click_{machine}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, STACK)
}

/// O índice de receitas do Metallurgic Infuser e o de conversões, lidos do conteúdo.
fn infuser_indexes() -> Indexes {
    let (recipes, conversions) = common::infuser_recipes_by_type();
    Indexes::Infuser {
        recipes: infusing_cache_of(recipes),
        conversions: ChemicalConversion::cache_of(conversions),
        energy: common::energy_conversions(),
    }
}

fn electric_indexes(recipes: Vec<Rc<ItemToItemRecipe>>) -> Indexes {
    Indexes::Electric {
        recipes: ItemToItemRecipe::cache_of(recipes),
        energy: common::energy_conversions(),
    }
}

/// As receitas de cozimento que o Java aceitou na entrada do Energized Smelter.
fn smelting_indexes(accepted: &[&str]) -> Indexes {
    let recipes = accepted
        .iter()
        .map(|input| {
            Rc::new(ItemToItemRecipe {
                id: format!("minecraft:smelting/{input}"),
                input: ItemIngredient::item(input),
                input_count: 1,
                output: ItemOutput {
                    resource: item("stone"),
                    count: 1,
                },
            })
        })
        .collect();
    electric_indexes(recipes)
}

/// Os itens que o Java aceitou na entrada, com o slot vazio e a mão cheia: de onde sai a lista de cozimento do Energized Smelter.
fn accepted_input(all: &[Value]) -> Vec<String> {
    let mut items: Vec<String> = all
        .iter()
        .filter(|row| row["kind"] == "input" && row["may_place"] == true)
        .map(|row| row["item"].as_str().unwrap().to_owned())
        .collect();
    items.sort();
    items.dedup();
    items
}

fn held(row: &Value) -> Held {
    let tank = row["tank"].as_str().unwrap();
    let input = row["input"].as_str().unwrap();
    Held {
        chemical: matches!(tank, "bio" | "fungi").then(|| ChemicalResource::new(tank)),
        input: (!input.is_empty()).then(|| item(input)),
    }
}

/// A chave do estado de um clique: tudo o que a regra olha.
fn key(row: &Value) -> String {
    format!(
        "{}/{}/{}/{}",
        row["tank"], row["input"], row["slot"], row["item"]
    )
}

/// O `may_place` do Java não muda com o que há no slot nem com o clique: a regra do port só olha o estado, o slot e o item.
fn rule_is_independent_of_the_click(all: &[Value]) {
    let mut seen: BTreeMap<String, bool> = BTreeMap::new();
    for row in all {
        let may = row["may_place"].as_bool().unwrap();
        if let Some(before) = seen.insert(key(row), may) {
            assert_eq!(
                before, may,
                "o mayPlace mudou com o conteúdo ou com o clique: {row}"
            );
        }
    }
}

fn check(machine: &str, indexes: &Indexes, slots: &SlotIndexes, minimum: usize) {
    let all = rows(machine);
    assert!(
        all.len() >= minimum,
        "{machine}: só {} linhas no rastro",
        all.len()
    );
    rule_is_independent_of_the_click(&all);
    let mut accepted = 0;
    for row in &all {
        let found = slot_accepts(
            indexes,
            slots,
            row["slot"].as_u64().unwrap() as usize,
            &item(row["item"].as_str().unwrap()),
            &held(row),
        );
        assert_eq!(
            found,
            row["may_place"].as_bool().unwrap(),
            "{machine}: {row}"
        );
        accepted += usize::from(found);
    }
    assert!(
        accepted > ITEMS,
        "{machine}: o rastro tem de aceitar itens, só {accepted}"
    );
    assert!(accepted < all.len() / 2, "{machine}: e recusar a maioria");
}

const ELECTRIC_SLOTS: SlotIndexes = SlotIndexes {
    input: 0,
    energy: 2,
    infusion: None,
};

#[test]
fn the_chamber_cursor_takes_what_a_recipe_uses_and_the_energy_items() {
    let indexes = electric_indexes(common::item_recipes_of_type("mekanism:enriching"));
    check(
        "enrichment_chamber",
        &indexes,
        &ELECTRIC_SLOTS,
        ELECTRIC_ROWS,
    );
}

#[test]
fn the_crusher_cursor_takes_what_a_recipe_uses_and_the_energy_items() {
    let indexes = electric_indexes(common::item_recipes_of_type("mekanism:crushing"));
    check("crusher", &indexes, &ELECTRIC_SLOTS, ELECTRIC_ROWS);
}

#[test]
fn the_smelter_cursor_takes_what_a_recipe_uses_and_the_energy_items() {
    let accepted = accepted_input(&rows("energized_smelter"));
    assert_eq!(accepted, ["clay", "cobblestone", "sand"]);
    let accepted: Vec<&str> = accepted.iter().map(String::as_str).collect();
    check(
        "energized_smelter",
        &smelting_indexes(&accepted),
        &ELECTRIC_SLOTS,
        ELECTRIC_ROWS,
    );
}

#[test]
fn the_infuser_cursor_follows_the_tank_and_the_input_it_holds() {
    let slots = SlotIndexes {
        input: 1,
        energy: 3,
        infusion: Some(0),
    };
    check(
        "metallurgic_infuser",
        &infuser_indexes(),
        &slots,
        INFUSER_ROWS,
    );
}

/// O Java varreu seis estados de tanque e entrada no Infuser: o teste confere que eles chegam a respostas diferentes, senão uma regra
/// que ignora o estado também passaria.
#[test]
fn the_infuser_answers_change_with_the_tank_and_the_input() {
    let all = rows("metallurgic_infuser");
    let mut by_state: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
    for row in &all {
        if row["may_place"] == true && row["content"] == "empty" && row["action"] == "left" {
            by_state
                .entry((
                    row["tank"].as_str().unwrap().to_owned(),
                    row["input"].as_str().unwrap().to_owned(),
                    row["kind"].as_str().unwrap().to_owned(),
                ))
                .or_default()
                .push(row["item"].as_str().unwrap().to_owned());
        }
    }
    let infusion = |tank: &str, input: &str| {
        by_state
            .get(&(tank.into(), input.into(), "infusion".into()))
            .cloned()
            .unwrap_or_default()
    };
    let input = |tank: &str, input: &str| {
        by_state
            .get(&(tank.into(), input.into(), "input".into()))
            .cloned()
            .unwrap_or_default()
    };
    assert!(infusion("empty", "").contains(&"coal".to_owned()));
    assert!(
        !infusion("bio", "").contains(&"coal".to_owned()),
        "com bio no tanque o carvão (carbono) não entra"
    );
    assert!(
        infusion("fungi", "dirt").is_empty(),
        "fungi no tanque: só o item que dá fungi entra, e nenhum da lista dá"
    );
    assert!(input("fungi", "").contains(&"dirt".to_owned()) && input("fungi", "").len() == 1);
    assert!(
        input("empty", "").len() > input("bio", "").len(),
        "o tanque vazio aceita mais entradas que o de bio"
    );
}

/// O Heat Generator (`mods/mekanismgenerators`) responde pelo tempo de queima do jogo, que um teste sem ele não tem: aqui só o que o
/// rastro diz, para a prova em jogo repetir. O combustível não muda com o tanque de lava, e o slot de energia (`EnergyInventorySlot.drain`)
/// não aceita nenhum item sem handler de energia.
#[test]
fn the_heat_generator_cursor_takes_fuel_whatever_the_tank_holds_and_no_energy_item() {
    let all = rows("heat_generator");
    assert!(all.len() >= HEAT_ROWS, "só {} linhas", all.len());
    rule_is_independent_of_the_click(&all);
    for tank in ["empty", "lava"] {
        let mut fuel: Vec<&str> = all
            .iter()
            .filter(|row| row["tank"] == tank && row["kind"] == "fuel" && row["may_place"] == true)
            .map(|row| row["item"].as_str().unwrap())
            .collect();
        fuel.sort_unstable();
        fuel.dedup();
        assert_eq!(
            fuel,
            ["bio_fuel", "coal", "lava_bucket", "stick"],
            "tanque {tank}"
        );
        assert!(
            all.iter()
                .filter(|row| row["kind"] == "energy")
                .all(|row| row["may_place"] == false),
            "o slot de energia do gerador não aceita item sem handler de energia"
        );
    }
}
