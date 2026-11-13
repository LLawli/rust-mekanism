//! O dono dos condutores termodinâmicos, irmão de [`super::owner::EnergyTransmitters`] e de [`super::resource_owner::ResourceTransmitters`]:
//! liga o `ThermodynamicConductor` (conexões por lado, redstone, Configurator, o capacitor), o registro de redes e a [`HeatNetworks`] ao que o
//! jogo entrega (`TileEntityTransmitter`, `BlockTransmitter.neighborChanged`, o `RefreshListener` do `AcceptorCache`, o `ServerTickEvent.Post`).
//! Os eventos são os mesmos dos outros donos ([`Event`], decisão 107) e a lib continua sem conhecer id: o jogo (ou o mundo simulado de um
//! teste) entra por [`HeatOwnerWorld`].
//!
//! O que difere dos donos com buffer, e que vem do Java:
//!
//! - a rede de calor não tem buffer nem share: nada de `takeShare`, de família nem de fusão manual (`ThermodynamicConductor` é um
//!   `Transmitter` simples, o `requestsUpdate` dele só manda o pacote de atualização);
//! - os outros condutores e os tiles com a capability `HEAT` são os aceitadores: um condutor vizinho é aceitador onde a conexão dele não é
//!   `NONE` e o redstone não o ativa (`TileEntityThermodynamicConductor`, o `HeatHandlerManager`);
//! - o calor de um condutor é um capacitor com diário: um `simulate` da rede abre uma transação só, e o `commit` dela chama o
//!   `onContentsChanged` de cada capacitor que mudou (`HeatNetwork.onUpdate`);
//! - `sideChanged` e `redstoneChanged` invalidam a capability `HEAT` do condutor na hora, e os vizinhos que a guardavam no cache de
//!   aceitadores a releem.
use super::connection::{ConnectionType, connection_map_contains_side};
use super::heat_network::{ConductorSet, HeatNetworks};
use super::owner::{CableView, Event};
use super::registry::TransmitterNetworkRegistry;
use super::thermodynamic_conductor::{
    CapacitorState, ConductorTag, ConductorTier, ConductorTile, ContentsChanged,
    ThermodynamicConductor, TierValues,
};
use super::transmitter::{
    Compound, Effect, NeighborTransmitter, Overlay, Transmitter, TransmitterWorld,
};
use super::{DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid};
use crate::heat::{self, AdjacentSink, AmbientSource, HeatCapacitors, HeatHandlers, HeatTransfer};
use crate::sides::Face;
use crate::sim::chunks::{BlockPos, MAX_VIEW_DISTANCE};
use std::collections::HashMap;

fn arr(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

fn bp(pos: [i32; 3]) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

/// O `IHeatHandler` de um vizinho de um condutor: outro condutor, ou o capacitor de um tile com a capability `HEAT` visto de um lado dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sink {
    Conductor(TransmitterId),
    Host { pos: BlockPos, side: Face },
}

/// O que um tile com a capability `HEAT` diz de um capacitor seu, visto de um lado (`IHeatHandler` e `IHeatCapacitor`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostCapacitor {
    pub temperature: f64,
    pub heat_capacity: f64,
    pub inverse_conduction: f64,
    pub inverse_insulation: f64,
}

/// O que o jogo diz de um condutor novo.
pub struct ConductorInfo {
    pub tier: ConductorTier,
    /// A condução, a capacidade e a isolação de agora (a config).
    pub values: TierValues,
    /// O que o block entity guardou (`TileEntityTransmitter.loadAdditional`) e o estado do capacitor, se veio de um save.
    pub saved: Option<(Compound, Option<CapacitorState>)>,
}

