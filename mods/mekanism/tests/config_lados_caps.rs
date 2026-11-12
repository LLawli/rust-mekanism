//! Repete os rastros `config_lados_caps` e `config_lados_increment` do original: a configuração de lados de cada máquina, posta por
//! `setBlock` e por um item, o que cada `DataType` faz com as capabilities de item e de energia de cada face, o giro do bloco e o
//! `incrementDataType`.
mod common;

use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::side_config::{
    ConfigData, DataType, RelativeSide, TileComponentConfig, TransmissionType,
};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{advanced_machine_default, electric_machine_default, item_instance};
use serde_json::Value;
use std::path::Path;

const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

fn trace(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line.get("meta").is_none())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap_or_else(|| panic!("face {name}"))
}

fn kind_of(name: &str) -> TransmissionType {
    TransmissionType::from_serialized_name(name).unwrap_or_else(|| panic!("tipo {name}"))
}

fn data_type(name: &str) -> DataType {
    DataType::from_serialized_name(name).unwrap_or_else(|| panic!("datatype {name}"))
}

fn machine_of(name: &str) -> ProgressMachine {
    if name == "metallurgic_infuser" {
        common::infuser()
    } else {
        common::chamber("clay")
    }
}

/// A máquina como ela nasce: a configuração em `NONE`. `item` aplica o padrão do item de bloco.
fn born(name: &str, origin: &str, facing: Face) -> ProgressMachine {
    let mut machine = machine_of(name);
    machine.set_facing(facing);
    if origin == "item" {
        let default = if name == "metallurgic_infuser" {
            advanced_machine_default()
        } else {
            electric_machine_default()
        };
        machine.apply_item_side_config(Some(&default), Some(&AttachedEjector::DEFAULT), false);
    }
    machine
}

/// Os seis `DataType` de cada tipo que a máquina tem, na ordem de `RelativeSide.VALUES`, como o rastro os grava.
fn config_of(config: &TileComponentConfig) -> Value {
    let mut out = serde_json::Map::new();
    for &kind in config.transmissions() {
        let info = config.config(kind).unwrap();
        let sides: Vec<Value> = RelativeSide::VALUES
            .iter()
            .map(|&side| Value::from(info.data_type(side).serialized_name()))
            .collect();
        out.insert(kind.serialized_name().to_owned(), Value::Array(sides));
    }
    Value::Object(out)
}

/// Põe no componente o que o rastro diz que o Java tinha: os seis lados e o `eject` de cada tipo.
fn load(machine: &mut ProgressMachine, expected: &Value) {
    let mut data = ConfigData::default();
    for (name, sides) in expected["config"].as_object().unwrap() {
        let ordinal = kind_of(name).ordinal();
        data.config[ordinal] = Some(
            sides
                .as_array()
                .unwrap()
                .iter()
                .map(|side| data_type(side.as_str().unwrap()).ordinal() as i32)
                .collect(),
        );
        data.eject[ordinal] = expected["ejecting"][name].as_bool();
    }
    machine.restore_side_config(Some(&data), None, false);
}

/// O que as capabilities de item de cada face deixam fazer: os slots, e se entram e saem (`ConfigHolder`).
fn item_caps(machine: &ProgressMachine) -> Value {
    let config = machine.side_config();
    let mut out = serde_json::Map::new();
    for name in FACES {
        let info = config.slot_info(TransmissionType::Item, machine.facing(), Some(face(name)));
        let reach = info.filter(|info| info.is_enabled());
        out.insert(
            name.to_owned(),
            serde_json::json!({
                "present": !config.is_capability_disabled(TransmissionType::Item, machine.facing(), Some(face(name))),
                "slots": reach.map_or(Vec::new(), |info| info.containers.clone()),
                "can_insert": reach.is_some_and(|info| info.can_input),
                "can_extract": reach.is_some_and(|info| info.can_output),
            }),
        );
    }
    Value::Object(out)
}

fn check_caps(expected: &Value, machine: &ProgressMachine, label: &str) {
    let config = machine.side_config();
    assert_eq!(config_of(config), expected["config"], "{label}: lados");
    for (kind, info) in expected["supported"].as_object().into_iter().flatten() {
        let ours: Vec<&str> = config
            .config(kind_of(kind))
            .unwrap()
            .supported_data_types()
            .into_iter()
            .map(DataType::serialized_name)
            .collect();
        let theirs: Vec<&str> = info
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(ours, theirs, "{label}: suportados de {kind}");
    }
    for (kind, can) in expected["can_eject"].as_object().into_iter().flatten() {
        let ours = config.config(kind_of(kind)).unwrap().can_eject();
        assert_eq!(Some(ours), can.as_bool(), "{label}: can_eject de {kind}");
    }
    if let Some(items) = expected.get("item") {
        assert_eq!(&item_caps(machine), items, "{label}: itens");
    }
    if let Some(energy) = expected.get("energy") {
        for name in FACES {
            let side = face(name);
            let ours = config.slot_info(TransmissionType::Energy, machine.facing(), Some(side));
            let want = &energy[name];
            let present = !config.is_capability_disabled(
                TransmissionType::Energy,
                machine.facing(),
                Some(side),
            );
            assert_eq!(
                Some(present),
                want["present"].as_bool(),
                "{label}: energia em {name}"
            );
            let accepts = ours.is_some_and(|info| info.can_input);
            assert_eq!(
                accepts,
                want["accepted"].as_i64().unwrap_or(0) > 0,
                "{label}: energia aceita em {name}"
            );
        }
    }
}

const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];

