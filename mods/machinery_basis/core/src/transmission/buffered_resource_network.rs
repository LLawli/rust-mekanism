//! `DynamicBufferedResourceNetwork` (`lib/transmitter/DynamicBufferedResourceNetwork.java`) e a ligação dela à rede genérica:
//! [`ResourceFamily`] guarda o recurso de cada rede e de cada transmissor com buffer (o cano de fluido, o tubo de químico) e responde
//! aos [`NetworkEvent`] nos pontos em que o Java sobrescreve o método. É genérica sobre o recurso `R`: a `FluidNetwork` usa
//! `ResourceFamily<FluidResource, _>` e a `ChemicalNetwork` usará a mesma base com o recurso dela.
//!
//! O dono dos transmissores (o jogo, ou o mundo simulado de um teste) repassa a ela o que o registro lhe diz: `network_event`, `on_update`,
//! `take_share` e `validate_and_take_share`, como na `EnergyFamily`. Aqui há mais três pontos que só os transmissores com buffer de
//! recurso têm:
//!
//! - a regra `isValidTransmitter` pelo conteúdo ([`ResourceFamily::is_valid_transmitter`]), que o dono liga a
//!   `TransmitterWorld::extra_valid_transmitter`, e o validador de órfãos ([`CompatibleResourceValidator`]), que compara o recurso do
//!   buffer (o do transmissor, ou o da rede se o dele está vazio);
//! - a fusão manual de `recheckConnectionPrechecked` ([`ResourceFamily::plan_merge`] decide, [`super::buffered_transmitter::merge_networks_manually`]
//!   funde e [`ResourceFamily::clamp_buffer`] apara), em passos separados porque o registro chama o dono, que guarda a família;
//! - o `pullFromAcceptors` ([`ResourceFamily::pull_from_acceptors`]).
//!
//! # O que difere do Java de propósito
//!
//! - `onContentsChanged` da rede não é um chamado na hora: o contêiner da rede guarda o que ele guardava a cada aviso
//!   ([`ResourceNetwork::take_pending`]) e a família os processa em ordem no fim de cada chamada dela (`flush_contents`). O resultado
//!   (o `lastType`, o `needsUpdate`, o `markDirty`) é o mesmo, porque o aviso nunca depende do que acontece entre a mudança e o fim da
//!   chamada.
//! - `DynamicNetwork.invalidate` abre uma transação raiz em volta de todos os `takeShare`; aqui cada `take_share_in_root` abre a sua
//!   (a mesma diferença da `EnergyFamily`).
//! - `disperse(trigger, resource, amount)`, o gancho que `updateSaveShares` e `onLastTransmitterRemoved` chamam, é vazio no original
//!   para o fluido e despeja radiação no de químico. A família o registra como [`Dispersal`] quando [`ResourceFamily::with_dispersal`] o
//!   liga (a de químico), e quem a usa lê [`ResourceFamily::take_dispersals`]: a radiação em si não é da lib.
//! - o cliente (`isClientSide`, `handleContentsUpdateTag`, `getPacketRange`, o pacote em si) e os textos (`getStoredInfo`,
//!   `getFlowInfo`, `getNeededInfo`): o `TransferEvent` fica numa lista ([`ResourceFamily::take_transfer_events`]) para quem liga ao
//!   jogo mandar o `PacketNetworkScale`.
use super::buffered_network::{BufferedNetwork, chunk_of};
use super::buffered_resource_transmitter::{
    ResourcePipe, ResourceTransmitterUpgradeData, SaveShare,
};
use super::buffered_transmitter::{Ends, primary_and_secondary};
use super::energy_container::add_clamped;
use super::registry::TransmitterNetworkRegistry;
use super::validator::CompatibleTransmitterValidator;
use super::{NetworkEvent, NetworkId, TransmitterHost, TransmitterId};
use crate::containers::AutomationType;
use crate::distribution::{
    ResourceHandlerKind, ResourceInsert, ResourceTransmitterSaveKind, Target, saturated_cast,
    send_to_acceptors,
};
use crate::energy_cube::scale_changed;
use crate::resource_tank::{Resource, ResourceTank, StackContents};
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::transaction::Transaction;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// O que `ResourceUtils.getTypeToExtract` e `pullFromAcceptors` pedem a um aceitador além de inserir: a lista de recursos que ele guarda
/// (`ResourceHandler.size`, `getResource(index)`, `getAmountAsInt(index)`) e `extract`.
pub trait ResourceExtract<R> {
    /// `ResourceHandler.size`.
    fn size(&self) -> usize;
    /// `getResource(index)`: `None` é o vazio.
    fn resource_at(&self, index: usize) -> Option<R>;
    /// `getAmountAsInt(index)`.
    fn amount_as_int_at(&self, index: usize) -> i32;
    /// `extract(index, resource, amount, transaction)`.
    fn extract_at(&self, index: usize, resource: &R, amount: i32, transaction: &Transaction)
    -> i32;
    /// `extract(resource, amount, transaction)`.
    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32;
}

