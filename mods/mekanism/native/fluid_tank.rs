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
use machinery_basis_core::bucket_item::{CauldronHandler, CauldronLevel, CauldronOutcome, UseHit, UseOutcome, cauldron_interact, dispense_as_bucket, use_as_bucket};
use machinery_basis_core::item_access::{AccessResourceHandler, HandAccess, ItemAccess, ItemHandlerLookup, ItemResourceHandler, OneByOne, PerItemRules, move_first};
use machinery_basis_core::item_content as content;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::tank::Tank;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::transaction::Transaction;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::ops::ControlFlow;

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
/// `NeoForgeMod.MILK`.
const MILK: &str = "minecraft:milk";
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
    /// O que o último `ItemBlockFluidTank.use` (ou o caldeirão) devolveu no turno desta mão, para o invólucro de [`use_hand`].
    static USE_RESULT: Cell<Option<UseOutcome>> = const { Cell::new(None) };
    /// A marca de reentrada do invólucro de [`use_hand`].
    static IN_USE_HAND: Cell<bool> = const { Cell::new(false) };
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
    items_as(false)
}

/// O mesmo, com o handler do item do tanque agindo como `MANUAL` (`AutomatedResourceHandler.manual`, o de `getOneByOneFluidHandler`) quando `manual`.
fn items_as(manual: bool) -> ItemHandlerLookup<FluidResource> {
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
            let handler = ComponentBackedHandler::new(access, codec.clone(), spec_of.clone(), clock.clone());
            let handler: Box<dyn ItemResourceHandler<FluidResource> + '_> = Box::new(if manual { handler.manual() } else { handler });
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
    reconcile_column(game, dimension, pos);
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

/// Os outros tanques da coluna que o tick deste alcança: o de baixo, e os de cima por onde sobe o que não coube.
fn column_others(dimension: &'static str, pos: [i32; 3]) -> Vec<[i32; 3]> {
    let mut others = vec![[pos[0], pos[1] - 1, pos[2]]];
    let mut y = pos[1] + 1;
    while live_at(&(dimension, [pos[0], y, pos[2]])).is_some() {
        others.push([pos[0], y, pos[2]]);
        y += 1;
    }
    others
}

/// Quem mexeu no NBT de outro tanque da coluna por fora depois do último tick dele (um comando de dados) vale antes do que este tick põe nele: sem isto, o
/// [`sync_column`] gravaria por cima a soma do conteúdo antigo com o que chegou, e o tick do outro não veria a mudança (o mesmo caso do `neighbour()` do Chemical Tank).
fn reconcile_column(game: &Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    for other in column_others(dimension, pos) {
        let Some(cell) = live_at(&(dimension, other)) else {
            continue;
        };
        let Ok(mut live) = cell.try_borrow_mut() else {
            continue;
        };
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(other[0], other[1], other[2])) else {
            continue;
        };
        let Some(tier) = tier_of(&entity.id) else {
            continue;
        };
        let in_nbt = read_stored(entity, tier_values(tier).0);
        if in_nbt != live.seen {
            set_stored(live.tile.tank(), &in_nbt);
            live.seen = in_nbt;
        }
    }
}

