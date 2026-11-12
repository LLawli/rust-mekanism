//! Repete o que um dropper e um dispenser apontados para cada face do cubo de energia (basic e creative) fazem, e o que cada face deixa sair
//! (`dev/referencia/rastros/dropper_energy_cube_*.jsonl`, escrito por `DropperTraceTest.java`): o cubo posto como um jogador o põe (a
//! configuração de lados do item), com o contêiner de energia vazio e com 1.000, um dropper com 3 itens de um tipo em cada vizinho,
//! ligado por um bloco de redstone, 10 ticks depois; e, na mesma hora, o `extract` da capability de item de cada face com o item posto em
//! cada slot.
//!
//! O dropper do Horizon (`dispense_from`) faz `add_item` pela face do cubo que ele toca: os slots da face na ordem, cada um por `can_place`
//! (as entradas do contêiner da instância) e pelo hook `container_accepts` (a face deixa entrar e o slot aceita o item de quem está de fora:
//! `EnergyInventorySlot.fillOrConvert` e `drain`). A saída é o `can_take` (a face deixa sair e o slot deixa tirar). O dispenser nunca insere.
use machinery_basis_core::cached_recipe::ItemIngredient;
use machinery_basis_core::containers::{AutomationType, ItemResource};
use machinery_basis_core::energy_conversion::EnergyConversion;
use machinery_basis_core::energy_cube::{self, ItemEnergy};
use machinery_basis_core::side_config::TileComponentConfig;
use machinery_basis_core::sides::Face;
use mekanism_rules as rules;
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const CUBES: [(&str, usize); 2] = [
    ("energy_cube_basic", 0),
    ("energy_cube_creative", energy_cube::CREATIVE),
];
const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

fn read(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap()
}

/// O item do rastro (o caminho sem o namespace) como o jogo o conhece, com o limite de pilha dele.
fn resource(path: &str) -> ItemResource {
    let (id, stack) = match path {
        "energy_tablet" => ("mekanism:energy_tablet", 1),
        "bio_fuel" => ("mekanism:bio_fuel", 64),
        "lava_bucket" => ("minecraft:lava_bucket", 1),
        "bucket" => ("minecraft:bucket", 1),
        other => return ItemResource::new(&format!("minecraft:{other}"), 64),
    };
    ItemResource::new(id, stack)
}

fn items() -> ItemEnergy {
    let config = rules::ServerConfig::DEFAULT;
    ItemEnergy {
        // Nenhum item do cenário traz energia: o tablet é o vazio.
        lookup: Rc::new(move |resource: &ItemResource| {
            rules::item_energy_spec(&resource.id, 0, &config)
        }),
        write: Rc::new(|_: &ItemResource, _: i64| None),
        // As conversões do cenário do original (`mekanism:energy_conversion/*`): a redstone e o bloco dela.
        conversions: EnergyConversion::cache_of(vec![
            EnergyConversion {
                id: String::new(),
                input: ItemIngredient::item("minecraft:redstone_block"),
                input_count: 1,
                output: 90_000,
            },
            EnergyConversion {
                id: String::new(),
                input: ItemIngredient::item("minecraft:redstone"),
                input_count: 1,
                output: 10_000,
            },
        ]),
    }
}

/// O cubo posto por um jogador: a configuração de lados do item.
fn placed_config() -> TileComponentConfig {
    let spec = energy_cube::side_config_spec();
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    config.apply_implicit_components(Some(&rules::energy_cube_default()), Face::North, false);
    config
}

/// O que o Horizon faz com um item que um dropper põe pela face `at`: o primeiro slot da face que é entrada, que a face deixa encher e que
/// o aceita.
fn landing(tier: usize, stored: i64, front: Face, at: Face, item: &ItemResource) -> Option<usize> {
    let instance = rules::cube_item_instance(&placed_config(), front);
    let slot_rules = rules::cube_slot_rules(tier, stored, &rules::ServerConfig::DEFAULT, &items());
    instance.face_slots()[at as usize]
        .iter()
        .copied()
        .find(|slot| {
            instance.inputs().contains(slot)
                && instance.accepts_through(at, *slot)
                && slot_rules.is_valid_for_insertion(*slot, item, AutomationType::External)
        })
}

