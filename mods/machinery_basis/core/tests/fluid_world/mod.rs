//! O mundo simulado dos testes da rede de fluido: canos mecânicos (o `Transmitter` da `rede-conexao`, o registro da `rede-registro`, a
//! família de fluido da lib) sobre os chunks falsos de `sim::chunks`, ligados como o `TileEntityTransmitter` os liga, e tanques como
//! vizinhos. Quem chama o registro é este arquivo; o cano, a rede e a fusão são os da lib.
#![allow(dead_code)]
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::resource_tank::{ResourceTank, StackContents};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, ChunkPos, LevelView, SimLevel};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::buffered_resource_network::{
    ResourceAcceptor, ResourceExtract,
};
use machinery_basis_core::transmission::buffered_transmitter::{
    extra_valid_transmitter, merge_networks_manually, recheck_connections_sides, requests_update,
};
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::fluid_network::FluidFamily;
use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::transmitter::{
    BlockPos as TPos, Compound, Effect, NeighborTransmitter, Transmitter, TransmitterWorld,
    relative,
};
use machinery_basis_core::transmission::validator::CompatibleTransmitterValidator;
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// O bloco de um tanque.
pub const TANK: &str = "test:tank";
/// O prefixo do bloco do cano: `test:pipe_<tier>`.
pub const PIPE_PREFIX: &str = "test:pipe_";
pub const DIMENSION: DimensionId = DimensionId(0);

/// `NeighborUpdater.UPDATE_ORDER`.
const UPDATE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

/// O aceitador de fluido que o vizinho dá ao cano (`ResourceHandler<FluidResource>`).
pub type Acceptor = Rc<dyn ResourceAcceptor<FluidResource>>;
pub type Family = FluidFamily<dyn ResourceAcceptor<FluidResource>>;
type Registry = TransmitterNetworkRegistry<Acceptor>;

pub fn fluid(id: &str) -> FluidResource {
    FluidResource::new(id)
}

/// Um tanque vizinho: o contêiner, e os lados em que ele expõe a capability de fluido.
pub struct Tank {
    pub tank: ResourceTank<FluidResource>,
    /// Os lados (por `Face::ALL`) que expõem a capability.
    pub sides: [bool; 6],
    /// A origem da quantidade infinita (o tanque criativo): `extract` entrega sem tirar nada do contêiner.
    pub infinite: bool,
}

impl Tank {
    pub fn exposes(&self, side: Face) -> bool {
        self.sides[Face::ALL.iter().position(|f| *f == side).unwrap()]
    }
}

impl ResourceInsert<FluidResource> for Tank {
    fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        self.tank
            .insert(resource, amount, transaction, AutomationType::External)
    }
}

impl ResourceExtract<FluidResource> for Tank {
    fn size(&self) -> usize {
        1
    }

    fn resource_at(&self, _: usize) -> Option<FluidResource> {
        self.tank.resource()
    }

    fn amount_as_int_at(&self, _: usize) -> i32 {
        self.tank.amount_as_int()
    }

    fn extract_at(
        &self,
        _: usize,
        resource: &FluidResource,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        self.extract(resource, amount, transaction)
    }

    fn extract(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        if self.infinite {
            if self.tank.resource().as_ref() == Some(resource) {
                amount.min(self.tank.amount_as_int())
            } else {
                0
            }
        } else {
            self.tank
                .extract(resource, amount, transaction, AutomationType::External)
        }
    }
}

impl ResourceAcceptor<FluidResource> for Tank {}

/// O `MechanicalPipe` e o `TileEntityMechanicalPipe` dele, no que a rede lê.
pub struct Pipe {
    pub pos: BlockPos,
    /// Fora do lugar enquanto uma operação o usa (veja `Ctx::with_tr`).
    pub tr: Option<Transmitter<Acceptor>>,
    /// `markJoined`.
    pub mark_joined: bool,
    /// A cópia da rede e do órfão que o `Transmitter` guarda, para ler com ele fora do lugar.
    pub network: Option<NetworkId>,
    pub orphan: bool,
    /// O `System.identityHashCode` do transmissor.
    pub hash: i32,
    pub tier: PipeTier,
}

impl Pipe {
    pub fn tr(&self) -> &Transmitter<Acceptor> {
        self.tr.as_ref().expect("cano em uso")
    }

    /// `hasTransmitterNetwork()`.
    pub fn net(&self) -> Option<NetworkId> {
        self.network.filter(|_| !self.orphan)
    }
}

