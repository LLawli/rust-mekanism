//! Repete, tick a tick, o rastro `infuser_arrival.jsonl` (`ArrivalTraceTest`): o Metallurgic Infuser com o tanque vazio, recebendo o
//! item de entrada de uma receita e o item que vira o químico dela, em três ordens (a entrada primeiro, a conversão primeiro e os
//! dois no mesmo tick). Um par por receita de infusão portada e por item que converte para o químico dela (bio, fungi, redstone,
//! diamond, gold e refined_obsidian; carbon e tin não têm receita no port).
//!
//! A máquina é a dos arquivos que o port entrega (`common::infuser_from_content`): as receitas `metallurgic_infusing` e as conversões
//! `chemical_conversion`, com as tags do conteúdo. Cada item chega como um jogador o põe (`insert` com `Manual`, o que o menu
//! usa), e a marca guarda quanto o slot aceitou no original.
mod common;

use common::{infuser_from_content, item};
use machinery_basis_core::containers::{AutomationType, InventorySlot};
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules::trace_line;
use serde_json::Value;
use std::path::Path;

/// Os pares do cenário: 13, cada um em 3 ordens.
const PAIRS: usize = 13;
const ORDERS: usize = 3;

struct Infuser(ProgressMachine);

impl SimBlockEntity for Infuser {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn slot(slot: &InventorySlot) -> Value {
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
    serde_json::json!([id, slot.amount()])
}

fn errors(machine: &ProgressMachine) -> Value {
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
        (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
    ];
    names
        .iter()
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| Value::from(*name))
        .collect()
}

fn state(t: &Value, marks: &Value, machine: &ProgressMachine) -> Value {
    let side = machine.chemical_side().unwrap();
    serde_json::json!({
        "t": t,
        "marks": marks,
        "progress": machine.recipe_progress(),
        "required": machine.ticks_required(),
        "energy": machine.energy().amount(),
        "capacity": machine.energy().capacity(),
        "per_tick": machine.energy().energy_per_tick(),
        "active": machine.tile().active(),
        "block_active": machine.tile().block_active(),
        "powered": machine.tile().is_powered(),
        "comparator": machine.tile().current_redstone_level(),
        "input": slot(machine.input()),
        "output": slot(machine.output()),
        "errors": errors(machine),
        "tank": side.tank.amount(),
        "tank_id": side.tank.resource().map_or(String::new(), |resource| resource.id.to_string()),
        "infusion": slot(&side.slot),
    })
}

/// Uma marca do rastro: `reset` esvazia a máquina e enche a energia; `arrive <par> <input|conversion> <item> 2 accepted <n>` põe o
/// item como um jogador e confere quanto o slot aceitou.
fn apply(machine: &ProgressMachine, mark: &str, arrivals: &mut usize) {
    let parts: Vec<&str> = mark.split_whitespace().collect();
    let side = machine.chemical_side().unwrap();
    match parts.as_slice() {
        ["reset"] => {
            machine.input().set_contents(None, 0, None);
            machine.output().set_contents(None, 0, None);
            machine.energy_slot().set_contents(None, 0, None);
            side.slot.set_contents(None, 0, None);
            side.tank.set_contents(None, 0, None);
            machine
                .energy()
                .set_energy(machine.energy().capacity(), None);
        }
        ["arrive", pair, which, id, amount, "accepted", accepted] => {
            let target = if *which == "input" {
                machine.input()
            } else {
                &side.slot
            };
            let transaction = Transaction::open_root();
            let moved = target.insert(
                &item(id),
                amount.parse().unwrap(),
                &transaction,
                AutomationType::Manual,
            );
            transaction.commit();
            assert_eq!(
                moved,
                accepted.parse::<u32>().unwrap(),
                "{pair} {which} {id}: quanto o slot aceitou"
            );
            *arrivals += 1;
        }
        other => panic!("marca desconhecida: {other:?}"),
    }
}

#[test]
fn the_infuser_follows_the_recorded_arrivals_tick_by_tick() {
    let lines = read_trace("infuser_arrival");
    assert!(lines.len() >= 900, "rastro curto demais: {}", lines.len());
    let mut world = SimWorld::new(Infuser(infuser_from_content()));
    let (mut arrivals, mut converted, mut processing) = (0, 0, 0);
    for expected in &lines {
        world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            apply(&world.block_entity.0, mark.as_str().unwrap(), &mut arrivals);
        }
        let machine = &world.block_entity.0;
        let actual = state(&expected["t"], &expected["marks"], machine);
        // O `MEKTRACE` do jogo tem os mesmos campos do rastro, sem o id do químico.
        let line = trace_line(expected["t"].as_i64().unwrap(), machine, None, false);
        let mut traced: Value = serde_json::from_str(&line).unwrap();
        traced["marks"] = expected["marks"].clone();
        traced["tank_id"] = actual["tank_id"].clone();
        assert_eq!(traced, actual, "tick {}: MEKTRACE", expected["t"]);
        assert_eq!(&actual, expected, "tick {}", expected["t"]);
        converted += i32::from(expected["tank"].as_u64().unwrap() > 0);
        processing += i32::from(expected["progress"].as_i64().unwrap() > 0);
        // A energia fica cheia depois de cada tick, como no cenário.
        let energy = machine.energy();
        energy.set_energy(energy.capacity(), None);
    }
    assert_eq!(arrivals, PAIRS * ORDERS * 2, "as chegadas do cenário");
    // O rastro tem de ter o efeito: o tanque recebe a conversão e a receita anda, nas três ordens de cada par.
    assert!(
        converted >= PAIRS as i32 * ORDERS as i32 * 10,
        "{converted}"
    );
    assert!(
        processing >= PAIRS as i32 * ORDERS as i32 * 5,
        "{processing}"
    );
}
