// Os Fluid Tanks no jogo (`TileEntityFluidTank`, `BlockFluidTank`). As regras ficam em machinery_basis_core::fluid_tank (o tile, os dois slots, o empurrão para baixo, a
// válvula, o comparador) e em machinery_basis_core::component_container (o tanque que mora no item); aqui ficam os valores do tier, o NBT, os componentes do item e os cliques.
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{JavaString, Tag};
use machinery_basis_core::component_container::{AttachedCodec, ComponentBackedHandler, ComponentTankSpec, ComponentTankSpecOf};
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::fluid_tank::{self, BelowHandler, FaceHandler, FluidTankConfig, FluidTankTank, FluidTankTile};
use machinery_basis_core::handler_slot::{ContainerEditMode, LastTransferDirection, ResourceContainer};
use machinery_basis_core::item_access::{AccessResourceHandler, ItemAccess, ItemHandlerLookup, ItemResourceHandler, OneByOne, PerItemRules, move_first};
use machinery_basis_core::item_content as content;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::tank::Tank;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Os slots do tile, na ordem de `getInitialInventory`: o de entrada e o de saída.
const SLOTS: usize = 2;
const REDSTONE_OWNER: &str = "mekanism_tank";
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const CURRENT_REDSTONE: &str = "current_redstone";
/// `SerializationConstants.EDIT_MODE`, `DELAY` e `ACTIVE_STATE`, e a direção do slot de entrada (`LAST_TRANSFER_DIRECTION`, que o Java grava dentro do slot).
const EDIT_MODE: &str = "edit_mode";
const DELAY: &str = "delay";
const ACTIVE_STATE: &str = "active_state";
const DIRECTION: &str = "last_transfer_direction";
const ACTIVE_PROPERTY: &str = "active";
const EMPTY_BUCKET: &str = "minecraft:bucket";
const BUCKET_VOLUME: i32 = 1000;
/// Um objeto vivo que não anda por tantos ticks sai da tabela (o bloco foi quebrado ou o chunk descarregado).
const STALE_TICKS: i64 = 100;
const SWEEP_TICKS: i64 = 200;

