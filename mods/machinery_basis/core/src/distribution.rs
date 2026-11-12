//! A distribuição genérica do Mekanism: `lib/distribution/SplitInfo`, `lib/distribution/Target`, `util/EmitUtils`, os alvos de
//! `content/network/distribution/` e a parte do `EnergyUtils.emit` que usa tudo isso. É a conta que um emissor (o ejetor de
//! energia de um cubo, a rede de cabos ao guardar a energia de volta nos cabos) usa para dividir uma quantidade entre vários
//! destinos, genérica sobre o recurso (energia, fluido, químico).
//!
//! O `Target` roda sobre as transações do NeoForge (`crate::transaction`), como o Java: `sendPossible` consulta cada destino numa
//! transação filha e só confirma a que cabe na parte, `shiftNeeded` e `sendRemainingSplit` inserem de verdade. Um handler que o
//! Horizon não desfaz (o vizinho de um bloco) não pode ser consultado e revertido: ele implementa `probe_insert` como simulação e
//! `commit_probe` como a inserção de verdade, e o `Target` chama o segundo no ponto em que o Java confirma a transação filha.
//!
//! A ordem dos destinos é a de entrada, e `needed` guarda a primeira posição de cada handler (o `Reference2LongLinkedOpenHashMap`
//! do Java, chaveado pelo objeto: o mesmo handler listado duas vezes mantém o lugar da primeira vez e troca só o valor).
use crate::containers::{AutomationType, EnergyContainer};
use crate::energy_cube::CubeEnergy;
use crate::transaction::Transaction;
use std::marker::PhantomData;
use std::rc::Rc;

