//! `Transmitter` e o que o `TileEntityTransmitter` faz com ele: a conexão por lado de um
//! transmissor, o redstone reativo, o ciclo do Configurator e o NBT.
//!
//! O original mexe na rede, nos vizinhos e no mundo de dentro das próprias funções. Aqui o estado
//! do transmissor fica na struct, o mundo entra por [`TransmitterWorld`] (só consultas) e tudo o
//! que a função faria fora dela, na ordem em que o faria, sai como [`Effect`] numa lista. Quem
//! chama aplica os efeitos em ordem: um `RefreshNeighbor` é uma chamada de verdade ao
//! `refreshConnections(side)` do vizinho, no ponto em que o original a faz.
//!
//! O que o original deixa para as subclasses (`recheckConnections`, `recheckConnection`,
//! `requestsUpdate`, `onConfigure`, `isValidTransmitter`, `handlesRedstone`) é um efeito ou um
//! parâmetro, e a versão da classe base está em `base_*`.

use super::acceptor_cache::AcceptorCache;
use super::connection::{
    ConnectionType, TransmissionSet, connection_map_contains_side, face_ordinal,
    get_connection_type, is_connection_type, set_connection_bit, side_bit,
};
use crate::sides::Face;
use std::cell::Cell;
use std::collections::BTreeMap;

/// A posição de um bloco.
pub type BlockPos = [i32; 3];

/// O UUID de uma rede.
pub type NetworkId = u128;

/// `DEFAULT_REDSTONE_REACTIVE`.
pub const DEFAULT_REDSTONE_REACTIVE: bool = false;
/// `DEFAULT_TRANSMITTER_CONNECTIONS`.
pub const DEFAULT_TRANSMITTER_CONNECTIONS: u8 = 0;

/// As chaves de NBT de `SerializationConstants`.
pub mod keys {
    pub const CURRENT_ACCEPTORS: &str = "acceptors";
    pub const CURRENT_CONNECTIONS: &str = "connections";
    pub const NETWORK: &str = "network";
    pub const CONNECTION: &str = "connection";
    pub const REDSTONE: &str = "redstone";
}

/// `blockPos.relative(side)`.
pub fn relative(pos: BlockPos, side: Face) -> BlockPos {
    let offset = side.offset();
    [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]]
}

/// O que um transmissor vê do transmissor vizinho.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeighborTransmitter {
    /// `getSupportedTransmissionTypes` do vizinho.
    pub supported: TransmissionSet,
    /// `canConnect(side.getOpposite())` do vizinho: se ele aceita a conexão por esta face.
    pub can_connect_opposite: bool,
}

/// O mundo que o transmissor consulta. Só leitura: o que o transmissor muda sai como [`Effect`].
pub trait TransmitterWorld {
    /// O que a capability do transmissor guarda (`ACCEPTOR`).
    type Acceptor: Clone;

    /// `WorldUtils.isBlockLoaded(level, pos)`.
    fn is_block_loaded(&self, pos: BlockPos) -> bool;

    /// `WorldUtils.isGettingPowered(level, pos)`.
    fn is_getting_powered(&self, pos: BlockPos) -> bool;

    /// O transmissor em `pos` (`WorldUtils.getTileEntity(TileEntityTransmitter.class, ...)`, que
    /// dá nulo em chunk não carregado), visto de `side`, o lado do transmissor que pergunta: o
    /// `can_connect_opposite` já é o `canConnect(side.getOpposite())` dele.
    fn transmitter_at(&self, pos: BlockPos, side: Face) -> Option<NeighborTransmitter>;

    /// A capability em `pos` vista pelo lado `from_side` (`BlockCapabilityCache.getCapability`).
    fn acceptor(&self, pos: BlockPos, from_side: Face) -> Option<Self::Acceptor>;

    /// A parte do `isValidTransmitter` que as subclasses acrescentam à regra básica (a de
    /// `BufferedTransmitter`: órfão contra rede; a do cano: o conteúdo). A base não acrescenta nada.
    fn extra_valid_transmitter(&self, _me: BlockPos, _other: BlockPos, _side: Face) -> bool {
        true
    }
}

/// `InteractionResult`, no que importa aqui.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionResult {
    Success,
    Fail,
    Pass,
    TryEmptyHandInteraction,
}

impl InteractionResult {
    /// `consumesAction()`.
    pub fn consumes_action(self) -> bool {
        self == InteractionResult::Success
    }
}

/// As mensagens de `sendOverlayMessage` que o transmissor manda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    /// `MekanismLang.REDSTONE_SENSITIVITY` (`configurator.<mod>.redstone_sensitivity`), cinza, com o
    /// estado em índigo.
    RedstoneSensitivity { on: bool },
    /// `MekanismLang.CONNECTION_TYPE` (`configurator.<mod>.mode_change`), cinza, com o tipo cru
    /// com a cor dele.
    ConnectionType { connection: ConnectionType },
}

