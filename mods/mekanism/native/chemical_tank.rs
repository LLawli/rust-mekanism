// Os Chemical Tanks no jogo (`TileEntityChemicalTank`). As regras ficam em machinery_basis_core::chemical_tank (o tile, os dois slots, o descarte, o ejetor de químico, a
// configuração de lados e o comparador) e em machinery_basis_core::component_container (o tanque que mora no item); aqui ficam os valores do tier, o NBT, os componentes do
// item, os vizinhos e os cliques.
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{JavaString, Tag};
use machinery_basis_core::chemical::{ChemicalResource, TankContents};
use machinery_basis_core::chemical_tank::{self, ChemicalNeighbours, ChemicalTankConfig, ChemicalTankTile, GasMode};
use machinery_basis_core::component_container::{AttachedCodec, ComponentBackedHandler, ComponentTankSpec, ComponentTankSpecOf};
use machinery_basis_core::components::Components;
use machinery_basis_core::config_window::{self, ConfigPacket, ConfigView};
use machinery_basis_core::container_sync::{StackValue, TrackedSource, Value};
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::ejector::TileComponentEjector;
use machinery_basis_core::slot_rules;
use machinery_basis_core::tile_trackers::source;
use machinery_basis_core::upgrade_window::UpgradeWindowSpec;
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::item_access::{ItemAccess, ItemHandlerLookup, ItemResourceHandler};
use machinery_basis_core::item_content as content;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::side_config::TileComponentConfig;
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::transaction::Transaction;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Os slots do tile, na ordem de `getInitialInventory`: o que enche o item (`drain`) e o que o esvazia (`fill`).
const SLOTS: usize = 2;
const REDSTONE_OWNER: &str = "mekanism_chemical_tank";
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const CURRENT_REDSTONE: &str = "current_redstone";
/// `SerializationConstants.DUMP_MODE`.
const DUMPING: &str = "dumping";
/// Um objeto vivo que não anda por tantos ticks sai da tabela (o bloco foi quebrado ou o chunk descarregado).
const STALE_TICKS: i64 = 100;
const SWEEP_TICKS: i64 = 200;

type Key = (&'static str, [i32; 3]);

/// O objeto vivo de um tanque. O Horizon só guarda a `ModBlockEntity`; o que o Java guarda no objeto (o contêiner com o limite de taxa, o ejetor, a configuração de lados)
/// fica aqui, entre os ticks, e o NBT leva o que o Java salva.
struct LiveTank {
    tile: ChemicalTankTile,
    last_seen: i64,
    /// O químico que o NBT tinha na última vez que este objeto o viu ou escreveu: o que difere disso veio de fora (um comando de dados, o item posto).
    seen: TankContents,
    /// O que a configuração de lados e o ejetor tinham no NBT na última vez que este objeto os viu ou escreveu.
    side_nbt: basis::SideNbt,
    /// `redstone`, o modo de controle e o nível do comparador que o NBT já tem.
    scalars: Option<(bool, RedstoneControl, i32)>,
    /// O modo de descarte que o NBT já tem.
    dumping: Option<GasMode>,
}

thread_local! {
    static LIVE: RefCell<HashMap<Key, Rc<RefCell<LiveTank>>>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
    /// `MekanismUtils.GAME_TIME_SUPPLIER`: o tick do jogo, para o limite de taxa do tanque que mora no item.
    static CLOCK: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    /// O conteúdo e a pilha de origem de cada item de tanque que passou por aqui, pela chave dos componentes dele.
    static ITEM_CHEMICALS: RefCell<HashMap<String, (TankContents, ItemStack)>> = RefCell::new(HashMap::new());
    /// O modo de descarte do item que acabou de ser posto, para o tile que o Horizon ainda não criou.
    static PENDING_DUMP: RefCell<HashMap<[i32; 3], GasMode>> = RefCell::new(HashMap::new());
}

pub(in crate::game) fn tier_of(block: &str) -> Option<usize> {
    rules::chemical_tank_tier(block)
}

pub(in crate::game) fn is_tank(block: &str) -> bool {
    tier_of(block).is_some()
}

/// `tier.getCapacity()` e `tier.getTransferRate()`, os do `tiers.toml`.
fn tier_values(tier: usize) -> (u64, i32) {
    let config = glue::server_config();
    (u64::try_from(config.chemical_tanks[tier]).unwrap_or(0), config.chemical_tank_rates[tier])
}

/// Como a configuração de lados do tanque é montada e o padrão do item dele (`ItemBlockChemicalTank.SIDE_CONFIG`).
pub(in crate::game) fn side_spec(block: &str) -> Option<basis::SideSpec> {
    is_tank(block).then(|| basis::SideSpec { spec: chemical_tank::side_config_spec(), default_side_config: rules::chemical_tank_default() })
}

// ---------------------------------------------------------------------------------------------------------------------------
// A capability de químico dos itens (`Capabilities.CHEMICAL.item()`): hoje só o item do próprio tanque.

/// O `mekanism:chemicals` do item do tanque. O conteúdo de um recurso que a lib fez (o item depois de encher ou esvaziar) não passou por uma pilha do jogo: fica na tabela,
/// pela chave dos componentes, com a pilha de origem, e vira pilha em [`stack_of`].
struct TankItems;

fn chemicals_text(contents: &TankContents) -> String {
    match &contents.resource {
        Some(resource) => format!("{{containers:[{{amount:{}L,type:\"{}\"}}]}}", contents.amount, resource.id),
        None => String::new(),
    }
}

impl AttachedCodec<ChemicalResource> for TankItems {
    fn contents(&self, item: &ItemResource) -> TankContents {
        if item.components.get(basis::CONTENT_CHEMICALS).is_none() {
            return StackContents::default();
        }
        ITEM_CHEMICALS.with_borrow(|table| table.get(item.components.key()).map(|(contents, _)| contents.clone())).unwrap_or_default()
    }

    fn with_contents(&self, item: &ItemResource, contents: &TankContents) -> Option<ItemResource> {
        let text = chemicals_text(contents);
        let mut entries: Vec<(&str, &str)> = item.components.iter().filter(|(id, _)| *id != basis::CONTENT_CHEMICALS).collect();
        if !contents.is_empty() {
            entries.push((basis::CONTENT_CHEMICALS, text.as_str()));
        }
        let components = Components::new(entries);
        let origin = ITEM_CHEMICALS
            .with_borrow(|table| table.get(item.components.key()).map(|(_, stack)| stack.clone()))
            .or_else(|| basis::stack_of_resource(item, 1))
            .unwrap_or_else(|| ItemStack::new(&item.id, 1));
        if !components.is_empty() {
            ITEM_CHEMICALS.with_borrow_mut(|table| table.insert(components.key().to_owned(), (contents.clone(), origin)));
        }
        Some(ItemResource::with_components(&item.id, item.max_stack_size, components))
    }
}

/// O que um `mekanism:chemicals` guarda.
fn tag_chemicals(tag: &Tag) -> TankContents {
    let Some(list) = content::containers_apply(&basis::tag_value(tag), 1) else {
        return StackContents::default();
    };
    let stored = &list[0];
    let amount = stored["amount"].as_u64().unwrap_or(0);
    StackContents::new(stored["type"].as_str().map(ChemicalResource::new), amount)
}

/// O que o `mekanism:chemicals` de uma pilha do jogo guarda. Sem o jogo (o `can_take` do Horizon), o componente vem do que a pilha traz.
fn stack_chemicals(game: Option<&Game<'_>>, stack: &ItemStack) -> TankContents {
    let tag = match game {
        Some(game) => game.native_component(stack, basis::CONTENT_CHEMICALS).ok().flatten().cloned(),
        None => stack.data().and_then(|data| data.other(basis::CONTENT_CHEMICALS).cloned()),
    };
    tag.map(|tag| tag_chemicals(&tag)).unwrap_or_default()
}

fn chemicals_json(contents: &TankContents) -> serde_json::Value {
    match &contents.resource {
        Some(resource) if contents.amount > 0 => serde_json::json!({"type": &*resource.id, "amount": contents.amount}),
        _ => serde_json::json!({}),
    }
}

/// Grava o conteúdo no `mekanism:chemicals` da pilha; vazio é o padrão do item, e o componente sai.
fn set_stack_chemicals(game: &Game<'_>, stack: &mut ItemStack, contents: &TankContents) {
    let result = match content::containers_collect(&[chemicals_json(contents)]) {
        Some(value) => game.native_set_component(stack, basis::CONTENT_CHEMICALS, basis::value_tag(&value, "")),
        None => game.native_remove_component(stack, basis::CONTENT_CHEMICALS),
    };
    if let Err(error) = result {
        eprintln!("mekanism: o item do tanque não leva {}: {error}", basis::CONTENT_CHEMICALS);
    }
}

/// Registra para o handler o químico de uma pilha de tanque, pela chave dos componentes do recurso dela.
fn note_item(game: Option<&Game<'_>>, stack: &ItemStack, resource: &ItemResource) {
    if is_tank(&stack.item) && !resource.components.is_empty() {
        let contents = stack_chemicals(game, stack);
        ITEM_CHEMICALS.with_borrow_mut(|table| table.insert(resource.components.key().to_owned(), (contents, stack.clone())));
    }
}

/// O recurso de uma pilha do jogo, com o químico dela registrado para o handler se for um item de tanque.
fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let resource = basis::item_resource(game, stack);
    note_item(Some(game), stack, &resource);
    resource
}

