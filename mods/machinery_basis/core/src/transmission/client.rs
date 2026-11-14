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

/// O canal dos pacotes dos transmissores entre o servidor e o cliente: o pacote de atualização (`getUpdateTag`/`handleUpdateTag`), a saída de um transmissor, o
/// `PacketNetworkScale` e o conteúdo da rede, em lotes na ordem em que saíram. O canal de mod do Horizon aceita 32 mensagens por jogador por tick, e uma rede que se
/// funde pede a atualização de cada transmissor dela: os pacotes vão juntos.
pub const OPS_CHANNEL: &str = "machinery_basis:transmitters";
/// O canal que esvazia o cliente (`MekanismClient.reset`, `TransmitterNetworkRegistry.clearClientNetworks`).
pub const RESET_CHANNEL: &str = "machinery_basis:transmitter_reset";
/// O que cabe numa mensagem de mod (`native_net_send_to_player`: 30.000 bytes), com folga para o cabeçalho.
pub const MAX_BATCH: usize = 28_000;

/// O chunk `at` está dentro da distância de visão (`ChunkMap.isChunkInRange`: o máximo das duas diferenças) de quem está no chunk `center`.
pub fn chunk_in_view(at: (i32, i32), center: (i32, i32), view: i32) -> bool {
    (at.0 - center.0).abs() <= view && (at.1 - center.1).abs() <= view
}

/// O `ChunkMap` manda o pacote de atualização dos block entities de um chunk a quem passa a vê-lo: `before` é o chunk de quem estava na mesma dimensão no tick anterior (`None` se
/// chegou agora ou veio de outra dimensão, e então todo chunk que ele vê é novo). Vale para o chunk `at` se está na vista de `center` e não estava na de `before`.
pub fn chunk_entered_view(
    at: (i32, i32),
    center: (i32, i32),
    before: Option<(i32, i32)>,
    view: i32,
) -> bool {
    chunk_in_view(at, center, view) && !before.is_some_and(|old| chunk_in_view(at, old, view))
}

/// O `ChunkMap` para de mandar um chunk a quem sai da vista dele (o `ClientboundForgetLevelChunkPacket`, e o `ClientChunkCache` o solta com os block entities): vale para o chunk
/// `at` se estava na vista de `before` e já não está na de `center`.
pub fn chunk_left_view(at: (i32, i32), center: (i32, i32), before: (i32, i32), view: i32) -> bool {
    chunk_in_view(at, before, view) && !chunk_in_view(at, center, view)
}

/// Os chunks que o jogador deixou de ver ao ir do chunk `before` ao chunk `center`, na ordem de `x` e `z`.
pub fn chunks_left_view(center: (i32, i32), before: (i32, i32), view: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for x in before.0 - view..=before.0 + view {
        for z in before.1 - view..=before.1 + view {
            if chunk_left_view((x, z), center, before, view) {
                out.push((x, z));
            }
        }
    }
    out
}

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
    /// `LogisticalTransporterBase`: `InventoryNetwork`, sem buffer (o pacote de atualização leva as pilhas em trânsito e a cor).
    Items,
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
    /// O que o pacote de atualização leva além do `Transmitter`: o calor do condutor e as pilhas e a cor do transportador.
    extra: ClientExtra,
}

/// Uma pilha em trânsito como o pacote de atualização a leva (`TransporterStack.writeToUpdateTag`, lida por `readFromUpdate`): o cliente a guarda no `transit` do
/// transportador, pelo índice que o servidor deu, para o desenho da Fase 4.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientStack {
    /// `SerializationConstants.INDEX`.
    pub index: i32,
    /// O id do item (`ItemResource.id`) e a quantidade (`LargeResourceStack`), vazios se o pacote não tinha o recurso (a pilha vazia de `readOrEmpty`).
    pub item: Option<(String, i64)>,
    /// O `components` do `ItemResource.CODEC`, quando o recurso tem algum.
    pub components: Option<Tag>,
    /// `color` (o nome da `EnumColor`), como o índice dela.
    pub color: Option<u8>,
    pub progress: i32,
    /// `original_location` (`Long.MAX_VALUE` sem posição).
    pub original_location: i64,
    /// `path_type` (`Path.serialized_name`).
    pub path_type: Option<String>,
    /// `next` e `previous`, os `clientNext` e `clientPrev` do cliente.
    pub next: Option<i64>,
    pub previous: Option<i64>,
}

