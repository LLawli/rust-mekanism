//! `writeConfigurationData` e `setConfigurationData` de `TileEntityConfigurableMachine`, o par que o Configurator e o cartão de configuração
//! usam: o modo de redstone, `component_config` e `component_ejector`.
mod common;

use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::side_config::{DataType, RelativeSide, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::{ConfigurationData, ProgressMachine, RedstoneControl};
use mekanism_rules::electric_machine_default;

fn placed() -> ProgressMachine {
    let mut machine = common::chamber("clay");
    machine.set_facing(Face::East);
    machine.apply_item_side_config(
        Some(&electric_machine_default()),
        Some(&AttachedEjector::DEFAULT),
        false,
    );
    machine
}

fn item_info(machine: &ProgressMachine) -> &machinery_basis_core::side_config::ConfigInfo {
    machine
        .side_config()
        .config(TransmissionType::Item)
        .unwrap()
}

#[test]
fn the_card_carries_the_redstone_mode_the_sides_and_the_ejector() {
    let mut from = placed();
    from.tile_mut().set_control_type(RedstoneControl::Low);
    {
        let info = from
            .side_config_mut()
            .config_mut(TransmissionType::Item)
            .unwrap();
        info.set_data_type(DataType::Output, RelativeSide::Front);
        info.set_data_type(DataType::InputOutput, RelativeSide::Top);
        info.set_ejecting(true);
    }
    from.ejector_mut().set_output_color(Some(4));
    from.ejector_mut().set_strict_input(true);
    let card = from.write_configuration_data();
    assert_eq!(card.control_type, Some(RedstoneControl::Low));
    assert!(card.config.is_some() && card.ejector.is_some());

    let mut to = common::chamber("clay");
    to.set_facing(Face::South);
    to.set_configuration_data(&card);
    assert_eq!(to.tile().control_type(), RedstoneControl::Low);
    assert_eq!(to.side_config().serialize(), from.side_config().serialize());
    assert_eq!(to.ejector().serialize(), from.ejector().serialize());
    // A configuração é relativa: a frente é a frente, qualquer que seja o bloco.
    assert_eq!(
        item_info(&to).data_type(RelativeSide::Front),
        DataType::Output
    );
    assert_eq!(
        to.side_config()
            .slot_info(TransmissionType::Item, Face::South, Some(Face::South))
            .map(|info| info.can_output),
        Some(true)
    );
}

#[test]
fn a_default_ejector_leaves_no_child_in_the_card() {
    let card = placed().write_configuration_data();
    assert!(card.ejector.is_none());
    assert_eq!(card.control_type, Some(RedstoneControl::Disabled));
}

#[test]
fn reading_a_card_without_a_child_keeps_that_component() {
    let mut to = placed();
    to.ejector_mut().set_output_color(Some(9));
    to.side_config_mut()
        .config_mut(TransmissionType::Item)
        .unwrap()
        .set_data_type(DataType::Output, RelativeSide::Left);
    to.tile_mut().set_control_type(RedstoneControl::High);
    // Sem `component_config` nem `component_ejector`: os dois ficam, mas o modo de redstone, que falta, volta a `DISABLED`.
    to.set_configuration_data(&ConfigurationData::default());
    assert_eq!(to.tile().control_type(), RedstoneControl::Disabled);
    assert_eq!(
        item_info(&to).data_type(RelativeSide::Left),
        DataType::Output
    );
    assert_eq!(to.ejector().output_color(), Some(9));
}

/// Os casos do cartão real do original (`config_lados_card`): o que `writeConfigurationData` entrega e o que `setConfigurationData` faz
/// num alvo com outro estado, com os dados cortados de quatro jeitos.
#[test]
fn the_card_cases_match_the_original() {
    use common::side::{born, config_nbt_from, ejector_nbt_from, load_state, state_of};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/config_lados_card.jsonl");
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &serde_json::Value| line.get("meta").is_none())
        .collect();
    let (mut written, mut cases) = (0, 0);
    for line in &lines {
        let Some(name) = line["machine"].as_str() else {
            continue;
        };
        if line["step"] == "source" {
            let mut from = born(name);
            load_state(&mut from, &line["source"]);
            let card = from.write_configuration_data();
            let direct = &line["write_direct"]["json"];
            assert_eq!(
                card.config,
                Some(config_nbt_from(&direct["component_config"])),
                "{name}: component_config"
            );
            assert_eq!(
                card.ejector,
                direct.get("component_ejector").map(ejector_nbt_from),
                "{name}: component_ejector"
            );
            assert_eq!(
                card.control_type.map(|c| c.serialized_name()),
                direct["control_type"].as_str(),
                "{name}: modo de redstone"
            );
            written += 1;
            continue;
        }
        let Some(case) = line["case"].as_str() else {
            continue;
        };
        // A origem é a fonte da máquina, e o alvo o `before` do caso. O cartão leva os dados cortados como o caso manda.
        let source = lines
            .iter()
            .find(|l| l["machine"] == name && l["step"] == "source")
            .unwrap();
        let mut from = born(name);
        load_state(&mut from, &source["source"]);
        let mut card = from.write_configuration_data();
        match case {
            "full" => {}
            "no_ejector" => card.ejector = None,
            "no_config" => card.config = None,
            "no_control_type" => card.control_type = None,
            // O cartão sem o filho `config` nem chama `setConfigurationData`: o alvo fica como está.
            "no_config_child" => {}
            other => panic!("caso desconhecido: {other}"),
        }
        let mut to = born(name);
        load_state(&mut to, &line["before"]);
        // `load_state` põe o modo de redstone e o ejetor do `before`, como o Java os tinha antes do cartão.
        if case != "no_config_child" {
            to.set_configuration_data(&card);
        }
        let after = state_of(&to);
        let mut want = line["after"].clone();
        want["facing"] = after["facing"].clone();
        assert_eq!(after, want, "{name} {case}");
        cases += 1;
    }
    assert!(written >= 4 && cases >= 20, "{written} {cases}");
}
