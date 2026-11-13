//! O dono dos transmissores de energia: liga o `Transmitter` (conexões por lado, redstone, Configurator), o registro de redes e a família de
//! energia ao que o jogo entrega (`TileEntityTransmitter`, `BlockTransmitter.neighborChanged`, o `RefreshListener` do `AcceptorCache`, o
//! `ServerTickEvent.Post`). É a cola que no original está espalhada em `TileEntityTransmitter` e `TileEntityUniversalCable`; a lib não conhece
//! nenhum id: o jogo (ou o mundo simulado de um teste) entra por [`OwnerWorld`].
//!
//! O dono guarda por dimensão o registro, os `Transmitter` e a [`EnergyFamily`]. Cada [`Event`] é um dos eventos do jogo da decisão 107, e o
//! que o `Transmitter` pede ao mundo sai como [`Effect`] na ordem do original (decisão 104), aplicado aqui:
//!
//! - `SendUpdatePacket` vira [`OwnerWorld::publish`] com a vista do cabo (as conexões por lado, o que o modelo e a capability usam);
//! - `NotifyTileChange` faz `onNeighborBlockChange(null)` nos transmissores vizinhos, na hora;
//! - `RefreshNeighbor` é a chamada de verdade a `refreshConnections(side)` do vizinho;
//! - `InvalidateTransmitter` e `AcceptorChanged` vão ao registro.
//!
//! Do cabo (`TileEntityUniversalCable`): `onUpdateServer` puxa dos aceitadores em `PULL` antes de tudo (`pullFromAcceptors`), e a capability
//! de energia é o contêiner da rede (`getContainer`), que só existe com rede; sem rede o cabo não aceita nada de fora (o buffer próprio é
//! `notExternal`).
use super::alloy::{AlloyRun, CableUpgrade, can_upgrade, order_by_distance};
use super::client::{Outgoing, ScaleOutbox};
use super::connection::{
    ConnectionType, TransmissionSet, TransmissionType, connection_map_contains_side,
};
use super::energy_container::BufferEnergyContainer;
use super::energy_network::EnergyFamily;
use super::registry::{TransmitterNetworkRegistry, set_transmitter_network};
use super::transmitter::{
    Compound, Effect, NeighborTransmitter, Overlay, Tag, Transmitter, TransmitterWorld,
};
use super::universal_cable::{UniversalCable, UniversalCableUpgradeData};
use super::{DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid};
use crate::containers::AutomationType;
use crate::distribution::EnergyInsert;
use crate::sides::Face;
use crate::sim::chunks::{BlockPos, ChunkPos, MAX_VIEW_DISTANCE};
use crate::transaction::Transaction;
use std::collections::HashMap;
use std::rc::Rc;

/// O aceitador de energia de um bloco vizinho (`EnergyHandler`).
pub type Acceptor = Rc<dyn EnergyInsert>;

/// A chave de NBT da energia guardada do cabo (`SerializationConstants.ENERGY`).
pub const ENERGY_KEY: &str = "energy";

/// Uma capability de energia que apareceu ou mudou de lado (`BlockEntity.invalidateCapabilities`, que o cabo vizinho ouve pelo `RefreshListener`): a máquina entrou (`old` é `None`:
/// o Horizon só dá as faces da instância no primeiro tick dela, e o cabo que a olhou antes a achou sem capability), trocou de bloco, ou mudou as faces que deixam entrar ou sair
/// (a configuração de lados, a frente do bloco). `old` e `new` são o id do bloco, as faces de entrada e as de saída; a capacidade sozinha não é capability.
pub fn capability_changed(
    old: Option<(&str, [bool; 6], [bool; 6])>,
    new: (&str, [bool; 6], [bool; 6]),
) -> bool {
    match old {
        None => true,
        Some(old) => old.0 != new.0 || old.1 != new.1 || old.2 != new.2,
    }
}

fn arr(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

fn bp(pos: [i32; 3]) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

/// O que o dono recebe do jogo (a decisão 107).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `TileEntityTransmitter.clearRemoved`: o cabo entrou no chunk, posto ou lido do save.
    ClearRemoved(BlockPos),
    /// `TileEntityTransmitter.onAdded`: o bloco posto por um item.
    PlacedByItem(BlockPos),
    /// `TileEntityTransmitter.onChunkUnloaded`.
    ChunkUnloaded(BlockPos),
    /// `TileEntityTransmitter.setRemoved`.
    Removed(BlockPos),
    /// `TileEntityTransmitter.onUpdateServer`.
    Update(BlockPos),
    /// `BlockTransmitter.neighborChanged`: `onNeighborBlockChange(null)`.
    Neighbor(BlockPos),
    /// O `RefreshListener` do `AcceptorCache`: a capability do vizinho no lado dado foi invalidada.
    Capability(BlockPos, Face),
    /// `ChunkTicketLevelUpdatedEvent`: o chunk foi de "descarregado" a carregado (`loaded`) ou o contrário.
    Ticket { chunk: ChunkPos, loaded: bool },
    /// `ServerTickEvent.Post`.
    ServerTickPost { runs_normally: bool },
    /// `ServerStoppedEvent`: o mundo fechou.
    Reset,
}