/// O que o transmissor pede ao mundo e à rede, na ordem em que o original o faz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// `TileEntityTransmitter.sendUpdatePacket`.
    SendUpdatePacket,
    /// `notifyTileChange` (`WorldUtils.notifyLoadedNeighborsOfTileChange`): cada vizinho carregado
    /// recebe `neighborChanged` sem orientação.
    NotifyTileChange,
    /// `TransmitterNetworkRegistry.invalidateTransmitter`.
    InvalidateTransmitter,
    /// `network.acceptorChanged(transmitter, side)`.
    AcceptorChanged(Face),
    /// `neighbor.getTransmitter().refreshConnections(side)`; `side` já é o lado do vizinho.
    RefreshNeighbor { pos: BlockPos, side: Face },
    /// `recheckConnections(newlyEnabled)`: virtual. A base é [`Transmitter::base_recheck_connections`].
    RecheckConnections(u8),
    /// `recheckConnection(side)`: virtual, a base não faz nada.
    RecheckConnection(Face),
    /// `requestsUpdate()`: virtual. A base é [`Transmitter::base_requests_update`].
    RequestsUpdate,
    /// `TileEntityTransmitter.setForceUpdate`.
    SetForceUpdate,
    /// `BlockEntity.setChanged`.
    SetChanged,
    /// `TileEntityTransmitter.sideChanged(side, old, type)`.
    SideChanged {
        side: Face,
        old: ConnectionType,
        new: ConnectionType,
    },
    /// `TileEntityTransmitter.redstoneChanged(powered)`.
    RedstoneChanged(bool),
    /// No cliente, a rede que o transmissor deixa (`theNetwork.removeTransmitter`).
    ClientNetworkRemove(NetworkId),
    /// No cliente, a rede que o transmissor passa a ter (`theNetwork.addTransmitter`).
    ClientNetworkAdd(NetworkId),
    /// `player.sendOverlayMessage`.
    Overlay(Overlay),
}

/// Uma etiqueta do NBT, no que as chaves do transmissor usam.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    IntArray(Vec<i32>),
}

impl Tag {
    /// `NbtOps.getNumberValue(...).byteValue()` (o `Number.byteValue` do Java: corta para o `int` e
    /// depois para o `byte`).
    pub fn byte_value(&self) -> Option<i8> {
        Some(match self {
            Tag::Byte(v) => *v,
            Tag::Short(v) => *v as i8,
            Tag::Int(v) => *v as i8,
            Tag::Long(v) => *v as i8,
            Tag::Float(v) => (*v as i32) as i8,
            Tag::Double(v) => (*v as i32) as i8,
            Tag::String(_) | Tag::IntArray(_) => return None,
        })
    }

    /// `NbtOps.getNumberValue(...).intValue()`.
    pub fn int_value(&self) -> Option<i32> {
        Some(match self {
            Tag::Byte(v) => *v as i32,
            Tag::Short(v) => *v as i32,
            Tag::Int(v) => *v,
            Tag::Long(v) => *v as i32,
            Tag::Float(v) => *v as i32,
            Tag::Double(v) => *v as i32,
            Tag::String(_) | Tag::IntArray(_) => return None,
        })
    }
}

/// Um `CompoundTag`.
pub type Compound = BTreeMap<String, Tag>;

fn uuid_to_int_array(id: NetworkId) -> Vec<i32> {
    vec![
        (id >> 96) as i32,
        (id >> 64) as i32,
        (id >> 32) as i32,
        id as i32,
    ]
}

fn uuid_from_int_array(array: &[i32]) -> Option<NetworkId> {
    if array.len() != 4 {
        return None;
    }
    Some(
        array
            .iter()
            .fold(0u128, |id, part| (id << 32) | u128::from(*part as u32)),
    )
}

/// O que `handleUpdateTag` pede à rede do cliente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateTagNetwork {
    /// O transmissor já está na rede do pacote, nada a fazer (o `return refreshModelData` cedo).
    Unchanged,
    /// A rede do pacote: o chamador procura a do cliente (`getClientNetwork`) ou cria uma vazia
    /// com este id, registra, e chama [`Transmitter::set_transmitter_network`] com ela.
    Attach(NetworkId),
    /// Sem rede no pacote: o transmissor já ficou sem rede.
    Detached,
}

/// O resultado de `handleUpdateTag`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateTag {
    /// "Se os dados do modelo mudaram": o valor que o original devolve.
    pub refresh_model_data: bool,
    pub network: UpdateTagNetwork,
}

