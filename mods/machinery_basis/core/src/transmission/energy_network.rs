//! `EnergyNetwork` (`content/network/EnergyNetwork.java`) e a ligação dela à rede genérica: [`EnergyFamily`] guarda a energia de cada rede e de
//! cada cabo e responde aos [`NetworkEvent`] nos pontos em que o Java sobrescreve o método (`DynamicBufferedNetwork` e `EnergyNetwork`).
//!
//! O dono dos transmissores (o jogo, ou o mundo simulado de um teste) repassa a ela o que o registro lhe diz: `network_event`, `on_update`,
//! `take_share` e `validate_and_take_share`. O aceitador `A` é o `Rc<H>` de um `EnergyHandler` ([`EnergyInsert`]); a divisão da energia
//! entre eles e a volta dela aos cabos são da `distribution`.
//!
//! # O que difere do Java de propósito
//!
//! - `DynamicNetwork.invalidate` abre uma transação raiz em volta de todos os `takeShare`; aqui cada `take_share` abre a sua. O estado
//!   que resulta é o mesmo (a energia da rede e o buffer de cada cabo), só os avisos de mudança vêm um a um.
//! - `getPacketRange`, o cliente (`clientNetworks`, o pacote em si) e o `EnergyTransferEvent` como evento do NeoForge: o evento fica numa
//!   lista ([`EnergyFamily::take_transfer_events`]) para quem liga ao jogo mandar o `PacketNetworkScale`.
use super::buffered_network::{BufferedNetwork, chunk_of};
use super::energy_container::{BufferEnergyContainer, add_clamped, always};
use super::registry::TransmitterNetworkRegistry;
use super::universal_cable::UniversalCable;
use super::{NetworkEvent, NetworkId, TransmitterId};
use crate::distribution::{
    EnergyInsert, EnergyTransmitterSaveKind, EnergyTransmitterSaveTarget, emit, send_to_acceptors,
};
use crate::sim::chunks::BlockPos;
use crate::transaction::Transaction;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Uma `EnergyNetwork`.
pub struct EnergyNetwork {
    pub(crate) buffered: BufferedNetwork,
    /// `energyContainer`: `VariableCapacityEnergyContainer.create(this::getCapacity, alwaysTrue, alwaysTrue, this)`.
    pub energy: BufferEnergyContainer,
    /// `prevTransferAmount`.
    pub(crate) prev_transfer_amount: i64,
}

impl EnergyNetwork {
    fn new() -> Self {
        Self {
            buffered: BufferedNetwork::default(),
            energy: BufferEnergyContainer::new(0, always, always, None),
            prev_transfer_amount: 0,
        }
    }

    /// `getCapacity()`.
    pub fn capacity(&self) -> i64 {
        self.buffered.capacity()
    }

    /// `getBuffer()`.
    pub fn buffer(&self) -> i64 {
        self.energy.amount()
    }

    /// `currentScale`.
    pub fn current_scale(&self) -> f32 {
        self.buffered.current_scale
    }

    /// `prevTransferAmount`.
    pub fn prev_transfer_amount(&self) -> i64 {
        self.prev_transfer_amount
    }

    /// O `getCapacity` mudou: o contêiner (de capacidade variável) lê o valor novo.
    fn sync_capacity(&self) {
        self.energy.set_capacity(self.buffered.capacity());
    }

    /// `forceScaleUpdate()`.
    fn force_scale_update(&mut self) {
        let capacity = self.capacity();
        if !self.energy.is_empty() && capacity > 0 {
            let ratio = self.energy.amount() as f64 / capacity as f64;
            self.buffered.set_forced_scale(ratio.min(1.0) as f32);
        } else {
            self.buffered.set_forced_scale(0.0);
        }
    }

    /// `computeContentScale()`.
    fn compute_content_scale(&self) -> f32 {
        let scale = self.energy.divide_to_level() as f32;
        let mut ret = self.buffered.current_scale.max(scale);
        if self.prev_transfer_amount != 0 && ret < 1.0 {
            ret = (ret + 0.02_f32).min(1.0);
        } else if self.prev_transfer_amount == 0 && ret > 0.0 {
            ret = scale.max(ret - 0.02_f32);
        }
        ret
    }
}

