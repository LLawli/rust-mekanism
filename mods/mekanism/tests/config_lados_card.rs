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
    machine.side_config().config(TransmissionType::Item).unwrap()
}

#[test]
fn the_card_carries_the_redstone_mode_the_sides_and_the_ejector() {
    let mut from = placed();
    from.tile_mut().set_control_type(RedstoneControl::Low);
    {
        let info = from.side_config_mut().config_mut(TransmissionType::Item).unwrap();
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
    assert_eq!(item_info(&to).data_type(RelativeSide::Front), DataType::Output);
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
    assert_eq!(item_info(&to).data_type(RelativeSide::Left), DataType::Output);
    assert_eq!(to.ejector().output_color(), Some(9));
}
