//! O lado do cliente das redes de transmissores e o pacote de escala (`PacketNetworkScale`).
//!
//! No original o cliente tem um `TransmitterNetworkRegistry.clientNetworks` (UUID para rede), cada `Transmitter` do cliente guarda a
//! `DynamicNetwork` que o servidor lhe disse no pacote de atualização (`Transmitter.handleUpdateTag`), e o servidor manda o
//! `currentScale` de uma rede em buffer (`DynamicBufferedNetwork`) a quem está perto dela (`PacketUtils.sendToAllTracking`). Aqui:
//!
//! - [`Range3D`], [`gen_packet_range`] e [`player_in_range`] são `DynamicBufferedNetwork.getPacketRange`/`genPacketRange` e o teste de
//!   raio de `PacketUtils.sendToAllTracking(network, ...)`;
//! - [`ScalePacket`] é o `PacketNetworkScale` e o seu `STREAM_CODEC` (`UUIDUtil.STREAM_CODEC`, dois `long`, e um `float`);
//! - [`ScaleOutbox`] guarda o `packetRange` de cada rede do servidor (a cache que `validTransmittersAdded` e `deregister` zeram) e os
//!   pacotes a mandar;
//! - [`ClientTransmitters`] é o cliente: os transmissores, as redes do `clientNetworks`, o `handleUpdateTag`, o
//!   `handleContentsUpdateTag`/`updateClientNetwork` de `BufferedTransmitter`, o `PacketNetworkScale.handle` e o
//!   `clearClientNetworks`.
//!
//! O desenho (o conteúdo animado do cano, o brilho do cabo pela escala) é da Fase 4: lê `current_scale`, `energy` e `last_type` de
//! [`ClientNetwork`] por [`ClientTransmitters::network_at`].
use super::connection::{TransmissionSet, TransmissionType};
use super::transmitter::{Compound, Effect, NetworkId, Tag, Transmitter, UpdateTagNetwork};
use super::{DimensionId, NetworkId as ServerNetworkId};
use crate::sim::chunks::BlockPos;
use std::collections::HashMap;

/// `mekanism:network_scale`, o canal do pacote entre o servidor e o cliente.
pub const SCALE_CHANNEL: &str = "machinery_basis:network_scale";
/// O canal do pacote de atualização de um transmissor (`getUpdateTag`/`handleUpdateTag`): a posição e o `writeReducedUpdatedTag`.
pub const TAG_CHANNEL: &str = "machinery_basis:transmitter_tag";
/// O canal da saída de um transmissor do cliente (`setRemoved`, ou o chunk que sai).
pub const GONE_CHANNEL: &str = "machinery_basis:transmitter_gone";

/// `Range3D`: o retângulo (em x e z) que a rede cobre, e a dimensão dela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range3D {
    pub x_min: i32,
    pub z_min: i32,
    pub x_max: i32,
    pub z_max: i32,
    pub dimension: DimensionId,
}

/// `DynamicBufferedNetwork.genPacketRange`: `None` se a rede está vazia (o original ainda chama `deregister`).
pub fn gen_packet_range(
    positions: impl IntoIterator<Item = BlockPos>,
    dimension: DimensionId,
) -> Option<Range3D> {
    let mut range: Option<(i32, i32, i32, i32)> = None;
    for pos in positions {
        range = Some(match range {
            None => (pos.x, pos.z, pos.x, pos.z),
            Some((mut min_x, mut min_z, mut max_x, mut max_z)) => {
                if pos.x < min_x {
                    min_x = pos.x;
                } else if pos.x > max_x {
                    max_x = pos.x;
                }
                if pos.z < min_z {
                    min_z = pos.z;
                } else if pos.z > max_z {
                    max_z = pos.z;
                }
                (min_x, min_z, max_x, max_z)
            }
        });
    }
    range.map(|(x_min, z_min, x_max, z_max)| Range3D {
        x_min,
        z_min,
        x_max,
        z_max,
        dimension,
    })
}