/// O mundo, como o dono o vê. O que `Transmitter` lê ([`TransmitterWorld`]) mais o que o dono manda de volta.
pub trait HeatOwnerWorld {
    /// `WorldUtils.isBlockLoaded`.
    fn is_block_loaded(&self, pos: BlockPos) -> bool;
    /// `WorldUtils.isGettingPowered`.
    fn is_getting_powered(&self, pos: BlockPos) -> bool;
    /// A capability `HEAT` do bloco em `pos` pelo lado `side` dele, ou `None` se não há (o dono nunca pergunta por um condutor).
    fn host(&self, pos: BlockPos, side: Face) -> Option<HostCapacitor>;
    /// `handleHeat(transfer, transaction)` do capacitor do tile em `pos`, visto pelo lado `side`.
    fn host_handle_heat(&self, pos: BlockPos, side: Face, transfer: f64);
    /// `HeatAPI.getAmbientTemp` do bloco em `pos` (`None`) ou do vizinho dele por `side` (`Some`); `None` é o `world == null`.
    fn ambient(&self, pos: BlockPos, side: Option<Face>) -> Option<f64>;
    /// O condutor que o bloco em `pos` é, ou `None` se o bloco não é um dos nossos.
    fn conductor_info(&mut self, pos: BlockPos) -> Option<ConductorInfo>;
    /// `sendUpdatePacket`: as conexões do condutor em `pos` mudaram.
    fn publish(&mut self, pos: BlockPos, view: &CableView);
    /// `BlockEntity.setChanged`.
    fn set_changed(&mut self, _pos: BlockPos) {}
    /// `onContentsChanged` com `sendUpdatePacket`: o calor do condutor em `pos` se afastou o bastante do que o cliente sabe.
    fn heat_packet(&mut self, _pos: BlockPos) {}
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

/// O que a rede fez, na ordem, para um teste comparar com o rastro (só se o dono grava: [`HeatTransmitters::record_events`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEvent {
    Update(BlockPos),
    Commit(NetworkId),
    Invalidate(NetworkId),
    NetUpdate(NetworkId),
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
}

struct Slot {
    pos: BlockPos,
    /// `None` só enquanto o dono opera nele (o mundo vê os outros).
    tc: Option<ThermodynamicConductor<Sink>>,
    /// `System.identityHashCode`.
    hash: i32,
    /// `TileEntityTransmitter.markJoined`.
    mark_joined: bool,
    /// A rede do registro (o `Transmitter` guarda só o UUID dela) e o órfão: cópias, para ler com o condutor fora do lugar.
    network: Option<NetworkId>,
    orphan: bool,
    /// O condutor saiu do mundo (`setRemoved`): o registro ainda o conhece até o próximo tick.
    dead: bool,
    /// Quantas vezes o `onContentsChanged` mandou o pacote de atualização.
    packets: u32,
}

struct State {
    dimension: DimensionId,
    slots: Vec<Slot>,
    by_pos: HashMap<BlockPos, TransmitterId>,
    heat_networks: HeatNetworks,
    hashes: Box<dyn FnMut(BlockPos) -> i32>,
    pending_uuid: Option<Uuid>,
    uuids: HashMap<NetworkId, Uuid>,
    /// Efeitos que um método do `TransmitterHost` pediu e que só o dono, com o registro, pode aplicar.
    deferred: Vec<(TransmitterId, Effect)>,
    record: bool,
    events: Vec<TraceEvent>,
}

impl State {
    fn live(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.by_pos.get(&pos).copied()
    }

    fn tc(&self, id: TransmitterId) -> &ThermodynamicConductor<Sink> {
        self.slots[id.0 as usize]
            .tc
            .as_ref()
            .expect("o condutor está no lugar")
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

/// O ar de um bloco, lido do mundo do jogo.
struct AmbientAt<'a, W: HeatOwnerWorld> {
    world: &'a W,
    pos: BlockPos,
}

impl<W: HeatOwnerWorld> AmbientSource for AmbientAt<'_, W> {
    fn read(&self, side: Option<Face>) -> Option<f64> {
        self.world.ambient(self.pos, side)
    }
}

/// O que o `Transmitter` enxerga do mundo: os outros condutores do dono e o jogo.
struct View<'a, W: HeatOwnerWorld> {
    st: &'a State,
    world: &'a W,
}

impl<W: HeatOwnerWorld> View<'_, W> {
    fn id_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        if !self.world.is_block_loaded(pos) {
            return None;
        }
        self.st.live(pos)
    }
}