impl ClientStack {
    fn read(index: i32, input: &Compound) -> Self {
        let long = |key: &str| match input.get(key) {
            Some(Tag::Long(value)) => Some(*value),
            Some(other) => other.int_value().map(i64::from),
            None => None,
        };
        let mut item = None;
        let mut components = None;
        if let Some(Tag::Compound(holder)) = input.get("item")
            && let Some(Tag::Compound(resource)) = holder.get("type")
            && let Some(Tag::String(id)) = resource.get("id")
            && let Some(amount) = match holder.get("amount") {
                Some(Tag::Long(value)) => Some(*value),
                Some(other) => other.int_value().map(i64::from),
                None => None,
            }
            && amount > 0
        {
            item = Some((id.clone(), amount));
            components = resource.get("components").cloned();
        }
        Self {
            index,
            item,
            components,
            color: match input.get("color") {
                Some(Tag::String(name)) => crate::side_config::ENUM_COLOR_NAMES
                    .iter()
                    .position(|candidate| candidate == name)
                    .map(|index| index as u8),
                _ => None,
            },
            progress: input.get("progress").and_then(Tag::int_value).unwrap_or(0),
            original_location: long("original_location").unwrap_or(i64::MAX),
            path_type: match input.get("path_type") {
                Some(Tag::String(name)) => Some(name.clone()),
                _ => None,
            },
            next: long("next"),
            previous: long("previous"),
        }
    }
}

/// O que o cliente guarda de um transmissor além das conexões e da rede: o `buffer` do condutor (`setHeat(getDoubleOr(HEAT_STORED, getHeat()))`) e o `transit` e a cor do
/// transportador. O desenho da Fase 4 lê daqui.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientExtra {
    /// `ThermodynamicConductor.buffer.getHeat()` depois do último pacote; `None` até o primeiro, que a conta de `getHeat` do cliente nunca viu.
    pub heat: Option<f64>,
    /// `LogisticalTransporter.color`.
    pub color: Option<u8>,
    /// `DiversionTransporter.modes` (`readModes`), um por lado; `None` se o pacote não tem a chave.
    pub modes: Option<[i32; 6]>,
    /// `LogisticalTransporterBase.transit`.
    pub transit: Vec<ClientStack>,
}

impl ClientExtra {
    /// `PacketTransporterSync.handle` e as atualizações de `PacketTransporterBatch.handle`: `transporter.addStack(id, stack)`, a pilha do pacote no lugar da que tinha o mesmo
    /// índice. O `STREAM_CODEC` lê o progresso zero como 5.
    fn put_stack(&mut self, index: i32, input: &Compound) {
        let mut stack = ClientStack::read(index, input);
        if stack.progress == 0 {
            stack.progress = 5;
        }
        match self.transit.iter_mut().find(|known| known.index == index) {
            Some(known) => *known = stack,
            None => self.transit.push(stack),
        }
    }

    /// `transporter.deleteStack(id)`.
    fn delete_stack(&mut self, index: i32) {
        self.transit.retain(|stack| stack.index != index);
    }

    /// `LogisticalTransporterBase.onUpdateClient`: cada pilha avança `speed` (`tier.getSpeed()`) até 100.
    fn tick_client(&mut self, speed: i32) {
        for stack in &mut self.transit {
            stack.progress = (stack.progress + speed).min(100);
        }
    }

