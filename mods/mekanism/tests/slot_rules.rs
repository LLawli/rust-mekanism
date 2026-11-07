//! Repete as perguntas que o Mekanism original respondeu sobre os slots de infusão e de energia
//! (`dev/referencia/rastros/slots_<máquina>.jsonl`, escrito por `SlotsTraceTest.java`): inserir e tirar um item, simulado, com cada
//! `AutomationType`. `ChemicalInventorySlot.fillOrConvert` e `EnergyInventorySlot.fillOrConvert` tiram com `!external ||
//! !canFillOrConvert(item)` e inserem com `internal || canFillOrConvert(item)`.
//!
//! O slot de infusão fica com o tanque e o slot de entrada em cada um dos estados do rastro. As conversões e as receitas são as
//! dos arquivos do original que o port já tem, uma por químico: o que a pergunta usa é só se o item converte e se o tanque aceita o
//! químico que ele dá. As conversões de energia são as duas receitas `energy_conversion` do original (`redstone` e `redstone_block`),
//! lidas dos arquivos que o port entrega.
//!
//! O mesmo rastro grava os hoppers, com a máquina posta como um jogador a põe (a configuração de lados do item,
//! `AttachedSideConfig`): um hopper de baixo, sozinho, com o slot de infusão em cada estado, e depois um hopper em cada face
//! apontando para a máquina, com um item de cada tipo. As faces e os slots que o port declara (`item_slots`, `sides`) e as regras
//! dos slots têm de dar o mesmo.
mod common;

use common::{item, machine, valid_input};
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules::{ELECTRIC_ENERGY_SLOT, ENERGY_SLOT, electric_item_slots, item_slots, sides};
use serde_json::Value;
use std::path::Path;

/// Cada máquina grava 7 itens no slot de energia, e o Infuser mais 4 tanques por 2 entradas por 7 itens no de infusão.
const ENERGY_PROBES: usize = 7;
const INFUSION_PROBES: usize = 4 * 2 * 7;

fn read_trace(machine: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("slots_{machine}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn automation(name: &str) -> AutomationType {
    match name {
        "EXTERNAL" => AutomationType::External,
        "INTERNAL" => AutomationType::Internal,
        "MANUAL" => AutomationType::Manual,
        other => panic!("automação {other}"),
    }
}

/// `insert` de um item (simulado) ou `extract` dos 3 que o slot guarda, e o que o original moveu.
fn moved(
    slot: &InventorySlot,
    op: &str,
    resource: &ItemResource,
    automation: AutomationType,
) -> u32 {
    let root = Transaction::open_root();
    match op {
        "insert" => {
            slot.set_contents(None, 0, None);
            slot.insert(resource, 1, &root, automation)
        }
        "extract" => {
            slot.set_contents(Some(resource.clone()), 3, None);
            slot.extract(resource, 3, &root, automation)
        }
        other => panic!("operação {other}"),
    }
}

/// Põe o tanque (`bio:500`, `empty:0`) e a entrada (`dirt` ou vazio) como a sonda do original os tinha.
fn set_state(machine: &ProgressMachine, tank: &str, input: &str) {
    let side = machine.chemical_side().unwrap();
    let (id, amount) = tank.split_once(':').unwrap();
    let amount: u64 = amount.parse().unwrap();
    let resource = (id != "empty").then(|| ChemicalResource::new(id));
    side.tank.set_contents(resource, amount, None);
    machine.input().set_contents(
        (!input.is_empty()).then(|| item(input)),
        u32::from(!input.is_empty()),
        None,
    );
}

fn replay(name: &str, probe: &str, minimum: usize) {
    let machine = machine(name);
    let mut seen = 0;
    for line in read_trace(name) {
        if line["probe"].as_str() != Some(probe) {
            continue;
        }
        let slot = match probe {
            "energy" => machine.energy_slot().clone(),
            _ => machine.chemical_side().unwrap().slot.clone(),
        };
        if probe == "infusion" {
            set_state(
                &machine,
                line["tank"].as_str().unwrap(),
                line["input"].as_str().unwrap(),
            );
        }
        let resource = item(line["item"].as_str().unwrap());
        for op in ["insert", "extract"] {
            for (kind, expected) in line[op].as_object().unwrap() {
                let found = moved(&slot, op, &resource, automation(kind));
                assert_eq!(
                    u64::from(found),
                    expected.as_u64().unwrap(),
                    "{name} {probe} {op} {kind}: {line}"
                );
            }
        }
        seen += 1;
    }
    assert!(
        seen >= minimum,
        "{name} {probe}: só {seen} sondas no rastro"
    );
}

#[test]
fn the_energy_slot_of_each_machine_follows_fill_or_convert() {
    for name in [
        "enrichment_chamber",
        "crusher",
        "energized_smelter",
        "metallurgic_infuser",
    ] {
        replay(name, "energy", ENERGY_PROBES);
    }
}

#[test]
fn the_infusion_slot_follows_fill_or_convert_in_every_tank_state() {
    replay("metallurgic_infuser", "infusion", INFUSION_PROBES);
}

const HOPPER_ITEMS: [&str; 5] = ["stick", "VALID", "redstone", "bio_fuel", "coal"];
const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];

