// Os canos mecânicos no jogo (frente `cano-jogo`, Fase 3). O dono dos transmissores de recurso (`machinery_basis_core::transmission::resource_owner`)
// guarda por dimensão o registro, os `Transmitter` e a família de fluido; esta parte o liga ao jogo como `machinery_basis` liga o dos cabos: os eventos da decisão 107 entram nele, as
// conexões que ele calcula viram as propriedades de lado do bloco, o NBT do cano (`TileEntityTransmitter.saveAdditional` e o `stored` de `BufferedResourceTransmitter.write`) sai no
// salvamento, e o vizinho com tanque (o `lavaTank` do Heat Generator) responde à rede por um espelho do contêiner dele (`HostHandler`, decisão 122). A lib não conhece nenhum id: o mod
// diz quais blocos são canos e o tier de cada um em `pipes_register`, e quais blocos têm um tanque de fluido em `fluid_neighbor_register` e `fluid_faces_set`.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use crate::game::machinery_basis::{TransmitterEvent, TransmitterKind};
use horizon_client::containers::BlockEntity;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos as SimPos, ChunkPos};
use machinery_basis_core::transmission::fluid_network::{FluidHandler, FluidTransmitters, fluid_transmitters};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::host_resource::{HostHandler, ResourceSnapshot};
use machinery_basis_core::transmission::owner::{CableView, Event as PipeEvent};
use machinery_basis_core::transmission::resource_owner::{PipeInfo, ResourceOwnerWorld};
use machinery_basis_core::transmission::transmitter::{Overlay as PipeOverlay, keys};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

/// A capacidade e a vazão de um bloco de cano (`PipeTier.getCapacity` e `getTransferRate`), ou `None` se o bloco não é um cano.
pub type PipeStats = fn(&str) -> Option<(i64, i32)>;

/// O que o Horizon guarda do cano (`stored`: o `saveShare`).
const STORED: &str = "stored";
/// `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE`: as propriedades de lado só vão aos clientes (`sendUpdatePacket` não avisa vizinho nem refaz forma).
const PIPE_FLAGS: i32 = 2 | 16;
/// O nome do dono dos canos entre os que ouvem os eventos dos transmissores.
const SINK_OWNER: &str = "machinery_pipes";

static PIPE_KINDS: Mutex<Vec<PipeStats>> = Mutex::new(Vec::new());