    /// `handleUpdateTag` do condutor e do transportador. Devolve se a cor mudou (`refreshModelData`).
    fn handle_update_tag(&mut self, kind: ClientKind, tag: &Compound) -> bool {
        match kind {
            ClientKind::Heat => {
                if let Some(Tag::Double(heat)) =
                    tag.get(super::thermodynamic_conductor::keys::HEAT_STORED)
                {
                    self.heat = Some(*heat);
                }
                false
            }
            ClientKind::Items => {
                self.transit.clear();
                if let Some(Tag::List(items)) = tag.get("items") {
                    for item in items {
                        let Tag::Compound(input) = item else {
                            continue;
                        };
                        // Um pacote sem índice é ignorado.
                        if let Some(index) = input.get("index").and_then(Tag::int_value) {
                            self.transit.retain(|stack| stack.index != index);
                            self.transit.push(ClientStack::read(index, input));
                        }
                    }
                }
                self.modes = match tag.get("mode") {
                    Some(Tag::IntArray(modes)) => {
                        let mut out = [0; 6];
                        for (slot, mode) in out.iter_mut().zip(modes) {
                            *slot = *mode;
                        }
                        Some(out)
                    }
                    _ => None,
                };
                let color = match tag.get("color") {
                    Some(Tag::String(name)) => crate::side_config::ENUM_COLOR_NAMES
                        .iter()
                        .position(|candidate| candidate == name)
                        .map(|index| index as u8),
                    _ => None,
                };
                let changed = self.color != color;
                self.color = color;
                changed
            }
            _ => false,
        }
    }
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
    /// As posições cujo dado do modelo (`TransmitterModelData`) o original refaria: o tile entrou ou saiu do mundo, ou o pacote de atualização mudou o que o modelo lê
    /// (`updateModelData`, `requestModelDataUpdate`). O desenho as esvazia a cada tique (`take_model_changes`).
    model_changes: std::collections::HashSet<BlockPos>,
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

    /// O que o pacote de atualização deixou no tile em `pos` além das conexões: o calor do condutor, as pilhas e a cor do transportador.
    pub fn extra_at(&self, pos: BlockPos) -> Option<&ClientExtra> {
        self.tiles.get(&pos).map(|tile| &tile.extra)
    }

    /// `PacketTransporterSync.handle`: o tile em `pos` é um transportador do cliente? Então ele guarda a pilha. Sem o tile no cliente o pacote se perde (`PacketUtils.blockEntity`).
    pub fn on_transporter_sync(&mut self, pos: BlockPos, id: i32, stack: &Compound) {
        if let Some(tile) = self.tiles.get_mut(&pos)
            && tile.kind == ClientKind::Items
        {
            tile.extra.put_stack(id, stack);
        }
    }

    /// `PacketTransporterBatch.handle`: primeiro as atualizações, depois as saídas.
    pub fn on_transporter_batch(
        &mut self,
        pos: BlockPos,
        deletes: &[i32],
        updates: &[(i32, Compound)],
    ) {
        if let Some(tile) = self.tiles.get_mut(&pos)
            && tile.kind == ClientKind::Items
        {
            for (id, stack) in updates {
                tile.extra.put_stack(*id, stack);
            }
            for id in deletes {
                tile.extra.delete_stack(*id);
            }
        }
    }

    /// `TileEntityTransmitter.tickClient` do transportador em `pos`: `onUpdateClient`, com a velocidade do tier dele (`getSpeed`).
    pub fn tick_client_at(&mut self, pos: BlockPos, speed: i32) {
        if let Some(tile) = self.tiles.get_mut(&pos)
            && tile.kind == ClientKind::Items
        {
            tile.extra.tick_client(speed);
        }
    }

    /// `ClientChunkCache.drop(chunk)` de cada chunk que o servidor mandou esquecer (`None`: o nível inteiro, que o cliente larga ao sair da dimensão): os block entities do chunk
    /// saem do mundo (`setRemoved`).
    pub fn forget_chunks(&mut self, chunks: Option<&[(i32, i32)]>) {
        let gone: Vec<BlockPos> = self
            .tiles
            .keys()
            .filter(|pos| chunks.is_none_or(|list| list.contains(&(pos.x >> 4, pos.z >> 4))))
            .copied()
            .collect();
        for pos in gone {
            self.remove_tile(pos);
        }
    }

    /// O número de tiles do cliente.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// A posição de cada tile do cliente, em ordem.
    pub fn tile_positions(&self) -> Vec<BlockPos> {
        let mut positions: Vec<BlockPos> = self.tiles.keys().copied().collect();
        positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        positions
    }

    /// As posições cujo dado do modelo mudou desde a última chamada, em ordem (o desenho lê `view_at` de cada uma: sem tile, o dado sai).
    pub fn take_model_changes(&mut self) -> Vec<BlockPos> {
        let mut changes: Vec<BlockPos> = self.model_changes.drain().collect();
        changes.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        changes
    }

