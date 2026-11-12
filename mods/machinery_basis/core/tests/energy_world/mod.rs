//! O mundo simulado dos testes da topologia de rede: cabos que ligam em todo lado onde há outro cabo, e o `TileEntityTransmitter`
//! (`clearRemoved`, o primeiro tick, `onChunkUnloaded`, `setRemoved`, `chunkAccessibilityChange`) sobre os chunks falsos de
//! `sim::chunks`. Quem chama o registro é este arquivo, como o tile do Mekanism chama; o registro em si é o da lib.
#![allow(dead_code)]
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, ChunkPos, LevelView, SimLevel};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::energy_network::EnergyFamily;
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::rc::Rc;

/// O bloco de uma máquina que aceita energia em todos os lados.
pub const MACHINE: &str = "test:machine";
/// O prefixo do bloco do cabo: `test:cable_<tier>`.
pub const CABLE_PREFIX: &str = "test:cable_";
pub const DIMENSION: DimensionId = DimensionId(0);

/// O aceitador de energia que a máquina dá ao cabo (`EnergyHandler`).
pub type Acceptor = Rc<dyn EnergyInsert>;

/// Uma máquina (Enrichment Chamber ou cubo de energia) com todos os lados em `INPUT`: o contêiner dela, sem limite de taxa.
pub struct Machine {
    pub container: BufferEnergyContainer,
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }
}

/// A capacidade de cada tier de cabo (`CableTier.getCableCapacity`, o campo `cap` do rastro).
pub fn tier_capacity(tier: &str) -> i64 {
    match tier {
        "basic" => 8_000,
        "advanced" => 128_000,
        "elite" => 1_024_000,
        "ultimate" => 8_192_000,
        other => panic!("tier desconhecido: {other}"),
    }
}

/// O `UniversalCable` e o `TileEntityTransmitter` dele, no que a rede lê.
#[derive(Debug)]
pub struct Tx {
    pub pos: BlockPos,
    pub removed: bool,
    /// `TileEntityTransmitter.loaded`.
    pub loaded: bool,
    /// `markJoined`.
    pub mark_joined: bool,
    pub network: Option<NetworkId>,
    pub orphan: bool,
    /// O `System.identityHashCode` do transmissor.
    pub hash: i32,
    pub shares_taken: u32,
}

/// O que a rede fez, na ordem, para o teste comparar com o rastro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Update(BlockPos),
    Commit(NetworkId),
    Invalidate(NetworkId),
    NetUpdate(NetworkId),
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
    Access(BlockPos, bool),
}

/// Uma chamada ao registro que o `TileEntityTransmitter` faria, para os testes que a fazem sem passar pelos chunks.
#[derive(Clone, Copy, Debug)]
pub enum RegistryCall {
    InvalidateTransmitter(BlockPos),
    RegisterOrphanTransmitter(BlockPos),
}

pub struct State {
    pub txs: Vec<Tx>,
    /// O `identityHashCode` do próximo transmissor posto em cada posição.
    pub hash_for: Box<dyn FnMut(BlockPos) -> i32>,
    pub events: Vec<Event>,
    pub runs_normally: bool,
    /// A energia dos cabos e das redes.
    pub family: EnergyFamily<dyn EnergyInsert>,
    /// As máquinas postas, por posição.
    pub machines: BTreeMap<BlockPos, Rc<Machine>>,
    /// Invalidações de capability a entregar no começo do próximo `onTick` (`RefreshListener.run` -> `markDirtyAcceptor`).
    pub acceptor_invalidations: Vec<(BlockPos, Face)>,
    /// Chamadas diretas ao registro, entregues no começo do próximo `onTick`, na ordem.
    pub registry_calls: Vec<RegistryCall>,
    pending_uuid: Option<Uuid>,
}

pub struct Hooks {
    pub registry: TransmitterNetworkRegistry<Acceptor>,
    pub state: State,
}

/// O que o registro enxerga num instante: os transmissores e os chunks do nível.
struct Ctx<'a> {
    st: &'a mut State,
    level: &'a LevelView<TransmitterId>,
}

