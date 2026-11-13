//! O `networks` único do original (`TransmitterNetworkRegistry.networks`) e os logs de depuração contra o jogo (`testes/NetworkReaderTraceTest.java`, `networkReaderDebug`,
//! `rastros/network_reader_debug.jsonl`): dez redes de cinco famílias, uma por vez, depois a união de duas, uma rede solta, uma entrada numa rede e um transmissor que sai.
//! Cada nota do rastro tem o registro na ordem do iterador (com o `hashCode` do UUID de cada rede) e as linhas de log desde a nota anterior. Cada família é um mundo com o
//! registro dela, e o espelho (`NetworkMirror`) dá a ordem do conjunto único.
mod transmission_world;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use machinery_basis_core::transmission::registry::NetworkMirror;
use serde_json::Value;
use std::collections::BTreeMap;
use transmission_world::*;

fn notes() -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/network_reader_debug.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row.get("registry").is_some())
        .collect()
}

fn hashes(note: &Value) -> Vec<i32> {
    note["registry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|network| network["hash"].as_i64().unwrap() as i32)
        .collect()
}

fn family(class: &str) -> &'static str {
    match class {
        "EnergyNetwork" => "energy",
        "FluidNetwork" => "fluid",
        "ChemicalNetwork" => "chemical",
        "HeatNetwork" => "heat",
        "InventoryNetwork" => "inventory",
        other => panic!("{other}"),
    }
}

/// Os hashes de UUID das redes de cada família, na ordem em que nasceram (a diferença entre notas seguidas).
fn births(notes: &[Value]) -> BTreeMap<&'static str, Vec<i32>> {
    let mut out: BTreeMap<&'static str, Vec<i32>> = BTreeMap::new();
    let mut seen: Vec<i32> = Vec::new();
    for note in notes {
        for network in note["registry"].as_array().unwrap() {
            let hash = network["hash"].as_i64().unwrap() as i32;
            if !seen.contains(&hash) {
                seen.push(hash);
                out.entry(family(network["class"].as_str().unwrap()))
                    .or_default()
                    .push(hash);
            }
        }
    }
    out
}

fn world(hashes: Vec<i32>) -> World {
    let origin = BlockPos::new(100, 64, 8);
    let mut counter = 1000;
    let mut world = World::new(origin, 8, hashes, move |_| {
        counter += 7919;
        counter
    });
    for x in 0..3 {
        world.level.set_ticket_level(
            ChunkPos::new(origin.chunk().x + x - 1, origin.chunk().z),
            ENTITY_TICKING_LEVEL,
        );
    }
    world.level.tick();
    world
}

/// O passo do cenário do original que cada nota fecha: a família, o que se põe (índices da linha) e o que se tira.
fn step(case: &str) -> (&'static str, Vec<i32>, Vec<i32>) {
    match case {
        "build/basic_universal_cable/1" => ("energy", vec![1, 2], vec![]),
        "build/basic_universal_cable/4" => ("energy", vec![4, 5, 6], vec![]),
        "build/basic_mechanical_pipe/1" => ("fluid", vec![1, 2], vec![]),
        "build/basic_mechanical_pipe/4" => ("fluid", vec![4, 5, 6], vec![]),
        "build/basic_pressurized_tube/1" => ("chemical", vec![1, 2], vec![]),
        "build/basic_pressurized_tube/4" => ("chemical", vec![4, 5, 6], vec![]),
        "build/basic_thermodynamic_conductor/1" => ("heat", vec![1, 2], vec![]),
        "build/basic_thermodynamic_conductor/4" => ("heat", vec![4, 5, 6], vec![]),
        "build/basic_logistical_transporter/1" => ("inventory", vec![1, 2], vec![]),
        "build/basic_logistical_transporter/4" => ("inventory", vec![4, 5, 6], vec![]),
        "built/debug_off" => ("energy", vec![], vec![]),
        "merge_two_networks" => ("energy", vec![3], vec![]),
        // Um transmissor solto: a linha seguinte da estrutura, sem vizinho.
        "lone_tube" => ("chemical", vec![8], vec![]),
        "pipe_joins_one_network" => ("fluid", vec![7], vec![]),
        "conductor_removed" => ("heat", vec![], vec![4]),
        other => panic!("{other}"),
    }
}

#[test]
fn the_mirror_gives_the_iteration_order_of_the_single_networks_set_of_the_original() {
    let notes = notes();
    let births = births(&notes);
    let mut worlds: BTreeMap<&str, World> = births
        .iter()
        .map(|(name, hashes)| (*name, world(hashes.clone())))
        .collect();
    let mirror = NetworkMirror::current();
    let mut checked = 0;
    for note in &notes {
        let case = note["case"].as_str().unwrap();
        let (name, place, remove) = step(case);
        let debug = note["debug"].as_bool().unwrap();
        for world in worlds.values_mut() {
            world.registry_mut().set_debug(debug);
        }
        let world = worlds.get_mut(name).unwrap();
        for x in place {
            world.place_cable(x);
        }
        for x in remove {
            world.break_block(world.cable(x));
        }
        world.level.tick_n(4);
        let order: Vec<i32> = mirror
            .in_order()
            .iter()
            .map(|key| {
                worlds
                    .values()
                    .find(|w| w.registry().id() == key.registry)
                    .unwrap()
                    .registry()
                    .network(key.network)
                    .uuid()
                    .hash_code()
            })
            .collect();
        assert_eq!(order, hashes(note), "{case}: a ordem do conjunto único");
        let mut log = Vec::new();
        for world in worlds.values_mut() {
            log.extend(world.registry_mut().take_debug_log());
        }
        let wanted: Vec<String> = note["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| {
                line.as_str()
                    .unwrap()
                    .strip_prefix("INFO ")
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(log, wanted, "{case}: o log");
        checked += 1;
    }
    assert!(checked >= 15, "{checked}");
}
