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
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::item_instance::InstanceItems;
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules::{
    ELECTRIC_ENERGY_SLOT, ELECTRIC_INPUT_SLOT, ENERGY_SLOT, Held, INFUSION_SLOT, INPUT_SLOT,
    Indexes, SlotIndexes, infusing_cache_of, item_instance, item_placed_config, item_sides,
    item_slots, slot_accepts, slot_extractable,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

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
    item_slots(name == "metallurgic_infuser", front, at)
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
            let declared = item_sides(name == "metallurgic_infuser", front);
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

// ---------------------------------------------------------------------------------------------------------------------------------
// H2: o contêiner por instância. O hopper do Horizon (`add_item`, `suck_from_slots`) no lugar da emulação do tick: as mesmas fases do
// rastro, vistas pelas faces da instância, pela regra de entrada (`container_accepts`, com a face) e pela de saída (`can_take`).
// ---------------------------------------------------------------------------------------------------------------------------------

fn slot_indexes(name: &str) -> SlotIndexes {
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

fn indexes_of(name: &str) -> Indexes {
    if name == "metallurgic_infuser" {
        let (recipes, conversions) = common::infuser_recipes_by_type();
        return Indexes::Infuser {
            recipes: infusing_cache_of(recipes),
            conversions: ChemicalConversion::cache_of(conversions),
            energy: common::energy_conversions(),
        };
    }
    let valid = valid_input(name);
    let recipe = Rc::new(ItemToItemRecipe {
        id: format!("test/{valid}"),
        input: ItemIngredient::item(valid),
        input_count: 1,
        output: ItemOutput {
            resource: item("clay_ball"),
            count: 1,
        },
    });
    Indexes::Electric {
        recipes: ItemToItemRecipe::cache_of(vec![recipe]),
        energy: common::energy_conversions(),
    }
}

fn held_of(tank: &str, input: &str) -> Held {
    let id = tank.split(':').next().unwrap();
    Held {
        chemical: matches!(id, "bio" | "fungi").then(|| ChemicalResource::new(id)),
        input: (!input.is_empty()).then(|| item(input)),
    }
}

/// A máquina como o jogador a põe (a configuração do item), virada para `front`: o que o contêiner da instância declara.
fn placed_instance(name: &str, front: Face) -> InstanceItems {
    let infuser = name == "metallurgic_infuser";
    item_instance(infuser, &item_placed_config(infuser), front)
}

/// O que o hopper do Horizon (`add_item`: os slots da face, na ordem, cada um por `can_place` sem face e pelo hook `container_accepts`
/// com ela) deixa de fora, na ordem dos itens.
fn horizon_left_in_the_hopper(
    name: &str,
    instance: &InstanceItems,
    at: Face,
    held: &Held,
) -> Vec<String> {
    let (indexes, slots) = (indexes_of(name), slot_indexes(name));
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
            let landing = instance.face_slots()[at as usize]
                .iter()
                .copied()
                .find(|slot| {
                    instance.inputs().contains(slot)
                        && instance.accepts_through(at, *slot)
                        && slot_accepts(&indexes, &slots, *slot, &resource, held)
                });
            landing.is_none()
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_horizon_hopper_pointed_at_each_face_puts_in_what_the_original_puts_in() {
    for name in MACHINES {
        let (front, phases) = phases(name);
        let instance = placed_instance(name, front);
        let held = held_of("empty:0", "");
        let mut seen = 0;
        for phase in phases {
            let Some(side) = phase["phase"].as_str().unwrap().strip_prefix("take_") else {
                continue;
            };
            let original: Vec<String> = phase["hoppers"][side]
                .as_array()
                .unwrap()
                .iter()
                .map(|stack| stack[0].as_str().unwrap().to_owned())
                .collect();
            let port = horizon_left_in_the_hopper(name, &instance, face(side), &held);
            assert_eq!(port, original, "{name} {side}: {phase}");
            seen += 1;
        }
        assert_eq!(seen, 5, "{name}: faltam faces no rastro");
    }
}

#[test]
fn the_instance_container_has_what_the_a_emulation_declared() {
    for name in MACHINES {
        let (front, _) = phases(name);
        let instance = placed_instance(name, front);
        for at in Face::ALL {
            let old = reached(name, front, at);
            let new = &instance.faces[at as usize];
            assert_eq!(
                new.can_input,
                !old.inputs.is_empty(),
                "{name} {at:?}: entrada"
            );
            assert_eq!(
                new.can_output,
                !old.outputs.is_empty(),
                "{name} {at:?}: saída"
            );
            if new.can_input {
                assert_eq!(new.slots, old.inputs, "{name} {at:?}: slots de entrada");
            }
            if new.can_output {
                assert_eq!(new.slots, old.outputs, "{name} {at:?}: slots de saída");
            }
        }
    }
}

/// O hopper de baixo do Horizon (`suck_from_slots`): os slots da face de baixo, na ordem, cada um por `can_take` (a face que deixa sair
/// e a regra do slot de quem está de fora). O rastro põe só o item do slot de infusão; os outros slots não deixam o hopper levar nada.
#[test]
fn the_horizon_hopper_below_gets_what_the_original_gives_it() {
    for name in MACHINES {
        let (front, phases) = phases(name);
        let instance = placed_instance(name, front);
        let (indexes, slots) = (indexes_of(name), slot_indexes(name));
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
            let (item_id, tank) = match label {
                "stick_in_infusion" => ("stick", "empty:0"),
                "bio_fuel_tank_full_bio" => ("bio_fuel", "bio:1000"),
                "bio_fuel_tank_fungi" => ("bio_fuel", "fungi:500"),
                "bio_fuel_tank_empty" => ("bio_fuel", "empty:0"),
                _ => ("", "empty:0"),
            };
            let held = held_of(tank, "");
            let mut expected = 0;
            if !item_id.is_empty() && name == "metallurgic_infuser" {
                // O tanque vazio converte o item no mesmo tick: o hopper não chega a ver nada, e o rastro mostra isso.
                let converted_at_once = tank == "empty:0" && item_id == "bio_fuel";
                let taken = instance.face_slots()[Face::Down as usize]
                    .iter()
                    .any(|&slot| {
                        slot == INFUSION_SLOT
                            && instance.releases_through(Face::Down, slot)
                            && slot_extractable(&indexes, &slots, slot, &item(item_id), &held)
                    });
                if taken && !converted_at_once {
                    expected = 3;
                }
            } else {
                // Nenhum slot das elétricas sai pela base, e nada das outras fases (o slot de energia guarda um graveto, que
                // nem a base alcança): a regra do slot sozinha não basta, a face é que fecha.
                assert!(
                    name == "metallurgic_infuser"
                        || !instance.faces[Face::Down as usize].can_output,
                    "{name}"
                );
            }
            assert_eq!(in_hopper, expected, "{name} {label}: {phase}");
            seen += 1;
        }
        assert!(seen >= 1, "{name}: sem fases de hopper embaixo");
    }
}

/// A regra do slot de quem está de fora, sem o objeto da lib: `slot_accepts` e `slot_extractable` dão o que o original respondeu a
/// cada sonda (`insert` e `extract` simulados com `EXTERNAL`).
#[test]
fn the_slot_rules_of_the_instance_follow_the_probes_of_the_original() {
    let mut seen = 0;
    for name in MACHINES {
        let (indexes, slots) = (indexes_of(name), slot_indexes(name));
        for line in read_trace(name) {
            let Some(probe) = line["probe"].as_str() else {
                continue;
            };
            let slot = if probe == "energy" {
                slots.energy
            } else {
                slots.infusion.unwrap()
            };
            let held = if probe == "energy" {
                held_of("empty:0", "")
            } else {
                held_of(
                    line["tank"].as_str().unwrap(),
                    line["input"].as_str().unwrap(),
                )
            };
            let resource = item(line["item"].as_str().unwrap());
            assert_eq!(
                u64::from(slot_accepts(&indexes, &slots, slot, &resource, &held)),
                line["insert"]["EXTERNAL"].as_u64().unwrap(),
                "{name} {probe} insert: {line}"
            );
            assert_eq!(
                slot_extractable(&indexes, &slots, slot, &resource, &held),
                line["extract"]["EXTERNAL"].as_u64().unwrap() > 0,
                "{name} {probe} extract: {line}"
            );
            seen += 1;
        }
    }
    assert!(
        seen >= 4 * ENERGY_PROBES + INFUSION_PROBES,
        "só {seen} sondas"
    );
}

/// `slot_extractable` e `slot_accepts` (o que o glue usa por instância, sem o objeto vivo) dão o mesmo que os slots de verdade da máquina,
/// que as sondas do original já conferem: cada slot, cada item, cada estado do tanque e da entrada, com `EXTERNAL`. Cobre o que as sondas não
/// tocam, o slot de entrada (`InputInventorySlot`, que não deixa o de fora tirar) e o de saída (que não deixa pôr).
#[test]
fn the_pure_slot_rules_agree_with_the_slots_of_the_machine() {
    let items = [
        "stick",
        "redstone",
        "redstone_block",
        "coal",
        "diamond",
        "bio_fuel",
        "red_mushroom",
        "dirt",
        "clay",
        "cobblestone",
        "sand",
    ];
    let mut compared = 0;
    for name in MACHINES {
        let (indexes, slots) = (indexes_of(name), slot_indexes(name));
        // As receitas de cada lado são as mesmas: o Infuser do conteúdo, e a máquina elétrica com a receita da sonda.
        let machine = if name == "metallurgic_infuser" {
            common::infuser_from_content_by_type()
        } else {
            machine(name)
        };
        for tank in ["empty:0", "bio:500", "bio:1000", "fungi:500"] {
            for input in ["", valid_input(name)] {
                if name != "metallurgic_infuser" && (tank != "empty:0") {
                    continue;
                }
                if name == "metallurgic_infuser" {
                    set_state(&machine, tank, input);
                } else {
                    machine.input().set_contents(
                        (!input.is_empty()).then(|| item(input)),
                        u32::from(!input.is_empty()),
                        None,
                    );
                }
                let held = held_of(tank, input);
                for (index, slot) in machine.inventory_slots().iter().enumerate() {
                    for id in items {
                        let resource = item(id);
                        // O slot de saída não aceita nada de fora, e `slot_accepts` o diz pelo índice que não é de nenhum dos três.
                        assert_eq!(
                            slot_accepts(&indexes, &slots, index, &resource, &held),
                            slot.is_valid_for_insertion(&resource, AutomationType::External),
                            "{name} slot {index} {id} tanque {tank} entrada {input}: entrada"
                        );
                        assert_eq!(
                            slot_extractable(&indexes, &slots, index, &resource, &held),
                            slot.is_valid_for_extraction(&resource, AutomationType::External),
                            "{name} slot {index} {id} tanque {tank} entrada {input}: saída"
                        );
                        compared += 1;
                    }
                }
            }
        }
    }
    assert!(compared >= 500, "só {compared} comparações");
}