impl<W: HeatOwnerWorld> TransmitterWorld for View<'_, W> {
    type Acceptor = Sink;

    fn is_block_loaded(&self, pos: [i32; 3]) -> bool {
        self.world.is_block_loaded(bp(pos))
    }

    fn is_getting_powered(&self, pos: [i32; 3]) -> bool {
        self.world.is_getting_powered(bp(pos))
    }

    fn transmitter_at(&self, pos: [i32; 3], side: Face) -> Option<NeighborTransmitter> {
        let id = self.id_at(bp(pos))?;
        let other = &self.st.slots[id.0 as usize].tc.as_ref()?.transmitter;
        Some(NeighborTransmitter {
            supported: other.supported_transmission_types(),
            can_connect_opposite: other.can_connect(side.opposite(), self),
        })
    }

    /// `Capabilities.HEAT` do vizinho. Um condutor responde com o capacitor dele, e não onde a conexão dele é `NONE` ou o redstone o ativa
    /// (`TileEntityThermodynamicConductor`); um tile qualquer responde pelo que o jogo diz dele.
    fn acceptor(&self, pos: [i32; 3], from_side: Face) -> Option<Sink> {
        let at = bp(pos);
        if let Some(id) = self.st.live(at) {
            if !self.world.is_block_loaded(at) {
                return None;
            }
            let transmitter = &self.st.slots[id.0 as usize].tc.as_ref()?.transmitter;
            let blocked = transmitter.get_connection_type_raw(from_side) == ConnectionType::None
                || transmitter.is_redstone_activated(self);
            return (!blocked).then_some(Sink::Conductor(id));
        }
        self.world.host(at, from_side)?;
        Some(Sink::Host {
            pos: at,
            side: from_side,
        })
    }
}

/// O que o registro enxerga num instante: os condutores e o jogo.
struct Ctx<'a, W: HeatOwnerWorld> {
    st: &'a mut State,
    world: &'a mut W,
}

type Reg = TransmitterNetworkRegistry<Sink>;