/// O que este tick pôs nos outros tanques da coluna (o de baixo, e os de cima por onde subiu o que não coube) vai ao NBT deles agora: o objeto vivo de cada um só o
/// gravaria no próprio tick, e até lá o mundo salvo teria o fluido que saiu daqui sem o que chegou lá.
fn sync_column(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    for other in column_others(dimension, pos) {
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

/// `BlockFluidTank.useItemOn` depois do `tryWrench`, sem agachar: `FluidUtil.interactWithFluidHandler` com a capability da face clicada. `Some(true)` se algo passou
/// (`SUCCESS`), `Some(false)` para quem não tem acesso (`FAIL`), `None` se nada passou (`TRY_WITH_EMPTY_HAND`).
pub(in crate::game) fn bucket_click(game: &mut Game<'_>, pos: [i32; 3], top: bool) -> Option<bool> {
    let stack = game.held().cloned()?;
    if !basis::security_access_or_error(game, pos, &|id| is_tank(id)) {
        return Some(false);
    }
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    let infinite = creative(game);
    let hand = hand_access(game, &stack, infinite, false);
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
    game.audio.play(&bucket_sound(&resource.id, pickup), Some(center(pos)), 1.0, 1.0);
    game.post_block_game_event(if pickup { "minecraft:fluid_pickup" } else { "minecraft:fluid_place" }, pos, Some(horizon_client::mobs::Who::Player), None);
    hand_apply(game, &hand, infinite);
    Some(true)
}

fn center(pos: [i32; 3]) -> [f64; 3] {
    [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5]
}

fn creative(game: &Game<'_>) -> bool {
    game.native_acting_player().is_some_and(|player| player.game_mode == 1)
}

/// `FluidType.getSound(BUCKET_FILL)` e `BUCKET_EMPTY` de um fluido.
fn bucket_sound(fluid: &str, pickup: bool) -> String {
    match (fluid, pickup) {
        ("minecraft:lava", true) => "minecraft:item.bucket.fill_lava".to_owned(),
        ("minecraft:lava", false) => "minecraft:item.bucket.empty_lava".to_owned(),
        (other, _) => match machinery_basis_core::liquid_data::current().iter().find(|spec| spec.id == other) {
            Some(spec) if pickup => spec.fill_sound.clone(),
            Some(spec) => spec.empty_sound.clone(),
            None if pickup => "minecraft:item.bucket.fill".to_owned(),
            None => "minecraft:item.bucket.empty".to_owned(),
        },
    }
}

/// `ItemAccess.forPlayerInteraction(player, hand)`: o que sobra de uma troca numa pilha vai para o inventário; no modo criativo (`infinite`) a mão não muda, e o item novo
/// só é dado se os slots principais ainda não têm um igual (o item e os componentes: `ResourceHandlerUtil.contains(mainSlots, resource)`). Com `side_effect_free`
/// (`ItemAccessUtils.sideEffectFreeAccess`, o do caldeirão no criativo) nada é dado.
fn hand_access(game: &Game<'_>, stack: &ItemStack, infinite: bool, side_effect_free: bool) -> HandAccess {
    let main: Vec<ItemResource> = game.inventory.iter().flatten().map(|held| resource_of(game, held)).collect();
    HandAccess::new(
        Some(resource_of(game, stack)),
        i32::try_from(stack.count).unwrap_or(i32::MAX),
        infinite,
        Rc::new(move |resource| side_effect_free || main.contains(resource)),
    )
}

/// O que a troca deixou na mão e o que transbordou para o inventário.
fn hand_apply(game: &mut Game<'_>, hand: &HandAccess, infinite: bool) {
    let state = hand.state();
    if !infinite {
        *game.held_mut() = state.resource.filter(|_| state.count > 0).map(|resource| stack_of(game, &resource, u32::try_from(state.count).unwrap_or(0)));
    }
    for (resource, count) in state.extra {
        let given = stack_of(game, &resource, u32::try_from(count).unwrap_or(0));
        game.give_or_drop(given);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O item usado no mundo (`ItemBlockFluidTank`): o modo balde.

/// `getMode(stack)`: o `mekanism:bucket_mode` de um item de Fluid Tank.
pub(in crate::game) fn bucket_mode(game: &Game<'_>, stack: &ItemStack) -> bool {
    is_tank(&stack.item) && matches!(game.native_component(stack, rules::BUCKET_MODE_COMPONENT), Ok(Some(Tag::Byte(value))) if *value != 0)
}

/// `setMode`: o padrão do item (`false`) é a falta do componente.
pub(in crate::game) fn set_bucket_mode(game: &Game<'_>, stack: &mut ItemStack, mode: bool) {
    let result = if mode { game.native_set_component(stack, rules::BUCKET_MODE_COMPONENT, Tag::Byte(1)) } else { game.native_remove_component(stack, rules::BUCKET_MODE_COMPONENT).map(|_| ()) };
    if let Err(error) = result {
        eprintln!("mekanism: o item do tanque não leva {}: {error}", rules::BUCKET_MODE_COMPONENT);
    }
}

/// `tryClaimItem` e `canAccessOrDisplayError` no começo de `use` e de `interactLivingEntity` (`ItemBlockFluidTank.java:82-87` e `:141-146`): `Some` encerra o uso.
fn claim_or_refuse(game: &mut Game<'_>, stack: &ItemStack) -> Option<UseOutcome> {
    let mut owned = stack.clone();
    if basis::security_item_claim(game, &mut owned) {
        *game.held_mut() = Some(owned);
        return Some(UseOutcome::Success);
    }
    if !basis::security_item_access_or_error(game, stack) {
        return Some(UseOutcome::Fail);
    }
    None
}

/// `ItemBlockFluidTank.use` (`:135-181`) com o item no modo balde: o primeiro uso só toma posse; depois, agachado o item põe um balde no bloco da mira (ou no vizinho
/// da face), e em pé tira um balde da fonte da mira.
fn item_use(game: &mut Game<'_>, stack: &ItemStack) -> UseOutcome {
    if let Some(outcome) = claim_or_refuse(game, stack) {
        return outcome;
    }
    let sneaking = game.native_sneaking();
    // `getPlayerPOVHitResult`: `ClipContext.Fluid.NONE` agachado, `SOURCE_ONLY` em pé (a fonte do vanilla ou a de um fluido de mod, a que vier antes do bloco).
    let hit = if sneaking {
        game.pick().map(|hit| UseHit { pos: hit.pos, offset: hit.beside() })
    } else {
        match crate::game::machinery_liquid::source_hit(game) {
            Some(cell) => Some(UseHit { pos: cell, offset: [cell[0], cell[1] + 1, cell[2]] }),
            None => game.bucket_hit().map(|hit| UseHit { pos: hit.pos, offset: hit.beside() }),
        }
    };
    // `level.mayInteract` e `player.mayUseItemAt`, como o balde do Horizon os confere.
    let may_use = game.may_build();
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    let infinite = creative(game);
    let hand = hand_access(game, stack, infinite, false);
    let lookup = items_as(true);
    let outcome = {
        let one = OneByOne(&hand);
        let held = lookup(&one);
        let mut world = crate::game::machinery_liquid::BucketWorld { game: &mut *game, by_player: true };
        let transaction = Transaction::open_safe();
        let outcome = use_as_bucket(held.as_deref(), sneaking, hit, may_use, &mut world, &transaction);
        if outcome == UseOutcome::Success {
            transaction.commit();
        }
        outcome
    };
    if outcome == UseOutcome::Success {
        hand_apply(game, &hand, infinite);
    }
    outcome
}

/// `FluidTankCauldronInteraction.interact` (`ItemBlockFluidTank.java:283-357`): o item no modo balde troca fluido com o caldeirão em `pos`. Devolve se algo passou
/// (`SUCCESS`); sem isso é `TRY_WITH_EMPTY_HAND`, e o clique segue.
fn cauldron_use(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) -> bool {
    let Some(before) = cauldron_level(game, pos) else {
        return false;
    };
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    let infinite = creative(game);
    let hand = hand_access(game, stack, infinite, true);
    let cauldron = CauldronHandler::new(before);
    let lookup = items_as(true);
    let outcome = {
        let one = OneByOne(&hand);
        let held = lookup(&one);
        let transaction = Transaction::open_safe();
        let outcome = cauldron_interact(held.as_deref(), &cauldron, &transaction);
        if outcome != CauldronOutcome::TryWithEmptyHand {
            transaction.commit();
        }
        outcome
    };
    let CauldronOutcome::Success { fluid, filled_cauldron } = outcome else {
        return false;
    };
    // `CauldronWrapper.onRootCommit`: o estado novo do caldeirão vai ao mundo.
    let after = cauldron.state();
    let (block, level) = match after.fluid.as_ref().map(|fluid| &*fluid.id) {
        None => ("minecraft:cauldron", None),
        Some("minecraft:water") => ("minecraft:water_cauldron", Some(after.level.to_string())),
        Some(_) => ("minecraft:lava_cauldron", None),
    };
    let new = game.with_level(|_, world, _| {
        let base = world.registry.default_state(block)?;
        match &level {
            Some(level) => world.native_state_with_property(base, "level", level),
            None => Some(base),
        }
    });
    if let Some(new) = new {
        let _ = game.level_set(pos, new);
    }
    game.award_custom_stat(if filled_cauldron { "fill_cauldron" } else { "use_cauldron" }, 1);
    game.award_stat("used", &stack.item, 1);
    game.audio.play(&bucket_sound(&fluid.id, !filled_cauldron), Some(center(pos)), 1.0, 1.0);
    game.post_block_game_event(if filled_cauldron { "minecraft:fluid_place" } else { "minecraft:fluid_pickup" }, pos, None, None);
    hand_apply(game, &hand, infinite);
    true
}

/// O gancho `use_item_on` (onde começa `ItemStack.useOn`, depois de o bloco ter a vez): o caldeirão responde ao item no modo balde (`CauldronInteraction`, que no original
/// é do `useItemOn` do bloco), e `ItemBlockFluidTank.useOn` passa (`:126-133`: no modo balde o item não põe o bloco). O clique de um convidado só chega até aqui (o `use`
/// vem no pacote seguinte): o gancho o toma para o bloco não ser posto, e o invólucro de [`use_hand`] desfaz o balanço da mão.
pub(in crate::game) fn use_item_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], claimed: &mut bool) {
    if *claimed || !bucket_mode(game, stack) {
        return;
    }
    // `Player.isSecondaryUseActive`: agachado com algo na mão o bloco não tem a vez.
    if !game.native_sneaking() && cauldron_use(game, stack, pos) {
        USE_RESULT.set(Some(UseOutcome::Success));
        *claimed = true;
        return;
    }
    if matches!(game.guest_use, Some(crate::game::GuestUse::OnBlock(_))) {
        USE_RESULT.set(Some(UseOutcome::Pass));
        *claimed = true;
    }
}

/// O gancho `use_item` (onde começa `Item.use`): o item no modo balde age como balde e nunca põe o bloco.
pub(in crate::game) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || !bucket_mode(game, stack) {
        return;
    }
    let outcome = item_use(game, stack);
    USE_RESULT.set(Some(outcome));
    // `PASS` só sai sem bloco na mira, e então o Horizon não tem bloco para pôr; com um bloco na mira o gancho toma o clique de qualquer jeito.
    *claimed = outcome != UseOutcome::Pass || game.pick().is_some();
}

/// O invólucro do turno de uma mão (`Game::use_hand`, o HEAD cancelável do Configurator, H72), para o item no modo balde: o gancho `use_item` só sabe tomar o clique, e
/// quem o toma balança a mão. O turno roda de novo por dentro, com a marca de reentrada, e aqui o `FAIL` (`use_failed`, sem balançar a mão) e o `PASS` (a outra mão tem a
/// vez) do original voltam a ser o que são.
pub(in crate::game) fn use_hand(game: &mut Game<'_>, hit: Option<&crate::game::Hit>) -> Option<ControlFlow<crate::game::Result<bool>>> {
    if IN_USE_HAND.get() || !game.held().is_some_and(|stack| bucket_mode(game, stack)) {
        return None;
    }
    IN_USE_HAND.set(true);
    USE_RESULT.set(None);
    let swing = (game.swing, game.swings_started);
    let result = game.use_hand(hit);
    IN_USE_HAND.set(false);
    let outcome = USE_RESULT.take();
    Some(ControlFlow::Break(match (outcome, result) {
        (Some(UseOutcome::Fail), Ok(_)) => {
            (game.swing, game.swings_started) = swing;
            game.use_failed = true;
            Ok(true)
        }
        (Some(UseOutcome::Pass), Ok(_)) => {
            (game.swing, game.swings_started) = swing;
            Ok(false)
        }
        (_, result) => result,
    }))
}

// O cliente. `MultiPlayerGameMode.useItemOn` e `useItem` rodam o item também no cliente: o `useOn` do item no modo balde passa (o bloco não é posto e o `UseItem` segue),
// e o resultado do `use` decide se a mão balança. O cliente do Horizon só prevê os itens dele: um item de bloco encerra o clique com a previsão do bloco posto, e o
// `UseItem` nunca sai. As duas injeções cruas `tank_use_item_on` e `tank_use_item` (HEAD canceláveis de `remote_use_item_on` e `remote_use_item`, com marca de reentrada)
// rodam a função de novo por dentro e respondem pelo item.

thread_local! {
    static IN_CLIENT_USE_ON: Cell<bool> = const { Cell::new(false) };
    static IN_CLIENT_USE: Cell<bool> = const { Cell::new(false) };
}

/// Um item que não é de bloco, não se usa e a que nenhum bloco do vanilla responde: fica na mão do cliente só durante a chamada de dentro de `remote_use_item_on`, para
/// ela não prever o bloco posto (`BlockItem.useOn`). Nenhum pacote leva o item da mão.
const CLIENT_STAND_IN: &str = "minecraft:stick";

/// O nível do caldeirão em `pos` (`CauldronFluidContent`): o vazio, a água em três níveis e a lava; o de neve em pó não tem capability de fluido.
fn cauldron_level(game: &Game<'_>, pos: [i32; 3]) -> Option<CauldronLevel> {
    let (state, _) = game.cauldron_at(pos)?;
    let name: &str = &state.name;
    match name {
        "minecraft:cauldron" => Some(CauldronLevel::default()),
        "minecraft:water_cauldron" => {
            Some(CauldronLevel { fluid: Some(FluidResource::new("minecraft:water")), level: state.properties.get("level").and_then(|level| level.parse().ok()).unwrap_or(0) })
        }
        "minecraft:lava_cauldron" => Some(CauldronLevel { fluid: Some(FluidResource::new("minecraft:lava")), level: 1 }),
        _ => None,
    }
}

/// O que `ItemBlockFluidTank.use` devolveria no cliente, sem mudar nada: a posse no primeiro uso, e depois o balde sobre o mundo que o cliente vê.
fn client_use_outcome(game: &Game<'_>, stack: &ItemStack) -> UseOutcome {
    if !matches!(game.native_component(stack, machinery_basis_core::security::OWNER_COMPONENT), Ok(Some(_))) {
        return UseOutcome::Success;
    }
    let sneaking = game.native_sneaking();
    let hit = if sneaking {
        game.pick().map(|hit| UseHit { pos: hit.pos, offset: hit.beside() })
    } else {
        match crate::game::machinery_liquid::source_hit(game) {
            Some(cell) => Some(UseHit { pos: cell, offset: [cell[0], cell[1] + 1, cell[2]] }),
            None => game.bucket_hit().map(|hit| UseHit { pos: hit.pos, offset: hit.beside() }),
        }
    };
    let hand = hand_access(game, stack, game.creative, false);
    let lookup = items_as(true);
    let one = OneByOne(&hand);
    let held = lookup(&one);
    let mut world = crate::game::machinery_liquid::DryBucketWorld { game };
    // A transação não é confirmada: nada do que o item mudou fica.
    let transaction = Transaction::open_safe();
    use_as_bucket(held.as_deref(), sneaking, hit, game.may_build(), &mut world, &transaction)
}

/// `FluidTankCauldronInteraction.interact` no cliente: se a troca com o caldeirão da mira fecharia (`SUCCESS`: a mão balança e o `UseItem` não sai).
fn client_cauldron_succeeds(game: &Game<'_>, stack: &ItemStack) -> bool {
    if game.native_sneaking() {
        return false;
    }
    let Some(level) = game.pick().and_then(|hit| cauldron_level(game, hit.pos)) else {
        return false;
    };
    let hand = hand_access(game, stack, game.creative, true);
    let cauldron = CauldronHandler::new(level);
    let lookup = items_as(true);
    let one = OneByOne(&hand);
    let held = lookup(&one);
    let transaction = Transaction::open_safe();
    cauldron_interact(held.as_deref(), &cauldron, &transaction) != CauldronOutcome::TryWithEmptyHand
}

/// O começo de `tank_use_item_on`: com o item no modo balde na mão `hand`, guarda a pilha, deixa o substituto no lugar e diz se o caldeirão responderia. `None` fora
/// do caso (ou na chamada de dentro).
pub(in crate::game) fn client_use_on_begin(game: &mut Game<'_>, hand: horizon_client::first_person::Hand) -> Option<(ItemStack, bool)> {
    if IN_CLIENT_USE_ON.get() {
        return None;
    }
    let stack = game.with_hand(hand, |game| game.held().cloned()).filter(|stack| bucket_mode(game, stack))?;
    let cauldron = game.with_hand(hand, |game| client_cauldron_succeeds(game, &stack));
    IN_CLIENT_USE_ON.set(true);
    game.with_hand(hand, |game| *game.held_mut() = Some(ItemStack::new(CLIENT_STAND_IN, 1)));
    Some((stack, cauldron))
}

/// O fim de `tank_use_item_on`: a pilha volta à mão. Devolve se o clique acaba aqui: o bloco respondeu (`done`), ou o caldeirão trocou (a mão balança).
pub(in crate::game) fn client_use_on_end(game: &mut Game<'_>, hand: horizon_client::first_person::Hand, saved: (ItemStack, bool), done: bool) -> bool {
    IN_CLIENT_USE_ON.set(false);
    let (stack, cauldron) = saved;
    game.with_hand(hand, |game| *game.held_mut() = Some(stack));
    if !done && cauldron {
        game.with_hand(hand, Game::swing_arm);
        return true;
    }
    done
}

/// O começo de `tank_use_item`: com o item no modo balde na mão `hand`, o que o `use` devolveria. `None` fora do caso (ou na chamada de dentro).
pub(in crate::game) fn client_use_begin(game: &mut Game<'_>, hand: horizon_client::first_person::Hand) -> Option<UseOutcome> {
    if IN_CLIENT_USE.get() {
        return None;
    }
    let stack = game.with_hand(hand, |game| game.held().cloned()).filter(|stack| bucket_mode(game, stack))?;
    let outcome = game.with_hand(hand, |game| client_use_outcome(game, &stack));
    IN_CLIENT_USE.set(true);
    Some(outcome)
}

/// O fim de `tank_use_item`: um `SUCCESS` balança a mão e encerra o clique (`SwingSource.CLIENT`); o `FAIL` e o `PASS` deixam a outra mão ter a vez.
pub(in crate::game) fn client_use_end(game: &mut Game<'_>, hand: horizon_client::first_person::Hand, outcome: UseOutcome, done: bool) -> bool {
    IN_CLIENT_USE.set(false);
    if done {
        return true;
    }
    if outcome == UseOutcome::Success {
        game.with_hand(hand, Game::swing_arm);
        return true;
    }
    false
}

/// `ItemBlockFluidTank.interactLivingEntity` (`:77-109`): o item no modo balde ordenha a vaca, o cogumelo-vaca e a cabra adultos. O primeiro uso só toma posse; depois o
/// leite (`NeoForgeMod.MILK`) entra no item, um balde ou o que couber, com o som do bicho.
pub(in crate::game) fn interact_entity(game: &mut Game<'_>, entity: i32, stack: Option<&ItemStack>, claimed: &mut bool) {
    use horizon_client::mobs::Species;
    let Some(stack) = stack else {
        return;
    };
    if *claimed || !bucket_mode(game, stack) {
        return;
    }
    let Some((species, baby, screaming)) = u32::try_from(entity).ok().and_then(|id| game.mobs.iter().find(|mob| mob.id == id)).map(|mob| (mob.species(), mob.is_baby(), mob.extra.screaming)) else {
        return;
    };
    if baby {
        return;
    }
    match claim_or_refuse(game, stack) {
        Some(UseOutcome::Success) => {
            *claimed = true;
            return;
        }
        // `FAIL` não consome a ação: o clique segue.
        Some(_) => return,
        None => {}
    }
    // `getMilkSound`.
    let sound = match species {
        Species::Goat if screaming => "minecraft:entity.goat.screaming.milk",
        Species::Goat => "minecraft:entity.goat.milk",
        Species::Mooshroom => "minecraft:entity.mooshroom.milk",
        Species::Cow => "minecraft:entity.cow.milk",
        _ => return,
    };
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    let infinite = creative(game);
    let hand = hand_access(game, stack, infinite, false);
    let lookup = items_as(true);
    let inserted = {
        let one = OneByOne(&hand);
        let Some(held) = lookup(&one) else {
            return;
        };
        let transaction = Transaction::open_safe();
        let inserted = held.insert(&FluidResource::new(MILK), BUCKET_VOLUME, &transaction);
        if inserted > 0 {
            transaction.commit();
        }
        inserted
    };
    // "Fail if we can't insert any, we allow partial bucket amounts for milking though".
    if inserted == 0 {
        return;
    }
    let at = game.native_acting_player().map(|player| player.position);
    game.audio.play(sound, at, 1.0, 1.0);
    hand_apply(game, &hand, infinite);
    *claimed = true;
}

/// `FluidTankItemDispenseBehavior.execute` (`:223-266`): no dispensador o item no modo balde pega a fonte da frente ou põe um balde ali; se nada passa, ou fora do modo
/// balde, ele é solto como um item comum.
pub(in crate::game) fn dispense(
    game: &mut Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    facing: horizon_core::geometry::Direction,
    stack: &ItemStack,
    answer: &mut Option<horizon_client::mod_api::NativeDispensed>,
) {
    use horizon_core::geometry::Direction;
    if answer.is_some() || !is_tank(&stack.item) {
        return;
    }
    if !bucket_mode(game, stack) {
        *answer = Some((game.native_dispense_default(dimension, pos, facing, stack.clone()), 1000));
        return;
    }
    let offset = match facing {
        Direction::Down => [0, -1, 0],
        Direction::Up => [0, 1, 0],
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
    };
    let target = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    CLOCK.with(|clock| clock.set(game.world.game_time as u64));
    // `ItemStacksResourceHandler(2)` com a pilha no primeiro slot: o que não cabe nele vai para o de sobra.
    let hand = HandAccess::new(Some(resource_of(game, stack)), i32::try_from(stack.count).unwrap_or(i32::MAX), false, Rc::new(|_| false));
    let source = crate::game::machinery_liquid::is_source(game, target);
    let lookup = items_as(true);
    let moved = {
        let one = OneByOne(&hand);
        let Some(held) = lookup(&one) else {
            return;
        };
        let mut world = crate::game::machinery_liquid::BucketWorld { game: &mut *game, by_player: false };
        let transaction = Transaction::open_safe();
        let moved = dispense_as_bucket(held.as_ref(), &mut world, target, source, &transaction);
        if moved.is_some() {
            transaction.commit();
        }
        moved
    };
    if moved.is_none() {
        *answer = Some((game.native_dispense_default(dimension, pos, facing, stack.clone()), 1000));
        return;
    }
    // `consumeWithRemainder(source, stack0, stack1)` com o `stack0.grow(1)` antes: o primeiro slot fica como está, e o de sobra entra no dispensador ou é solto.
    let state = hand.state();
    let kept = state.resource.filter(|_| state.count > 0).map(|resource| stack_of(game, &resource, u32::try_from(state.count).unwrap_or(0)));
    for (resource, count) in state.extra {
        let extra = stack_of(game, &resource, u32::try_from(count).unwrap_or(0));
        if let Some(left) = dispenser_insert(game, pos, extra) {
            let _ = game.native_dispense_default(dimension, pos, facing, left);
        }
    }
    *answer = Some((kept, 1000));
}

/// `DispenserBlockEntity.insertItem` (o de `addToInventoryOrDispense`): a pilha entra nos slots do dispensador, e o que não coube volta.
fn dispenser_insert(game: &mut Game<'_>, pos: [i32; 3], stack: ItemStack) -> Option<ItemStack> {
    let max = game.res.items().get(&stack.item).map_or(64, |item| item.max_stack).max(1);
    let mut rest = stack;
    let entity = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2]))?;
    for cell in entity.slots_mut() {
        if rest.count == 0 {
            break;
        }
        match cell {
            None => {
                let moved = rest.count.min(max);
                *cell = Some(ItemStack { count: moved, ..rest.clone() });
                rest.count -= moved;
            }
            Some(held) if held.same_item_same_components(&rest) => {
                let moved = rest.count.min(max.saturating_sub(held.count));
                held.count += moved;
                rest.count -= moved;
            }
            Some(_) => {}
        }
    }
    (rest.count > 0).then_some(rest)
}

