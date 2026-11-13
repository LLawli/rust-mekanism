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

    /// `itemType()`: o item da resposta, ou nada se é a vazia.
    pub fn item_type(&self) -> Option<&ItemResource> {
        self.resource.as_ref()
    }

    /// `slotData().getTotalCount()`.
    pub fn slot_total(&self) -> u32 {
        self.slot_data
            .as_ref()
            .map_or(0, |data| data.borrow().total)
    }

    /// `getRejected()`: o que o pedido tinha daquele item e o destino não aceitou.
    pub fn rejected(&self) -> u32 {
        if self.is_empty() {
            0
        } else {
            self.slot_total() - self.sending_amount
        }
    }

    /// `TransitResponse.equals` (o `equals` do `record`): o mesmo item, a mesma quantidade e um `ItemData` igual (o `ItemData.equals`
    /// compara a quantidade total e o item).
    pub fn same_as(&self, other: &TransitResponse) -> bool {
        self.resource == other.resource
            && self.sending_amount == other.sending_amount
            && match (&self.slot_data, &other.slot_data) {
                (Some(a), Some(b)) => {
                    let (a, b) = (a.borrow(), b.borrow());
                    a.total == b.total && a.resource == b.resource
                }
                (None, None) => true,
                _ => false,
            }
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

/// `TransitRequest`: o que se quer mandar. `Simple` é o `SimpleTransitRequest` (um item e uma quantidade, sem inventário de onde sair), e
/// `Handler` o `HandlerTransitRequest`.
pub enum TransitRequest {
    Simple(ItemDataRef),
    Handler(HandlerTransitRequest),
}

impl TransitRequest {
    /// `TransitRequest.simple(itemType, amount)`: a quantidade tem de ser positiva.
    pub fn simple(resource: &ItemResource, amount: u32) -> Self {
        assert!(amount > 0, "Amount must be positive");
        let data = JournalCell::new(ItemData {
            resource: resource.clone(),
            total: amount,
            slots: Vec::new(),
            request: Weak::new(),
            me: Weak::new(),
        });
        data.borrow_mut().me = Rc::downgrade(&data);
        TransitRequest::Simple(data)
    }

    /// Os `ItemData` do pedido, na ordem (`iterator()`).
    pub fn item_data(&self) -> Vec<ItemDataRef> {
        match self {
            TransitRequest::Simple(data) => vec![data.clone()],
            TransitRequest::Handler(request) => request.shared.items.borrow().clone(),
        }
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        match self {
            TransitRequest::Simple(_) => false,
            TransitRequest::Handler(request) => request.is_empty(),
        }
    }

    /// `addToInventory(level, pos, inventory, min, force, transaction)`: o que o destino aceita de volta ou de novo. Com `force` e um
    /// sorter no destino, é ele quem responde (`sendHome`).
    pub fn add_to_inventory(
        &self,
        world: &dyn crate::transporter::world::TransporterWorld,
        pos: i64,
        inventory: Option<&Rc<dyn ItemHandler>>,
        min: u32,
        force: bool,
        transaction: &Transaction,
    ) -> TransitResponse {
        if self.is_empty() {
            return TransitResponse::EMPTY;
        }
        if force && let Some(sorter) = world.sorter(pos) {
            return sorter.send_home(self, transaction);
        }
        self.add_to_inventory_unchecked(inventory, min, transaction)
    }

    /// `addToInventoryUnchecked(inventory, min, transaction)`: o primeiro tipo do pedido que o inventário aceita ao menos `min`.
    pub fn add_to_inventory_unchecked(
        &self,
        inventory: Option<&Rc<dyn ItemHandler>>,
        min: u32,
        transaction: &Transaction,
    ) -> TransitResponse {
        let Some(inventory) = inventory.filter(|inventory| inventory.size() > 0) else {
            return TransitResponse::EMPTY;
        };
        for data in self.item_data() {
            let (resource, count) = item_data_info(&data);
            if count == 0 {
                continue;
            }
            let sub = Transaction::open(Some(transaction));
            let inserted = inventory.insert(&resource, count, &sub);
            if inserted > 0 && inserted >= min {
                sub.commit();
                return self.create_response(&resource, inserted, &data);
            }
        }
        TransitResponse::EMPTY
    }

    /// `definedItem(inventory, min, max, finder, transaction)`: o que dá para tirar do inventário, slot a slot de trás para a frente, até
    /// `max` de cada tipo, deixando de fora os tipos de que há menos de `min`. `TransitRequest.anyItem` é o de `min` 1 e qualquer item.
    pub fn defined_item(
        inventory: &Rc<dyn ItemHandler>,
        min: u32,
        max: u32,
        finder: &dyn Fn(&ItemResource) -> bool,
        transaction: Option<&Transaction>,
    ) -> TransitRequest {
        let request = HandlerTransitRequest::new(Some(inventory.clone()));
        let simulation = Transaction::open(transaction);
        for index in (0..inventory.size()).rev() {
            let Some(resource) = inventory.resource(index) else {
                continue;
            };
            if !finder(&resource) {
                continue;
            }
            let extractable = inventory.extract_at(index, &resource, max, &simulation);
            if extractable > 0 {
                let to_use = extractable.min(max - request.count_of(&resource));
                if to_use == 0 {
                    continue;
                }
                request.add_item(&resource, to_use, index);
            }
        }
        drop(simulation);
        request.retain_at_least(min);
        TransitRequest::Handler(request)
    }

    /// `anyItem(inventory, amount, transaction)`.
    pub fn any_item(
        inventory: &Rc<dyn ItemHandler>,
        amount: u32,
        transaction: Option<&Transaction>,
    ) -> TransitRequest {
        Self::defined_item(inventory, 1, amount, &|_| true, transaction)
    }

    /// `createResponse(itemType, inserted, data)`.
    pub fn create_response(
        &self,
        resource: &ItemResource,
        inserted: u32,
        data: &ItemDataRef,
    ) -> TransitResponse {
        if inserted == 0 {
            return TransitResponse::EMPTY;
        }
        TransitResponse {
            resource: Some(resource.clone()),
            sending_amount: inserted,
            slot_data: Some(data.clone()),
        }
    }
}

/// `ItemData.getItemType()` e `getTotalCount()`, de um `ItemData` do pedido.
pub fn item_data_info(data: &ItemDataRef) -> (ItemResource, u32) {
    let state = data.borrow();
    (state.resource.clone(), state.total)
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

    /// O mesmo pedido, visto por outro dono (`TransitRequest.Handler` guarda o pedido por valor, e o ejetor segue com o dele): os dois dividem os itens.
    pub fn share(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }

    /// `eject`, para um alvo que é um transportador (`target instanceof TransporterItemHandler`): o pedido inteiro vai a `LogisticalTransporterBase.insert`, com a cor de saída.
    pub fn eject_to_transporter(
        &self,
        target: &dyn crate::item_handler::TransporterTarget,
        min: u32,
        output_color: Option<u8>,
        transaction: &Transaction,
    ) -> TransitResponse {
        if self.is_empty() {
            return TransitResponse::EMPTY;
        }
        target.insert_request(
            &TransitRequest::Handler(self.share()),
            output_color,
            min,
            transaction,
        )
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

    /// `getCount(itemType)`: quanto o pedido já tem do item.
    pub fn count_of(&self, resource: &ItemResource) -> u32 {
        self.shared
            .items
            .borrow()
            .iter()
            .find(|data| data.borrow().resource == *resource)
            .map_or(0, |data| data.borrow().total)
    }

    /// Tira do pedido os itens de que há menos de `min`.
    pub fn retain_at_least(&self, min: u32) {
        self.shared
            .items
            .borrow_mut()
            .retain(|data| data.borrow().total >= min);
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
