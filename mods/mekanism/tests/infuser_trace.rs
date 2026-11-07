//! Repete, tick a tick, o rastro gravado do Metallurgic Infuser original em `dev/referencia/rastros/infuser_run.jsonl`: o tanque
//! que falta, o que esvazia no meio, o item sem receita e o item de infusão convertido a cada tick.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalIngredient, ChemicalResource};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use mekanism_rules::{INFUSER_USAGE, TANK_CAPACITY, infuser_config, infusing_cache_of, trace_line};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 500;
/// As receitas de `metallurgic_infusing` em `mods/mekanism/content/data/mekanism/recipe`.
const RECIPE_FILES: usize = 19;

struct Infuser(ProgressMachine);

impl SimBlockEntity for Infuser {
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

fn redstone() -> ChemicalResource {
    ChemicalResource::new("redstone")
}

fn recipe(
    input: &str,
    chemical: &str,
    amount: u32,
    output: &str,
    count: u32,
) -> Rc<ItemChemicalToItemRecipe> {
    Rc::new(ItemChemicalToItemRecipe {
        id: format!("mekanism:metallurgic_infusing/{output}"),
        item_input: ItemIngredient::item(input),
        item_count: 1,
        chemical_input: ChemicalIngredient::chemical(chemical),
        chemical_amount: amount,
        output: ItemOutput {
            resource: item(output),
            count,
        },
        per_tick_usage: false,
    })
}

fn conversion(input: &str, chemical: &str, amount: i32) -> ChemicalConversion {
    ChemicalConversion {
        input: ItemIngredient::item(input),
        input_count: 1,
        output: ChemicalResource::new(chemical),
        amount,
    }
}

fn infuser() -> SimWorld<Infuser> {
    let recipes = vec![
        recipe("copper_ingot", "redstone", 10, "alloy_infused", 1),
        recipe("sand", "bio", 10, "dirt", 1),
        recipe("dirt", "bio", 10, "podzol", 1),
        recipe("dirt", "fungi", 10, "mycelium", 1),
    ];
    let conversions = vec![
        conversion("redstone", "redstone", 10),
        conversion("redstone_block", "redstone", 90),
        conversion("mushroom", "fungi", 10),
    ];
    let mut config = infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(recipes),
        ChemicalConversion::cache_of(conversions),
        0,
        common::upgrade_data(),
    );
    // As receitas `energy_conversion` que o port entrega, como o jogo as dá a toda máquina.
    config.energy_conversion = common::energy_conversions();
    SimWorld::new(Infuser(ProgressMachine::chemical(config)))
}

/// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
fn apply(machine: &ProgressMachine, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    let side = machine.chemical_side().unwrap();
    match parts.as_slice() {
        ["item", id, amount] => {
            machine
                .input()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None)
        }
        ["tank", id, amount] => {
            let amount: u64 = amount.parse().unwrap();
            let resource = (amount > 0).then(|| ChemicalResource::new(id));
            side.tank.set_contents(resource, amount, None);
        }
        ["infusion", id, amount] => {
            side.slot
                .set_contents(Some(item(id)), amount.parse().unwrap(), None)
        }
        ["energy", amount] => machine.energy().set_energy(amount.parse().unwrap(), None),
        ["max", "energy", amount] => machine.energy().set_max_energy(amount.parse().unwrap()),
        ["energy", "slot", id, amount] => {
            machine
                .energy_slot()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None);
        }
        other => panic!("marca desconhecida: {other:?}"),
    }
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
        "infusion": slot(&side.slot),
    })
}

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_infuser_follows_the_recorded_run_tick_by_tick() {
    let lines = read_trace("infuser_run");
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    let mut world = infuser();
    let mut seen_marks = 0;
    for expected in &lines {
        world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            seen_marks += 1;
            for action in mark.as_str().unwrap().split(", ") {
                apply(&world.block_entity.0, action);
            }
        }
        let machine = &world.block_entity.0;
        let actual = state(&expected["t"], &expected["marks"], machine);
        // O `MEKTRACE` do jogo tem os mesmos campos dos rastros, com as marcas vazias.
        let line = trace_line(expected["t"].as_i64().unwrap(), machine, None, false);
        let mut traced: Value = serde_json::from_str(&line).unwrap();
        traced["marks"] = expected["marks"].clone();
        assert_eq!(traced, actual, "tick {}: MEKTRACE", expected["t"]);
        assert_eq!(&actual, expected, "tick {}", expected["t"]);
    }
    assert_eq!(seen_marks, 7, "as marcas do cenário");
}