/// O que o jogo diz de um cabo novo.
pub struct CableInfo {
    /// `CableTier.getCableCapacity`.
    pub capacity: i64,
    /// O que o block entity guardou (`TileEntityTransmitter.loadAdditional`), se veio de um save.
    pub saved: Option<Compound>,
}

/// O que o cabo mostra por lado: o modelo (`TransmitterModelData`), a capability e o Configurator leem daqui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CableView {
    /// `getAllCurrentConnections`: o lado tem um braço.
    pub connected: [bool; 6],
    /// `getConnectionType(side)`: o tipo que o lado mostra (`NONE` sem conexão).
    pub kind: [ConnectionType; 6],
    /// `getConnectionTypeRaw(side)`: o que o Configurator escolheu.
    pub raw: [ConnectionType; 6],
    /// A capability existe no lado (`getContainer(side)` não nulo).
    pub exists: [bool; 6],
    /// A capability deixa inserir pelo lado (`canInsert`) e há contêiner que aceite de fora (a rede).
    pub insert: [bool; 6],
    pub has_network: bool,
    pub redstone_reactive: bool,
    pub redstone_activated: bool,
}

/// A capability de energia de um cabo vista num instante, para quem empurra energia para ele.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CableBuffer {
    pub view: CableView,
    /// A capacidade e a energia do contêiner (o da rede, ou o do cabo sem rede).
    pub capacity: i64,
    pub amount: i64,
}

/// O mundo, como o dono o vê. O que `Transmitter` lê (`TransmitterWorld`) mais o que o dono manda de volta.
pub trait OwnerWorld {
    /// `WorldUtils.isBlockLoaded`.
    fn is_block_loaded(&self, pos: BlockPos) -> bool;
    /// `WorldUtils.isGettingPowered`.
    fn is_getting_powered(&self, pos: BlockPos) -> bool;
    /// A capability de energia do bloco em `pos` pelo lado `side` dele (`BlockCapabilityCache.getCapability`), `None` se não há (ou se o
    /// bloco é um cabo: o dono nunca pergunta por transmissores).
    fn energy_handler(&self, pos: BlockPos, side: Face) -> Option<Acceptor>;
    /// O cabo que o bloco em `pos` é, ou `None` se o bloco não é um dos nossos.
    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo>;
    /// `sendUpdatePacket`: as conexões do cabo em `pos` mudaram.
    fn publish(&mut self, pos: BlockPos, view: &CableView);
    /// `BlockEntity.setChanged`.
    fn set_changed(&mut self, _pos: BlockPos) {}
    /// `TileEntityTransmitter.sideChanged`.
    fn side_changed(
        &mut self,
        _pos: BlockPos,
        _side: Face,
        _old: ConnectionType,
        _new: ConnectionType,
    ) {
    }
    /// `TileEntityTransmitter.redstoneChanged`.
    fn redstone_changed(&mut self, _pos: BlockPos, _powered: bool) {}
    /// `player.sendOverlayMessage`.
    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {}
}

struct Slot {
    pos: BlockPos,
    /// `None` só enquanto o dono opera nele (o mundo vê os outros).
    tx: Option<Transmitter<Acceptor>>,
    /// `System.identityHashCode`.
    hash: i32,
    /// `TileEntityTransmitter.markJoined`.
    mark_joined: bool,
    /// A rede do registro (o `Transmitter` guarda só o UUID dela).
    network: Option<NetworkId>,
    /// O `Transmitter` saiu do mundo (`setRemoved`): o registro ainda o conhece até o próximo tick.
    dead: bool,
}

struct State {
    dimension: DimensionId,
    slots: Vec<Slot>,
    by_pos: HashMap<BlockPos, TransmitterId>,
    family: EnergyFamily<dyn EnergyInsert>,
    hashes: Box<dyn FnMut(BlockPos) -> i32>,
    uuids: HashMap<NetworkId, Uuid>,
    /// A transação raiz de `DynamicNetwork.invalidate` (`begin_take_shares` a `end_take_shares`): todos os `takeShare` do laço andam nela e ela confirma no fim.
    take_root: Option<Transaction>,
    /// Efeitos que um método do `TransmitterHost` pediu e que só o dono, com o registro, pode aplicar.
    deferred: Vec<(TransmitterId, Effect)>,
    /// O `packetRange` de cada rede e os `PacketNetworkScale` que o `onUpdate` mandou.
    outbox: ScaleOutbox<()>,
}