impl<W: HeatOwnerWorld> Ctx<'_, W> {
    fn view(&self) -> View<'_, W> {
        View {
            st: &*self.st,
            world: &*self.world,
        }
    }

    fn slot(&mut self, id: TransmitterId) -> &mut Slot {
        &mut self.st.slots[id.0 as usize]
    }

    /// Roda uma operação do `Transmitter` com ele fora do lugar (ela lê o mundo, e o mundo guarda o condutor), e aplica os efeitos que ela
    /// pediu.
    fn with_tr<T>(
        &mut self,
        registry: &mut Reg,
        id: TransmitterId,
        f: impl FnOnce(&mut Transmitter<Sink>, &View<'_, W>, &mut Vec<Effect>) -> T,
    ) -> Option<T> {
        let mut tc = self.slot(id).tc.take()?;
        let mut out = Vec::new();
        let result = f(&mut tc.transmitter, &self.view(), &mut out);
        self.slot(id).tc = Some(tc);
        self.apply(registry, id, out);
        Some(result)
    }

    fn apply(&mut self, registry: &mut Reg, id: TransmitterId, effects: Vec<Effect>) {
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
                    self.with_tr(registry, id, |t, w, out| {
                        t.base_recheck_connections(mask, w, out)
                    });
                }
                Effect::InvalidateTransmitter => {
                    registry.invalidate_transmitter(&*self, id);
                }
                Effect::AcceptorChanged(side) => {
                    if let Some(network) = self.st.slots[id.0 as usize].network {
                        registry.network_acceptor_changed(&*self, network, id, side);
                    }
                }
                Effect::SetForceUpdate => {
                    if let Some(tc) = self.slot(id).tc.as_mut() {
                        tc.transmitter.set_force_update();
                    }
                }
                Effect::SendUpdatePacket | Effect::RequestsUpdate => self.publish(id),
                Effect::SetChanged => self.world.set_changed(pos),
                Effect::SideChanged { side, old, new } => {
                    self.world.side_changed(pos, side, old, new);
                    // `TileEntityThermodynamicConductor.sideChanged`: sem conexão no lado a capability some (`invalidateCapability`), e uma
                    // que volta avisa quem escuta a posição (`invalidateCapabilities`); nos dois casos o nível avisa os caches vizinhos.
                    if new == ConnectionType::None || old == ConnectionType::None {
                        self.invalidate_capabilities(registry, pos);
                    }
                    self.publish(id);
                }
                Effect::RedstoneChanged(powered) => {
                    self.world.redstone_changed(pos, powered);
                    // `redstoneChanged`: ativado ou não, o nível avisa os caches vizinhos (`invalidateCapabilityAll`, `invalidateCapabilities`).
                    self.invalidate_capabilities(registry, pos);
                    self.publish(id);
                }
                Effect::Overlay(overlay) => self.world.overlay(pos, overlay),
                // `Transmitter.recheckConnection(side)` é vazio no condutor (só o `BufferedTransmitter` o usa).
                Effect::RecheckConnection(_)
                | Effect::ClientNetworkRemove(_)
                | Effect::ClientNetworkAdd(_) => {}
            }
        }
    }

    /// `BlockEntity.invalidateCapabilities`: `Level.invalidateCapabilities(pos)` chama o `RefreshListener` de cada cache que escuta a posição,
    /// e aqui só os condutores vizinhos guardam um cache de aceitadores dela.
    fn invalidate_capabilities(&mut self, registry: &mut Reg, pos: BlockPos) {
        for side in Face::ALL {
            let Some(id) = self.view().id_at(pos.relative(side)) else {
                continue;
            };
            self.with_tr(registry, id, |t, _, out| {
                if let Some(dirty) = t
                    .acceptor_cache
                    .refresh_listener_run(side.opposite(), t.is_valid())
                {
                    t.mark_dirty_acceptor(dirty, out);
                }
            });
        }
    }

    /// `sendUpdatePacket`: o jogo recebe a vista do condutor. Um condutor que saiu, ou que está tirado do lugar por uma operação em
    /// andamento, não publica.
    fn publish(&mut self, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        if slot.tc.is_some() && !slot.dead {
            let pos = slot.pos;
            let view = view_of(&self.view(), id);
            self.world.publish(pos, &view);
        }
    }

    /// `Transmitter.notifyTileChange` -> `WorldUtils.notifyLoadedNeighborsOfTileChange`: os vizinhos carregados recebem `neighborChanged` sem orientação, na ordem de
    /// `EnumUtils.DIRECTIONS` (`Direction.values()`).
    fn notify_neighbors(&mut self, registry: &mut Reg, pos: BlockPos) {
        for side in Face::ALL {
            if let Some(id) = self.view().id_at(pos.relative(side)) {
                self.with_tr(registry, id, |t, w, out| {
                    t.on_neighbor_block_change(None, w, out)
                });
            }
        }
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(&mut self, registry: &mut Reg, id: TransmitterId, was_present: bool) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.st.tc(id).transmitter.loaded {
            self.slot(id)
                .tc
                .as_mut()
                .expect("no lugar")
                .transmitter
                .loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(&mut self, registry: &mut Reg, id: TransmitterId, still_present: bool) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.st.tc(id).transmitter.loaded {
            self.slot(id)
                .tc
                .as_mut()
                .expect("no lugar")
                .transmitter
                .loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }

    /// `onContentsChanged` de um capacitor que confirmou: o `setChanged` do tile sempre, e o pacote se a temperatura se afastou.
    fn contents_changed(&mut self, id: TransmitterId, changed: ContentsChanged) {
        let pos = self.st.slots[id.0 as usize].pos;
        self.world.set_changed(pos);
        if changed.send_update_packet {
            self.st.slots[id.0 as usize].packets += 1;
            self.world.heat_packet(pos);
        }
    }
}

/// A vista do condutor `id`: o que o modelo, a capability e o Configurator leem.
fn view_of<W: HeatOwnerWorld>(world: &View<'_, W>, id: TransmitterId) -> CableView {
    let tx = &world.st.tc(id).transmitter;
    let all = tx.get_all_current_connections();
    let activated = tx.is_redstone_activated(world);
    let mut view = CableView {
        connected: [false; 6],
        kind: [ConnectionType::None; 6],
        raw: [ConnectionType::None; 6],
        exists: [false; 6],
        insert: [false; 6],
        has_network: tx.has_transmitter_network(),
        redstone_reactive: tx.redstone_reactive,
        redstone_activated: activated,
    };
    for side in Face::ALL {
        let index = side as usize;
        view.connected[index] = connection_map_contains_side(all, side);
        view.kind[index] = tx.get_connection_type(side);
        view.raw[index] = tx.get_connection_type_raw(side);
        // `HeatHandlerManager`: nenhum capacitor onde a conexão crua é `NONE` ou com o redstone ativado.
        view.exists[index] = !(view.raw[index] == ConnectionType::None || activated);
    }
    view
}

impl<W: HeatOwnerWorld> TransmitterHost<Sink> for Ctx<'_, W> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.st.slots[transmitter.0 as usize].pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        self.st.dimension
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        self.st.slots[transmitter.0 as usize]
            .tc
            .as_ref()
            .is_some_and(|tc| tc.transmitter.is_valid())
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.st.slots[transmitter.0 as usize].orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        let slot = self.slot(transmitter);
        slot.orphan = orphan;
        if let Some(tc) = slot.tc.as_mut() {
            tc.transmitter.set_orphan(orphan);
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
        if let Some(tc) = slot.tc.as_mut() {
            tc.transmitter
                .set_transmitter_network(uuid, false, &mut Vec::new());
        }
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.st.slots[transmitter.0 as usize].hash
    }

    /// `Transmitter.requestsUpdate`: o pacote de atualização (o rastro anota o pedido).
    fn request_update(&mut self, transmitter: TransmitterId) {
        let pos = self.st.slots[transmitter.0 as usize].pos;
        self.st.event(TraceEvent::Update(pos));
        self.st
            .deferred
            .push((transmitter, Effect::SendUpdatePacket));
    }

    fn supports_transmission_type(&self, transmitter: TransmitterId, other: TransmitterId) -> bool {
        self.st
            .tc(transmitter)
            .transmitter
            .supports_transmission_type(
                self.st.tc(other).transmitter.supported_transmission_types(),
            )
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        let view = self.view();
        let other = &self.st.tc(other).transmitter;
        self.st
            .tc(transmitter)
            .transmitter
            .is_valid_transmitter_basic(
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

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut Reg,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face) {
        let Some(mut tc) = self.slot(transmitter).tc.take() else {
            return;
        };
        let mut out = Vec::new();
        tc.transmitter
            .refresh_acceptor_connections(side, &self.view(), &mut out);
        self.slot(transmitter).tc = Some(tc);
        self.st
            .deferred
            .extend(out.into_iter().map(|effect| (transmitter, effect)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Sink> {
        let tx = &self.st.tc(transmitter).transmitter;
        if tx.can_connect_to_acceptor(side) {
            tx.get_acceptor(side, &self.view())
        } else {
            None
        }
    }

    /// `HeatNetwork.onUpdate`, com os condutores da rede na ordem do registro.
    fn on_update(&mut self, registry: &mut Reg, network: NetworkId) {
        self.st.event(TraceEvent::NetUpdate(network));
        let conductors = registry.network(network).transmitters();
        let mut heat_networks = std::mem::take(&mut self.st.heat_networks);
        {
            let mut set = Set {
                ctx: &mut *self,
                registry: &*registry,
            };
            heat_networks.on_update(network, &mut set, &conductors);
        }
        self.st.heat_networks = heat_networks;
    }

    fn network_event(&mut self, _: &Reg, network: NetworkId, event: NetworkEvent) {
        self.st.heat_networks.network_event(network, &event);
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
            NetworkEvent::Committing => self.st.event(TraceEvent::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.event(TraceEvent::Invalidate(network)),
            _ => {}
        }
    }
}

/// Os `IHeatHandler` do mundo, por [`Sink`]. Os condutores guardam o calor e o diário em `Cell`; o tile vizinho responde pelo jogo.
struct Arena<'a, W: HeatOwnerWorld> {
    st: &'a State,
    world: &'a W,
}

impl<W: HeatOwnerWorld> Arena<'_, W> {
    fn conductor_ambient(&self, id: TransmitterId) -> AmbientAt<'_, W> {
        AmbientAt {
            world: &*self.world,
            pos: self.st.slots[id.0 as usize].pos,
        }
    }

    fn host(&self, pos: BlockPos, side: Face) -> HostCapacitor {
        self.world
            .host(pos, side)
            .expect("a capability existia ao escolher o vizinho")
    }
}

impl<W: HeatOwnerWorld> HeatHandlers for Arena<'_, W> {
    type Handle = Sink;

    fn temperature(&self, handle: Sink) -> f64 {
        match handle {
            Sink::Conductor(id) => self.st.tc(id).temperature(&self.conductor_ambient(id)),
            Sink::Host { pos, side } => self.host(pos, side).temperature,
        }
    }

    fn inverse_conduction(&self, handle: Sink) -> f64 {
        match handle {
            Sink::Conductor(id) => self.st.tc(id).inverse_conduction(),
            Sink::Host { pos, side } => self.host(pos, side).inverse_conduction,
        }
    }

    fn heat_capacity(&self, handle: Sink) -> f64 {
        match handle {
            Sink::Conductor(id) => self.st.tc(id).heat_capacity(),
            Sink::Host { pos, side } => self.host(pos, side).heat_capacity,
        }
    }

    fn handle_heat(&mut self, handle: Sink, transfer: f64) {
        match handle {
            Sink::Conductor(id) => {
                self.st
                    .tc(id)
                    .handle_heat(transfer, &self.conductor_ambient(id));
            }
            Sink::Host { pos, side } => self.world.host_handle_heat(pos, side, transfer),
        }
    }
}

