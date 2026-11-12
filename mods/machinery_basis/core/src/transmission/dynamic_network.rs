//! `DynamicNetwork`, na parte de topologia: os transmissores da rede, o que espera o `commit`, a fusão (`adoptTransmittersAndAcceptorsFrom`)
//! e a invalidação. As operações que mexem no registro (`register`, `deregister`, `registerOrphanTransmitter`) e nos transmissores são
//! métodos do [`TransmitterNetworkRegistry`], que é quem tem a rede, o conjunto `networks` e o dono dos transmissores.
use super::network_acceptors::NetworkAcceptorCache;
use super::registry::{TransmitterNetworkRegistry, set_transmitter_network};
use super::validator::CompatibleTransmitterValidator;
use super::{
    DimensionId, Hashed, LongKey, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::sim::open_hash::{OpenHashMap, OpenHashSet};

/// Os dados de topologia de uma `DynamicNetwork`.
pub struct DynamicNetwork<A> {
    pub(crate) uuid: Uuid,
    /// `world`: a dimensão, posta pelo primeiro `commit` que leva um transmissor válido.
    pub(crate) dimension: Option<DimensionId>,
    /// `positionedTransmitters`: `Long2ObjectOpenHashMap` por `BlockPos.asLong`.
    pub(crate) positioned: OpenHashMap<LongKey, TransmitterId>,
    /// `transmittersToAdd`.
    pub(crate) transmitters_to_add: OpenHashSet<Hashed<TransmitterId>>,
    pub(crate) acceptor_cache: NetworkAcceptorCache<A>,
    /// `transmitterValidator`, guardado até o `commit`.
    pub(crate) validator: Option<Box<dyn CompatibleTransmitterValidator<A>>>,
    pub(crate) registered: bool,
}

impl<A: Clone> DynamicNetwork<A> {
    pub(crate) fn new(uuid: Uuid) -> Self {
        Self {
            uuid,
            dimension: None,
            positioned: OpenHashMap::new(),
            transmitters_to_add: OpenHashSet::new(),
            acceptor_cache: NetworkAcceptorCache::default(),
            validator: None,
            registered: false,
        }
    }

    /// `getUUID`.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// `getWorld`.
    pub fn dimension(&self) -> Option<DimensionId> {
        self.dimension
    }

    /// Se a rede está no conjunto `networks` do registro.
    pub fn is_registered(&self) -> bool {
        self.registered
    }

    /// `getTransmitters()`: os transmissores na ordem do `Long2ObjectOpenHashMap`.
    pub fn transmitters(&self) -> Vec<TransmitterId> {
        self.positioned
            .entries_in_order()
            .into_iter()
            .map(|(_, transmitter)| *transmitter)
            .collect()
    }

    /// `transmittersSize()`.
    pub fn transmitters_size(&self) -> usize {
        self.positioned.len()
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.positioned.is_empty()
    }

    /// `getTransmitter(pos)`.
    pub fn transmitter_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.positioned.get(LongKey(pos.as_long())).copied()
    }

    /// Os transmissores que esperam o `commit`, na ordem do conjunto.
    pub fn transmitters_to_add(&self) -> Vec<TransmitterId> {
        self.transmitters_to_add
            .in_order()
            .into_iter()
            .map(|key| key.id)
            .collect()
    }

    /// `acceptorCache.getAcceptorValues()` achatado: os aceitadores na ordem da tabela e, em cada posição, na do `Direction`.
    pub fn acceptor_values(&self) -> Vec<A> {
        self.acceptor_cache.acceptor_values()
    }

    /// `getAcceptorCount()`.
    pub fn acceptor_count(&self) -> usize {
        self.acceptor_cache.acceptor_count()
    }

    /// `hasAcceptor(pos)`.
    pub fn has_acceptor(&self, pos: BlockPos) -> bool {
        self.acceptor_cache.has_acceptor(pos)
    }

    /// `getCachedAcceptor(pos, side)`.
    pub fn cached_acceptor(&self, pos: BlockPos, side: Face) -> Option<&A> {
        self.acceptor_cache.cached_acceptor(pos, side)
    }

    /// `getAcceptorDirections(pos)`.
    pub fn acceptor_directions(&self, pos: BlockPos) -> Vec<Face> {
        self.acceptor_cache.acceptor_directions(pos)
    }

    /// `getTransmitterValidator()`.
    pub fn validator(&self) -> Option<&dyn CompatibleTransmitterValidator<A>> {
        self.validator.as_deref()
    }
}