/// `Transmitter` mais o estado de conexão do `TileEntityTransmitter` que o acompanha.
#[derive(Clone, Debug)]
pub struct Transmitter<A: Clone> {
    pos: BlockPos,
    connection_types: [ConnectionType; 6],
    has_pull_side: bool,
    /// `currentTransmitterConnections`.
    pub current_transmitter_connections: u8,
    /// O `acceptorCache`, com o `currentAcceptorConnections` dentro.
    pub acceptor_cache: AcceptorCache<A>,
    supported: TransmissionSet,
    handles_redstone: bool,
    /// `redstoneReactive`.
    pub redstone_reactive: bool,
    redstone_powered: Cell<bool>,
    redstone_set: Cell<bool>,
    network: Option<NetworkId>,
    orphaned: bool,
    is_upgrading: bool,
    /// O tile tem nível (`transmitterTile.hasLevel()`).
    has_level: bool,
    /// O nível é do cliente. Sem nível conta como cliente (`level == null || level.isClientSide()`).
    client_side: bool,
    /// `TileEntityTransmitter.forceUpdate`, que nasce verdadeiro.
    force_update: bool,
    /// `isRemoved()` do tile.
    pub removed: bool,
    /// `TileEntityTransmitter.loaded`.
    pub loaded: bool,
}

impl<A: Clone> Transmitter<A> {
    /// Um transmissor recém-criado, sem nível. `handles_redstone` é o `handlesRedstone()` da
    /// classe (verdadeiro, menos nos transportadores).
    pub fn new(pos: BlockPos, supported: TransmissionSet, handles_redstone: bool) -> Self {
        Transmitter {
            pos,
            connection_types: [ConnectionType::Normal; 6],
            has_pull_side: false,
            current_transmitter_connections: DEFAULT_TRANSMITTER_CONNECTIONS,
            acceptor_cache: AcceptorCache::new(),
            supported,
            handles_redstone,
            redstone_reactive: DEFAULT_REDSTONE_REACTIVE,
            redstone_powered: Cell::new(false),
            redstone_set: Cell::new(false),
            network: None,
            orphaned: true,
            is_upgrading: false,
            has_level: false,
            client_side: true,
            force_update: true,
            removed: false,
            loaded: false,
        }
    }

    /// `setLevel(level)`: o tile ganha nível. No servidor cria o cache de aceitadores.
    pub fn set_level(&mut self, client_side: bool) {
        self.has_level = true;
        self.client_side = client_side;
        if !client_side {
            self.acceptor_cache.initialize_cache();
        }
    }

    pub fn pos(&self) -> BlockPos {
        self.pos
    }

    pub fn is_client_side(&self) -> bool {
        self.client_side
    }

    pub fn is_upgrading(&self) -> bool {
        self.is_upgrading
    }

    pub fn set_upgrading(&mut self, upgrading: bool) {
        self.is_upgrading = upgrading;
    }

    /// `hasPullSide`.
    pub fn has_pull_side(&self) -> bool {
        self.has_pull_side
    }

    /// `redstonePowered`, sem recalcular.
    pub fn redstone_powered(&self) -> bool {
        self.redstone_powered.get()
    }

    /// `redstoneSet`.
    pub fn redstone_set(&self) -> bool {
        self.redstone_set.get()
    }

    /// `forceUpdate` do tile.
    pub fn force_update(&self) -> bool {
        self.force_update
    }

    pub fn set_force_update(&mut self) {
        self.force_update = true;
    }

    /// `isValid()`: `!tile.isRemoved() && tile.isLoaded()`.
    pub fn is_valid(&self) -> bool {
        !self.removed && self.loaded
    }

    // ---------------------------------------------------------------------------------------
    // Tipos de conexão crus
    // ---------------------------------------------------------------------------------------

    /// `getConnectionTypesRaw`.
    pub fn connection_types_raw(&self) -> &[ConnectionType; 6] {
        &self.connection_types
    }

    /// `setConnectionTypesRaw`: troca o vetor inteiro e recalcula o `hasPullSide`.
    pub fn set_connection_types_raw(&mut self, types: [ConnectionType; 6]) {
        self.connection_types = types;
        self.has_pull_side = types.contains(&ConnectionType::Pull);
    }

    /// `getConnectionTypeRaw(side)`.
    pub fn get_connection_type_raw(&self, side: Face) -> ConnectionType {
        self.connection_types[face_ordinal(side)]
    }

    /// `setConnectionTypeRaw(side, type)`: só mexe, e só avisa o tile (`sideChanged`), se o tipo mudou.
    pub fn set_connection_type_raw(
        &mut self,
        side: Face,
        kind: ConnectionType,
        out: &mut Vec<Effect>,
    ) {
        let index = face_ordinal(side);
        let old = self.connection_types[index];
        if old != kind {
            self.connection_types[index] = kind;
            if kind == ConnectionType::Pull {
                self.has_pull_side = true;
            } else if self.has_pull_side {
                self.has_pull_side = self.connection_types.contains(&ConnectionType::Pull);
            }
            out.push(Effect::SideChanged {
                side,
                old,
                new: kind,
            });
        }
    }