/// `Ints.saturatedCast`.
pub fn saturated_cast(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

// ---------------------------------------------------------------------------------------------------------------------------
// `SplitInfo`

/// `SplitInfo`: a divisão de uma quantidade entre destinos, com o resto.
#[derive(Debug, Clone)]
pub struct SplitInfo {
    /// `amountPerChanged`: a parte por destino mudou desde a última conferência.
    amount_per_changed: bool,
    /// `toSplitAmong`.
    to_split_among: i32,
    amount_to_split: i64,
    amount_per_target: i64,
    sent_so_far: i64,
    remainder: i64,
}

impl SplitInfo {
    pub fn new(amount_to_split: i64, total_targets: i32) -> Self {
        let among = i64::from(total_targets);
        Self {
            amount_per_changed: false,
            to_split_among: total_targets,
            amount_to_split,
            amount_per_target: if total_targets == 0 {
                0
            } else {
                amount_to_split / among
            },
            sent_so_far: 0,
            remainder: if total_targets == 0 {
                0
            } else {
                amount_to_split % among
            },
        }
    }

    /// `send`: dá a quantidade por enviada. `decrement_targets` tira o destino da conta e refaz a parte.
    pub fn send(&mut self, amount_needed: i64, decrement_targets: bool) {
        self.amount_to_split -= amount_needed;
        self.sent_so_far += amount_needed;
        if !decrement_targets {
            let difference = amount_needed - self.amount_per_target;
            if difference > 0 {
                self.remainder -= difference;
            }
            return;
        }
        self.to_split_among -= 1;
        if amount_needed != self.amount_per_target && self.to_split_among != 0 {
            let among = i64::from(self.to_split_among);
            let last = self.amount_per_target;
            self.amount_per_target = self.amount_to_split / among;
            self.remainder = self.amount_to_split % among;
            if !self.amount_per_changed && self.amount_per_target != last {
                self.amount_per_changed = true;
            }
        }
    }

    /// `getShareAmount`.
    pub fn share_amount(&self) -> i64 {
        self.amount_per_target
    }

    /// `getRemainderAmount`: a parte, mais um enquanto sobra resto.
    pub fn remainder_amount(&self) -> i64 {
        if self.to_split_among != 0 && self.remainder > 0 {
            return self.amount_per_target + 1;
        }
        self.amount_per_target
    }

    /// `getUnsent`.
    pub fn unsent(&self) -> i64 {
        self.amount_to_split
    }

    /// `getTotalSent`.
    pub fn total_sent(&self) -> i64 {
        self.sent_so_far
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// `Target`

/// O que um `Target` concreto define: o handler, o recurso e o `accept`.
pub trait TargetKind {
    type Handler;
    type Resource: ?Sized;

    /// `accept`: insere de verdade na transação dada (desfeita se ela não confirmar).
    fn accept(
        &self,
        handler: &Self::Handler,
        resource: &Self::Resource,
        amount: i64,
        transaction: &Transaction,
    ) -> i64;

    /// A igualdade por referência do `Reference2LongMap`: o mesmo objeto listado duas vezes.
    fn same_handler(&self, a: &Self::Handler, b: &Self::Handler) -> bool;

    /// O `accept` do `sendPossible`, numa transação filha que só confirma se couber na parte. Um handler que não se desfaz
    /// simula aqui.
    fn probe(
        &self,
        handler: &Self::Handler,
        resource: &Self::Resource,
        amount: i64,
        transaction: &Transaction,
    ) -> i64 {
        self.accept(handler, resource, amount, transaction)
    }

    /// Chamado logo antes de a transação filha do `sendPossible` confirmar, com o que a consulta devolveu. Um handler que não
    /// se desfaz faz aqui a inserção de verdade.
    fn commit_probe(
        &self,
        _handler: &Self::Handler,
        _resource: &Self::Resource,
        _accepted: i64,
        _transaction: &Transaction,
    ) {
    }
}

/// `Target`: os destinos de uma emissão e o que cada um quis além da parte (`needed`).
pub struct Target<K: TargetKind> {
    kind: K,
    handlers: Vec<K::Handler>,
    /// `needed`: o índice em `handlers` da primeira vez de cada handler e o que ele pediu, na ordem em que apareceram.
    needed: Vec<(usize, i64)>,
    /// O `Target(Collection)` guarda a coleção sem copiar e não deixa acrescentar.
    fixed: bool,
}

impl<K: TargetKind> Target<K> {
    /// `Target()` e `Target(int expectedSize)`: vazio, para `add_handler`.
    pub fn new(kind: K) -> Self {
        Self {
            kind,
            handlers: Vec::new(),
            needed: Vec::new(),
            fixed: false,
        }
    }

    /// `Target(Collection<HANDLER> allHandlers)`.
    pub fn with_handlers(kind: K, handlers: Vec<K::Handler>) -> Self {
        Self {
            kind,
            handlers,
            needed: Vec::new(),
            fixed: true,
        }
    }

    /// `addHandler`. No `Target(Collection)` a coleção é só de leitura (`UnsupportedOperationException`).
    pub fn add_handler(&mut self, handler: K::Handler) {
        assert!(!self.fixed, "a coleção de handlers é só de leitura");
        self.handlers.push(handler);
    }

    /// `getHandlerCount`.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// `sendToSingularAcceptor`: tudo para o único handler, sem calcular divisão.
    pub fn send_to_singular_acceptor(
        &mut self,
        resource: &K::Resource,
        to_send: i64,
        transaction: Option<&Transaction>,
    ) -> i64 {
        assert!(
            self.handlers.len() == 1,
            "Send to singular acceptor should only be called when there is a single handler"
        );
        let sub = Transaction::open(transaction);
        let accepted = self.kind.accept(&self.handlers[0], resource, to_send, &sub);
        sub.commit();
        accepted
    }

    /// `sendToAcceptors(SplitInfo, ...)`: o total enviado.
    pub fn send_to_acceptors(
        &mut self,
        split: &mut SplitInfo,
        resource: &K::Resource,
        transaction: Option<&Transaction>,
    ) -> i64 {
        if self.handlers.is_empty() {
            return split.total_sent();
        } else if self.handlers.len() == 1 {
            return split.total_sent()
                + self.send_to_singular_acceptor(resource, split.unsent(), transaction);
        }
        let sub = Transaction::open(transaction);
        // Consulta cada destino; o que pede menos que a parte recebe já.
        self.send_possible(resource, split, &sub);
        // Só refaz se a parte por destino mudou desde a última conferência.
        while split.amount_per_changed {
            split.amount_per_changed = false;
            self.shift_needed(resource, split, &sub);
        }
        // Divide o que sobrou entre os que pediram mais que a parte.
        self.send_remaining_split(resource, split, &sub);
        sub.commit();
        split.total_sent()
    }

    /// `needed.put(entry, amountNeeded)`: um handler que já está mantém o lugar e troca o valor.
    fn put_needed(&mut self, index: usize, amount_needed: i64) {
        let kind = &self.kind;
        let handlers = &self.handlers;
        if let Some(entry) = self
            .needed
            .iter_mut()
            .find(|(known, _)| kind.same_handler(&handlers[*known], &handlers[index]))
        {
            entry.1 = amount_needed;
        } else {
            self.needed.push((index, amount_needed));
        }
    }

    fn send_possible(
        &mut self,
        resource: &K::Resource,
        split: &mut SplitInfo,
        transaction: &Transaction,
    ) {
        for index in 0..self.handlers.len() {
            let sub = Transaction::open(Some(transaction));
            let amount_needed =
                self.kind
                    .probe(&self.handlers[index], resource, split.unsent(), &sub);
            if amount_needed <= split.share_amount() {
                // Precisa de menos que a oferta: confirma já.
                split.send(amount_needed, true);
                self.kind
                    .commit_probe(&self.handlers[index], resource, amount_needed, &sub);
                sub.commit();
            } else {
                self.put_needed(index, amount_needed);
            }
        }
    }

    fn shift_needed(
        &mut self,
        resource: &K::Resource,
        split: &mut SplitInfo,
        transaction: &Transaction,
    ) {
        if self.needed.is_empty() || split.share_amount() == 0 {
            return;
        }
        let mut position = 0;
        while position < self.needed.len() {
            let (index, amount_needed) = self.needed[position];
            if amount_needed <= split.share_amount() {
                let accepted =
                    self.kind
                        .accept(&self.handlers[index], resource, amount_needed, transaction);
                split.send(accepted, true);
                self.needed.remove(position);
            } else {
                position += 1;
            }
        }
    }

    fn send_remaining_split(
        &mut self,
        resource: &K::Resource,
        split: &mut SplitInfo,
        transaction: &Transaction,
    ) {
        if self.needed.is_empty() || split.unsent() <= 0 {
            return;
        }
        if self.needed.len() == 1 {
            // Um só destino sobrou: tudo o que não saiu vai para ele.
            let index = self.needed[0].0;
            let accepted =
                self.kind
                    .accept(&self.handlers[index], resource, split.unsent(), transaction);
            split.send(accepted, false);
            return;
        }
        let mut position = 0;
        while position < self.needed.len() {
            let remainder_amount = split.remainder_amount();
            if remainder_amount == 0 {
                return;
            }
            let (index, amount_needed) = self.needed[position];
            if amount_needed <= remainder_amount {
                let accepted =
                    self.kind
                        .accept(&self.handlers[index], resource, amount_needed, transaction);
                split.send(accepted, true);
                self.needed.remove(position);
            } else {
                let accepted = self.kind.accept(
                    &self.handlers[index],
                    resource,
                    remainder_amount,
                    transaction,
                );
                split.send(accepted, false);
                position += 1;
            }
        }
        let mut unsent = split.unsent();
        if unsent > 0 {
            // O resto que ainda sobrou vai ao primeiro destino disposto a aceitá-lo.
            for position in 0..self.needed.len() {
                let index = self.needed[position].0;
                let accepted =
                    self.kind
                        .accept(&self.handlers[index], resource, unsent, transaction);
                split.send(accepted, false);
                unsent -= accepted;
                if unsent == 0 {
                    return;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// `EmitUtils`

/// `EmitUtils.sendToAcceptors(TARGET, long, RESOURCE, transaction)`.
pub fn send_to_acceptors<K: TargetKind>(
    available: Option<&mut Target<K>>,
    amount_to_split: i64,
    resource: &K::Resource,
    transaction: Option<&Transaction>,
) -> i64 {
    let Some(available) = available else {
        return 0;
    };
    if amount_to_split == 0 {
        return 0;
    }
    let handler_count = available.handler_count();
    if handler_count == 0 {
        return 0;
    } else if handler_count == 1 {
        // Sem `SplitInfo` no caso trivial.
        return available.send_to_singular_acceptor(resource, amount_to_split, transaction);
    }
    let mut split = SplitInfo::new(amount_to_split, handler_count as i32);
    available.send_to_acceptors(&mut split, resource, transaction)
}

/// `EmitUtils.sendToAcceptors(TARGET, int, RESOURCE, transaction)`.
pub fn send_to_acceptors_int<K: TargetKind>(
    available: Option<&mut Target<K>>,
    amount_to_split: i32,
    resource: &K::Resource,
    transaction: Option<&Transaction>,
) -> i32 {
    saturated_cast(send_to_acceptors(
        available,
        i64::from(amount_to_split),
        resource,
        transaction,
    ))
}

/// `EmitUtils.getHandlersFromCaches`: os que têm handler agora, na ordem dos caches.
pub fn handlers_from_caches<C, H>(caches: &[C], get: impl Fn(&C) -> Option<H>) -> Vec<H> {
    caches.iter().filter_map(get).collect()
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os alvos de `content/network/distribution/`

/// O `EnergyHandler` de um destino. `probe_insert` e `commit_probe` são para quem o Horizon não desfaz (veja o topo do módulo).
pub trait EnergyInsert {
    /// `EnergyHandler.insert`.
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32;

    fn probe_insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.insert(amount, transaction)
    }

    fn commit_probe(&self, _amount: i32, _transaction: &Transaction) {}
}

/// `EnergyHandlerTarget`: `accept` é `handler.insert(saturatedCast(amount))`.
pub struct EnergyHandlerKind<H: EnergyInsert + ?Sized>(PhantomData<fn(&H)>);

impl<H: EnergyInsert + ?Sized> EnergyHandlerKind<H> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<H: EnergyInsert + ?Sized> Default for EnergyHandlerKind<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: EnergyInsert + ?Sized> TargetKind for EnergyHandlerKind<H> {
    type Handler = Rc<H>;
    type Resource = ();

    fn accept(
        &self,
        handler: &Rc<H>,
        _resource: &(),
        amount: i64,
        transaction: &Transaction,
    ) -> i64 {
        i64::from(handler.insert(saturated_cast(amount), transaction))
    }

    fn same_handler(&self, a: &Rc<H>, b: &Rc<H>) -> bool {
        Rc::ptr_eq(a, b)
    }

    fn probe(
        &self,
        handler: &Rc<H>,
        _resource: &(),
        amount: i64,
        transaction: &Transaction,
    ) -> i64 {
        i64::from(handler.probe_insert(saturated_cast(amount), transaction))
    }

    fn commit_probe(
        &self,
        handler: &Rc<H>,
        _resource: &(),
        accepted: i64,
        transaction: &Transaction,
    ) {
        handler.commit_probe(saturated_cast(accepted), transaction);
    }
}

pub type EnergyHandlerTarget<H> = Target<EnergyHandlerKind<H>>;

/// O `ResourceHandler<RESOURCE>` de um destino de fluido ou de químico.
pub trait ResourceInsert<R: ?Sized> {
    /// `ResourceHandler.insert(resource, amount, transaction)`.
    fn insert(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32;
}

/// `ResourceHandlerTarget`: `accept` é `handler.insert(resource, saturatedCast(amount))`.
pub struct ResourceHandlerKind<H: ResourceInsert<R> + ?Sized, R: ?Sized>(PhantomData<fn(&H, &R)>);

impl<H: ResourceInsert<R> + ?Sized, R: ?Sized> ResourceHandlerKind<H, R> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<H: ResourceInsert<R> + ?Sized, R: ?Sized> Default for ResourceHandlerKind<H, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: ResourceInsert<R> + ?Sized, R: ?Sized> TargetKind for ResourceHandlerKind<H, R> {
    type Handler = Rc<H>;
    type Resource = R;

    fn accept(&self, handler: &Rc<H>, resource: &R, amount: i64, transaction: &Transaction) -> i64 {
        i64::from(handler.insert(resource, saturated_cast(amount), transaction))
    }

    fn same_handler(&self, a: &Rc<H>, b: &Rc<H>) -> bool {
        Rc::ptr_eq(a, b)
    }
}

pub type ResourceHandlerTarget<H, R> = Target<ResourceHandlerKind<H, R>>;

/// O `IEnergyContainer` que um `SaveHandler` guarda de volta: a energia, a capacidade e o `setEnergy` numa transação.
pub trait SaveContainer {
    fn amount(&self) -> i64;
    fn capacity(&self) -> i64;
    fn set_energy(&self, energy: i64, transaction: &Transaction);
}

impl SaveContainer for EnergyContainer {
    fn amount(&self) -> i64 {
        EnergyContainer::amount(self)
    }

    fn capacity(&self) -> i64 {
        EnergyContainer::capacity(self)
    }

    fn set_energy(&self, energy: i64, transaction: &Transaction) {
        EnergyContainer::set_energy(self, energy, Some(transaction));
    }
}

impl SaveContainer for CubeEnergy {
    fn amount(&self) -> i64 {
        CubeEnergy::amount(self)
    }

    fn capacity(&self) -> i64 {
        CubeEnergy::capacity(self)
    }

    fn set_energy(&self, energy: i64, transaction: &Transaction) {
        CubeEnergy::set_energy(self, energy, Some(transaction));
    }
}

/// `EnergySaveTarget.SaveHandler`: zera o contêiner ao começar e devolve a energia que a rede reparte, até a capacidade.
pub struct SaveHandler<C: SaveContainer> {
    delegate: C,
}

impl<C: SaveContainer> SaveHandler<C> {
    /// `startSaveHandling`: zera a energia do contêiner na transação.
    pub fn start_save_handling(delegate: C, transaction: &Transaction) -> Rc<Self> {
        delegate.set_energy(0, transaction);
        Rc::new(Self { delegate })
    }

    /// `accept`.
    pub fn accept(&self, amount: i64, transaction: &Transaction) -> i64 {
        if amount == 0 {
            // Nada a aceitar (o Java diz que não deve acontecer): falha.
            return 0;
        }
        let stored = self.delegate.amount();
        let to_accept = (self.delegate.capacity() - stored).clamp(0, amount);
        if to_accept > 0 {
            self.delegate.set_energy(stored + to_accept, transaction);
        }
        to_accept
    }

    /// O contêiner que o handler guarda de volta.
    pub fn delegate(&self) -> &C {
        &self.delegate
    }
}

/// `EnergySaveTarget`.
pub struct EnergySaveKind<C: SaveContainer>(PhantomData<fn(&C)>);

impl<C: SaveContainer> EnergySaveKind<C> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<C: SaveContainer> Default for EnergySaveKind<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: SaveContainer> TargetKind for EnergySaveKind<C> {
    type Handler = Rc<SaveHandler<C>>;
    type Resource = ();

    fn accept(
        &self,
        handler: &Rc<SaveHandler<C>>,
        _resource: &(),
        amount: i64,
        transaction: &Transaction,
    ) -> i64 {
        handler.accept(amount, transaction)
    }

    fn same_handler(&self, a: &Rc<SaveHandler<C>>, b: &Rc<SaveHandler<C>>) -> bool {
        Rc::ptr_eq(a, b)
    }
}

pub type EnergySaveTarget<C> = Target<EnergySaveKind<C>>;

/// O `UniversalCable.SaveShareJournal`: a parte de um cabo na energia que a rede guarda de volta.
pub trait EnergySaveShare {
    /// `SaveShareJournal.accept(amount, transaction)`.
    fn accept(&self, amount: i64, transaction: &Transaction) -> i64;
}

/// `EnergyTransmitterSaveTarget`.
pub struct EnergyTransmitterSaveKind<S: EnergySaveShare + ?Sized>(PhantomData<fn(&S)>);

impl<S: EnergySaveShare + ?Sized> EnergyTransmitterSaveKind<S> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S: EnergySaveShare + ?Sized> Default for EnergyTransmitterSaveKind<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: EnergySaveShare + ?Sized> TargetKind for EnergyTransmitterSaveKind<S> {
    type Handler = Rc<S>;
    type Resource = ();

    fn accept(
        &self,
        handler: &Rc<S>,
        _resource: &(),
        amount: i64,
        transaction: &Transaction,
    ) -> i64 {
        handler.accept(amount, transaction)
    }

    fn same_handler(&self, a: &Rc<S>, b: &Rc<S>) -> bool {
        Rc::ptr_eq(a, b)
    }
}

pub type EnergyTransmitterSaveTarget<S> = Target<EnergyTransmitterSaveKind<S>>;

/// O `BufferedResourceTransmitter.SaveShareJournal` de um recurso (fluido, químico).
pub trait ResourceSaveShare<R: ?Sized> {
    /// `SaveShareJournal.accept(resource, amount, transaction)`.
    fn accept(&self, resource: &R, amount: i64, transaction: &Transaction) -> i64;
}

/// `ResourceTransmitterSaveTarget`.
pub struct ResourceTransmitterSaveKind<S: ResourceSaveShare<R> + ?Sized, R: ?Sized>(
    PhantomData<fn(&S, &R)>,
);

impl<S: ResourceSaveShare<R> + ?Sized, R: ?Sized> ResourceTransmitterSaveKind<S, R> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S: ResourceSaveShare<R> + ?Sized, R: ?Sized> Default for ResourceTransmitterSaveKind<S, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: ResourceSaveShare<R> + ?Sized, R: ?Sized> TargetKind for ResourceTransmitterSaveKind<S, R> {
    type Handler = Rc<S>;
    type Resource = R;

    fn accept(&self, handler: &Rc<S>, resource: &R, amount: i64, transaction: &Transaction) -> i64 {
        handler.accept(resource, amount, transaction)
    }

    fn same_handler(&self, a: &Rc<S>, b: &Rc<S>) -> bool {
        Rc::ptr_eq(a, b)
    }
}

pub type ResourceTransmitterSaveTarget<S, R> = Target<ResourceTransmitterSaveKind<S, R>>;

// ---------------------------------------------------------------------------------------------------------------------------
// `EnergyUtils.emit`

/// O contêiner de que `EnergyUtils.emit` tira a energia (`IEnergyContainer`).
pub trait EmitSource {
    fn is_empty(&self) -> bool;
    fn extract(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32;
}

impl EmitSource for CubeEnergy {
    fn is_empty(&self) -> bool {
        CubeEnergy::is_empty(self)
    }

    fn extract(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        CubeEnergy::extract(self, amount, transaction, automation)
    }
}

impl EmitSource for EnergyContainer {
    fn is_empty(&self) -> bool {
        self.amount() == 0
    }

    fn extract(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        EnergyContainer::extract(self, amount, transaction, automation)
    }
}

/// `EnergyUtils.emit(List<EnergyHandler>, long, transaction)`: o primeiro e único handler recebe tudo, vários dividem pelo
/// `EnergyHandlerTarget`.
pub fn emit<H: EnergyInsert + ?Sized>(
    targets: Vec<Rc<H>>,
    energy_to_send: i64,
    transaction: Option<&Transaction>,
) -> i64 {
    if targets.is_empty() || energy_to_send <= 0 {
        return 0;
    } else if targets.len() == 1 {
        let sub = Transaction::open(transaction);
        let sent = targets[0].insert(saturated_cast(energy_to_send), &sub);
        sub.commit();
        return i64::from(sent);
    }
    let mut target = Target::with_handlers(EnergyHandlerKind::new(), targets);
    send_to_acceptors(Some(&mut target), energy_to_send, &(), transaction)
}

/// `EnergyUtils.emit(caches, container, maxOutput, transaction)`: a energia que saiu do contêiner e foi dividida entre os
/// handlers (já tirados dos caches, `handlers_from_caches`).
pub fn emit_from_container<H: EnergyInsert + ?Sized>(
    targets: Vec<Rc<H>>,
    container: &impl EmitSource,
    max_output: i32,
    transaction: Option<&Transaction>,
) -> i32 {
    if !container.is_empty() && max_output > 0 {
        let energy_to_send = {
            let simulation = Transaction::open(transaction);
            let energy_to_send =
                container.extract(max_output, &simulation, AutomationType::Internal);
            if energy_to_send == 0 {
                // Não saiu nada do contêiner: acaba aqui.
                return 0;
            }
            energy_to_send
        };
        let sub = Transaction::open(transaction);
        let sent = saturated_cast(emit(targets, i64::from(energy_to_send), Some(&sub)));
        if sent > 0 && container.extract(sent, &sub, AutomationType::Internal) == sent {
            // Em teoria sempre vale: o que saiu é o que o contêiner entregou.
            sub.commit();
            return sent;
        }
    }
    0
}