/// O que o `Transmitter` enxerga do mundo: os outros transmissores do dono e o jogo.
struct View<'a, W: OwnerWorld> {
    st: &'a State,
    world: &'a W,
}

impl<W: OwnerWorld> TransmitterWorld for View<'_, W> {
    type Acceptor = Acceptor;

    fn is_block_loaded(&self, pos: [i32; 3]) -> bool {
        self.world.is_block_loaded(bp(pos))
    }

    fn is_getting_powered(&self, pos: [i32; 3]) -> bool {
        self.world.is_getting_powered(bp(pos))
    }

    fn transmitter_at(&self, pos: [i32; 3], side: Face) -> Option<NeighborTransmitter> {
        let at = bp(pos);
        if !self.world.is_block_loaded(at) {
            return None;
        }
        let id = self.st.by_pos.get(&at)?;
        let other = self.st.slots[id.0 as usize].tx.as_ref()?;
        Some(NeighborTransmitter {
            supported: other.supported_transmission_types(),
            can_connect_opposite: other.can_connect(side.opposite(), self),
        })
    }

    fn acceptor(&self, pos: [i32; 3], from_side: Face) -> Option<Acceptor> {
        let at = bp(pos);
        if self.st.by_pos.contains_key(&at) {
            return None;
        }
        self.world.energy_handler(at, from_side)
    }
}

impl State {
    fn tx(&self, id: TransmitterId) -> &Transmitter<Acceptor> {
        self.slots[id.0 as usize]
            .tx
            .as_ref()
            .expect("o transmissor está no lugar")
    }

    fn live(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.by_pos.get(&pos).copied()
    }

    fn uuid_value(&self, network: NetworkId) -> u128 {
        let uuid = self.uuids.get(&network).copied().unwrap_or(Uuid {
            most_significant: 0,
            least_significant: i64::from(network.0),
        });
        (u128::from(uuid.most_significant as u64) << 64) | u128::from(uuid.least_significant as u64)
    }

    /// `getContainer()`.
    fn container(&self, id: TransmitterId) -> &BufferEnergyContainer {
        let slot = &self.slots[id.0 as usize];
        if let (Some(tx), Some(network)) = (slot.tx.as_ref(), slot.network)
            && tx.has_transmitter_network()
            && let Some(network) = self.family.network(network)
        {
            return &network.energy;
        }
        &self.family.cable(id).buffer
    }
}

/// O que o registro enxerga num instante: os transmissores e o jogo.
struct Ctx<'a, W: OwnerWorld> {
    st: &'a mut State,
    world: &'a mut W,
}

