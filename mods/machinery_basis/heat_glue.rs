// Os condutores termodinâmicos no jogo (frente `condutor-jogo`, Fase 3). O dono dos condutores (`machinery_basis_core::transmission::heat_owner`) guarda por dimensão o registro, os
// `ThermodynamicConductor` e a rede de calor; esta parte o liga ao jogo como `machinery_pipes` liga o dos canos: os eventos da decisão 107 entram nele, as conexões que ele calcula
// viram as propriedades de lado do bloco, o NBT do condutor (`TileEntityTransmitter.saveAdditional` e o `heat_capacitor/state` de `ThermodynamicConductor.write`) sai a cada mudança
// de calor e no salvamento, e o vizinho com a capability `HEAT` (o Heat Generator) responde à rede lendo e gravando o calor no NBT dele. A lib não conhece nenhum id: o mod diz
// quais blocos são condutores e o nível de cada um em `conductors_register`, e quais blocos têm um capacitor de calor em `heat_neighbor_register`.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use crate::game::machinery_basis::{TransmitterEvent, TransmitterKind};
use horizon_client::containers::BlockEntity;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos as SimPos, ChunkPos};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::heat_owner::{ConductorInfo, HeatOwnerWorld, HeatTransmitters, HostCapacitor};
use machinery_basis_core::transmission::owner::{CableView, Event as HeatEvent};
use machinery_basis_core::transmission::thermodynamic_conductor::{CapacitorState, ConductorTier, TierValues, keys as capacitor_keys};
use machinery_basis_core::transmission::transmitter::{Overlay as HeatOverlay, keys};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

/// O nível e os valores de um bloco de condutor (`ConductorTier` e a config de `tiers.toml`), ou `None` se o bloco não é um condutor.
pub type ConductorStats = fn(&str) -> Option<(ConductorTier, TierValues)>;

/// `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE`: as propriedades de lado só vão aos clientes (`sendUpdatePacket` não avisa vizinho nem refaz forma).
const CONDUCTOR_FLAGS: i32 = 2 | 16;
/// O nome do dono dos condutores entre os que ouvem os eventos dos transmissores.
const SINK_OWNER: &str = "machinery_conductors";

static CONDUCTOR_KINDS: Mutex<Vec<ConductorStats>> = Mutex::new(Vec::new());