/// O teste de `PacketUtils.sendToAllTracking(network, ...)` para um jogador: a mesma dimensão e o bloco do jogador a menos de
/// `viewDistance * 16` (mais as folgas `1.99999` e `0.99999` do original) do retângulo. `player_x` e `player_z` são os do
/// `blockPosition()`.
pub fn player_in_range(
    range: &Range3D,
    dimension: DimensionId,
    player_x: i32,
    player_z: i32,
    view_distance: i32,
) -> bool {
    if range.dimension != dimension {
        return false;
    }
    let radius = view_distance * 16;
    f64::from(player_x) + f64::from(radius) + 1.99999 > f64::from(range.x_min)
        && f64::from(range.x_max) + 0.99999 > f64::from(player_x) - f64::from(radius)
        && f64::from(player_z) + f64::from(radius) + 1.99999 > f64::from(range.z_min)
        && f64::from(range.z_max) + 0.99999 > f64::from(player_z) - f64::from(radius)
}

/// `PacketNetworkScale(networkID, scale)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalePacket {
    pub network: u128,
    pub scale: f32,
}

impl ScalePacket {
    /// O `STREAM_CODEC`: o UUID como dois `long` em ordem de rede (o mais significativo primeiro) e o `float`.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20);
        bytes.extend_from_slice(&self.network.to_be_bytes());
        bytes.extend_from_slice(&self.scale.to_bits().to_be_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 20 {
            return None;
        }
        let network = u128::from_be_bytes(bytes[..16].try_into().ok()?);
        let scale = f32::from_bits(u32::from_be_bytes(bytes[16..].try_into().ok()?));
        Some(Self { network, scale })
    }
}

/// O que o servidor manda de uma rede: o pacote, a quem (o retângulo) e o conteúdo que o evento de fluido ou de químico leva junto
/// (`PacketFluidNetworkContents`, `PacketChemicalNetworkContents`).
#[derive(Clone, Debug, PartialEq)]
pub struct Outgoing<C> {
    pub packet: ScalePacket,
    pub range: Range3D,
    pub contents: Option<C>,
}

/// O `packetRange` de cada rede do servidor e os pacotes que o `onUpdate` dela mandou.
#[derive(Debug)]
pub struct ScaleOutbox<C> {
    dimension: DimensionId,
    ranges: HashMap<ServerNetworkId, Option<Range3D>>,
    outgoing: Vec<Outgoing<C>>,
}

impl<C> ScaleOutbox<C> {
    pub fn new(dimension: DimensionId) -> Self {
        Self {
            dimension,
            ranges: HashMap::new(),
            outgoing: Vec::new(),
        }
    }

    /// `validTransmittersAdded` e `deregister` zeram a cache (`packetRange = null`).
    pub fn flush_range(&mut self, network: ServerNetworkId) {
        self.ranges.remove(&network);
    }

    /// `PacketUtils.sendToAllTracking(network, packets)`: calcula (ou lê da cache) o retângulo e põe o pacote na fila. Uma rede vazia não manda
    /// nada (`getPacketRange` nulo).
    pub fn send(
        &mut self,
        network: ServerNetworkId,
        uuid: u128,
        scale: f32,
        positions: impl IntoIterator<Item = BlockPos>,
        contents: Option<C>,
    ) {
        let dimension = self.dimension;
        let range = *self
            .ranges
            .entry(network)
            .or_insert_with(|| gen_packet_range(positions, dimension));
        let Some(range) = range else {
            return;
        };
        self.outgoing.push(Outgoing {
            packet: ScalePacket {
                network: uuid,
                scale,
            },
            range,
            contents,
        });
    }

    /// Os pacotes desde a última chamada, na ordem em que saíram.
    pub fn take(&mut self) -> Vec<Outgoing<C>> {
        std::mem::take(&mut self.outgoing)
    }
}

/// O que a rede do cliente guarda além do que o servidor lhe diz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientKind {
    /// `UniversalCable`: `EnergyNetwork`, com a energia (`energyContainer`).
    Energy,
    /// `MechanicalPipe` e `PressurizedTube`: `FluidNetwork` e `ChemicalNetwork`, com o último tipo (`lastType`).
    Resource,
    /// `ThermodynamicConductor`: `HeatNetwork`, sem buffer.
    Heat,
}

