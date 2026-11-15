//! Repete, tick a tick, o slot de energia das máquinas com um item que guarda energia (`EnergyInventorySlot.fillContainerFromSlot`, `dev/referencia/rastros/chamber_energy_items.jsonl` e
//! `infuser_energy_items.jsonl`, escritos por `MachineTraceTest.java`): um Energy Tablet que entrega a taxa de carga dele por tick, o mesmo quase vazio, o equipamento e o item de máquina,
//! que só entregam a quem é `MANUAL`, a pilha de dois cubos, o contêiner com espaço para parte do que o item tem, o tablet vazio e, no Infuser, a conversão de redstone depois de o item
//! falhar. A cada tick o rastro grava a energia da máquina e o slot de energia com o caminho do item, a quantidade e a energia da pilha.
mod common;

use machinery_basis_core::cached_recipe::ItemToItemRecipe;
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{
    ENERGY_COMPONENT, ENRICHMENT_USAGE, INFUSER_USAGE, electric_config, infuser_config,
    infusing_cache_of,
};
use serde_json::Value;
use std::path::Path;

struct Machine(ProgressMachine);

impl SimBlockEntity for Machine {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

/// Os itens do cenário com o namespace que o recurso tem no jogo; o que não é do Mekanism (`redstone`) fica como a receita o pede.
fn resource(name: &str, energy: i64) -> ItemResource {
    let id = match name {
        "energy_tablet" | "atomic_disassembler" | "basic_energy_cube" | "enrichment_chamber" => {
            format!("mekanism:{name}")
        }
        other => other.to_owned(),
    };
    let stack = if matches!(name, "basic_energy_cube" | "redstone") {
        64
    } else {
        1
    };
    if energy == 0 {
        ItemResource::new(&id, stack)
    } else {
        ItemResource::with_component(&id, stack, ENERGY_COMPONENT, &energy.to_string())
    }
}

/// O slot como o rastro o grava: o caminho do item, a quantidade e a energia da pilha.
fn energy_slot(machine: &ProgressMachine) -> Value {
    let slot = machine.energy_slot();
    let Some(resource) = slot.resource() else {
        return serde_json::json!(["", 0, 0]);
    };
    let energy = resource
        .components
        .get(ENERGY_COMPONENT)
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(0);
    let path = resource.id.rsplit(':').next().unwrap();
    serde_json::json!([path, slot.amount(), energy])
}

fn apply(machine: &ProgressMachine, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    match parts.as_slice() {
        ["energy", amount] => machine.energy().set_energy(amount.parse().unwrap(), None),
        ["max", "energy", amount] => machine.energy().set_max_energy(amount.parse().unwrap()),
        ["energy", "slot", name, energy, count] => machine.energy_slot().set_contents(
            Some(resource(name, energy.parse().unwrap())),
            count.parse().unwrap(),
            None,
        ),
        ["energy", "slot", name, count] => machine.energy_slot().set_contents(
            Some(resource(name, 0)),
            count.parse().unwrap(),
            None,
        ),
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn replay(scenario: &str, machine: ProgressMachine) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines.len() >= 60, "rastro curto demais: {}", lines.len());
    let mut world = SimWorld::new(Machine(machine));
    let mut moved = 0;
    let mut last = 0;
    for expected in &lines {
        world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&world.block_entity.0, action);
            }
        }
        let machine = &world.block_entity.0;
        assert_eq!(
            machine.energy().amount(),
            expected["energy"].as_i64().unwrap(),
            "{scenario}, tick {}: energia da máquina",
            expected["t"]
        );
        assert_eq!(
            energy_slot(machine),
            expected["energy_slot"],
            "{scenario}, tick {}: slot de energia",
            expected["t"]
        );
        let now = expected["energy"].as_i64().unwrap();
        if now != last {
            moved += 1;
        }
        last = now;
    }
    // O cenário tem de ter mexido na energia da máquina por itens: um rastro parado passaria no que está acima.
    assert!(moved >= 8, "{scenario}: a energia mexeu só {moved} vezes");
}

fn chamber() -> ProgressMachine {
    let mut config = electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(Vec::new()),
        0,
        common::upgrade_data(),
    );
    config.items = common::gear_item_energy();
    ProgressMachine::electric(config)
}

fn infuser() -> ProgressMachine {
    let mut config = infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(Vec::new()),
        ChemicalConversion::cache_of(Vec::new()),
        0,
        common::upgrade_data(),
    );
    config.items = common::gear_item_energy();
    ProgressMachine::chemical(config)
}

/// O tablet entrega a taxa dele por tick até o contêiner encher ou o item esvaziar; o equipamento (`manualOnly`) e o item de máquina não entregam a quem está de fora; a pilha de dois
/// cubos entrega por item; com espaço para 2 só entram 2; o tablet vazio não entrega.
#[test]
fn the_chamber_fills_from_the_item_in_its_energy_slot() {
    replay("chamber_energy_items", chamber());
}

/// O Infuser tenta o item primeiro e converte a redstone só quando o item não deu nada.
#[test]
fn the_infuser_fills_from_the_item_and_converts_only_when_the_item_fails() {
    replay("infuser_energy_items", infuser());
}