impl<W: HeatOwnerWorld> HeatCapacitors for Arena<'_, W> {
    fn inverse_insulation(&self, handle: Sink) -> f64 {
        match handle {
            Sink::Conductor(id) => self.st.tc(id).inverse_insulation(),
            Sink::Host { pos, side } => self.host(pos, side).inverse_insulation,
        }
    }
}

/// `HeatNetwork.onUpdate` sobre o mundo.
struct Set<'a, 'b, W: HeatOwnerWorld> {
    ctx: &'a mut Ctx<'b, W>,
    registry: &'a Reg,
}

impl<W: HeatOwnerWorld> Set<'_, '_, W> {
    /// O `commit()` da transação raiz: cada capacitor que mudou chama o `onContentsChanged` do dono dele.
    fn commit_all(&mut self) {
        commit_pending(self.ctx);
    }
}

fn commit_pending<W: HeatOwnerWorld>(ctx: &mut Ctx<'_, W>) {
    let mut changed = Vec::new();
    for (index, slot) in ctx.st.slots.iter().enumerate() {
        if let Some(tc) = &slot.tc {
            let ambient = AmbientAt {
                world: &*ctx.world,
                pos: slot.pos,
            };
            if let Some(contents) = tc.commit(&ambient) {
                changed.push((TransmitterId(index as u32), contents));
            }
        }
    }
    for (id, contents) in changed {
        ctx.contents_changed(id, contents);
    }
}

