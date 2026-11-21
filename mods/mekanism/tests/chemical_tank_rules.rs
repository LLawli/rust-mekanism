//! O que o mod dá ao Chemical Tank da `machinery_basis`: os atributos dos químicos (os mapas de dados `chemical_attribute_*` do original, copiados para o conteúdo) e o
//! tanque como um tubo vizinho o vê (`TileEntityMekanism.getCapability` com a configuração de lados e o `ChemicalAttributeValidator` de `ChemicalTankChemicalTank.create`).
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::chemical_tank::{ChemicalTankConfig, ChemicalTankTile};
use machinery_basis_core::sides::Face;
use mekanism_rules::{
    CHEMICAL_ATTRIBUTE_FOLDER, DUMP_EXCESS_KEEP_RATIO, chemical_attribute_maps,
    chemical_attributes_of, chemical_tank_default, chemical_tank_faces, chemical_tank_view,
};
use serde_json::Value;
use std::rc::Rc;
use std::sync::Arc;

/// Os quatro arquivos do conteúdo, com o id que o jogo lhes dá.
fn files() -> Vec<(String, Value)> {
    let folder = format!(
        "{}/content/data/mekanism/{CHEMICAL_ATTRIBUTE_FOLDER}",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        out.push((
            format!("mekanism:{name}"),
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        ));
    }
    assert_eq!(out.len(), 4, "os quatro mapas de dados dos químicos");
    out
}

fn tile(creative: bool, radiation: bool) -> ChemicalTankTile {
    let attributes = chemical_attributes_of(Arc::new(chemical_attribute_maps(&files(), radiation)));
    let mut tile = ChemicalTankTile::new(ChemicalTankConfig {
        capacity: 64_000,
        rate: 1_000,
        creative,
        dump_excess_keep_ratio: DUMP_EXCESS_KEEP_RATIO,
        attributes,
        items: Rc::new(|_| None),
    });
    tile.set_facing(Face::North);
    tile.apply_item_side_config(Some(&chemical_tank_default()), None, false);
    tile
}

#[test]
fn the_data_maps_give_each_chemical_its_attributes_and_only_radioactivity_asks_for_validation() {
    let map = chemical_attribute_maps(&files(), true);
    assert_eq!(map["mekanism:hydrogen"], vec![("fuel".to_owned(), false)]);
    assert_eq!(map["mekanism:ethene"], vec![("fuel".to_owned(), false)]);
    for waste in [
        "mekanism:nuclear_waste",
        "mekanism:plutonium",
        "mekanism:polonium",
        "mekanism:spent_nuclear_waste",
    ] {
        assert_eq!(
            map[waste],
            vec![("radioactivity".to_owned(), true)],
            "{waste}"
        );
    }
    assert!(
        map.values()
            .flatten()
            .all(|(kind, needs)| *needs == (kind == "radioactivity"))
    );
    assert!(
        map.values()
            .flatten()
            .any(|(kind, _)| kind == "cooled_coolant")
    );
    assert!(
        map.values()
            .flatten()
            .any(|(kind, _)| kind == "heated_coolant")
    );
    assert!(!map.contains_key("mekanism:oxygen"));
    // `IRadiationManager.isRadiationEnabled` falso: nada pede validação.
    let off = chemical_attribute_maps(&files(), false);
    assert_eq!(
        off["mekanism:polonium"],
        vec![("radioactivity".to_owned(), false)]
    );
}

#[test]
fn remove_and_tag_keys_stay_out_of_the_map() {
    let files = vec![
        (
            "mekanism:chemical_attribute_fuel".to_owned(),
            serde_json::json!({"values": {"mekanism:hydrogen": {}, "#mekanism:some_tag": {}, "mekanism:ethene": {}}, "remove": ["mekanism:ethene"]}),
        ),
        (
            "mekanism:other_map".to_owned(),
            serde_json::json!({"values": {"mekanism:oxygen": {}}}),
        ),
    ];
    let map = chemical_attribute_maps(&files, true);
    assert_eq!(map.len(), 1);
    assert!(map.contains_key("mekanism:hydrogen"));
}

#[test]
fn a_tube_sees_the_tank_by_the_side_configuration_and_the_attribute_validator() {
    let hydrogen = ChemicalResource::new("mekanism:hydrogen");
    let polonium = ChemicalResource::new("mekanism:polonium");
    let normal = tile(false, true);
    // O padrão do item: a frente (norte) só sai, o resto só entra.
    let front = chemical_tank_view(&normal, Face::North).unwrap();
    assert!((!front.can_insert) && front.can_extract);
    let back = chemical_tank_view(&normal, Face::South).unwrap();
    assert!(back.can_insert && !back.can_extract);
    assert_eq!(back.capacity, 64_000);
    assert!(!back.creative);
    // `ChemicalAttributeValidator.DEFAULT`: o que pede validação não entra; `ALWAYS_ALLOW` no criativo.
    assert!((back.accepts)(&hydrogen));
    assert!(!(back.accepts)(&polonium));
    let creative = tile(true, true);
    let seen = chemical_tank_view(&creative, Face::South).unwrap();
    assert!(seen.creative && (seen.accepts)(&polonium));
    // Com a radiação desligada o tanque comum aceita o radioativo.
    assert!((chemical_tank_view(&tile(false, false), Face::South)
        .unwrap()
        .accepts)(&polonium));
    assert_eq!(
        chemical_tank_faces(&normal)[Face::North as usize],
        Some((false, true))
    );
    assert_eq!(
        chemical_tank_faces(&normal)[Face::Up as usize],
        Some((true, false))
    );
    // Um tile sem configuração (posto por comando) não tem capability em face nenhuma.
    let mut bare = tile(false, true);
    for kind in [machinery_basis_core::side_config::TransmissionType::Chemical] {
        let info = bare.side_config_mut().config_mut(kind).unwrap();
        for side in machinery_basis_core::side_config::RelativeSide::VALUES {
            info.set_data_type(machinery_basis_core::side_config::DataType::None, side);
        }
    }
    assert!(chemical_tank_view(&bare, Face::South).is_none());
    assert_eq!(chemical_tank_faces(&bare), [None; 6]);
}

/// `tank.capacityAsLong(resource)` na tela (`GuiChemicalTank.java:49`): o tanque comum com um químico radioativo (posto por comando: ele o recusa) tem capacidade
/// zero para ele, e a barra enche (`MathUtils.divideToLevel` com zero); o criativo e os outros químicos seguem a capacidade do tier. Medido no cliente do original
/// com polônio num tanque básico: `5,000/0` e a barra cheia.
#[test]
fn a_tank_shows_no_capacity_for_a_chemical_it_refuses() {
    use mekanism_rules::{chemical_tank_bar_level, chemical_tank_shown_capacity};
    let attributes = chemical_attributes_of(Arc::new(chemical_attribute_maps(&files(), true)));
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:polonium", &attributes),
        0
    );
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:hydrogen", &attributes),
        64_000
    );
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "", &attributes),
        64_000
    );
    assert_eq!(
        chemical_tank_shown_capacity(i64::MAX, true, "mekanism:polonium", &attributes),
        i64::MAX
    );
    assert_eq!(chemical_tank_bar_level(5_000, 0), 1.0);
    // Com a radiação desligada o atributo não pede validação.
    let off = chemical_attributes_of(Arc::new(chemical_attribute_maps(&files(), false)));
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:polonium", &off),
        64_000
    );
}