type Key = (&'static str, [i32; 3]);
/// O que o NBT guarda do tanque: o fluido e a quantidade.
type Stored = (Option<String>, u64);

/// O objeto vivo de um tanque. O Horizon só guarda a `ModBlockEntity`; o que o Java guarda no objeto (o contêiner, a válvula, a escala, a direção do slot) fica aqui, entre os
/// ticks, e o NBT leva o que o Java salva.
struct LiveTank {
    tile: FluidTankTile,
    last_seen: i64,
    /// O fluido que o NBT tinha na última vez que este objeto o viu ou escreveu: o que difere disso veio de fora (um comando de dados, o item posto).
    seen: Stored,
    /// `redstone`, o modo de controle e o nível do comparador que o NBT já tem.
    scalars: Option<(bool, RedstoneControl, i32)>,
    /// O modo de edição, o atraso da luz, o estado ativo e a direção do slot que o NBT já tem.
    saved: Option<(ContainerEditMode, i32, bool, LastTransferDirection)>,
    /// A propriedade `active` que o bloco já tem.
    block_active: Option<bool>,
}

thread_local! {
    static LIVE: RefCell<HashMap<Key, Rc<RefCell<LiveTank>>>> = RefCell::new(HashMap::new());
    /// O contêiner de cada tanque vivo, para o de baixo achar o de cima no meio de uma inserção sem tocar no objeto dele (que pode estar no meio do próprio tick).
    static TANKS: RefCell<HashMap<Key, FluidTankTank>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
    /// `MekanismUtils.GAME_TIME_SUPPLIER`: o tick do jogo, para o limite de taxa do tanque que mora no item.
    static CLOCK: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    /// O conteúdo e a pilha de origem de cada item de tanque que passou por aqui, pela chave dos componentes dele.
    static ITEM_FLUIDS: RefCell<HashMap<String, (StackContents<FluidResource>, ItemStack)>> = RefCell::new(HashMap::new());
    /// O modo de edição do item que acabou de ser posto, para o tile que o Horizon ainda não criou, e o do tile que vai ser quebrado.
    static PENDING_MODE: RefCell<HashMap<[i32; 3], ContainerEditMode>> = RefCell::new(HashMap::new());
    static BROKEN_MODE: RefCell<HashMap<[i32; 3], ContainerEditMode>> = RefCell::new(HashMap::new());
}

pub(in crate::game) fn tier_of(block: &str) -> Option<usize> {
    rules::fluid_tank_tier(block)
}

pub(in crate::game) fn is_tank(block: &str) -> bool {
    tier_of(block).is_some()
}

/// `tier.getCapacity()` e `tier.getTransferRate()`, os do `tiers.toml`.
fn tier_values(tier: usize) -> (u64, i32) {
    let config = glue::server_config();
    (u64::try_from(config.fluid_tanks[tier]).unwrap_or(0), config.fluid_tank_rates[tier])
}

// ---------------------------------------------------------------------------------------------------------------------------
// A capability de fluido dos itens (`Capabilities.FLUID.item()`): os baldes e o item do próprio tanque.

/// `BucketResourceHandler`: o balde de água, o de lava e o de cada fluido de mod, e o balde vazio.
struct Buckets;

fn bucket_fluid(item: &str) -> Option<FluidResource> {
    match item {
        "minecraft:water_bucket" => Some(FluidResource::new("minecraft:water")),
        "minecraft:lava_bucket" => Some(FluidResource::new("minecraft:lava")),
        other => machinery_basis_core::liquid_data::current().by_bucket(other).map(|spec| FluidResource::new(&spec.id)),
    }
}

fn fluid_bucket(fluid: &str) -> Option<String> {
    match fluid {
        "minecraft:water" => Some("minecraft:water_bucket".to_owned()),
        "minecraft:lava" => Some("minecraft:lava_bucket".to_owned()),
        other => machinery_basis_core::liquid_data::current().iter().find(|spec| spec.id == other).map(|spec| spec.bucket.clone()),
    }
}

impl PerItemRules<FluidResource> for Buckets {
    fn resource_from(&self, item: &ItemResource) -> Option<FluidResource> {
        bucket_fluid(&item.id)
    }

    fn amount_from(&self, item: &ItemResource) -> i32 {
        if bucket_fluid(&item.id).is_some() { BUCKET_VOLUME } else { 0 }
    }

    fn capacity(&self, _resource: &FluidResource) -> i32 {
        BUCKET_VOLUME
    }

    fn update(&self, _item: &ItemResource, resource: &FluidResource, amount: i32) -> Option<ItemResource> {
        if amount == 0 {
            // `Items.BUCKET`, que empilha até 16.
            Some(ItemResource::new(EMPTY_BUCKET, 16))
        } else if amount != BUCKET_VOLUME {
            None
        } else {
            fluid_bucket(&resource.id).map(|bucket| ItemResource::new(&bucket, 1))
        }
    }
}

/// O `mekanism:fluids` do item do tanque. O conteúdo de um recurso que a lib fez (o item depois de encher ou esvaziar) não passou por uma pilha do jogo: fica na tabela, pela
/// chave dos componentes, com a pilha de origem, e vira pilha em [`stack_of`].
struct TankItems;

fn fluids_text(contents: &StackContents<FluidResource>) -> String {
    match &contents.resource {
        Some(resource) => format!("{{containers:[{{amount:{}L,type:{{id:\"{}\"}}}}]}}", contents.amount, resource.id),
        None => String::new(),
    }
}

impl AttachedCodec<FluidResource> for TankItems {
    fn contents(&self, item: &ItemResource) -> StackContents<FluidResource> {
        if item.components.get(basis::CONTENT_FLUIDS).is_none() {
            return StackContents::default();
        }
        ITEM_FLUIDS.with_borrow(|table| table.get(item.components.key()).map(|(contents, _)| contents.clone())).unwrap_or_default()
    }

    fn with_contents(&self, item: &ItemResource, contents: &StackContents<FluidResource>) -> Option<ItemResource> {
        let text = fluids_text(contents);
        let mut entries: Vec<(&str, &str)> = item.components.iter().filter(|(id, _)| *id != basis::CONTENT_FLUIDS).collect();
        if !contents.is_empty() {
            entries.push((basis::CONTENT_FLUIDS, text.as_str()));
        }
        let components = Components::new(entries);
        let origin = ITEM_FLUIDS
            .with_borrow(|table| table.get(item.components.key()).map(|(_, stack)| stack.clone()))
            .or_else(|| basis::stack_of_resource(item, 1))
            .unwrap_or_else(|| ItemStack::new(&item.id, 1));
        if !components.is_empty() {
            ITEM_FLUIDS.with_borrow_mut(|table| table.insert(components.key().to_owned(), (contents.clone(), origin)));
        }
        Some(ItemResource::with_components(&item.id, item.max_stack_size, components))
    }
}

/// O que o `mekanism:fluids` de uma pilha do jogo guarda.
fn stack_fluids(game: &Game<'_>, stack: &ItemStack) -> StackContents<FluidResource> {
    let Ok(Some(tag)) = game.native_component(stack, basis::CONTENT_FLUIDS) else {
        return StackContents::default();
    };
    let Some(list) = content::containers_apply(&basis::tag_value(tag), 1) else {
        return StackContents::default();
    };
    let stored = &list[0];
    let amount = stored["amount"].as_u64().unwrap_or(0);
    StackContents::new(stored["type"]["id"].as_str().map(FluidResource::new), amount)
}

fn fluids_json(contents: &StackContents<FluidResource>) -> serde_json::Value {
    match &contents.resource {
        Some(resource) if contents.amount > 0 => serde_json::json!({"type": {"id": &*resource.id}, "amount": contents.amount}),
        _ => serde_json::json!({}),
    }
}

/// Grava o conteúdo no `mekanism:fluids` da pilha; vazio é o padrão do item, e o componente sai.
fn set_stack_fluids(game: &Game<'_>, stack: &mut ItemStack, contents: &StackContents<FluidResource>) {
    let result = match content::containers_collect(&[fluids_json(contents)]) {
        Some(value) => game.native_set_component(stack, basis::CONTENT_FLUIDS, basis::value_tag(&value, "")),
        None => game.native_remove_component(stack, basis::CONTENT_FLUIDS),
    };
    if let Err(error) = result {
        eprintln!("mekanism: o item do tanque não leva {}: {error}", basis::CONTENT_FLUIDS);
    }
}

/// O recurso de uma pilha do jogo, com o fluido dela registrado para o handler se for um item de tanque.
fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let resource = basis::item_resource(game, stack);
    if is_tank(&stack.item) && !resource.components.is_empty() {
        let contents = stack_fluids(game, stack);
        ITEM_FLUIDS.with_borrow_mut(|table| table.insert(resource.components.key().to_owned(), (contents, stack.clone())));
    }
    resource
}