/// O `ResourceHandler<RESOURCE>` que um vizinho do transmissor expõe: insere (a emissão da rede) e extrai (o `pullFromAcceptors`).
pub trait ResourceAcceptor<R>: ResourceInsert<R> + ResourceExtract<R> {}

/// `ResourceHandlerUtil.findExtractableResource(handler, filter, transaction)`: o primeiro recurso que o filtro aceita e que o handler
/// consegue tirar (tentado numa transação filha que não confirma).
pub fn find_extractable_resource<R: Resource, H: ResourceExtract<R> + ?Sized>(
    handler: &H,
    filter: &dyn Fn(&R) -> bool,
    transaction: Option<&Transaction>,
) -> Option<R> {
    let temp = Transaction::open(transaction);
    for index in 0..handler.size() {
        let Some(resource) = handler.resource_at(index) else {
            continue;
        };
        if filter(&resource)
            && handler.extract_at(index, &resource, handler.amount_as_int_at(index), &temp) > 0
        {
            return Some(resource);
        }
    }
    None
}

/// `ResourceUtils.getTypeToExtract(type, handler, filter, transaction)`: com um recurso já escolhido, ele mesmo; sem, o primeiro que o
/// handler consegue tirar.
pub fn type_to_extract<R: Resource, H: ResourceExtract<R> + ?Sized>(
    current: Option<R>,
    handler: &H,
    filter: &dyn Fn(&R) -> bool,
    transaction: Option<&Transaction>,
) -> Option<R> {
    if current.is_none()
        && let Some(found) = find_extractable_resource(handler, filter, transaction)
    {
        return Some(found);
    }
    current
}

/// `ResourceUtils.emit(List<ResourceHandler>, resourceType, resourceAmount, transaction)`: o único destino recebe tudo, vários dividem
/// pelo `ResourceHandlerTarget`.
pub fn emit_resource<H: ResourceInsert<R> + ?Sized, R: ?Sized>(
    targets: Vec<Rc<H>>,
    resource: &R,
    amount: i64,
    transaction: Option<&Transaction>,
) -> i64 {
    if targets.is_empty() || amount <= 0 {
        return 0;
    } else if targets.len() == 1 {
        let sub = Transaction::open(transaction);
        let sent = targets[0].insert(resource, saturated_cast(amount), &sub);
        sub.commit();
        return i64::from(sent);
    }
    let mut target = Target::with_handlers(ResourceHandlerKind::new(), targets);
    send_to_acceptors(Some(&mut target), amount, resource, transaction)
}

/// `Math.max(float, float)`: um NaN em qualquer lado dá NaN (o `f32::max` do Rust o ignora).
fn java_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.max(b)
    }
}

/// `Math.min(float, float)`.
fn java_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.min(b)
    }
}

/// `triggerTransmitter != null && sent < toSend` de `updateSaveShares`: o que não coube nas shares dos transmissores e vai para `disperse`
/// (`toSend - sent`), se há um transmissor que disparou a atualização.
pub fn unsent(trigger: Option<TransmitterId>, to_send: i64, sent: i64) -> Option<i64> {
    (trigger.is_some() && sent < to_send).then_some(to_send - sent)
}

/// O que as redes de recurso expõem aos validadores de órfãos: o contêiner, o `lastType` e o `prevTransferAmount` de cada rede, e o
/// buffer de cada transmissor. Os validadores vivem numa caixa que o registro guarda (sem emprestar a família), então leem por aqui.
#[derive(Clone)]
pub struct NetworkView<R: Resource> {
    container: ResourceTank<R>,
    last_type: Rc<RefCell<Option<R>>>,
    prev_transfer_amount: Rc<Cell<i64>>,
}

struct Views<R: Resource> {
    pipes: BTreeMap<TransmitterId, ResourceTank<R>>,
    networks: BTreeMap<NetworkId, NetworkView<R>>,
}

/// As visões compartilhadas entre a família e os validadores dela.
pub struct SharedViews<R: Resource>(Rc<RefCell<Views<R>>>);

impl<R: Resource> Clone for SharedViews<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<R: Resource> Default for SharedViews<R> {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(Views {
            pipes: BTreeMap::new(),
            networks: BTreeMap::new(),
        })))
    }
}

impl<R: Resource> SharedViews<R> {
    /// `getBufferWithFallback().resource()` de um transmissor: o buffer dele, ou o da rede se o dele está vazio.
    fn buffer_with_fallback(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> Option<R> {
        let views = self.0.borrow();
        let own = views
            .pipes
            .get(&transmitter)
            .and_then(|tank| tank.resource());
        if own.is_none()
            && let Some(view) = network.and_then(|network| views.networks.get(&network))
        {
            return view.container.resource();
        }
        own
    }
}

/// `CompatibleFluidTransmitterValidator` e `CompatibleChemicalTransmitterValidator`: o recurso do transmissor que pediu a busca (o do
/// buffer dele, ou o da rede), e o primeiro que a busca viu, que as redes e os órfãos seguintes têm de igualar.
pub struct CompatibleResourceValidator<R: Resource> {
    buffer: Option<R>,
    views: SharedViews<R>,
}

impl<R: Resource> CompatibleResourceValidator<R> {
    /// `compareBuffers(otherBuffer)`.
    fn compare_buffers(&mut self, other: Option<R>) -> bool {
        if self.buffer.is_none() {
            self.buffer = other;
            return true;
        }
        other.is_none() || self.buffer == other
    }

