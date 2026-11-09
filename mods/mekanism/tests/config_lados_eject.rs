//! Repete, tick a tick, os rastros `config_lados_eject_*` do original: o ejetor guiado pela configuração de lados, com um baú em cada
//! face da máquina. Cada tick roda as marcas (`ServerTickEvent.Pre`), o tick da máquina, e então compara o que o rastro gravou
//! (`ServerTickEvent.Post`): os baús, a saída, a entrada, o `tickDelay` e o ejetar de cada tipo.
mod common;

use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::item_handler::{ItemCapabilities, VanillaContainer};
use machinery_basis_core::side_config::{DataType, RelativeSide, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{advanced_machine_default, electric_machine_default};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const CHEST_SLOTS: usize = 27;
const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const TICKS: usize = 80;

struct Machine(ProgressMachine);

impl SimBlockEntity for Machine {
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

fn read(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn slot_json(slot: &InventorySlot) -> Value {
    json!([
        slot.resource()
            .map_or(String::new(), |resource| resource.id.to_string()),
        slot.amount()
    ])
}

fn chests_json(chests: &BTreeMap<&str, Rc<VanillaContainer>>) -> Value {
    let mut out = serde_json::Map::new();
    for (face, chest) in chests {
        let mut held = serde_json::Map::new();
        for (_, resource, amount) in chest.stacks() {
            let entry = held.entry(resource.id.to_string()).or_insert(json!(0));
            *entry = json!(entry.as_u64().unwrap() + u64::from(amount));
        }
        if !held.is_empty() {
            out.insert((*face).to_owned(), Value::Object(held));
        }
    }
    Value::Object(out)
}

fn apply(machine: &mut ProgressMachine, mark: &str) {
    let parts: Vec<&str> = mark.split_whitespace().collect();
    match parts.as_slice() {
        ["item", "ejecting", on] => machine
            .side_config_mut()
            .config_mut(TransmissionType::Item)
            .unwrap()
            .set_ejecting(*on == "true"),
        ["item", assign] => {
            let (side, data_type) = assign.split_once('=').unwrap();
            let side = RelativeSide::from_serialized_name(side).unwrap();
            let data_type = DataType::from_serialized_name(data_type).unwrap();
            machine
                .side_config_mut()
                .config_mut(TransmissionType::Item)
                .unwrap()
                .set_data_type(data_type, side);
        }
        ["output", id, count] => {
            machine
                .output()
                .set_contents(Some(item(id)), count.parse().unwrap(), None)
        }
        ["input", id, count] => {
            machine
                .input()
                .set_contents(Some(item(id)), count.parse().unwrap(), None)
        }
        ["infusion", "slot", count, id] => machine.chemical_side().unwrap().slot.set_contents(
            Some(item(id)),
            count.parse().unwrap(),
            None,
        ),
        ["item", "ejecting", ..] | [] => {}
        [facing] if facing.starts_with("setFacing(") => {
            let name = facing
                .trim_start_matches("setFacing(")
                .trim_end_matches(')');
            machine.set_facing(Face::from_property(&name.to_lowercase()).unwrap());
        }
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn run(name: &str) -> usize {
    let lines = read(&format!("config_lados_eject_{name}.jsonl"));
    let meta = &lines[0]["meta"];
    let infuser = meta["machine"] == "metallurgic_infuser";
    let mut machine = if infuser {
        common::infuser()
    } else {
        common::chamber("clay")
    };
    machine.set_facing(Face::North);
    let default = if infuser {
        advanced_machine_default()
    } else {
        electric_machine_default()
    };
    machine.apply_item_side_config(Some(&default), Some(&AttachedEjector::DEFAULT), false);
    let mut world = SimWorld::new(Machine(machine));
    let mut chests = BTreeMap::new();
    for face in FACES {
        let chest = VanillaContainer::new(CHEST_SLOTS, Rc::new(|| {}));
        world
            .neighbours
            .set(Face::from_property(face).unwrap(), chest.clone());
        chests.insert(face, chest);
    }
    if name == "strict_other_type" {
        // `eject0` em 1 e `eject3` em 0 no NBT, lidos como um tile que carrega.
        let mut data = world.block_entity.0.side_config().serialize();
        data.eject[TransmissionType::Energy.ordinal()] = Some(true);
        data.eject[TransmissionType::Item.ordinal()] = Some(false);
        world
            .block_entity
            .0
            .restore_side_config(Some(&data), None, false);
    }
    let mut compared = 0;
    for expected in &lines[1..] {
        let t = expected["t"].as_u64().unwrap() as usize;
        for mark in expected["marks"].as_array().unwrap() {
            apply(&mut world.block_entity.0, mark.as_str().unwrap());
        }
        world.tick();
        let machine = &world.block_entity.0;
        let label = format!("{name} t={t}");
        assert_eq!(chests_json(&chests), expected["chests"], "{label}: baús");
        assert_eq!(
            slot_json(machine.output()),
            expected["output"],
            "{label}: saída"
        );
        assert_eq!(
            slot_json(machine.input()),
            expected["input"],
            "{label}: entrada"
        );
        if infuser {
            let side = machine.chemical_side().unwrap();
            assert_eq!(
                slot_json(&side.slot),
                expected["infusion"],
                "{label}: infusão"
            );
        }
        assert_eq!(
            machine.ejector().tick_delay(),
            expected["tick_delay"].as_u64().unwrap() as u32,
            "{label}: tickDelay"
        );
        for (kind, on) in expected["ejecting"].as_object().unwrap() {
            let kind = TransmissionType::from_serialized_name(kind).unwrap();
            let ours = machine.side_config().config(kind).unwrap().is_ejecting();
            assert_eq!(Value::Bool(ours), *on, "{label}: ejetar de {kind:?}");
        }
        compared += 1;
    }
    assert_eq!(compared, TICKS, "{name}: ticks comparados");
    compared
}

#[test]
fn the_output_on_the_right_leaves_by_the_right_every_eleven_ticks() {
    run("output_right");
}

#[test]
fn several_output_sides_send_the_whole_stack_to_the_first_face() {
    run("output_three");
}

#[test]
fn input_output_ejects_only_what_the_slot_lets_out() {
    run("input_output_front");
}

#[test]
fn every_side_in_none_ejects_nothing_but_still_waits() {
    run("no_data");
}

#[test]
fn turning_the_block_turns_the_output_side() {
    run("rotate");
}

#[test]
fn the_extra_type_does_not_eject_and_the_bottom_output_does() {
    run("infuser_extra");
}

#[test]
fn ejecting_only_the_energy_ejects_no_item() {
    run("strict_other_type");
}