impl<W: HeatOwnerWorld> ConductorSet for Set<'_, '_, W> {
    fn simulate(&mut self, conductor: TransmitterId) -> HeatTransfer {
        let slot = &self.ctx.st.slots[conductor.0 as usize];
        let tc = slot.tc.as_ref().expect("o condutor está no lugar");
        let ambient = AmbientAt {
            world: &*self.ctx.world,
            pos: slot.pos,
        };
        let network = slot.network;
        let registry = self.registry;
        let network_has = move |p: [i32; 3]| {
            network.is_some_and(|n| registry.network(n).transmitter_at(bp(p)).is_some())
        };
        let view = View {
            st: &*self.ctx.st,
            world: &*self.ctx.world,
        };
        let tile = ConductorTile {
            conductor: tc,
            own: Sink::Conductor(conductor),
            world: &view,
            ambient: &ambient,
            network_has: &network_has,
        };
        heat::simulate(
            &tile,
            &mut Arena {
                st: &*self.ctx.st,
                world: &*self.ctx.world,
            },
        )
    }

    fn commit(&mut self) {
        self.commit_all();
    }

    fn temperature(&mut self, conductor: TransmitterId) -> f64 {
        Arena {
            st: &*self.ctx.st,
            world: &*self.ctx.world,
        }
        .temperature(Sink::Conductor(conductor))
    }
}

/// O condutor `pos` visto como o vizinho de um tile que entrega calor por ele ([`AdjacentSink`]): o `IHeatHandler` que a capability `HEAT`
/// do condutor devolve a quem pergunta (`Capabilities.HEAT.getCapabilityIfLoaded(level, pos, side)`).
pub struct ConductorSink<'a, W: HeatOwnerWorld> {
    conductor: &'a ThermodynamicConductor<Sink>,
    world: &'a W,
    pos: BlockPos,
}

impl<W: HeatOwnerWorld> ConductorSink<'_, W> {
    fn ambient(&self) -> AmbientAt<'_, W> {
        AmbientAt {
            world: self.world,
            pos: self.pos,
        }
    }
}

impl<W: HeatOwnerWorld> AdjacentSink for ConductorSink<'_, W> {
    fn temperature(&mut self) -> f64 {
        self.conductor.temperature(&self.ambient())
    }

    fn heat_capacity(&mut self) -> f64 {
        self.conductor.heat_capacity()
    }

    fn inverse_conduction(&mut self) -> f64 {
        self.conductor.inverse_conduction()
    }

    fn handle_heat(&mut self, transfer: f64) {
        self.conductor.handle_heat(transfer, &self.ambient());
    }
}