/// A pilha de um recurso: a que passou pelo jogo, ou, para o que a lib fez, a de origem com o fluido novo.
fn stack_of(game: &Game<'_>, resource: &ItemResource, count: u32) -> ItemStack {
    let made = ITEM_FLUIDS.with_borrow(|table| table.get(resource.components.key()).cloned());
    if let Some((contents, origin)) = made.filter(|_| is_tank(&resource.id)) {
        let mut stack = origin;
        set_stack_fluids(game, &mut stack, &contents);
        stack.count = count;
        return stack;
    }
    if let Some(stack) = basis::stack_of_resource(resource, count) {
        return stack;
    }
    ItemStack::new(&resource.id, count)
}

/// `Capabilities.FLUID.item()`.
fn items() -> ItemHandlerLookup<FluidResource> {
    let buckets: Rc<dyn PerItemRules<FluidResource>> = Rc::new(Buckets);
    let codec: Rc<dyn AttachedCodec<FluidResource>> = Rc::new(TankItems);
    // `ComponentBackedFluidTankFluidTank.create`: o tier do item dá a capacidade, a taxa e o criativo.
    let spec_of: ComponentTankSpecOf<FluidResource> = Rc::new(|item: &ItemResource| {
        let tier = tier_of(&item.id).unwrap_or_default();
        let (capacity, rate) = tier_values(tier);
        ComponentTankSpec::open(capacity, rate, tier == rules::TANK_CREATIVE)
    });
    let clock = CLOCK.with(Rc::clone);
    Rc::new(move |access: &dyn ItemAccess| {
        let held = access.resource()?;
        if is_tank(&held.id) {
            let handler: Box<dyn ItemResourceHandler<FluidResource> + '_> = Box::new(ComponentBackedHandler::new(access, codec.clone(), spec_of.clone(), clock.clone()));
            return Some(handler);
        }
        if &*held.id != EMPTY_BUCKET && bucket_fluid(&held.id).is_none() {
            return None;
        }
        let handler: Box<dyn ItemResourceHandler<FluidResource> + '_> = Box::new(AccessResourceHandler { access, rules: buckets.clone() });
        Some(handler)
    })
}

// ---------------------------------------------------------------------------------------------------------------------------
// O NBT.

fn read_scalars(entity: &ModBlockEntity) -> (bool, RedstoneControl, i32) {
    (
        glue::nbt_bool(entity, REDSTONE),
        RedstoneControl::from_serialized_name(&glue::nbt_string(entity, CONTROL_TYPE).unwrap_or_default()),
        glue::nbt_int(entity, CURRENT_REDSTONE),
    )
}

fn write_scalars(entity: &mut ModBlockEntity, scalars: (bool, RedstoneControl, i32)) {
    glue::put(entity, REDSTONE, Tag::Byte(i8::from(scalars.0)));
    glue::put(entity, CONTROL_TYPE, Tag::String(JavaString::from(scalars.1.serialized_name())));
    glue::put(entity, CURRENT_REDSTONE, Tag::Int(scalars.2));
}

fn read_stored(entity: &ModBlockEntity, capacity: u64) -> Stored {
    let tanks = basis::load_tanks(entity, &[capacity]);
    let tank = &tanks[0];
    match tank.resource() {
        Some(id) if tank.amount() > 0 => (Some(id.to_owned()), tank.amount()),
        _ => (None, 0),
    }
}

fn write_stored(entity: &mut ModBlockEntity, capacity: u64, stored: &Stored) {
    basis::save_tanks(entity, &[Tank::restored(stored.0.clone(), stored.1, capacity)]);
}

fn stored_of(tank: &FluidTankTank) -> Stored {
    match tank.resource() {
        Some(resource) if tank.amount() > 0 => (Some(resource.id.to_string()), tank.amount()),
        _ => (None, 0),
    }
}

fn set_stored(tank: &FluidTankTank, stored: &Stored) {
    tank.inner().set_contents(stored.0.as_deref().map(FluidResource::new), stored.1, None);
}

fn read_mode(entity: &ModBlockEntity) -> ContainerEditMode {
    glue::nbt_string(entity, EDIT_MODE).and_then(|name| ContainerEditMode::from_serialized_name(&name)).unwrap_or(ContainerEditMode::Both)
}

