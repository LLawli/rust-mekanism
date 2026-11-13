//! `TransmitterNetworkRegistry`: os conjuntos de redes, de transmissores inválidos e de redes a confirmar, o `DimData` de cada dimensão
//! (os órfãos novos e os chunks que mudaram) e o `OrphanPathFinder` que junta um órfão às redes ao redor.
//!
//! A ordem de um tick (`onTick`, `TransmitterNetworkRegistry.java:126-135`): `handleChangedChunks`, `removeInvalidTransmitters`,
//! `assignOrphans`, `commitChanges` e, só com o tick rodando normalmente (`tickRateManager().runsNormally()`), o `onUpdate` de cada rede.
use super::dynamic_network::DynamicNetwork;
use super::validator::CompatibleTransmitterValidator;
use super::{DimensionId, Hashed, NetworkId, TransmitterHost, TransmitterId, Uuid};
use crate::sides::Face;
use crate::sim::chunks::{BlockPos, MAX_VIEW_DISTANCE};
use crate::sim::open_hash::OpenHashSet;
use std::collections::{BTreeMap, HashSet, VecDeque};

/// `Transmitter.setTransmitterNetwork(network, requestNow)` no servidor: troca a rede, marca o órfão (`orphaned = network == null`) e
/// pede a atualização agora, ou devolve `true` se quem chamou deve pedi-la depois.
pub fn set_transmitter_network<A>(
    host: &mut dyn TransmitterHost<A>,
    transmitter: TransmitterId,
    network: Option<NetworkId>,
    request_now: bool,
) -> bool {
    if host.transmitter_network(transmitter) == network {
        return false;
    }
    host.store_transmitter_network(transmitter, network);
    host.set_orphan(transmitter, network.is_none());
    if request_now {
        host.request_update(transmitter);
        false
    } else {
        true
    }
}

/// `DimData`: o que o registro guarda de cada dimensão.
#[derive(Default)]
struct DimData {
    /// `transmitters`, um `LongMultimap`: a árvore AVL por chunk (`ChunkPos.pack`), com a lista em ordem de inserção.
    transmitters: BTreeMap<i64, Vec<TransmitterId>>,
    /// `changedTicketChunks`, um `Long2BooleanRBTreeMap`: o chunk e se ele passou a estar carregado.
    changed_ticket_chunks: BTreeMap<i64, bool>,
    /// `newOrphanTransmitters`, um `Long2ObjectRBTreeMap` por `BlockPos.asLong`.
    new_orphan_transmitters: BTreeMap<i64, TransmitterId>,
}

/// O `TransmitterNetworkRegistry`, que no original é todo estático. `A` é o aceitador da família.
pub struct TransmitterNetworkRegistry<A> {
    /// `dimensionStore`. O original é um `HashMap<ResourceKey<Level>, DimData>`, cuja ordem vem do `hashCode` de identidade da chave e
    /// muda a cada execução; aqui a ordem é a do [`DimensionId`] (decisão 101).
    dimension_store: BTreeMap<DimensionId, DimData>,
    /// `networks`.
    networks: OpenHashSet<Hashed<NetworkId>>,
    /// `invalidTransmitters`.
    invalid_transmitters: OpenHashSet<Hashed<TransmitterId>>,
    /// `networksToChange`.
    networks_to_change: OpenHashSet<Hashed<NetworkId>>,
    /// Todas as redes que existiram. Uma rede desfeita continua aqui, sem transmissores e fora do `networks`.
    arena: Vec<DynamicNetwork<A>>,
    uuids: Box<dyn FnMut() -> Uuid>,
    /// As mensagens que o original põe no log como erro ou aviso.
    log: Vec<String>,
}

impl<A: Clone + 'static> Default for TransmitterNetworkRegistry<A> {
    fn default() -> Self {
        Self::new(Uuid::random)
    }
}

