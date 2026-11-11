//! Os pacotes das janelas de configuração de lados e de transporte sobre uma máquina de verdade (a Enrichment Chamber com o padrão do
//! item): o que `PacketSideData`, `PacketEjectConfiguration`, `PacketBatchConfiguration`, `PacketEjectColor`, `PacketInputColor` e o
//! `STRICT_INPUT` fazem, e o que eles deixam no NBT (`serialize`) que a tela relê.
mod common;

use machinery_basis_core::config_window::{ClickType, ConfigPacket};
use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::side_config::{DataType, RelativeSide, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::electric_machine_default;

fn placed() -> ProgressMachine {
    let mut machine = common::chamber("clay");
    machine.set_facing(Face::East);
    machine.apply_item_side_config(Some(&electric_machine_default()), Some(&AttachedEjector::DEFAULT), false);
    machine
}

fn send(machine: &mut ProgressMachine, packet: ConfigPacket) -> bool {
    let facing = machine.facing();
    let (config, ejector) = machine.side_parts_mut();
    packet.apply(config, ejector, facing)
}

fn data(machine: &ProgressMachine, side: RelativeSide) -> DataType {
    machine.side_config().data_type(TransmissionType::Item, side).unwrap()
}

#[test]
fn the_left_click_walks_the_supported_types_and_the_right_click_walks_back() {
    let mut machine = placed();
    // O padrão do item: a frente é entrada, a direita é saída.
    assert_eq!(data(&machine, RelativeSide::Front), DataType::Input);
    assert_eq!(data(&machine, RelativeSide::Right), DataType::Output);
    let click = |click, side| ConfigPacket::SideData { click, side, transmission: TransmissionType::Item };
    assert!(send(&mut machine, click(ClickType::Left, RelativeSide::Front)));
    assert_eq!(data(&machine, RelativeSide::Front), DataType::Output);
    assert!(send(&mut machine, click(ClickType::Left, RelativeSide::Front)));
    assert_eq!(data(&machine, RelativeSide::Front), DataType::InputOutput);
    assert!(send(&mut machine, click(ClickType::Right, RelativeSide::Front)));
    assert_eq!(data(&machine, RelativeSide::Front), DataType::Output);
    assert!(send(&mut machine, click(ClickType::ShiftLeft, RelativeSide::Front)));
    assert_eq!(data(&machine, RelativeSide::Front), DataType::None);
    // Shift em um lado que já é NONE não muda nada.
    assert!(!send(&mut machine, click(ClickType::ShiftLeft, RelativeSide::Front)));
    // Cada mudança avisou a face do mundo (a frente é leste).
    let changes = machine.take_side_changes();
    assert_eq!(changes.len(), 4);
    assert!(changes.iter().all(|change| change.direction == Face::East));
}

#[test]
fn the_batch_sets_every_side_of_the_type_or_of_all_types() {
    let mut machine = placed();
    let batch = |transmission, target| ConfigPacket::Batch { transmission, target };
    assert!(send(&mut machine, batch(Some(TransmissionType::Item), DataType::None)));
    for side in RelativeSide::VALUES {
        assert_eq!(data(&machine, side), DataType::None);
    }
    // Energia: o tipo da energia ficou como estava.
    assert_eq!(
        machine.side_config().data_type(TransmissionType::Energy, RelativeSide::Back),
        Some(DataType::Input)
    );
    assert!(send(&mut machine, batch(None, DataType::None)));
    assert_eq!(
        machine.side_config().data_type(TransmissionType::Energy, RelativeSide::Back),
        Some(DataType::None)
    );
    // Um DataType que o tipo não suporta (ENERGY na lista de itens... é suportado só em itens que o têm).
    assert!(!send(&mut machine, batch(Some(TransmissionType::Energy), DataType::Extra)));
}

#[test]
fn eject_and_strict_input_toggle_and_the_colors_cycle() {
    let mut machine = placed();
    let eject = ConfigPacket::Eject { transmission: TransmissionType::Item };
    send(&mut machine, eject);
    assert!(machine.side_config().config(TransmissionType::Item).unwrap().is_ejecting());
    send(&mut machine, eject);
    assert!(!machine.side_config().config(TransmissionType::Item).unwrap().is_ejecting());

    send(&mut machine, ConfigPacket::StrictInput);
    assert!(machine.ejector().has_strict_input());
    assert_eq!(machine.ejector().serialize().strict_input, Some(true));

    let color = |click| ConfigPacket::EjectColor { click };
    send(&mut machine, color(ClickType::Right));
    assert_eq!(machine.ejector().output_color(), Some(17));
    send(&mut machine, color(ClickType::Left));
    assert_eq!(machine.ejector().output_color(), None);
    send(&mut machine, color(ClickType::Left));
    assert_eq!(machine.ejector().output_color(), Some(0));
    send(&mut machine, color(ClickType::ShiftLeft));
    assert_eq!(machine.ejector().output_color(), None);

    let input = ConfigPacket::InputColor { click: ClickType::Left, side: RelativeSide::Left };
    send(&mut machine, input);
    send(&mut machine, input);
    assert_eq!(machine.ejector().input_color(RelativeSide::Left), Some(1));
    assert_eq!(machine.ejector().serialize().input_color.unwrap()[RelativeSide::Left.ordinal()], 1);
}

#[test]
fn a_view_built_from_the_nbt_data_shows_the_saved_sides() {
    use machinery_basis_core::ejector::TileComponentEjector;
    use machinery_basis_core::side_config::{ConfigData, TileComponentConfig};
    // O que `config_view` faz no glue: a configuração da máquina e, por cima, o `component_config` do NBT.
    let spec = mekanism_rules::electric_side_config();
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    let mut ejector = TileComponentEjector::new();
    ejector.set_output_data(&config, &spec.eject);
    let mut data = ConfigData::default();
    data.config[TransmissionType::Item.ordinal()] = Some(vec![1, 1, 4, 8, 1, 1]);
    data.eject[TransmissionType::Item.ordinal()] = Some(true);
    config.deserialize(&data, Face::North, false);
    assert_eq!(config.data_type(TransmissionType::Item, RelativeSide::Right), Some(DataType::Output));
    assert_eq!(config.data_type(TransmissionType::Item, RelativeSide::Back), Some(DataType::Energy));
    assert!(config.config(TransmissionType::Item).unwrap().is_ejecting());
    let _ = ejector;
}
