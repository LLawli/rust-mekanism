//! Repete, tick a tick, os rastros gravados da Enrichment Chamber original em `dev/referencia/rastros`.
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::item_handler::{
    ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer,
};
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{ElectricMachine, RedstoneControl};
use mekanism_rules::{CRUSHER_USAGE, ENRICHMENT_USAGE, SMELTER_USAGE, electric_config, trace_line};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 70;

/// O baú do cenário de ejeção: 27 slots, como o `ChestBlockEntity`.
const CHEST_SLOTS: usize = 27;
/// `AbstractFurnaceBlockEntity.SLOT_FUEL`.
const FURNACE_FUEL_SLOT: usize = 1;

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
    /// O número do primeiro slot que o lado alcança: o `1` do combustível, no forno.
    first_slot: usize,
}

fn scene() -> Scene {
    let chest = VanillaContainer::new(CHEST_SLOTS, Rc::new(|| {}));
    let mut world = chamber();
    world.neighbours.set(Face::East, chest.clone());
    Scene {
        world,
        chest,
        first_slot: 0,
    }
}

/// O forno pela lateral: só o slot de combustível, e só o que queima.
fn furnace_scene() -> Scene {
    let fuel = VanillaContainer::new(1, Rc::new(|| {}));
    let mut world = chamber();
    let burns = |_: usize, resource: &ItemResource| matches!(&*resource.id, "coal" | "charcoal");
    world
        .neighbours
        .set(Face::East, Rc::new(RuledHandler::new(fuel.clone(), burns)));
    Scene {
        world,
        chest: fuel,
        first_slot: FURNACE_FUEL_SLOT,
    }
}

fn machine(energy_per_tick: i32, recipes: Vec<Rc<ItemToItemRecipe>>) -> SimWorld<Chamber> {
    SimWorld::new(Chamber(ElectricMachine::new(electric_config(
        energy_per_tick,
        recipes,
        0,
    ))))
}

fn chamber() -> SimWorld<Chamber> {
    let recipes = vec![
        recipe("clay", 1, "clay_ball", 4),
        recipe("stone", 1, "cracked_stone_bricks", 1),
        recipe("hdpe_pellet", 3, "hdpe_sheet", 1),
    ];
    machine(ENRICHMENT_USAGE, recipes)
}

/// Só a máquina: Crusher e Energized Smelter são a mesma `TileEntityElectricMachine` da câmara, com outro consumo e outras
/// receitas.
fn alone(world: SimWorld<Chamber>) -> Scene {
    Scene {
        world,
        chest: VanillaContainer::new(0, Rc::new(|| {})),
        first_slot: 0,
    }
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
        ["item", id, amount] => {
            machine
                .input()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None);
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
fn chest(chest: &VanillaContainer, first_slot: usize) -> Value {
    chest
        .stacks()
        .into_iter()
        .map(|(index, resource, amount)| {
            serde_json::json!([index + first_slot, resource.id.to_string(), amount])
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
    let mut scene = match scenario {
        "chamber_eject_furnace" => furnace_scene(),
        "crusher_run" => alone(machine(
            CRUSHER_USAGE,
            vec![recipe("cobblestone", 1, "gravel", 1)],
        )),
        "smelter_run" => alone(machine(
            SMELTER_USAGE,
            vec![recipe("raw_iron", 1, "iron_ingot", 1)],
        )),
        _ => scene(),
    };
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
        // Só o cenário de ejeção grava o baú.
        if expected.get("chest").is_some() {
            actual["chest"] = chest(&scene.chest, scene.first_slot);
        }
        // O `MEKTRACE` do jogo tem os mesmos campos dos rastros, com as marcas vazias.
        let stacks: Vec<(usize, String, u32)> = scene
            .chest
            .stacks()
            .into_iter()
            .map(|(index, resource, amount)| {
                (index + scene.first_slot, resource.id.to_string(), amount)
            })
            .collect();
        let with_chest = expected.get("chest").is_some().then_some(stacks.as_slice());
        let line = trace_line(expected["t"].as_i64().unwrap(), machine, with_chest);
        let mut traced: Value = serde_json::from_str(&line).unwrap();
        traced["marks"] = expected["marks"].clone();
        assert_eq!(
            traced, actual,
            "{scenario}, tick {}: MEKTRACE",
            expected["t"]
        );
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
fn the_crusher_uses_its_own_energy_and_recipes() {
    replay("crusher_run");
}

#[test]
fn the_smelter_uses_its_own_energy_and_recipes() {
    replay("smelter_run");
}

#[test]
fn the_output_only_enters_a_furnace_side_as_fuel() {
    replay("chamber_eject_furnace");
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

/// Tudo o que o Java persiste volta: redstone, modo, estado ativo e atraso, nível do comparador, progresso, receitas usadas,
/// a última e o ejetor. Nada de `pausedForErrors`: o objeto novo anda como o Java depois de carregar.
#[test]
fn the_saved_state_round_trips_everything_the_java_persists() {
    let mut before = chamber();
    {
        let machine = &mut before.block_entity.0;
        machine.input().set_contents(Some(item("clay")), 3, None);
        machine.tile_mut().set_control_type(RedstoneControl::High);
        machine.tile_mut().update_power(true);
        machine.ejector_mut().set_ejecting(true);
    }
    // Uma operação inteira (200 ticks) e mais um pouco da seguinte.
    for _ in 0..230 {
        tick_with_energy(&mut before);
    }
    let saved = before.block_entity.0.saved_state();
    assert_eq!(saved.control_type, RedstoneControl::High);
    assert!(saved.redstone && saved.active && saved.ejecting);
    assert!(saved.progress > 0 && saved.progress < 200);
    assert_eq!(
        saved.recipes_used.get("mekanism:enriching/clay_ball"),
        Some(&1)
    );
    assert_eq!(
        saved.last_recipe_used.as_deref(),
        Some("mekanism:enriching/clay_ball")
    );
    assert_eq!(saved.last_recipe_amount, 1);

    let mut reborn = chamber();
    reborn.block_entity.0.restore(&saved, true);
    assert_eq!(reborn.block_entity.0.saved_state(), saved);
    assert!(reborn.block_entity.0.tile().block_active());
}