impl<A: Clone + 'static> TransmitterNetworkRegistry<A> {
    /// Um registro vazio. `uuids` dá o UUID de cada rede nova (`UUID.randomUUID()`): um teste dá os `hashCode` que o rastro gravou.
    pub fn new(uuids: impl FnMut() -> Uuid + 'static) -> Self {
        Self {
            dimension_store: BTreeMap::new(),
            networks: OpenHashSet::new(),
            invalid_transmitters: OpenHashSet::new(),
            networks_to_change: OpenHashSet::new(),
            arena: Vec::new(),
            uuids: Box::new(uuids),
            log: Vec::new(),
        }
    }

    /// `reset()`: o que o servidor faz ao parar. As redes antigas ficam na lista, sem registro.
    pub fn reset(&mut self) {
        self.networks.clear();
        self.networks_to_change.clear();
        self.invalid_transmitters.clear();
        self.dimension_store.clear();
        for network in &mut self.arena {
            network.registered = false;
        }
    }

    /// As mensagens de erro e de aviso do original desde a última chamada.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub(crate) fn log(&mut self, message: String) {
        self.log.push(message);
    }

    pub(crate) fn next_uuid(&mut self) -> Uuid {
        (self.uuids)()
    }

    pub(crate) fn push_network(&mut self, network: DynamicNetwork<A>) -> NetworkId {
        self.arena.push(network);
        NetworkId(u32::try_from(self.arena.len() - 1).expect("menos de 2^32 redes"))
    }

    /// A rede, viva ou já desfeita.
    pub fn network(&self, network: NetworkId) -> &DynamicNetwork<A> {
        &self.arena[network.0 as usize]
    }

    pub(crate) fn network_mut(&mut self, network: NetworkId) -> &mut DynamicNetwork<A> {
        &mut self.arena[network.0 as usize]
    }

    /// `network.getTransmitterValidator()`.
    pub fn network_validator(
        &self,
        network: NetworkId,
    ) -> Option<&dyn CompatibleTransmitterValidator<A>> {
        self.network(network).validator.as_deref()
    }

    pub(crate) fn network_key(&self, network: NetworkId) -> Hashed<NetworkId> {
        Hashed {
            id: network,
            hash: self.network(network).uuid.hash_code(),
        }
    }

    pub(crate) fn transmitter_key(
        host: &dyn TransmitterHost<A>,
        transmitter: TransmitterId,
    ) -> Hashed<TransmitterId> {
        Hashed {
            id: transmitter,
            hash: host.identity_hash(transmitter),
        }
    }

    /// As redes do conjunto `networks`, na ordem em que o `onTick` chama o `onUpdate`.
    pub fn networks_in_order(&self) -> Vec<NetworkId> {
        self.networks.in_order().into_iter().map(|k| k.id).collect()
    }

    /// Os transmissores rastreados num chunk, na ordem em que entraram (`DimData.transmitters.get(chunk)`).
    pub fn tracked_in_chunk(&self, dimension: DimensionId, chunk: i64) -> Vec<TransmitterId> {
        self.dimension_store
            .get(&dimension)
            .and_then(|data| data.transmitters.get(&chunk))
            .cloned()
            .unwrap_or_default()
    }

    fn dim_data(&mut self, dimension: DimensionId) -> &mut DimData {
        self.dimension_store.entry(dimension).or_default()
    }

    /// `trackTransmitter`.
    pub fn track_transmitter(&mut self, host: &dyn TransmitterHost<A>, transmitter: TransmitterId) {
        let chunk = host.block_pos(transmitter).chunk_key();
        self.dim_data(host.dimension(transmitter))
            .transmitters
            .entry(chunk)
            .or_default()
            .push(transmitter);
    }

    /// `untrackTransmitter`.
    pub fn untrack_transmitter(
        &mut self,
        host: &dyn TransmitterHost<A>,
        transmitter: TransmitterId,
    ) {
        let chunk = host.block_pos(transmitter).chunk_key();
        if let Some(data) = self.dimension_store.get_mut(&host.dimension(transmitter))
            && let Some(list) = data.transmitters.get_mut(&chunk)
        {
            if let Some(index) = list.iter().position(|tracked| *tracked == transmitter) {
                list.remove(index);
            }
            if list.is_empty() {
                data.transmitters.remove(&chunk);
            }
        }
    }

    /// `invalidateTransmitter`.
    pub fn invalidate_transmitter(
        &mut self,
        host: &dyn TransmitterHost<A>,
        transmitter: TransmitterId,
    ) {
        self.invalid_transmitters
            .insert(Self::transmitter_key(host, transmitter));
        let coord = host.block_pos(transmitter).as_long();
        let mut message = None;
        if let Some(data) = self.dimension_store.get_mut(&host.dimension(transmitter)) {
            // `DimData.invalidateTransmitter`
            if let Some(removed) = data.new_orphan_transmitters.remove(&coord)
                && removed != transmitter
            {
                message = Some(format!(
                    "Different orphan transmitter was registered at location during removal! {}",
                    block_to_str(host.block_pos(transmitter))
                ));
                // `put it back? TODO: work out if this is correct, probably continues the error`
                data.new_orphan_transmitters.insert(coord, transmitter);
            }
        }
        self.log.extend(message);
    }

    /// `registerOrphanTransmitter`.
    pub fn register_orphan_transmitter(
        &mut self,
        host: &dyn TransmitterHost<A>,
        transmitter: TransmitterId,
    ) {
        if self
            .invalid_transmitters
            .remove(Self::transmitter_key(host, transmitter))
        {
            // Já era um inválido: tirá-lo basta (se era órfão ainda é, se não era continua na rede em que estava).
            return;
        }
        let pos = host.block_pos(transmitter);
        let previous = self
            .dim_data(host.dimension(transmitter))
            .new_orphan_transmitters
            .insert(pos.as_long(), transmitter);
        if let Some(previous) = previous
            && previous != transmitter
            && host.is_valid(previous)
        {
            self.log.push(format!(
                "Different orphan transmitter was already registered at location! {}",
                block_to_str(pos)
            ));
        }
    }

    /// `registerChangedNetwork`.
    pub fn register_changed_network(&mut self, network: NetworkId) {
        let key = self.network_key(network);
        self.networks_to_change.insert(key);
    }

    /// `registerNetwork` (`register()` no servidor).
    pub fn register_network(&mut self, network: NetworkId) {
        let key = self.network_key(network);
        self.networks.insert(key);
        self.network_mut(network).registered = true;
    }

    /// `removeNetwork`.
    pub(crate) fn remove_network(&mut self, network: NetworkId) {
        let key = self.network_key(network);
        self.networks.remove(key);
        self.networks_to_change.remove(key);
        self.network_mut(network).registered = false;
    }

    /// `onTick(ServerTickEvent.Post)`. `runs_normally` é `tickRateManager().runsNormally()`.
    pub fn on_tick(&mut self, host: &mut dyn TransmitterHost<A>, runs_normally: bool) {
        self.handle_changed_chunks(host);
        self.remove_invalid_transmitters(host);
        self.assign_orphans(host);
        self.commit_changes(host);
        if runs_normally {
            for network in self.networks_in_order() {
                host.on_update(self, network);
            }
        }
    }

    /// `onTicketLevelChange(ChunkTicketLevelUpdatedEvent)`, com o chunk em `ChunkPos.pack`.
    pub fn on_ticket_level_change(
        &mut self,
        dimension: DimensionId,
        chunk: i64,
        old_ticket_level: i32,
        new_ticket_level: i32,
    ) {
        let loaded = if old_ticket_level > MAX_VIEW_DISTANCE
            && new_ticket_level <= MAX_VIEW_DISTANCE
        {
            // Foi de "descarregado" a carregado.
            true
        } else if new_ticket_level > MAX_VIEW_DISTANCE && old_ticket_level <= MAX_VIEW_DISTANCE {
            false
        } else {
            return;
        };
        let Some(data) = self.dimension_store.get_mut(&dimension) else {
            return;
        };
        // `DimData.onTicketLevelChange`: só acompanha o chunk se houver transmissores nele.
        if data.transmitters.contains_key(&chunk) {
            if data
                .changed_ticket_chunks
                .get(&chunk)
                .copied()
                .unwrap_or(loaded)
                != loaded
            {
                // O estado voltou ao que era: não mudou.
                data.changed_ticket_chunks.remove(&chunk);
            } else {
                data.changed_ticket_chunks.insert(chunk, loaded);
            }
        }
    }

    fn handle_changed_chunks(&mut self, host: &mut dyn TransmitterHost<A>) {
        let dimensions: Vec<DimensionId> = self.dimension_store.keys().copied().collect();
        for dimension in dimensions {
            let Some(data) = self.dimension_store.get_mut(&dimension) else {
                continue;
            };
            if data.changed_ticket_chunks.is_empty() {
                continue;
            }
            let changed = std::mem::take(&mut data.changed_ticket_chunks);
            for (chunk, loaded) in changed {
                let chunk_transmitters = self.tracked_in_chunk(dimension, chunk);
                for transmitter in chunk_transmitters {
                    host.chunk_accessibility_change(self, transmitter, loaded);
                }
            }
        }
    }

    fn remove_invalid_transmitters(&mut self, host: &mut dyn TransmitterHost<A>) {
        if self.invalid_transmitters.is_empty() {
            return;
        }
        // Copia os inválidos, para que os que o `invalidate` registrar como órfãos entrem de fato como órfãos, e não sejam tirados
        // da lista de inválidos.
        let to_invalidate = std::mem::take(&mut self.invalid_transmitters).in_order();
        for invalid in to_invalidate {
            self.remove_invalid_transmitter(host, invalid.id);
        }
    }

    fn remove_invalid_transmitter(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        invalid: TransmitterId,
    ) {
        if (!host.is_orphan(invalid) || !host.is_valid(invalid))
            && let Some(network) = host.transmitter_network(invalid)
        {
            self.invalidate_network(host, network, Some(invalid));
            if !host.is_valid(invalid) {
                // Sem rede, para que um transmissor que de algum jeito volte seja tratado como órfão.
                set_transmitter_network(host, invalid, None, false);
            }
        }
    }

    fn assign_orphans(&mut self, host: &mut dyn TransmitterHost<A>) {
        let dimensions: Vec<DimensionId> = self.dimension_store.keys().copied().collect();
        for dimension in dimensions {
            let Some(data) = self.dimension_store.get_mut(&dimension) else {
                continue;
            };
            if data.new_orphan_transmitters.is_empty() {
                continue;
            }
            let orphans = std::mem::take(&mut data.new_orphan_transmitters);
            for orphan in orphans.values() {
                if host.is_valid(*orphan) && host.is_orphan(*orphan) {
                    let finder = OrphanPathFinder::new(&*host, *orphan);
                    let network = finder.get_network_from_orphan(host, self, &orphans);
                    let key = self.network_key(network);
                    self.networks_to_change.insert(key);
                }
            }
        }
    }

    fn commit_changes(&mut self, host: &mut dyn TransmitterHost<A>) {
        if self.networks_to_change.is_empty() {
            return;
        }
        let networks = std::mem::take(&mut self.networks_to_change).in_order();
        for network in networks {
            self.commit_network(host, network.id);
        }
    }
}

