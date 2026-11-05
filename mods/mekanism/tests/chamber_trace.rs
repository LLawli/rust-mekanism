//! Repete, tick a tick, os rastros gravados da Enrichment Chamber original em `dev/referencia/rastros`.
use machinery_basis_core::cached_recipe::{ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{
    ElectricMachine, ElectricMachineConfig, RedstoneControl, UpgradeRules,
};
use serde_json::Value;
use std::path::Path;

const MINIMUM_LINES: usize = 70;

struct Chamber(ElectricMachine);

impl SimBlockEntity for Chamber {
    fn tick_server(&mut self, game_time: u64) {
        self.0.tick_server(game_time);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.state.tile.update_power(powered);
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn recipe(input: &str, input_count: u32, output: &str, count: u32) -> ItemToItemRecipe {
    ItemToItemRecipe {
        id: format!("mekanism:enriching/{output}"),
        input: item(input),
        input_count,
        output: ItemOutput {
            resource: item(output),
            count,
        },
    }
}

fn chamber() -> SimWorld<Chamber> {
    SimWorld::new(Chamber(ElectricMachine::new(ElectricMachineConfig {
        energy_per_tick: 5,
        base_ticks_required: 200,
        deactivation_delay: 60,
        upgrades: UpgradeRules {
            max_upgrade_multiplier: 10.0,
            max_speed: 8,
            max_energy: 8,
        },
        recipes: vec![
            recipe("clay", 1, "clay_ball", 4),
            recipe("stone", 1, "cracked_stone_bricks", 1),
            recipe("hdpe_pellet", 3, "hdpe_sheet", 1),
        ],
        tracked_errors: vec![
            RecipeError::NOT_ENOUGH_ENERGY,
            RecipeError::NOT_ENOUGH_INPUT,
            RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
            RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
        ],
        check_offset: 0,
    })))
}

/// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
fn apply(world: &mut SimWorld<Chamber>, upgrades: &mut (i32, i32), scenario: &str, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    let machine = &mut world.block_entity.0;
    let state = &mut machine.state;
    match parts.as_slice() {
        ["speed", amount] => {
            upgrades.0 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
            let capacity = machine.state.energy.capacity();
            machine.state.energy.set_energy(capacity);
        }
        ["energy", amount] if scenario == "chamber_upgrades" => {
            upgrades.1 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
            let capacity = machine.state.energy.capacity();
            machine.state.energy.set_energy(capacity);
        }
        ["upgrade", "energy", amount] => {
            upgrades.1 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
        }
        ["energy", amount] => state.energy.set_energy(amount.parse().unwrap()),
        ["insert", id, amount] | [id @ ("clay" | "hdpe_pellet"), amount] => {
            state
                .input
                .set_contents(Some(item(id)), amount.parse().unwrap());
        }
        ["output", id, amount] => state
            .output
            .set_contents(Some(item(id)), amount.parse().unwrap()),
        ["remove", "input"] => state.input.set_contents(None, 0),
        ["clear", "output"] => state.output.set_contents(None, 0),
        ["swap", "to", "stone"] => state.input.set_contents(Some(item("stone")), 1),
        ["mode", mode] => state.tile.set_control_type(match *mode {
            "high" => RedstoneControl::High,
            "low" => RedstoneControl::Low,
            "pulse" => RedstoneControl::Pulse,
            other => panic!("modo desconhecido: {other}"),
        }),
        ["signal", "on"] => world.set_signal(true),
        ["signal", "off"] => world.set_signal(false),
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn slot(slot: &machinery_basis_core::containers::InventorySlot) -> Value {
    let id = slot.resource().map_or("", |resource| resource.id.as_str());
    serde_json::json!([id, slot.amount()])
}

fn errors(machine: &ElectricMachine) -> Value {
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
    ];
    names
        .iter()
        .filter(|(error, _)| machine.state.has_error(*error))
        .map(|(_, name)| Value::from(*name))
        .collect()
}

fn replay(scenario: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    let mut world = chamber();
    let mut upgrades = (0, 0);
    for expected in &lines {
        world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&mut world, &mut upgrades, scenario, action);
            }
        }
        let machine = &world.block_entity.0;
        let state = &machine.state;
        let actual = serde_json::json!({
            "t": expected["t"],
            "marks": expected["marks"],
            "progress": state.recipe_progress(),
            "required": state.ticks_required(),
            "energy": state.energy.amount(),
            "capacity": state.energy.capacity(),
            "per_tick": state.energy.energy_per_tick(),
            "active": state.tile.active(),
            "block_active": state.tile.block_active(),
            "powered": state.tile.is_powered(),
            "comparator": state.tile.current_redstone_level(),
            "input": slot(&state.input),
            "output": slot(&state.output),
            "errors": errors(machine),
        });
        assert_eq!(&actual, expected, "{scenario}, tick {}", expected["t"]);
    }
}

#[test]
fn energy_running_out_and_coming_back() {
    replay("chamber_energy");
}

#[test]
fn input_removed_and_swapped() {
    replay("chamber_input_removed");
}

#[test]
fn input_below_what_the_recipe_needs() {
    replay("chamber_input_short");
}

#[test]
fn output_without_room_or_with_another_item() {
    replay("chamber_output");
}

#[test]
fn redstone_modes() {
    replay("chamber_redstone");
}

#[test]
fn deactivation_delay() {
    replay("chamber_active_delay");
}

#[test]
fn a_paused_recipe_ignores_what_does_not_notify_it() {
    replay("chamber_paused");
}

#[test]
fn upgrades() {
    replay("chamber_upgrades");
}
