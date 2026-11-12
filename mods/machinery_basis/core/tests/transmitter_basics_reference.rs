//! Os fatos estáticos e o NBT do `Transmitter` contra o original: `dev/referencia/rastros/transmitter_types.jsonl`
//! e `transmitter_nbt.jsonl` (`testes/TransmitterConnectionTraceTest.java`, `types` e `nbt`).

use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::transmitter::{Compound, Effect, Tag, Transmitter};
use serde_json::Value;
use std::path::PathBuf;

fn trace(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &Value| line.get("meta").is_none())
        .collect()
}

fn connection(name: &str) -> ConnectionType {
    ConnectionType::VALUES
        .into_iter()
        .find(|kind| kind.serialized_name() == name)
        .unwrap_or_else(|| panic!("tipo {name}"))
}

fn connection_by_enum_name(name: &str) -> ConnectionType {
    connection(&name.to_lowercase())
}

fn transmission_by_enum_name(name: &str) -> TransmissionType {
    TransmissionType::VALUES
        .into_iter()
        .find(|kind| kind.enum_name() == name)
        .unwrap_or_else(|| panic!("transmissão {name}"))
}

#[test]
fn connection_types_match_the_original() {
    let mut seen = 0;
    for line in trace("transmitter_types.jsonl") {
        if line["kind"] != "connection_type" {
            continue;
        }
        let kind = ConnectionType::VALUES[line["ordinal"].as_u64().unwrap() as usize];
        let label = line["name"].as_str().unwrap();
        assert_eq!(connection_by_enum_name(label), kind, "{label}");
        assert_eq!(kind.serialized_name(), line["serialized"], "{label}");
        assert_eq!(kind.color_name(), line["color"], "{label}");
        assert_eq!(
            format!("connection.mekanism.{}", kind.lang_name()),
            line["lang_key"].as_str().unwrap(),
            "{label}"
        );
        assert_eq!(kind.can_accept(), line["can_accept"], "{label}");
        assert_eq!(kind.can_send_to(), line["can_send_to"], "{label}");
        let name = |kind: ConnectionType| kind.serialized_name().to_uppercase();
        assert_eq!(name(kind.get_next()), line["next"], "{label} next");
        assert_eq!(name(kind.get_previous()), line["previous"], "{label}");
        let not_none = |kind: ConnectionType| kind != ConnectionType::None;
        assert_eq!(
            name(kind.get_next_valid(not_none)),
            line["next_skipping_none"],
            "{label}"
        );
        assert_eq!(
            name(kind.get_previous_valid(not_none)),
            line["previous_skipping_none"],
            "{label}"
        );
        assert_eq!(name(kind.adjust(2)), line["adjust_plus_2"], "{label}");
        assert_eq!(name(kind.adjust(-5)), line["adjust_minus_5"], "{label}");
        seen += 1;
    }
    assert_eq!(seen, 4);
}

#[test]
fn ids_wrap_or_fall_back_like_the_original() {
    let mut seen = 0;
    for line in trace("transmitter_types.jsonl") {
        if line["kind"] != "by_id" {
            continue;
        }
        let id = line["id"].as_i64().unwrap() as i32;
        assert_eq!(
            ConnectionType::by_id(id).serialized_name().to_uppercase(),
            line["connection"].as_str().unwrap(),
            "connection {id}"
        );
        assert_eq!(
            TransmissionType::by_id(id).enum_name(),
            line["transmission"].as_str().unwrap(),
            "transmission {id}"
        );
        seen += 1;
    }
    assert_eq!(seen, 22);
}

#[test]
fn transmission_types_match_the_original() {
    let mut seen = 0;
    for line in trace("transmitter_types.jsonl") {
        if line["kind"] != "transmission_type" {
            continue;
        }
        let kind = transmission_by_enum_name(line["name"].as_str().unwrap());
        assert_eq!(kind.ordinal() as u64, line["ordinal"].as_u64().unwrap());
        assert_eq!(kind.network_name(), line["network_name"]);
        assert_eq!(kind.transmission(), line["transmission"]);
        assert_eq!(kind.transmission(), line["serialized"]);
        assert_eq!(
            format!("mekanism:{}", kind.gui_texture_path()),
            line["gui_texture"].as_str().unwrap()
        );
        assert_eq!(
            format!("transmission.mekanism.{}", kind.transmission()),
            line["lang_key"].as_str().unwrap()
        );
        seen += 1;
    }
    assert_eq!(seen, 5);
}