/// Um cabo da família: o estado de energia e a posição (para o chunk da rede).
struct Cable {
    pos: BlockPos,
    state: UniversalCable,
}

/// As redes de energia e os cabos de um dono de transmissores.
pub struct EnergyFamily<H: EnergyInsert + ?Sized> {
    networks: BTreeMap<NetworkId, EnergyNetwork>,
    cables: BTreeMap<TransmitterId, Cable>,
    /// `world.getGameTime()`: o dono o atualiza a cada tick, antes de chamar a rede.
    pub game_time: i64,
    transfer_events: Vec<(NetworkId, f32)>,
    dirty_chunks: Vec<i64>,
    _handler: std::marker::PhantomData<fn(&H)>,
}

impl<H: EnergyInsert + ?Sized> Default for EnergyFamily<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: EnergyInsert + ?Sized> EnergyFamily<H> {
    pub fn new() -> Self {
        Self {
            networks: BTreeMap::new(),
            cables: BTreeMap::new(),
            game_time: 0,
            transfer_events: Vec::new(),
            dirty_chunks: Vec::new(),
            _handler: std::marker::PhantomData,
        }
    }

    /// Um cabo novo: `new UniversalCable(blockProvider, tile)`, com a capacidade do tier.
    pub fn add_cable(&mut self, transmitter: TransmitterId, pos: BlockPos, capacity: i64) {
        self.cables.insert(
            transmitter,
            Cable {
                pos,
                state: UniversalCable::new(capacity),
            },
        );
    }

    /// O estado de energia de um cabo.
    pub fn cable(&self, transmitter: TransmitterId) -> &UniversalCable {
        &self.cables[&transmitter].state
    }

    /// A rede de energia que o índice da rede genérica tem (ela nasce no `Created`).
    pub fn network(&self, network: NetworkId) -> Option<&EnergyNetwork> {
        self.networks.get(&network)
    }

    /// Os `EnergyTransferEvent` desde a última chamada (a rede, a escala): o `PacketNetworkScale` do jogo.
    pub fn take_transfer_events(&mut self) -> Vec<(NetworkId, f32)> {
        std::mem::take(&mut self.transfer_events)
    }