/// `(dimensão, posição do vizinho, lado dele que olha o cano)`.
type HandlerKey = (&'static str, [i32; 3], usize);

thread_local! {
    /// Os canos de cada dimensão do nível deste servidor.
    static PIPES: RefCell<HashMap<&'static str, FluidTransmitters>> = RefCell::new(HashMap::new());
    /// Os vizinhos que o dono guarda como aceitadores (o mesmo objeto que o registro tem no cache de aceitadores).
    static HANDLERS: RefCell<HashMap<HandlerKey, Rc<HostHandler<FluidResource>>>> = RefCell::new(HashMap::new());
    /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
    static OVERLAYS: RefCell<Vec<PipeOverlay>> = const { RefCell::new(Vec::new()) };
    /// O último `publish` de cada cano (o tipo de conexão visível de cada lado), para a mira do Configurator achar as peças de `getCollisionBoxes`.
    static PIPE_VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
    /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon: `(dimensão, posição, tick)`, tentados de novo.
    static PENDING: RefCell<Vec<(&'static str, [i32; 3], i64)>> = const { RefCell::new(Vec::new()) };
}

/// `id` (o `ModBlockEntity.id`) passa a ser um cano mecânico: os eventos do jogo dele vão ao dono dos canos, e `stats` dá a capacidade e a vazão do tier.
pub fn pipes_register(id: &str, stats: PipeStats) {
    crate::game::machinery_transmitter_client::register_source(crate::game::machinery_transmitter_client::TagSource { tag: pipe_tag, positions: pipe_positions });
    basis::transmitters_register(SINK_OWNER, id, pipes_event);
    let mut kinds = PIPE_KINDS.lock().unwrap_or_else(|e| e.into_inner());
    if !kinds.iter().any(|known| *known as usize == stats as usize) {
        kinds.push(stats);
    }
}

fn pipe_stats(block: &str) -> Option<(i64, i32)> {
    let kinds: Vec<PipeStats> = PIPE_KINDS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    kinds.into_iter().find_map(|stats| stats(block))
}

fn sim_pos(pos: [i32; 3]) -> SimPos {
    SimPos::new(pos[0], pos[1], pos[2])
}

fn arr(pos: SimPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os vizinhos com tanque de fluido. O mod diz o bloco, como ler o tanque dele e como gravar o que a rede mudou; as faces são da instância (`fluid_faces_set`), como as de energia (H1).

/// Lê o contêiner de fluido do bloco em `pos`, ou `None` se o tile não está no mundo agora (está no tick).
pub type FluidProbe = fn(&Game<'_>, &'static str, [i32; 3]) -> Option<ResourceSnapshot<FluidResource>>;
/// Grava o conteúdo que o contêiner passa a ter.
pub type FluidWrite = fn(&mut Game<'_>, &'static str, [i32; 3], StackContents<FluidResource>);

#[derive(Clone, Copy)]
struct FluidNeighbor {
    block: &'static str,
    probe: FluidProbe,
    write: FluidWrite,
}

static NEIGHBORS: Mutex<Vec<FluidNeighbor>> = Mutex::new(Vec::new());
/// As faces de fluido que a instância declarou: `(bloco, lados que expõem a capability)`.
static FACES: Mutex<Vec<((&'static str, [i32; 3]), (String, [bool; 6]))>> = Mutex::new(Vec::new());
/// As posições cuja capability de fluido mudou desde o último tick do servidor, para os canos vizinhos ouvirem.
static CAPABILITY_CHANGED: Mutex<Vec<(&'static str, [i32; 3])>> = Mutex::new(Vec::new());

/// O bloco `block` tem um tanque de fluido que a rede alcança: `probe` o lê e `write` grava o que a rede mudou. Pode ser dito mais de uma vez.
pub fn fluid_neighbor_register(block: &'static str, probe: FluidProbe, write: FluidWrite) {
    let mut neighbors = NEIGHBORS.lock().unwrap_or_else(|e| e.into_inner());
    if !neighbors.iter().any(|known| known.block == block) {
        neighbors.push(FluidNeighbor { block, probe, write });
    }
}

/// Dá ao bloco em `pos` as faces de fluido que ele tem agora (a capability por instância). Quem faz o tile andar chama a cada tick: só uma mudança avisa os canos vizinhos
/// (`BlockEntity.invalidateCapabilities`, que o cache de aceitadores do cano ouve).
pub fn fluid_faces_set(dimension: &'static str, pos: [i32; 3], block: &str, faces: [bool; 6]) {
    let mut table = FACES.lock().unwrap_or_else(|e| e.into_inner());
    let key = (dimension, pos);
    match table.iter_mut().find(|(known, _)| *known == key) {
        Some((_, current)) if current.0 == block && current.1 == faces => {}
        Some((_, current)) => {
            *current = (block.to_owned(), faces);
            CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner()).push(key);
        }
        None => {
            table.push((key, (block.to_owned(), faces)));
            CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner()).push(key);
        }
    }
}

/// O que um tanque criativo faria no lugar de um bloco de teste: com a sonda ligada (`enviar_fontes.sh`, nunca no repositório), o caldeirão de lava e o de água são tanques
/// criativos cheios que entregam e aceitam sem mudar (`FluidTankFluidTank.isCreative`), porque o Fluid Tank ainda não está portado.
fn probe_source(block: &str) -> Option<ResourceSnapshot<FluidResource>> {
    let fluid = match block {
        "minecraft:lava_cauldron" => "minecraft:lava",
        "minecraft:water_cauldron" => "minecraft:water",
        _ => return None,
    };
    let capacity = i64::MAX as u64;
    Some(ResourceSnapshot {
        contents: StackContents::new(Some(FluidResource::new(fluid)), capacity),
        capacity,
        accepts: Rc::new(|_| true),
        can_insert: true,
        can_extract: true,
        creative: true,
    })
}

/// O contêiner de fluido do bloco em `pos` visto pelo lado `side` dele, ou `None` se a capability não existe ali.
fn fluid_snapshot(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Face) -> Option<ResourceSnapshot<FluidResource>> {
    let block = game.native_block(pos);
    if basis::transmitter_probe_enabled()
        && let Some(source) = probe_source(&block)
    {
        return Some(source);
    }
    let exposed = FACES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|((dim, at), (known, faces))| *dim == dimension && *at == pos && known.as_str() == &*block && faces[side as usize]);
    if !exposed {
        return None;
    }
    let neighbor = NEIGHBORS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|known| known.block == &*block).copied()?;
    // O tile que está no tick está fora do mundo: a capability existe, mas não entrega nem recebe nada até ele voltar.
    Some((neighbor.probe)(game, dimension, pos).unwrap_or_else(ResourceSnapshot::closed))
}

/// Antes de um evento que pode distribuir ou puxar fluido: os aceitadores leem o contêiner de novo (`around` limita aos vizinhos de um cano).
fn handlers_refresh(game: &Game<'_>, dimension: &'static str, around: Option<[i32; 3]>) {
    HANDLERS.with_borrow_mut(|handlers| {
        // Quem só a tabela guarda já não está no cache de nenhum cano.
        handlers.retain(|_, handler| Rc::strong_count(handler) > 1);
        for ((dim, pos, side), handler) in handlers.iter() {
            if *dim != dimension {
                continue;
            }
            if let Some(pipe) = around
                && !Face::ALL.iter().any(|face| {
                    let [x, y, z] = face.offset();
                    [pipe[0] + x, pipe[1] + y, pipe[2] + z] == *pos
                })
            {
                continue;
            }
            let snapshot = fluid_snapshot(game, dimension, *pos, Face::ALL[*side]).unwrap_or_else(ResourceSnapshot::closed);
            handler.refresh(snapshot);
        }
    });
}

/// Depois do evento: o que a rede entregou e tirou vai para os contêineres dos vizinhos.
fn handlers_flush(game: &mut Game<'_>, dimension: &'static str) {
    let pending: Vec<([i32; 3], Rc<HostHandler<FluidResource>>)> = HANDLERS.with_borrow(|handlers| {
        handlers
            .iter()
            .filter(|((dim, _, _), handler)| *dim == dimension && handler.changed().is_some())
            .map(|((_, pos, _), handler)| (*pos, handler.clone()))
            .collect()
    });
    for (pos, handler) in pending {
        let Some(contents) = handler.changed() else {
            continue;
        };
        let block = game.native_block(pos);
        let write = NEIGHBORS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|known| known.block == &*block).map(|known| known.write);
        if let Some(write) = write {
            write(game, dimension, pos, contents);
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

impl ResourceOwnerWorld<FluidResource> for GameWorld<'_, '_> {
    fn is_block_loaded(&self, pos: SimPos) -> bool {
        self.game.borrow().world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
    }

    fn is_getting_powered(&self, pos: SimPos) -> bool {
        self.game.borrow_mut().native_neighbor_signal(self.dimension, arr(pos)) > 0
    }

    fn handler(&self, pos: SimPos, side: Face) -> Option<FluidHandler> {
        let key = (self.dimension, arr(pos), side as usize);
        // `getCapability` não nulo: o vizinho tem um contêiner de fluido que alguma configuração de lados deixa encher ou esvaziar por este lado.
        let snapshot = fluid_snapshot(&self.game.borrow(), self.dimension, arr(pos), side)?;
        if let Some(handler) = HANDLERS.with_borrow(|handlers| handlers.get(&key).cloned()) {
            handler.refresh(snapshot);
            return Some(handler as FluidHandler);
        }
        let handler = Rc::new(HostHandler::new(snapshot));
        HANDLERS.with_borrow_mut(|handlers| handlers.insert(key, handler.clone()));
        Some(handler)
    }

    fn pipe_info(&mut self, pos: SimPos) -> Option<PipeInfo<FluidResource>> {
        let game = self.game.borrow();
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos.x, pos.y, pos.z)) else {
            return None;
        };
        let (capacity, transfer_rate) = pipe_stats(&entity.id)?;
        let tag = basis::nbt_to_cable(&entity.nbt);
        let stored = read_stored(&entity.nbt);
        let from_save = tag.contains_key(keys::CONNECTION) || stored.is_some();
        Some(PipeInfo { capacity, transfer_rate, saved: from_save.then(|| (tag, stored.unwrap_or_default())) })
    }

    fn publish(&mut self, pos: SimPos, view: &CableView) {
        let at = arr(pos);
        let mut game = self.game.borrow_mut();
        for side in Face::ALL {
            let value = if view.connected[side as usize] { "true" } else { "false" };
            game.native_set_block_property_with(at, side.name(), value, PIPE_FLAGS);
        }
        if basis::transmitter_probe_enabled() {
            let kinds: Vec<&str> = view.kind.iter().map(|kind| kind.serialized_name()).collect();
            let raws: Vec<&str> = view.raw.iter().map(|kind| kind.serialized_name()).collect();
            eprintln!("MBW {{\"p\":[{},{},{}],\"conn\":{:?},\"kind\":{:?},\"raw\":{:?},\"net\":{}}}", at[0], at[1], at[2], view.connected, kinds, raws, view.has_network);
        }
        PIPE_VIEWS.with_borrow_mut(|views| views.insert((self.dimension, at), view.clone()));
        // `sendUpdatePacket`: o cliente recebe o pacote de atualização no começo do tick seguinte.
        crate::game::machinery_transmitter_client::dirty(self.dimension, at);
    }

    fn overlay(&mut self, _pos: SimPos, overlay: PipeOverlay) {
        OVERLAYS.with_borrow_mut(|list| list.push(overlay));
    }
}

/// Só com a sonda ligada (a cópia de teste): o NBT do cano com `probe_push: {fluid, amount}` empurra esse fluido para o contêiner da capability dele (`EXTERNAL`), como um vizinho que
/// tem um tanque faria, e a chave some. Serve ao teste em jogo repetir o `push` do rastro; nenhum bloco portado empurra fluido ainda.
fn probe_push(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    let key = JavaString::from("probe_push");
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let Some(Tag::Compound(push)) = entity.nbt.remove(&key) else {
        return;
    };
    let (Some(Tag::String(fluid)), Some(Tag::Int(amount))) = (push.get(&JavaString::from("fluid")), push.get(&JavaString::from("amount"))) else {
        return;
    };
    let fluid = FluidResource::new(&fluid.to_string_lossy());
    let got = with_owner(dimension, |owner| owner.insert_external(sim_pos(pos), &fluid, *amount));
    eprintln!("MBF {{\"push\":[{},{},{}],\"fluid\":\"{}\",\"amount\":{},\"got\":{:?}}}", pos[0], pos[1], pos[2], fluid.id, amount, got);
}

/// O `saveShare` que o Horizon guardou: `stored: {type: {id}, amount}` (`StackHelper.storeNonEmpty` com o `FluidResource.CODEC`).
fn read_stored(nbt: &Compound) -> Option<StackContents<FluidResource>> {
    let Some(Tag::Compound(stored)) = nbt.get(&JavaString::from(STORED)) else {
        return None;
    };
    let Some(Tag::Compound(kind)) = stored.get(&JavaString::from("type")) else {
        return None;
    };
    let Some(Tag::String(id)) = kind.get(&JavaString::from("id")) else {
        return None;
    };
    let amount = match stored.get(&JavaString::from("amount")) {
        Some(Tag::Long(amount)) => u64::try_from(*amount).unwrap_or(0),
        Some(Tag::Int(amount)) => u64::try_from(*amount).unwrap_or(0),
        _ => 0,
    };
    Some(StackContents::new(Some(FluidResource::new(&id.to_string_lossy())), amount))
}

fn write_stored(nbt: &mut Compound, stored: &StackContents<FluidResource>) {
    let key = JavaString::from(STORED);
    let Some(resource) = stored.resource.as_ref().filter(|_| stored.amount > 0) else {
        nbt.remove(&key);
        return;
    };
    let mut kind = Compound::default();
    kind.insert(JavaString::from("id"), Tag::String(JavaString::from(&*resource.id)));
    let mut tag = Compound::default();
    tag.insert(JavaString::from("type"), Tag::Compound(kind));
    tag.insert(JavaString::from("amount"), Tag::Long(i64::try_from(stored.amount).unwrap_or(i64::MAX)));
    nbt.insert(key, Tag::Compound(tag));
}

fn new_owner() -> FluidTransmitters {
    fluid_transmitters(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
}

/// O pacote de atualização do transmissor em `pos` (`writeReducedUpdatedTag`), para o cliente.
fn pipe_tag(_game: &Game<'_>, dimension: &'static str, pos: [i32; 3]) -> Option<crate::game::machinery_transmitter_client::TagOut> {
    let (tag, capacity) = with_owner(dimension, |owner| owner.client_update(sim_pos(pos))).flatten()?;
    Some(crate::game::machinery_transmitter_client::TagOut { kind: machinery_basis_core::transmission::client::ClientKind::Resource, capacity, tag })
}

fn pipe_positions() -> Vec<(&'static str, [i32; 3])> {
    PIPES.with(|owners| {
        owners
            .try_borrow()
            .map(|owners| owners.iter().flat_map(|(dimension, owner)| owner.positions().into_iter().map(move |pos| (*dimension, arr(pos)))).collect())
            .unwrap_or_default()
    })
}

/// Roda `f` no dono dos canos da dimensão. `None` se o dono já está ocupado (um evento dentro de outro).
fn with_owner<R>(dimension: &'static str, f: impl FnOnce(&mut FluidTransmitters) -> R) -> Option<R> {
    PIPES.with(|pipes| {
        let mut pipes = pipes.try_borrow_mut().ok()?;
        Some(f(pipes.entry(dimension).or_insert_with(new_owner)))
    })
}

/// O sink dos eventos dos transmissores: traduz o evento do jogo para o do dono dos canos.
fn pipes_event(game: &mut Game<'_>, event: &TransmitterEvent) {
    let dimension = game.native_dimension();
    pipes_retry_pending(game, dimension);
    pipes_deliver(game, dimension, event);
    // Um `ClearRemoved` sem block entity no mundo ainda: tenta de novo nos eventos seguintes.
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::ClearRemoved, .. } = event {
        let created = with_owner(dimension, |owner| owner.pipe_at(sim_pos(*pos)).is_some()).unwrap_or(true);
        if !created && is_pipe_entity(game, *pos) {
            let now = game.world.game_time;
            PENDING.with_borrow_mut(|pending| pending.push((dimension, *pos, now)));
        }
    }
}

/// O block entity em `pos` é um cano (ou ainda não está no mundo, e então a pergunta é de outro dono).
fn is_pipe_entity(game: &Game<'_>, pos: [i32; 3]) -> bool {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => pipe_stats(&entity.id).is_some(),
        _ => false,
    }
}

/// Os `ClearRemoved` que esperavam o block entity: o que já o tem entra, o que esperou demais sai da fila.
fn pipes_retry_pending(game: &mut Game<'_>, dimension: &'static str) {
    let waiting: Vec<(&'static str, [i32; 3], i64)> = PENDING.with_borrow_mut(|pending| std::mem::take(pending));
    for (dim, pos, since) in waiting {
        if dim != dimension {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
            continue;
        }
        if is_pipe_entity(game, pos) {
            pipes_deliver(game, dimension, &TransmitterEvent::Transmitter { dim: dimension.to_owned(), pos, kind: TransmitterKind::ClearRemoved });
        } else if game.world.game_time - since < 200 {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
        }
    }
}

fn pipes_deliver(game: &mut Game<'_>, dimension: &'static str, event: &TransmitterEvent) {
    let event = match event {
        TransmitterEvent::Transmitter { pos, kind, .. } => {
            let pos = sim_pos(*pos);
            match kind {
                TransmitterKind::ClearRemoved => PipeEvent::ClearRemoved(pos),
                TransmitterKind::PlacedByItem => PipeEvent::PlacedByItem(pos),
                TransmitterKind::ChunkUnloaded => PipeEvent::ChunkUnloaded(pos),
                TransmitterKind::Removed => PipeEvent::Removed(pos),
                TransmitterKind::Update => PipeEvent::Update(pos),
                TransmitterKind::Neighbor => PipeEvent::Neighbor(pos),
                TransmitterKind::Capability(side) => PipeEvent::Capability(pos, Face::ALL[*side]),
            }
        }
        TransmitterEvent::Ticket { chunk, loaded, .. } => PipeEvent::Ticket { chunk: ChunkPos::new(chunk[0], chunk[1]), loaded: *loaded },
        TransmitterEvent::ServerTickPost { runs_normally } => PipeEvent::ServerTickPost { runs_normally: *runs_normally },
        TransmitterEvent::Reset => {
            PIPES.with_borrow_mut(HashMap::clear);
            PIPE_VIEWS.with_borrow_mut(HashMap::clear);
            HANDLERS.with_borrow_mut(HashMap::clear);
            FACES.lock().unwrap_or_else(|e| e.into_inner()).clear();
            CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner()).clear();
            return;
        }
    };
    let post = matches!(event, PipeEvent::ServerTickPost { .. });
    // `onChunkUnloaded` toma a parte do cano e o chunk vai para o disco: o NBT sai agora, antes do `setRemoved` que o tira do dono.
    let unloaded = if let PipeEvent::ChunkUnloaded(pos) = &event { Some(arr(*pos)) } else { None };
    let removed = if let PipeEvent::Removed(pos) = &event { Some(arr(*pos)) } else { None };
    // Quem puxa (`Update`) ou distribui (`ServerTickPost`) fala com os vizinhos pelos aceitadores, que leem o contêiner deles antes e escrevem depois.
    match &event {
        PipeEvent::Update(pos) => handlers_refresh(game, dimension, Some(arr(*pos))),
        PipeEvent::ServerTickPost { .. } => handlers_refresh(game, dimension, None),
        _ => {}
    }
    if post && basis::transmitter_probe_enabled() {
        // O tile que está no tick está fora do mundo: o empurrão da sonda espera o fim do tick, antes do `onTick` do registro.
        let positions: Vec<[i32; 3]> = with_owner(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
        for pos in positions {
            probe_push(game, dimension, pos);
        }
    }
    if post {
        // As capabilities de fluido que mudaram durante o tick chegam aos canos vizinhos antes do `onTick` do registro.
        let changed: Vec<[i32; 3]> = {
            let mut list = CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner());
            let (here, other): (Vec<_>, Vec<_>) = std::mem::take(&mut *list).into_iter().partition(|(dim, _)| *dim == dimension);
            *list = other;
            here.into_iter().map(|(_, pos)| pos).collect()
        };
        for pos in changed {
            for side in Face::ALL {
                let [x, y, z] = side.offset();
                let neighbor = [pos[0] + x, pos[1] + y, pos[2] + z];
                if with_owner(dimension, |owner| owner.pipe_at(sim_pos(neighbor)).is_some()).unwrap_or(false) {
                    let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
                    with_owner(dimension, |owner| owner.handle(&mut world, PipeEvent::Capability(sim_pos(neighbor), side.opposite())));
                }
            }
        }
        let time = game.world.game_time;
        with_owner(dimension, |owner| owner.set_game_time(time));
    }
    let runs_normally = matches!(event, PipeEvent::ServerTickPost { runs_normally: true });
    let mut scale_packets = Vec::new();
    let dirty = with_owner(dimension, |owner| {
        {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            owner.handle(&mut world, event);
        }
        if post {
            scale_packets = owner.take_scale_packets();
        }
        if let Some(pos) = unloaded {
            pipe_write(game, owner, pos);
        }
        if post && basis::transmitter_probe_enabled() {
            // A sonda do fluido (`MBP`): o fluido, a quantidade, a capacidade e o que saiu de cada rede neste tick, e quantos canos e aceitadores ela tem.
            let nets: Vec<String> = owner
                .registry()
                .networks_in_order()
                .into_iter()
                .filter_map(|id| {
                    let net = owner.family().network(id)?;
                    let registered = owner.registry().network(id);
                    let buffer = net.buffer();
                    Some(format!(
                        "[\"{}\",{},{},{},{},{}]",
                        buffer.resource.as_ref().map_or("", |fluid| &*fluid.id),
                        buffer.amount,
                        net.capacity(),
                        net.prev_transfer_amount(),
                        registered.transmitters_size(),
                        registered.acceptor_count()
                    ))
                })
                .collect();
            eprintln!("MBP {{\"t\":{},\"runs\":{},\"nets\":[{}]}}", game.world.game_time, runs_normally, nets.join(","));
        }
        if post { owner.family_mut().take_dirty_chunks() } else { Vec::new() }
    });
    handlers_flush(game, dimension);
    // `Mekanism.onLiquidTransferred` e `onChemicalTransferred`: o `PacketNetworkScale` e o do conteúdo saem juntos, a quem está no raio da rede.
    for out in scale_packets {
        let contents = out.contents.map(|resource| resource.map(|resource| resource.id.to_string()));
        crate::game::machinery_transmitter_client::scale(game, dimension, out.packet, out.range, contents);
    }
    if let Some(pos) = removed {
        crate::game::machinery_transmitter_client::gone(game, dimension, pos);
        PIPE_VIEWS.with_borrow_mut(|views| views.remove(&(dimension, pos)));
    }
    // `markChunkDirty` da rede: o chunk do cano vai para o próximo salvamento.
    for key in dirty.unwrap_or_default() {
        let chunk = ChunkPos::unpack(key);
        game.world.modified.insert((chunk.x, chunk.z));
    }
}

/// `TileEntityTransmitter.saveAdditional` e `BufferedResourceTransmitter.write`: grava no block entity de `pos` o que o cano guarda.
fn pipe_write(game: &mut Game<'_>, owner: &mut FluidTransmitters, pos: [i32; 3]) {
    let Some((tag, stored)) = owner.write(sim_pos(pos)) else {
        return;
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        basis::cable_to_nbt(&tag, &mut entity.nbt);
        write_stored(&mut entity.nbt, &stored);
    }
}

/// O salvamento do nível (`Game::save_level`, antes de ele esvaziar os chunks alterados): os canos dos chunks que vão para o disco gravam o NBT deles.
pub(in crate::game) fn pipes_save(game: &mut Game<'_>) {
    let dimension = game.native_dimension();
    let chunks: Vec<(i32, i32)> = game.world.modified.iter().copied().collect();
    if chunks.is_empty() {
        return;
    }
    let positions: Vec<[i32; 3]> = with_owner(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
    for pos in positions {
        if chunks.contains(&(pos[0] >> 4, pos[2] >> 4)) {
            with_owner(dimension, |owner| pipe_write(game, owner, pos));
        }
    }
}

/// Os tipos de conexão visíveis do cano em `pos` (`getConnectionType(side)`: `NONE` onde não há braço), na ordem dos lados, como o último `publish` os deixou. `None` fora do
/// servidor, que é onde o dono vive, ou se o cano ainda não publicou.
pub(in crate::game) fn pipe_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    PIPE_VIEWS.with_borrow(|views| views.get(&(game.native_dimension(), pos)).map(|view| view.kind))
}

/// O clique do Configurator num cano (`ItemConfigurator.useOn`: `IConfigurable.onSneakRightClick` agachado, `onRightClick` senão). `side` é a face clicada e `hit_side` o lado do
/// braço que o olhar toca. Devolve se o clique foi do cano e os avisos para o jogador.
pub(in crate::game) fn pipe_configure(
    game: &mut Game<'_>,
    pos: [i32; 3],
    side: Face,
    hit_side: Option<Face>,
    sneaking: bool,
) -> Option<(bool, Vec<PipeOverlay>)> {
    let dimension = game.native_dimension();
    OVERLAYS.with_borrow_mut(Vec::clear);
    let consumed = with_owner(dimension, |owner| {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        if sneaking { owner.sneak_right_click(&mut world, sim_pos(pos), side, hit_side) } else { owner.right_click(&mut world, sim_pos(pos)) }
    })?;
    let overlays = OVERLAYS.with_borrow_mut(std::mem::take);
    Some((consumed, overlays))
}