impl Ctx<'_> {
    fn tx(&self, id: TransmitterId) -> &Tx {
        &self.st.txs[id.0 as usize]
    }

    fn tx_mut(&mut self, id: TransmitterId) -> &mut Tx {
        &mut self.st.txs[id.0 as usize]
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        id: TransmitterId,
        was_present: bool,
    ) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.tx(id).loaded {
            self.tx_mut(id).loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        id: TransmitterId,
        still_present: bool,
    ) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.tx(id).loaded {
            self.tx_mut(id).loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

impl TransmitterHost<Acceptor> for Ctx<'_> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.tx(transmitter).pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        DIMENSION
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        let tx = self.tx(transmitter);
        !tx.removed && tx.loaded
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.tx(transmitter).orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        self.tx_mut(transmitter).orphan = orphan;
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.tx(transmitter).network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        self.tx_mut(transmitter).network = network;
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.tx(transmitter).hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        let pos = self.tx(transmitter).pos;
        self.st.events.push(Event::Update(pos));
    }

    fn supports_transmission_type(&self, _: TransmitterId, _: TransmitterId) -> bool {
        true
    }

    fn is_valid_transmitter_basic(&self, _: TransmitterId, _: TransmitterId, _: Face) -> bool {
        true
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.level.block_entity(pos).copied()
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        let pos = self.tx(transmitter).pos;
        self.st.events.push(Event::Access(pos, loaded));
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            self.tx_mut(transmitter).shares_taken += 1;
            let network = self.tx(transmitter).network;
            self.st
                .family
                .validate_and_take_share(registry, transmitter, network, None);
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, _: TransmitterId, _: Face) {}

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Acceptor> {
        let pos = self.tx(transmitter).pos.relative(side);
        if self.level.block_if_loaded(pos) == Some(MACHINE) {
            let machine: Acceptor = self.st.machines.get(&pos)?.clone();
            Some(machine)
        } else {
            None
        }
    }

    fn take_share(&mut self, transmitter: TransmitterId) {
        let network = self.tx(transmitter).network;
        self.st.family.take_share_in_root(transmitter, network);
    }

    fn on_update(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        network: NetworkId,
    ) {
        self.st.events.push(Event::NetUpdate(network));
        self.st.family.on_update(registry, network);
    }

    fn network_event(
        &mut self,
        registry: &TransmitterNetworkRegistry<Acceptor>,
        network: NetworkId,
        event: NetworkEvent,
    ) {
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
            NetworkEvent::Committing => self.st.events.push(Event::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.events.push(Event::Invalidate(network)),
            _ => {}
        }
    }
}

impl ChunkHooks for Hooks {
    type BlockEntity = TransmitterId;
    type Saved = i64;

    fn new_block_entity(
        &mut self,
        pos: BlockPos,
        block: &str,
        saved: Option<&i64>,
    ) -> Option<TransmitterId> {
        let tier = block.strip_prefix(CABLE_PREFIX)?;
        let hash = (self.state.hash_for)(pos);
        self.state.txs.push(Tx {
            pos,
            removed: false,
            loaded: false,
            mark_joined: false,
            network: None,
            orphan: true,
            hash,
            shares_taken: 0,
        });
        let id = TransmitterId(self.state.txs.len() as u32 - 1);
        self.state.family.add_cable(id, pos, tier_capacity(tier));
        if let Some(energy) = saved {
            // `UniversalCable.read`: `saveShare = ENERGY` e `buffer.setEnergy(saveShare, null)`.
            self.state.family.cable(id).read_energy(*energy);
        }
        Some(id)
    }