/// A pilha de um recurso: a que passou pelo jogo, ou, para o que a lib fez, a de origem com o químico novo.
fn stack_of(game: &Game<'_>, resource: &ItemResource, count: u32) -> ItemStack {
    let made = ITEM_CHEMICALS.with_borrow(|table| table.get(resource.components.key()).cloned());
    if let Some((contents, origin)) = made.filter(|_| is_tank(&resource.id)) {
        let mut stack = origin;
        set_stack_chemicals(game, &mut stack, &contents);
        stack.count = count;
        return stack;
    }
    if let Some(stack) = basis::stack_of_resource(resource, count) {
        return stack;
    }
    ItemStack::new(&resource.id, count)
}

/// `Capabilities.CHEMICAL.item()`.
fn items() -> ItemHandlerLookup<ChemicalResource> {
    let codec: Rc<dyn AttachedCodec<ChemicalResource>> = Rc::new(TankItems);
    // `ComponentBackedChemicalTankChemicalTank.create`: o tier do item dá a capacidade, a taxa e o criativo.
    let spec_of: ComponentTankSpecOf<ChemicalResource> = Rc::new(|item: &ItemResource| {
        let tier = tier_of(&item.id).unwrap_or_default();
        let (capacity, rate) = tier_values(tier);
        ComponentTankSpec::open(capacity, rate, tier == rules::TANK_CREATIVE)
    });
    let clock = CLOCK.with(Rc::clone);
    Rc::new(move |access: &dyn ItemAccess| {
        let held = access.resource()?;
        if !is_tank(&held.id) {
            return None;
        }
        let handler: Box<dyn ItemResourceHandler<ChemicalResource> + '_> = Box::new(ComponentBackedHandler::new(access, codec.clone(), spec_of.clone(), clock.clone()));
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

/// O tanque do NBT, sem o que está vazio.
fn read_stored(entity: &ModBlockEntity) -> TankContents {
    let stored = glue::read_tank(entity);
    if stored.resource.is_some() && stored.amount > 0 { stored } else { StackContents::default() }
}

fn stored_of(tile: &ChemicalTankTile) -> TankContents {
    let contents = tile.tank().inner().contents();
    if contents.resource.is_some() && contents.amount > 0 { contents } else { StackContents::default() }
}

fn set_stored(tile: &ChemicalTankTile, stored: &TankContents) {
    tile.tank().inner().set_contents(stored.resource.clone(), stored.amount, None);
}

/// `readSustainedData`: sem a chave, `IDLE`.
fn read_dumping(entity: &ModBlockEntity) -> GasMode {
    glue::nbt_string(entity, DUMPING).and_then(|name| GasMode::from_serialized_name(&name)).unwrap_or_default()
}

/// O tile do que a `ModBlockEntity` guarda, virado para `facing` (`loadAdditional`).
fn tile_of(entity: &ModBlockEntity, tier: usize, facing: Face) -> (ChemicalTankTile, TankContents, basis::SideNbt) {
    let (capacity, rate) = tier_values(tier);
    let mut tile = ChemicalTankTile::new(ChemicalTankConfig {
        capacity,
        rate,
        creative: tier == rules::TANK_CREATIVE,
        dump_excess_keep_ratio: rules::DUMP_EXCESS_KEEP_RATIO,
        attributes: glue::chemical_attributes(),
        items: items(),
    });
    let scalars = read_scalars(entity);
    tile.tile_mut().restore(
        &machinery_basis_core::tile::SavedState { redstone: scalars.0, control_type: scalars.1, current_redstone: scalars.2, ..Default::default() },
        false,
    );
    tile.set_facing(facing);
    let side = basis::read_side_nbt(entity);
    tile.restore_side_config(side.config.as_ref(), side.ejector.as_ref(), false);
    let seen = read_stored(entity);
    set_stored(&tile, &seen);
    tile.set_dumping(read_dumping(entity));
    (tile, seen, side)
}

fn born(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, tier: usize) -> LiveTank {
    let (tile, seen, side_nbt) = tile_of(entity, tier, basis::front(game, pos));
    LiveTank { tile, last_seen: i64::MIN, seen, side_nbt, scalars: None, dumping: None }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que o Horizon deixa ver.

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    if is_tank(block) {
        // `AttributeRedstone`: o jogo avisa o bloco quando um vizinho muda.
        basis::watch_redstone(REDSTONE_OWNER, is_tank);
        register_windows(block);
        if entity.is_none() {
            *entity = Some((block.to_owned(), horizon_world::nbt::Compound::default()));
        }
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_tank(block) {
        *ticks = true;
        // A capability de químico do tanque, que os tubos pressurizados alcançam pelo contêiner, face a face (`setupIOConfig(TransmissionType.CHEMICAL, chemicalTank)`).
        for id in rules::CHEMICAL_TANK_TIERS {
            crate::game::machinery_resources::chemical_neighbor_register(id, probe, write);
        }
    }
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_tank(block) {
        // Os dois slots; as faces, as entradas e as saídas são da instância (H2), pela configuração de lados de `ITEM`. O menu é a tela `GuiChemicalTank` (`screen` do glue).
        *container = Some(ModContainer { slots: SLOTS, menu: Some(block.to_owned()), ..Default::default() });
    }
}

fn live_at(key: &Key) -> Option<Rc<RefCell<LiveTank>>> {
    LIVE.with(|live| live.try_borrow().ok().and_then(|live| live.get(key).cloned()))
}

/// O tanque como a rede de químico o vê por um lado: o contêiner do objeto vivo. `None` sem capability ali, ou com o tanque no próprio tick.
fn probe(
    game: &Game<'_>,
    dimension: &'static str,
    pos: [i32; 3],
    side: Face,
) -> Option<machinery_basis_core::transmission::host_resource::ResourceSnapshot<ChemicalResource>> {
    let cell = live_at(&(dimension, pos))?;
    // Quem mexeu no NBT por fora depois do último tick do tanque (um comando de dados): quem lê o tanque antes do tick dele já vê o conteúdo novo, como o
    // `loadAdditional` do original (ver o `probe` do Fluid Tank).
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2]))
        && let Ok(mut live) = cell.try_borrow_mut()
    {
        let in_nbt = read_stored(entity);
        if in_nbt != live.seen {
            set_stored(&live.tile, &in_nbt);
            live.seen = in_nbt;
        }
    }
    let live = cell.try_borrow().ok()?;
    rules::chemical_tank_view(&live.tile, side)
}

/// O que a rede mudou no tanque.
fn write(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], contents: TankContents) {
    let stored = if contents.resource.is_some() && contents.amount > 0 { contents } else { StackContents::default() };
    if let Some(cell) = live_at(&(dimension, pos))
        && let Ok(mut live) = cell.try_borrow_mut()
    {
        set_stored(&live.tile, &stored);
        live.seen = stored.clone();
    }
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        glue::write_tank(entity, &stored);
    }
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
}

