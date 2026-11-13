// Os transportadores logísticos no jogo (frente `transportador-jogo`, Fase 3). O dono dos transportadores (`machinery_basis_core::transmission::inventory_owner`) guarda por dimensão o
// registro, o gerenciador das pilhas em voo e o `LogisticalTransporterBase` de cada transportador; esta parte o liga ao jogo como `resource_glue.rs` liga o dos canos e o dos tubos: os eventos da
// decisão 107 entram nele, as conexões que ele calcula viram as propriedades de lado do bloco, o NBT do transportador (`TileEntityTransmitter.saveAdditional`, a cor, o `mode` do
// desviador e a lista `items` das pilhas em trânsito) sai no salvamento, o que um transportador solta ao quebrar vira item no mundo, e o vizinho com inventário (um baú, um funil,
// uma máquina com slots) responde à rede por um espelho dos slots que o lado alcança (`HostItemHandler`, a decisão 137 do fluido aplicada a itens). A lib não conhece nenhum id: o mod
// diz quais blocos são transportadores e o tipo de cada um em `transporters_register`.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use crate::game::machinery_transmitter_client as client;
use crate::game::machinery_basis::{TransmitterEvent, TransmitterKind, arr, sim_pos};
use horizon_client::containers::BlockEntity;
use horizon_core::random::RandomSource;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::ItemHandler;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos as SimPos;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::host_item::{HostItemHandler, MirrorSlot, SlotChange};
use machinery_basis_core::transmission::inventory_owner::{
    InventoryOwnerWorld, InventoryTransmitters, ItemAcceptor, SavedTransporter, TransporterInfo, TransporterPacket, hopper_outcome, keys,
};
use machinery_basis_core::transmission::owner::{CableView, Event as TransporterEvent};
use machinery_basis_core::transmission::transmitter::Overlay as TransporterOverlay;
use machinery_basis_core::transmission::{DimensionId, Uuid};
use machinery_basis_core::transporter::movement::WorldDrop;
use machinery_basis_core::transporter::nbt::{Compound as LibCompound, Nbt};
use machinery_basis_core::transporter::node::{CapabilityChange, DiversionControl, TransporterKind, TransporterTier};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

/// O tier e o tipo de um bloco de transportador (`LogisticalTransporter`, `RestrictiveTransporter` ou `DiversionTransporter`), ou `None` se o bloco não é um.
pub type TransporterStats = fn(&str) -> Option<(TransporterTier, TransporterKind)>;

/// O nome do dono dos transportadores entre os que ouvem os eventos dos transmissores.
const SINK_OWNER: &str = "machinery_transporters";

static KINDS: Mutex<Vec<TransporterStats>> = Mutex::new(Vec::new());