    /// `UniversalCable.write`: valida as shares e grava o `saveShare`.
    fn save_block_entity(&mut self, _: BlockPos, entity: &mut TransmitterId) -> i64 {
        let network = self.state.txs[entity.0 as usize].network;
        self.state
            .family
            .write_energy(&self.registry, *entity, network)
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block.starts_with(CABLE_PREFIX)
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

    /// `TileEntityTransmitter.clearRemoved`, `:146`, no servidor.
    fn block_entity_added(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let tx = &mut self.state.txs[entity.0 as usize];
        tx.removed = false;
        tx.mark_joined = true;
    }

    /// `TileEntityTransmitter.onUpdateServer`, `:93`.
    fn block_entity_tick(
        &mut self,
        level: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        if self.state.txs[entity.0 as usize].mark_joined {
            let mut ctx = Ctx {
                st: &mut self.state,
                level,
            };
            ctx.world_join(&mut self.registry, *entity, false);
            ctx.tx_mut(*entity).mark_joined = false;
        }
    }

    /// `TileEntityTransmitter.onChunkUnloaded`, `:156`: `validateAndTakeShare`.
    fn block_entity_chunk_unloaded(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        self.state.txs[entity.0 as usize].shares_taken += 1;
        let network = self.state.txs[entity.0 as usize].network;
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
        self.state.txs[entity.0 as usize].removed = true;
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        ctx.world_separate(&mut self.registry, *entity, false);
    }

    /// `Level.setBlock` confirmou um estado novo: o bloco avisa os vizinhos (`neighborChanged`, na ordem de `NeighborUpdater.UPDATE_ORDER`), e
    /// o cabo vizinho que está numa rede refaz a conexão daquele lado (`TileEntityTransmitter.onNeighborBlockChange(side)`,
    /// `Transmitter.refreshConnections(side)`): se o bloco não é um aceitador válido, o lado fica marcado como mudado e a rede entra em
    /// `networksToChange` (`getPossibleAcceptorConnection(side, true)`, `Transmitter.java:319-321`).
    fn block_changed(
        &mut self,
        level: &LevelView<TransmitterId>,
        pos: BlockPos,
        _old: Option<&str>,
        new: Option<&str>,
    ) {
        use Face::{Down, East, North, South, Up, West};
        for face in [West, East, Down, Up, North, South] {
            let Some(&id) = level.block_entity(pos.relative(face)) else {
                continue;
            };
            let tx = &self.state.txs[id.0 as usize];
            let Some(network) = tx.network.filter(|_| !tx.orphan) else {
                continue;
            };
            let _ = new;
            let ctx = Ctx {
                st: &mut self.state,
                level,
            };
            self.registry
                .network_acceptor_changed(&ctx, network, id, face.opposite());
        }
    }

    /// `TransmitterNetworkRegistry.onTick`.
    fn server_tick_post(&mut self, level: &LevelView<TransmitterId>) {
        let runs_normally = self.state.runs_normally;
        for call in std::mem::take(&mut self.state.registry_calls) {
            let (RegistryCall::InvalidateTransmitter(pos)
            | RegistryCall::RegisterOrphanTransmitter(pos)) = call;
            if let Some(&id) = level.block_entity(pos) {
                let ctx = Ctx {
                    st: &mut self.state,
                    level,
                };
                match call {
                    RegistryCall::InvalidateTransmitter(_) => {
                        self.registry.invalidate_transmitter(&ctx, id);
                    }
                    RegistryCall::RegisterOrphanTransmitter(_) => {
                        self.registry.register_orphan_transmitter(&ctx, id);
                    }
                }
            }
        }
        for (pos, side) in std::mem::take(&mut self.state.acceptor_invalidations) {
            if let Some(&id) = level.block_entity(pos) {
                let tx = &self.state.txs[id.0 as usize];
                if let Some(network) = tx.network.filter(|_| !tx.orphan) {
                    let ctx = Ctx {
                        st: &mut self.state,
                        level,
                    };
                    self.registry
                        .network_acceptor_changed(&ctx, network, id, side);
                }
            }
        }
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        self.registry.on_tick(&mut ctx, runs_normally);
    }
}

/// O mundo de um cenário: o nível, e a origem da estrutura (a posição do cabo 0).
pub struct World {
    pub level: SimLevel<Hooks>,
    pub origin: BlockPos,
    /// A largura da estrutura: o cabo de índice `i` está em `x = i % width`, `z = i / width`.
    pub width: i32,
}

impl World {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `hash_for` o `identityHashCode` de cada cabo novo.
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
                txs: Vec::new(),
                hash_for: Box::new(hash_for),
                events: Vec::new(),
                runs_normally: true,
                family: EnergyFamily::new(),
                machines: BTreeMap::new(),
                acceptor_invalidations: Vec::new(),
                registry_calls: Vec::new(),
                pending_uuid: None,
            },
        };
        Self {
            level: SimLevel::new(hooks),
            origin,
            width,
        }
    }

    /// A posição do cabo de índice `index` da estrutura.
    pub fn cable(&self, index: i32) -> BlockPos {
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

    pub fn registry(&self) -> &TransmitterNetworkRegistry<Acceptor> {
        &self.level.hooks.registry
    }

    pub fn place_cable(&mut self, x: i32, tier: &str) {
        let pos = self.cable(x);
        let block = format!("{CABLE_PREFIX}{tier}");
        assert!(self.level.set_block(pos, Some(&block)), "cabo {x}");
    }

    /// Uma máquina em `pos` (`y` é o `origin.y` mais o degrau), com a capacidade e a energia dadas.
    pub fn place_machine(&mut self, pos: BlockPos, max: i64, stored: i64) -> Rc<Machine> {
        let container = BufferEnergyContainer::new(max, always, always, None);
        container.set_energy(stored, None);
        let machine = Rc::new(Machine { container });
        self.level.hooks.state.machines.insert(pos, machine.clone());
        assert!(self.level.set_block(pos, Some(MACHINE)), "máquina {pos:?}");
        machine
    }

    /// Um tick do nível: o relógio do jogo (`getGameTime`) anda antes de tudo o que roda nele.
    pub fn tick_level(&mut self) {
        self.level.hooks.state.family.game_time += 1;
        self.level.tick_level();
    }

    pub fn break_block(&mut self, pos: BlockPos) {
        self.level.set_block(pos, None);
    }

    /// O transmissor do bloco em `pos`, se o chunk está carregado e há um cabo.
    pub fn tx_at(&self, pos: BlockPos) -> Option<(TransmitterId, &Tx)> {
        let id = *self.level.view().block_entity(pos)?;
        Some((id, &self.level.hooks.state.txs[id.0 as usize]))
    }

    /// Os eventos da rede desde a última chamada.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.level.hooks.state.events)
    }
}