/// O que a rede fez, na ordem, para o teste comparar com o rastro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Update(BlockPos),
    Commit(NetworkId),
    Invalidate(NetworkId),
    NetUpdate(NetworkId),
    Adopt {
        network: NetworkId,
        from: NetworkId,
    },
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
    Access(BlockPos, bool),
}

pub struct State {
    pub pipes: Vec<Pipe>,
    /// O `identityHashCode` do próximo cano posto em cada posição.
    pub hash_for: Box<dyn FnMut(BlockPos) -> i32>,
    pub events: Vec<Event>,
    pub runs_normally: bool,
    pub family: Family,
    /// Os tanques postos, por posição.
    pub tanks: BTreeMap<BlockPos, Rc<Tank>>,
    pending_uuid: Option<Uuid>,
}

pub struct Hooks {
    pub registry: Registry,
    pub state: State,
}

/// O que o registro enxerga num instante: os canos e os chunks do nível.
pub struct Ctx<'a> {
    pub st: &'a mut State,
    pub level: &'a LevelView<TransmitterId>,
}

fn tpos(pos: BlockPos) -> TPos {
    [pos.x, pos.y, pos.z]
}

fn spos(pos: TPos) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

impl Ctx<'_> {
    pub fn pipe(&self, id: TransmitterId) -> &Pipe {
        &self.st.pipes[id.0 as usize]
    }

    fn pipe_mut(&mut self, id: TransmitterId) -> &mut Pipe {
        &mut self.st.pipes[id.0 as usize]
    }

    fn id_at(&self, pos: TPos) -> Option<TransmitterId> {
        self.level.block_entity(spos(pos)).copied()
    }

    /// Roda uma operação do `Transmitter` com ele fora do lugar (ela lê o mundo, e o mundo guarda o transmissor), e aplica os
    /// efeitos que ela pediu.
    fn with_tr<T>(
        &mut self,
        registry: &mut Registry,
        id: TransmitterId,
        f: impl FnOnce(&mut Transmitter<Acceptor>, &Self, &mut Vec<Effect>) -> T,
    ) -> T {
        let mut tr = self.pipe_mut(id).tr.take().expect("cano em uso");
        let mut out = Vec::new();
        let result = f(&mut tr, &*self, &mut out);
        self.pipe_mut(id).tr = Some(tr);
        self.apply(registry, id, out);
        result
    }

    fn apply(&mut self, registry: &mut Registry, id: TransmitterId, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => self.notify_neighbors(registry, tpos(self.pipe(id).pos)),
                Effect::RefreshNeighbor { pos, side } => {
                    if let Some(other) = self.id_at(pos) {
                        self.with_tr(registry, other, |t, w, out| {
                            t.refresh_connections_side(side, w, out);
                        });
                    }
                }
                Effect::RecheckConnections(mask) => {
                    // `BufferedTransmitter.recheckConnections`.
                    let net = self.pipe(id).net();
                    match recheck_connections_sides(net.is_some(), true, mask) {
                        None => {
                            self.with_tr(registry, id, |t, w, out| {
                                t.base_recheck_connections(mask, w, out);
                            });
                        }
                        Some(sides) => {
                            for side in sides {
                                self.recheck_connection_prechecked(registry, id, side);
                            }
                        }
                    }
                }
                Effect::RecheckConnection(side) => {
                    // `BufferedTransmitter.recheckConnection`.
                    if self.pipe(id).net().is_some() {
                        self.recheck_connection_prechecked(registry, id, side);
                    }
                }
                Effect::RequestsUpdate => self.requests_update(registry, id),
                Effect::InvalidateTransmitter => {
                    registry.invalidate_transmitter(&*self, id);
                }
                Effect::AcceptorChanged(side) => {
                    let network = self.pipe(id).network.expect("só há AcceptorChanged com rede");
                    registry.network_acceptor_changed(&*self, network, id, side);
                }
                Effect::SetForceUpdate => {
                    self.pipe_mut(id).tr.as_mut().unwrap().set_force_update();
                }
                _ => {}
            }
        }
    }

    /// `BufferedTransmitter.requestsUpdate()`: o `update` do rastro e as conexões relidas.
    fn requests_update(&mut self, registry: &mut Registry, id: TransmitterId) {
        let pos = self.pipe(id).pos;
        self.st.events.push(Event::Update(pos));
        self.with_tr(registry, id, |t, w, out| requests_update(t, true, w, out));
    }

    /// `BufferedTransmitter.recheckConnectionPrechecked(side)`.
    fn recheck_connection_prechecked(
        &mut self,
        registry: &mut Registry,
        id: TransmitterId,
        side: Face,
    ) {
        let pos = self.pipe(id).pos;
        let Some(other) = self.level.block_entity(pos.relative(side)).copied() else {
            return;
        };
        let (Some(network), Some(other_network)) = (self.pipe(id).net(), self.pipe(other).net())
        else {
            return;
        };
        let Some((primary, secondary)) = self.st.family.plan_merge(id, network, other_network)
        else {
            return;
        };
        let adopted = merge_networks_manually(registry, self, primary, secondary);
        self.st.family.clamp_buffer(primary);
        self.with_tr(registry, other, |t, w, out| {
            t.refresh_connections_side(side.opposite(), w, out);
        });
        for transmitter in adopted {
            self.requests_update(registry, transmitter);
        }
    }

    /// Os vizinhos recebem `neighborChanged` sem orientação.
    fn notify_neighbors(&mut self, registry: &mut Registry, pos: TPos) {
        for side in UPDATE_ORDER {
            if let Some(id) = self.id_at(relative(pos, side)) {
                self.with_tr(registry, id, |t, w, out| {
                    t.on_neighbor_block_change(None, w, out);
                });
            }
        }
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(&mut self, registry: &mut Registry, id: TransmitterId, was_present: bool) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.pipe(id).tr().loaded {
            self.pipe_mut(id).tr.as_mut().unwrap().loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(&mut self, registry: &mut Registry, id: TransmitterId, still_present: bool) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.pipe(id).tr().loaded {
            self.pipe_mut(id).tr.as_mut().unwrap().loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

impl TransmitterWorld for Ctx<'_> {
    type Acceptor = Acceptor;

    fn is_block_loaded(&self, pos: TPos) -> bool {
        self.level.is_block_loaded(spos(pos))
    }

    fn is_getting_powered(&self, _: TPos) -> bool {
        false
    }

    fn transmitter_at(&self, pos: TPos, side: Face) -> Option<NeighborTransmitter> {
        let tr = self.pipe(self.id_at(pos)?).tr.as_ref()?;
        Some(NeighborTransmitter {
            supported: tr.supported_transmission_types(),
            can_connect_opposite: tr.can_connect(side.opposite(), self),
        })
    }

    /// A capability de fluido do vizinho: o tanque, nos lados em que ele a expõe.
    fn acceptor(&self, pos: TPos, from_side: Face) -> Option<Acceptor> {
        let pos = spos(pos);
        if self.level.block_if_loaded(pos) != Some(TANK) {
            return None;
        }
        let tank = self.st.tanks.get(&pos)?;
        tank.exposes(from_side).then(|| tank.clone() as Acceptor)
    }

    /// `MechanicalPipe.isValidTransmitter(transmitter, side)`, a parte que a regra básica não tem: `BufferedTransmitter` (rede contra
    /// órfão) e o conteúdo dos buffers.
    fn extra_valid_transmitter(&self, me: TPos, other: TPos, _side: Face) -> bool {
        let (Some(a), Some(b)) = (self.id_at(me), self.id_at(other)) else {
            return true;
        };
        let (pa, pb) = (self.pipe(a), self.pipe(b));
        let ends = |p: &Pipe| self.st.family.ends(p.net().is_some(), p.orphan);
        extra_valid_transmitter(ends(pa), ends(pb))
            && self.st.family.is_valid_transmitter(a, pa.net(), b, pb.net())
    }
}

impl TransmitterHost<Acceptor> for Ctx<'_> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.pipe(transmitter).pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        DIMENSION
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        self.pipe(transmitter).tr().is_valid()
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.pipe(transmitter).orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        let pipe = self.pipe_mut(transmitter);
        pipe.orphan = orphan;
        if let Some(tr) = pipe.tr.as_mut() {
            tr.set_orphan(orphan);
        }
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.pipe(transmitter).network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        let pipe = self.pipe_mut(transmitter);
        pipe.network = network;
        // A cópia do `Transmitter` da `rede-conexao`: o UUID dela é só uma marca de "tem rede".
        if let Some(tr) = pipe.tr.as_mut() {
            tr.set_transmitter_network(
                network.map(|n| u128::from(n.0) + 1),
                false,
                &mut Vec::new(),
            );
        }
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.pipe(transmitter).hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        panic!("o registro chama request_update_in; {transmitter:?}");
    }

    fn request_update_in(&mut self, registry: &mut Registry, transmitter: TransmitterId) {
        self.requests_update(registry, transmitter);
    }

    fn supports_transmission_type(&self, _: TransmitterId, _: TransmitterId) -> bool {
        true
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        let neighbor = TransmitterWorld::transmitter_at(self, tpos(self.pipe(other).pos), side)
            .expect("o vizinho existe");
        self.pipe(transmitter)
            .tr()
            .is_valid_transmitter_basic(&neighbor, side, self)
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.level.block_entity(pos).copied()
    }

    fn new_orphan_validator(
        &self,
        transmitter: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<Acceptor>> {
        let pipe = self.pipe(transmitter);
        Box::new(self.st.family.new_orphan_validator(transmitter, pipe.net()))
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut Registry,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        let pos = self.pipe(transmitter).pos;
        self.st.events.push(Event::Access(pos, loaded));
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            let network = self.pipe(transmitter).net();
            self.st
                .family
                .validate_and_take_share(registry, transmitter, network, None);
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, t: TransmitterId, side: Face) {
        let mut tr = self.pipe_mut(t).tr.take().unwrap();
        let mut out = Vec::new();
        tr.refresh_acceptor_connections(side, &*self, &mut out);
        self.pipe_mut(t).tr = Some(tr);
        // Os efeitos de `refreshAcceptorConnections` são só o pacote de atualização.
        debug_assert!(out.iter().all(|e| matches!(e, Effect::SendUpdatePacket)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Acceptor> {
        let tr = self.pipe(transmitter).tr();
        tr.can_connect_to_acceptor(side)
            .then(|| tr.get_acceptor(side, self))
            .flatten()
    }

    fn take_share(&mut self, transmitter: TransmitterId) {
        let network = self.pipe(transmitter).net();
        self.st.family.take_share_in_root(transmitter, network);
    }

    fn on_update(&mut self, registry: &mut Registry, network: NetworkId) {
        self.st.events.push(Event::NetUpdate(network));
        self.st.family.on_update(registry, network);
    }

    fn network_event(&mut self, registry: &Registry, network: NetworkId, event: NetworkEvent) {
        self.st
            .family
            .network_event(registry, network, event.clone());
        match event {
            NetworkEvent::Created { uuid } => self.st.pending_uuid = Some(uuid),
            NetworkEvent::Merged { from } => {
                let uuid = self.st.pending_uuid.take().expect("Created vem antes");
                self.st.events.push(Event::Created {
                    network,
                    hash: uuid.hash_code(),
                    from,
                });
            }
            NetworkEvent::Adopted { from } => self.st.events.push(Event::Adopt { network, from }),
            NetworkEvent::Committing => self.st.events.push(Event::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.events.push(Event::Invalidate(network)),
            _ => {}
        }
    }
}

/// O que um cano grava no chunk: o NBT do `Transmitter` e o `stored`.
pub type Saved = (Compound, StackContents<FluidResource>);

impl ChunkHooks for Hooks {
    type BlockEntity = TransmitterId;
    type Saved = Saved;

    fn new_block_entity(
        &mut self,
        pos: BlockPos,
        block: &str,
        saved: Option<&Saved>,
    ) -> Option<TransmitterId> {
        let tier = match block.strip_prefix(PIPE_PREFIX)? {
            "basic" => PipeTier::Basic,
            "advanced" => PipeTier::Advanced,
            "elite" => PipeTier::Elite,
            "ultimate" => PipeTier::Ultimate,
            other => panic!("tier desconhecido: {other}"),
        };
        let hash = (self.state.hash_for)(pos);
        let mut tr: Transmitter<Acceptor> = Transmitter::new(
            tpos(pos),
            TransmissionSet::of(&[TransmissionType::Fluid]),
            true,
        );
        let id = TransmitterId(self.state.pipes.len() as u32);
        if let Some((tag, stored)) = saved {
            tr.read(tag, &mut Vec::new());
            self.state.family.add_mechanical_pipe(id, pos, tier);
            self.state.family.read_stored(id, stored.clone());
        } else {
            self.state.family.add_mechanical_pipe(id, pos, tier);
        }
        self.state.pipes.push(Pipe {
            pos,
            tr: Some(tr),
            mark_joined: false,
            network: None,
            orphan: true,
            hash,
            tier,
        });
        Some(id)
    }

    /// `MechanicalPipe.write`: valida as shares e grava o `saveShare`, e o NBT do `Transmitter`.
    fn save_block_entity(&mut self, _: BlockPos, entity: &mut TransmitterId) -> Saved {
        let network = self.state.pipes[entity.0 as usize].net();
        let stored = self
            .state
            .family
            .write_stored(&self.registry, *entity, network);
        (self.state.pipes[entity.0 as usize].tr().write(), stored)
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block.starts_with(PIPE_PREFIX)
    }

    fn ticket_level_updated(
        &mut self,
        _: &LevelView<TransmitterId>,
        chunk: ChunkPos,
        old: i32,
        new: i32,
    ) {
        self.registry
            .on_ticket_level_change(DIMENSION, chunk.pack(), old, new);
    }

    /// `TileEntityTransmitter.clearRemoved`, `:146`, no servidor; `setLevel` antes.
    fn block_entity_added(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let pipe = &mut self.state.pipes[entity.0 as usize];
        let tr = pipe.tr.as_mut().unwrap();
        tr.set_level(false);
        tr.removed = false;
        pipe.mark_joined = true;
    }

    /// `TileEntityResourceTransmitter.onUpdateServer`: `pullFromAcceptors` e depois `TileEntityTransmitter.onUpdateServer`.
    fn block_entity_tick(
        &mut self,
        level: &LevelView<TransmitterId>,
        pos: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let id = *entity;
        self.pull(level, pos, id);
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        if ctx.pipe(id).mark_joined {
            ctx.world_join(&mut self.registry, id, false);
            ctx.pipe_mut(id).mark_joined = false;
        }
        ctx.with_tr(&mut self.registry, id, |t, w, out| t.on_update_server(w, out));
    }

    /// `TileEntityTransmitter.onChunkUnloaded`, `:156`: `validateAndTakeShare`.
    fn block_entity_chunk_unloaded(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let network = self.state.pipes[entity.0 as usize].net();
        self.state
            .family
            .validate_and_take_share(&self.registry, *entity, network, None);
    }

    /// `TileEntityTransmitter.setRemoved`, `:165`.
    fn block_entity_removed(
        &mut self,
        level: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        self.state.pipes[entity.0 as usize]
            .tr
            .as_mut()
            .unwrap()
            .removed = true;
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        ctx.world_separate(&mut self.registry, *entity, false);
    }

    /// `Level.setBlock` confirmou um estado novo: o `BlockCapabilityCache` de cada cano vizinho, no lado que olha `pos`, se invalida.
    fn block_changed(
        &mut self,
        level: &LevelView<TransmitterId>,
        pos: BlockPos,
        _old: Option<&str>,
        _new: Option<&str>,
    ) {
        for side in Face::ALL {
            let Some(&id) = level.block_entity(pos.relative(side)) else {
                continue;
            };
            let mut out = Vec::new();
            {
                let tr = self.state.pipes[id.0 as usize].tr();
                if let Some(dirty) = tr
                    .acceptor_cache
                    .refresh_listener_run(side.opposite(), tr.is_valid())
                {
                    tr.mark_dirty_acceptor(dirty, &mut out);
                }
            }
            let mut ctx = Ctx {
                st: &mut self.state,
                level,
            };
            ctx.apply(&mut self.registry, id, out);
        }
    }

    /// `BlockTransmitter.neighborChanged`: `onNeighborBlockChange(null)`.
    fn neighbor_changed(
        &mut self,
        level: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
        _: bool,
    ) {
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        ctx.with_tr(&mut self.registry, *entity, |t, w, out| {
            t.on_neighbor_block_change(None, w, out);
        });
    }

    /// `TransmitterNetworkRegistry.onTick`.
    fn server_tick_post(&mut self, level: &LevelView<TransmitterId>) {
        let runs_normally = self.state.runs_normally;
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        self.registry.on_tick(&mut ctx, runs_normally);
    }
}

impl Hooks {
    /// `BufferedResourceTransmitter.pullFromAcceptors(level)`.
    fn pull(&mut self, level: &LevelView<TransmitterId>, pos: BlockPos, id: TransmitterId) {
        let pipe = &self.state.pipes[id.0 as usize];
        let (net, has_pull) = (pipe.net(), pipe.tr().has_pull_side());
        // A família sai do estado enquanto lê o mundo (os aceitadores), que a contém.
        let mut family = std::mem::take(&mut self.state.family);
        {
            let ctx = Ctx {
                st: &mut self.state,
                level,
            };
            let tr = ctx.pipe(id).tr();
            family.pull_from_acceptors(
                id,
                net,
                has_pull,
                &|side| tr.is_connection_type(side, ConnectionType::Pull),
                &|side| {
                    tr.acceptor_cache.get_connected_acceptor(side, &|s| {
                        TransmitterWorld::acceptor(&ctx, tpos(pos.relative(s)), s.opposite())
                    })
                },
            );
        }
        self.state.family = family;
    }
}

/// O mundo de um cenário: o nível, e a origem da estrutura (a posição do cano 0).
pub struct World {
    pub level: SimLevel<Hooks>,
    pub origin: BlockPos,
    /// A largura da estrutura: o cano de índice `i` está em `x = i % width`, `z = i / width`.
    pub width: i32,
}

impl World {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `hash_for` o `identityHashCode` de cada cano novo.
    pub fn new(
        origin: BlockPos,
        width: i32,
        uuid_hashes: Vec<i32>,
        hash_for: impl FnMut(BlockPos) -> i32 + 'static,
    ) -> Self {
        let queue = Rc::new(RefCell::new(VecDeque::from(uuid_hashes)));
        let registry = TransmitterNetworkRegistry::new(move || {
            Uuid::with_hash_code(
                queue
                    .borrow_mut()
                    .pop_front()
                    .expect("mais redes do que o rastro tem"),
            )
        });
        let hooks = Hooks {
            registry,
            state: State {
                pipes: Vec::new(),
                hash_for: Box::new(hash_for),
                events: Vec::new(),
                runs_normally: true,
                family: Family::new(),
                tanks: BTreeMap::new(),
                pending_uuid: None,
            },
        };
        Self {
            level: SimLevel::new(hooks),
            origin,
            width,
        }
    }

    /// A posição do cano de índice `index` da estrutura.
    pub fn pipe_pos(&self, index: i32) -> BlockPos {
        BlockPos::new(
            self.origin.x + index % self.width,
            self.origin.y,
            self.origin.z + index / self.width,
        )
    }

    /// O índice da posição na estrutura (`z * width + x`), ou `None` fora dela.
    pub fn index_of(&self, pos: BlockPos, depth: i32) -> Option<i32> {
        let (x, z) = (pos.x - self.origin.x, pos.z - self.origin.z);
        ((0..self.width).contains(&x) && (0..depth).contains(&z) && pos.y == self.origin.y)
            .then_some(z * self.width + x)
    }

    pub fn registry(&self) -> &Registry {
        &self.level.hooks.registry
    }

    pub fn family(&self) -> &Family {
        &self.level.hooks.state.family
    }

    pub fn place_pipe(&mut self, x: i32, tier: &str) {
        let pos = self.pipe_pos(x);
        let block = format!("{PIPE_PREFIX}{tier}");
        assert!(self.level.set_block(pos, Some(&block)), "cano {x}");
    }

    /// Um tanque em `pos`, com a capacidade, o fluido e os lados dados.
    pub fn place_tank(
        &mut self,
        pos: BlockPos,
        capacity: u64,
        content: Option<(&str, u64)>,
        sides: [bool; 6],
        infinite: bool,
    ) -> Rc<Tank> {
        let tank = ResourceTank::open(capacity, None);
        if let Some((id, amount)) = content {
            tank.set_contents(Some(fluid(id)), amount, None);
        }
        let tank = Rc::new(Tank {
            tank,
            sides,
            infinite,
        });
        self.level.hooks.state.tanks.insert(pos, tank.clone());
        assert!(self.level.set_block(pos, Some(TANK)), "tanque {pos:?}");
        tank
    }

    /// Um tick do nível: o relógio do jogo (`getGameTime`) anda antes de tudo o que roda nele.
    pub fn tick_level(&mut self) {
        self.level.hooks.state.family.game_time += 1;
        self.level.tick_level();
    }

    pub fn break_block(&mut self, pos: BlockPos) {
        self.level.set_block(pos, None);
    }

    /// O cano do bloco em `pos`, se o chunk está carregado e há um cano.
    pub fn pipe_at(&self, pos: BlockPos) -> Option<(TransmitterId, &Pipe)> {
        let id = *self.level.view().block_entity(pos)?;
        Some((id, &self.level.hooks.state.pipes[id.0 as usize]))
    }

    /// Os eventos da rede desde a última chamada.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.level.hooks.state.events)
    }
}
