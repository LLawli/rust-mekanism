//! O dono dos transmissores com buffer de recurso (o cano de fluido, o tubo de químico), irmão de [`super::owner::EnergyTransmitters`]: liga o
//! `Transmitter` (conexões por lado, redstone, Configurator), o registro de redes e a [`ResourceFamily`] ao que o jogo entrega
//! (`TileEntityResourceTransmitter`, `BlockTransmitter.neighborChanged`, o `RefreshListener` do `AcceptorCache`, o `ServerTickEvent.Post`).
//! Os eventos são os mesmos do dono de energia ([`Event`], decisão 107) e a lib continua sem conhecer id: o jogo (ou o mundo simulado de um
//! teste) entra por [`ResourceOwnerWorld`].
//!
//! O que difere do cabo de energia, e que vem do Java:
//!
//! - `BufferedTransmitter.requestsUpdate` relê as conexões e pode fundir redes de dentro do `commit` (`buffered_transmitter`); a fusão manual
//!   de `recheckConnectionPrechecked` roda aqui, na ordem do original (decisão 125);
//! - `TileEntityResourceTransmitter.onUpdateServer` puxa dos aceitadores em `PULL` antes de tudo (`pullFromAcceptors`, na família);
//! - `DynamicNetwork.invalidate` abre uma transação raiz só em volta dos `takeShare` (`begin_take_shares` e `end_take_shares`);
//! - a capability é o contêiner da rede (`getContainer()`), e o buffer do cano, `notExternal`, recusa quem vem de fora sem rede;
//! - o NBT leva o `Transmitter` e o `saveShare` (`stored`), depois de validar as shares da rede.
use super::alloy::{AlloyRun, ResourceUpgrade, can_upgrade, order_by_distance};
use super::buffered_resource_network::{ResourceAcceptor, ResourceFamily};
use super::buffered_resource_transmitter::ResourceTransmitterUpgradeData;
use super::buffered_transmitter::{
    merge_networks_manually, recheck_connections_sides, requests_update,
};
use super::client::{Outgoing, ScaleOutbox};
use super::connection::{
    ConnectionType, TransmissionSet, TransmissionType, connection_map_contains_side,
};
use super::owner::{CableView, Event};
use super::registry::{TransmitterNetworkRegistry, set_transmitter_network};
use super::transmitter::{
    Compound, Effect, NeighborTransmitter, Overlay, Transmitter, TransmitterWorld,
};
use super::validator::CompatibleTransmitterValidator;
use super::{DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid};
use crate::containers::AutomationType;
use crate::resource_tank::{Resource, StackContents};
use crate::sides::Face;
use crate::sim::chunks::{BlockPos, MAX_VIEW_DISTANCE};
use crate::transaction::Transaction;
use std::collections::HashMap;
use std::rc::Rc;

/// A capability de recurso de um bloco vizinho (`ResourceHandler<R>`).
pub type Handler<R> = Rc<dyn ResourceAcceptor<R>>;
type Reg<R> = TransmitterNetworkRegistry<Handler<R>>;
/// A família de recurso do dono.
pub type Family<R> = ResourceFamily<R, dyn ResourceAcceptor<R>>;

