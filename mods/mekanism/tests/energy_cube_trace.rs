//! Repete, tick a tick, os rastros `energy_cube_*` do original (`EnergyCubeTraceTest`): o Energy Cube de cada tier com a configuração de lados
//! padrão do item, os slots de carga e descarga contra o handler de energia dos itens, o ejetor para os vizinhos (cubos e máquinas), a escala do
//! núcleo, o comparador e o `CubeSideState` de cada lado. Cada tick roda as marcas (`ServerTickEvent.Pre`), o tick do cubo, e então compara o
//! que o rastro gravou (`ServerTickEvent.Post`).
use machinery_basis_core::cached_recipe::ItemIngredient;
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{AutomationType, ItemResource};
use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::energy_conversion::EnergyConversion;
use machinery_basis_core::energy_cube::{EnergyCube, EnergyNeighbours, ItemEnergy};
use machinery_basis_core::side_config::{DataType, RelativeSide, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules as rules;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const ENERGY: &str = "mekanism:energy";

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

fn id_of(name: &str) -> (&'static str, u32) {
    match name {
        "tablet" => ("mekanism:energy_tablet", 1),
        "basic_cube" => ("mekanism:basic_energy_cube", 64),
        "redstone" => ("minecraft:redstone", 64),
        "redstone_block" => ("minecraft:redstone_block", 64),
        "stick" => ("minecraft:stick", 64),
        other => panic!("item desconhecido: {other}"),
    }
}

fn with_energy(id: &str, max_stack: u32, energy: i64) -> ItemResource {
    if energy == 0 {
        return ItemResource::new(id, max_stack);
    }
    ItemResource::with_component(id, max_stack, ENERGY, &energy.to_string())
}

fn energy_of(resource: &ItemResource) -> i64 {
    resource
        .components
        .get(ENERGY)
        .map_or(0, |value| value.parse().unwrap())
}

/// O que o jogo tem do outro lado de uma face: um contêiner de energia que aceita de fora até a capacidade.
struct Receiver {
    stored: i64,
    capacity: i64,
}

/// O mundo de um cubo: os vizinhos que são contêineres simples, ou outro cubo.
struct Neighbours {
    receivers: RefCell<BTreeMap<usize, Receiver>>,
    cube: Option<(Face, Rc<RefCell<EnergyCube>>, i64)>,
}

impl EnergyNeighbours for Neighbours {
    fn has_handler(&self, face: Face) -> bool {
        if let Some((at, cube, _)) = &self.cube
            && *at == face
        {
            let cube = cube.borrow();
            let side = RelativeSide::from_directions(cube.facing(), face.opposite());
            return cube
                .side_config()
                .config(TransmissionType::Energy)
                .and_then(|info| info.slot_info(side))
                .is_some();
        }
        self.receivers.borrow().contains_key(&(face as usize))
    }

    fn insert(&self, face: Face, amount: i32, simulate: bool) -> i32 {
        if let Some((at, cube, time)) = &self.cube
            && *at == face
        {
            let cube = cube.borrow();
            let side = RelativeSide::from_directions(cube.facing(), face.opposite());
            let can_input = cube
                .side_config()
                .config(TransmissionType::Energy)
                .and_then(|info| info.slot_info(side))
                .is_some_and(|info| info.can_input);
            if !can_input {
                return 0;
            }
            let transaction = Transaction::open(None);
            let inserted = cube
                .energy()
                .insert(amount, &transaction, AutomationType::External);
            if !simulate {
                transaction.commit();
                cube.energy_received(i64::from(inserted), *time as u64);
            }
            return inserted;
        }
        let mut receivers = self.receivers.borrow_mut();
        let Some(receiver) = receivers.get_mut(&(face as usize)) else {
            return 0;
        };
        let room = (receiver.capacity - receiver.stored).clamp(0, i64::from(i32::MAX)) as i32;
        let accepted = amount.min(room);
        if !simulate {
            receiver.stored += i64::from(accepted);
        }
        accepted
    }
}

fn items(tablet_capacity: i64, tablet_rate: i32) -> ItemEnergy {
    let mut config = rules::ServerConfig::DEFAULT;
    config.gear.tablet = rules::EnergyItem {
        capacity: tablet_capacity,
        rate: tablet_rate,
    };
    ItemEnergy {
        lookup: Rc::new(move |resource: &ItemResource| {
            rules::item_energy_spec(&resource.id, energy_of(resource), &config)
        }),
        write: Rc::new(|resource: &ItemResource, energy: i64| {
            Some(ItemResource::with_components(
                &resource.id,
                resource.max_stack_size,
                if energy == 0 {
                    Components::none()
                } else {
                    Components::new([(ENERGY, energy.to_string().as_str())])
                },
            ))
        }),
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

fn new_cube(tier: usize, meta: &Value) -> EnergyCube {
    let mut config = rules::ServerConfig::DEFAULT;
    config.gear.tablet.capacity = meta["tablet_capacity"].as_i64().unwrap();
    config.gear.tablet.rate = meta["tablet_rate"].as_i64().unwrap() as i32;
    let item = items(config.gear.tablet.capacity, config.gear.tablet.rate);
    let mut cube = EnergyCube::new(rules::energy_cube_config(tier, &config, item));
    cube.set_facing(Face::North);
    cube.apply_item_side_config(
        Some(&rules::energy_cube_default()),
        Some(&AttachedEjector::DEFAULT),
        false,
    );
    cube
}

fn set_slot(cube: &EnergyCube, slot: &str, spec: &str) {
    let parts: Vec<&str> = spec.split(':').collect();
    let (id, stack) = id_of(parts[0]);
    let resource = with_energy(id, stack, parts[1].parse().unwrap());
    let slot = if slot == "discharge" {
        cube.discharge_slot()
    } else {
        cube.charge_slot()
    };
    slot.set_contents(Some(resource), parts[2].parse().unwrap(), None);
}

fn apply(cube: &EnergyCube, neighbours: &Neighbours, mark: &str) {
    let parts: Vec<&str> = mark.split_whitespace().collect();
    match parts.as_slice() {
        ["energy", value] => cube.energy().set_energy(value.parse().unwrap(), None),
        ["charge", spec] => set_slot(cube, "charge", spec),
        ["discharge", spec] => set_slot(cube, "discharge", spec),
        ["recv", face, value] => {
            let face = Face::from_property(face).unwrap();
            neighbours
                .receivers
                .borrow_mut()
                .get_mut(&(face as usize))
                .unwrap()
                .stored = value.parse().unwrap();
        }
        _ => {}
    }
}

fn slot_json(slot: &machinery_basis_core::containers::InventorySlot) -> Value {
    let contents = slot.contents();
    match contents.resource {
        None => serde_json::json!(["", 0, 0]),
        Some(resource) => {
            let path = resource.id.split_once(':').unwrap().1.to_owned();
            serde_json::json!([path, contents.amount, energy_of(&resource)])
        }
    }
}

fn run(name: &str) -> usize {
    let lines = read(&format!("energy_cube_{name}.jsonl"));
    let meta = lines[0]["meta"].clone();
    let tier = meta["tier"].as_u64().unwrap() as usize;
    let mut cube = new_cube(tier, &meta);
    assert_eq!(cube.energy().capacity(), meta["capacity"].as_i64().unwrap());
    assert_eq!(
        i64::from(cube.transfer_rate()),
        meta["transfer_rate"].as_i64().unwrap()
    );
    let mut receivers = BTreeMap::new();
    for receiver in meta["receivers"].as_array().unwrap_or(&Vec::new()) {
        let face = Face::from_property(receiver["face"].as_str().unwrap()).unwrap();
        let capacity = if receiver["kind"] == "chamber" {
            1_000_000
        } else {
            4_000_000
        };
        receivers.insert(
            face as usize,
            Receiver {
                stored: receiver["energy"].as_i64().unwrap(),
                capacity,
            },
        );
    }
    let neighbours = Neighbours {
        receivers: RefCell::new(receivers),
        cube: None,
    };
    let mut differences = 0;
    for (index, line) in lines[1..].iter().enumerate() {
        let tick = line["t"].as_u64().unwrap();
        for mark in line["marks"].as_array().unwrap() {
            let mark = mark.as_str().unwrap();
            apply(&cube, &neighbours, mark);
            let parts: Vec<&str> = mark.split_whitespace().collect();
            match parts.as_slice() {
                ["eject", on] => cube
                    .side_config_mut()
                    .config_mut(TransmissionType::Energy)
                    .unwrap()
                    .set_ejecting(*on == "true"),
                ["mode", side, kind] => {
                    let side = RelativeSide::from_serialized_name(side).unwrap();
                    let kind = DataType::from_serialized_name(kind).unwrap();
                    cube.side_config_mut()
                        .config_mut(TransmissionType::Energy)
                        .unwrap()
                        .set_data_type(kind, side);
                }
                _ => {}
            }
        }
        cube.tick_server(100 + tick, &neighbours);
        let expected = &line["cube"];
        // O Java grava o `float` pelo texto mais curto; o `f64` que o JSON lê volta ao mesmo `f32`.
        let scale_ok = expected["scale"].as_f64().unwrap() as f32 == cube.energy_scale();
        if !scale_ok {
            differences += 1;
            eprintln!(
                "{name} t={index} scale: rust {} java {}",
                cube.energy_scale(),
                expected["scale"]
            );
        }
        let mut check = |what: &str, got: Value| {
            if got != expected[what] {
                differences += 1;
                if differences <= 12 {
                    eprintln!(
                        "{name} t={} {what}: rust {got} java {}",
                        index, expected[what]
                    );
                }
            }
        };
        check("energy", serde_json::json!(cube.energy().amount()));
        check(
            "comparator",
            serde_json::json!(cube.tile().current_redstone_level()),
        );
        check("charge", slot_json(cube.charge_slot()));
        check("discharge", slot_json(cube.discharge_slot()));
        check(
            "sides",
            serde_json::json!(
                cube.side_states()
                    .iter()
                    .map(|state| *state as usize)
                    .collect::<Vec<_>>()
            ),
        );
        for (face, energy) in line["receivers"].as_object().unwrap() {
            let face = Face::from_property(face).unwrap();
            let got = neighbours.receivers.borrow()[&(face as usize)].stored;
            if got != energy.as_i64().unwrap() {
                differences += 1;
                if differences <= 12 {
                    eprintln!("{name} t={index} receiver {face:?}: rust {got} java {energy}");
                }
            }
        }
    }
    differences
}

/// Um cubo avançado empurrando para um básico: o que o receptor conta como energia recebida (`getInputRate`). Os dois andam em ordem de colocação, o receptor primeiro.
fn run_receive() -> usize {
    let lines = read("energy_cube_receive.jsonl");
    let meta = lines[0]["meta"].clone();
    let subject = Rc::new(RefCell::new(new_cube(
        meta["tier"].as_u64().unwrap() as usize,
        &meta,
    )));
    let mut source = new_cube(meta["source_tier"].as_u64().unwrap() as usize, &meta);
    source.set_facing(Face::South);
    let mut differences = 0;
    for (index, line) in lines[1..].iter().enumerate() {
        let tick = line["t"].as_u64().unwrap();
        for mark in line["marks"].as_array().unwrap() {
            let parts: Vec<&str> = mark.as_str().unwrap().split_whitespace().collect();
            let mut cube = subject.borrow_mut();
            match parts.as_slice() {
                ["eject", on] => cube
                    .side_config_mut()
                    .config_mut(TransmissionType::Energy)
                    .unwrap()
                    .set_ejecting(*on == "true"),
                ["mode", side, kind] => {
                    cube.side_config_mut()
                        .config_mut(TransmissionType::Energy)
                        .unwrap()
                        .set_data_type(
                            DataType::from_serialized_name(kind).unwrap(),
                            RelativeSide::from_serialized_name(side).unwrap(),
                        );
                }
                ["src", value] => source.energy().set_energy(value.parse().unwrap(), None),
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let time = 100 + tick;
        subject
            .borrow_mut()
            .tick_server(time, &machinery_basis_core::energy_cube::NoEnergyNeighbours);
        // O tempo que o receptor vê quando o emissor empurra é o do tick.
        let neighbours = Neighbours {
            receivers: RefCell::new(BTreeMap::new()),
            cube: Some((Face::South, subject.clone(), time as i64)),
        };
        source.tick_server(time, &neighbours);
        let expected = &line["cube"];
        let cube = subject.borrow();
        for (what, got) in [
            ("energy", serde_json::json!(cube.energy().amount())),
            ("input_rate", serde_json::json!(cube.last_energy_received())),
            (
                "comparator",
                serde_json::json!(cube.tile().current_redstone_level()),
            ),
        ] {
            if got != expected[what] {
                differences += 1;
                eprintln!(
                    "receive t={index} {what}: rust {got} java {}",
                    expected[what]
                );
            }
        }
        if serde_json::json!(source.energy().amount()) != line["src"] {
            differences += 1;
            eprintln!(
                "receive t={index} src: rust {} java {}",
                source.energy().amount(),
                line["src"]
            );
        }
    }
    differences
}

#[test]
fn the_scenarios_of_the_original_repeat() {
    let mut total = 0;
    for name in [
        "eject_basic",
        "eject_split",
        "eject_odd",
        "eject_machines",
        "charge",
        "discharge",
        "creative",
        "scale",
    ] {
        total += run(name);
    }
    total += run_receive();
    assert_eq!(total, 0, "diferenças contra o original");
}