/// Uma `DynamicNetwork` do cliente.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientNetwork {
    pub uuid: u128,
    pub kind: ClientKind,
    /// `positionedTransmitters`: a posição e o transmissor (a geração do tile, para a identidade).
    transmitters: HashMap<BlockPos, u64>,
    /// `DynamicBufferedNetwork.currentScale`.
    pub current_scale: f32,
    /// `DynamicBufferedNetwork.capacity` (`updateCapacity`).
    pub capacity: i64,
    /// `EnergyNetwork.energyContainer`, só a quantidade.
    pub energy: i64,
    /// `DynamicBufferedResourceNetwork.lastType`: o id do fluido ou do químico, vazio se não há.
    pub last_type: Option<String>,
}

impl ClientNetwork {
    fn new(uuid: u128, kind: ClientKind) -> Self {
        Self {
            uuid,
            kind,
            transmitters: HashMap::new(),
            current_scale: 0.0,
            capacity: 0,
            energy: 0,
            last_type: None,
        }
    }

    /// `transmittersSize`.
    pub fn transmitters_size(&self) -> usize {
        self.transmitters.len()
    }

    pub fn has_transmitter(&self, pos: BlockPos) -> bool {
        self.transmitters.contains_key(&pos)
    }
}

/// O tile de um transmissor no cliente.
#[derive(Clone, Debug)]
struct ClientTile {
    generation: u64,
    kind: ClientKind,
    transmitter: Transmitter<()>,
    /// `getCapacity()` do transmissor (`CableTier.getCableCapacity`, `PipeTier.getPipeCapacity`, `TubeTier.getTubeCapacity`).
    capacity: i64,
    /// `TileEntityTransmitter.loaded`.
    loaded: bool,
}

/// O que o desenho lê de um transmissor do cliente: as conexões por lado (`getConnectionType`, com `PUSH` e `PULL` que o cliente só
/// sabe pelo pacote de atualização) e a rede.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientView {
    pub kind: [super::connection::ConnectionType; 6],
    pub network: Option<u128>,
}

/// O cliente de uma dimensão: os tiles dos transmissores e o `clientNetworks`.
#[derive(Debug, Default)]
pub struct ClientTransmitters {
    networks: HashMap<u128, ClientNetwork>,
    tiles: HashMap<BlockPos, ClientTile>,
    next_generation: u64,
}

impl ClientTransmitters {
    pub fn new() -> Self {
        Self::default()
    }

    /// `TransmitterNetworkRegistry.getClientNetwork`.
    pub fn get_client_network(&self, uuid: u128) -> Option<&ClientNetwork> {
        self.networks.get(&uuid)
    }

    /// A rede do transmissor em `pos`, para o desenho.
    pub fn network_at(&self, pos: BlockPos) -> Option<&ClientNetwork> {
        let tile = self.tiles.get(&pos)?;
        if !tile.transmitter.has_transmitter_network() {
            return None;
        }
        self.networks.get(&tile.transmitter.transmitter_network()?)
    }

    /// `TransmitterNetworkRegistry.clearClientNetworks` (`MekanismClient`, ao sair do mundo): o `clientNetworks` esvazia; os tiles seguem
    /// com a referência, como no original, e quem sai do mundo depois não encontra mais a rede.
    pub fn clear_client_networks(&mut self) {
        self.networks.clear();
    }

    /// O número de redes do `clientNetworks`.
    pub fn network_count(&self) -> usize {
        self.networks.len()
    }

    /// O que o desenho lê do transmissor em `pos`.
    pub fn view_at(&self, pos: BlockPos) -> Option<ClientView> {
        let tile = self.tiles.get(&pos)?;
        let kind = std::array::from_fn(|index| {
            tile.transmitter
                .get_connection_type(crate::sides::Face::ALL[index])
        });
        Some(ClientView {
            kind,
            network: tile
                .transmitter
                .has_transmitter_network()
                .then(|| tile.transmitter.transmitter_network())
                .flatten(),
        })
    }

