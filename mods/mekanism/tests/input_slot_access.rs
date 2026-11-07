//! `InputInventorySlot` passa `ConstantPredicates.notExternal()` como `canExtract` (`InputInventorySlot.java`, construtor
//! protegido, `super(capacity, ConstantPredicates.notExternal(), insertPredicate, ..)`), e `BasicResourceContainer
//! .isCurrentValidForExtraction` testa esse predicado com o `AutomationType` de quem pede. Quem está de fora (funil, cano, o
//! `ItemHandler` de um vizinho) não tira a entrada da máquina; o jogador (manual) e a própria máquina (interno) tiram.
//! A saída é o contrário: `OutputInventorySlot` extrai com qualquer um (`alwaysTrueBi`).
//!
//! Vale para os slots de entrada das quatro máquinas: a Enrichment Chamber, o Crusher e o Energized Smelter (os três
//! `ProgressMachine::electric`) e o Metallurgic Infuser (`ProgressMachine::chemical`). Trocar `Access::NotExternal` por
//! `Access::Any` em `tile.rs` tem de fazer este teste falhar (mutação).
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::chemical::ChemicalIngredient;
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use mekanism_rules::{
    ENRICHMENT_USAGE, INFUSER_USAGE, electric_config, infuser_config, infusing_cache_of,
};
use std::rc::Rc;

const STACK: u32 = 64;
const DIRT: &str = "minecraft:dirt";

fn dirt() -> ItemResource {
    ItemResource::new(DIRT, STACK)
}

fn chamber() -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: "dirt".to_owned(),
        input: ItemIngredient::item(DIRT),
        input_count: 1,
        output: ItemOutput {
            resource: ItemResource::new("minecraft:podzol", STACK),
            count: 1,
        },
    });
    ProgressMachine::electric(electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        common::upgrade_data(),
    ))
}

fn infuser() -> ProgressMachine {
    let recipe = Rc::new(ItemChemicalToItemRecipe {
        id: "dirt".to_owned(),
        item_input: ItemIngredient::item(DIRT),
        item_count: 1,
        chemical_input: ChemicalIngredient::chemical("a:bio"),
        chemical_amount: 10,
        output: ItemOutput {
            resource: ItemResource::new("minecraft:podzol", STACK),
            count: 1,
        },
        per_tick_usage: false,
    });
    ProgressMachine::chemical(infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(vec![recipe]),
        ChemicalConversion::cache_of(Vec::new()),
        0,
        common::upgrade_data(),
    ))
}

fn pulled(slot: &InventorySlot, automation: AutomationType) -> u32 {
    let transaction = Transaction::open_root();
    let moved = slot.extract(&dirt(), 1, &transaction, automation);
    // Só mede: nada fica gravado, para o próximo pedido ver a mesma entrada.
    drop(transaction);
    moved
}

fn fill(slot: &InventorySlot) {
    slot.set_contents(Some(dirt()), 3, None);
}

fn only_outsiders_are_refused(machine: &ProgressMachine, name: &str) {
    let input = machine.input();
    fill(input);
    assert_eq!(
        pulled(input, AutomationType::External),
        0,
        "{name}: de fora"
    );
    assert_eq!(pulled(input, AutomationType::Manual), 1, "{name}: jogador");
    assert_eq!(
        pulled(input, AutomationType::Internal),
        1,
        "{name}: a máquina"
    );
}

#[test]
fn the_input_slot_refuses_external_extraction_like_input_inventory_slot() {
    only_outsiders_are_refused(&chamber(), "Enrichment Chamber, Crusher, Energized Smelter");
    only_outsiders_are_refused(&infuser(), "Metallurgic Infuser");
}

#[test]
fn the_output_slot_still_gives_to_outsiders() {
    for (name, machine) in [("electric", chamber()), ("chemical", infuser())] {
        let output = machine.output();
        output.set_contents(Some(dirt()), 3, None);
        assert_eq!(pulled(output, AutomationType::External), 1, "{name}");
    }
}