#[test]
fn the_caps_of_every_case_match_the_original() {
    let lines = trace("config_lados_caps.jsonl");
    let (mut compared, mut placed, mut moded) = (0, 0, 0);
    for line in &lines {
        let name = line["machine"].as_str().unwrap();
        if !MACHINES.contains(&name) {
            continue;
        }
        let step = line["step"].as_str().unwrap();
        let facing = face(line["facing"].as_str().unwrap());
        let label = format!("{name} {} {facing:?} {step}", line["origin"]);
        // Todo caso: as faces que a configuração do Java dá, lidas pelo componente da lib.
        let mut machine = born(name, "set_block", facing);
        load(&mut machine, line);
        check_caps(line, &machine, &label);
        compared += 1;
        // A máquina recém-posta tem de nascer com a configuração que o Java tinha.
        if step == "placed" {
            let machine = born(name, line["origin"].as_str().unwrap(), facing);
            check_caps(line, &machine, &label);
            placed += 1;
        }
        // Um `DataType` posto num lado: parte do padrão do item e põe, como o Java.
        if let Some(rest) = step.strip_prefix("set_mode:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if let [kind, side, mode] = parts[..] {
                let mut machine = born(name, "item", facing);
                let side = RelativeSide::from_serialized_name(side).unwrap();
                let changed = machine.side_config_mut().set_mode(
                    kind_of(kind),
                    side,
                    data_type(mode),
                    facing,
                );
                let _ = changed;
                assert_eq!(
                    config_of(machine.side_config()),
                    line["config"],
                    "{label}: depois do set_mode"
                );
                moded += 1;
            }
        }
    }
    assert!(compared >= 100, "só {compared} casos");
    assert!(placed >= 32, "só {placed} máquinas postas");
    assert!(moded >= 100, "só {moded} mudanças de modo");
}

#[test]
fn the_increment_sequences_match_the_original() {
    let lines = trace("config_lados_increment.jsonl");
    let mut compared = 0;
    for line in &lines {
        let Some(name) = line["machine"].as_str() else {
            continue;
        };
        if !MACHINES.contains(&name) || line.get("increment").is_none() {
            continue;
        }
        let kind = kind_of(line["type"].as_str().unwrap());
        let side = RelativeSide::from_serialized_name(line["side"].as_str().unwrap()).unwrap();
        let mut machine = born(name, "item", Face::North);
        let info = machine.side_config_mut().config_mut(kind).unwrap();
        let mut ups = Vec::new();
        for _ in 0..12 {
            ups.push(info.increment_data_type(side).serialized_name());
        }
        // O lado volta ao começo entre as duas sequências.
        info.set_data_type(data_type(line["start"].as_str().unwrap()), side);
        let mut downs = Vec::new();
        for _ in 0..12 {
            downs.push(info.decrement_data_type(side).serialized_name());
        }
        let theirs = |key: &str| -> Vec<&str> {
            line[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect()
        };
        assert_eq!(ups, theirs("increment"), "{name} {kind:?} {side:?}");
        assert_eq!(downs, theirs("decrement"), "{name} {kind:?} {side:?}");
        compared += 1;
    }
    assert!(compared >= 12, "só {compared} sequências");
}

/// O contêiner da instância (H2): em cada uma das configurações do original, o que cada face alcança e deixa fazer é o que as
/// capabilities de item do Java deixam, e o que o Horizon lê sem face (`inputs`, `outputs`) é a soma das faces que deixam.
#[test]
fn the_container_of_the_instance_gives_each_face_what_the_original_gives_it() {
    let (mut cases, mut faces_with_slots) = (0, 0);
    for line in &trace("config_lados_caps.jsonl") {
        let name = line["machine"].as_str().unwrap();
        let Some(items) = line.get("item") else {
            continue;
        };
        if !MACHINES.contains(&name) {
            continue;
        }
        let facing = face(line["facing"].as_str().unwrap());
        let label = format!("{name} {} {facing:?} {}", line["origin"], line["step"]);
        let mut machine = born(name, "set_block", facing);
        load(&mut machine, line);
        let instance = item_instance(name == "metallurgic_infuser", machine.side_config(), facing);
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for text in FACES {
            let side = face(text);
            let want = &items[text];
            let reached: Vec<usize> = want["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|slot| slot.as_u64().unwrap() as usize)
                .collect();
            assert_eq!(instance.is_present(side), want["present"], "{label}: capability em {text}");
            assert_eq!(instance.face_slots()[side as usize], reached, "{label}: slots em {text}");
            for &slot in &reached {
                assert_eq!(
                    instance.accepts_through(side, slot),
                    want["can_insert"] == true,
                    "{label}: entrada em {text}, slot {slot}"
                );
                assert_eq!(
                    instance.releases_through(side, slot),
                    want["can_extract"] == true,
                    "{label}: saída em {text}, slot {slot}"
                );
                faces_with_slots += 1;
                if want["can_insert"] == true && !inputs.contains(&slot) {
                    inputs.push(slot);
                }
                if want["can_extract"] == true && !outputs.contains(&slot) {
                    outputs.push(slot);
                }
            }
            if reached.is_empty() {
                assert!(
                    !instance.accepts_through(side, 0) && !instance.releases_through(side, 0),
                    "{label}: {text} sem slot não deixa nada"
                );
            }
        }
        inputs.sort_unstable();
        outputs.sort_unstable();
        assert_eq!(instance.inputs(), inputs, "{label}: entradas do contêiner");
        assert_eq!(instance.outputs(), outputs, "{label}: saídas do contêiner");
        cases += 1;
    }
    assert!(cases >= 100, "só {cases} configurações");
    assert!(faces_with_slots >= 300, "só {faces_with_slots} faces com slot");
}