/// O tile novo do que a `ModBlockEntity` guarda (`loadAdditional`).
fn born(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], entity: &ModBlockEntity, tier: usize) -> LiveTank {
    let (capacity, rate) = tier_values(tier);
    let mut tile = FluidTankTile::new(FluidTankConfig {
        capacity,
        rate,
        creative: tier == rules::TANK_CREATIVE,
        deactivation_delay: rules::block_deactivation_delay_now(),
        items: items(),
    });
    let scalars = read_scalars(entity);
    let active = glue::nbt_bool(entity, ACTIVE_STATE);
    tile.tile_mut().restore(
        &machinery_basis_core::tile::SavedState { redstone: scalars.0, control_type: scalars.1, current_redstone: scalars.2, active, ..Default::default() },
        game.native_block_property(pos, ACTIVE_PROPERTY).as_deref() == Some("true"),
    );
    let seen = read_stored(entity, capacity);
    set_stored(tile.tank(), &seen);
    tile.restore_edit_mode(read_mode(entity));
    tile.set_light_update_delay(glue::nbt_int(entity, DELAY));
    tile.input_slot()
        .set_last_transfer_direction(LastTransferDirection::from_serialized_name(&glue::nbt_string(entity, DIRECTION).unwrap_or_default()), None);
    // `WorldUtils.getTileEntity(TileEntityFluidTank.class, level, pos.above())`: o tanque de cima, se ele está vivo.
    let above = (dimension, [pos[0], pos[1] + 1, pos[2]]);
    tile.tank().set_above(Rc::new(move || TANKS.with(|tanks| tanks.try_borrow().ok().and_then(|tanks| tanks.get(&above).cloned()))));
    LiveTank { tile, last_seen: i64::MIN, seen, scalars: None, saved: None, block_active: None }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que o Horizon deixa ver.

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    if is_tank(block) {
        // `AttributeRedstone`: o jogo avisa o bloco quando um vizinho muda.
        basis::watch_redstone(REDSTONE_OWNER, is_tank);
        if entity.is_none() {
            *entity = Some((block.to_owned(), horizon_world::nbt::Compound::default()));
        }
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_tank(block) {
        *ticks = true;
        // A capability de fluido do tanque, que os canos mecânicos alcançam pelo contêiner (a válvula do topo e o que transborda para o tanque de cima ficam para a frente dos
        // vizinhos que empurram).
        for id in rules::FLUID_TANK_TIERS {
            crate::game::machinery_resources::fluid_neighbor_register(id, probe, write);
        }
    }
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_tank(block) {
        // Os dois slots, sem face nenhuma: `TileEntityFluidTank` não expõe a capability de item.
        *container = Some(ModContainer { slots: SLOTS, ..Default::default() });
    }
}

/// O tanque como a rede de fluido o vê: o contêiner do objeto vivo, ou o do NBT.
fn probe(
    game: &Game<'_>,
    dimension: &'static str,
    pos: [i32; 3],
) -> Option<machinery_basis_core::transmission::host_resource::ResourceSnapshot<FluidResource>> {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    let tier = tier_of(&entity.id)?;
    let (capacity, _) = tier_values(tier);
    let stored =
        TANKS.with(|tanks| tanks.try_borrow().ok().and_then(|tanks| tanks.get(&(dimension, pos)).map(stored_of))).unwrap_or_else(|| read_stored(entity, capacity));
    Some(machinery_basis_core::transmission::host_resource::ResourceSnapshot {
        contents: StackContents::new(stored.0.as_deref().map(FluidResource::new), stored.1),
        capacity,
        accepts: Rc::new(|_: &FluidResource| true),
        can_insert: true,
        can_extract: true,
        creative: tier == rules::TANK_CREATIVE,
        increment: 1,
    })
}

/// O que a rede mudou no tanque.
fn write(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], contents: StackContents<FluidResource>) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let Some(tier) = tier_of(&entity.id) else {
        return;
    };
    let stored: Stored = (contents.resource.map(|fluid| fluid.id.to_string()), contents.amount);
    TANKS.with(|tanks| {
        if let Ok(tanks) = tanks.try_borrow()
            && let Some(tank) = tanks.get(&(dimension, pos))
        {
            set_stored(tank, &stored);
        }
    });
    write_stored(entity, tier_values(tier).0, &stored);
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os slots entre a `ModBlockEntity` e o objeto.

fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => {
            if !slot.is_empty() {
                slot.set_contents(None, 0, None);
            }
        }
        Some(stack) => {
            let resource = resource_of(game, stack);
            if slot.resource().as_ref() != Some(&resource) || slot.amount() != stack.count {
                slot.set_contents(Some(resource), stack.count, None);
            }
        }
    }
}

fn slot_to_entity(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource == resource_of(game, stack) && contents.amount == stack.count,
        _ => false,
    };
    if !same {
        entity.items[index] = contents.resource.map(|resource| stack_of(game, &resource, contents.amount));
    }
}

