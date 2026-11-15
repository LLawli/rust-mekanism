//! Repete o que um dropper e um dispenser fazem apontados para cada face das quatro máquinas
//! (`dev/referencia/rastros/dropper_<máquina>.jsonl`, escrito por `DropperTraceTest.java`): a máquina posta como um jogador a põe (a
//! configuração de lados do item), um dropper com 3 itens de um tipo em cada vizinho, ligado por um bloco de redstone, 10 ticks depois.
//!
//! O dropper do Horizon (`dispense_from`) faz `add_item` pela face da máquina que ele toca: os slots da face na ordem, cada um por
//! `can_place` (as entradas do contêiner da instância, sem face) e pelo hook `container_accepts` (a face deixa entrar e o slot aceita o
//! item de quem está de fora). O item que ninguém aceita fica no dropper: o original também não solta nada no chão. O dispenser nunca
//! insere (`DefaultDispenseItemBehavior`): solta o item.
mod common;

use common::{item, valid_input};
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::sides::Face;
use mekanism_rules::{
    ELECTRIC_ENERGY_SLOT, ELECTRIC_INPUT_SLOT, ENERGY_SLOT, Held, INFUSION_SLOT, INPUT_SLOT,
    Indexes, SlotIndexes, infusing_cache_of, item_instance, item_placed_config, slot_accepts,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];
const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
/// Seis faces por cinco itens por máquina, para o dropper e para o dispenser.
const PHASES: usize = 2 * 6 * 5;

fn read(path: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .filter(|line| !line.contains("\"meta\""))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap()
}

fn slots_of(name: &str) -> SlotIndexes {
    if name == "metallurgic_infuser" {
        SlotIndexes {
            input: INPUT_SLOT,
            energy: ENERGY_SLOT,
            infusion: Some(INFUSION_SLOT),
        }
    } else {
        SlotIndexes {
            input: ELECTRIC_INPUT_SLOT,
            energy: ELECTRIC_ENERGY_SLOT,
            infusion: None,
        }
    }
}

/// O que o Energized Smelter aceitou na entrada: as receitas de cozimento são do jogo, que um teste sem ele não tem; a lista sai da sonda
/// do cursor (`slot_click`), do mesmo original.
fn smelting_inputs() -> Vec<String> {
    let mut items: Vec<String> = read("slot_click_energized_smelter.jsonl")
        .iter()
        .filter(|row| row["kind"] == "input" && row["may_place"] == true)
        .map(|row| row["item"].as_str().unwrap().to_owned())
        .collect();
    items.sort();
    items.dedup();
    items
}

fn indexes_of(name: &str) -> Indexes {
    let electric = |recipes| Indexes::Electric {
        recipes: ItemToItemRecipe::cache_of(recipes),
        energy: common::item_energy(),
    };
    match name {
        "enrichment_chamber" => electric(common::item_recipes_of_type("mekanism:enriching")),
        "crusher" => electric(common::item_recipes_of_type("mekanism:crushing")),
        "energized_smelter" => electric(
            smelting_inputs()
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
                .collect(),
        ),
        _ => {
            let (recipes, conversions) = common::infuser_recipes_by_type();
            Indexes::Infuser {
                recipes: infusing_cache_of(recipes),
                conversions: ChemicalConversion::cache_of(conversions),
                energy: common::item_energy(),
            }
        }
    }
}

/// O slot em que o `add_item` do Horizon deixa um item posto pela face `at`, ou nada.
fn landing(name: &str, at: Face, front: Face, id: &str, indexes: &Indexes) -> Option<usize> {
    let infuser = name == "metallurgic_infuser";
    let instance = item_instance(infuser, &item_placed_config(infuser), front);
    let held = Held {
        chemical: None,
        input: None,
    };
    let (slots, resource) = (slots_of(name), item(id));
    instance.face_slots()[at as usize]
        .iter()
        .copied()
        .find(|slot| {
            instance.inputs().contains(slot)
                && instance.accepts_through(at, *slot)
                && slot_accepts(indexes, &slots, *slot, &resource, &held)
        })
}

#[test]
fn a_dropper_pointed_at_each_face_puts_in_what_the_original_puts_in() {
    let mut inserted = 0;
    let mut refused = 0;
    for name in MACHINES {
        let lines = read(&format!("dropper_{name}.jsonl"));
        let front = face(lines[0]["front"].as_str().unwrap());
        let indexes = indexes_of(name);
        let mut seen = 0;
        for line in &lines[1..] {
            let phase = line["phase"].as_str().unwrap();
            let Some(rest) = phase.strip_prefix("dropper_") else {
                continue;
            };
            let (side, id) = rest.split_once('_').unwrap();
            assert!(FACES.contains(&side), "{phase}");
            let id = if id == valid_input(name) {
                valid_input(name)
            } else {
                id
            };
            let left = line["dropper"][0][1].as_u64().unwrap();
            let slots = line["slots"].as_array().unwrap();
            let landed = landing(name, face(side), front, id, &indexes);
            match landed {
                Some(slot) => {
                    // O dropper deu um item: o original o tem no slot, ou, no slot de infusão, no tanque (a conversão do mesmo tick).
                    assert_eq!(
                        left, 2,
                        "{name} {phase}: o dropper devia ter dado um item: {line}"
                    );
                    if Some(slot) == slots_of(name).infusion {
                        assert!(
                            line["tank"][1].as_u64().unwrap() > 0,
                            "{name} {phase}: tanque: {line}"
                        );
                    } else {
                        assert_eq!(
                            slots[slot][1].as_u64().unwrap(),
                            1,
                            "{name} {phase}: slot {slot}: {line}"
                        );
                    }
                    assert_eq!(
                        slots.iter().filter(|s| s[1].as_u64().unwrap() > 0).count(),
                        usize::from(Some(slot) != slots_of(name).infusion)
                    );
                    inserted += 1;
                }
                None => {
                    assert_eq!(
                        left, 3,
                        "{name} {phase}: o dropper devia ficar com os 3: {line}"
                    );
                    assert!(
                        slots.iter().all(|s| s[1].as_u64().unwrap() == 0),
                        "{name} {phase}: {line}"
                    );
                    refused += 1;
                }
            }
            assert_eq!(
                line["dropped"], 0,
                "{name} {phase}: o dropper não solta no chão o que a máquina recusa"
            );
            seen += 1;
        }
        assert_eq!(seen, PHASES / 2, "{name}: faltam fases do dropper");
    }
    assert!(
        inserted >= 30 && refused >= 80,
        "{inserted} entraram, {refused} recusados"
    );
}

#[test]
fn a_dispenser_drops_the_item_and_never_inserts() {
    for name in MACHINES {
        let lines = read(&format!("dropper_{name}.jsonl"));
        let mut seen = 0;
        for line in &lines[1..] {
            let phase = line["phase"].as_str().unwrap();
            if !phase.starts_with("dispenser_") {
                continue;
            }
            assert_eq!(
                line["dropper"][0][1], 2,
                "{name} {phase}: o dispenser dá um item: {line}"
            );
            assert_eq!(
                line["dropped"], 1,
                "{name} {phase}: e o solta no chão: {line}"
            );
            assert!(
                line["slots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s[1].as_u64().unwrap() == 0),
                "{name} {phase}: {line}"
            );
            seen += 1;
        }
        assert_eq!(seen, PHASES / 2, "{name}: faltam fases do dispenser");
    }
}