    // ---------------------------------------------------------------------------------------
    // Rede
    // ---------------------------------------------------------------------------------------

    /// `getTransmitterNetwork`, o id da rede.
    pub fn transmitter_network(&self) -> Option<NetworkId> {
        self.network
    }

    /// `setTransmitterNetwork(network, requestNow)`: devolve se ainda falta pedir a atualização
    /// (`requestNow` falso no servidor).
    pub fn set_transmitter_network(
        &mut self,
        network: Option<NetworkId>,
        request_now: bool,
        out: &mut Vec<Effect>,
    ) -> bool {
        if self.network == network {
            return false;
        }
        if self.client_side
            && let Some(old) = self.network
        {
            out.push(Effect::ClientNetworkRemove(old));
        }
        self.network = network;
        self.orphaned = network.is_none();
        if self.client_side {
            if let Some(new) = self.network {
                out.push(Effect::ClientNetworkAdd(new));
            }
        } else if request_now {
            out.push(Effect::RequestsUpdate);
        } else {
            return true;
        }
        false
    }

    /// `hasTransmitterNetwork`: `!isOrphan() && getTransmitterNetwork() != null`.
    pub fn has_transmitter_network(&self) -> bool {
        !self.is_orphan() && self.network.is_some()
    }

    pub fn is_orphan(&self) -> bool {
        self.orphaned
    }

    pub fn set_orphan(&mut self, now_orphaned: bool) {
        self.orphaned = now_orphaned;
    }

    pub fn supported_transmission_types(&self) -> TransmissionSet {
        self.supported
    }

    /// `supportsTransmissionType(other)`.
    pub fn supports_transmission_type(&self, other: TransmissionSet) -> bool {
        other.intersects(self.supported)
    }

