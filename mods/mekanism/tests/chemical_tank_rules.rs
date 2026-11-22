//! O que o mod dá ao Chemical Tank da `machinery_basis`: os atributos dos químicos (os mapas de dados `chemical_attribute_*` do original, copiados para o conteúdo) e o
//! tanque como um tubo vizinho o vê (`TileEntityMekanism.getCapability` com a configuração de lados e o `ChemicalAttributeValidator` de `ChemicalTankChemicalTank.create`).
use machinery_basis_core::chemical::ChemicalAttributes;
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::chemical_tank::{ChemicalTankConfig, ChemicalTankTile};
use machinery_basis_core::data_map::DataMapFile;
use machinery_basis_core::sides::Face;
use mekanism_rules::{
    CHEMICAL_ATTRIBUTE_FOLDER, ChemicalAttributeMap, DUMP_EXCESS_KEEP_RATIO, DataMapStack,
    chemical_attribute_data, chemical_attribute_maps, chemical_attributes_of,
    chemical_tank_default, chemical_tank_faces, chemical_tank_view, data_file_id,
};
use serde_json::Value;
use std::cell::Cell;
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

/// A pilha de quem só tem um arquivo por id, já lido.
fn data_map_stack(files: &[(String, Value)]) -> DataMapStack {
    files
        .iter()
        .map(|(id, json)| (id.clone(), vec![DataMapFile::of_value(json).unwrap()]))
        .collect()
}

/// Sem tags: a datagen do original só tem ids.
fn no_tags(_: &str) -> Vec<String> {
    Vec::new()
}

fn map() -> ChemicalAttributeMap {
    chemical_attribute_maps(&data_map_stack(&files()), &no_tags)
}

fn attributes(radiation: bool) -> ChemicalAttributes {
    chemical_attributes_of(Arc::new(map()), Rc::new(move || radiation))
}

fn tile(creative: bool, radiation: bool) -> ChemicalTankTile {
    let attributes = attributes(radiation);
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
    let map = map();
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
    let polonium = ChemicalResource::new("mekanism:polonium");
    assert!(
        attributes(true)(&polonium)
            .iter()
            .any(|a| a.needs_validation)
    );
    assert!(
        attributes(false)(&polonium)
            .iter()
            .all(|a| !a.needs_validation)
    );
}

/// `ChemicalRadioactivity.needsValidation` pergunta à configuração a cada vez (`RadiationManager.java:60`): a opção que muda vale para os atributos já entregues.
#[test]
fn the_radiation_option_is_asked_at_each_query() {
    let enabled = Rc::new(Cell::new(true));
    let asked = enabled.clone();
    let attributes = chemical_attributes_of(Arc::new(map()), Rc::new(move || asked.get()));
    let polonium = ChemicalResource::new("mekanism:polonium");
    let hydrogen = ChemicalResource::new("mekanism:hydrogen");
    assert!(attributes(&polonium)[0].needs_validation);
    enabled.set(false);
    assert!(!attributes(&polonium)[0].needs_validation);
    enabled.set(true);
    assert!(attributes(&polonium)[0].needs_validation);
    // O combustível nunca pede, com a radiação ligada ou não.
    assert!(!attributes(&hydrogen)[0].needs_validation);
}

fn file(text: &str) -> DataMapFile {
    DataMapFile::parse(text).unwrap()
}

