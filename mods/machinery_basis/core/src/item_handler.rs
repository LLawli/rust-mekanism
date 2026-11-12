//! Porte de `ResourceHandler<ItemResource>` do NeoForge: o que um bloco expõe de itens a quem está do lado, e o
//! `VanillaContainerWrapper`, que dá um baú a ele. As mudanças passam por transações, em qualquer bloco.
use crate::containers::{AutomationType, InventorySlot, ItemResource};
use crate::transaction::{Callback, JournalCell, RootCommitJournal, Transaction};
use std::rc::Rc;

/// `ResourceHandler<ItemResource>`. Cada método muda o estado dentro da transação dada.
pub trait ItemHandler {
    fn size(&self) -> usize;
    fn resource(&self, index: usize) -> Option<ItemResource>;
    fn amount(&self, index: usize) -> u32;
    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32;
    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32;

    /// O `TransporterItemHandler`, se este é o que um transportador logístico expõe (`target instanceof TransporterItemHandler`).
    fn as_transporter_handler(&self) -> Option<&crate::transporter::movement::TransporterItemHandler> {
        None
    }

    /// `ResourceHandler.insert(resource, amount, tx)`: slot a slot, na ordem, até acabar.
    fn insert(&self, resource: &ItemResource, amount: u32, transaction: &Transaction) -> u32 {
        let mut inserted = 0;
        for index in 0..self.size() {
            inserted += self.insert_at(index, resource, amount - inserted, transaction);
            if inserted == amount {
                break;
            }
        }
        inserted
    }

    /// `ResourceHandler.extract(resource, amount, tx)`.
    fn extract(&self, resource: &ItemResource, amount: u32, transaction: &Transaction) -> u32 {
        let mut extracted = 0;
        for index in 0..self.size() {
            extracted += self.extract_at(index, resource, amount - extracted, transaction);
            if extracted == amount {
                break;
            }
        }
        extracted
    }
}

/// Se o slot aceita o item.
type SlotRule = Box<dyn Fn(usize, &ItemResource) -> bool>;

/// Um inventário com regra por slot: o `isValid` do `ResourceHandler`, como o forno, que pela lateral só aceita combustível.
/// O limite de pilha continua com o inventário de dentro.
pub struct RuledHandler {
    inner: Rc<dyn ItemHandler>,
    accepts: SlotRule,
}

impl RuledHandler {
    pub fn new(
        inner: Rc<dyn ItemHandler>,
        accepts: impl Fn(usize, &ItemResource) -> bool + 'static,
    ) -> Self {
        Self {
            inner,
            accepts: Box::new(accepts),
        }
    }
}

impl ItemHandler for RuledHandler {
    fn size(&self) -> usize {
        self.inner.size()
    }

    fn resource(&self, index: usize) -> Option<ItemResource> {
        self.inner.resource(index)
    }

    fn amount(&self, index: usize) -> u32 {
        self.inner.amount(index)
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        if (self.accepts)(index, resource) {
            self.inner.insert_at(index, resource, amount, transaction)
        } else {
            0
        }
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        self.inner.extract_at(index, resource, amount, transaction)
    }
}

/// Quem diz, de um bloco vizinho, o que ele expõe de itens por uma face (`BlockCapabilityCache.getCapability`).
pub trait ItemCapabilities {
    fn item_handler(&self, face: crate::sides::Face) -> Option<Rc<dyn ItemHandler>>;
}

/// Sem vizinho nenhum.
pub struct NoNeighbours;

impl ItemCapabilities for NoNeighbours {
    fn item_handler(&self, _face: crate::sides::Face) -> Option<Rc<dyn ItemHandler>> {
        None
    }
}

/// O que um bloco com slots mostra por um lado: os slots daquele lado, usados como quem está de fora
/// (`AutomationType.EXTERNAL`).
pub struct SlotHandler {
    slots: Vec<InventorySlot>,
}

impl SlotHandler {
    pub fn new(slots: Vec<InventorySlot>) -> Self {
        Self { slots }
    }
}

impl ItemHandler for SlotHandler {
    fn size(&self) -> usize {
        self.slots.len()
    }

    fn resource(&self, index: usize) -> Option<ItemResource> {
        self.slots[index].resource()
    }

    fn amount(&self, index: usize) -> u32 {
        self.slots[index].amount()
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        self.slots[index].insert(resource, amount, transaction, AutomationType::External)
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        self.slots[index].extract(resource, amount, transaction, AutomationType::External)
    }
}

/// `VanillaContainerWrapper` de um baú: cada slot guarda seu retrato, e o `setChanged` do baú avisa uma vez, no commit da
/// raiz, se algo mudou.
pub struct VanillaContainer {
    slots: Vec<InventorySlot>,
    set_changed: Rc<JournalCell<RootCommitJournal>>,
}

impl VanillaContainer {
    /// `on_set_changed` é o `Container.setChanged`.
    pub fn new(size: usize, on_set_changed: Callback) -> Rc<Self> {
        Rc::new(Self {
            slots: (0..size).map(|_| InventorySlot::new(None)).collect(),
            set_changed: RootCommitJournal::new(on_set_changed),
        })
    }

    /// `Container.setItem`, de fora de uma transação.
    pub fn set_item(&self, index: usize, resource: Option<ItemResource>, amount: u32) {
        self.slots[index].set_contents(resource, amount, None);
    }

    pub fn clear(&self) {
        for slot in &self.slots {
            slot.set_contents(None, 0, None);
        }
    }

    /// Os slots com algo, como `(índice, recurso, quantidade)`.
    pub fn stacks(&self) -> Vec<(usize, ItemResource, u32)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| Some((index, slot.resource()?, slot.amount())))
            .collect()
    }
}

impl ItemHandler for VanillaContainer {
    fn size(&self) -> usize {
        self.slots.len()
    }

    fn resource(&self, index: usize) -> Option<ItemResource> {
        self.slots[index].resource()
    }

    fn amount(&self, index: usize) -> u32 {
        self.slots[index].amount()
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        let inserted =
            self.slots[index].insert(resource, amount, transaction, AutomationType::Manual);
        if inserted > 0 {
            self.set_changed.touch(transaction);
        }
        inserted
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        let extracted =
            self.slots[index].extract(resource, amount, transaction, AutomationType::Manual);
        if extracted > 0 {
            self.set_changed.touch(transaction);
        }
        extracted
    }
}