impl<W: OwnerWorld> Ctx<'_, W> {
    fn view(&self) -> View<'_, W> {
        View {
            st: &*self.st,
            world: &*self.world,
        }
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
        if !self.st.tx(id).loaded {
            self.st.slots[id.0 as usize]
                .tx
                .as_mut()
                .expect("no lugar")
                .loaded = true;
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
        if self.st.tx(id).loaded {
            self.st.slots[id.0 as usize]
                .tx
                .as_mut()
                .expect("no lugar")
                .loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

impl<W: OwnerWorld> TransmitterHost<Acceptor> for Ctx<'_, W> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.st.slots[transmitter.0 as usize].pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        self.st.dimension
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        self.st.slots[transmitter.0 as usize]
            .tx
            .as_ref()
            .is_some_and(Transmitter::is_valid)
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.st.tx(transmitter).is_orphan()
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        self.st.slots[transmitter.0 as usize]
            .tx
            .as_mut()
            .expect("no lugar")
            .set_orphan(orphan);
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.st.slots[transmitter.0 as usize].network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        let uuid = network.map(|network| self.st.uuid_value(network));
        let slot = &mut self.st.slots[transmitter.0 as usize];
        slot.network = network;
        // Só a atribuição: o `requestsUpdate` é do registro (`request_update`).
        let mut out = Vec::new();
        slot.tx
            .as_mut()
            .expect("no lugar")
            .set_transmitter_network(uuid, false, &mut out);
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.st.slots[transmitter.0 as usize].hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        // `BufferedTransmitter.requestsUpdate` do cabo de energia é a base: o pacote de atualização.
        self.st
            .deferred
            .push((transmitter, Effect::SendUpdatePacket));
    }

    fn supports_transmission_type(&self, transmitter: TransmitterId, other: TransmitterId) -> bool {
        self.st
            .tx(transmitter)
            .supports_transmission_type(self.st.tx(other).supported_transmission_types())
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        let view = self.view();
        let me = self.st.tx(transmitter);
        let other = self.st.tx(other);
        me.is_valid_transmitter_basic(
            &NeighborTransmitter {
                supported: other.supported_transmission_types(),
                can_connect_opposite: other.can_connect(side.opposite(), &view),
            },
            side,
            &view,
        )
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        if !self.world.is_block_loaded(pos) {
            return None;
        }
        self.st.live(pos)
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            let network = self.st.slots[transmitter.0 as usize].network;
            self.st
                .family
                .validate_and_take_share(registry, transmitter, network, None);
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face) {
        let mut tx = self.st.slots[transmitter.0 as usize]
            .tx
            .take()
            .expect("no lugar");
        let mut out = Vec::new();
        tx.refresh_acceptor_connections(
            side,
            &View {
                st: &*self.st,
                world: &*self.world,
            },
            &mut out,
        );
        self.st.slots[transmitter.0 as usize].tx = Some(tx);
        self.st
            .deferred
            .extend(out.into_iter().map(|effect| (transmitter, effect)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Acceptor> {
        let tx = self.st.tx(transmitter);
        if tx.can_connect_to_acceptor(side) {
            tx.get_acceptor(side, &self.view())
        } else {
            None
        }
    }

    fn begin_take_shares(&mut self) {
        self.st.take_root = Some(Transaction::open_root());
    }

    fn take_share(&mut self, transmitter: TransmitterId) {
        let network = self.st.slots[transmitter.0 as usize].network;
        match self.st.take_root.take() {
            // O laço de `invalidate` abre uma raiz só, que `end_take_shares` confirma.
            Some(root) => {
                self.st.family.take_share(transmitter, network, Some(&root));
                self.st.take_root = Some(root);
            }
            None => self.st.family.take_share_in_root(transmitter, network),
        }
    }

    fn end_take_shares(&mut self) {
        if let Some(root) = self.st.take_root.take() {
            root.commit();
        }
    }

    fn on_update(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<Acceptor>,
        network: NetworkId,
    ) {
        self.st.family.on_update(registry, network);
        // `Mekanism.onEnergyTransferred`: o `PacketNetworkScale` sai no ponto do evento, com o retângulo da rede.
        for (event_network, scale) in self.st.family.take_transfer_events() {
            let uuid = self.st.uuid_value(event_network);
            let positions: Vec<BlockPos> = registry
                .network(event_network)
                .transmitters()
                .into_iter()
                .map(|id| self.st.slots[id.0 as usize].pos)
                .collect();
            self.st
                .outbox
                .send(event_network, uuid, scale, positions, None);
        }
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
        if matches!(
            event,
            NetworkEvent::ValidTransmittersAdded | NetworkEvent::Deregistered
        ) {
            self.st.outbox.flush_range(network);
        }
        // O UUID existe desde a construção da rede (`this(UUID.randomUUID())`), antes de ela adotar os transmissores: o que cada um guarda dela (`setTransmitterNetwork`) já é este.
        if let NetworkEvent::Created { uuid } = event {
            self.st.uuids.insert(network, uuid);
        }
    }
}

/// Os transmissores de energia de uma dimensão.
pub struct EnergyTransmitters {
    registry: TransmitterNetworkRegistry<Acceptor>,
    st: State,
}

impl EnergyTransmitters {
    /// `uuids` dá o UUID de cada rede nova (`UUID.randomUUID`) e `hashes` o `identityHashCode` de cada cabo novo (decisão 101).
    pub fn new(
        dimension: DimensionId,
        uuids: impl FnMut() -> Uuid + 'static,
        hashes: impl FnMut(BlockPos) -> i32 + 'static,
    ) -> Self {
        Self {
            registry: TransmitterNetworkRegistry::new(uuids),
            st: State {
                dimension,
                slots: Vec::new(),
                by_pos: HashMap::new(),
                family: EnergyFamily::new(),
                hashes: Box::new(hashes),
                uuids: HashMap::new(),
                take_root: None,
                deferred: Vec::new(),
                outbox: ScaleOutbox::new(dimension),
            },
        }
    }

    pub fn registry(&self) -> &TransmitterNetworkRegistry<Acceptor> {
        &self.registry
    }

    pub fn family(&self) -> &EnergyFamily<dyn EnergyInsert> {
        &self.st.family
    }

    pub fn family_mut(&mut self) -> &mut EnergyFamily<dyn EnergyInsert> {
        &mut self.st.family
    }

    /// `world.getGameTime()`, antes de cada `ServerTickPost`.
    pub fn set_game_time(&mut self, time: i64) {
        self.st.family.game_time = time;
    }

    /// O cabo vivo em `pos`.
    pub fn cable_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.st.live(pos)
    }

    /// A rede do registro do cabo em `pos`.
    pub fn network_of(&self, pos: BlockPos) -> Option<NetworkId> {
        let id = self.st.live(pos)?;
        self.st.slots[id.0 as usize].network
    }

    pub fn transmitter(&self, pos: BlockPos) -> Option<&Transmitter<Acceptor>> {
        let id = self.st.live(pos)?;
        self.st.slots[id.0 as usize].tx.as_ref()
    }

    /// Os `PacketNetworkScale` que as redes mandaram desde a última chamada, com o retângulo de cada uma.
    pub fn take_scale_packets(&mut self) -> Vec<Outgoing<()>> {
        self.st.outbox.take()
    }

    /// As posições dos cabos vivos.
    pub fn positions(&self) -> Vec<BlockPos> {
        let mut list: Vec<BlockPos> = self.st.by_pos.keys().copied().collect();
        list.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        list
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Os eventos

    pub fn handle<W: OwnerWorld>(&mut self, world: &mut W, event: Event) {
        match event {
            Event::ClearRemoved(pos) => self.clear_removed(world, pos),
            Event::PlacedByItem(pos) => {
                if let Some(id) = self.st.live(pos) {
                    // `onAdded`: `onWorldJoin(level, false)` e `refreshConnections()`.
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.world_join(&mut self.registry, id, false);
                    self.with(world, id, |tx, view, out| tx.refresh_connections(view, out));
                }
            }
            Event::ChunkUnloaded(pos) => {
                if let Some(id) = self.st.live(pos) {
                    let network = self.st.slots[id.0 as usize].network;
                    self.st
                        .family
                        .validate_and_take_share(&self.registry, id, network, None);
                }
            }
            Event::Removed(pos) => self.removed(world, pos),
            Event::Update(pos) => self.update(world, pos),
            Event::Neighbor(pos) => {
                if let Some(id) = self.st.live(pos) {
                    self.with(world, id, |tx, view, out| {
                        tx.on_neighbor_block_change(None, view, out)
                    });
                }
            }
            Event::Capability(pos, side) => {
                if let Some(id) = self.st.live(pos) {
                    self.with(world, id, |tx, _, out| {
                        if let Some(dirty) =
                            tx.acceptor_cache.refresh_listener_run(side, tx.is_valid())
                        {
                            tx.mark_dirty_acceptor(dirty, out);
                        }
                    });
                }
            }
            Event::Ticket { chunk, loaded } => {
                let (old, new) = if loaded {
                    (MAX_VIEW_DISTANCE + 1, MAX_VIEW_DISTANCE)
                } else {
                    (MAX_VIEW_DISTANCE, MAX_VIEW_DISTANCE + 1)
                };
                self.registry
                    .on_ticket_level_change(self.st.dimension, chunk.pack(), old, new);
            }
            Event::ServerTickPost { runs_normally } => {
                let mut ctx = Ctx {
                    st: &mut self.st,
                    world,
                };
                self.registry.on_tick(&mut ctx, runs_normally);
            }
            Event::Reset => {
                self.registry.reset();
                let dimension = self.st.dimension;
                let hashes = std::mem::replace(&mut self.st.hashes, Box::new(|_| 0));
                self.st = State {
                    dimension,
                    slots: Vec::new(),
                    by_pos: HashMap::new(),
                    family: EnergyFamily::new(),
                    hashes,
                    uuids: HashMap::new(),
                    take_root: None,
                    deferred: Vec::new(),
                    outbox: ScaleOutbox::new(dimension),
                };
            }
        }
        self.flush_deferred(world);
    }

    fn clear_removed<W: OwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        if let Some(id) = self.st.live(pos) {
            // O mesmo block entity entrando de novo: só marca para entrar.
            self.st.slots[id.0 as usize].mark_joined = true;
            return;
        }
        let Some(info) = world.cable_info(pos) else {
            return;
        };
        let hash = (self.st.hashes)(pos);
        let id = TransmitterId(self.st.slots.len() as u32);
        let mut tx = Transmitter::new(
            arr(pos),
            TransmissionSet::of(&[TransmissionType::Energy]),
            true,
        );
        self.st.family.add_cable(id, pos, info.capacity);
        if let Some(saved) = &info.saved {
            // `loadAdditional`, antes de `setLevel`.
            let mut ignored = Vec::new();
            tx.read(saved, &mut ignored);
            if let Some(energy) = saved.get(ENERGY_KEY).and_then(|tag| match tag {
                Tag::Long(value) => Some(*value),
                other => other.int_value().map(i64::from),
            }) {
                self.st.family.cable(id).read_energy(energy);
            }
        }
        tx.set_level(false);
        self.st.slots.push(Slot {
            pos,
            tx: Some(tx),
            hash,
            mark_joined: true,
            network: None,
            dead: false,
        });
        self.st.by_pos.insert(pos, id);
    }

    fn removed<W: OwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        {
            let slot = &mut self.st.slots[id.0 as usize];
            slot.tx.as_mut().expect("no lugar").removed = true;
            slot.dead = true;
        }
        // `setRemoved`: `onWorldSeparate(level, false)` e `remove()`.
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        ctx.world_separate(&mut self.registry, id, false);
        self.st.by_pos.remove(&pos);
    }

    fn update<W: OwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        self.pull_from_acceptors(world, id);
        if self.st.slots[id.0 as usize].mark_joined {
            let mut ctx = Ctx {
                st: &mut self.st,
                world,
            };
            ctx.world_join(&mut self.registry, id, false);
            self.st.slots[id.0 as usize].mark_joined = false;
        }
        self.with(world, id, |tx, view, out| tx.on_update_server(view, out));
    }

    /// `UniversalCable.pullFromAcceptors`.
    fn pull_from_acceptors<W: OwnerWorld>(&mut self, world: &mut W, id: TransmitterId) {
        let tx = self.st.tx(id);
        if !tx.has_pull_side() {
            return;
        }
        let pos = self.st.slots[id.0 as usize].pos;
        let container = self.st.container(id);
        if UniversalCable::available_pull(container) <= 0 {
            return;
        }
        for side in Face::ALL {
            if !tx.is_connection_type(side, ConnectionType::Pull) {
                continue;
            }
            let acceptor = tx.acceptor_cache.get_connected_acceptor(side, &|s| {
                world.energy_handler(pos.relative(s), s.opposite())
            });
            let Some(acceptor) = acceptor else {
                continue;
            };
            let available = UniversalCable::available_pull(self.st.container(id));
            let container = self.st.container(id);
            let extracted = acceptor.extract(available, true);
            if extracted == 0 {
                continue;
            }
            let transaction = Transaction::open_root();
            if container.insert(extracted, &transaction, AutomationType::Internal) == extracted {
                // O vizinho não desfaz: a simulação diz o que sai, o buffer confirma, e só então a retirada de verdade.
                transaction.commit();
                acceptor.extract(extracted, false);
                if container.amount() >= container.capacity() {
                    break;
                }
            }
        }
    }

    /// Roda `op` no transmissor `id`, tirado do lugar para o mundo ver os outros, e aplica os efeitos.
    fn with<W: OwnerWorld, R>(
        &mut self,
        world: &mut W,
        id: TransmitterId,
        op: impl FnOnce(&mut Transmitter<Acceptor>, &View<'_, W>, &mut Vec<Effect>) -> R,
    ) -> Option<R> {
        let mut tx = self.st.slots[id.0 as usize].tx.take()?;
        let mut out = Vec::new();
        let result = op(
            &mut tx,
            &View {
                st: &self.st,
                world: &*world,
            },
            &mut out,
        );
        self.st.slots[id.0 as usize].tx = Some(tx);
        self.apply(world, id, out);
        Some(result)
    }

    fn flush_deferred<W: OwnerWorld>(&mut self, world: &mut W) {
        while !self.st.deferred.is_empty() {
            for (id, effect) in std::mem::take(&mut self.st.deferred) {
                self.apply(world, id, vec![effect]);
            }
        }
    }

    fn apply<W: OwnerWorld>(&mut self, world: &mut W, id: TransmitterId, effects: Vec<Effect>) {
        let pos = self.st.slots[id.0 as usize].pos;
        for effect in effects {
            match effect {
                Effect::SendUpdatePacket | Effect::RequestsUpdate => self.publish(world, id),
                Effect::NotifyTileChange => {
                    for side in Face::ALL {
                        let neighbor = pos.relative(side);
                        if !world.is_block_loaded(neighbor) {
                            continue;
                        }
                        if let Some(other) = self.st.live(neighbor) {
                            self.with(world, other, |tx, view, out| {
                                tx.on_neighbor_block_change(None, view, out)
                            });
                        }
                    }
                }
                Effect::InvalidateTransmitter => {
                    let ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    self.registry.invalidate_transmitter(&ctx, id);
                }
                Effect::AcceptorChanged(side) => {
                    if let Some(network) = self.st.slots[id.0 as usize].network {
                        let ctx = Ctx {
                            st: &mut self.st,
                            world,
                        };
                        self.registry
                            .network_acceptor_changed(&ctx, network, id, side);
                    }
                }
                Effect::RefreshNeighbor { pos: other, side } => {
                    if let Some(other) = self.st.live(bp(other)) {
                        self.with(world, other, |tx, view, out| {
                            tx.refresh_connections_side(side, view, out)
                        });
                    }
                }
                Effect::RecheckConnections(mask) => {
                    self.with(world, id, |tx, view, out| {
                        tx.base_recheck_connections(mask, view, out)
                    });
                }
                Effect::RecheckConnection(_)
                | Effect::ClientNetworkRemove(_)
                | Effect::ClientNetworkAdd(_) => {}
                Effect::SetForceUpdate => {
                    if let Some(tx) = self.st.slots[id.0 as usize].tx.as_mut() {
                        tx.set_force_update();
                    }
                }
                Effect::SetChanged => world.set_changed(pos),
                Effect::SideChanged { side, old, new } => {
                    world.side_changed(pos, side, old, new);
                    self.publish(world, id);
                }
                Effect::RedstoneChanged(powered) => {
                    world.redstone_changed(pos, powered);
                    self.publish(world, id);
                }
                Effect::Overlay(overlay) => world.overlay(pos, overlay),
            }
        }
    }

    /// `sendUpdatePacket`: o jogo recebe a vista do cabo. Um cabo que saiu, ou que está tirado do lugar por uma operação em andamento, não publica.
    fn publish<W: OwnerWorld>(&self, world: &mut W, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        if slot.tx.is_some() && !slot.dead {
            let view = self.cable_view(&*world, id);
            world.publish(slot.pos, &view);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // O que o jogo pergunta

    /// A vista do cabo em `pos`.
    pub fn view_at<W: OwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<CableView> {
        let id = self.st.live(pos)?;
        Some(self.cable_view(world, id))
    }

    fn cable_view<W: OwnerWorld>(&self, world: &W, id: TransmitterId) -> CableView {
        let tx = self.st.tx(id);
        let all = tx.get_all_current_connections();
        let activated = tx.is_redstone_activated(&View {
            st: &self.st,
            world,
        });
        let has_network = tx.has_transmitter_network();
        let mut view = CableView {
            connected: [false; 6],
            kind: [ConnectionType::None; 6],
            raw: [ConnectionType::None; 6],
            exists: [false; 6],
            insert: [false; 6],
            has_network,
            redstone_reactive: tx.redstone_reactive,
            redstone_activated: activated,
        };
        for side in Face::ALL {
            let index = side as usize;
            view.connected[index] = connection_map_contains_side(all, side);
            view.kind[index] = tx.get_connection_type(side);
            view.raw[index] = tx.get_connection_type_raw(side);
            // `getContainer(direction)`: nulo se o lado é `NONE` cru ou o cabo está ativado por redstone.
            view.exists[index] = !(view.raw[index] == ConnectionType::None || activated);
            // `canInsert(direction)` e o contêiner que aceita de fora: o da rede (o buffer do cabo é `notExternal`).
            view.insert[index] = view.exists[index] && has_network && tx.can_insert(Some(side));
        }
        view
    }

    /// A capability de energia do cabo em `pos`, para quem empurra energia para ele.
    pub fn buffer_at<W: OwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<CableBuffer> {
        let id = self.st.live(pos)?;
        let container = self.st.container(id);
        Some(CableBuffer {
            view: self.cable_view(world, id),
            capacity: container.capacity(),
            amount: container.amount(),
        })
    }

    /// A capacidade e a energia do contêiner do cabo em `pos` (`getContainer()`).
    pub fn container_state(&self, pos: BlockPos) -> Option<(i64, i64)> {
        let id = self.st.live(pos)?;
        let container = self.st.container(id);
        Some((container.capacity(), container.amount()))
    }

    /// `getContainer().insert(amount, transaction, EXTERNAL)`: o que o vizinho empurrou para o cabo.
    pub fn insert_external(&mut self, pos: BlockPos, amount: i32) -> i32 {
        let transaction = Transaction::open_root();
        let accepted = self.insert_external_in(pos, amount, &transaction);
        transaction.commit();
        accepted
    }

    /// O mesmo `insert`, dentro da transação de quem empurra (o `EnergyUtils.emit` de um gerador abre a dele, e só pode haver uma raiz).
    pub fn insert_external_in(
        &mut self,
        pos: BlockPos,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        let Some(id) = self.st.live(pos) else {
            return 0;
        };
        self.st
            .container(id)
            .insert(amount, transaction, AutomationType::External)
    }

    /// O que o cabo guarda no block entity (`TileEntityTransmitter.saveAdditional`, `UniversalCable.write`): a conexão por lado e a parte
    /// da energia que cabe a este cabo. Valida as shares da rede, como o original.
    pub fn write(&mut self, pos: BlockPos) -> Option<Compound> {
        let id = self.st.live(pos)?;
        let network = self.st.slots[id.0 as usize].network;
        let share = self.st.family.write_energy(&self.registry, id, network);
        let mut tag = self.st.tx(id).write();
        tag.insert(ENERGY_KEY.to_owned(), Tag::Long(share));
        Some(tag)
    }

    /// `TileEntityTransmitter.onAlloyInteraction`, até a lista: o cabo clicado em `pos` tem rede, e os da rede que a liga de nível `alloy_level` sobe (`canUpgrade`) vêm do mais
    /// perto para o mais longe. `tier_of` dá o nível do tier do bloco em cada posição.
    pub fn alloy_start(
        &self,
        pos: BlockPos,
        alloy_level: i32,
        tier_of: &dyn Fn(BlockPos) -> Option<i32>,
    ) -> Option<AlloyRun> {
        let id = self.st.live(pos)?;
        let slot = &self.st.slots[id.0 as usize];
        if !slot.tx.as_ref()?.has_transmitter_network() {
            return None;
        }
        let network = slot.network?;
        let members: Vec<BlockPos> = self
            .registry
            .network(network)
            .transmitters()
            .into_iter()
            .map(|member| &self.st.slots[member.0 as usize])
            .filter(|member| !member.dead)
            .map(|member| member.pos)
            .filter(|at| tier_of(*at).is_some_and(|level| can_upgrade(alloy_level, level)))
            .collect();
        Some(AlloyRun {
            network,
            targets: order_by_distance(pos, members),
            shares_set: false,
            upgraded: 0,
        })
    }

    /// O começo da troca de um cabo: na primeira, as shares da rede se validam (`validateSaveShares`, numa transação própria); depois `startUpgrading`
    /// (`isUpgrading`, `takeShare(null)`, `setTransmitterNetwork(null)`) e o dado de upgrade (`getUpgradeData`), antes de o bloco mudar.
    pub fn alloy_take<W: OwnerWorld>(
        &mut self,
        world: &mut W,
        run: &mut AlloyRun,
        pos: BlockPos,
    ) -> Option<CableUpgrade> {
        let id = self.st.live(pos)?;
        if !run.shares_set {
            let root = Transaction::open_root();
            self.st
                .family
                .validate_save_shares(&self.registry, run.network, id, Some(&root));
            root.commit();
            run.shares_set = true;
        }
        let network = {
            let slot = &self.st.slots[id.0 as usize];
            slot.network.filter(|_| slot.tx.as_ref().is_some_and(Transmitter::has_transmitter_network))
        };
        self.st.slots[id.0 as usize].tx.as_mut()?.set_upgrading(true);
        self.st.family.take_share(id, network, None);
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world,
            };
            set_transmitter_network(&mut ctx, id, None, true);
        }
        let buffer = self.st.family.cable(id).upgrade_data().buffer;
        let tx = self.st.slots[id.0 as usize].tx.as_ref()?;
        let data = CableUpgrade {
            redstone_reactive: tx.redstone_reactive,
            connection_types: *tx.connection_types_raw(),
            buffer,
        };
        self.flush_deferred(world);
        Some(data)
    }

    /// O fim da troca: o cabo novo em `pos` recebe o dado (`parseUpgradeData`). `false` se o bloco novo não tem cabo ("Error upgrading transmitter"): o laço segue sem contar.
    pub fn alloy_give(&mut self, run: &mut AlloyRun, pos: BlockPos, data: &CableUpgrade) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        if let Some(tx) = self.st.slots[id.0 as usize].tx.as_mut() {
            tx.redstone_reactive = data.redstone_reactive;
            tx.set_connection_types_raw(data.connection_types);
        }
        let root = Transaction::open_root();
        self.st.family.cable(id).parse_upgrade_data(
            &UniversalCableUpgradeData {
                buffer: data.buffer,
            },
            &root,
        );
        root.commit();
        run.upgraded += 1;
        true
    }

    /// `transmitterNetwork.invalidate(null)` ao fim do laço, se algum subiu.
    pub fn alloy_finish<W: OwnerWorld>(&mut self, world: &mut W, run: &AlloyRun) {
        if !run.did_upgrade() {
            return;
        }
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        self.registry.invalidate_network(&mut ctx, run.network, None);
        self.flush_deferred(world);
    }

    /// O pacote de atualização do cabo (`writeReducedUpdatedTag`) e o `getCapacity()` dele: o que o cliente recebe a cada `sendUpdatePacket`.
    pub fn client_update(&self, pos: BlockPos) -> Option<(Compound, i64)> {
        let id = self.st.live(pos)?;
        Some((
            self.st.tx(id).write_reduced_updated_tag(),
            self.st.family.cable(id).capacity(),
        ))
    }

    /// Os cliques do Configurator (`TileEntityTransmitter.onSneakRightClick`): `hit_side` é o lado do braço que o olhar toca. Devolve se o
    /// clique foi do cabo.
    pub fn sneak_right_click<W: OwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        hit_side: Option<Face>,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        let result = self.with(world, id, |tx, view, out| {
            tx.on_sneak_right_click(side, hit_side, |t, _| t.base_on_configure(), view, out)
        });
        self.flush_deferred(world);
        result.is_some_and(|result| result.consumes_action())
    }

    /// O lado do braço que o olhar toca (`getSideLookingAt`), pelo índice da caixa tocada.
    pub fn side_looking_at(&self, pos: BlockPos, box_index: Option<usize>) -> Option<Face> {
        self.transmitter(pos)?.side_looking_at(box_index)
    }

    /// `onRightClick`: liga e desliga o redstone reativo.
    pub fn right_click<W: OwnerWorld>(&mut self, world: &mut W, pos: BlockPos) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        let result = self.with(world, id, |tx, view, out| tx.on_right_click(view, out));
        self.flush_deferred(world);
        result.is_some_and(|result| result.consumes_action())
    }
}
