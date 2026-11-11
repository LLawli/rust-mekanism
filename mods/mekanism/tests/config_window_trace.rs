//! Repete o rastro `config_pacotes` do original: o estado da configuração de lados e do ejetor depois de cada pacote que as janelas de lados e de
//! transporte mandam (`PacketSideData`, `PacketEjectConfiguration`, `PacketBatchConfiguration`, `PacketEjectColor`, `PacketInputColor` e
//! `STRICT_INPUT`), tratados pelo `handle` do Java sobre cada uma das quatro máquinas postas por um jogador (virada para leste).
mod common;
use common::side::{born, color_id, default_of, kind_of, state_of};
use machinery_basis_core::config_window::{ClickType, ConfigPacket};
use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::side_config::{DataType, RelativeSide};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use serde_json::Value;
use std::path::Path;

fn trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/config_pacotes.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line.get("meta").is_none())
        .collect()
}

fn click(name: &str) -> ClickType {
    match name {
        "LEFT" => ClickType::Left,
        "RIGHT" => ClickType::Right,
        "SHIFT_LEFT" => ClickType::ShiftLeft,
        other => panic!("clique {other}"),
    }
}

fn side(name: &str) -> RelativeSide {
    RelativeSide::from_serialized_name(name).unwrap_or_else(|| panic!("lado {name}"))
}

fn packet_of(json: &Value) -> ConfigPacket {
    let text = |key: &str| json[key].as_str().unwrap();
    match text("kind") {
        "side_data" => ConfigPacket::SideData {
            click: click(text("click")),
            side: side(text("side")),
            transmission: kind_of(text("transmission")),
        },
        "eject" => ConfigPacket::Eject {
            transmission: kind_of(text("transmission")),
        },
        "batch" => ConfigPacket::Batch {
            transmission: (!text("transmission").is_empty()).then(|| kind_of(text("transmission"))),
            target: DataType::from_serialized_name(text("target")).unwrap(),
        },
        "eject_color" => ConfigPacket::EjectColor {
            click: click(text("click")),
        },
        "input_color" => ConfigPacket::InputColor {
            click: click(text("click")),
            side: side(text("side")),
        },
        "strict_input" => ConfigPacket::StrictInput,
        other => panic!("pacote {other}"),
    }
}

fn placed(name: &str) -> ProgressMachine {
    let mut machine = born(name);
    machine.set_facing(Face::East);
    machine.apply_item_side_config(
        Some(&default_of(name)),
        Some(&AttachedEjector::DEFAULT),
        false,
    );
    machine
}

#[test]
fn every_packet_leaves_the_state_the_original_leaves() {
    let lines = trace();
    assert!(lines.len() > 1000, "{}", lines.len());
    let mut machines: std::collections::HashMap<String, ProgressMachine> =
        std::collections::HashMap::new();
    let mut checked = 0;
    for line in &lines {
        let name = line["machine"].as_str().unwrap();
        let machine = machines
            .entry(name.to_owned())
            .or_insert_with(|| placed(name));
        let facing = machine.facing();
        let (config, ejector) = machine.side_parts_mut();
        packet_of(&line["packet"]).apply(config, ejector, facing);
        let ours = state_of(machine);
        let step = &line["step"];
        for key in ["config", "ejecting"] {
            assert_eq!(
                ours[key], line[key],
                "{name} passo {step}: {key} ({})",
                line["packet"]
            );
        }
        // O rastro grava a cor pelo nome de `getSerializedName`; a lib a guarda pelo `ordinal`.
        for field in ["strict_input", "output_color", "input_colors"] {
            assert_eq!(
                ours["ejector"][field], line["ejector"][field],
                "{name} passo {step}: {field} ({})",
                line["packet"]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, lines.len());
    assert!(color_id("black").is_some());
}