/// Um transmissor recém-criado de cada bloco do original nasce com os seis lados `NORMAL`, sem
/// conexão nenhuma, e é o `handlesRedstone()` e os tipos dele que o mod passa.
#[test]
fn every_transmitter_block_is_born_the_same() {
    let mut seen = 0;
    for line in trace("transmitter_types.jsonl") {
        if line["kind"] != "block" {
            continue;
        }
        let supported: Vec<TransmissionType> = line["supported"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| transmission_by_enum_name(name.as_str().unwrap()))
            .collect();
        let transmitter: Transmitter<()> = Transmitter::new(
            [0, 0, 0],
            TransmissionSet::of(&supported),
            line["handles_redstone"].as_bool().unwrap(),
        );
        let id = line["id"].as_str().unwrap();
        assert_eq!(transmitter.handles_redstone(), line["handles_redstone"], "{id}");
        for (index, side) in Face::ALL.iter().enumerate() {
            assert_eq!(
                transmitter.get_connection_type_raw(*side).serialized_name(),
                line["raw"][index].as_str().unwrap(),
                "{id} {side:?}"
            );
            assert_eq!(
                transmitter.get_connection_type(*side).serialized_name(),
                line["types"][index].as_str().unwrap(),
                "{id} {side:?}"
            );
        }
        assert_eq!(
            u64::from(transmitter.current_transmitter_connections),
            line["cur_t"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(transmitter.acceptor_cache.current_acceptor_connections),
            line["cur_a"].as_u64().unwrap()
        );
        seen += 1;
    }
    assert_eq!(seen, 22);
}

/// A etiqueta tipada que o rastro grava.
pub fn tag_from(value: &Value) -> Tag {
    if let Some(v) = value.get("byte") {
        Tag::Byte(v.as_i64().unwrap() as i8)
    } else if let Some(v) = value.get("int") {
        Tag::Int(v.as_i64().unwrap() as i32)
    } else if let Some(v) = value.get("string") {
        Tag::String(v.as_str().unwrap().to_owned())
    } else if let Some(v) = value.get("int_array") {
        Tag::IntArray(
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap() as i32)
                .collect(),
        )
    } else {
        let text = value["other"].as_str().unwrap();
        let (number, suffix) = text.split_at(text.len() - 1);
        match suffix {
            "d" => Tag::Double(number.parse().unwrap()),
            "L" => Tag::Long(number.parse().unwrap()),
            "s" => Tag::Short(number.parse().unwrap()),
            "f" => Tag::Float(number.parse().unwrap()),
            other => panic!("etiqueta {text} ({other})"),
        }
    }
}

pub fn compound_from(value: &Value) -> Compound {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, tag)| (key.clone(), tag_from(tag)))
        .collect()
}

fn state_matches(t: &Transmitter<()>, expected: &Value, label: &str) {
    assert_eq!(
        u64::from(t.current_transmitter_connections),
        expected["cur_t"].as_u64().unwrap(),
        "{label} cur_t"
    );
    assert_eq!(
        u64::from(t.acceptor_cache.current_acceptor_connections),
        expected["cur_a"].as_u64().unwrap(),
        "{label} cur_a"
    );
    for (index, side) in Face::ALL.iter().enumerate() {
        assert_eq!(
            t.get_connection_type_raw(*side).serialized_name(),
            expected["raw"][index].as_str().unwrap(),
            "{label} raw {side:?}"
        );
        assert_eq!(
            t.get_connection_type(*side).serialized_name(),
            expected["types"][index].as_str().unwrap(),
            "{label} type {side:?}"
        );
    }
    assert_eq!(t.redstone_reactive, expected["reactive"], "{label} reactive");
    assert_eq!(t.has_pull_side(), expected["has_pull"], "{label} has_pull");
    assert_eq!(
        t.has_transmitter_network(),
        expected["has_network"],
        "{label} has_network"
    );
}

#[test]
fn read_and_update_tag_match_the_original() {
    let mut cases = 0;
    for line in trace("transmitter_nbt.jsonl") {
        let label = line["case"].as_str().unwrap();
        let tag = compound_from(&line["tag"]);
        let supported = if label.contains("cable") {
            TransmissionSet::of(&[TransmissionType::Energy])
        } else {
            TransmissionSet::of(&[TransmissionType::Fluid])
        };
        let mut read: Transmitter<()> = Transmitter::new([0, 0, 0], supported, true);
        read.read(&tag, &mut Vec::new());
        state_matches(&read, &line["read"], &format!("{label} read"));

        let mut update: Transmitter<()> = Transmitter::new([0, 0, 0], supported, true);
        let result = update.handle_update_tag(&tag, &mut Vec::new());
        assert_eq!(
            result.refresh_model_data, line["update"]["refresh_model_data"],
            "{label} refresh_model_data"
        );
        state_matches(&update, &line["update"], &format!("{label} update"));
        cases += 1;
    }
    assert_eq!(cases, 30);
}

#[test]
fn reading_a_tag_reports_each_changed_side_once() {
    let mut t: Transmitter<()> = Transmitter::new([0, 0, 0], TransmissionSet::default(), true);
    let mut out = Vec::new();
    t.read(
        &compound_from(&serde_json::json!({"connection": {"int_array": [0, 1, 2, 3, 1, 1]}})),
        &mut out,
    );
    assert_eq!(
        out,
        vec![
            Effect::SideChanged {
                side: Face::Up,
                old: ConnectionType::Normal,
                new: ConnectionType::Push
            },
            Effect::SideChanged {
                side: Face::North,
                old: ConnectionType::Normal,
                new: ConnectionType::Pull
            },
            Effect::SideChanged {
                side: Face::South,
                old: ConnectionType::Normal,
                new: ConnectionType::None
            },
            Effect::SideChanged {
                side: Face::West,
                old: ConnectionType::Normal,
                new: ConnectionType::Push
            },
            Effect::SideChanged {
                side: Face::East,
                old: ConnectionType::Normal,
                new: ConnectionType::Push
            },
        ]
    );
}