    /// O tile do transmissor entra no mundo do cliente (`clearRemoved` no cliente: `onWorldJoin` só marca `loaded`; o tile novo tira o
    /// anterior da mesma posição só quando ele sai).
    pub fn add_tile(&mut self, pos: BlockPos, kind: ClientKind, capacity: i64) {
        let transmission = match kind {
            ClientKind::Energy => TransmissionType::Energy,
            ClientKind::Resource => TransmissionType::Fluid,
            ClientKind::Heat => TransmissionType::Heat,
            ClientKind::Items => TransmissionType::Item,
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
            extra: ClientExtra::default(),
        };
        self.tiles.insert(pos, tile);
        self.model_changes.insert(pos);
    }

    /// `TileEntityTransmitter.setRemoved` no cliente (`onWorldSeparate`): se estava carregado, `setTransmitterNetwork(null)`.
    pub fn remove_tile(&mut self, pos: BlockPos) {
        let Some(mut tile) = self.tiles.remove(&pos) else {
            return;
        };
        self.model_changes.insert(pos);
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
        let mut update = tile.transmitter.handle_update_tag(tag, &mut effects);
        update.refresh_model_data |= tile.extra.handle_update_tag(tile.kind, tag);
        let generation = tile.generation;
        let kind = tile.kind;
        self.apply_effects(pos, generation, kind, &effects);
        effects.clear();
        match update.network {
            UpdateTagNetwork::Unchanged => {}
            UpdateTagNetwork::Detached => {}
            UpdateTagNetwork::Attach(id) => {
                let created = match self.networks.entry(id) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(ClientNetwork::new(id, kind));
                        true
                    }
                    std::collections::hash_map::Entry::Occupied(_) => false,
                };
                if created {
                    // `createEmptyNetworkWithID`, `register`, `setTransmitterNetwork` e `handleContentsUpdateTag`.
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
        if update.refresh_model_data {
            self.model_changes.insert(pos);
        }
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
            ClientKind::Heat | ClientKind::Items => {}
        }
    }

    /// `network.updateCapacity()`: a soma de `getCapacity()` dos transmissores da rede (`BufferedTransmitter`, não o calor).
    fn update_capacity(&mut self, network: u128, joining: &ClientTile, joining_pos: BlockPos) {
        let Some(kind) = self.networks.get(&network).map(|net| net.kind) else {
            return;
        };
        // `updateCapacity` é do `DynamicBufferedNetwork`: o calor e os itens não têm buffer.
        if matches!(kind, ClientKind::Heat | ClientKind::Items) {
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
    fn apply_effects(
        &mut self,
        pos: BlockPos,
        generation: u64,
        _kind: ClientKind,
        effects: &[Effect],
    ) {
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

/// Uma etiqueta no fio: o código do tipo e o valor (`List` e `Compound` são recursivos).
fn put_tag(out: &mut Vec<u8>, value: &Tag) {
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
            put_string(out, v);
        }
        Tag::IntArray(v) => {
            out.push(8);
            out.extend_from_slice(&(v.len() as u16).to_be_bytes());
            for item in v {
                out.extend_from_slice(&item.to_be_bytes());
            }
        }
        Tag::ByteArray(v) => {
            out.push(9);
            out.extend_from_slice(&(v.len() as u32).to_be_bytes());
            out.extend(v.iter().map(|byte| *byte as u8));
        }
        Tag::LongArray(v) => {
            out.push(10);
            out.extend_from_slice(&(v.len() as u32).to_be_bytes());
            for item in v {
                out.extend_from_slice(&item.to_be_bytes());
            }
        }
        Tag::List(items) => {
            out.push(11);
            out.extend_from_slice(&(items.len() as u32).to_be_bytes());
            for item in items {
                put_tag(out, item);
            }
        }
        Tag::Compound(entries) => {
            out.push(12);
            out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
            for (key, item) in entries {
                put_string(out, key);
                put_tag(out, item);
            }
        }
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

    fn u32(&mut self) -> Option<usize> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?) as usize)
    }