/// A face de cima do Fluid Tank de baixo: o `ValveFluidTankWrapper`, que o `BelowContainerCache` não reconhece como `FluidTankFluidTank` (`TileEntityFluidTank.java:117-118`).
struct Below(Rc<RefCell<LiveTank>>);

impl BelowHandler for Below {
    fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        match self.0.try_borrow() {
            Ok(live) => live.tile.handler_insert(true, resource, amount, transaction),
            Err(_) => 0,
        }
    }

    fn fluid_tank_needed(&self) -> Option<i32> {
        None
    }
}

fn live_at(key: &Key) -> Option<Rc<RefCell<LiveTank>>> {
    LIVE.with(|live| live.try_borrow().ok().and_then(|live| live.get(key).cloned()))
}

// ---------------------------------------------------------------------------------------------------------------------------
// O tick.

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let Some(tier) = tier_of(&entity.id) else {
        return;
    };
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    if entity.items.len() < SLOTS {
        entity.items.resize(SLOTS, None);
    }
    basis::name_apply_pending(entity, pos);
    basis::security_apply_pending(entity, pos);
    basis::content_apply_pending(entity, pos, &content_spec());
    if let Some(mode) = PENDING_MODE.with_borrow_mut(|pending| pending.remove(&pos)) {
        glue::put(entity, EDIT_MODE, Tag::String(JavaString::from(mode.serialized_name())));
    }
    CLOCK.with(|clock| clock.set(now as u64));
    let (capacity, _) = tier_values(tier);
    let cell = match live_at(&key) {
        Some(cell) if cell.borrow().last_seen + 1 == now && glue::has_saved(entity) => cell,
        _ => {
            let cell = Rc::new(RefCell::new(born(game, dimension, pos, entity, tier)));
            TANKS.with_borrow_mut(|tanks| tanks.insert(key, cell.borrow().tile.tank().clone()));
            LIVE.with_borrow_mut(|live| live.insert(key, cell.clone()));
            crate::game::machinery_resources::fluid_faces_set(dimension, pos, &entity.id, [true; 6]);
            cell
        }
    };
    let mut live = cell.borrow_mut();
    slot_from_entity(game, live.tile.input_slot().slot(), entity.items[fluid_tank::INPUT_SLOT].as_ref());
    slot_from_entity(game, live.tile.output_slot(), entity.items[fluid_tank::OUTPUT_SLOT].as_ref());
    // Quem mexeu no NBT por fora (um comando de dados, o item posto) muda o contêiner e o modo.
    let in_nbt = read_stored(entity, capacity);
    if in_nbt != live.seen {
        set_stored(live.tile.tank(), &in_nbt);
        live.seen = in_nbt;
    }
    if let Some(saved) = live.saved {
        let mode = read_mode(entity);
        if mode != saved.0 {
            live.tile.restore_edit_mode(mode);
        }
        let active = glue::nbt_bool(entity, ACTIVE_STATE);
        if active != saved.2 {
            live.tile.tile_mut().set_active(active);
        }
        let direction = LastTransferDirection::from_serialized_name(&glue::nbt_string(entity, DIRECTION).unwrap_or_default());
        if direction != saved.3 {
            live.tile.input_slot().set_last_transfer_direction(direction, None);
        }
    }
    if let Some(powered) = basis::take_power(game, pos) {
        live.tile.tile_mut().update_power(powered);
    }

    // `BelowContainerCache`: o handler de fluido do bloco de baixo, pela face de cima dele. Só o Fluid Tank de baixo é alcançado aqui; os outros vizinhos ficam para a frente
    // dos vizinhos que empurram.
    let below = live_at(&(dimension, [pos[0], pos[1] - 1, pos[2]])).filter(|below| now - below.borrow().last_seen <= 1).map(Below);
    let result = live.tile.tick_server(now as u64, below.as_ref().map(|below| below as &dyn BelowHandler));
    glue::publish_comparator(game, dimension, pos, live.tile.tile().current_redstone_level(), result.comparator_changed);

    slot_to_entity(game, entity, fluid_tank::INPUT_SLOT, live.tile.input_slot().slot());
    slot_to_entity(game, entity, fluid_tank::OUTPUT_SLOT, live.tile.output_slot());
    let stored = stored_of(live.tile.tank());
    if stored != live.seen {
        write_stored(entity, capacity, &stored);
        live.seen = stored;
    }
    let scalars = {
        let tile = live.tile.tile();
        (tile.is_powered(), tile.control_type(), tile.current_redstone_level())
    };
    if live.scalars != Some(scalars) {
        write_scalars(entity, scalars);
        live.scalars = Some(scalars);
    }
    let saved = (live.tile.edit_mode(), live.tile.light_update_delay(), live.tile.tile().active(), live.tile.input_slot().last_transfer_direction());
    if live.saved != Some(saved) {
        glue::put(entity, EDIT_MODE, Tag::String(JavaString::from(saved.0.serialized_name())));
        glue::put(entity, DELAY, Tag::Int(saved.1));
        glue::put(entity, ACTIVE_STATE, Tag::Byte(i8::from(saved.2)));
        glue::put(entity, DIRECTION, Tag::String(JavaString::from(saved.3.serialized_name())));
        live.saved = Some(saved);
    }
    // `setActive` -> `Attribute.setActive`: a propriedade `active` do bloco acompanha o tile.
    if live.block_active != Some(saved.2) {
        game.native_set_block_property(pos, ACTIVE_PROPERTY, if saved.2 { "true" } else { "false" });
        live.block_active = Some(saved.2);
    }
    if entity.data.is_empty() {
        entity.data = vec![1];
    }
    live.last_seen = now;
    drop(live);
    sync_column(game, dimension, pos);
    if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
        LAST_SWEEP.set(now);
        LIVE.with_borrow_mut(|live| live.retain(|_, tank| tank.try_borrow().map_or(true, |tank| now - tank.last_seen <= STALE_TICKS)));
        let alive: Vec<Key> = LIVE.with_borrow(|live| live.keys().copied().collect());
        TANKS.with_borrow_mut(|tanks| tanks.retain(|key, _| alive.contains(key)));
    }
}