impl<A: Clone + 'static> TransmitterNetworkRegistry<A> {
    /// `new Network(Collection<Network>)` (`this(UUID.randomUUID()); adoptAllAndRegister(networks)`), o que
    /// `Transmitter.createNetworkByMerging` faz. A rede nova adota as dadas e se registra; sem nenhuma, nasce vazia.
    pub fn create_network_by_merging(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        networks: &[NetworkId],
    ) -> NetworkId {
        let uuid = self.next_uuid();
        let network = self.push_network(DynamicNetwork::new(uuid));
        host.network_event(&*self, network, NetworkEvent::Created { uuid });
        self.adopt_all_and_register(host, network, networks);
        host.network_event(
            &*self,
            network,
            NetworkEvent::Merged {
                from: networks.to_vec(),
            },
        );
        network
    }

    /// `adoptAllAndRegister(networks)`.
    fn adopt_all_and_register(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        into: NetworkId,
        networks: &[NetworkId],
    ) {
        let mut transmitters_to_update = Vec::new();
        for &network in networks {
            transmitters_to_update
                .extend(self.adopt_transmitters_and_acceptors_from(host, into, network));
            self.deregister_network(host, network);
        }
        self.register_network(into);
        for transmitter in transmitters_to_update {
            host.request_update(transmitter);
        }
    }

    /// `adoptTransmittersAndAcceptorsFrom(net)`: devolve os transmissores que mudaram de rede e esperam o `requestsUpdate`.
    pub fn adopt_transmitters_and_acceptors_from(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        into: NetworkId,
        from: NetworkId,
    ) -> Vec<TransmitterId> {
        let (entries, to_add, acceptors) = {
            let other = self.network(from);
            (
                other
                    .positioned
                    .entries_in_order()
                    .into_iter()
                    .map(|(key, transmitter)| (key, *transmitter))
                    .collect::<Vec<_>>(),
                other.transmitters_to_add.in_order(),
                other.acceptor_cache.clone(),
            )
        };
        // `positionedTransmitters.putAll(net.positionedTransmitters)`, e depois o laço que põe de novo e troca a rede de cada um.
        self.network_mut(into).positioned.put_all(entries.clone());
        let mut transmitters_to_update = Vec::new();
        for (key, transmitter) in entries {
            self.network_mut(into).positioned.insert(key, transmitter);
            if set_transmitter_network(host, transmitter, Some(into), false) {
                transmitters_to_update.push(transmitter);
            }
        }
        let mine = self.network_mut(into);
        mine.transmitters_to_add.add_all(&to_add);
        mine.acceptor_cache.adopt_acceptors(&acceptors);
        host.network_event(&*self, into, NetworkEvent::Adopted { from });
        transmitters_to_update
    }

    /// `addNewTransmitters(newTransmitters, validator)`.
    pub fn add_new_transmitters(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        network: NetworkId,
        new_transmitters: &[TransmitterId],
        validator: Box<dyn CompatibleTransmitterValidator<A>>,
    ) {
        let keys: Vec<Hashed<TransmitterId>> = new_transmitters
            .iter()
            .map(|transmitter| Self::transmitter_key(&*host, *transmitter))
            .collect();
        let network_data = self.network_mut(network);
        network_data.transmitters_to_add.add_all(&keys);
        // Fica na rede até o `commit`, para que órfãos dos dois lados de uma rede existente, que guardam conteúdo, não se juntem se não
        // puderem se ligar.
        network_data.validator = Some(validator);
        host.network_event(&*self, network, NetworkEvent::NewTransmitters);
    }

    /// `commit()`.
    pub fn commit_network(&mut self, host: &mut dyn TransmitterHost<A>, network: NetworkId) {
        host.network_event(&*self, network, NetworkEvent::Committing);
        let to_add = self.network(network).transmitters_to_add();
        if !to_add.is_empty() {
            let mut added_valid_transmitters = false;
            let mut transmitters_to_update = Vec::new();
            for transmitter in to_add {
                if host.is_valid(transmitter) {
                    added_valid_transmitters = true;
                    if self.network(network).dimension.is_none() {
                        self.network_mut(network).dimension = Some(host.dimension(transmitter));
                    }
                    for side in Face::ALL {
                        self.network_mut(network)
                            .acceptor_cache
                            .update_transmitter_on_side(host, transmitter, side);
                    }
                    if set_transmitter_network(host, transmitter, Some(network), false) {
                        transmitters_to_update.push(transmitter);
                    }
                    // `addTransmitterFromCommit`
                    self.network_mut(network)
                        .positioned
                        .insert(LongKey(host.block_pos(transmitter).as_long()), transmitter);
                    host.network_event(
                        &*self,
                        network,
                        NetworkEvent::TransmitterCommitted(transmitter),
                    );
                }
            }
            self.network_mut(network).transmitters_to_add.clear();
            if added_valid_transmitters {
                host.network_event(&*self, network, NetworkEvent::ValidTransmittersAdded);
                for transmitter in transmitters_to_update {
                    host.request_update(transmitter);
                }
            }
        }
        self.network_mut(network).acceptor_cache.commit(host);
        self.network_mut(network).validator = None;
        host.network_event(&*self, network, NetworkEvent::Committed);
    }

    /// `invalidate(triggerTransmitter)`: tira os inválidos, devolve os válidos como órfãos e desfaz a rede.
    pub fn invalidate_network(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        network: NetworkId,
        trigger: Option<TransmitterId>,
    ) {
        host.network_event(&*self, network, NetworkEvent::Invalidating(trigger));
        if let Some(trigger) = trigger
            && self.network(network).transmitters_size() == 1
            && !host.is_valid(trigger)
        {
            // Está destruindo o último transmissor da rede (e não só mudando a sensibilidade ao redstone).
            host.network_event(
                &*self,
                network,
                NetworkEvent::LastTransmitterRemoved(trigger),
            );
        }
        // `removeInvalid`: `getTransmitters().removeIf(transmitter -> !transmitter.isValid())`.
        let invalid: Vec<LongKey> = self
            .network(network)
            .positioned
            .entries_in_order()
            .into_iter()
            .filter(|(_, transmitter)| !host.is_valid(**transmitter))
            .map(|(key, _)| key)
            .collect();
        for key in invalid {
            self.network_mut(network).positioned.remove(key);
        }
        host.network_event(&*self, network, NetworkEvent::InvalidRemoved(trigger));
        // Agora os transmissores que sobraram.
        for transmitter in self.network(network).transmitters() {
            if host.is_valid(transmitter) {
                host.take_share(transmitter);
                set_transmitter_network(host, transmitter, None, true);
                self.register_orphan_transmitter(&*host, transmitter);
            }
        }
        self.deregister_network(host, network);
    }

    /// `acceptorChanged(transmitter, side)`: marca o lado e põe a rede do transmissor entre as que o `commitChanges` confirma. O Java
    /// pede `getTransmitterNetworkNN()`, então um transmissor sem rede é um erro de quem chama.
    pub fn network_acceptor_changed(
        &mut self,
        host: &dyn TransmitterHost<A>,
        network: NetworkId,
        transmitter: TransmitterId,
        side: Face,
    ) {
        let key = Self::transmitter_key(host, transmitter);
        self.network_mut(network)
            .acceptor_cache
            .mark_changed(key, side);
        let transmitter_network = host
            .transmitter_network(transmitter)
            .expect("getTransmitterNetworkNN: o transmissor tem de estar numa rede");
        self.register_changed_network(transmitter_network);
    }

    /// `deregister()`: esvazia a rede e a tira do registro.
    pub fn deregister_network(&mut self, host: &mut dyn TransmitterHost<A>, network: NetworkId) {
        let data = self.network_mut(network);
        data.positioned.clear_keeping_table();
        data.transmitters_to_add.clear();
        data.acceptor_cache.deregister();
        data.validator = None;
        self.remove_network(network);
        host.network_event(&*self, network, NetworkEvent::Deregistered);
    }

    /// `addTransmitter(transmitter)`.
    pub fn add_transmitter(
        &mut self,
        host: &dyn TransmitterHost<A>,
        network: NetworkId,
        transmitter: TransmitterId,
    ) {
        self.network_mut(network)
            .positioned
            .insert(LongKey(host.block_pos(transmitter).as_long()), transmitter);
    }

    /// `removeTransmitter(transmitter)`: tira o transmissor e desfaz a rede se esvaziou.
    pub fn remove_transmitter(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        network: NetworkId,
        transmitter: TransmitterId,
    ) {
        let pos = host.block_pos(transmitter);
        let key = LongKey(pos.as_long());
        if let Some(current) = self.network(network).positioned.get(key).copied() {
            if current != transmitter {
                self.log(format!(
                    "Removed transmitter at position: {pos:?} in {:?} was different than expected.",
                    host.dimension(transmitter)
                ));
            }
            self.network_mut(network).positioned.remove(key);
        }
        if self.network(network).is_empty() {
            self.deregister_network(host, network);
        }
    }
}
