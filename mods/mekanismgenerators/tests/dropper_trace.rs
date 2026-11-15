//! Repete o que um dropper e um dispenser apontados para cada face do Heat Generator fazem, e o que cada face deixa sair
//! (`dev/referencia/rastros/dropper_heat_generator.jsonl`, escrito por `DropperTraceTest.java`): o gerador posto como um jogador o põe, com o
//! contêiner de energia vazio e com 1.000 e o tanque de lava cheio, um dropper com 3 itens de um tipo em cada vizinho, ligado por um bloco de
//! redstone, 10 ticks depois; e, na mesma hora, o `extract` da capability de item de cada face com o item posto em cada slot.
//!
//! O gerador não tem configuração de lados: o combustível está em todas as faces menos a direita, e o slot de energia só na direita
//! (`item_instance`). A regra de cada slot é a do Java: o de combustível (`FluidFuelInventorySlot.forFuel`) aceita o que tem `COOKING_FUEL` ou enche o
//! tanque (o balde de lava) e só deixa sair o que não é nem uma coisa nem outra; o de energia (`EnergyInventorySlot.drain`) aceita o que `canDrain` e só
//! deixa sair o que não.
use machinery_basis_core::containers::{Access, ItemResource};
use machinery_basis_core::energy_conversion::EnergyConversion;
use machinery_basis_core::energy_cube::{ItemEnergy, ItemEnergySpec};
use machinery_basis_core::sides::Face;
use mekanismgenerators_rules::{
    ENERGY_SLOT, FUEL_SLOT, HeatConfig, fuel_slot_extractable, fuel_slot_insertable, item_instance,
    slot_can_drain,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
/// Os combustíveis do cenário (o componente `cooking_fuel`): o carvão, o graveto, o `bio_fuel` do Mekanism e o balde de lava.
const FUELS: [&str; 4] = ["coal", "stick", "bio_fuel", "lava_bucket"];
/// O Energy Tablet vazio: o único item do cenário com handler de energia, que aceita energia de fora.
const TABLET_CAPACITY: i64 = 1_000_000;

fn read() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/dropper_heat_generator.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap()
}

fn split(phase: &str, prefix: &str) -> (Face, String, i64) {
    let rest = phase.strip_prefix(prefix).unwrap();
    let (rest, energy) = rest.split_once('@').unwrap();
    let (side, id) = rest.split_once('_').unwrap();
    assert!(FACES.contains(&side), "{phase}");
    (face(side), id.to_owned(), energy.parse().unwrap())
}

/// O handler de energia dos itens do cenário: o Energy Tablet (`basicCreator(alwaysTrue, alwaysTrue, ..)`, a taxa de carga de `gear.toml`), e mais nenhum.
fn items() -> ItemEnergy {
    ItemEnergy {
        lookup: Rc::new(|item: &ItemResource| {
            (&*item.id == "energy_tablet").then_some(ItemEnergySpec {
                amount: 0,
                capacity: TABLET_CAPACITY,
                rate: 5_000,
                can_extract: Access::Any,
                can_insert: Access::Any,
                creative: false,
            })
        }),
        write: Rc::new(|_, _| None),
        conversions: EnergyConversion::cache_of(Vec::new()),
    }
}

/// `EnergyInventorySlot.canDrain` do slot de energia com `stored` no buffer.
fn energy_can_drain(stored: i64, id: &str) -> bool {
    let stored = i32::try_from(stored).unwrap();
    slot_can_drain(
        &items(),
        stored,
        HeatConfig::DEFAULT.energy_capacity,
        &ItemResource::new(id, 64),
    )
}

fn is_fuel(id: &str) -> bool {
    FUELS.contains(&id)
}

fn fills_tank(id: &str) -> bool {
    id == "lava_bucket"
}

/// `isValidForInsertion(item, EXTERNAL)` do slot.
fn accepts(slot: usize, id: &str, stored: i64) -> bool {
    match slot {
        FUEL_SLOT => fuel_slot_insertable(is_fuel(id), fills_tank(id)),
        ENERGY_SLOT => energy_can_drain(stored, id),
        _ => false,
    }
}