fn block_to_str(pos: BlockPos) -> String {
    format!("x={}, y={}, z={}", pos.x, pos.y, pos.z)
}

/// `OrphanPathFinder`: a busca em largura que junta um órfão aos órfãos ao redor e às redes que encosta.
struct OrphanPathFinder<A> {
    transmitter_validator: Box<dyn CompatibleTransmitterValidator<A>>,
    connected_transmitters: OpenHashSet<Hashed<TransmitterId>>,
    networks_found: OpenHashSet<Hashed<NetworkId>>,
    iterated: HashSet<BlockPos>,
    queue: VecDeque<BlockPos>,
    start_point: TransmitterId,
    world: DimensionId,
}

impl<A: Clone + 'static> OrphanPathFinder<A> {
    fn new(host: &dyn TransmitterHost<A>, start: TransmitterId) -> Self {
        Self {
            transmitter_validator: host.new_orphan_validator(start),
            connected_transmitters: OpenHashSet::new(),
            networks_found: OpenHashSet::new(),
            iterated: HashSet::new(),
            queue: VecDeque::new(),
            start_point: start,
            world: host.dimension(start),
        }
    }

    fn get_network_from_orphan(
        mut self,
        host: &mut dyn TransmitterHost<A>,
        registry: &mut TransmitterNetworkRegistry<A>,
        orphan_transmitters: &BTreeMap<i64, TransmitterId>,
    ) -> NetworkId {
        // `queue.push(...)`: a fila está vazia, então vale `addFirst`.
        self.queue.push_front(host.block_pos(self.start_point));
        while let Some(from) = self.queue.pop_front() {
            self.iterate(host, registry, orphan_transmitters, from);
        }
        let found = self.networks_found.in_order();
        let network = if found.len() == 1 {
            found[0].id
        } else {
            let found: Vec<NetworkId> = found.into_iter().map(|key| key.id).collect();
            registry.create_network_by_merging(host, &found)
        };
        let connected: Vec<TransmitterId> = self
            .connected_transmitters
            .in_order()
            .into_iter()
            .map(|key| key.id)
            .collect();
        registry.add_new_transmitters(host, network, &connected, self.transmitter_validator);
        network
    }

    fn iterate(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        registry: &TransmitterNetworkRegistry<A>,
        orphan_transmitters: &BTreeMap<i64, TransmitterId>,
        from: BlockPos,
    ) {
        if !self.iterated.insert(from) {
            return;
        }
        if let Some(&transmitter) = orphan_transmitters.get(&from.as_long()) {
            if host.is_valid(transmitter)
                && host.is_orphan(transmitter)
                && host.supports_transmission_type(self.start_point, transmitter)
                && self
                    .transmitter_validator
                    .is_transmitter_compatible(&*host, transmitter)
            {
                self.connected_transmitters
                    .insert(TransmitterNetworkRegistry::transmitter_key(
                        &*host,
                        transmitter,
                    ));
                host.set_orphan(transmitter, false);
                for direction in Face::ALL {
                    let direction_pos = from.relative(direction);
                    if !self.iterated.contains(&direction_pos)
                        && let Some(tile) = host.transmitter_at(self.world, direction_pos)
                        && host.is_valid_transmitter_basic(transmitter, tile, direction)
                    {
                        self.queue.push_back(direction_pos);
                    }
                }
            }
        } else if let Some(tile) = host.transmitter_at(self.world, from)
            && host.supports_transmission_type(self.start_point, tile)
        {
            // Precisa de uma rede de fora.
            if let Some(network) = host.transmitter_network(tile)
                && self
                    .transmitter_validator
                    .is_network_compatible(&*host, registry, network)
            {
                self.networks_found.insert(registry.network_key(network));
            }
        }
    }
}