/// `(dimensão, posição do vizinho, lado dele que olha o transportador)`.
type HandlerKey = (&'static str, [i32; 3], usize);

thread_local! {
    /// Os transportadores de cada dimensão do nível deste servidor.
    static OWNERS: RefCell<HashMap<&'static str, InventoryTransmitters>> = RefCell::new(HashMap::new());
    /// Os vizinhos que o dono guarda como aceitadores (o mesmo objeto que o registro tem no cache de aceitadores).
    static HANDLERS: RefCell<HashMap<HandlerKey, Rc<HostItemHandler>>> = RefCell::new(HashMap::new());
    /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
    static OVERLAYS: RefCell<Vec<TransporterOverlay>> = const { RefCell::new(Vec::new()) };
    /// O último `publish` de cada transportador (o tipo de conexão visível de cada lado), para a mira do Configurator achar as peças de `getCollisionBoxes`.
    static VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
    /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon: `(dimensão, posição, tick)`, tentados de novo.
    static PENDING: RefCell<Vec<(&'static str, [i32; 3], i64)>> = const { RefCell::new(Vec::new()) };
}

/// `id` (o `ModBlockEntity.id`) passa a ser um transportador: os eventos do jogo dele vão ao dono dos transportadores, e `stats` dá o tier e o tipo.
pub fn transporters_register(id: &str, stats: TransporterStats) {
    basis::transmitters_register(SINK_OWNER, id, transporters_event);
    client::register_source(client::TagSource { tag: transporter_tag, positions: transporter_positions });
    let mut kinds = KINDS.lock().unwrap_or_else(|e| e.into_inner());
    if !kinds.iter().any(|known| *known as usize == stats as usize) {
        kinds.push(stats);
    }
}

fn transporter_stats(block: &str) -> Option<(TransporterTier, TransporterKind)> {
    let kinds: Vec<TransporterStats> = KINDS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    kinds.into_iter().find_map(|stats| stats(block))
}

// ---------------------------------------------------------------------------------------------------------------------------
// O NBT: do `Compound` do Horizon para o da lib e de volta (só o que o transportador guarda: números, texto, listas e compostos).

fn tag_to_nbt(tag: &Tag) -> Option<Nbt> {
    Some(match tag {
        Tag::Byte(v) => Nbt::Byte(*v),
        Tag::Short(v) => Nbt::Short(*v),
        Tag::Int(v) => Nbt::Int(*v),
        Tag::Long(v) => Nbt::Long(*v),
        Tag::Float(v) => Nbt::Float(*v),
        Tag::Double(v) => Nbt::Double(*v),
        Tag::String(v) => Nbt::String(v.to_string_lossy()),
        Tag::ByteArray(v) => Nbt::ByteArray(v.clone()),
        Tag::IntArray(v) => Nbt::IntArray(v.clone()),
        Tag::LongArray(v) => Nbt::LongArray(v.clone()),
        Tag::List(items) => Nbt::List(items.iter().filter_map(tag_to_nbt).collect()),
        Tag::Compound(compound) => Nbt::Compound(compound_to_lib(compound)),
        Tag::End => return None,
    })
}

fn compound_to_lib(compound: &Compound) -> LibCompound {
    compound
        .iter()
        .filter_map(|(key, tag)| Some((key.to_string_lossy(), tag_to_nbt(tag)?)))
        .collect()
}

fn nbt_to_tag(nbt: &Nbt) -> Tag {
    match nbt {
        Nbt::Byte(v) => Tag::Byte(*v),
        Nbt::Short(v) => Tag::Short(*v),
        Nbt::Int(v) => Tag::Int(*v),
        Nbt::Long(v) => Tag::Long(*v),
        Nbt::Float(v) => Tag::Float(*v),
        Nbt::Double(v) => Tag::Double(*v),
        Nbt::String(v) => Tag::String(JavaString::from(v.as_str())),
        Nbt::ByteArray(v) => Tag::ByteArray(v.clone()),
        Nbt::IntArray(v) => Tag::IntArray(v.clone()),
        Nbt::LongArray(v) => Tag::LongArray(v.clone()),
        Nbt::List(items) => Tag::List(items.iter().map(nbt_to_tag).collect()),
        Nbt::Compound(compound) => Tag::Compound(lib_to_compound(compound)),
    }
}

fn lib_to_compound(compound: &LibCompound) -> Compound {
    let mut out = Compound::default();
    for (key, nbt) in compound {
        out.insert(JavaString::from(key.as_str()), nbt_to_tag(nbt));
    }
    out
}

fn read_key(nbt: &Compound, key: &str) -> Option<Nbt> {
    tag_to_nbt(nbt.get(&JavaString::from(key))?)
}

fn write_key(nbt: &mut Compound, key: &str, value: Option<&Nbt>) {
    let key = JavaString::from(key);
    match value {
        Some(value) => {
            nbt.insert(key, nbt_to_tag(value));
        }
        None => {
            nbt.remove(&key);
        }
    }
}

/// `ItemResource.CODEC` ao escrever: o `id` e, quando o recurso tem componentes, o `components` (o `DataComponentPatch.CODEC`: o dano e cada componente na forma que a pilha do jogo tem),
/// como o `optionalFieldOf("components", EMPTY)` do original. Vale para o NBT salvo (`items`) e para o de atualização.
pub(in crate::game) fn encode_item(item: &ItemResource) -> LibCompound {
    let mut resource = machinery_basis_core::transporter::stack::plain_item_encode(item);
    if let Some(Tag::Compound(components)) = basis::components_tag(item)
        && !components.is_empty()
    {
        resource.insert("components".into(), Nbt::Compound(compound_to_lib(&components)));
    }
    resource
}

/// `ItemResource.CODEC` ao ler: o `id` e o `components`, com o limite de pilha do item no registro.
pub(in crate::game) fn decode_item(game: &Game<'_>, tag: &LibCompound) -> Option<ItemResource> {
    let id = tag.get("id")?.as_str()?;
    let components = tag.get("components").map(nbt_to_tag);
    Some(basis::resource_from_tag(game, id, components.as_ref()))
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os vizinhos com inventário. O Horizon não desfaz uma inserção, então o dono usa um espelho dos slots que o lado alcança; o fim do evento aplica o que mudou.

/// Os itens que a rede pode oferecer a um vizinho no evento que vem: os que os transportadores levam e os que os vizinhos guardam. O Horizon não simula a inserção, então a regra de
/// cada slot (`isValid` do slot da máquina) é perguntada de antemão para eles (`machinery_slot_accepts`).
fn offered_resources(game: &Game<'_>, dimension: &'static str) -> Vec<ItemResource> {
    let mut out: Vec<ItemResource> = Vec::new();
    let mut add = |resource: ItemResource| {
        if !out.contains(&resource) {
            out.push(resource);
        }
    };
    with_owner(dimension, |owner| {
        for pos in owner.positions() {
            if let Some(base) = owner.transporter_at(pos) {
                for (_, stack) in base.transit_stacks() {
                    if let Some(item) = stack.borrow().item_type() {
                        add(item.clone());
                    }
                }
            }
        }
    });
    HANDLERS.with_borrow(|handlers| {
        for ((dim, _, _), handler) in handlers.iter() {
            if *dim == dimension {
                for index in 0..handler.size() {
                    if let Some(resource) = handler.resource(index) {
                        add(resource);
                    }
                }
            }
        }
    });
    let _ = game;
    out
}

thread_local! {
    /// O que `offered_resources` achou antes do evento em andamento.
    static OFFERED: RefCell<Vec<ItemResource>> = const { RefCell::new(Vec::new()) };
}

/// Os slots do contêiner do bloco em `pos` que o lado `side` dele alcança, ou `None` se ali não há contêiner (um bloco sem slots, outra dimensão, um chunk que não carregou, o tile
/// que está no tick). Cada slot traz o que o jogo recusa nele entre os itens que a rede pode oferecer.
fn mirror_slots(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Face) -> Option<Vec<MirrorSlot>> {
    let reached = basis::direction(side);
    let slots = game.native_container_slots(dimension, pos, reached)?;
    let offered: Vec<(ItemResource, Option<horizon_client::items::ItemStack>)> =
        OFFERED.with_borrow(|list| list.iter().map(|resource| (resource.clone(), basis::stack_of_resource(resource, 1))).collect());
    Some(
        slots
            .into_iter()
            .map(|slot| MirrorSlot {
                index: slot.slot,
                contents: slot.stack.as_ref().map(|stack| (basis::item_resource(game, stack), stack.count)),
                can_take: slot.can_take,
                refuses: offered
                    .iter()
                    .filter(|(_, stack)| stack.as_ref().is_some_and(|stack| !game.machinery_slot_accepts(pos, slot.slot, stack, reached)))
                    .map(|(resource, _)| resource.clone())
                    .collect(),
            })
            .collect(),
    )
}

/// Antes de um evento que anda pilhas ou calcula caminhos: os espelhos leem os contêineres de novo.
fn handlers_refresh(game: &Game<'_>, dimension: &'static str) {
    let offered = offered_resources(game, dimension);
    OFFERED.with_borrow_mut(|list| *list = offered);
    HANDLERS.with_borrow_mut(|handlers| {
        // Quem só a tabela guarda já não está no cache de nenhum transportador.
        handlers.retain(|_, handler| Rc::strong_count(handler) > 1);
        for ((dim, pos, side), handler) in handlers.iter() {
            if *dim != dimension {
                continue;
            }
            handler.refresh(mirror_slots(game, dimension, *pos, Face::ALL[*side]).unwrap_or_default());
        }
    });
}

/// Depois do evento: o que a rede pôs nos contêineres e tirou deles vai para o jogo.
fn handlers_flush(game: &mut Game<'_>, dimension: &'static str) {
    let pending: Vec<(([i32; 3], usize), Rc<HostItemHandler>)> = HANDLERS.with_borrow(|handlers| {
        handlers
            .iter()
            .filter(|((dim, _, _), handler)| *dim == dimension && !handler.changes().is_empty())
            .map(|((_, pos, side), handler)| ((*pos, *side), handler.clone()))
            .collect()
    });
    for ((pos, side), handler) in pending {
        let face = basis::direction(Face::ALL[side]);
        for change in handler.changes() {
            match change {
                SlotChange::Take { index, count } => {
                    if basis::transmitter_probe_enabled() {
                        eprintln!("MBX {{\"t\":{},\"p\":{pos:?},\"take\":{count},\"slot\":{index}}}", game.world.game_time);
                    }
                    // O jogo entrega as pilhas tiradas à rede, que já as tem em trânsito.
                    if game.native_extract_item(dimension, pos, face, index, count).is_none() {
                        eprintln!("mekanism: o slot {index} de {pos:?} não deixou tirar {count}");
                    }
                }
                SlotChange::Put { resource, count } => {
                    let stack = basis::stack_of_resource(&resource, count).unwrap_or_else(|| {
                        eprintln!("mekanism: o recurso {} perdeu os componentes ao entrar no vizinho", resource.id);
                        horizon_client::items::ItemStack::new(&resource.id, count)
                    });
                    if basis::transmitter_probe_enabled() {
                        eprintln!("MBX {{\"t\":{},\"p\":{pos:?},\"put\":{count},\"item\":{:?}}}", game.world.game_time, resource.id);
                    }
                    if let Some(rest) = game.native_insert_item(pos, face, stack) {
                        // O que o vizinho recusou (a regra do slot mudou entre a pergunta e a entrega) não se perde: cai ao lado dele.
                        eprintln!("mekanism: {} x{} recusados pelo vizinho {pos:?}, soltos no mundo", rest.item, rest.count);
                        let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
                        game.spawn_item(rest, at, [0.0; 3], 10);
                    }
                }
            }
        }
        handler.settle();
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O jogo visto pelo dono durante um evento.

struct GameWorld<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
}

impl InventoryOwnerWorld for GameWorld<'_, '_> {
    fn is_block_loaded(&self, pos: SimPos) -> bool {
        self.game.borrow().world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
    }

    fn is_getting_powered(&self, pos: SimPos) -> bool {
        self.game.borrow_mut().native_neighbor_signal(self.dimension, arr(pos)) > 0
    }

    fn item_handler(&self, pos: SimPos, side: Face) -> Option<ItemAcceptor> {
        let key = (self.dimension, arr(pos), side as usize);
        // `getCapability` não nulo: o vizinho tem um contêiner de itens que este lado alcança.
        let slots = mirror_slots(&self.game.borrow(), self.dimension, arr(pos), side)?;
        if let Some(handler) = HANDLERS.with_borrow(|handlers| handlers.get(&key).cloned()) {
            // O mesmo objeto de antes, com o que o evento já mudou nele: o `BlockCapabilityCache` do original devolve sempre o mesmo handler vivo.
            handler.refresh_if_clean(slots);
            return Some(handler as ItemAcceptor);
        }
        let handler = Rc::new(HostItemHandler::new(slots));
        HANDLERS.with_borrow_mut(|handlers| handlers.insert(key, handler.clone()));
        Some(handler)
    }

    fn transporter_info(&mut self, pos: SimPos) -> Option<TransporterInfo> {
        let game = self.game.borrow();
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos.x, pos.y, pos.z)) else {
            return None;
        };
        let (tier, kind) = transporter_stats(&entity.id)?;
        let tag = basis::nbt_to_cable(&entity.nbt);
        let items = read_key(&entity.nbt, keys::ITEMS);
        let color = read_key(&entity.nbt, keys::COLOR);
        let modes = read_key(&entity.nbt, keys::MODE);
        let from_save = tag.contains_key(machinery_basis_core::transmission::transmitter::keys::CONNECTION) || items.is_some() || color.is_some() || modes.is_some();
        Some(TransporterInfo { tier, kind, saved: from_save.then_some(SavedTransporter { tag, color, modes, items }) })
    }

    fn decode_item(&self, tag: &LibCompound) -> Option<ItemResource> {
        decode_item(&self.game.borrow(), tag)
    }

    fn encode_item(&self, item: &ItemResource) -> LibCompound {
        encode_item(item)
    }

    fn publish(&mut self, pos: SimPos, view: &CableView) {
        let at = arr(pos);
        basis::publish_arms(&mut self.game.borrow_mut(), at, view, "MBW");
        VIEWS.with_borrow_mut(|views| views.insert((self.dimension, at), view.clone()));
        // `sendUpdatePacket`: o cliente recebe o pacote de atualização (as pilhas, a cor e os modos) no começo do tick seguinte.
        client::dirty(self.dimension, at);
    }

    fn set_changed(&mut self, pos: SimPos) {
        self.game.borrow_mut().world.modified.insert((pos.x >> 4, pos.z >> 4));
    }

    fn transporter_packet(&mut self, pos: SimPos, packet: TransporterPacket) {
        client::transporter_packet(self.dimension, arr(pos), packet);
    }

    fn overlay(&mut self, _pos: SimPos, overlay: TransporterOverlay) {
        OVERLAYS.with_borrow_mut(|list| list.push(overlay));
    }

    fn capability_changed(&mut self, _pos: SimPos, _change: CapabilityChange) {
        // `invalidateCapabilities()` / `invalidateCapability(ITEM, side)`: quem escuta a posição (um funil, o ejetor de outra máquina) só relê a capability na próxima consulta.
    }

    /// `Block.popResource(level, pos, stack)`: a pilha sai do centro do bloco com um sorteio de -0,25 a 0,25 em cada eixo (e o meio da altura do item a menos em y), com a
    /// espera de 10 ticks para ser pega.
    fn world_drop(&mut self, drop: WorldDrop) {
        let mut game = self.game.borrow_mut();
        if basis::transmitter_probe_enabled() {
            eprintln!("MBD {{\"t\":{},\"p\":[{},{},{}],\"item\":{:?},\"n\":{}}}", game.world.game_time, drop.pos.x, drop.pos.y, drop.pos.z, drop.item.id, drop.amount);
        }
        let Some(stack) = basis::stack_of_resource(&drop.item, drop.amount) else {
            eprintln!("mekanism: o recurso {} perdeu os componentes ao cair no mundo", drop.item.id);
            return;
        };
        let random = &mut game.world.random;
        let mut at = [0.0; 3];
        for (axis, value) in at.iter_mut().enumerate() {
            let base = f64::from([drop.pos.x, drop.pos.y, drop.pos.z][axis]) + 0.5;
            let offset = random.next_double() * 0.5 - 0.25;
            *value = if axis == 1 { base + offset - 0.125 } else { base + offset };
        }
        // `ItemEntity(level, x, y, z, stack)`: o movimento inicial é sorteado.
        let motion = [random.next_double() * 0.2 - 0.1, 0.2, random.next_double() * 0.2 - 0.1];
        game.spawn_item(stack, at, motion, 10);
    }
}

/// `LogisticalTransporterBase.onUpdateClient` do transportador em `pos`, no tick do cliente (`TileEntityTransmitter.tickClient`): as pilhas do cliente avançam a velocidade do tier. `id` é o
/// bloco; fora dos transportadores não faz nada.
pub(in crate::game) fn transporter_client_tick(game: &Game<'_>, pos: [i32; 3], id: &str) {
    if let Some((tier, _)) = transporter_stats(id) {
        client::tick_transporter(game.native_dimension(), pos, tier.speed());
    }
}

fn new_owner() -> InventoryTransmitters {
    InventoryTransmitters::new(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
}

/// Roda `f` no dono dos transportadores da dimensão. `None` se o dono já está ocupado (um evento dentro de outro).
fn with_owner<R>(dimension: &'static str, f: impl FnOnce(&mut InventoryTransmitters) -> R) -> Option<R> {
    OWNERS.with(|owners| {
        let mut owners = owners.try_borrow_mut().ok()?;
        Some(f(owners.entry(dimension).or_insert_with(new_owner)))
    })
}

/// O sink dos eventos dos transmissores: traduz o evento do jogo para o do dono dos transportadores.
fn transporters_event(game: &mut Game<'_>, event: &TransmitterEvent) {
    let dimension = game.native_dimension();
    transporters_retry_pending(game, dimension);
    transporters_deliver(game, dimension, event);
    // Um `ClearRemoved` sem block entity no mundo ainda: tenta de novo nos eventos seguintes.
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::ClearRemoved, .. } = event {
        let created = with_owner(dimension, |owner| owner.transporter_at(sim_pos(*pos)).is_some()).unwrap_or(true);
        if !created && is_transporter_entity(game, *pos) {
            let now = game.world.game_time;
            PENDING.with_borrow_mut(|pending| pending.push((dimension, *pos, now)));
        }
    }
}

/// O block entity em `pos` é um transportador (ou ainda não está no mundo, e então a pergunta é de outro dono).
fn is_transporter_entity(game: &Game<'_>, pos: [i32; 3]) -> bool {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => transporter_stats(&entity.id).is_some(),
        _ => false,
    }
}

/// Os `ClearRemoved` que esperavam o block entity: o que já o tem entra, o que esperou demais sai da fila.
fn transporters_retry_pending(game: &mut Game<'_>, dimension: &'static str) {
    let waiting: Vec<(&'static str, [i32; 3], i64)> = PENDING.with_borrow_mut(|pending| std::mem::take(pending));
    for (dim, pos, since) in waiting {
        if dim != dimension {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
            continue;
        }
        if is_transporter_entity(game, pos) {
            transporters_deliver(game, dimension, &TransmitterEvent::Transmitter { dim: dimension.to_owned(), pos, kind: TransmitterKind::ClearRemoved });
        } else if game.world.game_time - since < 200 {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
        }
    }
}

fn transporters_deliver(game: &mut Game<'_>, dimension: &'static str, event: &TransmitterEvent) {
    let Some(event) = basis::owner_event(event) else {
        OWNERS.with_borrow_mut(HashMap::clear);
        VIEWS.with_borrow_mut(HashMap::clear);
        HANDLERS.with_borrow_mut(HashMap::clear);
        return;
    };
    // `onChunkUnloaded` toma a parte do transportador e o chunk vai para o disco: o NBT sai agora, antes do `setRemoved` que o tira do dono.
    let unloaded = if let TransporterEvent::ChunkUnloaded(pos) = &event { Some(arr(*pos)) } else { None };
    let removed = if let TransporterEvent::Removed(pos) = &event { Some(arr(*pos)) } else { None };
    // Quem anda pilhas (`Update`) ou calcula caminhos pela rede inteira lê os contêineres dos vizinhos, que o espelho copia antes.
    let reads_neighbors = matches!(event, TransporterEvent::Update(_) | TransporterEvent::ServerTickPost { .. } | TransporterEvent::Removed(_));
    if reads_neighbors {
        handlers_refresh(game, dimension);
    }
    if basis::transmitter_probe_enabled() && matches!(event, TransporterEvent::ServerTickPost { .. }) {
        // O tile que está no tick está fora do mundo: a sonda espera o fim do tick.
        probe_apply(game, dimension);
    }
    let probe = match &event {
        TransporterEvent::Update(pos) if basis::transmitter_probe_enabled() => Some(*pos),
        _ => None,
    };
    with_owner(dimension, |owner| {
        {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            owner.handle(&mut world, event);
        }
        if let Some(pos) = unloaded {
            transporter_write(game, owner, pos);
        }
        if let Some(pos) = probe {
            transit_probe(game, owner, pos);
        }
    });
    handlers_flush(game, dimension);
    if let Some(pos) = removed {
        client::gone(dimension, pos);
        VIEWS.with_borrow_mut(|views| views.remove(&(dimension, pos)));
    }
}

/// O pacote de atualização do transportador em `pos` (`writeReducedUpdatedTag`: as conexões, a rede, as pilhas em trânsito, a cor e os modos), para o cliente.
fn transporter_tag(_game: &Game<'_>, dimension: &'static str, pos: [i32; 3]) -> Option<client::TagOut> {
    let (tag, capacity) = with_owner(dimension, |owner| owner.client_update(sim_pos(pos), &encode_item)).flatten()?;
    Some(client::TagOut { kind: machinery_basis_core::transmission::client::ClientKind::Items, capacity, tag })
}

fn transporter_positions() -> Vec<(&'static str, [i32; 3])> {
    OWNERS.with(|owners| {
        owners
            .try_borrow()
            .map(|owners| owners.iter().flat_map(|(dimension, owner)| owner.positions().into_iter().map(move |pos| (*dimension, arr(pos)))).collect())
            .unwrap_or_default()
    })
}

/// `ItemAlloy.useOn` num transportador: a liga de nível `alloy_level` sobe os `LogisticalTransporter` da rede que `canUpgrade` aceita (`blocks` dá o bloco de cada nível, do básico ao
/// supremo; o restritivo e o desviador não são `IUpgradeableTransmitter`). `None` se o bloco clicado não é do dono; senão se algum subiu (a liga se gasta).
pub(in crate::game) fn transporter_alloy(game: &mut Game<'_>, pos: [i32; 3], alloy_level: i32, blocks: &[&str]) -> Option<bool> {
    let dimension = game.native_dimension();
    let level_of = |game: &Game<'_>, at: SimPos| -> Option<i32> {
        let block = game.native_block(arr(at));
        blocks.iter().position(|id| *id == block).map(|level| level as i32)
    };
    let mut run = {
        let game_ref: &Game<'_> = game;
        with_owner(dimension, |owner| owner.alloy_start(sim_pos(pos), alloy_level, &|at| level_of(game_ref, at)))??
    };
    let upgraded_block = *blocks.get(usize::try_from(alloy_level).ok()?)?;
    for target in run.targets.clone() {
        let data = {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            with_owner(dimension, |owner| owner.alloy_take(&mut world, &mut run, target)).flatten()
        };
        let Some(data) = data else {
            continue;
        };
        let carried: u32 = data.transit.transit.iter().map(|(_, stack)| stack.borrow().size()).sum();
        let replaced = basis::alloy_replace(game, arr(target), upgraded_block);
        let given = with_owner(dimension, |owner| owner.alloy_give(&mut run, target, data)).unwrap_or(false);
        if basis::transmitter_probe_enabled() {
            eprintln!("MBL {{\"e\":\"tstep\",\"p\":{:?},\"replaced\":{replaced},\"given\":{given},\"carried\":{carried}}}", arr(target));
        }
        if given && run.full() {
            break;
        }
    }
    {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        with_owner(dimension, |owner| owner.alloy_finish(&mut world, &run));
    }
    Some(run.did_upgrade())
}

/// Só com a sonda ligada (a cópia de teste, `enviar_fontes.sh`; nunca no repositório): o NBT de um transportador com `probe_set: {raw: [I;6], mode: [I;6], color: n, delay: n}` põe os tipos de conexão
/// crus (e o `delay` do puxar, como o teste do rastro) (o Configurator do original faz um por clique), o modo do desviador de cada lado e a cor (`-1`: nenhuma), e a chave some. Serve ao teste em jogo montar a cena do rastro sem o
/// jogador; o Configurator de verdade tem o roteiro dele.
fn probe_apply(game: &mut Game<'_>, dimension: &'static str) {
    let positions: Vec<[i32; 3]> = with_owner(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
    for pos in positions {
        let key = JavaString::from("probe_set");
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
            continue;
        };
        let Some(Tag::Compound(set)) = entity.nbt.remove(&key) else {
            continue;
        };
        let array = |name: &str| match set.get(&JavaString::from(name)) {
            Some(Tag::IntArray(values)) if values.len() == 6 => Some(values.clone()),
            _ => None,
        };
        let (raw, modes) = (array("raw"), array("mode"));
        let color = match set.get(&JavaString::from("color")) {
            Some(Tag::Int(color)) => Some(*color),
            _ => None,
        };
        let delay = match set.get(&JavaString::from("delay")) {
            Some(Tag::Int(delay)) => Some(*delay),
            _ => None,
        };
        with_owner(dimension, |owner| {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            // O `restore_counters` do teste do rastro: o `delay` de quem puxa, para a cena começar na fase do original.
            if let Some(delay) = delay {
                owner.with_transporter(&world, sim_pos(pos), |base, _| {
                    let (_, delay_count, next_id) = base.counters();
                    base.restore_counters(delay, delay_count, next_id);
                });
            }
            if let Some(color) = color {
                owner.set_color(&mut world, sim_pos(pos), u8::try_from(color).ok());
            }
            for (side, kind) in raw.iter().flatten().enumerate() {
                owner.set_mode(&mut world, sim_pos(pos), Face::ALL[side], ConnectionType::by_id(*kind));
            }
            for (side, mode) in modes.iter().flatten().enumerate() {
                owner.set_diversion_mode(&mut world, sim_pos(pos), Face::ALL[side], DiversionControl::by_id(*mode));
            }
        });
        eprintln!("MBS {{\"set\":[{},{},{}]}}", pos[0], pos[1], pos[2]);
    }
}

/// A sonda do movimento (`MBI`): as pilhas em trânsito do transportador que acabou de andar, para comparar com os rastros `transporter_move_*.jsonl`.
fn transit_probe(game: &Game<'_>, owner: &InventoryTransmitters, pos: SimPos) {
    let Some(base) = owner.transporter_at(pos) else {
        return;
    };
    let (delay, delay_count, next_id) = base.counters();
    let stacks: Vec<String> = base
        .transit_stacks()
        .iter()
        .map(|(id, stack)| {
            let stack = stack.borrow();
            format!("[{},\"{}\",{},{}]", id, stack.item_type().map_or("", |item| &*item.id), stack.size(), stack.progress)
        })
        .collect();
    eprintln!("MBI {{\"t\":{},\"p\":[{},{},{}],\"dl\":{},\"dc\":{},\"nid\":{},\"stacks\":[{}]}}", game.world.game_time, pos.x, pos.y, pos.z, delay, delay_count, next_id, stacks.join(","));
}

/// `TileEntityTransmitter.saveAdditional` e `LogisticalTransporterBase.write`: grava no block entity de `pos` o que o transportador guarda.
fn transporter_write(game: &mut Game<'_>, owner: &mut InventoryTransmitters, pos: [i32; 3]) {
    let Some(saved) = owner.write(sim_pos(pos), &encode_item) else {
        return;
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        basis::cable_to_nbt(&saved.tag, &mut entity.nbt);
        write_key(&mut entity.nbt, keys::COLOR, saved.color.as_ref());
        write_key(&mut entity.nbt, keys::MODE, saved.modes.as_ref());
        write_key(&mut entity.nbt, keys::ITEMS, saved.items.as_ref());
    }
}

/// O salvamento do nível (`Game::save_level`, antes de ele esvaziar os chunks alterados): os transportadores dos chunks que vão para o disco gravam o NBT deles.
pub(in crate::game) fn transporters_save(game: &mut Game<'_>) {
    let dimension = game.native_dimension();
    let chunks: Vec<(i32, i32)> = game.world.modified.iter().copied().collect();
    if chunks.is_empty() {
        return;
    }
    let positions: Vec<[i32; 3]> = with_owner(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
    for pos in positions {
        if chunks.contains(&(pos[0] >> 4, pos[2] >> 4)) {
            with_owner(dimension, |owner| transporter_write(game, owner, pos));
        }
    }
}

/// Os tipos de conexão visíveis do transportador em `pos` (`getConnectionType(side)`: `NONE` onde não há braço), na ordem dos lados, como o último `publish` os deixou. `None` fora do
/// servidor, que é onde o dono vive, ou se o transportador ainda não publicou.
pub(in crate::game) fn transporter_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    VIEWS.with_borrow(|views| views.get(&(game.native_dimension(), pos)).map(|view| view.kind))
}

/// `DiversionTransporter.modes` do desviador em `pos`, um por lado (`RenderTickHandler.onBlockHover` lê `transporter.getTransmitter().modes[face.ordinal()]`). O servidor (e o
/// anfitrião, que o roda) lê o dono, que tem o objeto vivo; o convidado de LAN lê o que o pacote de atualização trouxe (`handleUpdateTag` -> `readModes`: sem a chave, todos
/// desligados). `None` se não há transportador ali, ou se ele não é um desviador no dono.
pub(in crate::game) fn transporter_modes(game: &Game<'_>, pos: [i32; 3]) -> Option<[DiversionControl; 6]> {
    let dimension = game.native_dimension();
    let live = with_owner(dimension, |owner| owner.transporter_at(sim_pos(pos)).map(|base| base.diversion_modes())).flatten();
    match live {
        Some(modes) => modes,
        None => client::modes_at(dimension, pos),
    }
}

/// O clique do Configurator num transportador (`ItemConfigurator.useOn`: `IConfigurable.onSneakRightClick` agachado, `onRightClick` senão). `side` é a face clicada e `hit_side` o
/// lado do braço que o olhar toca. Devolve se o clique foi do transportador e os avisos para o jogador.
pub(in crate::game) fn transporter_configure(
    game: &mut Game<'_>,
    pos: [i32; 3],
    side: Face,
    hit_side: Option<Face>,
    sneaking: bool,
) -> Option<(bool, Vec<TransporterOverlay>)> {
    let dimension = game.native_dimension();
    OVERLAYS.with_borrow_mut(Vec::clear);
    let consumed = with_owner(dimension, |owner| {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        if sneaking {
            owner.sneak_right_click(&mut world, sim_pos(pos), side, hit_side)
        } else {
            owner.right_click(&mut world, sim_pos(pos), side, hit_side)
        }
    })?;
    let overlays = OVERLAYS.with_borrow_mut(std::mem::take);
    Some((consumed, overlays))
}

/// O item que um funil, um dropper ou o ejetor de outra máquina põe no bloco em `pos` pela face `face` dele (`TransporterItemHandler.insert`, o que o hook `transporter_insert` chama de
/// `Game::move_in_item`). `None` se o bloco não é um transportador (o corpo original segue). Senão, `(o que sobra, se algo entrou)`: a pilha entra em trânsito até a vazão do tier
/// (`pullAmount`) e só se o lado expõe a capability (`exposesInsertCap`) e há rede.
pub(in crate::game) fn hopper_insert(
    game: &mut Game<'_>,
    pos: [i32; 3],
    face: Option<horizon_core::geometry::Direction>,
    stack: &horizon_client::items::ItemStack,
) -> Option<(Option<horizon_client::items::ItemStack>, bool)> {
    let dimension = game.native_dimension();
    if !with_owner(dimension, |owner| owner.transporter_at(sim_pos(pos)).is_some()).unwrap_or(false) {
        return None;
    }
    let refused = Some((Some(stack.clone()), false));
    let Some(face) = face else {
        return refused;
    };
    let side = basis::face_of(face);
    let resource = basis::item_resource(game, stack);
    // O caminho simula a inserção em todos os destinos da rede: os espelhos leem os contêineres antes e o que a pilha mudar sai depois.
    handlers_refresh(game, dimension);
    let inserted = with_owner(dimension, |owner| {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        owner.insert(&mut world, sim_pos(pos), side, &resource, stack.count)
    })
    .flatten();
    handlers_flush(game, dimension);
    let outcome = hopper_outcome(stack.count, inserted);
    if !outcome.moved {
        return refused;
    }
    let mut rest = stack.clone();
    rest.count = outcome.rest;
    Some(((rest.count > 0).then_some(rest), true))
}