/// `isValidForExtraction(item, EXTERNAL)` do slot.
fn extractable(slot: usize, id: &str, stored: i64) -> bool {
    match slot {
        FUEL_SLOT => fuel_slot_extractable(is_fuel(id), fills_tank(id)),
        ENERGY_SLOT => !energy_can_drain(stored, id),
        _ => false,
    }
}

#[test]
fn a_dropper_pointed_at_each_face_of_the_generator_puts_in_what_the_original_puts_in() {
    let lines = read();
    let front = face(lines[0]["front"].as_str().unwrap());
    let instance = item_instance(front);
    let (mut inserted, mut refused, mut seen) = (0, 0, 0);
    for line in &lines[1..] {
        let phase = line["phase"].as_str().unwrap();
        if !phase.starts_with("dropper_") {
            continue;
        }
        let (side, id, stored) = split(phase, "dropper_");
        // O balde de lava não empilha: o dropper só tem um.
        let before: u64 = if id == "lava_bucket" { 1 } else { 3 };
        let left = line["dropper"]
            .get(0)
            .map_or(0, |stack| stack[1].as_u64().unwrap());
        let landed = instance.face_slots()[side as usize]
            .iter()
            .copied()
            .find(|slot| {
                instance.inputs().contains(slot)
                    && instance.accepts_through(side, *slot)
                    && accepts(*slot, &id, stored)
            });
        if landed.is_some() {
            assert_eq!(
                left,
                before - 1,
                "{phase}: o dropper devia ter dado um item: {line}"
            );
            inserted += 1;
        } else {
            assert_eq!(
                left, before,
                "{phase}: o dropper devia ficar com tudo: {line}"
            );
            refused += 1;
        }
        assert_eq!(line["dropped"], 0, "{phase}");
        seen += 1;
    }
    assert_eq!(seen, 60);
    assert!(
        inserted >= 25 && refused >= 5,
        "{inserted} entraram, {refused} recusados"
    );
}

#[test]
fn a_dispenser_drops_the_item_into_the_world_and_never_inserts_into_the_generator() {
    let lines = read();
    let mut seen = 0;
    for line in &lines[1..] {
        let phase = line["phase"].as_str().unwrap();
        if !phase.starts_with("dispenser_") {
            continue;
        }
        if !phase.contains("lava_bucket") {
            assert_eq!(line["dropper"][0][1], 2, "{phase}: {line}");
            // O item solto cai no chão; o rastro varia entre execuções (decisão 99): as fases com balde de lava deixam o gerador com lava, que corre para fora e queima o item que a alcança (o tablet, o graveto e o carvão, ao acaso; `dropper_fate_heat_generator.jsonl`), então o que vale é que o dispenser deu um item e que no máximo um item ficou no chão.
            assert!(line["dropped"].as_u64().unwrap() <= 1, "{phase}: {line}");
        }
        // O balde de lava é do comportamento do dispenser do vanilla (esvazia o balde no bloco que guarda fluido, ou o solta), e depende do fluido que
        // as fases anteriores deixaram: o que vale aqui é que nunca entra no slot.
        assert!(
            line["slots"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s[1].as_u64().unwrap() == 0),
            "{phase}: {line}"
        );
        seen += 1;
    }
    assert_eq!(seen, 60);
}

#[test]
fn each_face_of_the_generator_lets_out_what_the_original_lets_out() {
    let lines = read();
    let instance = item_instance(face(lines[0]["front"].as_str().unwrap()));
    let (mut out, mut kept, mut seen) = (0, 0, 0);
    for line in &lines[1..] {
        let phase = line["phase"].as_str().unwrap();
        if !phase.starts_with("extract_") {
            continue;
        }
        let (side, id, stored) = split(phase, "extract_");
        let original: Vec<i64> = line["out"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap())
            .collect();
        let port: Vec<i64> = (0..original.len())
            .map(|slot| {
                i64::from(instance.releases_through(side, slot) && extractable(slot, &id, stored))
            })
            .collect();
        assert_eq!(port, original, "{phase}: {line}");
        out += port.iter().sum::<i64>();
        kept += port.iter().filter(|n| **n == 0).count();
        seen += 1;
    }
    assert_eq!(seen, 2 * 6 * 6);
    assert!(out >= 20 && kept >= 30, "{out} saíram, {kept} ficaram");
}
