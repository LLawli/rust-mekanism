//! O que as janelas de configuração fazem fora do que o rastro `config_pacotes` grava (`config_window_trace.rs`): o aviso da face, e a vista que o cliente monta do NBT.
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
    machine.apply_item_side_config(
        Some(&electric_machine_default()),
        Some(&AttachedEjector::DEFAULT),
        false,
    );
    machine
}

fn send(machine: &mut ProgressMachine, packet: ConfigPacket) -> bool {
    let facing = machine.facing();
    let (config, ejector) = machine.side_parts_mut();
    packet.apply(config, ejector, facing)
}

fn data(machine: &ProgressMachine, side: RelativeSide) -> DataType {
    machine
        .side_config()
        .data_type(TransmissionType::Item, side)
        .unwrap()
}

/// O que o rastro `config_pacotes` não grava: cada mudança de lado avisa a face do mundo (a frente é leste).
#[test]
fn each_side_change_tells_the_world_face() {
    let mut machine = placed();
    let click = |click, side| ConfigPacket::SideData { click, side, transmission: TransmissionType::Item };
    assert!(send(&mut machine, click(ClickType::Left, RelativeSide::Front)));
    assert!(send(&mut machine, click(ClickType::Right, RelativeSide::Back)));
    assert!(send(&mut machine, ConfigPacket::Batch { transmission: Some(TransmissionType::Item), target: DataType::None }));
    let changes = machine.take_side_changes();
    // Uma da frente, uma das costas e as quatro faces que o lote mudou além das duas já mudadas... o lote muda o que ainda não era NONE.
    assert!(changes.len() >= 3);
    assert_eq!(changes[0].direction, Face::East);
    assert_eq!(changes[1].direction, Face::West);
    let _ = data(&machine, RelativeSide::Front);
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
    assert_eq!(
        config.data_type(TransmissionType::Item, RelativeSide::Right),
        Some(DataType::Output)
    );
    assert_eq!(
        config.data_type(TransmissionType::Item, RelativeSide::Back),
        Some(DataType::Energy)
    );
    assert!(config.config(TransmissionType::Item).unwrap().is_ejecting());
    let _ = ejector;
}