    /// Os chunks que `markChunkDirty` marcou desde a última chamada.
    pub fn take_dirty_chunks(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.dirty_chunks)
    }

    fn net(&mut self, network: NetworkId) -> &mut EnergyNetwork {
        self.networks
            .get_mut(&network)
            .expect("a rede de energia nasce no evento Created")
    }

    /// `markDirty()` depois de uma mudança do contêiner da rede (`onContentsChanged`).
    fn mark_dirty(&mut self, network: NetworkId) {
        let time = self.game_time;
        let chunks = self.net(network).buffered.chunks_to_mark_dirty(time);
        self.dirty_chunks.extend(chunks);
    }

    /// Passa a um `update_capacity` o conjunto de transmissores da rede.
    fn update_capacity<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        let capacities: Vec<i64> = registry
            .network(network)
            .transmitters()
            .iter()
            .map(|transmitter| self.cables[transmitter].state.capacity())
            .collect();
        let net = self.net(network);
        net.buffered.update_capacity(capacities);
        net.sync_capacity();
    }

    /// Reage a um [`NetworkEvent`] nos pontos em que `DynamicBufferedNetwork` e `EnergyNetwork` sobrescrevem o método.
    pub fn network_event<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        event: NetworkEvent,
    ) {
        match event {
            NetworkEvent::Created { .. } => {
                self.networks.insert(network, EnergyNetwork::new());
            }
            NetworkEvent::Adopted { from } => self.adopted(registry, network, from),
            NetworkEvent::NewTransmitters => {
                let empty = registry.network(network).is_empty();
                self.net(network).buffered.new_transmitters_added(empty);
            }
            NetworkEvent::TransmitterCommitted(transmitter) => {
                let (pos, capacity) = {
                    let cable = &self.cables[&transmitter];
                    (cable.pos, cable.state.capacity())
                };
                let chunk = chunk_of(pos.x, pos.z);
                let net = self.net(network);
                net.buffered.transmitter_committed(chunk, capacity);
                net.sync_capacity();
                // `absorbBuffer(transmitter, transaction)`, numa transação raiz.
                let energy = self.cables[&transmitter].state.release_share();
                if energy > 0 {
                    let transaction = Transaction::open_root();
                    let net = self.net(network);
                    net.energy
                        .set_energy(add_clamped(net.energy.amount(), energy), Some(&transaction));
                    transaction.commit();
                    self.mark_dirty(network);
                }
            }
            NetworkEvent::ValidTransmittersAdded => {
                let net = self.net(network);
                net.energy.clamp_contents(true, None);
                if net.buffered.valid_transmitters_added() {
                    net.force_scale_update();
                }
            }
            NetworkEvent::InvalidRemoved(trigger) => {
                self.net(network).energy.clamp_contents(true, None);
                let transaction = Transaction::open_root();
                self.update_save_shares(registry, network, trigger, &transaction);
                transaction.commit();
            }
            NetworkEvent::Deregistered => {
                if let Some(net) = self.networks.get_mut(&network) {
                    net.buffered.deregistered();
                }
            }
            NetworkEvent::Merged { .. }
            | NetworkEvent::Committing
            | NetworkEvent::Invalidating(_)
            | NetworkEvent::Committed
            | NetworkEvent::LastTransmitterRemoved(_) => {}
        }
    }

    /// `EnergyNetwork.adoptTransmittersAndAcceptorsFrom(net)`, depois do `super` (que já somou os chunks e recalculou a capacidade).
    fn adopted<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        from: NetworkId,
    ) {
        let old_capacity = self.networks[&network].capacity();
        let (their_capacity, their_scale, their_chunks) = {
            let other = &self.networks[&from];
            (
                other.capacity(),
                other.current_scale(),
                other.buffered.chunks().collect::<Vec<_>>(),
            )
        };
        // `DynamicBufferedNetwork`: `chunks.addAll(net.chunks)` e `updateCapacity()`.
        self.net(network).buffered.adopt_chunks(their_chunks);
        self.update_capacity(registry, network);
        // Soma as escalas.
        let net = self.net(network);
        let our_scale = if net.buffered.current_scale == 0.0 {
            0_i64
        } else {
            (old_capacity as f32 * net.buffered.current_scale) as i64
        };
        let their_scale_amount = if their_scale == 0.0 {
            0_i64
        } else {
            (their_capacity as f32 * their_scale) as i64
        };
        let capacity = net.capacity();
        let merged = if capacity == 0 {
            0.0
        } else {
            (our_scale.wrapping_add(their_scale_amount)) as f64 / capacity as f64
        };
        net.buffered.set_forced_scale(merged.min(1.0) as f32);
        // `!isClientSide() && !net.energyContainer.isEmpty()`: a energia da outra rede passa para esta.
        let their_energy = self.networks[&from].energy.amount();
        if their_energy != 0 {
            let net = self.net(network);
            net.energy
                .set_energy(add_clamped(net.energy.amount(), their_energy), None);
            self.net(from).energy.set_energy(0, None);
            self.mark_dirty(network);
            self.mark_dirty(from);
        }
    }

    /// `EnergyNetwork.updateSaveShares(trigger, transaction)`.
    fn update_save_shares<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        _trigger: Option<TransmitterId>,
        transaction: &Transaction,
    ) {
        if registry.network(network).is_empty() {
            return;
        }
        let mut target = EnergyTransmitterSaveTarget::new(EnergyTransmitterSaveKind::new());
        for transmitter in registry.network(network).transmitters() {
            target.add_handler(
                self.cables[&transmitter]
                    .state
                    .start_new_save_share(transaction),
            );
        }
        let amount = self.net(network).energy.amount();
        send_to_acceptors(Some(&mut target), amount, &(), Some(transaction));
    }

    /// `validateSaveShares(trigger, transaction)`: uma vez por tick do jogo, repartir a energia da rede entre os cabos.
    pub fn validate_save_shares<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        trigger: TransmitterId,
        transaction: Option<&Transaction>,
    ) {
        let time = self.game_time;
        if self.net(network).buffered.should_validate_save_shares(time) {
            let sub = Transaction::open(transaction);
            self.update_save_shares(registry, network, Some(trigger), &sub);
            sub.commit();
        }
    }

    /// `UniversalCable.takeShare(transaction)`: o cabo tira a parte dele da energia da rede e a põe no buffer.
    pub fn take_share(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        transaction: Option<&Transaction>,
    ) {
        let Some(network) = network else {
            return;
        };
        let share = self.cables[&transmitter].state.save_share_amount();
        let net = self.net(network);
        if !net.energy.is_empty() && share > 0 {
            net.energy
                .set_energy(net.energy.amount() - share, transaction);
            self.cables[&transmitter]
                .state
                .buffer
                .set_energy(share, transaction);
            self.mark_dirty(network);
        }
    }

    /// `DynamicNetwork.invalidate`, o `takeShare` de cada cabo válido: numa transação raiz por cabo (veja o topo do módulo).
    pub fn take_share_in_root(&mut self, transmitter: TransmitterId, network: Option<NetworkId>) {
        let transaction = Transaction::open_root();
        self.take_share(transmitter, network, Some(&transaction));
        transaction.commit();
    }

    /// `BufferedTransmitter.validateAndTakeShare(transaction)`.
    pub fn validate_and_take_share<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        transaction: Option<&Transaction>,
    ) {
        if let Some(network) = network {
            self.validate_save_shares(registry, network, transmitter, transaction);
        }
        self.take_share(transmitter, network, transaction);
    }

    /// `UniversalCable.write`, a parte de energia: valida as shares (sem transação) e devolve o `saveShare` que vai para o NBT.
    pub fn write_energy<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> i64 {
        if let Some(network) = network {
            self.validate_save_shares(registry, network, transmitter, None);
        }
        self.cables[&transmitter].state.save_share_amount()
    }

    /// `DynamicNetwork.onUpdate()`, no `ServerTickEvent.Post`: a escala, o evento e o `tickEmit`.
    pub fn on_update<A: Clone + 'static + Into<Rc<H>>>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        // `DynamicBufferedNetwork.onUpdate`.
        let scale = self.net(network).compute_content_scale();
        let net = self.net(network);
        net.buffered.update_scale(scale);
        if net.buffered.needs_update {
            let scale = net.buffered.current_scale;
            net.buffered.needs_update = false;
            self.transfer_events.push((network, scale));
        }
        self.tick_emit(registry, network);
    }

    /// `tickEmit()`.
    fn tick_emit<A: Clone + 'static + Into<Rc<H>>>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        if self.net(network).energy.is_empty() {
            self.net(network).prev_transfer_amount = 0;
            return;
        }
        let transaction = Transaction::open_root();
        let current = self.net(network).energy.amount();
        let targets: Vec<Rc<H>> = registry
            .network(network)
            .acceptor_values()
            .into_iter()
            .map(Into::into)
            .collect();
        let sent = if targets.is_empty() {
            0
        } else {
            emit(targets, current, Some(&transaction))
        };
        let net = self.net(network);
        net.prev_transfer_amount = sent;
        net.energy.set_energy(current - sent, Some(&transaction));
        transaction.commit();
        self.mark_dirty(network);
    }

    /// `UniversalCable.getBufferWithFallback()`.
    pub fn buffer_with_fallback(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> i64 {
        let buffer = self.cables[&transmitter].state.share();
        match network {
            Some(network) if buffer == 0 => self.networks[&network].buffer(),
            _ => buffer,
        }
    }
}