    /// `put_tag` ao contrário.
    fn tag(&mut self) -> Option<Tag> {
        Some(match self.u8()? {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(i16::from_be_bytes(self.take(2)?.try_into().ok()?)),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => Tag::String(self.string()?),
            8 => {
                let len = u16::from_be_bytes(self.take(2)?.try_into().ok()?);
                let mut items = Vec::new();
                for _ in 0..len {
                    items.push(self.i32()?);
                }
                Tag::IntArray(items)
            }
            9 => {
                let len = self.u32()?;
                Tag::ByteArray(self.take(len)?.iter().map(|byte| *byte as i8).collect())
            }
            10 => {
                let len = self.u32()?;
                let mut items = Vec::new();
                for _ in 0..len {
                    items.push(self.i64()?);
                }
                Tag::LongArray(items)
            }
            11 => {
                let len = self.u32()?;
                let mut items = Vec::new();
                for _ in 0..len {
                    items.push(self.tag()?);
                }
                Tag::List(items)
            }
            12 => {
                let count = u16::from_be_bytes(self.take(2)?.try_into().ok()?);
                let mut entries = Compound::new();
                for _ in 0..count {
                    let key = self.string()?;
                    entries.insert(key, self.tag()?);
                }
                Tag::Compound(entries)
            }
            _ => return None,
        })
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
            ClientKind::Items => 3,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => ClientKind::Energy,
            1 => ClientKind::Resource,
            2 => ClientKind::Heat,
            3 => ClientKind::Items,
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
            put_tag(&mut out, value);
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
            let value = reader.tag()?;
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

/// Um pacote do servidor ao cliente.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// O pacote de atualização de um transmissor (`handleUpdateTag`).
    Tag(TagMessage),
    /// O transmissor saiu do mundo do cliente (`setRemoved`).
    Gone { dimension: String, pos: BlockPos },
    /// `PacketNetworkScale`.
    Scale(ScalePacket),
    /// `PacketFluidNetworkContents` e `PacketChemicalNetworkContents`.
    Contents {
        network: u128,
        resource: Option<String>,
    },
    /// `PacketTransporterSync`.
    TransporterSync {
        dimension: String,
        pos: BlockPos,
        id: i32,
        stack: Compound,
    },
    /// `PacketTransporterBatch`.
    TransporterBatch {
        dimension: String,
        pos: BlockPos,
        deletes: Vec<i32>,
        updates: Vec<(i32, Compound)>,
    },
    /// `ClientboundForgetLevelChunkPacket` de cada chunk (`None`: o nível inteiro, que o cliente largou ao trocar de dimensão).
    Forget {
        dimension: String,
        chunks: Option<Vec<(i32, i32)>>,
    },
}

impl Op {
    fn encode(&self) -> Vec<u8> {
        match self {
            Op::Tag(message) => [&[0u8][..], &message.encode()].concat(),
            Op::Gone { dimension, pos } => [&[1u8][..], &encode_gone(dimension, *pos)].concat(),
            Op::Scale(packet) => [&[2u8][..], &packet.encode()].concat(),
            Op::Contents { network, resource } => {
                [&[3u8][..], &encode_contents(*network, resource.as_deref())].concat()
            }
            Op::TransporterSync {
                dimension,
                pos,
                id,
                stack,
            } => {
                let mut out = vec![4u8];
                put_string(&mut out, dimension);
                put_pos(&mut out, *pos);
                out.extend_from_slice(&id.to_be_bytes());
                put_tag(&mut out, &Tag::Compound(stack.clone()));
                out
            }
            Op::TransporterBatch {
                dimension,
                pos,
                deletes,
                updates,
            } => {
                let mut out = vec![5u8];
                put_string(&mut out, dimension);
                put_pos(&mut out, *pos);
                out.extend_from_slice(&(deletes.len() as u32).to_be_bytes());
                for id in deletes {
                    out.extend_from_slice(&id.to_be_bytes());
                }
                out.extend_from_slice(&(updates.len() as u32).to_be_bytes());
                for (id, stack) in updates {
                    out.extend_from_slice(&id.to_be_bytes());
                    put_tag(&mut out, &Tag::Compound(stack.clone()));
                }
                out
            }
            Op::Forget { dimension, chunks } => {
                let mut out = vec![6u8];
                put_string(&mut out, dimension);
                match chunks {
                    None => out.push(0),
                    Some(list) => {
                        out.push(1);
                        out.extend_from_slice(&(list.len() as u32).to_be_bytes());
                        for (x, z) in list {
                            out.extend_from_slice(&x.to_be_bytes());
                            out.extend_from_slice(&z.to_be_bytes());
                        }
                    }
                }
                out
            }
        }
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let (kind, rest) = bytes.split_first()?;
        Some(match kind {
            0 => Op::Tag(TagMessage::decode(rest)?),
            1 => {
                let (dimension, pos) = decode_gone(rest)?;
                Op::Gone { dimension, pos }
            }
            2 => Op::Scale(ScalePacket::decode(rest)?),
            3 => {
                let (network, resource) = decode_contents(rest)?;
                Op::Contents { network, resource }
            }
            4 => {
                let mut reader = Reader(rest);
                let dimension = reader.string()?;
                let pos = reader.pos()?;
                let id = reader.i32()?;
                let Tag::Compound(stack) = reader.tag()? else {
                    return None;
                };
                reader.0.is_empty().then_some(Op::TransporterSync {
                    dimension,
                    pos,
                    id,
                    stack,
                })?
            }
            5 => {
                let mut reader = Reader(rest);
                let dimension = reader.string()?;
                let pos = reader.pos()?;
                let mut deletes = Vec::new();
                for _ in 0..reader.u32()? {
                    deletes.push(reader.i32()?);
                }
                let mut updates = Vec::new();
                for _ in 0..reader.u32()? {
                    let id = reader.i32()?;
                    let Tag::Compound(stack) = reader.tag()? else {
                        return None;
                    };
                    updates.push((id, stack));
                }
                reader.0.is_empty().then_some(Op::TransporterBatch {
                    dimension,
                    pos,
                    deletes,
                    updates,
                })?
            }
            6 => {
                let mut reader = Reader(rest);
                let dimension = reader.string()?;
                let chunks = match reader.u8()? {
                    0 => None,
                    1 => {
                        let mut list = Vec::new();
                        for _ in 0..reader.u32()? {
                            list.push((reader.i32()?, reader.i32()?));
                        }
                        Some(list)
                    }
                    _ => return None,
                };
                reader
                    .0
                    .is_empty()
                    .then_some(Op::Forget { dimension, chunks })?
            }
            _ => return None,
        })
    }
}

/// Os pacotes em mensagens de até [`MAX_BATCH`] bytes, na ordem: cada mensagem é uma contagem e, por pacote, o tamanho e os bytes.
pub fn encode_batches(ops: &[Op]) -> Vec<Vec<u8>> {
    let mut batches = Vec::new();
    let mut body: Vec<u8> = Vec::new();
    let mut count: u16 = 0;
    let finish = |body: &mut Vec<u8>, count: &mut u16, batches: &mut Vec<Vec<u8>>| {
        if *count > 0 {
            let mut message = count.to_be_bytes().to_vec();
            message.append(body);
            batches.push(message);
            *count = 0;
        }
    };
    for op in ops {
        let bytes = op.encode();
        if !body.is_empty() && (body.len() + bytes.len() + 4 > MAX_BATCH || count == u16::MAX) {
            finish(&mut body, &mut count, &mut batches);
        }
        body.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(&bytes);
        count += 1;
    }
    finish(&mut body, &mut count, &mut batches);
    batches
}

/// Uma mensagem de [`encode_batches`]; `None` se está cortada ou tem sobra.
pub fn decode_batch(bytes: &[u8]) -> Option<Vec<Op>> {
    let mut reader = Reader(bytes);
    let count = u16::from_be_bytes(reader.take(2)?.try_into().ok()?);
    let mut ops = Vec::new();
    for _ in 0..count {
        let len = u32::from_be_bytes(reader.take(4)?.try_into().ok()?) as usize;
        ops.push(Op::decode(reader.take(len)?)?);
    }
    reader.0.is_empty().then_some(ops)
}

impl ClientTransmitters {
    /// Aplica um pacote do servidor no cliente (o `handle` de cada um).
    pub fn apply(&mut self, op: &Op) {
        match op {
            Op::Tag(message) => {
                if self.view_at(message.pos).is_none() {
                    self.add_tile(message.pos, message.kind, message.capacity);
                }
                self.handle_update_tag(message.pos, &message.tag);
            }
            Op::Gone { pos, .. } => self.remove_tile(*pos),
            Op::Scale(packet) => self.on_scale_packet(packet),
            Op::Contents { network, resource } => {
                self.on_contents_packet(*network, resource.clone())
            }
            Op::TransporterSync { pos, id, stack, .. } => {
                self.on_transporter_sync(*pos, *id, stack)
            }
            Op::TransporterBatch {
                pos,
                deletes,
                updates,
                ..
            } => self.on_transporter_batch(*pos, deletes, updates),
            Op::Forget { chunks, .. } => self.forget_chunks(chunks.as_deref()),
        }
    }
}