    /// O tile do transmissor entra no mundo do cliente (`clearRemoved` no cliente: `onWorldJoin` só marca `loaded`; o tile novo tira o
    /// anterior da mesma posição só quando ele sai).
    pub fn add_tile(&mut self, pos: BlockPos, kind: ClientKind, capacity: i64) {
        let transmission = match kind {
            ClientKind::Energy => TransmissionType::Energy,
            ClientKind::Resource => TransmissionType::Fluid,
            ClientKind::Heat => TransmissionType::Heat,
        };
        let mut transmitter = Transmitter::new(
            [pos.x, pos.y, pos.z],
            TransmissionSet::of(&[transmission]),
            true,
        );
        transmitter.set_level(true);
        self.next_generation += 1;
        let tile = ClientTile {
            generation: self.next_generation,
            kind,
            transmitter,
            capacity,
            loaded: true,
        };
        self.tiles.insert(pos, tile);
    }

    /// `TileEntityTransmitter.setRemoved` no cliente (`onWorldSeparate`): se estava carregado, `setTransmitterNetwork(null)`.
    pub fn remove_tile(&mut self, pos: BlockPos) {
        let Some(mut tile) = self.tiles.remove(&pos) else {
            return;
        };
        if tile.loaded {
            tile.loaded = false;
            let mut effects = Vec::new();
            tile.transmitter
                .set_transmitter_network(None, true, &mut effects);
            self.apply_effects(pos, tile.generation, tile.kind, &effects);
        }
    }

    /// `Transmitter.handleUpdateTag` e o que `TileEntityTransmitter.handleUpdateTag` faz com ele. Devolve se os dados do modelo mudaram.
    pub fn handle_update_tag(&mut self, pos: BlockPos, tag: &Compound) -> bool {
        let Some(mut tile) = self.tiles.remove(&pos) else {
            return false;
        };
        let mut effects = Vec::new();
        let update = tile.transmitter.handle_update_tag(tag, &mut effects);
        let generation = tile.generation;
        let kind = tile.kind;
        self.apply_effects(pos, generation, kind, &effects);
        effects.clear();
        match update.network {
            UpdateTagNetwork::Unchanged => {}
            UpdateTagNetwork::Detached => {}
            UpdateTagNetwork::Attach(id) => {
                if !self.networks.contains_key(&id) {
                    // `createEmptyNetworkWithID`, `register`, `setTransmitterNetwork` e `handleContentsUpdateTag`.
                    self.networks.insert(id, ClientNetwork::new(id, kind));
                    tile.transmitter
                        .set_transmitter_network(Some(id), true, &mut effects);
                    self.apply_effects(pos, generation, kind, &effects);
                    self.handle_contents_update_tag(id, tag);
                    self.update_capacity(id, &tile, pos);
                } else {
                    // `updateClientNetwork`: `register` (já está), `setTransmitterNetwork`, e `updateCapacity` nos buffers.
                    tile.transmitter
                        .set_transmitter_network(Some(id), true, &mut effects);
                    self.apply_effects(pos, generation, kind, &effects);
                    self.update_capacity(id, &tile, pos);
                }
            }
        }
        self.tiles.insert(pos, tile);
        update.refresh_model_data
    }

    /// `PacketNetworkScale.handle`: a rede do cliente, se há e é em buffer, ganha a escala.
    pub fn on_scale_packet(&mut self, packet: &ScalePacket) {
        if let Some(network) = self.networks.get_mut(&packet.network)
            && network.kind != ClientKind::Heat
        {
            network.current_scale = packet.scale;
        }
    }

    /// `PacketFluidNetworkContents.handle` e `PacketChemicalNetworkContents.handle`: `setLastType`.
    pub fn on_contents_packet(&mut self, network: u128, resource: Option<String>) {
        if let Some(network) = self.networks.get_mut(&network)
            && network.kind == ClientKind::Resource
        {
            network.last_type = resource;
        }
    }

    /// `handleContentsUpdateTag` do `UniversalCable` e do `BufferedResourceTransmitter`, sobre a rede nova.
    fn handle_contents_update_tag(&mut self, network: u128, tag: &Compound) {
        let Some(net) = self.networks.get_mut(&network) else {
            return;
        };
        let float = |key: &str| match tag.get(key) {
            Some(Tag::Float(value)) => Some(*value),
            Some(Tag::Double(value)) => Some(*value as f32),
            _ => None,
        };
        match net.kind {
            ClientKind::Energy => {
                net.energy = match tag.get("energy") {
                    Some(Tag::Long(value)) => *value,
                    _ => 0,
                };
                net.current_scale = float("scale").unwrap_or(net.current_scale);
            }
            ClientKind::Resource => {
                net.current_scale = float("scale").unwrap_or(0.0);
                net.last_type = match tag.get("stored") {
                    Some(Tag::String(id)) => Some(id.clone()),
                    _ => None,
                };
            }
            ClientKind::Heat => {}
        }
    }