/// O índice do slot de energia em `getInventorySlots()` de cada máquina.
fn energy_slot_index(name: &str) -> usize {
    if name == "metallurgic_infuser" {
        ENERGY_SLOT
    } else {
        ELECTRIC_ENERGY_SLOT
    }
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap()
}

fn phases(name: &str) -> (Face, Vec<Value>) {
    let trace = read_trace(name);
    let front = face(trace[0]["front"].as_str().unwrap());
    let phases = trace
        .into_iter()
        .filter(|line| line["phase"].is_string())
        .collect();
    (front, phases)
}

fn reached(name: &str, front: Face, at: Face) -> machinery_basis_core::sides::FaceSlots {
    if name == "metallurgic_infuser" {
        item_slots(front, at)
    } else {
        electric_item_slots(front, at)
    }
}

/// Os itens que um hopper em `at` não consegue pôr na máquina, na ordem do hopper, pelas regras do port: só entra pelo slot que a
/// face alcança (`item_slots`), e o slot decide pelo item (`isValidForInsertion` de quem está de fora).
fn left_in_the_hopper(name: &str, machine: &ProgressMachine, front: Face, at: Face) -> Vec<String> {
    let slots = reached(name, front, at);
    HOPPER_ITEMS
        .iter()
        .map(|id| {
            if *id == "VALID" {
                valid_input(name)
            } else {
                id
            }
        })
        .filter(|id| {
            let resource = item(id);
            !slots.inputs.iter().any(|&slot| {
                let target = if slot == energy_slot_index(name) {
                    machine.energy_slot()
                } else if name == "metallurgic_infuser" && slot == 0 {
                    &machine.chemical_side().unwrap().slot
                } else {
                    machine.input()
                };
                target.is_valid_for_insertion(&resource, AutomationType::External)
            })
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_hopper_pointed_at_each_face_puts_in_what_the_original_puts_in() {
    for name in MACHINES {
        let machine = machine(name);
        if name == "metallurgic_infuser" {
            set_state(&machine, "empty:0", "");
        }
        let (front, phases) = phases(name);
        let mut seen = 0;
        for phase in phases {
            let Some(side) = phase["phase"].as_str().unwrap().strip_prefix("take_") else {
                continue;
            };
            let at = face(side);
            let original: Vec<String> = phase["hoppers"][side]
                .as_array()
                .unwrap()
                .iter()
                .map(|stack| stack[0].as_str().unwrap().to_owned())
                .collect();
            let port = left_in_the_hopper(name, &machine, front, at);
            assert_eq!(port, original, "{name} {side}: {phase}");
            seen += 1;
        }
        assert_eq!(seen, 5, "{name}: faltam faces no rastro");
    }
}

/// O hopper de baixo só tira o que o slot de infusão deixa tirar de quem está de fora: nenhum slot das máquinas elétricas, porque a
/// base delas é `DataType.INPUT` (`InventorySlotInfo(true, false, ..)`).
#[test]
fn the_hopper_below_gets_what_the_original_gives_it() {
    for name in MACHINES {
        let machine = machine(name);
        let (front, phases) = phases(name);
        let mut seen = 0;
        for phase in phases {
            let Some(label) = phase["phase"].as_str().unwrap().strip_prefix("give_") else {
                continue;
            };
            let in_hopper: u64 = phase["hoppers"]["down"]
                .as_array()
                .unwrap()
                .iter()
                .map(|stack| stack[1].as_u64().unwrap())
                .sum();
            // O que o slot de infusão guardava no começo da fase, e o tanque: o original os grava depois da fase, então o
            // estado é o que `SlotsTraceTest.give` pôs (nome da fase).
            let (item_id, tank) = match label {
                "stick_in_infusion" => ("stick", "empty:0"),
                "bio_fuel_tank_full_bio" => ("bio_fuel", "bio:1000"),
                "bio_fuel_tank_fungi" => ("bio_fuel", "fungi:500"),
                "bio_fuel_tank_empty" => ("bio_fuel", "empty:0"),
                _ => ("", "empty:0"),
            };
            let slots = reached(name, front, Face::Down);
            let declared = sides(
                front,
                if name == "metallurgic_infuser" {
                    item_slots
                } else {
                    electric_item_slots
                },
            );
            assert_eq!(declared.gives_output(Face::Down), !slots.outputs.is_empty());
            // A base das máquinas elétricas é `DataType.INPUT`: o hopper de baixo não leva nada, nem a entrada nem a saída.
            assert!(
                name == "metallurgic_infuser" || !declared.gives_output(Face::Down),
                "{name}"
            );
            let mut expected = 0;
            if !item_id.is_empty() && name == "metallurgic_infuser" {
                set_state(&machine, tank, "");
                let slot = &machine.chemical_side().unwrap().slot;
                let gives = declared.gives_output(Face::Down)
                    && slots.outputs.contains(&0)
                    && slot.is_valid_for_extraction(&item(item_id), AutomationType::External);
                // O tanque vazio converte o item no mesmo tick: o hopper não chega a ver nada, e o rastro mostra isso.
                let converted_at_once = tank == "empty:0" && item_id == "bio_fuel";
                if gives && !converted_at_once {
                    expected = 3;
                }
            }
            assert_eq!(in_hopper, expected, "{name} {label}: {phase}");
            seen += 1;
        }
        assert!(seen >= 1, "{name}: sem fases de hopper embaixo");
    }
}