thread_local! {
    /// Os condutores de cada dimensão do nível deste servidor.
    static CONDUCTORS: RefCell<HashMap<&'static str, HeatTransmitters>> = RefCell::new(HashMap::new());
    /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
    static OVERLAYS: RefCell<Vec<HeatOverlay>> = const { RefCell::new(Vec::new()) };
    /// O último `publish` de cada condutor (o tipo de conexão visível de cada lado), para a mira do Configurator achar as peças de `getCollisionBoxes`.
    static VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
    /// Os condutores cujo `setChanged` rodou durante o evento: o NBT deles é regravado quando o dono volta ao jogo.
    static DIRTY: RefCell<Vec<[i32; 3]>> = const { RefCell::new(Vec::new()) };
    /// As posições cujo ar a sonda já anotou (`MBA`).
    static AMBIENT_SEEN: RefCell<std::collections::HashSet<[i32; 3]>> = RefCell::new(std::collections::HashSet::new());
    /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon: `(dimensão, posição, tick)`, tentados de novo.
    static PENDING: RefCell<Vec<(&'static str, [i32; 3], i64)>> = const { RefCell::new(Vec::new()) };
}

/// `id` (o `ModBlockEntity.id`) passa a ser um condutor termodinâmico: os eventos do jogo dele vão ao dono dos condutores, e `stats` dá o nível e os valores.
pub fn conductors_register(id: &str, stats: ConductorStats) {
    basis::transmitters_register(SINK_OWNER, id, conductors_event);
    let mut kinds = CONDUCTOR_KINDS.lock().unwrap_or_else(|e| e.into_inner());
    if !kinds.iter().any(|known| *known as usize == stats as usize) {
        kinds.push(stats);
    }
}

fn conductor_stats(block: &str) -> Option<(ConductorTier, TierValues)> {
    let kinds: Vec<ConductorStats> = CONDUCTOR_KINDS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    kinds.into_iter().find_map(|stats| stats(block))
}

fn sim_pos(pos: [i32; 3]) -> SimPos {
    SimPos::new(pos[0], pos[1], pos[2])
}

fn arr(pos: SimPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os vizinhos com a capability `HEAT`. O mod diz o bloco, como ler o capacitor dele visto de um lado e como gravar o calor que a rede lhe deu; a capability existe nos seis lados.

/// O capacitor do bloco em `pos` visto pelo lado `side` dele, ou `None` se o tile não está no mundo agora (está no tick).
pub type HeatProbe = fn(&Game<'_>, &'static str, [i32; 3], Face) -> Option<HostCapacitor>;
/// `handleHeat(transfer, transaction)` no capacitor do bloco em `pos`.
pub type HeatWrite = fn(&mut Game<'_>, &'static str, [i32; 3], f64);

#[derive(Clone, Copy)]
struct HeatNeighbor {
    block: &'static str,
    probe: HeatProbe,
    write: HeatWrite,
}

static NEIGHBORS: Mutex<Vec<HeatNeighbor>> = Mutex::new(Vec::new());

/// O bloco `block` tem um capacitor de calor com a capability `HEAT` nos seis lados: `probe` o lê e `write` grava o calor que a rede lhe deu. Pode ser dito mais de uma vez.
pub fn heat_neighbor_register(block: &'static str, probe: HeatProbe, write: HeatWrite) {
    let mut neighbors = NEIGHBORS.lock().unwrap_or_else(|e| e.into_inner());
    if !neighbors.iter().any(|known| known.block == block) {
        neighbors.push(HeatNeighbor { block, probe, write });
    }
}

fn neighbor_of(block: &str) -> Option<HeatNeighbor> {
    NEIGHBORS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|known| known.block == block).copied()
}

// ---------------------------------------------------------------------------------------------------------------------------
// O jogo visto pelo dono durante um evento.

struct GameWorld<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
}

impl HeatOwnerWorld for GameWorld<'_, '_> {
    fn is_block_loaded(&self, pos: SimPos) -> bool {
        self.game.borrow().world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
    }

    fn is_getting_powered(&self, pos: SimPos) -> bool {
        self.game.borrow_mut().native_neighbor_signal(self.dimension, arr(pos)) > 0
    }

    fn has_host(&self, pos: SimPos, _side: Face) -> bool {
        let game = self.game.borrow();
        let block = game.native_block(arr(pos));
        neighbor_of(&block).is_some()
    }

    fn host(&self, pos: SimPos, side: Face) -> HostCapacitor {
        let game = self.game.borrow();
        let block = game.native_block(arr(pos));
        // O tile que está no tick está fora do mundo: a rede só lê o capacitor no fim do tick (`ServerTickEvent.Post`), com todos de volta, então o valor neutro daqui não chega a ser usado.
        neighbor_of(&block)
            .and_then(|neighbor| (neighbor.probe)(&game, self.dimension, arr(pos), side))
            .unwrap_or(HostCapacitor { temperature: machinery_basis_core::heat::AMBIENT_TEMP, heat_capacity: 1.0, inverse_conduction: 1.0, inverse_insulation: 0.0 })
    }

    fn host_handle_heat(&self, pos: SimPos, _side: Face, transfer: f64) {
        let mut game = self.game.borrow_mut();
        let block = game.native_block(arr(pos));
        if let Some(neighbor) = neighbor_of(&block) {
            (neighbor.write)(&mut **game, self.dimension, arr(pos), transfer);
        }
    }