// ---------------------------------------------------------------------------------------------------------------------------
// As faces e as regras de slot de quem está de fora (H2): o Horizon pergunta ao contêiner da instância, e o tanque responde pelo que o tile guarda.

/// `ConfigHolder` do tanque: o `component_config` do NBT do tile, com a frente do bloco.
pub(in crate::game) fn item_faces(block: &str, entity: &ModBlockEntity, front: Face) -> Option<machinery_basis_core::item_instance::InstanceItems> {
    let spec = side_spec(block)?.spec;
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    if let Some(data) = &basis::read_side_nbt(entity).config {
        config.deserialize(data, front, false);
    }
    Some(machinery_basis_core::item_instance::InstanceItems::of_config(&config, front, &[]))
}

/// `isValidForExtraction(item, EXTERNAL)` do slot (`ChemicalInventorySlot.drain` e `.fill`), sem o jogo (o `can_take` do Horizon).
pub(in crate::game) fn item_extractable(block: &str, entity: &ModBlockEntity, slot: usize, stack: &ItemStack, limit: u32) -> bool {
    let Some(tier) = tier_of(block) else {
        return false;
    };
    let (tile, _, _) = tile_of(entity, tier, Face::North);
    let resource = basis::item_resource_with_limit(stack, limit);
    note_item(None, stack, &resource);
    tile.inventory_slots().get(slot).is_some_and(|held| held.is_valid_for_extraction(&resource, AutomationType::External))
}