fn arr(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

fn bp(pos: [i32; 3]) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

/// O que o jogo diz de um transmissor novo.
pub struct PipeInfo<R: Resource> {
    /// `tier.getCapacity()`.
    pub capacity: i64,
    /// `tier.getTransferRate()`.
    pub transfer_rate: i32,
    /// O que o block entity guardou (`TileEntityTransmitter.loadAdditional`) e o `stored`, se veio de um save.
    pub saved: Option<(Compound, StackContents<R>)>,
}

/// O mundo, como o dono o vê. O que `Transmitter` lê ([`TransmitterWorld`]) mais o que o dono manda de volta.
pub trait ResourceOwnerWorld<R: Resource> {
    /// `WorldUtils.isBlockLoaded`.
    fn is_block_loaded(&self, pos: BlockPos) -> bool;
    /// `WorldUtils.isGettingPowered`.
    fn is_getting_powered(&self, pos: BlockPos) -> bool;
    /// A capability de recurso do bloco em `pos` pelo lado `side` dele (`BlockCapabilityCache.getCapability`), `None` se não há (ou se o bloco
    /// é um transmissor: o dono nunca pergunta por eles).
    fn handler(&self, pos: BlockPos, side: Face) -> Option<Handler<R>>;
    /// O transmissor que o bloco em `pos` é, ou `None` se o bloco não é um dos nossos.
    fn pipe_info(&mut self, pos: BlockPos) -> Option<PipeInfo<R>>;
    /// `sendUpdatePacket`: as conexões do transmissor em `pos` mudaram.
    fn publish(&mut self, pos: BlockPos, view: &CableView);
    /// `BlockEntity.setChanged`.
    fn set_changed(&mut self, _pos: BlockPos) {}
    /// `TileEntityResourceTransmitter.sideChanged`.
    fn side_changed(
        &mut self,
        _pos: BlockPos,
        _side: Face,
        _old: ConnectionType,
        _new: ConnectionType,
    ) {
    }
    /// `TileEntityResourceTransmitter.redstoneChanged`.
    fn redstone_changed(&mut self, _pos: BlockPos, _powered: bool) {}
    /// `player.sendOverlayMessage`.
    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {}
}

/// O que a rede fez, na ordem, para um teste comparar com o rastro (só se o dono grava: [`ResourceTransmitters::record_events`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEvent {
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

struct Slot<R: Resource> {
    pos: BlockPos,
    /// `None` só enquanto o dono opera nele (o mundo vê os outros).
    tx: Option<Transmitter<Handler<R>>>,
    /// `System.identityHashCode`.
    hash: i32,
    /// `TileEntityTransmitter.markJoined`.
    mark_joined: bool,
    /// A rede do registro (o `Transmitter` guarda só o UUID dela) e o órfão: cópias, para ler com o transmissor fora do lugar.
    network: Option<NetworkId>,
    orphan: bool,
    /// O transmissor saiu do mundo (`setRemoved`): o registro ainda o conhece até o próximo tick.
    dead: bool,
}

impl<R: Resource> Slot<R> {
    /// `hasTransmitterNetwork()`.
    fn net(&self) -> Option<NetworkId> {
        self.network.filter(|_| !self.orphan)
    }
}

struct State<R: Resource> {
    dimension: DimensionId,
    transmission: TransmissionType,
    slots: Vec<Slot<R>>,
    by_pos: HashMap<BlockPos, TransmitterId>,
    family: Family<R>,
    make_family: Box<dyn Fn() -> Family<R>>,
    hashes: Box<dyn FnMut(BlockPos) -> i32>,
    pending_uuid: Option<Uuid>,
    uuids: HashMap<NetworkId, Uuid>,
    /// A transação raiz de `DynamicNetwork.invalidate` (`begin_take_shares` a `end_take_shares`).
    take_root: Option<Transaction>,
    /// Efeitos que um método do `TransmitterHost` pediu e que só o dono, com o registro, pode aplicar.
    deferred: Vec<(TransmitterId, Effect)>,
    /// A fusão manual está rodando: só o `adoptTransmittersAndAcceptorsFrom` dela vai para o rastro (o do construtor aparece em `created`).
    manual_merge: bool,
    record: bool,
    events: Vec<TraceEvent>,
    /// Os `TransferEvent` que o `onUpdate` viu, com o registro ligado (para os testes: o dono manda os pacotes e esvazia a lista da família).
    transfer_log: Vec<super::buffered_resource_network::TransferEvent<R>>,
    /// O `packetRange` de cada rede e os `PacketNetworkScale` (e o conteúdo do evento) que o `onUpdate` mandou.
    outbox: ScaleOutbox<Option<R>>,
}

impl<R: Resource> State<R> {
    fn new(
        dimension: DimensionId,
        transmission: TransmissionType,
        make_family: Box<dyn Fn() -> Family<R>>,
        hashes: Box<dyn FnMut(BlockPos) -> i32>,
    ) -> Self {
        Self {
            dimension,
            transmission,
            slots: Vec::new(),
            by_pos: HashMap::new(),
            family: make_family(),
            make_family,
            hashes,
            pending_uuid: None,
            uuids: HashMap::new(),
            take_root: None,
            deferred: Vec::new(),
            manual_merge: false,
            record: false,
            events: Vec::new(),
            transfer_log: Vec::new(),
            outbox: ScaleOutbox::new(dimension),
        }
    }

    fn tx(&self, id: TransmitterId) -> &Transmitter<Handler<R>> {
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

    fn event(&mut self, event: TraceEvent) {
        if self.record {
            self.events.push(event);
        }
    }
}

/// O que o `Transmitter` enxerga do mundo: os outros transmissores do dono e o jogo.
struct View<'a, R: Resource, W: ResourceOwnerWorld<R>> {
    st: &'a State<R>,
    world: &'a W,
}

impl<R: Resource, W: ResourceOwnerWorld<R>> View<'_, R, W> {
    fn id_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        if !self.world.is_block_loaded(pos) {
            return None;
        }
        self.st.live(pos)
    }
}