/// O que o jogo lê de um condutor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductorState {
    /// `storedHeat`, `None` enquanto vale -1.
    pub heat: Option<f64>,
    pub heat_capacity: f64,
    /// `clientTemperature`.
    pub client_temperature: f64,
    pub network: Option<NetworkId>,
    pub orphan: bool,
    pub valid: bool,
    /// Quantas vezes o `onContentsChanged` mandou o pacote de atualização.
    pub packets: u32,
}

/// Os condutores termodinâmicos de uma dimensão.
pub struct HeatTransmitters {
    registry: Reg,
    st: State,
}

impl HeatTransmitters {
    /// `uuids` dá o UUID de cada rede nova (`UUID.randomUUID`) e `hashes` o `identityHashCode` de cada condutor novo (decisão 101).
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
                heat_networks: HeatNetworks::default(),
                hashes: Box::new(hashes),
                pending_uuid: None,
                uuids: HashMap::new(),
                deferred: Vec::new(),
                record: false,
                events: Vec::new(),
            },
        }
    }

    /// Liga ou desliga o registro dos eventos da rede ([`TraceEvent`]), para os testes.
    pub fn record_events(&mut self, on: bool) {
        self.st.record = on;
    }

    pub fn take_events(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.st.events)
    }

    pub fn registry(&self) -> &Reg {
        &self.registry
    }

    pub fn heat_networks(&self) -> &HeatNetworks {
        &self.st.heat_networks
    }

    /// O condutor vivo em `pos`.
    pub fn conductor_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.st.live(pos)
    }

    /// A posição de um condutor (vivo ou não).
    pub fn pos_of(&self, id: TransmitterId) -> BlockPos {
        self.st.slots[id.0 as usize].pos
    }

    /// As posições dos condutores vivos, em ordem.
    pub fn positions(&self) -> Vec<BlockPos> {
        let mut list: Vec<BlockPos> = self.st.by_pos.keys().copied().collect();
        list.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        list
    }

    pub fn transmitter(&self, pos: BlockPos) -> Option<&Transmitter<Sink>> {
        Some(
            &self.st.slots[self.st.live(pos)?.0 as usize]
                .tc
                .as_ref()?
                .transmitter,
        )
    }

    /// O capacitor e a rede do condutor em `pos`.
    pub fn state_at(&self, pos: BlockPos) -> Option<ConductorState> {
        let slot = &self.st.slots[self.st.live(pos)?.0 as usize];
        let tc = slot.tc.as_ref()?;
        Some(ConductorState {
            heat: tc.stored_heat(),
            heat_capacity: tc.heat_capacity(),
            client_temperature: tc.client_temperature(),
            network: slot.network,
            orphan: slot.orphan,
            valid: tc.transmitter.is_valid(),
            packets: slot.packets,
        })
    }

    /// A temperatura do condutor em `pos` (`getTemperature`, que inicializa o calor na primeira leitura).
    pub fn temperature_at<W: HeatOwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<f64> {
        let slot = &self.st.slots[self.st.live(pos)?.0 as usize];
        let ambient = AmbientAt { world, pos };
        Some(slot.tc.as_ref()?.temperature(&ambient))
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Os eventos

    pub fn handle<W: HeatOwnerWorld>(&mut self, world: &mut W, event: Event) {
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
            // O calor não tem parte na rede para tomar (`takeShare` vazio): o chunk que sai só leva o NBT, que o jogo grava.
            Event::ChunkUnloaded(_) => {}
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
                self.st.slots.clear();
                self.st.by_pos.clear();
                self.st.heat_networks = HeatNetworks::default();
                self.st.pending_uuid = None;
                self.st.uuids.clear();
                self.st.deferred.clear();
            }
        }
        self.flush_deferred(world);
    }

    fn clear_removed<W: HeatOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        if let Some(id) = self.st.live(pos) {
            // O mesmo block entity entrando de novo: só marca para entrar.
            self.st.slots[id.0 as usize].mark_joined = true;
            return;
        }
        let Some(info) = world.conductor_info(pos) else {
            return;
        };
        let hash = (self.st.hashes)(pos);
        let id = TransmitterId(self.st.slots.len() as u32);
        let mut tc = ThermodynamicConductor::with_values(arr(pos), info.tier, info.values);
        let mut contents = None;
        if let Some((tag, capacitor)) = info.saved {
            // `loadAdditional`, antes de `setLevel`.
            let ambient = AmbientAt {
                world: &*world,
                pos,
            };
            contents = tc.read(&tag, capacitor, &ambient, &mut Vec::new());
        }
        tc.transmitter.set_level(false);
        self.st.slots.push(Slot {
            pos,
            tc: Some(tc),
            hash,
            mark_joined: true,
            network: None,
            orphan: true,
            dead: false,
            packets: 0,
        });
        self.st.by_pos.insert(pos, id);
        if let Some(contents) = contents {
            let mut ctx = Ctx {
                st: &mut self.st,
                world,
            };
            ctx.contents_changed(id, contents);
        }
    }

    fn removed<W: HeatOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        {
            let slot = &mut self.st.slots[id.0 as usize];
            slot.tc.as_mut().expect("no lugar").transmitter.removed = true;
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

    /// `TileEntityTransmitter.onUpdateServer`.
    fn update<W: HeatOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
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

    fn flush_deferred<W: HeatOwnerWorld>(&mut self, world: &mut W) {
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

    /// A vista do condutor em `pos`.
    pub fn view_at<W: HeatOwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<CableView> {
        let id = self.st.live(pos)?;
        self.st.slots[id.0 as usize].tc.as_ref()?;
        Some(view_of(
            &View {
                st: &self.st,
                world,
            },
            id,
        ))
    }

    /// `handleHeat(amount, transaction)` numa transação raiz que confirma em seguida, como um aquecedor põe calor num condutor: o capacitor
    /// que mudou chama o `onContentsChanged`.
    pub fn handle_heat_at<W: HeatOwnerWorld>(&mut self, world: &mut W, pos: BlockPos, amount: f64) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        let ambient = AmbientAt {
            world: &*world,
            pos,
        };
        self.st.tc(id).handle_heat(amount, &ambient);
        self.commit_pending(world);
    }

    /// O condutor em `pos` como vizinho de um tile que entrega calor pelo lado `side` dele (o lado do condutor que olha o tile): `None` se a
    /// capability `HEAT` não existe ali (conexão `NONE`, redstone ativado, condutor que não está no mundo).
    pub fn sink_at<'a, W: HeatOwnerWorld>(
        &'a self,
        world: &'a W,
        pos: BlockPos,
        side: Face,
    ) -> Option<ConductorSink<'a, W>> {
        if !world.is_block_loaded(pos) {
            return None;
        }
        let slot = &self.st.slots[self.st.live(pos)?.0 as usize];
        let tc = slot.tc.as_ref()?;
        let view = View {
            st: &self.st,
            world,
        };
        let blocked = tc.transmitter.get_connection_type_raw(side) == ConnectionType::None
            || tc.transmitter.is_redstone_activated(&view);
        (!blocked).then_some(ConductorSink {
            conductor: tc,
            world,
            pos,
        })
    }

    /// O `transaction.commit()` de um tile que cedeu calor a condutores (o Heat Generator no fim do `onUpdateServer`): cada capacitor
    /// que mudou chama o `onContentsChanged` dele.
    pub fn commit_pending<W: HeatOwnerWorld>(&mut self, world: &mut W) {
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        commit_pending(&mut ctx);
    }

    /// `heat_capacitor/state` e o NBT do transmissor (`TileEntityTransmitter.saveAdditional`, `ThermodynamicConductor.write`). `getHeat`
    /// inicializa o calor do condutor que nunca foi lido.
    pub fn write<W: HeatOwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<ConductorTag> {
        let slot = &self.st.slots[self.st.live(pos)?.0 as usize];
        let ambient = AmbientAt { world, pos };
        Some(slot.tc.as_ref()?.write(&ambient))
    }

    /// O `writeReducedUpdatedTag` do condutor em `pos` (o pacote de atualização: as conexões e o calor).
    pub fn update_tag<W: HeatOwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<Compound> {
        let slot = &self.st.slots[self.st.live(pos)?.0 as usize];
        let ambient = AmbientAt { world, pos };
        Some(slot.tc.as_ref()?.write_reduced_updated_tag(&ambient))
    }

    /// Os cliques do Configurator (`TileEntityTransmitter.onSneakRightClick`): `hit_side` é o lado do braço que o olhar toca. Devolve se o
    /// clique foi do condutor.
    pub fn sneak_right_click<W: HeatOwnerWorld>(
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
    pub fn right_click<W: HeatOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) -> bool {
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
    pub fn set_mode<W: HeatOwnerWorld>(
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
