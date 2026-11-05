//! Repete, tick a tick, os rastros gravados da Enrichment Chamber original em `dev/referencia/rastros`.
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler, VanillaContainer};
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{ElectricMachine, RedstoneControl};
use mekanism_rules::{chamber_config, trace_line};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 70;

/// O baú do cenário de ejeção: 27 slots, como o `ChestBlockEntity`.
const CHEST_SLOTS: usize = 27;

struct Chamber(ElectricMachine);

impl SimBlockEntity for Chamber {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn recipe(input: &str, input_count: u32, output: &str, count: u32) -> Rc<ItemToItemRecipe> {
    Rc::new(ItemToItemRecipe {
        id: format!("mekanism:enriching/{output}"),
        input: ItemIngredient::item(input),
        input_count,
        output: ItemOutput {
            resource: item(output),
            count,
        },
    })
}

/// A câmara e o baú ao lado dela, a leste, que só o cenário de ejeção usa.
struct Scene {
    world: SimWorld<Chamber>,
    chest: Rc<VanillaContainer>,
}

fn scene() -> Scene {
    let chest = VanillaContainer::new(CHEST_SLOTS, Rc::new(|| {}));
    let mut world = chamber();
    world.neighbours.set(Face::East, chest.clone());
    Scene { world, chest }
}

fn chamber() -> SimWorld<Chamber> {
    let recipes = vec![
        recipe("clay", 1, "clay_ball", 4),
        recipe("stone", 1, "cracked_stone_bricks", 1),
        recipe("hdpe_pellet", 3, "hdpe_sheet", 1),
    ];
    SimWorld::new(Chamber(ElectricMachine::new(chamber_config(recipes, 0))))
}

/// Enche o baú como o cenário em Java: `fillChest`.
fn fill_chest(chest: &VanillaContainer, id: &str, count: u32) {
    for index in 0..CHEST_SLOTS {
        chest.set_item(index, Some(item(id)), count);
    }
}

/// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
fn apply(scene: &mut Scene, upgrades: &mut (i32, i32), scenario: &str, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    let world = &mut scene.world;
    let machine = &mut world.block_entity.0;
    match parts.as_slice() {
        ["speed", amount] => {
            upgrades.0 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
            machine
                .energy()
                .set_energy(machine.energy().capacity(), None);
        }
        ["energy", amount] if scenario == "chamber_upgrades" => {
            upgrades.1 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
            machine
                .energy()
                .set_energy(machine.energy().capacity(), None);
        }
        ["upgrade", "energy", amount] => {
            upgrades.1 = amount.parse().unwrap();
            machine.set_upgrades(upgrades.0, upgrades.1);
        }
        ["energy", amount] => machine.energy().set_energy(amount.parse().unwrap(), None),
        ["insert", id, amount] | [id @ ("clay" | "hdpe_pellet"), amount] => {
            machine
                .input()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None);
        }
        ["output", id, amount] => {
            machine
                .output()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None)
        }
        ["remove", "input"] => machine.input().set_contents(None, 0, None),
        ["clear", "output"] => machine.output().set_contents(None, 0, None),
        ["swap", "to", "stone"] => machine.input().set_contents(Some(item("stone")), 1, None),
        ["mode", mode] => machine.tile_mut().set_control_type(match *mode {
            "high" => RedstoneControl::High,
            "low" => RedstoneControl::Low,
            "pulse" => RedstoneControl::Pulse,
            other => panic!("modo desconhecido: {other}"),
        }),
        ["signal", "on"] => world.set_signal(true),
        ["signal", "off"] => world.set_signal(false),
        // `ejectEast`: a saída vai para o lado do baú, e o ejetor liga.
        ["eject", "on"] => {
            let ejector = machine.ejector_mut();
            ejector.set_sides(0, vec![Face::East]);
            ejector.set_ejecting(true);
        }
        ["chest", "room", "2"] => {
            fill_chest(&scene.chest, "stone", 64);
            scene
                .chest
                .set_item(CHEST_SLOTS - 1, Some(item("clay_ball")), 62);
        }
        ["chest", "room", "0"] => fill_chest(&scene.chest, "stone", 64),
        ["chest", "clear"] => scene.chest.clear(),
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn slot(slot: &InventorySlot) -> Value {
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
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
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| Value::from(*name))
        .collect()
}

/// O baú como o rastro o grava: só os slots com algo, `[índice, id, quantidade]`.
fn chest(chest: &VanillaContainer) -> Value {
    chest
        .stacks()
        .into_iter()
        .map(|(index, resource, amount)| {
            serde_json::json!([index, resource.id.to_string(), amount])
        })
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
    let mut scene = scene();
    let mut upgrades = (0, 0);
    for expected in &lines {
        scene.world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&mut scene, &mut upgrades, scenario, action);
            }
        }
        let machine = &scene.world.block_entity.0;
        let mut actual = serde_json::json!({
            "t": expected["t"],
            "marks": expected["marks"],
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
        });
        // O `MEKTRACE` do jogo tem os mesmos campos dos rastros, com as marcas vazias.
        let mut traced: Value =
            serde_json::from_str(&trace_line(expected["t"].as_i64().unwrap(), machine)).unwrap();
        traced["marks"] = expected["marks"].clone();
        assert_eq!(
            traced, actual,
            "{scenario}, tick {}: MEKTRACE",
            expected["t"]
        );
        // Só o cenário de ejeção grava o baú.
        if expected.get("chest").is_some() {
            actual["chest"] = chest(&scene.chest);
        }
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

#[test]
fn the_output_ejects_into_a_chest_with_room_for_all_part_and_none() {
    replay("chamber_eject");
}

/// O baú de `ItemHandler` serve também de alvo: confere que o `Rc` dele vira o handler que o mundo entrega.
#[test]
fn the_chest_is_the_handler_the_east_side_reaches() {
    let scene = scene();
    let handler: Option<Rc<dyn ItemHandler>> = scene.world.neighbours.item_handler(Face::East);
    assert_eq!(handler.map(|handler| handler.size()), Some(CHEST_SLOTS));
}

/// Salvar e reabrir no meio de uma operação: o objeto nasce de novo dos itens, da energia e do progresso salvos, que é o
/// que o glue faz com a `ModBlockEntity`, e a operação segue de onde parou.
#[test]
fn a_reborn_machine_goes_on_from_the_saved_progress() {
    let mut before = chamber();
    {
        let machine = &before.block_entity.0;
        machine.input().set_contents(Some(item("clay")), 3, None);
        machine.energy().set_energy(20, None);
    }
    for _ in 0..60 {
        tick_with_energy(&mut before);
    }
    let machine = &before.block_entity.0;
    let (progress, energy) = (machine.recipe_progress(), machine.energy().amount());
    let (input, output) = (machine.input().contents(), machine.output().contents());
    assert!(progress > 0 && progress < 200, "no meio da operação");

    let mut reborn = chamber();
    {
        let machine = &reborn.block_entity.0;
        machine.load_operating_ticks(progress);
        machine
            .input()
            .set_contents(input.resource, input.amount, None);
        machine
            .output()
            .set_contents(output.resource, output.amount, None);
        machine.energy().set_energy(energy, None);
    }
    tick_with_energy(&mut reborn);
    assert_eq!(reborn.block_entity.0.recipe_progress(), progress + 1);
}

/// Um tick com a energia sempre cheia, como um cabo que a mantém.
fn tick_with_energy(world: &mut SimWorld<Chamber>) {
    let energy = world.block_entity.0.energy();
    energy.set_energy(energy.capacity(), None);
    world.tick();
}