/// `isValidForInsertion(item, EXTERNAL)` do slot pela face `face`: a face deixa entrar e o slot aceita o item.
pub(in crate::game) fn item_accepts(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, slot: usize, stack: &ItemStack, face: Face) -> bool {
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let front = basis::front(game, pos);
    if !item_faces(&entity.id, entity, front).is_some_and(|items| items.accepts_through(face, slot)) {
        return false;
    }
    let (tile, _, _) = tile_of(entity, tier, front);
    let resource = resource_of(game, stack);
    tile.inventory_slots().get(slot).is_some_and(|held| held.is_valid_for_insertion(&resource, AutomationType::External))
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

// ---------------------------------------------------------------------------------------------------------------------------
// Os vizinhos do ejetor (`Capabilities.CHEMICAL` do bloco do outro lado, pela face oposta). Hoje só outro Chemical Tank; os tubos e as máquinas ficam para a frente dos
// vizinhos que empurram.

/// A capability de químico de um tanque vizinho, pela face dele que encosta neste.
struct Receiver {
    tank: Rc<RefCell<LiveTank>>,
    face: Face,
}

impl ResourceInsert<ChemicalResource> for Receiver {
    fn insert(&self, resource: &ChemicalResource, amount: i32, transaction: &Transaction) -> i32 {
        match self.tank.try_borrow() {
            Ok(live) => live.tile.handler_insert(self.face, resource, amount, transaction).unwrap_or(0),
            Err(_) => 0,
        }
    }
}

type Inserter = Rc<dyn ResourceInsert<ChemicalResource>>;

/// A capability de químico de cada vizinho, pela face dele que olha para o tanque (`BlockCapabilityCache`), pedida só quando o tile a usa: outro Chemical Tank (o
/// objeto vivo dele), um tubo pressurizado (o contêiner da rede dele) ou uma máquina com tanque de químico, pela capability que a `machinery_basis` dá a quem empurra.
struct Around<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
    pos: [i32; 3],
    now: i64,
    /// O que cada lado já respondeu neste tick: o espelho de um vizinho só vale enquanto ninguém o refaz.
    made: RefCell<[Option<Option<Inserter>>; 6]>,
    /// Os vizinhos que não são tanques e receberam um handler: o que entrou neles vai ao jogo depois do tick.
    pushed: RefCell<Vec<([i32; 3], Rc<Pushed>)>>,
}

/// O handler de um tubo ou de uma máquina vizinha.
struct Pushed(crate::game::machinery_resources::PushTarget<ChemicalResource>);

impl ResourceInsert<ChemicalResource> for Pushed {
    fn insert(&self, resource: &ChemicalResource, amount: i32, transaction: &Transaction) -> i32 {
        self.0.insert(resource, amount, transaction)
    }
}

impl Around<'_, '_> {
    fn make(&self, side: Face) -> Option<Inserter> {
        let mut game = self.game.borrow_mut();
        let other = offset(self.pos, side);
        let opposite = side.opposite();
        if is_tank(&game.native_block(other)) {
            let cell = neighbour(&**game, self.dimension, other, self.now)?;
            if !cell.try_borrow().is_ok_and(|live| live.tile.has_chemical_capability(opposite)) {
                return None;
            }
            let handler: Inserter = Rc::new(Receiver { tank: cell, face: opposite });
            return Some(handler);
        }
        let target = crate::game::machinery_resources::chemical_push_target(&mut **game, self.dimension, other, opposite)?;
        let pushed = Rc::new(Pushed(target));
        self.pushed.borrow_mut().push((other, pushed.clone()));
        Some(pushed)
    }
}

impl ChemicalNeighbours for Around<'_, '_> {
    fn handler(&self, side: Face) -> Option<Inserter> {
        if let Some(known) = &self.made.borrow()[side as usize] {
            return known.clone();
        }
        let made = self.make(side);
        self.made.borrow_mut()[side as usize] = Some(made.clone());
        made
    }
}

fn offset(pos: [i32; 3], face: Face) -> [i32; 3] {
    let step = face.offset();
    [pos[0] + step[0], pos[1] + step[1], pos[2] + step[2]]
}

/// O objeto vivo do tanque vizinho em `pos`: o que já anda, ou um feito agora do NBT (o tile do Java existe desde que o bloco é posto), que o tick dele mantém.
fn neighbour(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], now: i64) -> Option<Rc<RefCell<LiveTank>>> {
    let key = (dimension, pos);
    if let Some(cell) = live_at(&key)
        && cell.try_borrow().is_ok_and(|live| now.saturating_sub(live.last_seen) <= 1)
    {
        // Quem mexeu no NBT do vizinho por fora depois do último tick dele (um comando de dados) vale antes do que este tick põe nele: sem isto, o `sync_neighbours`
        // gravaria por cima a soma do conteúdo antigo com o que chegou, e o tick do vizinho não veria a mudança.
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2]))
            && let Ok(mut live) = cell.try_borrow_mut()
        {
            let in_nbt = read_stored(entity);
            if in_nbt != live.seen {
                set_stored(&live.tile, &in_nbt);
                live.seen = in_nbt;
            }
        }
        return Some(cell);
    }
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    let tier = tier_of(&entity.id)?;
    let mut live = born(game, pos, entity, tier);
    live.last_seen = now - 1;
    let cell = Rc::new(RefCell::new(live));
    LIVE.with(|table| {
        if let Ok(mut table) = table.try_borrow_mut() {
            table.insert(key, cell.clone());
        }
    });
    Some(cell)
}

