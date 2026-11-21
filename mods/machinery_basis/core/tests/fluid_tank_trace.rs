//! Repete, tick a tick, os rastros `tank_fluid_*` do original (`TankTraceTest`): o Fluid Tank de cada tier numa coluna, com os dois
//! slots e os baldes, os três modos de edição, o empurrão para baixo de um tanque ativo, o que transborda para o tanque de cima, a
//! válvula do topo, o criativo, a escala, o comparador, e o tanque quebrado e reposto pelo item.
//!
//! As ações de cada tick (`marks`) rodam antes do tick dos tiles, que andam de baixo para cima, e a linha é o estado depois deles.
use machinery_basis_core::component_container::{
    AttachedCodec, ComponentBackedHandler, ComponentTankSpec, ComponentTankSpecOf,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{InventorySlot, ItemResource, SlotContents};
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::fluid_tank::{
    BelowHandler, FluidTankConfig, FluidTankItem, FluidTankTank, FluidTankTile,
};
use machinery_basis_core::handler_slot::ResourceContainer;
use machinery_basis_core::item_access::{
    AccessResourceHandler, ItemAccess, ItemHandlerLookup, ItemResourceHandler, PerItemRules,
};
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::transaction::Transaction;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

const BUCKET: &str = "minecraft:bucket";
const BUCKET_VOLUME: i32 = 1000;

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

fn item(id: &str) -> ItemResource {
    ItemResource::new(
        id,
        if id == BUCKET {
            16
        } else if id.ends_with("_bucket") {
            1
        } else {
            64
        },
    )
}

fn fluid(name: &str) -> FluidResource {
    FluidResource::new(&format!("minecraft:{name}"))
}

/// `BucketResourceHandler`: o balde de água e o de lava, e o balde vazio.
struct Buckets;

impl PerItemRules<FluidResource> for Buckets {
    fn resource_from(&self, item: &ItemResource) -> Option<FluidResource> {
        match &*item.id {
            "minecraft:water_bucket" => Some(fluid("water")),
            "minecraft:lava_bucket" => Some(fluid("lava")),
            _ => None,
        }
    }

    fn amount_from(&self, item: &ItemResource) -> i32 {
        if self.resource_from(item).is_some() {
            BUCKET_VOLUME
        } else {
            0
        }
    }

    fn capacity(&self, _resource: &FluidResource) -> i32 {
        BUCKET_VOLUME
    }

    fn update(
        &self,
        _item: &ItemResource,
        resource: &FluidResource,
        amount: i32,
    ) -> Option<ItemResource> {
        if amount == 0 {
            Some(item(BUCKET))
        } else if amount != BUCKET_VOLUME {
            None
        } else {
            Some(item(&format!("{}_bucket", resource.id)))
        }
    }
}

/// O componente do item do tanque (`mekanism:fluids`), aqui como `fluido|mB`. Vazio é o padrão do item: o patch não o leva.
const FLUIDS: &str = "mekanism:fluids";

/// `FluidTankTier`: a capacidade e a taxa.
fn tier_of(id: &str) -> Option<(u64, i32)> {
    let tier = id.strip_prefix("mekanism:")?.strip_suffix("_fluid_tank")?;
    Some(match tier {
        "basic" => (32_000, 1_000),
        "advanced" => (64_000, 4_000),
        "elite" => (128_000, 16_000),
        "ultimate" => (256_000, 64_000),
        "creative" => (i64::MAX as u64, i32::MAX),
        other => panic!("tier desconhecido: {other}"),
    })
}

struct TankItems;

impl AttachedCodec<FluidResource> for TankItems {
    fn contents(&self, item: &ItemResource) -> StackContents<FluidResource> {
        match item.components.get(FLUIDS) {
            Some(text) => {
                let (id, amount) = text.split_once('|').unwrap();
                StackContents::new(Some(FluidResource::new(id)), amount.parse().unwrap())
            }
            None => StackContents::default(),
        }
    }

    fn with_contents(
        &self,
        item: &ItemResource,
        contents: &StackContents<FluidResource>,
    ) -> Option<ItemResource> {
        let components = match &contents.resource {
            Some(resource) => Components::new([(
                FLUIDS,
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

/// O item de um Fluid Tank com `amount` de `fluid`.
fn tank_item(tier: &str, held: &str, amount: u64) -> ItemResource {
    let empty = ItemResource::new(&format!("mekanism:{tier}_fluid_tank"), 64);
    TankItems
        .with_contents(&empty, &StackContents::new(Some(fluid(held)), amount))
        .unwrap()
}

fn lookup(clock: Rc<Cell<u64>>) -> ItemHandlerLookup<FluidResource> {
    let rules: Rc<dyn PerItemRules<FluidResource>> = Rc::new(Buckets);
    let codec: Rc<dyn AttachedCodec<FluidResource>> = Rc::new(TankItems);
    let spec_of: ComponentTankSpecOf<FluidResource> = Rc::new(|item: &ItemResource| {
        let (capacity, rate) = tier_of(&item.id).unwrap();
        ComponentTankSpec::open(capacity, rate, item.id.contains("creative"))
    });
    Rc::new(move |access: &dyn ItemAccess| {
        let held = access.resource()?;
        if tier_of(&held.id).is_some() {
            let handler: Box<dyn ItemResourceHandler<FluidResource> + '_> = Box::new(
                ComponentBackedHandler::new(access, codec.clone(), spec_of.clone(), clock.clone()),
            );
            return Some(handler);
        }
        if &*held.id != BUCKET && !held.id.ends_with("_bucket") {
            return None;
        }
        let handler: Box<dyn ItemResourceHandler<FluidResource> + '_> =
            Box::new(AccessResourceHandler {
                access,
                rules: rules.clone(),
            });
        Some(handler)
    })
}

type Tanks = Rc<RefCell<Vec<Option<FluidTankTank>>>>;

struct Column {
    metas: Vec<Value>,
    tiles: Vec<Option<RefCell<FluidTankTile>>>,
    /// Os tanques pela altura, para o de baixo achar o de cima no meio de uma inserção.
    tanks: Tanks,
    held: Vec<Option<FluidTankItem>>,
    time: u64,
    /// `MekanismUtils.GAME_TIME_SUPPLIER`, para o limite de taxa do tanque que mora no item.
    clock: Rc<Cell<u64>>,
}

/// A face de cima do Fluid Tank de baixo: o `ValveFluidTankWrapper`, que o `BelowContainerCache` não reconhece.
struct Below<'a>(&'a RefCell<FluidTankTile>);

impl BelowHandler for Below<'_> {
    fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        self.0
            .borrow()
            .handler_insert(true, resource, amount, transaction)
    }

    fn fluid_tank_needed(&self) -> Option<i32> {
        None
    }
}

impl Column {
    fn new(metas: &[Value]) -> Self {
        let mut column = Self {
            metas: metas.to_vec(),
            tiles: Vec::new(),
            tanks: Rc::new(RefCell::new(vec![None; metas.len()])),
            held: vec![None; metas.len()],
            time: 0,
            clock: Rc::new(Cell::new(0)),
        };
        for index in 0..metas.len() {
            column.tiles.push(None);
            column.place(index, None);
        }
        column
    }

    fn place(&mut self, index: usize, from: Option<FluidTankItem>) {
        let meta = &self.metas[index];
        let mut tile = FluidTankTile::new(FluidTankConfig {
            capacity: meta["capacity"].as_u64().unwrap(),
            rate: i32::try_from(meta["rate"].as_i64().unwrap()).unwrap(),
            creative: meta["tier"] == "creative",
            deactivation_delay: 60,
            items: lookup(self.clock.clone()),
        });
        let tanks = self.tanks.clone();
        tile.tank().set_above(Rc::new(move || {
            tanks.borrow().get(index + 1).cloned().flatten()
        }));
        if let Some(from) = from {
            tile.apply_item(&from);
        }
        self.tanks.borrow_mut()[index] = Some(tile.tank().clone());
        self.tiles[index] = Some(RefCell::new(tile));
    }

    fn tile(&self, index: usize) -> &RefCell<FluidTankTile> {
        self.tiles[index].as_ref().unwrap()
    }

    fn apply(&mut self, mark: &str, results: &mut Vec<i64>, drops: &mut Vec<FluidTankItem>) {
        let part: Vec<&str> = mark.split(' ').collect();
        let index: usize = part[1].parse().unwrap();
        match part[0] {
            "fluid" => {
                let tile = self.tile(index).borrow();
                if part[2] == "none" {
                    tile.tank().inner().set_contents(None, 0, None);
                } else {
                    tile.tank().inner().set_contents(
                        Some(fluid(part[2])),
                        part[3].parse().unwrap(),
                        None,
                    );
                }
            }
            "in" | "out" => {
                let tile = self.tile(index).borrow();
                let slot: &InventorySlot = if part[0] == "in" {
                    tile.input_slot().slot()
                } else {
                    tile.output_slot()
                };
                let spec: Vec<&str> = part[2].split(':').collect();
                if spec[0] == "none" {
                    slot.set_contents(None, 0, None);
                } else if spec[0] == "ftank" {
                    // `ftank:<tier>:<fluido>:<mB>:<quantidade>`.
                    let held = tank_item(spec[1], spec[2], spec[3].parse().unwrap());
                    slot.set_contents(Some(held), spec[4].parse().unwrap(), None);
                } else {
                    let count = spec.get(1).map_or(1, |count| count.parse().unwrap());
                    slot.set_contents(Some(item(&format!("minecraft:{}", spec[0]))), count, None);
                }
            }
            "mode" => {
                let mut tile = self.tile(index).borrow_mut();
                if part[2] == "next" {
                    tile.next_mode()
                } else {
                    tile.previous_mode()
                }
            }
            "active" => self
                .tile(index)
                .borrow()
                .tile_mut()
                .set_active(part[2] == "true"),
            "push" | "pull" => {
                let tile = self.tile(index).borrow();
                let resource = fluid(part[3]);
                let amount: i32 = part[4].parse().unwrap();
                let transaction = Transaction::open_root();
                let moved = if part[0] == "push" {
                    tile.handler_insert(part[2] == "up", &resource, amount, &transaction)
                } else {
                    tile.handler_extract(&resource, amount, &transaction)
                };
                transaction.commit();
                results.push(i64::from(moved));
            }
            "break" => {
                let contents = self.tile(index).borrow().collect_item();
                drops.push(contents.clone());
                self.held[index] = Some(contents);
                self.tiles[index] = None;
                self.tanks.borrow_mut()[index] = None;
            }
            "place" => {
                let from = self.held[index].take();
                self.place(index, from);
            }
            other => panic!("marca desconhecida: {other}"),
        }
    }

    fn tick(&mut self) {
        self.time += 1;
        self.clock.set(self.time);
        for index in 0..self.tiles.len() {
            let Some(tile) = &self.tiles[index] else {
                continue;
            };
            let below = index
                .checked_sub(1)
                .and_then(|below| self.tiles[below].as_ref())
                .map(Below);
            tile.borrow_mut().tick_server(
                self.time,
                below.as_ref().map(|below| below as &dyn BelowHandler),
            );
        }
    }
}

fn slot_line(slot: &InventorySlot) -> (String, u64) {
    let SlotContents { resource, amount } = slot.contents();
    (
        resource.map_or(String::new(), |resource| resource.id.to_string()),
        u64::from(amount),
    )
}

fn stack_json(resource: &str, amount: u64) -> Value {
    serde_json::json!({ "type": { "id": resource }, "amount": amount })
}

/// Os componentes do item solto que vêm do tile: `mekanism:fluids`, `mekanism:items` e `mekanism:edit_mode`.
fn drop_components(item: &FluidTankItem) -> Value {
    let mut components = serde_json::Map::new();
    let slots: Vec<Value> = item
        .slots
        .iter()
        .filter(|slot| !slot.is_empty())
        .map(|slot| stack_json(&slot.resource.as_ref().unwrap().id, u64::from(slot.amount)))
        .collect();
    if !slots.is_empty() {
        components.insert(
            "mekanism:items".into(),
            serde_json::json!({ "containers": slots }),
        );
    }
    if let Some(resource) = &item.fluid.resource {
        components.insert(
            "mekanism:fluids".into(),
            serde_json::json!({ "containers": [stack_json(&resource.id, item.fluid.amount)] }),
        );
    }
    components.insert(
        "mekanism:edit_mode".into(),
        Value::from(item.edit_mode.serialized_name()),
    );
    Value::Object(components)
}

fn replay(name: &str) {
    let lines = read(&format!("tank_fluid_{name}.jsonl"));
    let mut column = Column::new(lines[0]["meta"]["tanks"].as_array().unwrap());
    let mut compared = 0;
    for line in &lines[1..] {
        let t = line["t"].as_i64().unwrap();
        let mut results = Vec::new();
        let mut drops = Vec::new();
        for mark in line["marks"].as_array().unwrap() {
            column.apply(mark.as_str().unwrap(), &mut results, &mut drops);
        }
        column.tick();
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
                let stack = &recorded["stacks"][0];
                assert_eq!(stack["count"], 1, "{name} t={t}");
                assert_eq!(
                    stack["components"],
                    drop_components(ours),
                    "{name} t={t}: os componentes do item solto"
                );
            }
        }
        for (index, recorded) in line["tanks"].as_array().unwrap().iter().enumerate() {
            let Some(cell) = &column.tiles[index] else {
                assert!(
                    recorded.is_null(),
                    "{name} t={t}: o tanque {index} devia existir"
                );
                continue;
            };
            let tile = cell.borrow();
            let at = format!("{name} t={t} tanque {index}");
            let held = tile
                .tank()
                .resource()
                .map_or(String::new(), |resource| resource.id.to_string());
            assert_eq!(held, recorded["fluid"].as_str().unwrap(), "{at}: fluido");
            assert_eq!(
                tile.tank().amount(),
                recorded["amount"].as_u64().unwrap(),
                "{at}: quantidade"
            );
            assert_eq!(
                tile.scale(),
                recorded["scale"].as_f64().unwrap() as f32,
                "{at}: escala"
            );
            assert_eq!(
                i64::from(tile.tile().current_redstone_level()),
                recorded["comparator"].as_i64().unwrap(),
                "{at}: comparador"
            );
            assert_eq!(
                tile.tile().active(),
                recorded["active"].as_bool().unwrap(),
                "{at}: ativo"
            );
            assert_eq!(
                tile.edit_mode().serialized_name(),
                recorded["mode"].as_str().unwrap(),
                "{at}: modo"
            );
            let valve = tile
                .valve_fluid()
                .map_or(String::new(), |resource| resource.id.to_string());
            assert_eq!(valve, recorded["valve"].as_str().unwrap(), "{at}: válvula");
            assert_eq!(
                i64::from(tile.light_update_delay()),
                recorded["light"].as_i64().unwrap(),
                "{at}: atraso da luz"
            );
            for (key, slot) in [
                ("in", tile.input_slot().slot()),
                ("out", tile.output_slot()),
            ] {
                let (id, count) = slot_line(slot);
                assert_eq!(
                    id,
                    recorded[key][0].as_str().unwrap(),
                    "{at}: item do slot {key}"
                );
                assert_eq!(
                    count,
                    recorded[key][1].as_u64().unwrap(),
                    "{at}: quantidade do slot {key}"
                );
                // O que o item do slot guarda (por item, sem multiplicar pela pilha): o balde pelo que ele é, o tanque pelo componente.
                let inside = match slot.contents().resource {
                    Some(held) if tier_of(&held.id).is_some() => TankItems.contents(&held),
                    Some(held) => match Buckets.resource_from(&held) {
                        Some(resource) => StackContents::new(Some(resource), 1000),
                        None => StackContents::default(),
                    },
                    None => StackContents::default(),
                };
                assert_eq!(
                    inside
                        .resource
                        .map_or(String::new(), |resource| resource.id.to_string()),
                    recorded[key][2].as_str().unwrap(),
                    "{at}: fluido do item do slot {key}"
                );
                assert_eq!(
                    inside.amount,
                    recorded[key][3].as_u64().unwrap(),
                    "{at}: mB do item do slot {key}"
                );
            }
            compared += 1;
        }
    }
    assert!(compared >= 96, "{name}: só {compared} estados comparados");
}

#[test]
fn the_slots_in_both_mode_follow_the_original() {
    replay("slots_both");
}

#[test]
fn the_fill_and_empty_modes_follow_the_original() {
    replay("slots_modes");
}

#[test]
fn the_column_pushes_down_overflows_up_and_marks_the_valve_like_the_original() {
    replay("column");
}

#[test]
fn the_four_tiers_drain_into_the_bottom_tank_like_the_original() {
    replay("tiers");
}

#[test]
fn the_creative_tank_follows_the_original() {
    replay("creative");
}

#[test]
fn the_tank_broken_and_placed_back_keeps_what_the_item_carries() {
    replay("item");
}

#[test]
fn a_tank_item_in_the_slot_follows_the_original() {
    replay("slots_tank_item");
}

#[test]
fn changing_the_edit_mode_clears_the_direction_of_the_slot_like_the_original() {
    replay("mode_reset");
}

#[test]
fn the_light_delay_follows_the_original() {
    replay("light");
}

/// `BlockFluidTank.useItemOn` -> `FluidUtil.interactWithFluidHandler`: primeiro o tanque enche o item da mão, e só se nada passou o item
/// esvazia no tanque. Sem rastro: confere a ordem lida no NeoForge (`FluidUtil.java`, `ResourceHandlerUtil.moveFirst`).
#[test]
fn a_bucket_clicked_on_the_block_is_filled_first_and_emptied_only_if_nothing_moved() {
    use machinery_basis_core::fluid_tank::FaceHandler;
    use machinery_basis_core::item_access::{StackAccess, move_first};
    let clock = Rc::new(Cell::new(1));
    let items = lookup(clock);
    let tile = FluidTankTile::new(FluidTankConfig {
        capacity: 32_000,
        rate: 1_000,
        creative: false,
        deactivation_delay: 60,
        items: items.clone(),
    });
    tile.tank()
        .inner()
        .set_contents(Some(fluid("water")), 5_000, None);
    let click = |hand: &StackAccess, top: bool| {
        let block = FaceHandler { tile: &tile, top };
        let held = items(hand).unwrap();
        let transaction = Transaction::open_root();
        let moved = move_first(&block, held.as_ref(), i32::MAX, Some(&transaction))
            .map(|(_, amount)| -amount)
            .or_else(|| {
                move_first(held.as_ref(), &block, i32::MAX, Some(&transaction))
                    .map(|(_, amount)| amount)
            });
        transaction.commit();
        moved
    };
    // O balde vazio enche do tanque.
    let hand = StackAccess::of(&item(BUCKET));
    assert_eq!(click(&hand, false), Some(-1_000));
    assert_eq!(&*hand.resource().unwrap().id, "minecraft:water_bucket");
    assert_eq!(tile.tank().amount(), 4_000);
    // O balde cheio não recebe mais nada, e esvazia no tanque; pelo topo a válvula marca.
    assert_eq!(click(&hand, true), Some(1_000));
    assert_eq!(&*hand.resource().unwrap().id, BUCKET);
    assert_eq!(tile.tank().amount(), 5_000);
    assert_eq!(tile.valve_fluid(), Some(fluid("water")));
    // Lava num tanque de água não entra, e o balde de lava não recebe água.
    let lava = StackAccess::of(&item("minecraft:lava_bucket"));
    assert_eq!(click(&lava, false), None);
    // Um item de tanque vazio recebe tudo o que o tanque tem: a mão é `MANUAL` só no uso do item no mundo, aqui o limite é o da taxa.
    let tank_item = StackAccess::of(&ItemResource::new("mekanism:basic_fluid_tank", 64));
    assert_eq!(click(&tank_item, false), Some(-1_000));
    assert_eq!(tile.tank().amount(), 4_000);
}