    fn ambient(&self, pos: SimPos, side: Option<Face>) -> Option<f64> {
        let at = match side {
            Some(side) => {
                let [x, y, z] = side.offset();
                [pos.x + x, pos.y + y, pos.z + z]
            }
            None => arr(pos),
        };
        let value = basis::ambient_temperature(&self.game.borrow(), at);
        if let Some(value) = value {
            ambient_probe(at, value);
        }
        value
    }

    fn conductor_info(&mut self, pos: SimPos) -> Option<ConductorInfo> {
        let game = self.game.borrow();
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos.x, pos.y, pos.z)) else {
            return None;
        };
        let (tier, values) = conductor_stats(&entity.id)?;
        let tag = basis::nbt_to_cable(&entity.nbt);
        let capacitor = read_capacitor(&entity.nbt);
        let from_save = tag.contains_key(keys::CONNECTION) || capacitor.is_some();
        Some(ConductorInfo { tier, values, saved: from_save.then_some((tag, capacitor)) })
    }

    fn publish(&mut self, pos: SimPos, view: &CableView) {
        let at = arr(pos);
        let mut game = self.game.borrow_mut();
        for side in Face::ALL {
            let value = if view.connected[side as usize] { "true" } else { "false" };
            game.native_set_block_property_with(at, side.name(), value, CONDUCTOR_FLAGS);
        }
        if basis::transmitter_probe_enabled() {
            let kinds: Vec<&str> = view.kind.iter().map(|kind| kind.serialized_name()).collect();
            let raws: Vec<&str> = view.raw.iter().map(|kind| kind.serialized_name()).collect();
            eprintln!("MBH {{\"p\":[{},{},{}],\"conn\":{:?},\"kind\":{:?},\"raw\":{:?},\"net\":{}}}", at[0], at[1], at[2], view.connected, kinds, raws, view.has_network);
        }
        VIEWS.with_borrow_mut(|views| views.insert((self.dimension, at), view.clone()));
    }

    fn set_changed(&mut self, pos: SimPos) {
        DIRTY.with_borrow_mut(|dirty| {
            if !dirty.contains(&arr(pos)) {
                dirty.push(arr(pos));
            }
        });
    }

    fn overlay(&mut self, _pos: SimPos, overlay: HeatOverlay) {
        OVERLAYS.with_borrow_mut(|list| list.push(overlay));
    }
}

/// A sonda do ar (`MBA`, só na cópia de teste): o ar de cada posição que a rede leu, uma vez por posição. O teste da lib repete o jogo com esses valores.
fn ambient_probe(at: [i32; 3], value: f64) {
    if basis::transmitter_probe_enabled() && AMBIENT_SEEN.with_borrow_mut(|seen| seen.insert(at)) {
        eprintln!("MBA {{\"at\":[{},{},{}],\"v\":{:?}}}", at[0], at[1], at[2], value);
    }
}

/// O `heat_capacitor/state` que o Horizon guardou (`IHeatCapacitor.serialize`: o `CapacitorState.CODEC` na chave `state`, com o calor e a capacidade).
fn read_capacitor(nbt: &Compound) -> Option<CapacitorState> {
    let Some(Tag::Compound(container)) = nbt.get(&JavaString::from(capacitor_keys::HEAT_CAPACITOR)) else {
        return None;
    };
    let Some(Tag::Compound(state)) = container.get(&JavaString::from(capacitor_keys::STATE)) else {
        return None;
    };
    let number = |key: &str| match state.get(&JavaString::from(key)) {
        Some(Tag::Double(bits)) => Some(f64::from_bits(*bits)),
        _ => None,
    };
    CapacitorState::decode(number(capacitor_keys::HEAT_STORED), number(capacitor_keys::HEAT_CAPACITY))
}