    /// O recurso guardado até agora.
    pub fn buffer(&self) -> Option<&R> {
        self.buffer.as_ref()
    }
}

impl<R: Resource, A: Clone + 'static> CompatibleTransmitterValidator<A>
    for CompatibleResourceValidator<R>
{
    fn is_network_compatible(
        &mut self,
        _host: &dyn TransmitterHost<A>,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) -> bool {
        let other = match registry
            .network_validator(network)
            .and_then(|validator| validator.as_any().downcast_ref::<Self>())
        {
            Some(validator) => validator.buffer.clone(),
            None => {
                let views = self.views.0.borrow();
                let view = &views.networks[&network];
                let mut other = view.container.resource();
                if other.is_none() && view.prev_transfer_amount.get() > 0 {
                    other = view.last_type.borrow().clone();
                }
                other
            }
        };
        self.compare_buffers(other)
    }

    fn is_transmitter_compatible(
        &mut self,
        host: &dyn TransmitterHost<A>,
        transmitter: TransmitterId,
    ) -> bool {
        let network = host.transmitter_network(transmitter);
        let other = self.views.buffer_with_fallback(transmitter, network);
        self.compare_buffers(other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Uma `DynamicBufferedResourceNetwork`.
pub struct ResourceNetwork<R: Resource> {
    pub(crate) buffered: BufferedNetwork,
    /// `container`: `containerCreator.create(this::getCapacity, alwaysTrueBi, alwaysTrueBi, alwaysTrue, this)`.
    container: ResourceTank<R>,
    /// `lastType`.
    last_type: Rc<RefCell<Option<R>>>,
    /// `prevTransferAmount`.
    prev_transfer_amount: Rc<Cell<i64>>,
    /// O que o contêiner guardava a cada `onContentsChanged` ainda não processado.
    pending: Rc<RefCell<Vec<StackContents<R>>>>,
}

impl<R: Resource> ResourceNetwork<R> {
    fn new() -> Self {
        let pending: Rc<RefCell<Vec<StackContents<R>>>> = Rc::new(RefCell::new(Vec::new()));
        // O ouvinte lê o contêiner por uma referência fraca: o contêiner guarda o ouvinte, e uma forte faria um ciclo.
        let slot: Rc<RefCell<Option<crate::resource_tank::WeakTank<R>>>> =
            Rc::new(RefCell::new(None));
        let listener: crate::transaction::Callback = {
            let pending = pending.clone();
            let slot = slot.clone();
            Rc::new(move || {
                if let Some(tank) = slot.borrow().as_ref().and_then(|weak| weak.upgrade()) {
                    pending.borrow_mut().push(tank.contents());
                }
            })
        };
        let container = ResourceTank::create(
            0,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(|_| true),
            Some(listener),
        );
        *slot.borrow_mut() = Some(container.downgrade());
        Self {
            buffered: BufferedNetwork::default(),
            container,
            last_type: Rc::new(RefCell::new(None)),
            prev_transfer_amount: Rc::new(Cell::new(0)),
            pending,
        }
    }

    fn view(&self) -> NetworkView<R> {
        NetworkView {
            container: self.container.clone(),
            last_type: self.last_type.clone(),
            prev_transfer_amount: self.prev_transfer_amount.clone(),
        }
    }

    /// `getCapacity()`.
    pub fn capacity(&self) -> i64 {
        self.buffered.capacity()
    }

    /// `getContainer()`.
    pub fn container(&self) -> &ResourceTank<R> {
        &self.container
    }

    /// `getBuffer()`: `container.asStack()`.
    pub fn buffer(&self) -> StackContents<R> {
        self.container.contents()
    }

    /// `currentScale`.
    pub fn current_scale(&self) -> f32 {
        self.buffered.current_scale
    }

    /// `getPrevTransferAmount()`.
    pub fn prev_transfer_amount(&self) -> i64 {
        self.prev_transfer_amount.get()
    }

    /// `getLastType()`.
    pub fn last_type(&self) -> Option<R> {
        self.last_type.borrow().clone()
    }

    /// `needsUpdate`.
    pub fn needs_update(&self) -> bool {
        self.buffered.needs_update
    }

    /// Os avisos do contêiner que ainda não foram processados (o que a família processa no fim de cada chamada).
    pub fn take_pending(&self) -> Vec<StackContents<R>> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }

    /// O `getCapacity` mudou: o contêiner (de capacidade variável) lê o valor novo.
    fn sync_capacity(&self) {
        self.container.set_capacity(self.buffered.capacity() as u64);
    }

    /// `forceScaleUpdate()`.
    fn force_scale_update(&mut self) {
        let capacity = self.capacity();
        if !self.container.is_empty() && capacity > 0 {
            let ratio = self.container.amount() as f64 / capacity as f64;
            self.buffered.set_forced_scale(ratio.min(1.0) as f32);
        } else {
            self.buffered.set_forced_scale(0.0);
        }
    }

    /// `computeContentScale()`.
    fn compute_content_scale(&self) -> f32 {
        let scale = (self.container.amount() as f64 / self.capacity() as f64) as f32;
        let mut ret = java_max(self.buffered.current_scale, scale);
        let prev = self.prev_transfer_amount.get();
        if prev > 0 && ret < 1.0 {
            ret = java_min(1.0, ret + 0.02_f32);
        } else if prev <= 0 && ret > 0.0 {
            ret = java_max(scale, ret - 0.02_f32);
        }
        ret
    }

    /// `clampBuffer()`: `ContainerType.clampContents(container, null)`. Com capacidade zero, o contêiner de capacidade variável não é
    /// cortado, porque a rede finge capacidade zero até terminar de se montar.
    fn clamp_buffer(&self) {
        if let Some(resource) = self.container.resource() {
            let capacity = self.container.capacity(&resource);
            if capacity == 0 {
                return;
            }
            if self.container.amount() > capacity {
                self.container.set_contents(Some(resource), capacity, None);
            }
        }
    }

    /// `isCompatibleWith(other)`.
    fn is_compatible_with(&self, other: &Self) -> bool {
        self.container.is_empty()
            || other.container.is_empty()
            || self.container.resource() == other.container.resource()
    }
}

/// O que o `EnergyTransferEvent` da energia é para o recurso: o `FluidTransferEvent` (a rede, a escala e o `lastType`).
#[derive(Clone, Debug, PartialEq)]
pub struct TransferEvent<R> {
    pub network: NetworkId,
    pub scale: f32,
    pub last_type: Option<R>,
}

/// O que `onContentsChanged` de uma rede viu: a rede e o que o contêiner guardava.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentsChanged<R> {
    pub network: NetworkId,
    pub contents: StackContents<R>,
}

/// `disperse(triggerTransmitter, resource, amount)`: o que a rede de químico despeja como radiação, e onde (a posição do transmissor que
/// disparou a remoção).
#[derive(Clone, Debug, PartialEq)]
pub struct Dispersal<R> {
    pub transmitter: TransmitterId,
    pub pos: BlockPos,
    pub resource: R,
    pub amount: i64,
}

/// Um transmissor da família: o estado de buffer e a posição (para o chunk da rede).
struct Pipe<R: Resource> {
    pos: BlockPos,
    state: ResourcePipe<R>,
}

/// O que a fusão manual decidiu: a rede principal e a que ela adota.
pub type MergePlan = (NetworkId, NetworkId);

/// As redes de recurso e os transmissores com buffer de um dono de transmissores.
pub struct ResourceFamily<R: Resource, H: ResourceAcceptor<R> + ?Sized> {
    networks: BTreeMap<NetworkId, ResourceNetwork<R>>,
    pipes: BTreeMap<TransmitterId, Pipe<R>>,
    views: SharedViews<R>,
    /// `world.getGameTime()`: o dono o atualiza a cada tick, antes de chamar a rede.
    pub game_time: i64,
    transfer_events: Vec<TransferEvent<R>>,
    contents_events: Vec<ContentsChanged<R>>,
    dirty_chunks: Vec<i64>,
    /// `Mekanism.logger.error("Incompatible buffed resource networks merged: ...")`.
    errors: Vec<String>,
    /// Os `disperse` desde a última chamada, ou `None` se a família não os registra (o `disperse` do fluido é vazio).
    dispersals: Option<Vec<Dispersal<R>>>,
    _handler: std::marker::PhantomData<fn(&H)>,
}

impl<R: Resource, H: ResourceAcceptor<R> + ?Sized> Default for ResourceFamily<R, H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Resource, H: ResourceAcceptor<R> + ?Sized> ResourceFamily<R, H> {
    pub fn new() -> Self {
        Self {
            networks: BTreeMap::new(),
            pipes: BTreeMap::new(),
            views: SharedViews::default(),
            game_time: 0,
            transfer_events: Vec::new(),
            contents_events: Vec::new(),
            dirty_chunks: Vec::new(),
            errors: Vec::new(),
            dispersals: None,
            _handler: std::marker::PhantomData,
        }
    }

    /// Liga o registro dos `disperse` (a `ChemicalNetwork` sobrescreve o gancho; a `FluidNetwork` não).
    pub fn with_dispersal(mut self) -> Self {
        self.dispersals = Some(Vec::new());
        self
    }

    /// Os `disperse` desde a última chamada, na ordem. Vazio se a família não os registra.
    pub fn take_dispersals(&mut self) -> Vec<Dispersal<R>> {
        self.dispersals
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// `disperse(triggerTransmitter, resource, amount)`.
    fn disperse(&mut self, trigger: TransmitterId, resource: R, amount: i64) {
        let pos = self.pipes[&trigger].pos;
        if let Some(dispersals) = self.dispersals.as_mut() {
            dispersals.push(Dispersal {
                transmitter: trigger,
                pos,
                resource,
                amount,
            });
        }
    }

    /// Um transmissor novo: `new MechanicalPipe(blockProvider, tile)`, com a capacidade e a taxa do tier.
    pub fn add_pipe(
        &mut self,
        transmitter: TransmitterId,
        pos: BlockPos,
        capacity: i64,
        transfer_rate: i32,
    ) {
        let state = ResourcePipe::new(capacity, transfer_rate);
        self.views
            .0
            .borrow_mut()
            .pipes
            .insert(transmitter, state.buffer.clone());
        self.pipes.insert(transmitter, Pipe { pos, state });
    }

    /// O estado de buffer de um transmissor.
    pub fn pipe(&self, transmitter: TransmitterId) -> &ResourcePipe<R> {
        &self.pipes[&transmitter].state
    }

    /// A rede de recurso que o índice da rede genérica tem (ela nasce no `Created`).
    pub fn network(&self, network: NetworkId) -> Option<&ResourceNetwork<R>> {
        self.networks.get(&network)
    }

    /// Os `TransferEvent` desde a última chamada: o `PacketNetworkScale` do jogo.
    pub fn take_transfer_events(&mut self) -> Vec<TransferEvent<R>> {
        std::mem::take(&mut self.transfer_events)
    }

    /// Os `onContentsChanged` das redes desde a última chamada, na ordem (a rede e o que o contêiner guardava).
    pub fn take_contents_events(&mut self) -> Vec<ContentsChanged<R>> {
        std::mem::take(&mut self.contents_events)
    }

    /// Os chunks que `markChunkDirty` marcou desde a última chamada.
    pub fn take_dirty_chunks(&mut self) -> Vec<i64> {
        std::mem::take(&mut self.dirty_chunks)
    }

    /// As mensagens de erro que o Java mandaria ao log (redes incompatíveis fundidas).
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    fn net(&mut self, network: NetworkId) -> &mut ResourceNetwork<R> {
        self.networks
            .get_mut(&network)
            .expect("a rede de recurso nasce no evento Created")
    }

    /// `markDirty()` depois de uma mudança do contêiner da rede (`onContentsChanged`).
    fn mark_dirty(&mut self, network: NetworkId) {
        let time = self.game_time;
        let chunks = self.net(network).buffered.chunks_to_mark_dirty(time);
        self.dirty_chunks.extend(chunks);
    }

    /// `onContentsChanged()` da rede, uma vez por aviso que o contêiner mandou: `markDirty()` e, se o recurso mudou, o `lastType` e o
    /// `needsUpdate`.
    fn process_contents(&mut self, network: NetworkId) {
        for contents in self.net(network).take_pending() {
            self.mark_dirty(network);
            let net = self.net(network);
            if contents.resource != *net.last_type.borrow() {
                // O tipo mudou: guarda-o se há um (esvaziar não o apaga) e pede a atualização.
                if !contents.is_empty() {
                    *net.last_type.borrow_mut() = contents.resource.clone();
                }
                net.buffered.needs_update = true;
            }
            self.contents_events
                .push(ContentsChanged { network, contents });
        }
    }

    /// Processa os avisos pendentes de todas as redes (o que o contêiner de uma rede avisou num commit de raiz que o dono abriu).
    pub fn flush_contents(&mut self) {
        let networks: Vec<NetworkId> = self.networks.keys().copied().collect();
        for network in networks {
            self.process_contents(network);
        }
    }

    /// Passa a `update_capacity` o conjunto de transmissores da rede.
    fn update_capacity<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        let capacities: Vec<i64> = registry
            .network(network)
            .transmitters()
            .iter()
            .map(|transmitter| self.pipes[transmitter].state.capacity())
            .collect();
        let net = self.net(network);
        net.buffered.update_capacity(capacities);
        net.sync_capacity();
    }

    /// Reage a um [`NetworkEvent`] nos pontos em que `DynamicBufferedNetwork` e `DynamicBufferedResourceNetwork` sobrescrevem o método.
    pub fn network_event<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        event: NetworkEvent,
    ) {
        self.flush_contents();
        match event {
            NetworkEvent::Created { .. } => {
                let net = ResourceNetwork::new();
                self.views
                    .0
                    .borrow_mut()
                    .networks
                    .insert(network, net.view());
                self.networks.insert(network, net);
            }
            NetworkEvent::Adopted { from } => self.adopted(registry, network, from),
            NetworkEvent::NewTransmitters => {
                let empty = registry.network(network).is_empty();
                self.net(network).buffered.new_transmitters_added(empty);
            }
            NetworkEvent::TransmitterCommitted(transmitter) => {
                let (pos, capacity) = {
                    let pipe = &self.pipes[&transmitter];
                    (pipe.pos, pipe.state.capacity())
                };
                let chunk = chunk_of(pos.x, pos.z);
                let net = self.net(network);
                net.buffered.transmitter_committed(chunk, capacity);
                net.sync_capacity();
                // `absorbBuffer(transmitter, transaction)`, numa transação raiz.
                let released = self.pipes[&transmitter].state.release_share();
                if !released.is_empty() {
                    let net = self.net(network);
                    if net.container.is_empty() || released.resource == net.container.resource() {
                        let transaction = Transaction::open_root();
                        net.container.set_contents(
                            released.resource.clone(),
                            net.container.amount().saturating_add(released.amount),
                            Some(&transaction),
                        );
                        transaction.commit();
                    }
                }
            }
            NetworkEvent::ValidTransmittersAdded => {
                let net = self.net(network);
                net.clamp_buffer();
                if net.buffered.valid_transmitters_added() {
                    net.force_scale_update();
                }
            }
            NetworkEvent::InvalidRemoved(trigger) => {
                self.net(network).clamp_buffer();
                let transaction = Transaction::open_root();
                self.update_save_shares(registry, network, trigger, &transaction);
                transaction.commit();
            }
            NetworkEvent::Deregistered => {
                if let Some(net) = self.networks.get_mut(&network) {
                    net.buffered.deregistered();
                }
            }
            NetworkEvent::LastTransmitterRemoved(trigger) => {
                // `onLastTransmitterRemoved(trigger)`: o que a rede guarda é despejado.
                let contents = self.net(network).container.contents();
                if let Some(resource) = contents.resource {
                    self.disperse(trigger, resource, contents.amount as i64);
                }
            }
            NetworkEvent::Merged { .. }
            | NetworkEvent::Committing
            | NetworkEvent::Invalidating(_)
            | NetworkEvent::Committed => {}
        }
        self.flush_contents();
    }

    /// `DynamicBufferedResourceNetwork.adoptTransmittersAndAcceptorsFrom(net)`, depois do `super` (`DynamicBufferedNetwork`: os chunks da
    /// outra e `updateCapacity`, que o registro não fez).
    fn adopted<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        from: NetworkId,
    ) {
        let old_scale = self.networks[&network].current_scale();
        let old_capacity = self.networks[&network].capacity();
        let their_chunks: Vec<i64> = self.networks[&from].buffered.chunks().collect();
        self.net(network).buffered.adopt_chunks(their_chunks);
        self.update_capacity(registry, network);
        // Soma as escalas, pelo peso da capacidade de cada rede (em `float`, como o Java).
        let (their_scale, their_capacity) = {
            let other = &self.networks[&from];
            (other.current_scale(), other.capacity())
        };
        let net = self.net(network);
        let capacity = net.capacity();
        let merged = if capacity == 0 {
            0.0_f32
        } else {
            (net.buffered.current_scale * old_capacity as f32 + their_scale * their_capacity as f32)
                / capacity as f32
        };
        net.buffered.set_forced_scale(java_min(1.0, merged));
        // `!isClientSide() && !net.container.isEmpty()`: o recurso da outra rede passa para esta.
        let theirs = self.networks[&from].container.contents();
        if !theirs.is_empty() {
            let mine = self.networks[&network].container.clone();
            if mine.is_empty() {
                mine.set_contents(theirs.resource.clone(), theirs.amount, None);
            } else if mine.resource() == theirs.resource {
                let amount = add_clamped(mine.amount() as i64, theirs.amount as i64) as u64;
                mine.set_contents(theirs.resource.clone(), amount, None);
            } else {
                self.errors.push(format!(
                    "Incompatible buffed resource networks merged: {:?}, {:?}.",
                    mine.resource(),
                    theirs.resource
                ));
            }
            self.networks[&from].container.set_contents(None, 0, None);
        }
        let net = self.net(network);
        if scale_changed(net.buffered.current_scale, old_scale) {
            // Queremos atualizar para a mudança de escala.
            net.buffered.needs_update = true;
        }
        self.process_contents(network);
        self.process_contents(from);
    }

