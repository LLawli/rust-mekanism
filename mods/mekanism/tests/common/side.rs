//! O que os testes da configuração de lados partilham: o estado de uma máquina como o rastro o grava, e os formatos do NBT e do item
//! lidos do JSON do rastro.
use machinery_basis_core::ejector::{AttachedEjector, EjectorData};
use machinery_basis_core::side_config::{
    AttachedSideConfig, ConfigData, DataType, ENUM_COLOR_NAMES, LightConfigInfo, RelativeSide,
    TransmissionType,
};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::{ProgressMachine, RedstoneControl};
use mekanism_rules::{advanced_machine_default, electric_machine_default};
use serde_json::{Value, json};

pub fn color_id(name: &str) -> Option<u8> {
    ENUM_COLOR_NAMES
        .iter()
        .position(|known| *known == name)
        .map(|i| i as u8)
}

pub fn kind_of(name: &str) -> TransmissionType {
    TransmissionType::from_serialized_name(name).unwrap()
}

pub fn born(name: &str) -> ProgressMachine {
    let mut machine = if name == "metallurgic_infuser" {
        super::infuser()
    } else {
        super::chamber("clay")
    };
    machine.set_facing(Face::North);
    machine
}

pub fn default_of(name: &str) -> AttachedSideConfig {
    if name == "metallurgic_infuser" {
        advanced_machine_default()
    } else {
        electric_machine_default()
    }
}

/// O estado como o rastro o grava: a frente, o modo de redstone, os seis lados e o ejetar de cada tipo, e o ejetor.
pub fn state_of(machine: &ProgressMachine) -> Value {
    let config = machine.side_config();
    let mut sides = serde_json::Map::new();
    let mut ejecting = serde_json::Map::new();
    for &kind in config.transmissions() {
        let info = config.config(kind).unwrap();
        let names: Vec<Value> = RelativeSide::VALUES
            .iter()
            .map(|&side| json!(info.data_type(side).serialized_name()))
            .collect();
        sides.insert(kind.serialized_name().to_owned(), Value::Array(names));
        ejecting.insert(kind.serialized_name().to_owned(), json!(info.is_ejecting()));
    }
    let ejector = machine.ejector();
    let colors: Vec<Value> = RelativeSide::VALUES
        .iter()
        .map(|&side| {
            json!(
                ejector
                    .input_color(side)
                    .map_or("", |color| ENUM_COLOR_NAMES[usize::from(color)])
            )
        })
        .collect();
    json!({
        "facing": "north",
        "control_type": machine.tile().control_type().serialized_name(),
        "config": sides,
        "ejecting": ejecting,
        "ejector": {
            "strict_input": ejector.has_strict_input(),
            "output_color": ejector.output_color().map_or("", |color| ENUM_COLOR_NAMES[usize::from(color)]),
            "input_colors": colors,
        },
    })
}

/// Põe no objeto o estado do rastro, pelo caminho do NBT: o que `loadAdditional` lê.
pub fn load_state(machine: &mut ProgressMachine, state: &Value) {
    let mut data = ConfigData::default();
    for (name, sides) in state["config"].as_object().unwrap() {
        let ordinal = kind_of(name).ordinal();
        data.config[ordinal] = Some(
            sides
                .as_array()
                .unwrap()
                .iter()
                .map(|side| {
                    DataType::from_serialized_name(side.as_str().unwrap())
                        .unwrap()
                        .ordinal() as i32
                })
                .collect(),
        );
        data.eject[ordinal] = state["ejecting"][name].as_bool();
    }
    let ejector = &state["ejector"];
    let colors: Vec<i32> = ejector["input_colors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|color| color_id(color.as_str().unwrap()).map_or(-1, i32::from))
        .collect();
    let ejector_data = EjectorData {
        strict_input: ejector["strict_input"].as_bool().filter(|&strict| strict),
        color: color_id(ejector["output_color"].as_str().unwrap()),
        input_color: colors.iter().any(|&color| color >= 0).then_some(colors),
    };
    machine.restore_side_config(Some(&data), Some(&ejector_data), false);
    machine
        .tile_mut()
        .set_control_type(RedstoneControl::from_serialized_name(
            state["control_type"].as_str().unwrap(),
        ));
}

/// `AttachedSideConfig.CODEC` lido do JSON do rastro.
pub fn side_config_from(json: &Value) -> AttachedSideConfig {
    let mut attached = AttachedSideConfig::default();
    for (name, info) in json["config"].as_object().unwrap() {
        let mut light = LightConfigInfo {
            ejecting: info["eject"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        for (side, data_type) in info["side"].as_object().unwrap() {
            light.side_config.insert(
                RelativeSide::from_serialized_name(side).unwrap(),
                DataType::from_serialized_name(data_type.as_str().unwrap()).unwrap(),
            );
        }
        attached.config_info.insert(kind_of(name), light);
    }
    attached
}

/// `AttachedEjector.CODEC`: a cor de saída vai na chave `types`.
pub fn ejector_from(json: &Value) -> AttachedEjector {
    let mut attached = AttachedEjector::DEFAULT;
    for (slot, color) in attached
        .input_colors
        .iter_mut()
        .zip(json["input_color"].as_array().unwrap())
    {
        *slot = color.as_str().and_then(color_id);
    }
    attached.strict_input = json["strict_input"].as_bool().unwrap();
    attached.output_color = json["types"].as_str().and_then(color_id);
    attached
}

/// O que o Java grava no NBT do tile (chaves ordenadas, números sem tipo) lido de volta nos dados da lib.
pub fn config_nbt_from(json: &Value) -> ConfigData {
    let mut data = ConfigData::default();
    for kind in TransmissionType::VALUES {
        let ordinal = kind.ordinal();
        data.eject[ordinal] = json[format!("eject{ordinal}")]
            .as_i64()
            .map(|flag| flag != 0);
        data.config[ordinal] = json[format!("config{ordinal}")].as_array().map(|sides| {
            sides
                .iter()
                .map(|side| side.as_i64().unwrap() as i32)
                .collect()
        });
    }
    data
}

pub fn ejector_nbt_from(json: &Value) -> EjectorData {
    EjectorData {
        strict_input: json["strict_input"].as_i64().map(|flag| flag != 0),
        color: json["color"].as_str().and_then(color_id),
        input_color: json["input_color"]
            .as_array()
            .map(|colors| colors.iter().map(|c| c.as_i64().unwrap() as i32).collect()),
    }
}