/// `ContainerType.HEAT.saveTo`: o `CapacitorState.CODEC` omite o campo que vale o padrão (`CapacitorState::encode`), e um filho vazio é descartado.
fn write_capacitor(nbt: &mut Compound, state: &CapacitorState) {
    let container_key = JavaString::from(capacitor_keys::HEAT_CAPACITOR);
    let (heat, capacity) = state.encode();
    let mut fields = Compound::default();
    if let Some(heat) = heat {
        fields.insert(JavaString::from(capacitor_keys::HEAT_STORED), Tag::Double(heat.to_bits()));
    }
    if let Some(capacity) = capacity {
        fields.insert(JavaString::from(capacitor_keys::HEAT_CAPACITY), Tag::Double(capacity.to_bits()));
    }
    if fields.is_empty() {
        nbt.remove(&container_key);
        return;
    }
    let mut container = Compound::default();
    container.insert(JavaString::from(capacitor_keys::STATE), Tag::Compound(fields));
    nbt.insert(container_key, Tag::Compound(container));
}

fn new_owner() -> HeatTransmitters {
    HeatTransmitters::new(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
}

/// Roda `f` no dono dos condutores da dimensão. `None` se o dono já está ocupado (um evento dentro de outro).
fn with_owner<R>(dimension: &'static str, f: impl FnOnce(&mut HeatTransmitters) -> R) -> Option<R> {
    CONDUCTORS.with(|conductors| {
        let mut conductors = conductors.try_borrow_mut().ok()?;
        Some(f(conductors.entry(dimension).or_insert_with(new_owner)))
    })
}

/// O sink dos eventos dos transmissores: traduz o evento do jogo para o do dono dos condutores.
fn conductors_event(game: &mut Game<'_>, event: &TransmitterEvent) {
    let dimension = game.native_dimension();
    conductors_retry_pending(game, dimension);
    conductors_deliver(game, dimension, event);
    // Um `ClearRemoved` sem block entity no mundo ainda: tenta de novo nos eventos seguintes.
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::ClearRemoved, .. } = event {
        let created = with_owner(dimension, |owner| owner.conductor_at(sim_pos(*pos)).is_some()).unwrap_or(true);
        if !created && is_conductor_entity(game, *pos) {
            let now = game.world.game_time;
            PENDING.with_borrow_mut(|pending| pending.push((dimension, *pos, now)));
        }
    }
}

/// O block entity em `pos` é um condutor (ou ainda não está no mundo, e então a pergunta é de outro dono).
fn is_conductor_entity(game: &Game<'_>, pos: [i32; 3]) -> bool {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => conductor_stats(&entity.id).is_some(),
        _ => false,
    }
}

