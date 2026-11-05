//! Porte de `TransitRequest`, `HandlerTransitRequest`, `TransitResponse` e `InventoryUtils.getEjectItemMap`: o pedido de
//! envio de itens de um bloco a outro, com seus próprios diários dentro da transação.
use crate::containers::InventorySlot;
use crate::containers::{AutomationType, ItemResource};
use crate::item_handler::ItemHandler;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// O pedido, sem o que o Java guarda em volta: o `handler` do lado de onde sai, e os dados por tipo de item.
struct RequestShared {
    handler: RefCell<Option<Rc<dyn ItemHandler>>>,
    /// `itemMap`, na ordem em que os itens entraram (`LinkedHashMap`).
    items: RefCell<Vec<ItemDataRef>>,
}

pub type ItemDataRef = Rc<JournalCell<ItemData>>;

/// `HandlerItemData`: quanto há de um tipo de item no pedido, e em quais slots.
pub struct ItemData {
    resource: ItemResource,
    total: u32,
    /// `slotMap`, na ordem de inserção.
    slots: Vec<(usize, Rc<JournalCell<AmountJournal>>)>,
    request: Weak<RequestShared>,
    me: Weak<JournalCell<ItemData>>,
}

impl Journaled for ItemData {
    type Snapshot = u32;

    fn revert_to_snapshot(&mut self, snapshot: u32) {
        self.total = snapshot;
    }

    /// Este item deixou de estar guardado: sai do pedido.
    fn on_root_commit(&mut self, _original: u32) -> Vec<Callback> {
        if self.total == 0
            && let (Some(request), Some(me)) = (self.request.upgrade(), self.me.upgrade())
        {
            request
                .items
                .borrow_mut()
                .retain(|data| !Rc::ptr_eq(data, &me));
        }
        Vec::new()
    }
}

/// `AmountJournal`: quanto ainda há num slot do pedido.
pub struct AmountJournal {
    value: u32,
    slot: usize,
    data: Weak<JournalCell<ItemData>>,
}

impl Journaled for AmountJournal {
    type Snapshot = u32;

    fn revert_to_snapshot(&mut self, snapshot: u32) {
        self.value = snapshot;
    }

    /// Não sobra mais nada neste slot: ele sai do pedido.
    fn on_root_commit(&mut self, _original: u32) -> Vec<Callback> {
        if self.value == 0
            && let Some(data) = self.data.upgrade()
        {
            let slot = self.slot;
            data.borrow_mut().slots.retain(|(index, _)| *index != slot);
        }
        Vec::new()
    }
}

/// `HandlerItemData.use`: tira `amount` do item, pelo handler, e desconta do pedido. Devolve quanto usou.
fn use_items(data: &ItemDataRef, amount: u32, transaction: &Transaction) -> u32 {
    let (resource, slots, handler) = {
        let state = data.borrow();
        let Some(request) = state.request.upgrade() else {
            return 0;
        };
        let handler = request.handler.borrow().clone();
        (state.resource.clone(), state.slots.clone(), handler)
    };
    let Some(handler) = handler else {
        return 0;
    };
    if slots.is_empty() {
        return 0;
    }
    let sub = Transaction::open(Some(transaction));
    let mut used_so_far = 0;
    for (slot, journal) in &slots {
        let to_use = (amount - used_so_far).min(journal.borrow().value);
        if to_use == 0 {
            // Chamado antes de commitar tudo: o valor pode estar zerado.
            continue;
        }
        let extracted = handler.extract_at(*slot, &resource, to_use, &sub);
        if extracted < to_use {
            // A previsão do pedido não bate com o inventário: devolve 0, e quem chamou desfaz.
            return 0;
        }
        used_so_far += extracted;
        {
            let mut state = data.borrow_mut();
            data.update_snapshots(&sub, || state.total);
            state.total -= extracted;
        }
        {
            let mut state = journal.borrow_mut();
            journal.update_snapshots(&sub, || state.value);
            state.value -= extracted;
        }
        if used_so_far == amount {
            break;
        }
    }
    sub.commit();
    used_so_far
}

/// `TransitResponse`.
#[derive(Clone)]
pub struct TransitResponse {
    resource: Option<ItemResource>,
    sending_amount: u32,
    slot_data: Option<ItemDataRef>,
}

impl TransitResponse {
    pub const EMPTY: Self = Self {
        resource: None,
        sending_amount: 0,
        slot_data: None,
    };

    pub fn is_empty(&self) -> bool {
        match (&self.resource, &self.slot_data) {
            (Some(_), Some(data)) => self.sending_amount == 0 || data.borrow().total == 0,
            _ => true,
        }
    }

    pub fn sending_amount(&self) -> u32 {
        self.sending_amount
    }

    pub fn use_items(&self, amount: u32, transaction: &Transaction) -> u32 {
        match &self.slot_data {
            Some(data) if !self.is_empty() => use_items(data, amount, transaction),
            _ => 0,
        }
    }