/// O que este tick pôs nos tanques vizinhos vai ao NBT deles agora: o objeto vivo de cada um só o gravaria no próprio tick, e até lá o mundo salvo teria o químico que saiu
/// daqui sem o que chegou lá.
fn sync_neighbours(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    for face in Face::ALL {
        let other = offset(pos, face);
        let Some(cell) = live_at(&(dimension, other)) else {
            continue;
        };
        let Ok(mut live) = cell.try_borrow_mut() else {
            continue;
        };
        let stored = stored_of(&live.tile);
        if stored == live.seen {
            continue;
        }
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(other[0], other[1], other[2])) else {
            continue;
        };
        if !is_tank(&entity.id) {
            continue;
        }
        glue::write_tank(entity, &stored);
        live.seen = stored;
        game.world.modified.insert((other[0] >> 4, other[2] >> 4));
    }
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
    basis::side_apply_pending(entity, pos);
    basis::content_apply_pending(entity, pos, &content_spec());
    if let Some(mode) = PENDING_DUMP.with_borrow_mut(|pending| pending.remove(&pos)) {
        glue::put(entity, DUMPING, Tag::String(JavaString::from(mode.serialized_name())));
    }
    CLOCK.with(|clock| clock.set(now as u64));
    let cell = match live_at(&key) {
        Some(cell) if cell.borrow().last_seen + 1 == now && glue::has_saved(entity) => cell,
        _ => {
            let cell = Rc::new(RefCell::new(born(game, pos, entity, tier)));
            LIVE.with_borrow_mut(|live| live.insert(key, cell.clone()));
            cell
        }
    };
    let mut live = cell.borrow_mut();
    slot_from_entity(game, live.tile.drain_slot().slot(), entity.items[chemical_tank::DRAIN_SLOT].as_ref());
    slot_from_entity(game, live.tile.fill_slot().slot(), entity.items[chemical_tank::FILL_SLOT].as_ref());
    // Quem mexeu no NBT por fora (um comando de dados, o item posto) muda o contêiner, o modo de descarte e a configuração.
    let in_nbt = read_stored(entity);
    if in_nbt != live.seen {
        set_stored(&live.tile, &in_nbt);
        live.seen = in_nbt;
    }
    if let Some(saved) = live.dumping {
        let mode = read_dumping(entity);
        if mode != saved {
            live.tile.set_dumping(mode);
        }
    }
    if let Some(powered) = basis::take_power(game, pos) {
        live.tile.tile_mut().update_power(powered);
    }
    live.tile.set_facing(basis::front(game, pos));
    let side_in_nbt = basis::read_side_nbt(entity);
    if side_in_nbt != live.side_nbt {
        live.tile.restore_side_config(side_in_nbt.config.as_ref(), side_in_nbt.ejector.as_ref(), true);
        live.side_nbt = side_in_nbt;
    }

    let (result, pushed) = {
        let neighbours = Around { game: RefCell::new(&mut *game), dimension, pos, now, made: RefCell::new(Default::default()), pushed: RefCell::new(Vec::new()) };
        let result = live.tile.tick_server(now as u64, &neighbours);
        (result, neighbours.pushed.into_inner())
    };
    for (other, target) in &pushed {
        crate::game::machinery_resources::chemical_push_flush(game, dimension, *other, &target.0);
    }
    live.tile.take_side_changes();
    glue::publish_comparator(game, dimension, pos, live.tile.tile().current_redstone_level(), result.comparator_changed);

    slot_to_entity(game, entity, chemical_tank::DRAIN_SLOT, live.tile.drain_slot().slot());
    slot_to_entity(game, entity, chemical_tank::FILL_SLOT, live.tile.fill_slot().slot());
    let stored = stored_of(&live.tile);
    if stored != live.seen {
        glue::write_tank(entity, &stored);
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
    let dumping = live.tile.dumping();
    if live.dumping != Some(dumping) {
        glue::put(entity, DUMPING, Tag::String(JavaString::from(dumping.serialized_name())));
        live.dumping = Some(dumping);
    }
    let side_now = basis::SideNbt::of(live.tile.side_config().serialize(), live.tile.ejector().serialize());
    if side_now != live.side_nbt {
        basis::write_side_nbt(entity, &side_now);
        live.side_nbt = basis::read_side_nbt(entity);
    }
    crate::game::machinery_resources::chemical_faces_set(dimension, pos, &entity.id, rules::chemical_tank_faces(&live.tile));
    if entity.data.is_empty() {
        entity.data = vec![1];
    }
    live.last_seen = now;
    drop(live);
    sync_neighbours(game, dimension, pos);
    if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
        LAST_SWEEP.set(now);
        LIVE.with_borrow_mut(|live| live.retain(|_, tank| tank.try_borrow().map_or(true, |tank| now.saturating_sub(tank.last_seen) <= STALE_TICKS)));
    }
}

/// Corre `act` no tile em `pos`: o objeto vivo, ou um feito do NBT que não fica na tabela. O que `act` mudou (o modo de descarte, a configuração de lados, o ejetor e o
/// modo de redstone) vai ao NBT na hora, e as faces de químico são avisadas aos tubos. Quem usa: o Configurator e o cartão de configuração.
pub(in crate::game) fn with_tile<R>(game: &mut Game<'_>, pos: [i32; 3], act: impl FnOnce(&mut ChemicalTankTile) -> R) -> Option<R> {
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
            Rc::new(RefCell::new(born(game, pos, entity, tier)))
        }
    };
    let mut live = cell.try_borrow_mut().ok()?;
    let result = act(&mut live.tile);
    live.tile.take_side_changes();
    let side_now = basis::SideNbt::of(live.tile.side_config().serialize(), live.tile.ejector().serialize());
    let dumping = live.tile.dumping();
    let control = live.tile.tile().control_type();
    let faces = rules::chemical_tank_faces(&live.tile);
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        basis::write_side_nbt(entity, &side_now);
        live.side_nbt = basis::read_side_nbt(entity);
        glue::put(entity, DUMPING, Tag::String(JavaString::from(dumping.serialized_name())));
        live.dumping = Some(dumping);
        glue::put(entity, CONTROL_TYPE, Tag::String(JavaString::from(control.serialized_name())));
        if let Some(scalars) = live.scalars.as_mut() {
            scalars.1 = control;
        }
        crate::game::machinery_resources::chemical_faces_set(dimension, pos, &entity.id, faces);
    }
    game.world.modified.insert((at.0 >> 4, at.2 >> 4));
    Some(result)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O Configurator e o cartão de configuração.