/// O que este tick pôs nos outros tanques da coluna (o de baixo, e os de cima por onde subiu o que não coube) vai ao NBT deles agora: o objeto vivo de cada um só o
/// gravaria no próprio tick, e até lá o mundo salvo teria o fluido que saiu daqui sem o que chegou lá.
fn sync_column(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    let mut others = vec![[pos[0], pos[1] - 1, pos[2]]];
    let mut y = pos[1] + 1;
    while live_at(&(dimension, [pos[0], y, pos[2]])).is_some() {
        others.push([pos[0], y, pos[2]]);
        y += 1;
    }
    for other in others {
        let Some(cell) = live_at(&(dimension, other)) else {
            continue;
        };
        let Ok(mut live) = cell.try_borrow_mut() else {
            continue;
        };
        let stored = stored_of(live.tile.tank());
        if stored == live.seen {
            continue;
        }
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(other[0], other[1], other[2])) else {
            continue;
        };
        let Some(tier) = tier_of(&entity.id) else {
            continue;
        };
        write_stored(entity, tier_values(tier).0, &stored);
        live.seen = stored;
        game.world.modified.insert((other[0] >> 4, other[2] >> 4));
    }
}

/// Corre `act` no tile em `pos`: o objeto vivo, ou um feito do NBT que não fica na tabela. O conteúdo e o estado que `act` mudou vão ao NBT na hora.
fn with_tile<R>(game: &mut Game<'_>, pos: [i32; 3], act: impl FnOnce(&mut Game<'_>, &mut FluidTankTile) -> R) -> Option<R> {
    let block = game.native_block(pos);
    let tier = tier_of(&block)?;
    let dimension = game.native_dimension();
    let at = (pos[0], pos[1], pos[2]);
    let cell = match live_at(&(dimension, pos)) {
        Some(cell) => cell,
        None => {
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
                return None;
            };
            Rc::new(RefCell::new(born(game, dimension, pos, entity, tier)))
        }
    };
    let mut live = cell.try_borrow_mut().ok()?;
    let result = act(game, &mut live.tile);
    let stored = stored_of(live.tile.tank());
    let active = live.tile.tile().active();
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        if stored != live.seen {
            write_stored(entity, tier_values(tier).0, &stored);
            live.seen = stored;
        }
        glue::put(entity, ACTIVE_STATE, Tag::Byte(i8::from(active)));
        if let Some(saved) = live.saved.as_mut() {
            saved.2 = active;
        }
    }
    if live.block_active != Some(active) {
        game.native_set_block_property(pos, ACTIVE_PROPERTY, if active { "true" } else { "false" });
        live.block_active = Some(active);
    }
    game.world.modified.insert((at.0 >> 4, at.2 >> 4));
    Some(result)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os cliques.

/// `TileEntityFluidTank.onSneakRightClick` (`:236-242`), pelo Configurator agachado: liga e desliga o empurrão para baixo, com o clique de botão. Sem agachar é
/// `onRightClick`, que passa. Devolve se o clique foi do item.
pub(in crate::game) fn configurator_click(game: &mut Game<'_>, pos: [i32; 3], sneaking: bool) -> bool {
    if !sneaking {
        return false;
    }
    if with_tile(game, pos, |_, tile| tile.toggle_active()).is_none() {
        return false;
    }
    game.audio.play("minecraft:ui.button.click", Some([f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])]), 0.3, 1.0);
    true
}

/// O item da mão do jogador, um por um (`ItemAccess.forPlayerInteraction(player, hand).oneByOne()`): o que sobra de uma troca numa pilha vai para o inventário, e no modo
/// criativo a mão não muda e o item novo é dado à parte.
struct HandState {
    resource: Option<ItemResource>,
    count: i32,
    extra: Vec<(ItemResource, i32)>,
}

impl Journaled for HandState {
    type Snapshot = (Option<ItemResource>, i32, Vec<(ItemResource, i32)>);

    fn revert_to_snapshot(&mut self, snapshot: Self::Snapshot) {
        (self.resource, self.count, self.extra) = snapshot;
    }

    fn on_root_commit(&mut self, _original: Self::Snapshot) -> Vec<Callback> {
        Vec::new()
    }
}

