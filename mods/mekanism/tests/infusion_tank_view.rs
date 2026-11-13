//! O tanque de infusão do Metallurgic Infuser como um tubo pressurizado vizinho o vê (`TileEntityMekanism.getCapability` com a capability de químico): onde a configuração de lados
//! liga a capability (`isCapabilityDisabled`), se entra e se sai por cada lado (`ConfigHolder.canInsert` e `canExtract`), e o que o tanque aceita de fora
//! (`containsRecipeBA(inputSlot.resource(), chemical)`).
mod common;

use common::{infuser, item};
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::side_config::{ConfigData, DataType, SlotInfo, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::host_resource::HostHandler;
use mekanism_rules::{infusion_tank_faces, infusion_tank_view};

fn chemical(id: &str) -> ChemicalResource {
    ChemicalResource::new(id)
}

/// Um Infuser virado para o norte, com os seis lados do químico (frente, esquerda, direita, trás, cima, baixo) como pedido.
fn configured(sides: [DataType; 6]) -> ProgressMachine {
    let mut machine = infuser();
    machine.set_facing(Face::North);
    let mut data = ConfigData::default();
    data.config[TransmissionType::Chemical.ordinal()] =
        Some(sides.map(|side| side.ordinal() as i32).to_vec());
    machine.restore_side_config(Some(&data), None, false);
    machine
}

fn face_index(face: Face) -> usize {
    Face::ALL.iter().position(|known| *known == face).unwrap()
}

#[test]
fn a_side_with_no_data_type_has_no_capability_and_the_others_follow_the_slot_info() {
    // Virado para o norte: frente norte, esquerda leste, direita oeste, trás sul (`RelativeSide.fromDirections`).
    let machine = configured([
        DataType::Input,
        DataType::Input,
        DataType::Output,
        DataType::InputOutput,
        DataType::None,
        DataType::None,
    ]);
    let faces = infusion_tank_faces(&machine);
    assert_eq!(faces[face_index(Face::North)], Some((true, false)));
    assert_eq!(faces[face_index(Face::East)], Some((true, false)));
    assert_eq!(faces[face_index(Face::West)], Some((false, true)));
    assert_eq!(faces[face_index(Face::South)], Some((true, true)));
    assert_eq!(faces[face_index(Face::Up)], None);
    assert_eq!(faces[face_index(Face::Down)], None);
    assert!(infusion_tank_view(&machine, Face::Up).is_none());
    // A máquina posta por comando nasce com tudo em `NONE`: nenhum lado tem a capability.
    let fresh = infuser();
    assert!(infusion_tank_faces(&fresh).iter().all(Option::is_none));
}

#[test]
fn the_tank_takes_from_outside_only_what_a_recipe_uses_with_the_item_in_the_input_slot() {
    let machine = configured([DataType::Input; 6]);
    let view = infusion_tank_view(&machine, Face::North).unwrap();
    assert_eq!(view.capacity, 1_000);
    // Slot de entrada vazio: qualquer químico de alguma receita entra (`carbon`, `bio`), o que nenhuma usa não.
    assert!((view.accepts)(&chemical("carbon")));
    assert!((view.accepts)(&chemical("bio")));
    assert!(!(view.accepts)(&chemical("hydrogen")));
    // Com ferro no slot, só o que combina com ele (`iron_ingot` + `carbon`).
    machine
        .input()
        .set_contents(Some(item("iron_ingot")), 1, None);
    let view = infusion_tank_view(&machine, Face::North).unwrap();
    assert!((view.accepts)(&chemical("carbon")));
    assert!(!(view.accepts)(&chemical("bio")));
    // O espelho que a rede usa responde igual, dentro de uma transação, e só o conteúdo final volta ao tanque.
    let handler = HostHandler::new(view);
    let transaction = Transaction::open_root();
    assert_eq!(handler.insert(&chemical("bio"), 100, &transaction), 0);
    assert_eq!(handler.insert(&chemical("carbon"), 400, &transaction), 400);
    transaction.commit();
    let changed = handler.changed().expect("o espelho mudou");
    assert_eq!(changed.amount, 400);
    assert_eq!(machine.chemical_side().unwrap().tank.amount(), 0);
}

#[test]
fn a_tank_that_holds_one_chemical_refuses_another_and_an_output_side_lets_a_pulling_tube_take() {
    let machine = configured([
        DataType::Output,
        DataType::Input,
        DataType::Input,
        DataType::Input,
        DataType::Input,
        DataType::Input,
    ]);
    machine
        .chemical_side()
        .unwrap()
        .tank
        .set_contents(Some(chemical("carbon")), 300, None);
    let output = infusion_tank_view(&machine, Face::North).unwrap();
    assert!(!output.can_insert && output.can_extract);
    assert_eq!(output.contents.amount, 300);
    let handler = HostHandler::new(output);
    let transaction = Transaction::open_root();
    use machinery_basis_core::transmission::buffered_resource_network::ResourceExtract;
    assert_eq!(
        handler.extract(&chemical("carbon"), 1_000, &transaction),
        300
    );
    drop(transaction);
    let input = infusion_tank_view(&machine, Face::East).unwrap();
    assert!(input.can_insert && !input.can_extract);
    let handler = HostHandler::new(input);
    let transaction = Transaction::open_root();
    // Outro químico não entra num tanque que guarda `carbon`.
    assert_eq!(handler.insert(&chemical("bio"), 100, &transaction), 0);
    assert_eq!(handler.insert(&chemical("carbon"), 900, &transaction), 700);
}

#[test]
fn a_slot_info_that_neither_takes_nor_gives_leaves_the_capability_disabled() {
    // `TileComponentConfig.isCapabilityDisabled`: o `SlotInfo` do lado não está habilitado (`isEnabled` é `canInput || canOutput`), e a capability não existe ali.
    let mut machine = configured([DataType::None; 6]);
    machine
        .side_config_mut()
        .config_mut(TransmissionType::Chemical)
        .unwrap()
        .add_slot_info(
            DataType::None,
            SlotInfo::new(TransmissionType::Chemical, false, false, vec![0]),
        );
    assert!(
        machine
            .side_config()
            .slot_info(TransmissionType::Chemical, Face::North, Some(Face::North))
            .is_some()
    );
    assert!(infusion_tank_faces(&machine).iter().all(Option::is_none));
}