    /// `network.updateCapacity()`: a soma de `getCapacity()` dos transmissores da rede (`BufferedTransmitter`, não o calor).
    fn update_capacity(&mut self, network: u128, joining: &ClientTile, joining_pos: BlockPos) {
        let Some(kind) = self.networks.get(&network).map(|net| net.kind) else {
            return;
        };
        if kind == ClientKind::Heat {
            return;
        }
        let positions: Vec<BlockPos> = self.networks[&network]
            .transmitters
            .keys()
            .copied()
            .collect();
        let mut sum: i64 = 0;
        for pos in positions {
            let capacity = if pos == joining_pos {
                joining.capacity
            } else {
                self.tiles.get(&pos).map_or(0, |tile| tile.capacity)
            };
            // `if (transmitterCapacity > Long.MAX_VALUE - capacity) sum = Long.MAX_VALUE`: o `capacity` é o da rede antes da soma.
            let current = self.networks[&network].capacity;
            if capacity > i64::MAX - current {
                sum = i64::MAX;
                break;
            }
            sum += capacity;
        }
        if let Some(net) = self.networks.get_mut(&network) {
            net.capacity = sum;
        }
    }

    /// Os `ClientNetworkRemove`/`ClientNetworkAdd` de `setTransmitterNetwork`.
    fn apply_effects(&mut self, pos: BlockPos, generation: u64, _kind: ClientKind, effects: &[Effect]) {
        for effect in effects {
            match effect {
                Effect::ClientNetworkRemove(old) => self.remove_transmitter(*old, pos, generation),
                Effect::ClientNetworkAdd(new) => {
                    if let Some(network) = self.networks.get_mut(new) {
                        network.transmitters.insert(pos, generation);
                    }
                }
                _ => {}
            }
        }
    }