/// `DataMapLoader.buildDataMap`: `remove` tira, a chave de tag vale para cada membro da tag, e um mapa de outro nome não é atributo.
#[test]
fn remove_takes_out_and_a_tag_key_gives_the_attribute_to_each_member() {
    let members = |tag: &str| match tag {
        "mekanism:some_tag" => vec!["mekanism:oxygen".to_owned(), "mekanism:ethene".to_owned()],
        _ => Vec::new(),
    };
    let stack: DataMapStack = vec![
        (
            "mekanism:chemical_attribute_fuel".to_owned(),
            vec![file(
                r##"{"values": {"mekanism:hydrogen": {"energy": 1, "maxBurnPerTick": 1}, "#mekanism:some_tag": {"energy": 7, "maxBurnPerTick": 2}, "mekanism:ethene": {"energy": 3, "maxBurnPerTick": 4}}, "remove": ["mekanism:hydrogen"]}"##,
            )],
        ),
        (
            "mekanism:chemical_attribute_radioactivity".to_owned(),
            vec![file(
                r##"{"values": {"#mekanism:some_tag": 0.5, "#mekanism:no_such_tag": 9.0}}"##,
            )],
        ),
        (
            "mekanism:other_map".to_owned(),
            vec![file(r#"{"values": {"mekanism:water_vapor": {}}}"#)],
        ),
    ];
    let map = chemical_attribute_maps(&stack, &members);
    assert_eq!(map.len(), 2);
    assert_eq!(
        map["mekanism:oxygen"],
        vec![
            ("fuel".to_owned(), false),
            ("radioactivity".to_owned(), true)
        ]
    );
    assert_eq!(map["mekanism:ethene"].len(), 2);
    let data = chemical_attribute_data(&stack, &members);
    // O id depois da tag no documento fica com o valor dele; o membro que só a tag alcança, com o da tag.
    assert_eq!(data["mekanism:ethene"].fuel, Some((4, 3)));
    assert_eq!(data["mekanism:oxygen"].fuel, Some((2, 7)));
    assert_eq!(data["mekanism:oxygen"].radioactivity, Some(0.5));
    assert!(!data.contains_key("mekanism:hydrogen"));
    // Sem os membros (a tag vazia) só o id fica.
    let alone = chemical_attribute_maps(&stack, &no_tags);
    assert_eq!(alone.len(), 1);
    assert_eq!(alone["mekanism:ethene"], vec![("fuel".to_owned(), false)]);
}

/// Os arquivos de um id em vários pacotes se empilham (`listMatchingResourceStacks`): o de cima acrescenta, troca e tira; com `replace`, larga os de baixo.
#[test]
fn the_files_of_every_pack_stack_as_the_loader_stacks_them() {
    let bottom = file(
        r#"{"values": {"mekanism:hydrogen": {"energy": 1, "maxBurnPerTick": 1}, "mekanism:ethene": {"energy": 3, "maxBurnPerTick": 4}}}"#,
    );
    let top = file(
        r#"{"values": {"mekanism:oxygen": {"energy": 5, "maxBurnPerTick": 6}, "mekanism:ethene": {"energy": 8, "maxBurnPerTick": 9}}, "remove": ["mekanism:hydrogen"]}"#,
    );
    let id = "mekanism:chemical_attribute_fuel".to_owned();
    let data = chemical_attribute_data(
        &vec![(id.clone(), vec![bottom.clone(), top.clone()])],
        &no_tags,
    );
    assert_eq!(data.len(), 2);
    assert_eq!(data["mekanism:ethene"].fuel, Some((9, 8)));
    assert_eq!(data["mekanism:oxygen"].fuel, Some((6, 5)));
    // Só o de cima, como o gancho do Horizon entrega sem a injeção: o hidrogênio do de baixo nem chega a existir, e o eteno do de baixo se perde.
    let data = chemical_attribute_data(&vec![(id.clone(), vec![top.clone()])], &no_tags);
    assert_eq!(data.len(), 2);
    let replacing = DataMapFile {
        replace: true,
        ..top
    };
    let data = chemical_attribute_data(&vec![(id, vec![bottom, replacing])], &no_tags);
    assert_eq!(data.len(), 2);
    assert!(!data.contains_key("mekanism:hydrogen"));
}

#[test]
fn a_pack_file_gets_the_id_the_loader_gives_it() {
    let folder = CHEMICAL_ATTRIBUTE_FOLDER;
    assert_eq!(
        data_file_id(
            "data/mekanism/data_maps/mekanism/chemical/chemical_attribute_fuel.json",
            folder
        )
        .as_deref(),
        Some("mekanism:chemical_attribute_fuel")
    );
    assert_eq!(
        data_file_id("data/other/data_maps/mekanism/chemical/sub/x.json", folder).as_deref(),
        Some("other:sub/x")
    );
    assert_eq!(
        data_file_id("data/mekanism/data_maps/mekanism/chemical/x.txt", folder),
        None
    );
    assert_eq!(
        data_file_id("data/mekanism/data_maps/mekanism/chemicals/x.json", folder),
        None
    );
    assert_eq!(
        data_file_id("data/mekanism/data_maps/mekanism/chemical/.json", folder),
        None
    );
    assert_eq!(
        data_file_id("assets/mekanism/data_maps/mekanism/chemical/x.json", folder),
        None
    );
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
    let on = attributes(true);
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:polonium", &on),
        0
    );
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:hydrogen", &on),
        64_000
    );
    assert_eq!(chemical_tank_shown_capacity(64_000, false, "", &on), 64_000);
    assert_eq!(
        chemical_tank_shown_capacity(i64::MAX, true, "mekanism:polonium", &on),
        i64::MAX
    );
    assert_eq!(chemical_tank_bar_level(5_000, 0), 1.0);
    // Com a radiação desligada o atributo não pede validação.
    let off = attributes(false);
    assert_eq!(
        chemical_tank_shown_capacity(64_000, false, "mekanism:polonium", &off),
        64_000
    );
}

use machinery_basis_core::chemical_tank::NoChemicalNeighbours as Alone;

/// `ChemicalUtils.dump` em `DUMPING_EXCESS` lê `MekanismConfig.general.dumpExcessKeepRatio` a cada tick (`ChemicalUtils.java:95`): o alvo é a capacidade vezes a
/// opção de agora, e por tick sai o que passa dele, até a taxa do tier.
#[test]
fn the_keep_ratio_of_dumping_excess_is_the_one_of_each_tick() {
    use machinery_basis_core::chemical_tank::GasMode;
    let hydrogen = ChemicalResource::new("mekanism:hydrogen");
    let mut tile = tile(false, true);
    tile.tank()
        .inner()
        .set_contents(Some(hydrogen), 64_000, None);
    tile.set_dumping(GasMode::DumpingExcess);
    // 0,85 de 64.000 é 54.400: sai a taxa do tier, 1.000.
    tile.tick_server(1, &Alone);
    assert_eq!(tile.tank().amount(), 63_000);
    // 0,99 dá 63.360: nada passa.
    tile.set_dump_excess_keep_ratio(0.99);
    tile.tick_server(2, &Alone);
    assert_eq!(tile.tank().amount(), 63_000);
    // 0,984 dá 62.976: saem os 24 que passam.
    tile.set_dump_excess_keep_ratio(0.984);
    tile.tick_server(3, &Alone);
    assert_eq!(tile.tank().amount(), 62_976);
    // 0,001, o mínimo da opção: o alvo é 64, e a taxa segura a saída.
    tile.set_dump_excess_keep_ratio(0.001);
    tile.tick_server(4, &Alone);
    assert_eq!(tile.tank().amount(), 61_976);
}