/// `can_take` do Horizon: a face deixa sair e o slot deixa tirar.
fn takes(
    tier: usize,
    stored: i64,
    front: Face,
    at: Face,
    slot: usize,
    item: &ItemResource,
) -> bool {
    let instance = rules::cube_item_instance(&placed_config(), front);
    let slot_rules = rules::cube_slot_rules(tier, stored, &rules::ServerConfig::DEFAULT, &items());
    instance.releases_through(at, slot)
        && slot_rules.is_valid_for_extraction(slot, item, AutomationType::External)
}

/// `["dropper_down_stick@0", ...]` em (lado, item, energia).
fn split(phase: &str, prefix: &str) -> (String, String, i64) {
    let rest = phase.strip_prefix(prefix).unwrap();
    let (rest, energy) = rest.split_once('@').unwrap();
    let (side, id) = rest.split_once('_').unwrap();
    assert!(FACES.contains(&side), "{phase}");
    (side.to_owned(), id.to_owned(), energy.parse().unwrap())
}

#[test]
fn a_dropper_pointed_at_each_face_of_the_cube_puts_in_what_the_original_puts_in() {
    let (mut inserted, mut refused) = (0, 0);
    for (name, tier) in CUBES {
        let lines = read(&format!("dropper_{name}"));
        let front = face(lines[0]["front"].as_str().unwrap());
        let mut seen = 0;
        for line in &lines[1..] {
            let phase = line["phase"].as_str().unwrap();
            if !phase.starts_with("dropper_") {
                continue;
            }
            let (side, id, energy) = split(phase, "dropper_");
            let item = resource(&id);
            let left = line["dropper"][0][1].as_u64().unwrap();
            let landed = landing(tier, energy, front, face(&side), &item);
            if landed.is_some() {
                assert_eq!(
                    left, 2,
                    "{name} {phase}: o dropper devia ter dado um item: {line}"
                );
                inserted += 1;
            } else {
                assert_eq!(
                    left, 3,
                    "{name} {phase}: o dropper devia ficar com os 3: {line}"
                );
                refused += 1;
            }
            assert_eq!(line["dropped"], 0, "{name} {phase}");
            seen += 1;
        }
        assert_eq!(seen, 60, "{name}: faltam fases do dropper");
    }
    assert!(
        inserted >= 8 && refused >= 40,
        "{inserted} entraram, {refused} recusados"
    );
}

#[test]
fn a_dispenser_drops_the_item_into_the_world_and_never_inserts_into_the_cube() {
    for (name, _) in CUBES {
        let lines = read(&format!("dropper_{name}"));
        let mut seen = 0;
        for line in &lines[1..] {
            let phase = line["phase"].as_str().unwrap();
            if !phase.starts_with("dispenser_") {
                continue;
            }
            assert_eq!(line["dropper"][0][1], 2, "{name} {phase}: {line}");
            assert_eq!(line["dropped"], 1, "{name} {phase}: {line}");
            assert!(
                line["slots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s[1].as_u64().unwrap() == 0),
                "{name} {phase}: {line}"
            );
            seen += 1;
        }
        assert_eq!(seen, 60, "{name}: faltam fases do dispenser");
    }
}

#[test]
fn each_face_of_the_cube_lets_out_what_the_original_lets_out() {
    let mut out = 0;
    let mut kept = 0;
    for (name, tier) in CUBES {
        let lines = read(&format!("dropper_{name}"));
        let front = face(lines[0]["front"].as_str().unwrap());
        let mut seen = 0;
        for line in &lines[1..] {
            let phase = line["phase"].as_str().unwrap();
            if !phase.starts_with("extract_") {
                continue;
            }
            let (side, id, energy) = split(phase, "extract_");
            let item = resource(&id);
            let original: Vec<i64> = line["out"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap())
                .collect();
            let port: Vec<i64> = (0..original.len())
                .map(|slot| i64::from(takes(tier, energy, front, face(&side), slot, &item)))
                .collect();
            assert_eq!(port, original, "{name} {phase}: {line}");
            out += port.iter().sum::<i64>();
            kept += port.iter().filter(|n| **n == 0).count();
            seen += 1;
        }
        assert_eq!(seen, 2 * 6 * 6, "{name}: faltam fases de saída");
    }
    assert!(out >= 40 && kept >= 100, "{out} saíram, {kept} ficaram");
}