impl<R: Resource, W: ResourceOwnerWorld<R>> TransmitterWorld for View<'_, R, W> {
    type Acceptor = Handler<R>;

    fn is_block_loaded(&self, pos: [i32; 3]) -> bool {
        self.world.is_block_loaded(bp(pos))
    }

    fn is_getting_powered(&self, pos: [i32; 3]) -> bool {
        self.world.is_getting_powered(bp(pos))
    }

    fn transmitter_at(&self, pos: [i32; 3], side: Face) -> Option<NeighborTransmitter> {
        let id = self.id_at(bp(pos))?;
        let other = self.st.slots[id.0 as usize].tx.as_ref()?;
        Some(NeighborTransmitter {
            supported: other.supported_transmission_types(),
            can_connect_opposite: other.can_connect(side.opposite(), self),
        })
    }

    fn acceptor(&self, pos: [i32; 3], from_side: Face) -> Option<Handler<R>> {
        let at = bp(pos);
        if self.st.by_pos.contains_key(&at) {
            return None;
        }
        self.world.handler(at, from_side)
    }

    /// `MechanicalPipe.isValidTransmitter(transmitter, side)`, a parte que a regra básica não tem: `BufferedTransmitter` (rede contra órfão)
    /// e o conteúdo dos buffers.
    fn extra_valid_transmitter(&self, me: [i32; 3], other: [i32; 3], _side: Face) -> bool {
        let (Some(a), Some(b)) = (self.id_at(bp(me)), self.id_at(bp(other))) else {
            return true;
        };
        let (pa, pb) = (&self.st.slots[a.0 as usize], &self.st.slots[b.0 as usize]);
        let ends = |slot: &Slot<R>| self.st.family.ends(slot.net().is_some(), slot.orphan);
        super::buffered_transmitter::extra_valid_transmitter(ends(pa), ends(pb))
            && self
                .st
                .family
                .is_valid_transmitter(a, pa.net(), b, pb.net())
    }
}

/// O que o registro enxerga num instante: os transmissores e o jogo.
struct Ctx<'a, R: Resource, W: ResourceOwnerWorld<R>> {
    st: &'a mut State<R>,
    world: &'a mut W,
}