/// `TileEntityChemicalTank.writeConfigurationData`: o modo de redstone e o de descarte (`writeSustainedData`), `component_config` e `component_ejector`.
pub(in crate::game) fn write_configuration_data(game: &Game<'_>, pos: [i32; 3]) -> Option<horizon_world::nbt::Compound> {
    let live = live_at(&(game.native_dimension(), pos)).and_then(|cell| cell.try_borrow().ok().map(|live| live.tile.write_configuration_data()));
    let (data, dumping) = match live {
        Some(data) => data,
        None => {
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
                return None;
            };
            let tier = tier_of(&entity.id)?;
            born(game, pos, entity, tier).tile.write_configuration_data()
        }
    };
    let mut out = horizon_world::nbt::Compound::default();
    if let Some(control) = data.control_type {
        glue::put_compound(&mut out, CONTROL_TYPE, Tag::String(JavaString::from(control.serialized_name())));
    }
    glue::put_compound(&mut out, DUMPING, Tag::String(JavaString::from(dumping.serialized_name())));
    basis::side_nbt_into(&mut out, &basis::SideNbt { config: data.config, ejector: data.ejector });
    Some(out)
}

/// `TileEntityChemicalTank.setConfigurationData`.
pub(in crate::game) fn set_configuration_data(game: &mut Game<'_>, pos: [i32; 3], input: &horizon_world::nbt::Compound) -> bool {
    let side = basis::side_nbt_from(input);
    let control_type = match input.get(&JavaString::from(CONTROL_TYPE)) {
        Some(Tag::String(name)) => Some(RedstoneControl::from_serialized_name(&name.to_string_lossy())),
        _ => None,
    };
    let dumping = match input.get(&JavaString::from(DUMPING)) {
        Some(Tag::String(name)) => GasMode::from_serialized_name(&name.to_string_lossy()),
        _ => None,
    };
    let data = machinery_basis_core::tile::ConfigurationData { control_type, config: side.config, ejector: side.ejector };
    with_tile(game, pos, |tile| tile.set_configuration_data(&data, dumping)).is_some()
}

// ---------------------------------------------------------------------------------------------------------------------------
// O item: o que ele traz ao ser posto e o que leva ao ser quebrado (`loot_table/blocks/*_chemical_tank.json`: `mekanism:dump_mode`, `owner`, `security`, `side_config`,
// `ejector`, `items` e `chemicals`).

fn content_spec() -> basis::ContentSpec<'static> {
    basis::ContentSpec { is_ours: &is_tank, item_slots: SLOTS, collect: &content_collect, apply: &content_apply }
}

/// O químico do tile (`ContainerType.CHEMICAL`, o componente `mekanism:chemicals`) e o modo de descarte (`collectImplicitComponents`), que só sai do padrão do item
/// (`IDLE`) quando o tile tem outro.
fn content_collect(_: &Game<'_>, entity: &ModBlockEntity, _: [i32; 3]) -> Vec<(&'static str, Tag)> {
    let mut out = glue::chemical_collect(entity);
    let dumping = read_dumping(entity);
    if dumping != GasMode::Idle {
        out.push((rules::DUMP_MODE_COMPONENT, Tag::String(JavaString::from(dumping.serialized_name()))));
    }
    out
}

/// O contrário: o químico do item entra no tile (`copyToContainer`).
fn content_apply(entity: &mut ModBlockEntity, of: &dyn Fn(&str) -> Option<Tag>) {
    glue::chemical_apply(entity, of);
}

fn stack_dumping(game: &Game<'_>, stack: &ItemStack) -> Option<GasMode> {
    match game.native_component(stack, rules::DUMP_MODE_COMPONENT) {
        Ok(Some(Tag::String(name))) => GasMode::from_serialized_name(&name.to_string_lossy()),
        _ => None,
    }
}