/// `ItemBlockFluidTank.changeMode` na pilha da mão: devolve a mudança, com a pilha já trocada.
pub(in crate::game) fn change_mode(game: &mut Game<'_>, stack: &ItemStack, shift: i32, display: rules::DisplayChange) -> rules::BucketModeChange {
    let change = rules::bucket_mode_change(bucket_mode(game, stack), shift, display);
    if change.changed {
        let mut next = stack.clone();
        set_bucket_mode(game, &mut next, change.mode);
        *game.held_mut() = Some(next);
    }
    change
}

/// As linhas dos appenders do item do Fluid Tank (o fluido, a capacidade do tier, o inventário e o modo balde), entre a segurança e a descrição.
pub(in crate::game) fn tooltip_lines(game: &Game<'_>, stack: &ItemStack) -> Vec<rules::Chat> {
    let Some(tier) = tier_of(&stack.item) else {
        return Vec::new();
    };
    let contents = stack_fluids(game, stack);
    let stored = contents.resource.as_ref().filter(|_| contents.amount > 0).map(|resource| (&*resource.id, contents.amount));
    let items = game.native_component(stack, basis::CONTENT_ITEMS).ok().flatten().map(basis::tag_value);
    rules::tank_tooltip_lines(&rules::TankTooltip {
        chemical: false,
        tier,
        stored,
        capacity: tier_values(tier).0,
        inventory: content::has_inventory(items.as_ref()),
        bucket_mode: Some(bucket_mode(game, stack)),
    })
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
