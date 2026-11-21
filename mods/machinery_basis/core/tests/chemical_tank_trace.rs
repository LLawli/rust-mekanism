//! Repete, tick a tick, os rastros `tank_chemical_*` do original (`TankTraceTest`): o Chemical Tank com o padrão do item (a frente
//! `OUTPUT`, o resto `INPUT`, ejetando), os modos de descarte dos quatro tiers, o ejetor para os tanques vizinhos, o que cada face
//! aceita e entrega pela capability, o comparador, e o tanque quebrado e reposto pelo item.
//!
//! As ações de cada tick (`marks`) rodam antes do tick dos tiles, e a linha é o estado depois deles.
use machinery_basis_core::chemical::{ChemicalAttributes, ChemicalResource};
use machinery_basis_core::chemical_tank::{
    ChemicalNeighbours, ChemicalTankConfig, ChemicalTankItem, ChemicalTankTile,
};
use machinery_basis_core::component_container::{
    AttachedCodec, ComponentBackedHandler, ComponentTankSpec, ComponentTankSpecOf,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::ejector::AttachedEjector;
use machinery_basis_core::handler_slot::ResourceContainer;
use machinery_basis_core::item_access::{ItemAccess, ItemHandlerLookup, ItemResourceHandler};
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::side_config::{
    AttachedSideConfig, DataType, RelativeSide, TransmissionType,
};
use machinery_basis_core::sides::Face;
use machinery_basis_core::transaction::Transaction;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

fn read(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn chemical(name: &str) -> ChemicalResource {
    ChemicalResource::new(&format!("mekanism:{name}"))
}

fn face(name: &str) -> Face {
    [
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ]
    .into_iter()
    .find(|face| face.name() == name)
    .unwrap()
}

/// O componente do item do tanque (`mekanism:chemicals`), aqui como `químico|mB`. Vazio é o padrão do item: o patch não o leva.
const CHEMICALS: &str = "mekanism:chemicals";

/// `ChemicalTankTier`: a capacidade e a taxa.
fn tier_of(id: &str) -> Option<(u64, i32)> {
    let tier = id
        .strip_prefix("mekanism:")?
        .strip_suffix("_chemical_tank")?;
    Some(match tier {
        "basic" => (64_000, 1_000),
        "advanced" => (256_000, 16_000),
        "elite" => (1_024_000, 128_000),
        "ultimate" => (8_192_000, 512_000),
        "creative" => (i64::MAX as u64, i32::MAX),
        other => panic!("tier desconhecido: {other}"),
    })
}

struct TankItems;

impl AttachedCodec<ChemicalResource> for TankItems {
    fn contents(&self, item: &ItemResource) -> StackContents<ChemicalResource> {
        match item.components.get(CHEMICALS) {
            Some(text) => {
                let (id, amount) = text.split_once('|').unwrap();
                StackContents::new(Some(ChemicalResource::new(id)), amount.parse().unwrap())
            }
            None => StackContents::default(),
        }
    }

    fn with_contents(
        &self,
        item: &ItemResource,
        contents: &StackContents<ChemicalResource>,
    ) -> Option<ItemResource> {
        let components = match &contents.resource {
            Some(resource) => Components::new([(
                CHEMICALS,
                format!("{}|{}", resource.id, contents.amount).as_str(),
            )]),
            None => Components::none(),
        };
        Some(ItemResource::with_components(
            &item.id,
            item.max_stack_size,
            components,
        ))
    }
}

/// `item`, ou `ctank:<tier>:<químico>:<mB>:<quantidade>` (o item do Chemical Tank).
fn stack_of(spec: &str) -> (ItemResource, u32) {
    let part: Vec<&str> = spec.split(':').collect();
    if part[0] != "ctank" {
        return (ItemResource::new(&format!("minecraft:{}", part[0]), 64), 1);
    }
    let empty = ItemResource::new(&format!("mekanism:{}_chemical_tank", part[1]), 64);
    let amount: u64 = part[3].parse().unwrap();
    let held = TankItems
        .with_contents(&empty, &StackContents::new(Some(chemical(part[2])), amount))
        .unwrap();
    (held, part[4].parse().unwrap())
}

fn lookup(clock: Rc<Cell<u64>>) -> ItemHandlerLookup<ChemicalResource> {
    let codec: Rc<dyn AttachedCodec<ChemicalResource>> = Rc::new(TankItems);
    let spec_of: ComponentTankSpecOf<ChemicalResource> = Rc::new(|item: &ItemResource| {
        let (capacity, rate) = tier_of(&item.id).unwrap();
        ComponentTankSpec::open(capacity, rate, item.id.contains("creative"))
    });
    Rc::new(move |access: &dyn ItemAccess| {
        let held = access.resource()?;
        tier_of(&held.id)?;
        let handler: Box<dyn ItemResourceHandler<ChemicalResource> + '_> = Box::new(
            ComponentBackedHandler::new(access, codec.clone(), spec_of.clone(), clock.clone()),
        );
        Some(handler)
    })
}

fn new_tile(meta: &Value, clock: &Rc<Cell<u64>>) -> ChemicalTankTile {
    let items = lookup(clock.clone());
    let attributes: ChemicalAttributes = Rc::new(|_| Vec::new());
    ChemicalTankTile::new(ChemicalTankConfig {
        capacity: meta["capacity"].as_u64().unwrap(),
        // O vizinho do rastro só diz a capacidade: a taxa dele (interna) não entra em nenhum cenário.
        rate: i32::try_from(meta["rate"].as_i64().unwrap_or(1000)).unwrap(),
        creative: meta["tier"] == "creative",
        dump_excess_keep_ratio: 0.85,
        attributes,
        items,
    })
}

/// O padrão do item do tanque (`ItemBlockChemicalTank.SIDE_CONFIG`) ou o dos vizinhos do rastro: os lados de químico, e os de item
/// só no padrão do item (a frente `OUTPUT` e o resto `INPUT`, como os de químico).
fn set_all(
    tile: &mut ChemicalTankTile,
    front: DataType,
    rest: DataType,
    ejecting: bool,
    items: bool,
) {
    let kinds: &[TransmissionType] = if items {
        &[TransmissionType::Chemical, TransmissionType::Item]
    } else {
        &[TransmissionType::Chemical]
    };
    for &kind in kinds {
        let info = tile.side_config_mut().config_mut(kind).unwrap();
        for side in RelativeSide::VALUES {
            info.set_data_type(
                if side == RelativeSide::Front {
                    front
                } else {
                    rest
                },
                side,
            );
        }
    }
    let info = tile
        .side_config_mut()
        .config_mut(TransmissionType::Chemical)
        .unwrap();
    info.set_ejecting(ejecting);
}

/// A capability de químico de um tanque vizinho, pela face dele que encosta no tanque do cenário.
struct Receiver {
    tile: Rc<RefCell<ChemicalTankTile>>,
    face: Face,
}

impl ResourceInsert<ChemicalResource> for Receiver {
    fn insert(&self, resource: &ChemicalResource, amount: i32, transaction: &Transaction) -> i32 {
        self.tile
            .borrow()
            .handler_insert(self.face, resource, amount, transaction)
            .unwrap_or(0)
    }
}

struct Around(Vec<(Face, Rc<dyn ResourceInsert<ChemicalResource>>)>);

impl ChemicalNeighbours for Around {
    fn handler(&self, side: Face) -> Option<Rc<dyn ResourceInsert<ChemicalResource>>> {
        self.0
            .iter()
            .find(|(face, _)| *face == side)
            .map(|(_, handler)| handler.clone())
    }
}

type Held = (ChemicalTankItem, AttachedSideConfig, AttachedEjector);

fn set_chemical(tile: &ChemicalTankTile, name: &str, amount: i64) {
    if name == "none" {
        tile.tank().inner().set_contents(None, 0, None);
        return;
    }
    let resource = chemical(name);
    let capacity = tile.tank().inner().capacity(&resource);
    let value = match amount {
        -1 => capacity,
        -90 => capacity / 10 * 9 + 1,
        amount => u64::try_from(amount).unwrap(),
    };
    tile.tank()
        .inner()
        .set_contents(Some(resource), value, None);
}

fn drop_components(held: &Held) -> Value {
    let (item, sides, _) = held;
    let mut components = serde_json::Map::new();
    let mut config = serde_json::Map::new();
    for (kind, name) in [
        (TransmissionType::Chemical, "chemicals"),
        (TransmissionType::Item, "items"),
    ] {
        let info = &sides.config_info[&kind];
        let mut side = serde_json::Map::new();
        for relative in RelativeSide::VALUES {
            side.insert(
                relative.serialized_name().into(),
                Value::from(info.data_type(relative).serialized_name()),
            );
        }
        let mut entry = serde_json::Map::new();
        entry.insert("side".into(), Value::Object(side));
        if info.ejecting {
            entry.insert("eject".into(), Value::Bool(true));
        }
        config.insert(name.into(), Value::Object(entry));
    }
    components.insert(
        "mekanism:side_config".into(),
        serde_json::json!({ "config": config }),
    );
    if let Some(resource) = &item.chemical.resource {
        components.insert(
            "mekanism:chemicals".into(),
            serde_json::json!({ "containers": [{ "type": &*resource.id, "amount": item.chemical.amount }] }),
        );
    }
    components.insert(
        "mekanism:dump_mode".into(),
        Value::from(item.dumping.serialized_name()),
    );
    Value::Object(components)
}

/// O slot como o rastro o grava: `[item, quantidade, fluido do item, mB, químico do item, mB]`, por item.
fn assert_slot(slot: &InventorySlot, recorded: &Value, at: &str) {
    let contents = slot.contents();
    let inside = contents
        .resource
        .as_ref()
        .map_or_else(StackContents::default, |held| TankItems.contents(held));
    let ours = serde_json::json!([
        contents
            .resource
            .as_ref()
            .map_or(String::new(), |held| held.id.to_string()),
        contents.amount,
        "",
        0,
        inside
            .resource
            .map_or(String::new(), |resource| resource.id.to_string()),
        inside.amount,
    ]);
    assert_eq!(&ours, recorded, "{at}");
}

fn replay(name: &str) {
    let lines = read(&format!("tank_chemical_{name}.jsonl"));
    let meta = &lines[0]["meta"];
    let tank_meta = meta["tanks"][0].clone();
    let clock = Rc::new(Cell::new(0));
    let mut tile = Some(new_tile(&tank_meta, &clock));
    set_all(
        tile.as_mut().unwrap(),
        DataType::Output,
        DataType::Input,
        true,
        true,
    );
    let mut receivers: Vec<(Face, Rc<RefCell<ChemicalTankTile>>)> = Vec::new();
    for receiver in meta["receivers"].as_array().unwrap() {
        let mut other = new_tile(receiver, &clock);
        set_all(
            &mut other,
            DataType::InputOutput,
            DataType::InputOutput,
            false,
            false,
        );
        receivers.push((
            face(receiver["face"].as_str().unwrap()),
            Rc::new(RefCell::new(other)),
        ));
    }
    let around = Around(
        receivers
            .iter()
            .map(|(side, tile)| {
                let handler: Rc<dyn ResourceInsert<ChemicalResource>> = Rc::new(Receiver {
                    tile: tile.clone(),
                    face: side.opposite(),
                });
                (*side, handler)
            })
            .collect(),
    );
    let mut held: Option<Held> = None;
    let mut time = 0;
    let mut compared = 0;
    for line in &lines[1..] {
        let t = line["t"].as_i64().unwrap();
        let mut results: Vec<i64> = Vec::new();
        let mut drops: Vec<Held> = Vec::new();
        for mark in line["marks"].as_array().unwrap() {
            let part: Vec<&str> = mark.as_str().unwrap().split(' ').collect();
            match part[0] {
                "chem" => set_chemical(
                    tile.as_ref().unwrap(),
                    part[1],
                    part.get(2).map_or(0, |v| v.parse().unwrap()),
                ),
                "recv" => {
                    let (_, other) = receivers
                        .iter()
                        .find(|(side, _)| *side == face(part[1]))
                        .unwrap();
                    set_chemical(&other.borrow(), part[2], part[3].parse().unwrap());
                }
                "dump" => tile.as_mut().unwrap().next_mode(),
                "fillslot" | "drainslot" => {
                    let tile = tile.as_ref().unwrap();
                    let slot = if part[0] == "fillslot" {
                        tile.fill_slot().slot()
                    } else {
                        tile.drain_slot().slot()
                    };
                    let (held, count) = stack_of(part[1]);
                    slot.set_contents(Some(held), count, None);
                }
                "eject" => tile
                    .as_mut()
                    .unwrap()
                    .side_config_mut()
                    .config_mut(TransmissionType::Chemical)
                    .unwrap()
                    .set_ejecting(part[1] == "true"),
                "side" => {
                    let side = RelativeSide::from_serialized_name(part[1]).unwrap();
                    let data = DataType::from_serialized_name(part[2]).unwrap();
                    tile.as_mut()
                        .unwrap()
                        .side_config_mut()
                        .config_mut(TransmissionType::Chemical)
                        .unwrap()
                        .set_data_type(data, side);
                }
                "push" | "pull" => {
                    let tile = tile.as_ref().unwrap();
                    let resource = chemical(part[2]);
                    let amount: i32 = part[3].parse().unwrap();
                    let transaction = Transaction::open_root();
                    let moved = if part[0] == "push" {
                        tile.handler_insert(face(part[1]), &resource, amount, &transaction)
                    } else {
                        tile.handler_extract(face(part[1]), &resource, amount, &transaction)
                    };
                    transaction.commit();
                    results.push(moved.map_or(-1, i64::from));
                }
                "break" => {
                    let old = tile.take().unwrap();
                    let (sides, ejector) = old.collect_item_side_config();
                    let entry = (old.collect_item(), sides, ejector);
                    drops.push(entry.clone());
                    held = Some(entry);
                }
                "place" => {
                    let (item, sides, ejector) = held.take().unwrap();
                    let mut placed = new_tile(&tank_meta, &clock);
                    placed.apply_item_side_config(Some(&sides), Some(&ejector), true);
                    placed.apply_item(&item);
                    tile = Some(placed);
                }
                other => panic!("marca desconhecida: {other}"),
            }
        }
        time += 1;
        clock.set(time);
        if let Some(tile) = tile.as_mut() {
            tile.tick_server(time, &around);
        }
        for (_, other) in &receivers {
            other.borrow_mut().tick_server(time, &Around(Vec::new()));
        }
        let expected: Vec<i64> = line["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(
            results, expected,
            "{name} t={t}: o que cada push e pull moveu"
        );
        if let Some(recorded) = line.get("drops") {
            let recorded = recorded.as_array().unwrap();
            assert_eq!(recorded.len(), drops.len(), "{name} t={t}: quebras");
            for (recorded, ours) in recorded.iter().zip(&drops) {
                assert_eq!(
                    recorded["stacks"][0]["components"],
                    drop_components(ours),
                    "{name} t={t}: os componentes do item solto"
                );
            }
        }
        for (side, other) in &receivers {
            let other = other.borrow();
            let recorded = &line["receivers"][side.name()];
            let held = other
                .tank()
                .resource()
                .map_or(String::new(), |resource| resource.id.to_string());
            assert_eq!(
                held,
                recorded[0].as_str().unwrap(),
                "{name} t={t}: químico do vizinho {}",
                side.name()
            );
            assert_eq!(
                other.tank().amount(),
                recorded[1].as_u64().unwrap(),
                "{name} t={t}: vizinho {}",
                side.name()
            );
        }
        let recorded = &line["tank"];
        let Some(tile) = &tile else {
            assert!(recorded.is_null(), "{name} t={t}: o tanque devia existir");
            continue;
        };
        let at = format!("{name} t={t}");
        let stored = tile
            .tank()
            .resource()
            .map_or(String::new(), |resource| resource.id.to_string());
        assert_eq!(
            stored,
            recorded["chemical"].as_str().unwrap(),
            "{at}: químico"
        );
        assert_eq!(
            tile.tank().amount(),
            recorded["amount"].as_u64().unwrap(),
            "{at}: quantidade"
        );
        assert_eq!(
            i64::from(tile.tile().current_redstone_level()),
            recorded["comparator"].as_i64().unwrap(),
            "{at}: comparador"
        );
        assert_eq!(
            tile.dumping().serialized_name(),
            recorded["dumping"].as_str().unwrap(),
            "{at}: descarte"
        );
        let info = tile
            .side_config()
            .config(TransmissionType::Chemical)
            .unwrap();
        assert_eq!(
            info.is_ejecting(),
            recorded["ejecting"].as_bool().unwrap(),
            "{at}: ejetando"
        );
        for (side, recorded) in RelativeSide::VALUES
            .into_iter()
            .zip(recorded["sides"].as_array().unwrap())
        {
            assert_eq!(
                info.data_type(side).serialized_name(),
                recorded.as_str().unwrap(),
                "{at}: lado"
            );
        }
        for (key, slot) in [
            ("drain", tile.drain_slot().slot()),
            ("fill", tile.fill_slot().slot()),
        ] {
            assert_slot(slot, &recorded[key], &format!("{at}: slot {key}"));
        }
        compared += 1;
    }
    assert!(compared >= 96, "{name}: só {compared} estados comparados");
}

#[test]
fn the_dumping_modes_of_the_four_tiers_follow_the_original() {
    for tier in ["basic", "advanced", "elite", "ultimate"] {
        replay(&format!("dump_{tier}"));
    }
}

#[test]
fn the_ejector_follows_the_original() {
    replay("eject");
}

#[test]
fn the_tank_broken_and_placed_back_and_the_faces_follow_the_original() {
    replay("item");
}

#[test]
fn the_tank_items_in_the_two_slots_follow_the_original() {
    replay("slots");
}

#[test]
fn the_creative_tank_follows_the_original() {
    replay("creative");
}