impl<R: Resource, W: ResourceOwnerWorld<R>> Ctx<'_, R, W> {
    fn view(&self) -> View<'_, R, W> {
        View {
            st: &*self.st,
            world: &*self.world,
        }
    }

    fn slot(&mut self, id: TransmitterId) -> &mut Slot<R> {
        &mut self.st.slots[id.0 as usize]
    }

    /// Roda uma operação do `Transmitter` com ele fora do lugar (ela lê o mundo, e o mundo guarda o transmissor), e aplica os efeitos que ela
    /// pediu.
    fn with_tr<T>(
        &mut self,
        registry: &mut Reg<R>,
        id: TransmitterId,
        f: impl FnOnce(&mut Transmitter<Handler<R>>, &View<'_, R, W>, &mut Vec<Effect>) -> T,
    ) -> Option<T> {
        let mut tx = self.slot(id).tx.take()?;
        let mut out = Vec::new();
        let result = f(&mut tx, &self.view(), &mut out);
        self.slot(id).tx = Some(tx);
        self.apply(registry, id, out);
        Some(result)
    }

    fn apply(&mut self, registry: &mut Reg<R>, id: TransmitterId, effects: Vec<Effect>) {
        let pos = self.st.slots[id.0 as usize].pos;
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => self.notify_neighbors(registry, pos),
                Effect::RefreshNeighbor { pos: other, side } => {
                    if let Some(other) = self.view().id_at(bp(other)) {
                        self.with_tr(registry, other, |t, w, out| {
                            t.refresh_connections_side(side, w, out)
                        });
                    }
                }
                Effect::RecheckConnections(mask) => {
                    // `BufferedTransmitter.recheckConnections`.
                    let net = self.st.slots[id.0 as usize].net();
                    match recheck_connections_sides(net.is_some(), true, mask) {
                        None => {
                            self.with_tr(registry, id, |t, w, out| {
                                t.base_recheck_connections(mask, w, out)
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
                    if self.st.slots[id.0 as usize].net().is_some() {
                        self.recheck_connection_prechecked(registry, id, side);
                    }
                }
                Effect::RequestsUpdate => self.requests_update(registry, id),
                Effect::SendUpdatePacket => self.publish(id),
                Effect::InvalidateTransmitter => {
                    registry.invalidate_transmitter(&*self, id);
                }
                Effect::AcceptorChanged(side) => {
                    if let Some(network) = self.st.slots[id.0 as usize].network {
                        registry.network_acceptor_changed(&*self, network, id, side);
                    }
                }
                Effect::SetForceUpdate => {
                    if let Some(tx) = self.slot(id).tx.as_mut() {
                        tx.set_force_update();
                    }
                }
                Effect::SetChanged => self.world.set_changed(pos),
                Effect::SideChanged { side, old, new } => {
                    self.world.side_changed(pos, side, old, new);
                    self.publish(id);
                }
                Effect::RedstoneChanged(powered) => {
                    self.world.redstone_changed(pos, powered);
                    self.publish(id);
                }
                Effect::Overlay(overlay) => self.world.overlay(pos, overlay),
                Effect::ClientNetworkRemove(_) | Effect::ClientNetworkAdd(_) => {}
            }
        }
    }

    /// `sendUpdatePacket`: o jogo recebe a vista do transmissor. Um transmissor que saiu, ou que está tirado do lugar por uma operação em
    /// andamento, não publica.
    fn publish(&mut self, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        if slot.tx.is_some() && !slot.dead {
            let pos = slot.pos;
            let view = view_of(&self.view(), id);
            self.world.publish(pos, &view);
        }
    }

    /// `BufferedTransmitter.requestsUpdate()`: o `update` do rastro e as conexões relidas.
    fn requests_update(&mut self, registry: &mut Reg<R>, id: TransmitterId) {
        let pos = self.st.slots[id.0 as usize].pos;
        self.st.event(TraceEvent::Update(pos));
        self.with_tr(registry, id, |t, w, out| requests_update(t, true, w, out));
    }

    /// `BufferedTransmitter.recheckConnectionPrechecked(side)`.
    fn recheck_connection_prechecked(
        &mut self,
        registry: &mut Reg<R>,
        id: TransmitterId,
        side: Face,
    ) {
        let pos = self.st.slots[id.0 as usize].pos;
        let Some(other) = self.view().id_at(pos.relative(side)) else {
            return;
        };
        let (Some(network), Some(other_network)) = (
            self.st.slots[id.0 as usize].net(),
            self.st.slots[other.0 as usize].net(),
        ) else {
            return;
        };
        let Some((primary, secondary)) = self.st.family.plan_merge(id, network, other_network)
        else {
            return;
        };
        self.st.manual_merge = true;
        let adopted = merge_networks_manually(registry, &mut *self, primary, secondary);
        self.st.manual_merge = false;
        self.st.family.clamp_buffer(primary);
        self.with_tr(registry, other, |t, w, out| {
            t.refresh_connections_side(side.opposite(), w, out)
        });
        for transmitter in adopted {
            self.requests_update(registry, transmitter);
        }
    }

    /// `Transmitter.notifyTileChange` -> `WorldUtils.notifyLoadedNeighborsOfTileChange`: os vizinhos carregados recebem `neighborChanged` sem orientação, na ordem de
    /// `EnumUtils.DIRECTIONS` (`Direction.values()`), e não na de `Level.updateNeighborsAt` (`NeighborUpdater.UPDATE_ORDER`).
    fn notify_neighbors(&mut self, registry: &mut Reg<R>, pos: BlockPos) {
        for side in Face::ALL {
            if let Some(id) = self.view().id_at(pos.relative(side)) {
                self.with_tr(registry, id, |t, w, out| {
                    t.on_neighbor_block_change(None, w, out)
                });
            }
        }
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(&mut self, registry: &mut Reg<R>, id: TransmitterId, was_present: bool) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.st.tx(id).loaded {
            self.slot(id).tx.as_mut().expect("no lugar").loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(&mut self, registry: &mut Reg<R>, id: TransmitterId, still_present: bool) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.st.tx(id).loaded {
            self.slot(id).tx.as_mut().expect("no lugar").loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

/// A vista do transmissor `id`: o que o modelo, a capability e o Configurator leem.
fn view_of<R: Resource, W: ResourceOwnerWorld<R>>(
    world: &View<'_, R, W>,
    id: TransmitterId,
) -> CableView {
    let tx = world.st.tx(id);
    let all = tx.get_all_current_connections();
    let activated = tx.is_redstone_activated(world);
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
        // `getContainers(direction)`: vazio se o lado é `NONE` cru ou o transmissor está ativado por redstone.
        view.exists[index] = !(view.raw[index] == ConnectionType::None || activated);
        // `canInsert(direction)` e o contêiner que aceita de fora: o da rede (o buffer do transmissor é `notExternal`).
        view.insert[index] = view.exists[index] && has_network && tx.can_insert(Some(side));
    }
    view
}

impl<R: Resource, W: ResourceOwnerWorld<R>> TransmitterHost<Handler<R>> for Ctx<'_, R, W> {
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
        self.st.slots[transmitter.0 as usize].orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        let slot = self.slot(transmitter);
        slot.orphan = orphan;
        if let Some(tx) = slot.tx.as_mut() {
            tx.set_orphan(orphan);
        }
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
        let slot = self.slot(transmitter);
        slot.network = network;
        // Só a atribuição: o `requestsUpdate` é do registro (`request_update_in`).
        if let Some(tx) = slot.tx.as_mut() {
            tx.set_transmitter_network(uuid, false, &mut Vec::new());
        }
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.st.slots[transmitter.0 as usize].hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        self.st.deferred.push((transmitter, Effect::RequestsUpdate));
    }

    fn request_update_in(&mut self, registry: &mut Reg<R>, transmitter: TransmitterId) {
        self.requests_update(registry, transmitter);
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
        let other = self.st.tx(other);
        self.st.tx(transmitter).is_valid_transmitter_basic(
            &NeighborTransmitter {
                supported: other.supported_transmission_types(),
                can_connect_opposite: other.can_connect(side.opposite(), &view),
            },
            side,
            &view,
        )
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.view().id_at(pos)
    }

    fn new_orphan_validator(
        &self,
        transmitter: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<Handler<R>>> {
        let net = self.st.slots[transmitter.0 as usize].net();
        Box::new(self.st.family.new_orphan_validator(transmitter, net))
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut Reg<R>,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        let pos = self.st.slots[transmitter.0 as usize].pos;
        self.st.event(TraceEvent::Access(pos, loaded));
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            let network = self.st.slots[transmitter.0 as usize].net();
            self.st
                .family
                .validate_and_take_share(registry, transmitter, network, None);
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face) {
        let Some(mut tx) = self.slot(transmitter).tx.take() else {
            return;
        };
        let mut out = Vec::new();
        tx.refresh_acceptor_connections(side, &self.view(), &mut out);
        self.slot(transmitter).tx = Some(tx);
        self.st
            .deferred
            .extend(out.into_iter().map(|effect| (transmitter, effect)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Handler<R>> {
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

    fn end_take_shares(&mut self) {
        if let Some(root) = self.st.take_root.take() {
            root.commit();
        }
        self.st.family.flush_contents();
    }

    fn take_share(&mut self, transmitter: TransmitterId) {
        let network = self.st.slots[transmitter.0 as usize].net();
        match self.st.take_root.take() {
            Some(root) => {
                self.st.family.take_share(transmitter, network, Some(&root));
                self.st.take_root = Some(root);
            }
            None => self.st.family.take_share_in_root(transmitter, network),
        }
    }

    fn on_update(&mut self, registry: &mut Reg<R>, network: NetworkId) {
        self.st.event(TraceEvent::NetUpdate(network));
        self.st.family.on_update(registry, network);
        // `Mekanism.onLiquidTransferred` e `onChemicalTransferred`: o `PacketNetworkScale` e o do conteúdo saem juntos (um `ClientboundBundlePacket`).
        for event in self.st.family.take_transfer_events() {
            if self.st.record {
                self.st.transfer_log.push(event.clone());
            }
            let uuid = self.st.uuid_value(event.network);
            let positions: Vec<BlockPos> = registry
                .network(event.network)
                .transmitters()
                .into_iter()
                .map(|id| self.st.slots[id.0 as usize].pos)
                .collect();
            self.st.outbox.send(
                event.network,
                uuid,
                event.scale,
                positions,
                Some(event.last_type),
            );
        }
    }

    fn network_event(&mut self, registry: &Reg<R>, network: NetworkId, event: NetworkEvent) {
        self.st
            .family
            .network_event(registry, network, event.clone());
        if matches!(
            event,
            NetworkEvent::ValidTransmittersAdded | NetworkEvent::Deregistered
        ) {
            self.st.outbox.flush_range(network);
        }
        match event {
            NetworkEvent::Created { uuid } => self.st.pending_uuid = Some(uuid),
            NetworkEvent::Merged { from } => {
                if let Some(uuid) = self.st.pending_uuid.take() {
                    self.st.uuids.insert(network, uuid);
                    self.st.event(TraceEvent::Created {
                        network,
                        hash: uuid.hash_code(),
                        from,
                    });
                }
            }
            NetworkEvent::Adopted { from } if self.st.manual_merge => {
                self.st.event(TraceEvent::Adopt { network, from });
            }
            NetworkEvent::Committing => self.st.event(TraceEvent::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.event(TraceEvent::Invalidate(network)),
            _ => {}
        }
    }
}

/// Os transmissores de recurso de uma dimensão.
pub struct ResourceTransmitters<R: Resource> {
    registry: Reg<R>,
    st: State<R>,
}

impl<R: Resource> ResourceTransmitters<R> {
    /// `make_family` dá a família (a de químico nasce com despejos); `uuids` dá o UUID de cada rede nova (`UUID.randomUUID`) e `hashes` o
    /// `identityHashCode` de cada transmissor novo (decisão 101).
    pub fn new(
        dimension: DimensionId,
        transmission: TransmissionType,
        make_family: impl Fn() -> Family<R> + 'static,
        uuids: impl FnMut() -> Uuid + 'static,
        hashes: impl FnMut(BlockPos) -> i32 + 'static,
    ) -> Self {
        Self {
            registry: TransmitterNetworkRegistry::new(uuids),
            st: State::new(
                dimension,
                transmission,
                Box::new(make_family),
                Box::new(hashes),
            ),
        }
    }

    /// Os `PacketNetworkScale` (e o conteúdo do tipo, `None` se a rede acabou vazia) que as redes mandaram desde a última chamada.
    pub fn take_scale_packets(&mut self) -> Vec<Outgoing<Option<R>>> {
        self.st.outbox.take()
    }

    /// Liga ou desliga o registro dos eventos da rede ([`TraceEvent`]), para os testes.
    pub fn record_events(&mut self, on: bool) {
        self.st.record = on;
    }

    /// Os `TransferEvent` desde a última chamada, com o registro ligado ([`Self::record_events`]).
    pub fn take_transfer_events(
        &mut self,
    ) -> Vec<super::buffered_resource_network::TransferEvent<R>> {
        std::mem::take(&mut self.st.transfer_log)
    }

    pub fn take_events(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.st.events)
    }

    pub fn registry(&self) -> &Reg<R> {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Reg<R> {
        &mut self.registry
    }

    pub fn family(&self) -> &Family<R> {
        &self.st.family
    }

    pub fn family_mut(&mut self) -> &mut Family<R> {
        &mut self.st.family
    }

    /// `world.getGameTime()`, antes de cada `ServerTickPost`.
    pub fn set_game_time(&mut self, time: i64) {
        self.st.family.game_time = time;
    }

    /// O transmissor vivo em `pos`.
    pub fn pipe_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.st.live(pos)
    }

    /// A posição de um transmissor (vivo ou não).
    pub fn pos_of(&self, id: TransmitterId) -> BlockPos {
        self.st.slots[id.0 as usize].pos
    }

    /// `hasTransmitterNetwork()` do transmissor em `pos`: a rede, se não é órfão.
    pub fn network_of(&self, pos: BlockPos) -> Option<NetworkId> {
        self.st.slots[self.st.live(pos)?.0 as usize].net()
    }

    /// A rede que o transmissor guarda, órfão ou não (`getTransmitterNetwork`).
    pub fn raw_network_of(&self, pos: BlockPos) -> Option<NetworkId> {
        self.st.slots[self.st.live(pos)?.0 as usize].network
    }

    pub fn is_orphan(&self, pos: BlockPos) -> Option<bool> {
        Some(self.st.slots[self.st.live(pos)?.0 as usize].orphan)
    }

    pub fn transmitter(&self, pos: BlockPos) -> Option<&Transmitter<Handler<R>>> {
        self.st.slots[self.st.live(pos)?.0 as usize].tx.as_ref()
    }

    /// As posições dos transmissores vivos, em ordem.
    pub fn positions(&self) -> Vec<BlockPos> {
        let mut list: Vec<BlockPos> = self.st.by_pos.keys().copied().collect();
        list.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        list
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Os eventos

    pub fn handle<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, event: Event) {
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
                    ctx.with_tr(&mut self.registry, id, |tx, view, out| {
                        tx.refresh_connections(view, out)
                    });
                }
            }
            Event::ChunkUnloaded(pos) => {
                if let Some(id) = self.st.live(pos) {
                    let network = self.st.slots[id.0 as usize].net();
                    self.st
                        .family
                        .validate_and_take_share(&self.registry, id, network, None);
                }
            }
            Event::Removed(pos) => self.removed(world, pos),
            Event::Update(pos) => self.update(world, pos),
            Event::Neighbor(pos) => {
                if let Some(id) = self.st.live(pos) {
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.with_tr(&mut self.registry, id, |tx, view, out| {
                        tx.on_neighbor_block_change(None, view, out)
                    });
                }
            }
            Event::Capability(pos, side) => {
                if let Some(id) = self.st.live(pos) {
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.with_tr(&mut self.registry, id, |tx, _, out| {
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
                let transmission = self.st.transmission;
                let make_family = std::mem::replace(
                    &mut self.st.make_family,
                    Box::new(|| unreachable!("a família foi movida")),
                );
                let hashes = std::mem::replace(&mut self.st.hashes, Box::new(|_| 0));
                let record = self.st.record;
                self.st = State::new(dimension, transmission, make_family, hashes);
                self.st.record = record;
            }
        }
        self.flush_deferred(world);
    }

    fn clear_removed<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, pos: BlockPos) {
        if let Some(id) = self.st.live(pos) {
            // O mesmo block entity entrando de novo: só marca para entrar.
            self.st.slots[id.0 as usize].mark_joined = true;
            return;
        }
        let Some(info) = world.pipe_info(pos) else {
            return;
        };
        let hash = (self.st.hashes)(pos);
        let id = TransmitterId(self.st.slots.len() as u32);
        let mut tx = Transmitter::new(arr(pos), TransmissionSet::of(&[self.st.transmission]), true);
        self.st
            .family
            .add_pipe(id, pos, info.capacity, info.transfer_rate);
        if let Some((tag, stored)) = info.saved {
            // `loadAdditional`, antes de `setLevel`.
            tx.read(&tag, &mut Vec::new());
            self.st.family.read_stored(id, stored);
        }
        tx.set_level(false);
        self.st.slots.push(Slot {
            pos,
            tx: Some(tx),
            hash,
            mark_joined: true,
            network: None,
            orphan: true,
            dead: false,
        });
        self.st.by_pos.insert(pos, id);
    }

    fn removed<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, pos: BlockPos) {
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

    fn update<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        self.pull_from_acceptors(world, id);
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        if ctx.st.slots[id.0 as usize].mark_joined {
            ctx.world_join(&mut self.registry, id, false);
            ctx.slot(id).mark_joined = false;
        }
        ctx.with_tr(&mut self.registry, id, |tx, view, out| {
            tx.on_update_server(view, out)
        });
    }

    /// `BufferedResourceTransmitter.pullFromAcceptors(level)`.
    fn pull_from_acceptors<W: ResourceOwnerWorld<R>>(&mut self, world: &W, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        let Some(tx) = slot.tx.as_ref() else {
            return;
        };
        let (net, pos, has_pull) = (slot.net(), slot.pos, tx.has_pull_side());
        // A família sai do estado enquanto lê o mundo (os aceitadores), que a contém.
        let mut family = std::mem::take(&mut self.st.family);
        family.pull_from_acceptors(
            id,
            net,
            has_pull,
            &|side| tx.is_connection_type(side, ConnectionType::Pull),
            &|side| {
                tx.acceptor_cache
                    .get_connected_acceptor(side, &|s| world.handler(pos.relative(s), s.opposite()))
            },
        );
        self.st.family = family;
    }

    fn flush_deferred<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W) {
        while !self.st.deferred.is_empty() {
            for (id, effect) in std::mem::take(&mut self.st.deferred) {
                let mut ctx = Ctx {
                    st: &mut self.st,
                    world: &mut *world,
                };
                ctx.apply(&mut self.registry, id, vec![effect]);
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // O que o jogo pergunta

    /// A vista do transmissor em `pos`.
    pub fn view_at<W: ResourceOwnerWorld<R>>(&self, world: &W, pos: BlockPos) -> Option<CableView> {
        let id = self.st.live(pos)?;
        self.st.slots[id.0 as usize].tx.as_ref()?;
        Some(view_of(
            &View {
                st: &self.st,
                world,
            },
            id,
        ))
    }

    /// O contêiner da capability do transmissor em `pos` (`getContainer()`): o da rede, ou o buffer dele sem rede.
    pub fn container_at(&self, pos: BlockPos) -> Option<crate::resource_tank::ResourceTank<R>> {
        let id = self.st.live(pos)?;
        Some(
            self.st
                .family
                .container_of(id, self.st.slots[id.0 as usize].net()),
        )
    }

    /// `getContainer().insert(resource, amount, transaction, EXTERNAL)`: o que o vizinho empurrou para o transmissor.
    pub fn insert_external(&mut self, pos: BlockPos, resource: &R, amount: i32) -> i32 {
        let Some(container) = self.container_at(pos) else {
            return 0;
        };
        let transaction = Transaction::open_root();
        let accepted = container.insert(resource, amount, &transaction, AutomationType::External);
        transaction.commit();
        self.st.family.flush_contents();
        accepted
    }

    /// O que o transmissor guarda no block entity (`TileEntityTransmitter.saveAdditional`, `BufferedResourceTransmitter.write`): o
    /// `Transmitter` e o `saveShare` (`stored`), depois de validar as shares da rede.
    pub fn write(&mut self, pos: BlockPos) -> Option<(Compound, StackContents<R>)> {
        let id = self.st.live(pos)?;
        let network = self.st.slots[id.0 as usize].net();
        let stored = self.st.family.write_stored(&self.registry, id, network);
        Some((self.st.tx(id).write(), stored))
    }

    /// `TileEntityTransmitter.onAlloyInteraction`, até a lista (veja `EnergyTransmitters::alloy_start`).
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

    /// O começo da troca de um transmissor (veja `EnergyTransmitters::alloy_take`).
    pub fn alloy_take<W: ResourceOwnerWorld<R>>(
        &mut self,
        world: &mut W,
        run: &mut AlloyRun,
        pos: BlockPos,
    ) -> Option<ResourceUpgrade<StackContents<R>>> {
        let id = self.st.live(pos)?;
        if !run.shares_set {
            let root = Transaction::open_root();
            self.st
                .family
                .validate_save_shares(&self.registry, run.network, id, Some(&root));
            root.commit();
            run.shares_set = true;
        }
        let network = self.st.slots[id.0 as usize].net();
        self.st.slots[id.0 as usize]
            .tx
            .as_mut()?
            .set_upgrading(true);
        self.st.family.take_share(id, network, None);
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world,
            };
            set_transmitter_network(&mut ctx, id, None, true);
        }
        let buffer = self.st.family.pipe(id).upgrade_data().buffer;
        let tx = self.st.slots[id.0 as usize].tx.as_ref()?;
        let data = ResourceUpgrade {
            redstone_reactive: tx.redstone_reactive,
            connection_types: *tx.connection_types_raw(),
            buffer,
        };
        self.flush_deferred(world);
        Some(data)
    }

    /// O fim da troca: o transmissor novo em `pos` recebe o dado.
    pub fn alloy_give(
        &mut self,
        run: &mut AlloyRun,
        pos: BlockPos,
        data: &ResourceUpgrade<StackContents<R>>,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        if let Some(tx) = self.st.slots[id.0 as usize].tx.as_mut() {
            tx.redstone_reactive = data.redstone_reactive;
            tx.set_connection_types_raw(data.connection_types);
        }
        let root = Transaction::open_root();
        self.st.family.pipe(id).parse_upgrade_data(
            &ResourceTransmitterUpgradeData {
                buffer: data.buffer.clone(),
            },
            &root,
        );
        root.commit();
        run.upgraded += 1;
        true
    }

    /// `transmitterNetwork.invalidate(null)` ao fim do laço, se algum subiu.
    pub fn alloy_finish<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, run: &AlloyRun) {
        if !run.did_upgrade() {
            return;
        }
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        self.registry
            .invalidate_network(&mut ctx, run.network, None);
        self.flush_deferred(world);
    }

    /// O pacote de atualização do transmissor (`writeReducedUpdatedTag`) e o `getCapacity()` dele: o que o cliente recebe a cada `sendUpdatePacket`.
    pub fn client_update(&self, pos: BlockPos) -> Option<(Compound, i64)> {
        let id = self.st.live(pos)?;
        Some((
            self.st.tx(id).write_reduced_updated_tag(),
            self.st.family.pipe(id).capacity(),
        ))
    }

    /// Os cliques do Configurator (`TileEntityTransmitter.onSneakRightClick`): `hit_side` é o lado do braço que o olhar toca. Devolve se o
    /// clique foi do transmissor.
    pub fn sneak_right_click<W: ResourceOwnerWorld<R>>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        hit_side: Option<Face>,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        let result = Ctx {
            st: &mut self.st,
            world: &mut *world,
        }
        .with_tr(&mut self.registry, id, |tx, view, out| {
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
    pub fn right_click<W: ResourceOwnerWorld<R>>(&mut self, world: &mut W, pos: BlockPos) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        let result = Ctx {
            st: &mut self.st,
            world: &mut *world,
        }
        .with_tr(&mut self.registry, id, |tx, view, out| {
            tx.on_right_click(view, out)
        });
        self.flush_deferred(world);
        result.is_some_and(|result| result.consumes_action())
    }

    /// O que a chave inglesa faz num lado (`setConnectionTypeRaw`, `onModeChange`, `refreshConnections` e `notifyTileChange`), para os testes.
    pub fn set_mode<W: ResourceOwnerWorld<R>>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        kind: ConnectionType,
    ) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world: &mut *world,
            };
            ctx.with_tr(&mut self.registry, id, |t, _, out| {
                t.set_connection_type_raw(side, kind, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, w, out| {
                t.on_mode_change(side, w, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, w, out| {
                t.refresh_connections(w, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, _, out| {
                t.notify_tile_change(out)
            });
        }
        self.flush_deferred(world);
    }
}

impl<R: Resource> super::reading::NetworkReader for ResourceTransmitters<R> {
    type Resource = R;

    /// `ItemNetworkReader.displayTransmitterInfo` para o cano ou tubo em `pos`: `getNeededInfo`, `getStoredInfo`, `getFlowInfo` e `getNetworkReaderCapacity` da rede de recurso.
    fn read_network(&self, pos: BlockPos) -> super::reading::TransmitterReading<R> {
        use super::reading::{NetworkReading, ReadingDetail, TransmitterReading};
        if self.st.live(pos).is_none() {
            return TransmitterReading::NoTransmitter;
        }
        let Some(network) = self.network_of(pos) else {
            return TransmitterReading::NoNetwork;
        };
        let Some(resource) = self.st.family.network(network) else {
            return TransmitterReading::NoNetwork;
        };
        let dynamic = self.registry.network(network);
        let container = resource.container();
        let stored = i64::try_from(container.amount()).unwrap_or(i64::MAX);
        let capacity = resource.capacity();
        TransmitterReading::Network(NetworkReading {
            transmitters: dynamic.transmitters_size(),
            acceptors: dynamic.acceptor_count(),
            detail: ReadingDetail::Resource {
                needed: super::reading::needed(capacity, stored),
                stored: container
                    .resource()
                    .filter(|_| !container.is_empty())
                    .map(|held| (held, stored)),
                flow: resource.prev_transfer_amount(),
                capacity,
            },
        })
    }

    fn acceptor_sides(
        &self,
        transmitter: BlockPos,
        acceptor: BlockPos,
    ) -> Option<super::reading::AcceptorSides> {
        let network = self.raw_network_of(transmitter)?;
        let dynamic = self.registry.network(network);
        dynamic
            .has_acceptor(acceptor)
            .then(|| super::reading::AcceptorSides {
                network,
                sides: dynamic.acceptor_directions(acceptor),
            })
    }
}