/// O slot de energia do Infuser converte `redstone` a cada tick, antes do slot de infusão, e cada um tem o seu contêiner: o
/// tanque recebe 10 de redstone por item e a energia 10.000, quando cabe.
#[test]
fn the_infuser_converts_redstone_into_energy_and_into_infusion_in_the_same_tick() {
    let lines = read_trace("infuser_energy_conversion");
    assert!(lines.len() >= 50, "rastro curto demais: {}", lines.len());
    let mut world = infuser();
    let (mut jumps, mut before) = (0, 0);
    for expected in &lines {
        world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&world.block_entity.0, action);
            }
        }
        let machine = &world.block_entity.0;
        let mut actual = state(&expected["t"], &expected["marks"], machine);
        actual["energy_slot"] = slot(machine.energy_slot());
        assert_eq!(&actual, expected, "tick {}", expected["t"]);
        // O `MEKTRACE` do jogo tem os mesmos campos do rastro, sem o slot de energia nem as marcas.
        let line = trace_line(expected["t"].as_i64().unwrap(), machine, None, false);
        let mut traced: Value = serde_json::from_str(&line).unwrap();
        traced["marks"] = expected["marks"].clone();
        traced["energy_slot"] = actual["energy_slot"].clone();
        assert_eq!(traced, actual, "tick {}: MEKTRACE", expected["t"]);
        // O rastro tem de ter o efeito: a energia sobe 10.000 de uma vez, em mais de um tick.
        let energy = expected["energy"].as_i64().unwrap();
        jumps += i32::from(energy - before >= 9_000);
        before = energy;
    }
    assert!(jumps >= 3, "o rastro só converte {jumps} vezes");
}

/// As 19 receitas de infusão do Mekanism gastam o químico de uma vez, no fim: nenhuma tem `per_tick_usage`, e é por isso
/// que o `ProgressMachine::chemical` monta só o `TwoInputCachedRecipe`, sem escolher a receita de uso constante.
#[test]
fn no_infusing_recipe_spends_the_chemical_per_tick() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanism/recipe/metallurgic_infusing");
    let mut files = Vec::new();
    let mut pending = vec![dir];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    assert_eq!(files.len(), RECIPE_FILES);
    for file in files {
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(json["per_tick_usage"], false, "{file:?}");
    }
}

/// Um tick com a energia sempre cheia, como um cabo que a mantém.
fn tick_with_energy(world: &mut SimWorld<Infuser>) {
    let energy = world.block_entity.0.energy();
    energy.set_energy(energy.capacity(), None);
    world.tick();
}

/// O tanque só recebe a conversão se o item da entrada combina com ela (`containsRecipeBA`).
#[test]
fn a_conversion_the_input_slot_does_not_pair_with_stays_in_the_slot() {
    let mut world = infuser();
    {
        let machine = &world.block_entity.0;
        machine
            .input()
            .set_contents(Some(item("copper_ingot")), 1, None);
        let slot = &machine.chemical_side().unwrap().slot;
        slot.set_contents(Some(item("mushroom")), 2, None);
    }
    world.tick();
    let machine = &world.block_entity.0;
    let side = machine.chemical_side().unwrap();
    assert_eq!(side.slot.amount(), 2, "fungi não combina com cobre");
    assert_eq!(side.tank.amount(), 0);
    // Sem item na entrada qualquer químico de alguma receita entra.
    machine.input().set_contents(None, 0, None);
    world.tick();
    let side = world.block_entity.0.chemical_side().unwrap();
    assert_eq!(side.slot.amount(), 1);
    assert_eq!(side.tank.amount(), 10);
}