    /// `DynamicBufferedResourceNetwork.updateSaveShares(trigger, transaction)`.
    fn update_save_shares<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
        trigger: Option<TransmitterId>,
        transaction: &Transaction,
    ) {
        if registry.network(network).is_empty() {
            return;
        }
        let mut target = Target::new(ResourceTransmitterSaveKind::<SaveShare<R>, R>::new());
        for transmitter in registry.network(network).transmitters() {
            target.add_handler(
                self.pipes[&transmitter]
                    .state
                    .start_new_save_share(transaction),
            );
        }
        let contents = self.net(network).container.contents();
        if let Some(resource) = contents.resource.as_ref() {
            let to_send = contents.amount as i64;
            let sent = send_to_acceptors(Some(&mut target), to_send, resource, Some(transaction));
            if let (Some(trigger), Some(left)) = (trigger, unsent(trigger, to_send, sent)) {
                self.disperse(trigger, resource.clone(), left);
            }
        }
    }

    /// `validateSaveShares(trigger, transaction)`: uma vez por tick do jogo, repartir o recurso da rede entre os transmissores.
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
        self.flush_contents();
    }

    /// `BufferedResourceTransmitter.takeShare(transaction)`: o transmissor tira a parte dele do recurso da rede e a põe no buffer.
    pub fn take_share(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        transaction: Option<&Transaction>,
    ) {
        let Some(network) = network else {
            return;
        };
        let share = self.pipes[&transmitter].state.save_share_contents();
        let container = self.net(network).container.clone();
        if !container.is_empty() && !share.is_empty() {
            container.set_contents(
                container.resource(),
                container.amount().saturating_sub(share.amount),
                transaction,
            );
            self.pipes[&transmitter].state.buffer.set_contents(
                share.resource,
                share.amount,
                transaction,
            );
        }
        self.flush_contents();
    }

    /// `DynamicNetwork.invalidate`, o `takeShare` de cada transmissor válido: numa transação raiz por transmissor (veja o topo do
    /// módulo).
    pub fn take_share_in_root(&mut self, transmitter: TransmitterId, network: Option<NetworkId>) {
        let transaction = Transaction::open_root();
        self.take_share(transmitter, network, Some(&transaction));
        transaction.commit();
        self.flush_contents();
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

    /// `BufferedResourceTransmitter.write`, a parte do buffer: valida as shares (sem transação) e devolve o `saveShare` que vai para o
    /// NBT (`stored`).
    pub fn write_stored<A: Clone + 'static>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> StackContents<R> {
        if let Some(network) = network {
            self.validate_save_shares(registry, network, transmitter, None);
        }
        self.pipes[&transmitter].state.save_share_contents()
    }

    /// `BufferedResourceTransmitter.read`, a parte do buffer.
    pub fn read_stored(&mut self, transmitter: TransmitterId, stored: StackContents<R>) {
        self.pipes[&transmitter].state.read_stored(stored);
    }

    /// `getUpgradeData()`.
    pub fn upgrade_data(&self, transmitter: TransmitterId) -> ResourceTransmitterUpgradeData<R> {
        self.pipes[&transmitter].state.upgrade_data()
    }

    /// `DynamicNetwork.onUpdate()`, no `ServerTickEvent.Post`: a escala, o evento (`FluidNetwork.onUpdate`) e o `tickEmit`.
    pub fn on_update<A: Clone + 'static + Into<Rc<H>>>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        self.flush_contents();
        // `DynamicBufferedNetwork.onUpdate`.
        let net = self.net(network);
        let scale = net.compute_content_scale();
        net.buffered.update_scale(scale);
        if net.buffered.needs_update {
            let event = TransferEvent {
                network,
                scale: net.buffered.current_scale,
                last_type: net.last_type.borrow().clone(),
            };
            net.buffered.needs_update = false;
            self.transfer_events.push(event);
        }
        self.tick_emit(registry, network);
        self.flush_contents();
    }

    /// `tickEmit()`.
    fn tick_emit<A: Clone + 'static + Into<Rc<H>>>(
        &mut self,
        registry: &TransmitterNetworkRegistry<A>,
        network: NetworkId,
    ) {
        let container = self.net(network).container.clone();
        let Some(resource) = container.resource() else {
            self.net(network).prev_transfer_amount.set(0);
            return;
        };
        let transaction = Transaction::open_root();
        let current = container.amount() as i64;
        let targets: Vec<Rc<H>> = registry
            .network(network)
            .acceptor_values()
            .into_iter()
            .map(Into::into)
            .collect();
        // Os alvos entram sem checar se aceitam: o `sendToAcceptors` calcula isso.
        let sent = emit_resource(targets, &resource, current, Some(&transaction));
        self.net(network).prev_transfer_amount.set(sent);
        container.set_contents(Some(resource), (current - sent) as u64, Some(&transaction));
        transaction.commit();
        self.flush_contents();
    }

    // -----------------------------------------------------------------------------------------------------------------------
    // O transmissor com a rede

    /// `getContainer()`: o contêiner da rede se há uma, o buffer do transmissor se não.
    pub fn container_of(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> ResourceTank<R> {
        match network {
            Some(network) => self.networks[&network].container.clone(),
            None => self.pipes[&transmitter].state.buffer.clone(),
        }
    }

    /// `getBufferWithFallback()`: o buffer do transmissor, ou o da rede se o dele está vazio.
    pub fn buffer_with_fallback(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> StackContents<R> {
        let buffer = self.pipes[&transmitter].state.share();
        match network {
            Some(network) if buffer.is_empty() => self.networks[&network].buffer(),
            _ => buffer,
        }
    }

    /// `noBufferOrFallback()`.
    pub fn no_buffer_or_fallback(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> bool {
        self.buffer_with_fallback(transmitter, network).is_empty()
    }

    /// `getBufferOrFallback()`: o recurso de `getBufferWithFallback()` e, se ainda vazio e a rede moveu algo no último tick, o `lastType`.
    pub fn buffer_or_fallback(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> Option<R> {
        let buffer = self.buffer_with_fallback(transmitter, network).resource;
        if buffer.is_none()
            && let Some(network) = network
            && self.networks[&network].prev_transfer_amount() > 0
        {
            return self.networks[&network].last_type();
        }
        buffer
    }

    /// `PressurizedTube.getRadiationScale(level)`: se o contêiner do tubo (`getContainer()`) guarda um recurso radioativo (`radioactive`, que
    /// vem da radioatividade do químico), quanto dele há para a capacidade (`amountAsLong() / (float) capacityAsLong(resource)`), senão 0.
    pub fn radiation_scale(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        radioactive: &dyn Fn(&R) -> bool,
    ) -> f32 {
        let container = self.container_of(transmitter, network);
        if let Some(resource) = container.resource()
            && radioactive(&resource)
        {
            return container.amount() as f32 / container.capacity(&resource) as f32;
        }
        0.0
    }

    /// `BufferedResourceTransmitter.isValidTransmitter(other)`: o recurso do buffer de um e do outro não diferem.
    pub fn is_valid_transmitter(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        other: TransmitterId,
        other_network: Option<NetworkId>,
    ) -> bool {
        let Some(buffer) = self.buffer_or_fallback(transmitter, network) else {
            return true;
        };
        let other_buffer = self.buffer_or_fallback(other, other_network);
        other_buffer.is_none_or(|other_buffer| other_buffer == buffer)
    }

    /// O que `BufferedTransmitter.isValidTransmitter` lê de cada ponta (`canHaveIncompatibleNetworks` é sempre verdadeiro para um
    /// transmissor com buffer de recurso).
    pub fn ends(&self, has_network: bool, is_orphan: bool) -> Ends {
        Ends {
            can_have_incompatible_networks: true,
            has_network,
            is_orphan,
        }
    }

    /// `getNewOrphanValidator()`: o validador que parte do recurso do buffer deste transmissor.
    pub fn new_orphan_validator(
        &self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) -> CompatibleResourceValidator<R> {
        CompatibleResourceValidator {
            buffer: self.buffer_with_fallback(transmitter, network).resource,
            views: self.views.clone(),
        }
    }

    /// `recheckConnectionPrechecked(side)`, depois de achar a rede do vizinho: se são redes diferentes e compatíveis, quem funde com
    /// quem (`noBufferOrFallback` troca os papéis). O dono então chama [`super::buffered_transmitter::merge_networks_manually`],
    /// [`Self::clamp_buffer`], manda o vizinho reler o lado (`refreshConnections(side.getOpposite())`) e chama `requestsUpdate` dos
    /// adotados.
    pub fn plan_merge(
        &self,
        transmitter: TransmitterId,
        network: NetworkId,
        other_network: NetworkId,
    ) -> Option<MergePlan> {
        if network != other_network
            && self.networks[&network].is_compatible_with(&self.networks[&other_network])
        {
            Some(primary_and_secondary(
                network,
                other_network,
                self.no_buffer_or_fallback(transmitter, Some(network)),
            ))
        } else {
            None
        }
    }

    /// `network.clampBuffer()`.
    pub fn clamp_buffer(&mut self, network: NetworkId) {
        self.net(network).clamp_buffer();
        self.flush_contents();
    }

    /// `BufferedResourceTransmitter.pullFromAcceptors(level)`: `has_pull_side` é o do transmissor, `is_pull(side)` se o lado é `PULL`
    /// e `acceptor(side)` o aceitador conectado do cache (`getConnectedAcceptor`), na ordem dos lados do Java.
    pub fn pull_from_acceptors(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
        has_pull_side: bool,
        is_pull: &dyn Fn(Face) -> bool,
        acceptor: &dyn Fn(Face) -> Option<Rc<H>>,
    ) {
        let rate = self.pipes[&transmitter].state.transfer_rate();
        let container = self.container_of(transmitter, network);
        // `getAvailablePull()`: o menor entre a taxa do tier e o que falta para encher.
        let available = |container: &ResourceTank<R>| {
            rate.min(container.needed_as_int(container.resource().as_ref()))
        };
        if !has_pull_side || available(&container) <= 0 {
            return;
        }
        for side in Face::ALL {
            if !is_pull(side) {
                continue;
            }
            let Some(connected) = acceptor(side) else {
                continue;
            };
            let received =
                type_to_extract(container.resource(), connected.as_ref(), &|_| true, None);
            if let Some(received) = received {
                let transaction = Transaction::open_root();
                let extracted = connected.extract(&received, available(&container), &transaction);
                if extracted > 0
                    && container.insert(
                        &received,
                        extracted,
                        &transaction,
                        AutomationType::Internal,
                    ) == extracted
                {
                    // Primeiro simula o próprio contêiner e só então confirma: um aceitador que deu um valor errado não duplica nada.
                    transaction.commit();
                    if container.is_full() {
                        break;
                    }
                }
            }
        }
        self.flush_contents();
    }
}