/// Os `ClearRemoved` que esperavam o block entity: o que já o tem entra, o que esperou demais sai da fila.
fn conductors_retry_pending(game: &mut Game<'_>, dimension: &'static str) {
    let waiting: Vec<(&'static str, [i32; 3], i64)> = PENDING.with_borrow_mut(|pending| std::mem::take(pending));
    for (dim, pos, since) in waiting {
        if dim != dimension {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
            continue;
        }
        if is_conductor_entity(game, pos) {
            conductors_deliver(game, dimension, &TransmitterEvent::Transmitter { dim: dimension.to_owned(), pos, kind: TransmitterKind::ClearRemoved });
        } else if game.world.game_time - since < 200 {
            PENDING.with_borrow_mut(|pending| pending.push((dim, pos, since)));
        }
    }
}

fn conductors_deliver(game: &mut Game<'_>, dimension: &'static str, event: &TransmitterEvent) {
    let event = match event {
        TransmitterEvent::Transmitter { pos, kind, .. } => {
            let pos = sim_pos(*pos);
            match kind {
                TransmitterKind::ClearRemoved => HeatEvent::ClearRemoved(pos),
                TransmitterKind::PlacedByItem => HeatEvent::PlacedByItem(pos),
                TransmitterKind::ChunkUnloaded => HeatEvent::ChunkUnloaded(pos),
                TransmitterKind::Removed => HeatEvent::Removed(pos),
                TransmitterKind::Update => HeatEvent::Update(pos),
                TransmitterKind::Neighbor => HeatEvent::Neighbor(pos),
                TransmitterKind::Capability(side) => HeatEvent::Capability(pos, Face::ALL[*side]),
            }
        }
        TransmitterEvent::Ticket { chunk, loaded, .. } => HeatEvent::Ticket { chunk: ChunkPos::new(chunk[0], chunk[1]), loaded: *loaded },
        TransmitterEvent::ServerTickPost { runs_normally } => HeatEvent::ServerTickPost { runs_normally: *runs_normally },
        TransmitterEvent::Reset => {
            CONDUCTORS.with_borrow_mut(HashMap::clear);
            VIEWS.with_borrow_mut(HashMap::clear);
            DIRTY.with_borrow_mut(Vec::clear);
            return;
        }
    };
    // `onChunkUnloaded` e o chunk vai para o disco: o NBT sai agora, antes do `setRemoved` que tira o condutor do dono.
    let unloaded = if let HeatEvent::ChunkUnloaded(pos) = &event { Some(arr(*pos)) } else { None };
    let removed = if let HeatEvent::Removed(pos) = &event { Some(arr(*pos)) } else { None };
    let post = matches!(event, HeatEvent::ServerTickPost { .. });
    let runs_normally = matches!(event, HeatEvent::ServerTickPost { runs_normally: true });
    with_owner(dimension, |owner| {
        {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            owner.handle(&mut world, event);
        }
        if let Some(pos) = unloaded {
            DIRTY.with_borrow_mut(|dirty| dirty.push(pos));
        }
        flush_dirty(game, owner, dimension);
        if post && basis::transmitter_probe_enabled() {
            probe_line(game, owner, runs_normally);
        }
    });
    if let Some(pos) = removed {
        VIEWS.with_borrow_mut(|views| views.remove(&(dimension, pos)));
    }
}

/// A sonda do calor (`MBQ`, só na cópia de teste): o calor de cada condutor e a temperatura média, a perda e a troca de cada rede neste tick.
fn probe_line(game: &Game<'_>, owner: &HeatTransmitters, runs_normally: bool) {
    let nets: Vec<String> = owner
        .registry()
        .networks_in_order()
        .into_iter()
        .filter_map(|id| {
            let heat = owner.heat_networks().get(id)?;
            let registered = owner.registry().network(id);
            (registered.transmitters_size() > 0).then(|| format!("[{:?},{:?},{:?},{},{}]", heat.mean_temp(), heat.heat_lost(), heat.heat_transferred(), registered.transmitters_size(), registered.acceptor_count()))
        })
        .collect();
    let conductors: Vec<String> = owner
        .positions()
        .into_iter()
        .filter_map(|pos| {
            let state = owner.state_at(pos)?;
            let heat = state.heat.map_or_else(|| "null".to_owned(), |heat| format!("{heat:?}"));
            Some(format!("[{},{},{},{},{}]", pos.x, pos.y, pos.z, heat, state.packets))
        })
        .collect();
    eprintln!("MBQ {{\"t\":{},\"runs\":{},\"nets\":[{}],\"cond\":[{}]}}", game.world.game_time, runs_normally, nets.join(","), conductors.join(","));
}

/// `setChanged`: o NBT de cada condutor que mudou volta ao block entity (o `data get` e o próximo salvamento leem o calor de agora) e o chunk dele fica marcado para salvar.
fn flush_dirty(game: &mut Game<'_>, owner: &mut HeatTransmitters, dimension: &'static str) {
    let dirty: Vec<[i32; 3]> = DIRTY.with_borrow_mut(std::mem::take);
    for pos in dirty {
        conductor_write(game, owner, dimension, pos);
    }
}

/// `TileEntityTransmitter.saveAdditional` e `ThermodynamicConductor.write`: grava no block entity de `pos` o que o condutor guarda.
fn conductor_write(game: &mut Game<'_>, owner: &HeatTransmitters, dimension: &'static str, pos: [i32; 3]) {
    let tag = {
        let world = GameWorld { game: RefCell::new(&mut *game), dimension };
        owner.write(&world, sim_pos(pos))
    };
    let Some(tag) = tag else {
        return;
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        basis::cable_to_nbt(&tag.transmitter, &mut entity.nbt);
        write_capacitor(&mut entity.nbt, &tag.capacitor);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
}

/// O salvamento do nível (`Game::save_level`, antes de ele esvaziar os chunks alterados): os condutores dos chunks que vão para o disco gravam o NBT deles.
pub(in crate::game) fn conductors_save(game: &mut Game<'_>) {
    let dimension = game.native_dimension();
    let chunks: Vec<(i32, i32)> = game.world.modified.iter().copied().collect();
    if chunks.is_empty() {
        return;
    }
    with_owner(dimension, |owner| {
        for pos in owner.positions().into_iter().map(arr) {
            if chunks.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                conductor_write(game, owner, dimension, pos);
            }
        }
    });
}

/// `ITileHeatHandler.simulateAdjacent` de um tile que entrega calor pelo lado `side` dele (o Heat Generator, por baixo): o vizinho nesse lado, se é um condutor com a capability
/// `HEAT` voltada ao tile, recebe o calor, e o capacitor dele confirma no fim (o `transaction.commit()` do tile). Devolve a temperatura que passou.
pub fn heat_adjacent(game: &mut Game<'_>, pos: [i32; 3], side: Face, capacitor: &mut Capacitor) -> f64 {
    let dimension = game.native_dimension();
    let [x, y, z] = side.offset();
    let neighbor = SimPos::new(pos[0] + x, pos[1] + y, pos[2] + z);
    if basis::transmitter_probe_enabled() {
        // O ar que o gerador lê (o dele e o de cada vizinho): o `CachedAmbientTemperature` dele guarda os mesmos valores.
        for face in std::iter::once(None).chain(Face::ALL.into_iter().map(Some)) {
            let at = match face {
                Some(face) => {
                    let [x, y, z] = face.offset();
                    [pos[0] + x, pos[1] + y, pos[2] + z]
                }
                None => pos,
            };
            if let Some(value) = basis::ambient_temperature(game, at) {
                ambient_probe(at, value);
            }
        }
    }
    let transfer = with_owner(dimension, |owner| {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        let transfer = match owner.sink_at(&world, neighbor, side.opposite()) {
            Some(mut sink) => capacitor.simulate_adjacent(&mut sink),
            None => 0.0,
        };
        owner.commit_pending(&mut world);
        flush_dirty(game, owner, dimension);
        transfer
    });
    transfer.unwrap_or(0.0)
}

/// Os tipos de conexão visíveis do condutor em `pos` (`getConnectionType(side)`: `NONE` onde não há braço), na ordem dos lados, como o último `publish` os deixou. `None` fora do
/// servidor, que é onde o dono vive, ou se o condutor ainda não publicou.
pub(in crate::game) fn conductor_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    VIEWS.with_borrow(|views| views.get(&(game.native_dimension(), pos)).map(|view| view.kind))
}

/// O clique do Configurator num condutor (`ItemConfigurator.useOn`: `IConfigurable.onSneakRightClick` agachado, `onRightClick` senão). `side` é a face clicada e `hit_side` o lado do
/// braço que o olhar toca. Devolve se o clique foi do condutor e os avisos para o jogador.
pub(in crate::game) fn conductor_configure(
    game: &mut Game<'_>,
    pos: [i32; 3],
    side: Face,
    hit_side: Option<Face>,
    sneaking: bool,
) -> Option<(bool, Vec<HeatOverlay>)> {
    let dimension = game.native_dimension();
    OVERLAYS.with_borrow_mut(Vec::clear);
    let consumed = with_owner(dimension, |owner| {
        let consumed = {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            if sneaking { owner.sneak_right_click(&mut world, sim_pos(pos), side, hit_side) } else { owner.right_click(&mut world, sim_pos(pos)) }
        };
        flush_dirty(game, owner, dimension);
        consumed
    })?;
    let overlays = OVERLAYS.with_borrow_mut(std::mem::take);
    Some((consumed, overlays))
}