    /// `getAcceptor(side)`: o aceitador do lado, se o mapa de conexões o tem.
    pub fn get_acceptor<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        world: &W,
    ) -> Option<A> {
        self.acceptor_cache.get_cached_acceptor(side, &|s| {
            world.acceptor(relative(self.pos, s), s.opposite())
        })
    }

    // ---------------------------------------------------------------------------------------
    // Redstone
    // ---------------------------------------------------------------------------------------

    /// `handlesRedstone()`.
    pub fn handles_redstone(&self) -> bool {
        self.handles_redstone
    }

    /// `setRedstoneState`: assume `handlesRedstone()`.
    fn set_redstone_state<W: TransmitterWorld<Acceptor = A>>(&self, world: &W) {
        self.redstone_powered
            .set(self.redstone_reactive && self.has_level && world.is_getting_powered(self.pos));
        self.redstone_set.set(true);
    }

    /// `isRedstoneActivated()`.
    pub fn is_redstone_activated<W: TransmitterWorld<Acceptor = A>>(&self, world: &W) -> bool {
        if self.handles_redstone && self.redstone_reactive {
            if !self.redstone_set.get() {
                self.set_redstone_state(world);
            }
            return self.redstone_powered.get();
        }
        false
    }

    /// `recheckRedstone`: relê o sinal e, se mudou, invalida o transmissor e avisa o tile.
    fn recheck_redstone<W: TransmitterWorld<Acceptor = A>>(
        &self,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.handles_redstone {
            let previously_powered = self.redstone_powered.get();
            self.set_redstone_state(world);
            if previously_powered != self.redstone_powered.get() {
                self.mark_dirty_transmitters(out);
                out.push(Effect::RedstoneChanged(self.redstone_powered.get()));
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Conexões possíveis
    // ---------------------------------------------------------------------------------------

    /// `getPossibleTransmitterConnections`.
    pub fn get_possible_transmitter_connections<W: TransmitterWorld<Acceptor = A>>(
        &self,
        world: &W,
    ) -> u8 {
        let mut connections = 0u8;
        if self.is_redstone_activated(world) {
            return connections;
        }
        for side in Face::ALL {
            let pos = relative(self.pos, side);
            if let Some(tile) = world.transmitter_at(pos, side)
                && self.is_valid_transmitter(&tile, pos, side, world)
            {
                connections |= side_bit(side);
            }
        }
        connections
    }

    /// `getPossibleAcceptorConnection(side, markDirty)`.
    fn get_possible_acceptor_connection<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        mark_dirty: bool,
        world: &W,
        out: &mut Vec<Effect>,
    ) -> bool {
        if self.is_redstone_activated(world) {
            return false;
        }
        let tile = world.transmitter_at(relative(self.pos, side), side);
        if self.can_connect_mutual(side, tile.as_ref(), world)
            && self.is_valid_acceptor(tile.as_ref(), side, world)
        {
            return true;
        }
        if mark_dirty {
            self.mark_dirty_acceptor(side, out);
        }
        false
    }

    /// `getPossibleTransmitterConnection(side)`.
    fn get_possible_transmitter_connection<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        world: &W,
    ) -> bool {
        if self.is_redstone_activated(world) {
            return false;
        }
        let pos = relative(self.pos, side);
        match world.transmitter_at(pos, side) {
            Some(tile) => self.is_valid_transmitter(&tile, pos, side, world),
            None => false,
        }
    }

    /// `getPossibleAcceptorConnections`.
    pub fn get_possible_acceptor_connections<W: TransmitterWorld<Acceptor = A>>(
        &self,
        world: &W,
        out: &mut Vec<Effect>,
    ) -> u8 {
        let mut connections = 0u8;
        if self.is_redstone_activated(world) {
            return connections;
        }
        for side in Face::ALL {
            let pos = relative(self.pos, side);
            let tile = world.transmitter_at(pos, side);
            if self.can_connect_mutual(side, tile.as_ref(), world) {
                if !self.client_side && !world.is_block_loaded(pos) {
                    out.push(Effect::SetForceUpdate);
                    continue;
                }
                if self.is_valid_acceptor(tile.as_ref(), side, world) {
                    connections |= side_bit(side);
                    continue;
                }
            }
            self.mark_dirty_acceptor(side, out);
        }
        connections
    }

    /// `getAllCurrentConnections`.
    pub fn get_all_current_connections(&self) -> u8 {
        self.current_transmitter_connections | self.acceptor_cache.current_acceptor_connections
    }

    /// `isValidTransmitter(transmitter, side)`: a regra básica mais o que as subclasses acrescentam.
    pub fn is_valid_transmitter<W: TransmitterWorld<Acceptor = A>>(
        &self,
        other: &NeighborTransmitter,
        other_pos: BlockPos,
        side: Face,
        world: &W,
    ) -> bool {
        self.is_valid_transmitter_basic(other, side, world)
            && world.extra_valid_transmitter(self.pos, other_pos, side)
    }

    /// `isValidTransmitterBasic`: tipos em comum e conexão dos dois lados.
    pub fn is_valid_transmitter_basic<W: TransmitterWorld<Acceptor = A>>(
        &self,
        other: &NeighborTransmitter,
        side: Face,
        world: &W,
    ) -> bool {
        self.supports_transmission_type(other.supported)
            && self.can_connect_mutual(side, Some(other), world)
    }

    /// `canConnectToAcceptor(side)`.
    pub fn can_connect_to_acceptor(&self, side: Face) -> bool {
        self.get_connection_type_raw(side).can_send_to()
    }

    /// `isValidAcceptor(tile, side)`: um vizinho que não é transmissor do mesmo tipo vale se a
    /// capability dele existe.
    fn is_valid_acceptor<W: TransmitterWorld<Acceptor = A>>(
        &self,
        tile: Option<&NeighborTransmitter>,
        side: Face,
        world: &W,
    ) -> bool {
        let is_transmitter_of_ours =
            tile.is_some_and(|t| self.supports_transmission_type(t.supported));
        if !is_transmitter_of_ours {
            return self
                .acceptor_cache
                .get_connected_acceptor(side, &|s| {
                    world.acceptor(relative(self.pos, s), s.opposite())
                })
                .is_some();
        }
        false
    }

    /// `canConnectMutual(side, tile)`.
    pub fn can_connect_mutual<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        tile: Option<&NeighborTransmitter>,
        world: &W,
    ) -> bool {
        if !self.can_connect(side, world) {
            return false;
        }
        tile.is_none_or(|t| t.can_connect_opposite)
    }

    /// `canConnect(side)`.
    pub fn can_connect<W: TransmitterWorld<Acceptor = A>>(&self, side: Face, world: &W) -> bool {
        if self.get_connection_type_raw(side) == ConnectionType::None {
            return false;
        }
        if self.handles_redstone && self.redstone_reactive {
            if !self.redstone_set.get() {
                self.set_redstone_state(world);
            }
            return !self.redstone_powered.get();
        }
        true
    }

    // ---------------------------------------------------------------------------------------
    // Pedidos à rede
    // ---------------------------------------------------------------------------------------

    /// `requestsUpdate()` da base: manda o pacote de atualização.
    pub fn base_requests_update(out: &mut Vec<Effect>) {
        out.push(Effect::SendUpdatePacket);
    }

    /// `markDirtyTransmitters`: avisa os vizinhos e, com rede, invalida o transmissor.
    pub fn mark_dirty_transmitters(&self, out: &mut Vec<Effect>) {
        out.push(Effect::NotifyTileChange);
        if self.has_transmitter_network() {
            out.push(Effect::InvalidateTransmitter);
        }
    }

    /// `markDirtyAcceptor(side)`.
    pub fn mark_dirty_acceptor(&self, side: Face, out: &mut Vec<Effect>) {
        if self.has_transmitter_network() {
            out.push(Effect::AcceptorChanged(side));
        }
    }

    /// `notifyTileChange`.
    pub fn notify_tile_change(&self, out: &mut Vec<Effect>) {
        out.push(Effect::NotifyTileChange);
    }

    // ---------------------------------------------------------------------------------------
    // Atualização das conexões
    // ---------------------------------------------------------------------------------------

    /// `refreshConnections()`: relê todas as conexões (só no servidor).
    pub fn refresh_connections<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.client_side {
            return;
        }
        self.recheck_redstone(world, out);
        let possible_transmitters = self.get_possible_transmitter_connections(world);
        let possible_acceptors = self.get_possible_acceptor_connections(world, out);
        let mut newly_enabled_transmitters = 0u8;
        let mut send_desc = false;
        if (possible_transmitters | possible_acceptors) != self.get_all_current_connections() {
            send_desc = true;
            if possible_transmitters != self.current_transmitter_connections {
                newly_enabled_transmitters =
                    possible_transmitters ^ self.current_transmitter_connections;
                newly_enabled_transmitters &= !self.current_transmitter_connections;
            }
        }
        self.current_transmitter_connections = possible_transmitters;
        self.acceptor_cache.current_acceptor_connections = possible_acceptors;
        if newly_enabled_transmitters != 0 {
            out.push(Effect::RecheckConnections(newly_enabled_transmitters));
        }
        if send_desc {
            out.push(Effect::SendUpdatePacket);
        }
    }

    /// `refreshAcceptorConnections(side)`: o que o cache de aceitadores da rede pede antes de
    /// consultar o aceitador de um lado.
    pub fn refresh_acceptor_connections<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        side: Face,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.client_side {
            return;
        }
        let possible_acceptor = self.get_possible_acceptor_connection(side, false, world, out);
        if possible_acceptor
            != connection_map_contains_side(self.acceptor_cache.current_acceptor_connections, side)
        {
            self.acceptor_cache.current_acceptor_connections = set_connection_bit(
                self.acceptor_cache.current_acceptor_connections,
                possible_acceptor,
                side,
            );
            out.push(Effect::SendUpdatePacket);
        }
    }

    /// `refreshConnections(side)`: relê só um lado.
    pub fn refresh_connections_side<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        side: Face,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.client_side {
            return;
        }
        let possible_transmitter = self.get_possible_transmitter_connection(side, world);
        let possible_acceptor = self.get_possible_acceptor_connection(side, true, world, out);
        let mut transmitter_changed = false;
        let mut send_desc = false;
        if (possible_transmitter || possible_acceptor)
            != connection_map_contains_side(self.get_all_current_connections(), side)
        {
            send_desc = true;
            if possible_transmitter
                != connection_map_contains_side(self.current_transmitter_connections, side)
            {
                transmitter_changed = possible_transmitter;
            }
        }
        self.current_transmitter_connections = set_connection_bit(
            self.current_transmitter_connections,
            possible_transmitter,
            side,
        );
        self.acceptor_cache.current_acceptor_connections = set_connection_bit(
            self.acceptor_cache.current_acceptor_connections,
            possible_acceptor,
            side,
        );
        if transmitter_changed {
            out.push(Effect::RecheckConnection(side));
        }
        if send_desc {
            out.push(Effect::SendUpdatePacket);
        }
    }

    /// `recheckConnections(newlyEnabledTransmitters)` da base: sem rede, cada vizinho que acabou
    /// de virar transmissor relê o lado de volta (o que conserta cano que não reconecta entre
    /// chunks).
    pub fn base_recheck_connections<W: TransmitterWorld<Acceptor = A>>(
        &self,
        newly_enabled_transmitters: u8,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if !self.has_transmitter_network() {
            for side in Face::ALL {
                if connection_map_contains_side(newly_enabled_transmitters, side) {
                    let pos = relative(self.pos, side);
                    if world.transmitter_at(pos, side).is_some() {
                        out.push(Effect::RefreshNeighbor {
                            pos,
                            side: side.opposite(),
                        });
                    }
                }
            }
        }
    }

    /// `onModeChange(side)`.
    pub fn on_mode_change<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        self.mark_dirty_acceptor(side, out);
        if self.get_possible_transmitter_connections(world) != self.current_transmitter_connections
        {
            self.mark_dirty_transmitters(out);
        }
        out.push(Effect::SetChanged);
    }

    /// `onNeighborBlockChange(side)`: com redstone reativo relê tudo; senão só o lado que mudou
    /// (e tudo, se não se sabe qual).
    pub fn on_neighbor_block_change<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        side: Option<Face>,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.handles_redstone && self.redstone_reactive {
            self.refresh_connections(world, out);
        } else {
            match side {
                None => self.refresh_connections(world, out),
                Some(side) => self.refresh_connections_side(side, world, out),
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Tipo de conexão visto
    // ---------------------------------------------------------------------------------------

    /// `getConnectionType(side)`: o que o lado mostra e o que a forma e o modelo usam.
    pub fn get_connection_type(&self, side: Face) -> ConnectionType {
        get_connection_type(
            side,
            self.get_all_current_connections(),
            self.current_transmitter_connections,
            &self.connection_types,
        )
    }

    /// `isConnectionType(side, type)`.
    pub fn is_connection_type(&self, side: Face, type_to_check: ConnectionType) -> bool {
        is_connection_type(
            side,
            self.get_all_current_connections(),
            self.current_transmitter_connections,
            &self.connection_types,
            type_to_check,
        )
    }

    // ---------------------------------------------------------------------------------------
    // Cliques
    // ---------------------------------------------------------------------------------------

    /// `onConfigure(player, side)` da base.
    pub fn base_on_configure(&self) -> InteractionResult {
        InteractionResult::Pass
    }

    /// `onRightClick(level, player, side)` da base: liga e desliga o redstone reativo.
    pub fn on_right_click<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        world: &W,
        out: &mut Vec<Effect>,
    ) -> InteractionResult {
        if self.handles_redstone {
            self.redstone_reactive = !self.redstone_reactive;
            self.refresh_connections(world, out);
            self.notify_tile_change(out);
            out.push(Effect::Overlay(Overlay::RedstoneSensitivity {
                on: self.redstone_reactive,
            }));
        }
        InteractionResult::Success
    }

    /// `TileEntityTransmitter.getSideLookingAt(player)`: o lado do braço que o olhar toca. `box_index`
    /// é a posição da caixa tocada na lista de `getCollisionBoxes` (os braços na ordem dos lados e o
    /// centro por último); `None` é o olhar fora das caixas. Só os braços dão um lado.
    pub fn side_looking_at(&self, box_index: Option<usize>) -> Option<Face> {
        let connections = self.get_all_current_connections();
        let sides: Vec<Face> = Face::ALL
            .into_iter()
            .filter(|side| connection_map_contains_side(connections, *side))
            .collect();
        box_index.and_then(|index| sides.get(index).copied())
    }

    /// `TileEntityTransmitter.onSneakRightClick(level, player, side)`: o clique do Configurator
    /// agachado. `hit_side` é o lado do olhar (`getSideLookingAt`); `on_configure` a
    /// `onConfigure` da subclasse (a da base é [`Transmitter::base_on_configure`]).
    pub fn on_sneak_right_click<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        side: Face,
        hit_side: Option<Face>,
        on_configure: impl FnOnce(&mut Self, Face) -> InteractionResult,
        world: &W,
        out: &mut Vec<Effect>,
    ) -> InteractionResult {
        if self.client_side {
            return InteractionResult::Success;
        }
        let hit_side = match hit_side {
            Some(hit) => hit,
            None => {
                if self.get_connection_type_raw(side) != ConnectionType::None {
                    let result = on_configure(self, side);
                    if result.consumes_action() {
                        self.refresh_connections(world, out);
                        self.notify_tile_change(out);
                        return result;
                    }
                }
                side
            }
        };
        let next = self.get_connection_type_raw(hit_side).get_next();
        self.set_connection_type_raw(hit_side, next, out);
        self.on_mode_change(hit_side, world, out);
        self.refresh_connections(world, out);
        self.notify_tile_change(out);
        out.push(Effect::Overlay(Overlay::ConnectionType {
            connection: self.get_connection_type_raw(hit_side),
        }));
        out.push(Effect::SendUpdatePacket);
        InteractionResult::Success
    }

    /// `TileEntityTransmitter.canExtract(side)`: sem lado, `true` (a extração é bloqueada por
    /// quem lê); com lado, só se o tipo visto deixa mandar.
    pub fn can_extract(&self, side: Option<Face>) -> bool {
        match side {
            None => true,
            Some(side) => self.get_connection_type(side).can_send_to(),
        }
    }

    /// `TileEntityTransmitter.canInsert(side)`.
    pub fn can_insert(&self, side: Option<Face>) -> bool {
        match side {
            None => true,
            Some(side) => self.get_connection_type(side).can_accept(),
        }
    }

    /// `TileEntityTransmitter.onUpdateServer`, a parte das conexões: com `forceUpdate`, relê tudo
    /// uma vez.
    pub fn on_update_server<W: TransmitterWorld<Acceptor = A>>(
        &mut self,
        world: &W,
        out: &mut Vec<Effect>,
    ) {
        if self.force_update {
            self.refresh_connections(world, out);
            self.force_update = false;
        }
    }

    // ---------------------------------------------------------------------------------------
    // NBT
    // ---------------------------------------------------------------------------------------

    fn raw_connections(&self) -> Vec<i32> {
        Face::ALL
            .iter()
            .map(|side| self.get_connection_type_raw(*side).ordinal() as i32)
            .collect()
    }

    /// `write(output)`.
    pub fn write(&self) -> Compound {
        let mut tag = Compound::new();
        tag.insert(
            keys::REDSTONE.into(),
            Tag::Byte(i8::from(self.redstone_reactive)),
        );
        tag.insert(
            keys::CURRENT_CONNECTIONS.into(),
            Tag::Byte(self.current_transmitter_connections as i8),
        );
        tag.insert(
            keys::CURRENT_ACCEPTORS.into(),
            Tag::Byte(self.acceptor_cache.current_acceptor_connections as i8),
        );
        tag.insert(
            keys::CONNECTION.into(),
            Tag::IntArray(self.raw_connections()),
        );
        tag
    }

    /// `writeReducedUpdatedTag(output)`: o que vai no pacote de atualização (sem o `redstone`, com
    /// a rede, se há).
    pub fn write_reduced_updated_tag(&self) -> Compound {
        let mut tag = Compound::new();
        tag.insert(
            keys::CURRENT_CONNECTIONS.into(),
            Tag::Byte(self.current_transmitter_connections as i8),
        );
        tag.insert(
            keys::CURRENT_ACCEPTORS.into(),
            Tag::Byte(self.acceptor_cache.current_acceptor_connections as i8),
        );
        tag.insert(
            keys::CONNECTION.into(),
            Tag::IntArray(self.raw_connections()),
        );
        if self.has_transmitter_network()
            && let Some(id) = self.network
        {
            tag.insert(keys::NETWORK.into(), Tag::IntArray(uuid_to_int_array(id)));
        }
        tag
    }

    fn byte_or(tag: &Compound, key: &str, default: u8) -> u8 {
        tag.get(key)
            .and_then(Tag::byte_value)
            .map_or(default, |v| v as u8)
    }

    fn boolean_or(tag: &Compound, key: &str, default: bool) -> bool {
        tag.get(key)
            .and_then(Tag::byte_value)
            .map_or(default, |v| v != 0)
    }

    /// `readRawConnections(input)`.
    fn read_raw_connections(&mut self, tag: &Compound, out: &mut Vec<Effect>) {
        let raw: Vec<i32> = match tag.get(keys::CONNECTION) {
            Some(Tag::IntArray(array)) => array.clone(),
            _ => vec![0; 6],
        };
        for (index, side) in Face::ALL.iter().enumerate() {
            if index < raw.len() {
                self.set_connection_type_raw(*side, ConnectionType::by_id(raw[index]), out);
            }
        }
    }

    /// `read(input)`.
    pub fn read(&mut self, tag: &Compound, out: &mut Vec<Effect>) {
        self.redstone_reactive = Self::boolean_or(tag, keys::REDSTONE, DEFAULT_REDSTONE_REACTIVE);
        self.current_transmitter_connections = Self::byte_or(
            tag,
            keys::CURRENT_CONNECTIONS,
            DEFAULT_TRANSMITTER_CONNECTIONS,
        );
        self.acceptor_cache.current_acceptor_connections = Self::byte_or(
            tag,
            keys::CURRENT_ACCEPTORS,
            super::acceptor_cache::DEFAULT_ACCEPTOR_CONNECTIONS,
        );
        self.read_raw_connections(tag, out);
    }

    /// `handleUpdateTag(input)`: o pacote de atualização no cliente. Devolve se os dados do modelo
    /// mudaram e o que fazer com a rede.
    pub fn handle_update_tag(&mut self, tag: &Compound, out: &mut Vec<Effect>) -> UpdateTag {
        let old: Vec<ConnectionType> = Face::ALL
            .iter()
            .map(|side| self.get_connection_type(*side))
            .collect();
        self.current_transmitter_connections = Self::byte_or(
            tag,
            keys::CURRENT_CONNECTIONS,
            DEFAULT_TRANSMITTER_CONNECTIONS,
        );
        self.acceptor_cache.current_acceptor_connections = Self::byte_or(
            tag,
            keys::CURRENT_ACCEPTORS,
            super::acceptor_cache::DEFAULT_ACCEPTOR_CONNECTIONS,
        );
        self.read_raw_connections(tag, out);
        let refresh_model_data = Face::ALL
            .iter()
            .enumerate()
            .any(|(index, side)| self.get_connection_type(*side) != old[index]);
        let network_id = match tag.get(keys::NETWORK) {
            Some(Tag::IntArray(array)) => uuid_from_int_array(array),
            _ => None,
        };
        let network = match network_id {
            Some(id) => {
                if self.has_transmitter_network() && self.network == Some(id) {
                    UpdateTagNetwork::Unchanged
                } else {
                    UpdateTagNetwork::Attach(id)
                }
            }
            None => {
                self.set_transmitter_network(None, true, out);
                UpdateTagNetwork::Detached
            }
        };
        UpdateTag {
            refresh_model_data,
            network,
        }
    }
}
