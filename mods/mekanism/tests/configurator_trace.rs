//! O Configurator (`ItemConfigurator`) e o cartão de configuração (`ItemConfigurationCard`) contra o original em jogo
//! (`testes/ConfiguratorTraceTest.java`, os `rastros/configurator_*.jsonl`): os modos, as mensagens, o clique que muda os lados e a troca de modo.
mod common;

use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::side_config::{DataType, RelativeSide, SideClick, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{
    ConfiguratorMode, DisplayChange, advanced_machine_default, card_cleared_message,
    card_got_message, card_set_message, card_tooltip_line, card_unequal_message, change_mode,
    configurator_side_message, electric_machine_default, rotate_facing,
};
use mekanism_rules::{HeldItem, TileUse, tile_use};
use serde_json::Value;
use std::path::Path;

fn rows(file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect()
}

fn mode_of(name: &str) -> ConfiguratorMode {
    ConfiguratorMode::from_serialized_name(name).unwrap_or_else(|| panic!("modo {name}"))
}

fn face_of(name: &str) -> Face {
    Face::from_property(name).unwrap_or_else(|| panic!("face {name}"))
}

fn data_type_of(name: &str) -> DataType {
    DataType::from_serialized_name(name).unwrap_or_else(|| panic!("tipo {name}"))
}

#[test]
fn the_eight_modes_are_the_originals() {
    let mut checked = 0;
    for row in rows("configurator_modes.jsonl") {
        if row["case"] != "mode" {
            continue;
        }
        let mode = mode_of(row["mode"].as_str().unwrap());
        assert_eq!(mode.ordinal() as i64, row["ordinal"].as_i64().unwrap());
        assert_eq!(
            mode.transmission().map(|kind| kind.serialized_name()),
            row["transmission"].as_str()
        );
        assert_eq!(
            mode.is_configurating(),
            row["is_configurating"].as_bool().unwrap()
        );
        let rgb: Vec<u32> = row["color"]["rgb"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            mekanism_rules::enum_color_rgb(mode.color()),
            rgb[0] << 16 | rgb[1] << 8 | rgb[2],
            "{mode:?}"
        );
        assert_eq!(
            machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(mode.color())],
            row["color"]["name"].as_str().unwrap()
        );
        assert_eq!(mode.text().to_json(), row["text_component"], "{mode:?}");
        assert_eq!(mode.text().to_json(), row["scroll_text"], "{mode:?}");
        assert_eq!(
            vec![mode.tooltip_line().to_json()],
            *row["add_to_tooltip"].as_array().unwrap(),
            "{mode:?}"
        );
        // As linhas do tooltip da pilha: o nome (o jogo) e a do appender.
        assert_eq!(row["item_tooltip"][1], mode.tooltip_line().to_json());
        for (shift, expected) in row["adjust"].as_object().unwrap() {
            assert_eq!(
                mode.adjust(shift.parse().unwrap()).serialized_name(),
                expected.as_str().unwrap(),
                "{mode:?} {shift}"
            );
        }
        let abilities = mode.abilities();
        for (name, expected) in row["abilities"].as_object().unwrap() {
            assert_eq!(
                abilities.get(name.as_str()),
                expected.as_bool().as_ref(),
                "{mode:?} {name}"
            );
        }
        assert_eq!(
            mode.sneak_bypasses_use(),
            row["sneak_bypass_use"].as_bool().unwrap(),
            "{mode:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

/// Uma pilha nova traz o modo padrão (o protótipo do item tem o componente) e o tooltip mostra a linha; uma pilha a que se tirou o componente responde
/// o modo padrão e não mostra a linha (o appender só roda com o componente).
#[test]
fn a_stack_without_the_component_is_the_default_mode() {
    let mut seen = 0;
    for row in rows("configurator_modes.jsonl") {
        match row["case"].as_str().unwrap() {
            "fresh" => {
                assert_eq!(
                    row["mode_component"],
                    ConfiguratorMode::DEFAULT.serialized_name()
                );
                assert_eq!(
                    row["item_tooltip"][1],
                    ConfiguratorMode::DEFAULT.tooltip_line().to_json()
                );
                seen += 1;
            }
            "removed" => {
                assert_eq!(row["item_tooltip"].as_array().unwrap().len(), 1);
                assert_eq!(row["get_mode"], ConfiguratorMode::DEFAULT.serialized_name());
                seen += 1;
            }
            _ => {}
        }
    }
    assert_eq!(seen, 2);
}

fn machine(name: &str) -> ProgressMachine {
    let mut machine = match name {
        "metallurgic_infuser" => common::infuser(),
        _ => common::chamber("clay"),
    };
    machine.set_facing(Face::North);
    let default = if name == "metallurgic_infuser" {
        advanced_machine_default()
    } else {
        electric_machine_default()
    };
    machine.apply_item_side_config(Some(&default), Some(&AttachedEjector::DEFAULT), false);
    machine
}

fn config_names(machine: &ProgressMachine, kind: TransmissionType) -> Option<Vec<&'static str>> {
    let info = machine.side_config().config(kind)?;
    Some(
        RelativeSide::VALUES
            .iter()
            .map(|side| info.data_type(*side).serialized_name())
            .collect(),
    )
}

/// O ciclo de `incrementDataType` com doze cliques agachados na mesma face, e a mensagem de cada um.
#[test]
fn twelve_sneaking_clicks_walk_the_data_types_with_the_messages_of_the_original() {
    let mut checked = 0;
    let mut started = std::collections::BTreeSet::new();
    let mut current: Option<ProgressMachine> = None;
    for row in rows("configurator_click_sequence.jsonl") {
        let name = row["machine"].as_str().unwrap();
        let mode = mode_of(row["mode"].as_str().unwrap());
        let face = face_of(row["face"].as_str().unwrap());
        let key = (
            name.to_owned(),
            row["mode"].as_str().unwrap().to_owned(),
            row["face"].as_str().unwrap().to_owned(),
        );
        if started.insert(key) {
            current = Some(machine(name));
        }
        let machine = current.as_mut().unwrap();
        let kind = mode.transmission().unwrap();
        let click = machine
            .side_config_mut()
            .configurator_click(kind, Face::North, face, true);
        assert_eq!(
            click.is_some(),
            row["supported"].as_bool().unwrap(),
            "{:?}",
            row["marks"]
        );
        match click {
            None => assert!(
                row["overlay"].as_array().unwrap().is_empty() && row["result"]["class"] == "Pass",
                "{:?}",
                row["marks"]
            ),
            Some(SideClick::Toggled { old, new }) => {
                assert_eq!(
                    data_type_of(row["data_type"].as_str().unwrap()),
                    new,
                    "{:?}",
                    row["marks"]
                );
                if new == old {
                    assert!(
                        row["overlay"].as_array().unwrap().is_empty(),
                        "{:?}",
                        row["marks"]
                    );
                } else {
                    assert_eq!(
                        vec![configurator_side_message(true, kind, new).to_json()],
                        *row["overlay"].as_array().unwrap(),
                        "{:?}",
                        row["marks"]
                    );
                }
                for (type_name, expected) in row["config_after"].as_object().unwrap() {
                    let kind = TransmissionType::from_serialized_name(type_name).unwrap();
                    let expected: Vec<&str> = expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect();
                    assert_eq!(
                        config_names(machine, kind).unwrap(),
                        expected,
                        "{:?} {type_name}",
                        row["marks"]
                    );
                }
            }
            Some(SideClick::View(_)) => panic!("agachado não só olha"),
        }
        machine.take_side_changes();
        checked += 1;
    }
    assert_eq!(checked, 240);
}

/// O clique sem agachar só olha (`CONFIGURATOR_VIEW_MODE`), sem mudar nada.
#[test]
fn a_click_without_sneaking_only_views_the_side() {
    let mut machine = machine("enrichment_chamber");
    let before = config_names(&machine, TransmissionType::Item);
    let click = machine.side_config_mut().configurator_click(
        TransmissionType::Item,
        Face::North,
        Face::West,
        false,
    );
    assert_eq!(click, Some(SideClick::View(DataType::Output)));
    assert_eq!(config_names(&machine, TransmissionType::Item), before);
    assert!(machine.take_side_changes().is_empty());
    assert_eq!(
        configurator_side_message(false, TransmissionType::Item, DataType::Output).to_json()["translate"],
        "configurator.mekanism.view_mode"
    );
}

/// Uma máquina sem o tipo (fluido e calor na Chamber) não entra no ramo de configurar.
#[test]
fn a_type_the_machine_lacks_is_not_a_side_click() {
    let mut machine = machine("enrichment_chamber");
    for kind in [
        TransmissionType::Fluid,
        TransmissionType::Heat,
        TransmissionType::Chemical,
    ] {
        assert_eq!(
            machine
                .side_config_mut()
                .configurator_click(kind, Face::North, Face::North, true),
            None
        );
    }
}

#[test]
fn rotating_follows_the_original() {
    let mut checked = 0;
    for file in [
        "configurator_click_enrichment_chamber.jsonl",
        "configurator_click_metallurgic_infuser.jsonl",
        "configurator_click_crusher.jsonl",
        "configurator_click_heat_generator.jsonl",
    ] {
        for row in rows(file) {
            if row["mode"] != "rotate" || row["before"]["facing"].is_null() {
                continue;
            }
            let from = face_of(row["before"]["facing"].as_str().unwrap());
            let side = face_of(row["face"].as_str().unwrap());
            // Agachado vai o item (`setFacing(side.getOpposite())`); sem agachar o bloco gira (`MekanismUtils.rotate`, com o Y).
            let want = if row["sneak"].as_bool().unwrap() {
                side.opposite()
            } else {
                rotate_facing(from, true)
            };
            assert_eq!(
                want,
                face_of(row["after"]["facing"].as_str().unwrap()),
                "{:?}",
                row["marks"]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 4 * 12);
}

#[test]
fn the_card_says_what_the_original_says() {
    let rows = rows("configurator_card.jsonl");
    let mut checked = 0;
    for row in &rows {
        let step = row["marks"][0].as_str().unwrap();
        let machine_block = |name: &str| {
            let namespace = if name == "heat_generator" {
                "mekanismgenerators"
            } else {
                "mekanism"
            };
            mekanism_rules::block_description_id(&format!("{namespace}:{name}"))
        };
        match step {
            "read" | "read_overwrite" => {
                let key = machine_block(row["marks"][1].as_str().unwrap());
                assert_eq!(
                    vec![card_got_message(&key).to_json()],
                    *row["overlay"].as_array().unwrap()
                );
                assert_eq!(
                    row["card_after"]["configuration_json"]["data_name"],
                    key.as_str()
                );
                checked += 1;
            }
            "apply" if row["marks"][5] == "sneak" => {
                // Agachado o cartão lê outra vez, mesmo com dados.
                let key = machine_block(row["marks"][1].as_str().unwrap());
                assert_eq!(
                    vec![card_got_message(&key).to_json()],
                    *row["overlay"].as_array().unwrap()
                );
                checked += 1;
            }
            "apply" => {
                let from = row["marks"][1].as_str().unwrap();
                let to = row["marks"][3].as_str().unwrap();
                let message = if from == to {
                    card_set_message(Some(&machine_block(from)))
                } else {
                    card_unequal_message()
                };
                assert_eq!(
                    vec![message.to_json()],
                    *row["overlay"].as_array().unwrap(),
                    "{:?}",
                    row["marks"]
                );
                checked += 1;
            }
            "use_air" if row["sneak"].as_bool().unwrap() => {
                assert_eq!(
                    vec![card_cleared_message().to_json()],
                    *row["overlay"].as_array().unwrap()
                );
                checked += 1;
            }
            "tooltip" => {
                let name = row["marks"][1].as_str().unwrap();
                let key = (name != "empty").then(|| machine_block(name));
                assert_eq!(
                    row["tooltip"][1],
                    card_tooltip_line(key.as_deref()).to_json(),
                    "{name}"
                );
                checked += 1;
            }
            _ => {}
        }
    }
    assert!(checked >= 60, "{checked}");
}

#[test]
fn changing_the_mode_follows_the_original() {
    let mut checked = 0;
    for row in rows("configurator_changemode.jsonl") {
        let mode = mode_of(row["mode"].as_str().unwrap());
        let display = match row["display"].as_str().unwrap() {
            "NONE" => DisplayChange::None,
            "MAIN_HAND" => DisplayChange::MainHand,
            _ => DisplayChange::Other,
        };
        let change = change_mode(mode, row["shift"].as_i64().unwrap() as i32, display);
        assert_eq!(
            change.mode.serialized_name(),
            row["after"].as_str().unwrap(),
            "{row}"
        );
        assert_eq!(change.changed, row["changed"].as_bool().unwrap(), "{row}");
        assert_eq!(
            change.show,
            row["show_mode_change"].as_bool().unwrap(),
            "{row}"
        );
        let overlay: Vec<Value> = change.overlay.iter().map(|chat| chat.to_json()).collect();
        assert_eq!(overlay, *row["overlay"].as_array().unwrap(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 168);
}

/// `MekanismUtils.rotate` (`MekanismUtils.java:234-246`), nas seis direções de partida: o rastro só gira a partir do norte.
#[test]
fn rotate_walks_the_cycle_of_the_original_from_every_face() {
    for (from, with_y, without_y) in [
        (Face::Up, Face::North, Face::Up),
        (Face::Down, Face::South, Face::Down),
        (Face::North, Face::East, Face::East),
        (Face::South, Face::West, Face::West),
        (Face::West, Face::Up, Face::North),
        (Face::East, Face::Down, Face::South),
    ] {
        assert_eq!(rotate_facing(from, true), with_y, "{from:?}");
        // `Direction.getClockWise` (só as horizontais; as verticais não giram).
        assert_eq!(rotate_facing(from, false), without_y, "{from:?}");
    }
}

/// O centro de cada tamanho de transmissor (`box(5..11)` e `box(4..12)`); a seleção parcial é de `tests/transmitter_pieces.rs`.
#[test]
fn the_center_of_a_transmitter_is_the_one_of_its_size() {
    use mekanism_rules::transmitter_center;
    let center = transmitter_center("mekanism:basic_universal_cable").unwrap();
    assert_eq!(
        center,
        [
            5.0 / 16.0,
            5.0 / 16.0,
            5.0 / 16.0,
            11.0 / 16.0,
            11.0 / 16.0,
            11.0 / 16.0
        ]
    );
    assert_eq!(
        transmitter_center("mekanism:ultimate_mechanical_pipe").unwrap()[0],
        4.0 / 16.0
    );
    assert_eq!(
        transmitter_center("mekanism:diversion_transporter").unwrap()[3],
        12.0 / 16.0
    );
    assert!(transmitter_center("mekanism:enrichment_chamber").is_none());
}

/// `TileEntityMekanism.openGui` e `BlockTile.useItemOn` no clique da mão principal num bloco de Mekanism: a tela abre (`menu` do rastro) só quando o bloco a abre, e o
/// giro e o cartão passam à frente dela.
#[test]
fn the_block_acts_before_the_screen_like_the_original() {
    let mut checked = 0;
    for file in [
        "configurator_click_enrichment_chamber.jsonl",
        "configurator_click_metallurgic_infuser.jsonl",
        "configurator_click_crusher.jsonl",
        "configurator_click_heat_generator.jsonl",
    ] {
        for row in rows(file) {
            let held = HeldItem::Configurator(mode_of(row["mode"].as_str().unwrap()));
            let sneaking = row["sneak"].as_bool().unwrap();
            let opened = !row["menu"].is_null();
            let use_ = tile_use(held, sneaking);
            assert_eq!(opened, use_ == TileUse::Menu, "{:?}", row["marks"]);
            // Sem agachar, só o giro responde `SUCCESS` sem abrir a tela e com o balanço previsto (do cliente).
            let rotated = !opened && !sneaking;
            assert_eq!(rotated, use_ == TileUse::Rotate, "{:?}", row["marks"]);
            if rotated {
                assert_eq!(
                    row["result"]["swing_source"], "PREDICTED",
                    "{:?}",
                    row["marks"]
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 4 * 96);
}

#[test]
fn the_card_passes_the_screen_to_the_item_like_the_original() {
    let mut checked = 0;
    for row in rows("configurator_card.jsonl") {
        let step = row["marks"][0].as_str().unwrap();
        if !matches!(step, "read" | "read_overwrite" | "apply" | "apply_empty") {
            continue;
        }
        let sneaking = row["sneak"].as_bool().unwrap();
        let use_ = tile_use(HeldItem::Card, sneaking);
        assert!(row["menu"].is_null(), "{:?}", row["marks"]);
        assert_eq!(
            use_,
            if sneaking {
                TileUse::Nothing
            } else {
                TileUse::PassToItem
            },
            "{:?}",
            row["marks"]
        );
        // O cartão responde `SUCCESS` com o balanço do servidor, ou `PASS` sem dados.
        let class = row["result"]["class"].as_str().unwrap();
        assert!(matches!(class, "Success" | "Pass"), "{:?}", row["marks"]);
        checked += 1;
    }
    assert_eq!(checked, 66);
}

#[test]
fn the_screen_opens_only_standing_without_the_card_or_the_rotation() {
    let modes = ConfiguratorMode::VALUES;
    for mode in modes {
        let want = if mode == ConfiguratorMode::Rotate {
            TileUse::Rotate
        } else {
            TileUse::Menu
        };
        assert_eq!(tile_use(HeldItem::Configurator(mode), false), want);
        assert_eq!(
            tile_use(HeldItem::Configurator(mode), true),
            TileUse::Nothing
        );
    }
    assert_eq!(tile_use(HeldItem::Hands, false), TileUse::Menu);
    assert_eq!(tile_use(HeldItem::Other, false), TileUse::Menu);
    assert_eq!(tile_use(HeldItem::Hands, true), TileUse::Nothing);
    assert_eq!(tile_use(HeldItem::Other, true), TileUse::Nothing);
}