    pub fn use_all(&self, transaction: &Transaction) -> bool {
        !self.is_empty() && self.use_items(self.sending_amount, transaction) == self.sending_amount
    }
}

/// `HandlerTransitRequest` (e `EjectTransitRequest`, que só deixa trocar o handler).
pub struct HandlerTransitRequest {
    shared: Rc<RequestShared>,
}

impl HandlerTransitRequest {
    pub fn new(handler: Option<Rc<dyn ItemHandler>>) -> Self {
        Self {
            shared: Rc::new(RequestShared {
                handler: RefCell::new(handler),
                items: RefCell::new(Vec::new()),
            }),
        }
    }

    /// `EjectTransitRequest.setHandler`.
    pub fn set_handler(&self, handler: Option<Rc<dyn ItemHandler>>) {
        *self.shared.handler.borrow_mut() = handler;
    }

    /// `addItem`: um slot do inventário e quanto dele sai.
    pub fn add_item(&self, resource: &ItemResource, amount: u32, slot: usize) {
        if amount == 0 {
            return;
        }
        let existing = self
            .shared
            .items
            .borrow()
            .iter()
            .find(|data| data.borrow().resource == *resource)
            .cloned();
        let data = existing.unwrap_or_else(|| {
            let data = JournalCell::new_cyclic_item(resource.clone(), &self.shared);
            self.shared.items.borrow_mut().push(data.clone());
            data
        });
        let journal = JournalCell::new(AmountJournal {
            value: amount,
            slot,
            data: Rc::downgrade(&data),
        });
        let mut state = data.borrow_mut();
        // `slotMap.put`: um slot repetido troca o diário sem mudar de lugar.
        match state.slots.iter_mut().find(|(index, _)| *index == slot) {
            Some(entry) => entry.1 = journal,
            None => state.slots.push((slot, journal)),
        }
        state.total += amount;
    }

    pub fn is_empty(&self) -> bool {
        self.shared
            .items
            .borrow()
            .iter()
            .all(|data| data.borrow().total == 0)
    }

    /// `eject`, para um alvo que não é um transportador: `addToInventoryUnchecked`. Devolve o que o alvo aceitou do
    /// primeiro tipo de item que coube, já commitado na transação dada.
    pub fn eject(
        &self,
        target: Option<&Rc<dyn ItemHandler>>,
        min: u32,
        transaction: &Transaction,
    ) -> TransitResponse {
        if self.is_empty() {
            return TransitResponse::EMPTY;
        }
        let Some(inventory) = target.filter(|inventory| inventory.size() > 0) else {
            return TransitResponse::EMPTY;
        };
        let items = self.shared.items.borrow().clone();
        for data in items {
            let (resource, count) = {
                let state = data.borrow();
                (state.resource.clone(), state.total)
            };
            if count == 0 {
                continue;
            }
            let sub = Transaction::open(Some(transaction));
            let inserted = inventory.insert(&resource, count, &sub);
            if inserted > 0 && inserted >= min {
                sub.commit();
                return TransitResponse {
                    resource: Some(resource),
                    sending_amount: inserted,
                    slot_data: Some(data),
                };
            }
        }
        TransitResponse::EMPTY
    }
}

impl JournalCell<ItemData> {
    fn new_cyclic_item(resource: ItemResource, request: &Rc<RequestShared>) -> ItemDataRef {
        let cell = JournalCell::new(ItemData {
            resource,
            total: 0,
            slots: Vec::new(),
            request: Rc::downgrade(request),
            me: Weak::new(),
        });
        cell.borrow_mut().me = Rc::downgrade(&cell);
        cell
    }
}

/// `Collections.shuffle`: o sorteio de `java.util.Random` fica por conta de quem chama, com `below(n)` em `0..n`.
pub fn shuffle<T>(list: &mut [T], below: &mut dyn FnMut(usize) -> usize) {
    for i in (2..=list.len()).rev() {
        list.swap(i - 1, below(i));
    }
}

/// `InventoryUtils.getEjectItemMap`: o que dá para tirar de cada slot, vendo os slots numa ordem sorteada para não
/// formar padrão. Cada tentativa é uma simulação, que sempre desfaz.
pub fn eject_item_map(
    request: HandlerTransitRequest,
    slots: &[InventorySlot],
    below: &mut dyn FnMut(usize) -> usize,
    transaction: Option<&Transaction>,
) -> HandlerTransitRequest {
    let mut shuffled: Vec<&InventorySlot> = slots.iter().collect();
    shuffle(&mut shuffled, below);
    for slot in shuffled {
        if let Some(resource) = slot.resource() {
            let simulation = Transaction::open(transaction);
            // `EXTERNAL`: é o que a extração de verdade usa.
            let extracted = slot.extract(
                &resource,
                slot.amount(),
                &simulation,
                AutomationType::External,
            );
            drop(simulation);
            if extracted > 0 {
                let index = slots
                    .iter()
                    .position(|candidate| candidate.same_slot(slot))
                    .unwrap_or_default();
                request.add_item(&resource, extracted, index);
            }
        }
    }
    request
}