    /// `DynamicNetwork.removeTransmitter(transmitter)` no cliente: sai só se o tile é o mesmo (o do mesmo lugar que o chunk recarregado
    /// pôs antes não sai), e a rede vazia se desregistra.
    fn remove_transmitter(&mut self, network: NetworkId, pos: BlockPos, generation: u64) {
        let Some(net) = self.networks.get_mut(&network) else {
            return;
        };
        match net.transmitters.get(&pos) {
            Some(current) if *current != generation => return,
            Some(_) => {
                net.transmitters.remove(&pos);
            }
            None => {}
        }
        if net.transmitters.is_empty() {
            self.networks.remove(&network);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os pacotes do canal entre o servidor e o cliente. O Horizon junta o pacote de atualização de um block entity de mod no `nbt` da cópia do cliente sem avisar o
// mod (`native_block_entity_update_tag`), então o `handleUpdateTag` não teria onde rodar; a cola manda o mesmo compound (`writeReducedUpdatedTag`) por um canal
// de mod, que roda o `handleUpdateTag` ao chegar.

/// Um pacote de atualização de transmissor: o que o cliente precisa para criar o tile (`kind` e `capacity`) e o compound do `writeReducedUpdatedTag`.
#[derive(Clone, Debug, PartialEq)]
pub struct TagMessage {
    pub dimension: String,
    pub pos: BlockPos,
    pub kind: ClientKind,
    pub capacity: i64,
    pub tag: Compound,
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u16).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn put_pos(out: &mut Vec<u8>, pos: BlockPos) {
    for value in [pos.x, pos.y, pos.z] {
        out.extend_from_slice(&value.to_be_bytes());
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (head, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = u16::from_be_bytes(self.take(2)?.try_into().ok()?) as usize;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }

    fn pos(&mut self) -> Option<BlockPos> {
        Some(BlockPos::new(self.i32()?, self.i32()?, self.i32()?))
    }
}

impl ClientKind {
    fn code(self) -> u8 {
        match self {
            ClientKind::Energy => 0,
            ClientKind::Resource => 1,
            ClientKind::Heat => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => ClientKind::Energy,
            1 => ClientKind::Resource,
            2 => ClientKind::Heat,
            _ => return None,
        })
    }
}

impl TagMessage {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_string(&mut out, &self.dimension);
        put_pos(&mut out, self.pos);
        out.push(self.kind.code());
        out.extend_from_slice(&self.capacity.to_be_bytes());
        out.extend_from_slice(&(self.tag.len() as u16).to_be_bytes());
        for (key, value) in &self.tag {
            put_string(&mut out, key);
            match value {
                Tag::Byte(v) => out.extend_from_slice(&[1, *v as u8]),
                Tag::Short(v) => {
                    out.push(2);
                    out.extend_from_slice(&v.to_be_bytes());
                }
                Tag::Int(v) => {
                    out.push(3);
                    out.extend_from_slice(&v.to_be_bytes());
                }
                Tag::Long(v) => {
                    out.push(4);
                    out.extend_from_slice(&v.to_be_bytes());
                }
                Tag::Float(v) => {
                    out.push(5);
                    out.extend_from_slice(&v.to_bits().to_be_bytes());
                }
                Tag::Double(v) => {
                    out.push(6);
                    out.extend_from_slice(&v.to_bits().to_be_bytes());
                }
                Tag::String(v) => {
                    out.push(7);
                    put_string(&mut out, v);
                }
                Tag::IntArray(v) => {
                    out.push(8);
                    out.extend_from_slice(&(v.len() as u16).to_be_bytes());
                    for item in v {
                        out.extend_from_slice(&item.to_be_bytes());
                    }
                }
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        let dimension = reader.string()?;
        let pos = reader.pos()?;
        let kind = ClientKind::from_code(reader.u8()?)?;
        let capacity = reader.i64()?;
        let count = u16::from_be_bytes(reader.take(2)?.try_into().ok()?);
        let mut tag = Compound::new();
        for _ in 0..count {
            let key = reader.string()?;
            let value = match reader.u8()? {
                1 => Tag::Byte(reader.u8()? as i8),
                2 => Tag::Short(i16::from_be_bytes(reader.take(2)?.try_into().ok()?)),
                3 => Tag::Int(reader.i32()?),
                4 => Tag::Long(reader.i64()?),
                5 => Tag::Float(f32::from_bits(reader.i32()? as u32)),
                6 => Tag::Double(f64::from_bits(reader.i64()? as u64)),
                7 => Tag::String(reader.string()?),
                8 => {
                    let len = u16::from_be_bytes(reader.take(2)?.try_into().ok()?);
                    let mut items = Vec::new();
                    for _ in 0..len {
                        items.push(reader.i32()?);
                    }
                    Tag::IntArray(items)
                }
                _ => return None,
            };
            tag.insert(key, value);
        }
        reader.0.is_empty().then_some(Self {
            dimension,
            pos,
            kind,
            capacity,
            tag,
        })
    }
}

/// O transmissor saiu do mundo do cliente (`setRemoved`).
pub fn encode_gone(dimension: &str, pos: BlockPos) -> Vec<u8> {
    let mut out = Vec::new();
    put_string(&mut out, dimension);
    put_pos(&mut out, pos);
    out
}

pub fn decode_gone(bytes: &[u8]) -> Option<(String, BlockPos)> {
    let mut reader = Reader(bytes);
    let dimension = reader.string()?;
    let pos = reader.pos()?;
    reader.0.is_empty().then_some((dimension, pos))
}

/// `PacketFluidNetworkContents` e `PacketChemicalNetworkContents`: a rede e o último tipo (`None` é o recurso vazio).
pub fn encode_contents(network: u128, resource: Option<&str>) -> Vec<u8> {
    let mut out = network.to_be_bytes().to_vec();
    match resource {
        Some(id) => {
            out.push(1);
            put_string(&mut out, id);
        }
        None => out.push(0),
    }
    out
}

pub fn decode_contents(bytes: &[u8]) -> Option<(u128, Option<String>)> {
    let mut reader = Reader(bytes);
    let network = u128::from_be_bytes(reader.take(16)?.try_into().ok()?);
    let resource = match reader.u8()? {
        0 => None,
        1 => Some(reader.string()?),
        _ => return None,
    };
    reader.0.is_empty().then_some((network, resource))
}