struct HandAccess {
    cell: Rc<JournalCell<HandState>>,
    infinite: bool,
}

impl HandAccess {
    fn touch(&self, transaction: &Transaction) {
        let snapshot = {
            let state = self.cell.borrow();
            (state.resource.clone(), state.count, state.extra.clone())
        };
        self.cell.update_snapshots(transaction, || snapshot);
    }
}

impl ItemAccess for HandAccess {
    fn resource(&self) -> Option<ItemResource> {
        self.cell.borrow().resource.clone()
    }

    fn amount(&self) -> i32 {
        self.cell.borrow().count
    }

    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        if amount <= 0 {
            return 0;
        }
        self.touch(transaction);
        let mut state = self.cell.borrow_mut();
        let fits_hand = !self.infinite
            && match &state.resource {
                None => true,
                Some(held) => held == resource && state.count + amount <= i32::try_from(resource.max_stack_size).unwrap_or(i32::MAX),
            };
        if fits_hand {
            state.resource = Some(resource.clone());
            state.count += amount;
        } else {
            state.extra.push((resource.clone(), amount));
        }
        amount
    }

    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        let held = self.cell.borrow().resource.clone();
        if held.as_ref() != Some(resource) || amount <= 0 {
            return 0;
        }
        if self.infinite {
            return amount;
        }
        self.touch(transaction);
        let mut state = self.cell.borrow_mut();
        let taken = amount.min(state.count);
        state.count -= taken;
        if state.count == 0 {
            state.resource = None;
        }
        taken
    }
}

/// `BlockFluidTank.useItemOn` depois do `tryWrench`, sem agachar: `FluidUtil.interactWithFluidHandler` com a capability da face clicada. `Some(true)` se algo passou
/// (`SUCCESS`), `Some(false)` para quem não tem acesso (`FAIL`), `None` se nada passou (`TRY_WITH_EMPTY_HAND`).
pub(in crate::game) fn bucket_click(game: &mut Game<'_>, pos: [i32; 3], top: bool) -> Option<bool> {
    let stack = game.held().cloned()?;
    if !basis::security_access_or_error(game, pos, &|id| is_tank(id)) {
        return Some(false);
    }
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    let infinite = game.native_acting_player().is_some_and(|player| player.game_mode == 1);
    let hand = HandAccess {
        cell: JournalCell::new(HandState { resource: Some(resource_of(game, &stack)), count: i32::try_from(stack.count).unwrap_or(i32::MAX), extra: Vec::new() }),
        infinite,
    };
    let lookup = items();
    let moved = with_tile(game, pos, |_, tile| {
        let one = OneByOne(&hand);
        let held = lookup(&one)?;
        let block = FaceHandler { tile, top };
        let transaction = Transaction::open_safe();
        // Primeiro o tanque enche o item (`pickup`); só se nada passou o item esvazia no tanque.
        let moved = move_first(&block, held.as_ref(), i32::MAX, Some(&transaction))
            .map(|(resource, _)| (resource, true))
            .or_else(|| move_first(held.as_ref(), &block, i32::MAX, Some(&transaction)).map(|(resource, _)| (resource, false)));
        if moved.is_some() {
            transaction.commit();
        }
        moved
    })
    .flatten();
    let (resource, pickup) = moved?;
    // `FluidUtil.triggerSoundAndGameEvent`: o som do balde do fluido e o evento, no centro do bloco.
    let sound = match (&*resource.id, pickup) {
        ("minecraft:lava", true) => "minecraft:item.bucket.fill_lava".to_owned(),
        ("minecraft:lava", false) => "minecraft:item.bucket.empty_lava".to_owned(),
        (other, _) => match machinery_basis_core::liquid_data::current().iter().find(|spec| spec.id == other) {
            Some(spec) if pickup => spec.fill_sound.clone(),
            Some(spec) => spec.empty_sound.clone(),
            None if pickup => "minecraft:item.bucket.fill".to_owned(),
            None => "minecraft:item.bucket.empty".to_owned(),
        },
    };
    game.audio.play(&sound, Some([f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5]), 1.0, 1.0);
    game.post_block_game_event(if pickup { "minecraft:fluid_pickup" } else { "minecraft:fluid_place" }, pos, Some(horizon_client::mobs::Who::Player), None);
    let (in_hand, count, extra) = {
        let state = hand.cell.borrow();
        (state.resource.clone(), state.count, state.extra.clone())
    };
    *game.held_mut() = in_hand.filter(|_| count > 0).map(|resource| stack_of(game, &resource, u32::try_from(count).unwrap_or(0)));
    for (resource, count) in extra {
        let given = stack_of(game, &resource, u32::try_from(count).unwrap_or(0));
        // `ItemAccess.forInfiniteMaterials`: no criativo o item novo só entra se o jogador ainda não tem um igual.
        if infinite && game.native_count_items(&given.item) > 0 {
            continue;
        }
        game.give_or_drop(given);
    }
    Some(true)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O item: o que ele traz ao ser posto e o que leva ao ser quebrado (`loot_table/blocks/*_fluid_tank.json`: `mekanism:edit_mode`, `owner`, `security`, `items` e `fluids`).

fn content_spec() -> basis::ContentSpec<'static> {
    basis::ContentSpec { is_ours: &is_tank, item_slots: SLOTS, collect: &content_collect, apply: &content_apply }
}