/// A conversão só acontece se o tanque aceita tudo o que ela dá: 90 não cabem em 1000 com 950 dentro.
#[test]
fn a_conversion_that_overflows_the_tank_is_not_made() {
    let mut world = infuser();
    let machine = &world.block_entity.0;
    let side = machine.chemical_side().unwrap();
    side.tank
        .set_contents(Some(redstone()), TANK_CAPACITY - 50, None);
    side.slot
        .set_contents(Some(item("redstone_block")), 1, None);
    world.tick();
    let side = world.block_entity.0.chemical_side().unwrap();
    assert_eq!(
        (side.slot.amount(), side.tank.amount()),
        (1, TANK_CAPACITY - 50)
    );
}

/// Salvar e reabrir no meio de uma operação: o objeto nasce de novo do que o Java persiste (os itens, o tanque, a energia e o
/// progresso), e a operação segue de onde parou e termina gastando o tanque.
#[test]
fn a_reborn_infuser_goes_on_from_the_saved_progress() {
    let mut before = infuser();
    {
        let machine = &before.block_entity.0;
        machine
            .input()
            .set_contents(Some(item("copper_ingot")), 2, None);
        let side = machine.chemical_side().unwrap();
        side.tank.set_contents(Some(redstone()), 25, None);
    }
    for _ in 0..60 {
        tick_with_energy(&mut before);
    }
    let machine = &before.block_entity.0;
    let saved = machine.saved_state();
    let (input, output) = (machine.input().contents(), machine.output().contents());
    let tank = machine.chemical_side().unwrap().tank.contents();
    let energy = machine.energy().amount();
    assert!(
        saved.progress > 0 && saved.progress < 200,
        "no meio da operação"
    );

    let mut reborn = infuser();
    {
        let machine = &mut reborn.block_entity.0;
        machine.restore(&saved, true);
        machine
            .input()
            .set_contents(input.resource, input.amount, None);
        machine
            .output()
            .set_contents(output.resource, output.amount, None);
        machine.energy().set_energy(energy, None);
        machine
            .chemical_side()
            .unwrap()
            .tank
            .set_contents(tank.resource, tank.amount, None);
    }
    tick_with_energy(&mut reborn);
    assert_eq!(reborn.block_entity.0.recipe_progress(), saved.progress + 1);
    // 200 ticks no total terminam a primeira operação e gastam 10 do tanque.
    for _ in 0..(200 - saved.progress - 1) {
        tick_with_energy(&mut reborn);
    }
    let machine = &reborn.block_entity.0;
    assert_eq!(machine.output().amount(), 1);
    assert_eq!(machine.input().amount(), 1);
    assert_eq!(machine.chemical_side().unwrap().tank.amount(), 15);
}

/// Tudo o que o Java persiste volta: redstone, modo, estado ativo e atraso, nível do comparador, progresso, `used_so_far`,
/// receitas usadas e a última.
#[test]
fn the_saved_state_round_trips_everything_the_java_persists() {
    let mut before = infuser();
    {
        let machine = &before.block_entity.0;
        machine
            .input()
            .set_contents(Some(item("copper_ingot")), 3, None);
        let side = machine.chemical_side().unwrap();
        side.tank.set_contents(Some(redstone()), 100, None);
    }
    // Uma operação inteira (200 ticks) e mais um pouco da seguinte.
    for _ in 0..230 {
        tick_with_energy(&mut before);
    }
    let mut saved = before.block_entity.0.saved_state();
    assert!(saved.active);
    assert!(saved.progress > 0 && saved.progress < 200);
    assert_eq!(
        saved
            .recipes_used
            .get("mekanism:metallurgic_infusing/alloy_infused"),
        Some(&1)
    );
    assert_eq!(
        saved.last_recipe_used.as_deref(),
        Some("mekanism:metallurgic_infusing/alloy_infused")
    );
    assert_eq!(saved.last_recipe_amount, 1);
    // O Infuser sem uso por tick nunca muda o `usedSoFar`, mas o grava e o relê.
    saved.used_so_far = 7;
    let mut reborn = infuser();
    reborn.block_entity.0.restore(&saved, true);
    assert_eq!(reborn.block_entity.0.saved_state(), saved);
    assert!(reborn.block_entity.0.tile().block_active());
}