/// O item que coloca o tanque (`setPlacedBy`/`applyImplicitComponents`): o químico, os dois slots e o modo de descarte. A configuração de lados, o dono e o nome são do glue.
pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`.
    basis::on_added(game, pos);
    basis::content_placed(game, stack, pos, &content_spec());
    // `input.getOrDefault(DUMP_MODE, IDLE)`.
    let mode = stack_dumping(game, stack).unwrap_or_default();
    let dimension = game.native_dimension();
    // O objeto vivo de um tanque que esteve aqui antes não é o deste.
    LIVE.with_borrow_mut(|live| live.remove(&(dimension, pos)));
    match game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => {
            glue::put(entity, DUMPING, Tag::String(JavaString::from(mode.serialized_name())));
            PENDING_DUMP.with_borrow_mut(|pending| pending.remove(&pos));
        }
        _ => PENDING_DUMP.with_borrow_mut(|pending| {
            pending.insert(pos, mode);
        }),
    }
    // O tile novo guarda `currentRedstoneLevel = 0`.
    glue::publish_comparator(game, dimension, pos, 0, false);
}

/// O tile vai ser quebrado: o conteúdo fica guardado para o item, e o objeto vivo sai.
pub(in crate::game) fn destroying(game: &mut Game<'_>, pos: [i32; 3]) {
    basis::content_destroying(game, pos, &content_spec());
    let key = (game.native_dimension(), pos);
    LIVE.with_borrow_mut(|live| live.remove(&key));
}

/// [`destroying`] para o tile que o nível destruiu e já saiu do mundo (`basis::level_drop_flush`): `entity` é a cópia tirada antes.
pub(in crate::game) fn leaving(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity) {
    basis::content_leaving(game, entity, pos, &content_spec());
    let key = (game.native_dimension(), pos);
    LIVE.with_borrow_mut(|live| live.remove(&key));
}

/// `copy_components` da tabela de loot: o `mekanism:chemicals`, o `mekanism:items` e o `mekanism:dump_mode`.
pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    if !drops.iter().any(|stack| is_tank(&stack.item)) {
        return;
    }
    basis::content_loot(game, context, drops, &content_spec());
}

/// As linhas dos appenders do item do Chemical Tank (o químico, a capacidade do tier e o inventário), entre a segurança e a descrição.
pub(in crate::game) fn tooltip_lines(game: &Game<'_>, stack: &ItemStack) -> Vec<rules::Chat> {
    let Some(tier) = tier_of(&stack.item) else {
        return Vec::new();
    };
    let contents = stack_chemicals(Some(game), stack);
    let stored = contents.resource.as_ref().filter(|_| contents.amount > 0).map(|resource| (&*resource.id, contents.amount));
    let items = game.native_component(stack, basis::CONTENT_ITEMS).ok().flatten().map(basis::tag_value);
    rules::tank_tooltip_lines(&rules::TankTooltip {
        chemical: true,
        tier,
        stored,
        capacity: tier_values(tier).0,
        inventory: machinery_basis_core::item_content::has_inventory(items.as_ref()),
        bucket_mode: None,
    })
}

/// Os componentes do item do tanque que o jogo precisa conhecer.
pub(in crate::game) fn component_types(types: &mut Vec<String>) {
    types.push(rules::DUMP_MODE_COMPONENT.to_owned());
}

// ---------------------------------------------------------------------------------------------------------------------------
// A tela (`GuiChemicalTank`): o menu, os dados dele, as abas de lados e de transporte e o botão do modo de descarte. O tanque não é `IUpgradeTile`: não há janela de upgrades.

/// A janela do tanque: só dados. A mecânica (abas, janelas de configuração, sessões do menu) é da machinery_basis; `upgrades: false` tira a aba e os slots virtuais.
const fn window_spec_of(block: &'static str) -> UpgradeWindowSpec {
    UpgradeWindowSpec {
        block,
        upgrades: false,
        recipe_book: false,
        book: None,
        book_slots: [0, 0],
        virtual_slots: [SLOTS, SLOTS + 1],
        trackers: rules::chemical_tank_trackers,
        window_trackers: rules::upgrade_window_data,
        ns: "mekanism",
        window_type: rules::UPGRADE_WINDOW,
        window_types: &rules::WINDOW_TYPES,
        base_slice: rules::PANEL_SLICE,
        max_multiplier: machinery_basis_core::settings::max_upgrade_multiplier,
        client_config: rules::CLIENT_CONFIG_FILE,
        data: glue::window_upgrade_data,
        color: rules::enum_color,
        item: rules::UPGRADE_ITEM,
        item_component: rules::UPGRADE_TYPE_COMPONENT,
    }
}

static WINDOW_SPECS: [UpgradeWindowSpec; 5] = [
    window_spec_of(rules::CHEMICAL_TANK_TIERS[0]),
    window_spec_of(rules::CHEMICAL_TANK_TIERS[1]),
    window_spec_of(rules::CHEMICAL_TANK_TIERS[2]),
    window_spec_of(rules::CHEMICAL_TANK_TIERS[3]),
    window_spec_of(rules::CHEMICAL_TANK_TIERS[4]),
];

fn register_windows(block: &str) {
    if let Some(spec) = WINDOW_SPECS.iter().find(|spec| spec.block == block) {
        Game::mb_window_register(spec, remove_upgrade, add_upgrades, window_source);
        Game::mb_config_register(spec.block, config_view, config_apply, config_snapshot);
    }
}

/// O tanque não tem upgrades: o item de upgrade não entra e nada sai.
fn remove_upgrade(_: &mut Game<'_>, _: [i32; 3], _: &str, _: bool) {}

fn add_upgrades(_: &mut Game<'_>, _: [i32; 3], _: &str, _: i32) -> i32 {
    0
}

/// Os dados do menu do tanque (`TileEntityChemicalTank.addContainerTrackers`), lidos do objeto.
struct TankSync<'a> {
    tile: &'a ChemicalTankTile,
    security: machinery_basis_core::security::SecuritySync,
}

impl TrackedSource for TankSync<'_> {
    fn value(&self, id: u16) -> Value {
        let redstone = self.tile.tile().redstone_sync();
        match id {
            source::SECURITY_FREQUENCY => {
                let (hash, bytes) = self.security.frequency();
                Value::Codec { hash, bytes }
            }
            source::SECURITY_MODE => Value::Enum(self.security.mode()),
            source::CONTROL_TYPE => Value::Enum(redstone.control_type.ordinal()),
            source::POWERED => Value::Boolean(redstone.powered),
            source::WAS_POWERED => Value::Boolean(redstone.was_powered),
            source::CHEMICAL_TANK => {
                let stored = stored_of(self.tile);
                Value::Stack(StackValue { resource: stored.resource.map(|resource| resource.id.to_string()).unwrap_or_default(), amount: stored.amount })
            }
            rules::SOURCE_TANK_MODE => Value::Enum(self.tile.dumping().ordinal() as i32),
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

/// O valor vivo dos dados do menu do tanque em `pos` (`MekanismContainer.broadcastChanges`): o objeto vivo, ou, até o primeiro tick do tile, um feito do NBT. Um aviso de
/// redstone que o tile ainda não leu entra na hora, como o `updatePower` do Java.
fn window_source(game: &mut Game<'_>, pos: [i32; 3], act: &mut dyn FnMut(&dyn TrackedSource)) -> bool {
    let at = (pos[0], pos[1], pos[2]);
    let ticked = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if is_tank(&entity.id) => !entity.data.is_empty(),
        _ => return false,
    };
    let power = if ticked { basis::take_power(game, pos) } else { basis::peek_power(game, pos) };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
        return false;
    };
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let security = basis::security_sync(game, entity);
    let cell = match live_at(&(game.native_dimension(), pos)).filter(|_| ticked) {
        Some(cell) => cell,
        None => Rc::new(RefCell::new(born(game, pos, entity, tier))),
    };
    let Ok(mut live) = cell.try_borrow_mut() else {
        return false;
    };
    if let Some(powered) = power {
        live.tile.tile_mut().update_power(powered);
    }
    act(&TankSync { tile: &live.tile, security });
    true
}

/// O que as janelas de configuração leem do tanque em `pos`: a configuração e o ejetor do NBT do tile, e a frente de agora.
fn config_view(game: &Game<'_>, pos: [i32; 3]) -> Option<ConfigView> {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    tier_of(&entity.id)?;
    let spec = chemical_tank::side_config_spec();
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    let mut ejector = TileComponentEjector::new();
    ejector.set_output_data(&config, &spec.eject);
    let facing = basis::front(game, pos);
    let nbt = basis::read_side_nbt(entity);
    if let Some(data) = &nbt.config {
        config.deserialize(data, facing, false);
    }
    if let Some(data) = &nbt.ejector {
        ejector.deserialize(data);
    }
    Some(ConfigView { config, ejector, facing })
}

/// O estado vivo da configuração e do ejetor, que o servidor manda a quem tem uma janela de configuração aberta.
fn config_snapshot(game: &mut Game<'_>, pos: [i32; 3]) -> Option<Vec<u8>> {
    let live = live_at(&(game.native_dimension(), pos))
        .and_then(|cell| cell.try_borrow().ok().map(|live| config_window::encode_snapshot(live.tile.side_config(), live.tile.ejector())));
    if live.is_some() {
        return live;
    }
    let view = config_view(game, pos)?;
    Some(config_window::encode_snapshot(&view.config, &view.ejector))
}

/// O `handle` dos pacotes de configuração no servidor: aplica ao tanque (o objeto vivo, ou um feito do NBT) e grava a configuração no NBT na hora, como `sideChanged`.
fn config_apply(game: &mut Game<'_>, pos: [i32; 3], packet: ConfigPacket) {
    let facing = basis::front(game, pos);
    with_tile(game, pos, |tile| {
        tile.set_facing(facing);
        let (config, ejector) = tile.side_parts_mut();
        packet.apply(config, ejector, facing);
    });
}

/// `PacketGuiInteract.GAS_MODE_BUTTON` (`TileEntityChemicalTank.nextMode(0)`, `:125-130`): o modo de descarte anda no objeto vivo e no NBT, de onde um tile que ainda não
/// andou o lê. Quem não tem acesso ao tile é ignorado (`PacketUtils.blockEntity`). Devolve se o botão é desta tela.
pub(in crate::game) fn menu_button(game: &Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) -> bool {
    if !is_tank(&entity.id) || id != rules::TANK_GAS_MODE_BUTTON {
        return false;
    }
    if !basis::gui_interact_allowed(game, entity) {
        return true;
    }
    let live = live_at(&(game.native_dimension(), pos)).and_then(|cell| {
        let mut live = cell.try_borrow_mut().ok()?;
        live.tile.next_mode();
        let now = live.tile.dumping();
        live.dumping = Some(now);
        Some(now)
    });
    let mode = live.unwrap_or_else(|| read_dumping(entity).next());
    glue::put(entity, DUMPING, Tag::String(JavaString::from(mode.serialized_name())));
    true
}

/// `PacketGuiInteract.NEXT_REDSTONE_CONTROL` e `PREVIOUS_REDSTONE_CONTROL`: `setControlType(getControlType().getNext(supportsMode))` no objeto vivo do tanque, e no NBT, de
/// onde um tanque que ainda não andou o lê.
pub(in crate::game) fn step_control(game: &Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, forward: bool) {
    let live = live_at(&(game.native_dimension(), pos)).and_then(|cell| {
        let mut live = cell.try_borrow_mut().ok()?;
        let next = machinery_basis_core::redstone_tab::cycle(live.tile.tile().control_type(), forward);
        live.tile.tile_mut().set_control_type(next);
        if let Some(scalars) = live.scalars.as_mut() {
            scalars.1 = next;
        }
        Some(next)
    });
    let next = live.unwrap_or_else(|| machinery_basis_core::redstone_tab::cycle(basis::redstone_read(entity), forward));
    basis::redstone_write(entity, next);
}

/// O limite de pilha de um item que o hook do menu entrega sem o jogo: não entra na validação do slot.
const UNKNOWN_STACK: u32 = 64;

/// O shift-clique no menu do Chemical Tank (`MekanismContainer.quickMoveStack`): os dois slots do tile na ordem de `getInitialInventory` (o de esvaziar e o de
/// encher), com o `isValidForInsertion(item, MANUAL)` de cada um, a barra de armadura e o inventário. Devolve se o bloco é daqui.
pub(in crate::game) fn quick_move(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, container_slots: usize, slot: usize, stack: &mut ItemStack) -> bool {
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let tile = born(game, pos, entity, tier).tile;
    let accepts = |slot: usize, stack: &ItemStack| {
        let resource = basis::item_resource_with_limit(stack, UNKNOWN_STACK);
        note_item(None, stack, &resource);
        tile.inventory_slots().get(slot).is_some_and(|held| held.is_valid_for_insertion(&resource, AutomationType::Manual))
    };
    let menu = entity.id.clone();
    basis::bar_menu_quick_move(game, entity, &menu, container_slots, slot, stack, &[chemical_tank::DRAIN_SLOT, chemical_tank::FILL_SLOT], &accepts);
    true
}

/// `InventoryContainerSlot.mayPlace` do tanque aberto: o `isValidForInsertion(item, MANUAL)` do slot, com o químico que a pilha do clique guarda. O hook do Horizon roda
/// dentro do menu, sem o jogo: a regra leva um tile feito do NBT agora.
pub(in crate::game) fn publish_slot_rules(game: &Game<'_>, pos: [i32; 3], owner: &'static str) -> bool {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return false;
    };
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let tile = born(game, pos, entity, tier).tile;
    slot_rules::set(
        owner,
        &entity.id,
        Rc::new(move |slot, item, stack| {
            let resource = match stack.downcast_ref::<ItemStack>() {
                Some(stack) => {
                    let resource = basis::item_resource_with_limit(stack, UNKNOWN_STACK);
                    note_item(None, stack, &resource);
                    resource
                }
                None => ItemResource::new(item, UNKNOWN_STACK),
            };
            tile.inventory_slots().get(slot).is_some_and(|held| held.is_valid_for_insertion(&resource, AutomationType::Manual))
        }),
    );
    true
}