/// O fluido do tile (`ContainerType.FLUID`, o componente `mekanism:fluids`).
fn content_collect(_: &Game<'_>, entity: &ModBlockEntity, _: [i32; 3]) -> Vec<(&'static str, Tag)> {
    let Some(tier) = tier_of(&entity.id) else {
        return Vec::new();
    };
    let stored = read_stored(entity, tier_values(tier).0);
    let contents = StackContents::new(stored.0.as_deref().map(FluidResource::new), stored.1);
    match content::containers_collect(&[fluids_json(&contents)]) {
        Some(value) => vec![(basis::CONTENT_FLUIDS, basis::value_tag(&value, ""))],
        None => Vec::new(),
    }
}

/// O contrário: o fluido do item entra no tile (`copyToContainer`).
fn content_apply(entity: &mut ModBlockEntity, of: &dyn Fn(&str) -> Option<Tag>) {
    let Some(tier) = tier_of(&entity.id) else {
        return;
    };
    if let Some(fluids) = of(basis::CONTENT_FLUIDS)
        && let Some(list) = content::containers_apply(&basis::tag_value(&fluids), 1)
    {
        let stored = &list[0];
        let amount = stored["amount"].as_u64().unwrap_or(0);
        let id = stored["type"]["id"].as_str().map(str::to_owned).filter(|_| amount > 0);
        write_stored(entity, tier_values(tier).0, &(id, amount));
    }
}

fn stack_mode(game: &Game<'_>, stack: &ItemStack) -> Option<ContainerEditMode> {
    match game.native_component(stack, rules::EDIT_MODE_COMPONENT) {
        Ok(Some(Tag::String(name))) => ContainerEditMode::from_serialized_name(&name.to_string_lossy()),
        _ => None,
    }
}

/// O item que coloca o tanque (`setPlacedBy`/`applyImplicitComponents`): o fluido, os dois slots e o modo de edição.
pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`.
    basis::on_added(game, pos);
    basis::content_placed(game, stack, pos, &content_spec());
    let mode = stack_mode(game, stack);
    let key = (game.native_dimension(), pos);
    // O objeto vivo de um tanque que esteve aqui antes não é o deste.
    LIVE.with_borrow_mut(|live| live.remove(&key));
    TANKS.with_borrow_mut(|tanks| tanks.remove(&key));
    match (game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])), mode) {
        (Some(BlockEntity::Mod(entity)), Some(mode)) => glue::put(entity, EDIT_MODE, Tag::String(JavaString::from(mode.serialized_name()))),
        (Some(BlockEntity::Mod(_)), None) => {}
        (_, Some(mode)) => PENDING_MODE.with_borrow_mut(|pending| {
            pending.insert(pos, mode);
        }),
        (_, None) => PENDING_MODE.with_borrow_mut(|pending| {
            pending.remove(&pos);
        }),
    }
}

/// O tile vai ser quebrado: o conteúdo e o modo de edição ficam guardados para o item, e o objeto vivo sai.
pub(in crate::game) fn destroying(game: &mut Game<'_>, pos: [i32; 3]) {
    basis::content_destroying(game, pos, &content_spec());
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        let mode = read_mode(entity);
        BROKEN_MODE.with_borrow_mut(|broken| broken.insert(pos, mode));
    }
    let key = (game.native_dimension(), pos);
    LIVE.with_borrow_mut(|live| live.remove(&key));
    TANKS.with_borrow_mut(|tanks| tanks.remove(&key));
}

/// `copy_components` da tabela de loot: o `mekanism:fluids` e o `mekanism:items` (pelo conteúdo da `machinery_basis`) e o `mekanism:edit_mode`, que só sai do padrão do item
/// (`BOTH`) quando o tile tem outro.
pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    if !drops.iter().any(|stack| is_tank(&stack.item)) {
        return;
    }
    basis::content_loot(game, context, drops, &content_spec());
    let Some((pos, _)) = context.block else {
        return;
    };
    let mode = BROKEN_MODE.with_borrow_mut(|broken| broken.remove(&pos));
    if let Some(mode) = mode.filter(|mode| *mode != ContainerEditMode::Both) {
        for stack in drops.iter_mut().filter(|stack| is_tank(&stack.item)) {
            if let Err(error) = game.native_set_component(stack, rules::EDIT_MODE_COMPONENT, Tag::String(JavaString::from(mode.serialized_name()))) {
                eprintln!("mekanism: o item do tanque não leva {}: {error}", rules::EDIT_MODE_COMPONENT);
            }
        }
    }
}

/// Os componentes do item do tanque que o jogo precisa conhecer.
pub(in crate::game) fn component_types(types: &mut Vec<String>) {
    types.push(rules::EDIT_MODE_COMPONENT.to_owned());
    types.push(rules::BUCKET_MODE_COMPONENT.to_owned());
}
