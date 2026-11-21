//! Porte de `ResourceHandlerSlot`: o slot que troca um recurso com o item que guarda (`handleContainer` com os três modos de
//! `ContainerEditMode`, `drainContainerIntoSlot`, `fillContainerFromSlot`, `tryFillOrMove`, `canInput`), de
//! `LastTransferDirection` e de `InOutSlotResourceItemAccess`, que decide se o item trocado volta ao slot de entrada ou vai para o
//! de saída. É o `FluidInventorySlot.handleTank` do Fluid Tank e o `ChemicalInventorySlot` das máquinas de químico.
//!
//! A lib não conhece nenhum item nem recurso: a capability do item entra como [`ItemHandlerLookup`].
use crate::containers::{AutomationType, InventorySlot, ItemResource, WeakInventorySlot};
use crate::item_access::{
    ItemAccess, ItemHandlerLookup, ItemResourceHandler, OneByOne, SlotAccess, StackAccess,
};
use crate::resource_tank::{Resource, ResourceTank};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;

/// `IResourceContainer<R>`, o que o slot precisa do contêiner: o `insert` e o `extract` podem ser os de uma subclasse (o tanque
/// criativo, o tanque que transborda para o de cima).
pub trait ResourceContainer<R> {
    /// `resource`: `None` é o vazio.
    fn resource(&self) -> Option<R>;
    fn amount_as_int(&self) -> i32;
    fn is_empty(&self) -> bool;
    /// `capacityAsInt(resource)`; `None` é o recurso vazio.
    fn capacity_as_int(&self, resource: Option<&R>) -> i32;
    /// `getNeededAsInt(resource)`.
    fn needed_as_int(&self, resource: &R) -> i32;
    fn is_valid_for_insertion(&self, resource: &R, automation: AutomationType) -> bool;
    fn insert(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32;
    fn extract(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32;
}

impl<R: Resource> ResourceContainer<R> for ResourceTank<R> {
    fn resource(&self) -> Option<R> {
        ResourceTank::resource(self)
    }

    fn amount_as_int(&self) -> i32 {
        ResourceTank::amount_as_int(self)
    }

    fn is_empty(&self) -> bool {
        ResourceTank::is_empty(self)
    }

    fn capacity_as_int(&self, resource: Option<&R>) -> i32 {
        i32::try_from(self.capacity_for(resource)).unwrap_or(i32::MAX)
    }

    fn needed_as_int(&self, resource: &R) -> i32 {
        ResourceTank::needed_as_int(self, Some(resource))
    }

    fn is_valid_for_insertion(&self, resource: &R, automation: AutomationType) -> bool {
        ResourceTank::is_valid_for_insertion(self, resource, automation)
    }

    fn insert(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        ResourceTank::insert(self, resource, amount, transaction, automation)
    }

    fn extract(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        ResourceTank::extract(self, resource, amount, transaction, automation)
    }
}

/// `IFluidContainerManager.ContainerEditMode`, na ordem do enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContainerEditMode {
    #[default]
    Both,
    Fill,
    Empty,
}

impl ContainerEditMode {
    pub const VALUES: [Self; 3] = [Self::Both, Self::Fill, Self::Empty];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `BY_ID`, com `OutOfBoundsStrategy.ZERO`.
    pub fn from_ordinal(ordinal: usize) -> Self {
        Self::VALUES.get(ordinal).copied().unwrap_or_default()
    }

    /// `getNext`.
    pub fn next(self) -> Self {
        Self::VALUES[(self.ordinal() + 1) % 3]
    }

    /// `getPrevious`.
    pub fn previous(self) -> Self {
        Self::VALUES[(self.ordinal() + 2) % 3]
    }

    pub fn serialized_name(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Fill => "fill",
            Self::Empty => "empty",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|mode| mode.serialized_name() == name)
    }
}

/// `LastTransferDirection`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LastTransferDirection {
    #[default]
    Unknown,
    FillFromItem,
    DrainIntoItem,
}

impl LastTransferDirection {
    pub fn serialized_name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::FillFromItem => "fill_from_item",
            Self::DrainIntoItem => "drain_into_item",
        }
    }

    pub fn from_serialized_name(name: &str) -> Self {
        match name {
            "fill_from_item" => Self::FillFromItem,
            "drain_into_item" => Self::DrainIntoItem,
            _ => Self::Unknown,
        }
    }
}

/// `LastDirectionJournal`.
struct DirectionState(LastTransferDirection);

impl Journaled for DirectionState {
    type Snapshot = LastTransferDirection;

    fn revert_to_snapshot(&mut self, snapshot: LastTransferDirection) {
        self.0 = snapshot;
    }

    fn on_root_commit(&mut self, _original: LastTransferDirection) -> Vec<Callback> {
        Vec::new()
    }
}

type Direction = Rc<JournalCell<DirectionState>>;

fn set_direction(
    cell: &Direction,
    direction: LastTransferDirection,
    transaction: Option<&Transaction>,
) {
    if let Some(transaction) = transaction {
        let current = cell.borrow().0;
        cell.update_snapshots(transaction, || current);
    }
    cell.borrow_mut().0 = direction;
}

/// `InOutSlotResourceItemAccess`: o slot de entrada visto por dentro (`INTERNAL`), com o item que volta de uma troca indo para o
/// slot de entrada ou para o de saída conforme a última direção.
struct InOutAccess<'a, R> {
    input: SlotAccess,
    output: &'a InventorySlot,
    lookup: &'a ItemHandlerLookup<R>,
    direction: &'a Direction,
    /// `currentStoredContents` e `currentTypeCapacity`, lidos do contêiner quando o acesso nasce.
    stored: Option<R>,
    capacity: i32,
}

impl<R> ItemAccess for InOutAccess<'_, R> {
    fn resource(&self) -> Option<ItemResource> {
        self.input.resource()
    }

    fn amount(&self) -> i32 {
        self.input.amount()
    }

    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        if amount == 0 {
            return 0;
        }
        let internal = AutomationType::Internal;
        let amount_u32 = u32::try_from(amount).unwrap_or(0);
        if self.amount() == 0 {
            // Vazio: escolhe o slot pelo que o item que entra ainda pode fazer.
            let mut to_input = false;
            let loose = StackAccess::of(resource);
            if let Some(inserted) = (self.lookup)(&loose) {
                let simulation = Transaction::open(Some(transaction));
                let last = self.direction.borrow().0;
                match &self.stored {
                    Some(stored) => {
                        if last == LastTransferDirection::FillFromItem {
                            to_input = inserted.extract(stored, self.capacity, &simulation) > 0;
                        } else if last == LastTransferDirection::DrainIntoItem {
                            to_input = inserted.insert(stored, self.capacity, &simulation) > 0;
                        }
                    }
                    None if last == LastTransferDirection::FillFromItem => {
                        for index in 0..inserted.size() {
                            if let Some(held) = inserted.resource(index)
                                && inserted.extract_at(
                                    index,
                                    &held,
                                    inserted.amount_as_int(index),
                                    &simulation,
                                ) > 0
                            {
                                to_input = true;
                                break;
                            }
                        }
                    }
                    None => {}
                }
            }
            let target = if to_input {
                &self.input.slot
            } else {
                self.output
            };
            return i32::try_from(target.insert(resource, amount_u32, transaction, internal))
                .unwrap_or(i32::MAX);
        }
        // Senão, o slot de entrada primeiro e o que não couber vai para o de saída.
        let mut inserted = self.input.insert(resource, amount, transaction);
        if inserted < amount {
            let rest = u32::try_from(amount - inserted).unwrap_or(0);
            inserted += i32::try_from(self.output.insert(resource, rest, transaction, internal))
                .unwrap_or(0);
        }
        inserted
    }

    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        self.input.extract(resource, amount, transaction)
    }
}

/// `ResourceHandlerSlot`: o slot e a última direção de troca dele.
#[derive(Clone)]
pub struct HandlerSlot {
    slot: InventorySlot,
    direction: Direction,
}

impl HandlerSlot {
    /// `build` recebe o aviso de mudança que o slot tem de usar: o `listener` de quem cria e, depois dele, a volta da direção a
    /// `UNKNOWN` quando o slot fica vazio (`onContentsChanged`).
    pub fn new(
        listener: Option<Callback>,
        build: impl FnOnce(Option<Callback>) -> InventorySlot,
    ) -> Self {
        let direction: Direction = JournalCell::new(DirectionState(LastTransferDirection::Unknown));
        let holder: Rc<RefCell<Option<WeakInventorySlot>>> = Rc::new(RefCell::new(None));
        let callback: Callback = {
            let direction = direction.clone();
            let holder = holder.clone();
            Rc::new(move || {
                if let Some(listener) = &listener {
                    listener();
                }
                let slot = holder
                    .borrow()
                    .as_ref()
                    .and_then(WeakInventorySlot::upgrade);
                if slot.is_some_and(|slot| slot.is_empty()) {
                    set_direction(&direction, LastTransferDirection::Unknown, None);
                }
            })
        };
        let slot = build(Some(callback));
        *holder.borrow_mut() = Some(slot.downgrade());
        Self { slot, direction }
    }

    pub fn slot(&self) -> &InventorySlot {
        &self.slot
    }

    pub fn last_transfer_direction(&self) -> LastTransferDirection {
        self.direction.borrow().0
    }

    /// `setLastTransferDirection`.
    pub fn set_last_transfer_direction(
        &self,
        direction: LastTransferDirection,
        transaction: Option<&Transaction>,
    ) {
        set_direction(&self.direction, direction, transaction);
    }

    /// `resetLastTransferDirection`.
    pub fn reset_last_transfer_direction(&self, transaction: Option<&Transaction>) {
        self.set_last_transfer_direction(LastTransferDirection::Unknown, transaction);
    }

    fn in_out<'a, R>(
        &'a self,
        container: &dyn ResourceContainer<R>,
        output: &'a InventorySlot,
        lookup: &'a ItemHandlerLookup<R>,
    ) -> InOutAccess<'a, R> {
        let stored = container.resource();
        let capacity = container.capacity_as_int(stored.as_ref());
        InOutAccess {
            input: SlotAccess {
                slot: self.slot.clone(),
                automation: AutomationType::Internal,
            },
            output,
            lookup,
            direction: &self.direction,
            stored,
            capacity,
        }
    }

    /// `handleContainer(resourceContainer, outputSlot, editMode, containerType, transaction)`.
    pub fn handle_container<R: Resource + Eq + Hash>(
        &self,
        container: &dyn ResourceContainer<R>,
        output: &InventorySlot,
        edit_mode: ContainerEditMode,
        lookup: &ItemHandlerLookup<R>,
        transaction: Option<&Transaction>,
    ) {
        if self.slot.is_empty() {
            return;
        }
        let access = self.in_out(container, output, lookup);
        // `getHandler`: sempre pelo acesso de um por um.
        let one = OneByOne(&access);
        let Some(handler) = lookup(&one) else {
            return;
        };
        let handler = handler.as_ref();
        let sub = Transaction::open(transaction);
        let commit = match edit_mode {
            ContainerEditMode::Fill => {
                self.set_last_transfer_direction(LastTransferDirection::DrainIntoItem, Some(&sub));
                drain_into_handler(container, handler, &sub)
            }
            ContainerEditMode::Empty => {
                self.set_last_transfer_direction(LastTransferDirection::FillFromItem, Some(&sub));
                fill_from_handler(container, handler, &sub)
            }
            ContainerEditMode::Both => match self.last_transfer_direction() {
                LastTransferDirection::Unknown => {
                    self.set_last_transfer_direction(
                        LastTransferDirection::DrainIntoItem,
                        Some(&sub),
                    );
                    if drain_into_handler(container, handler, &sub) {
                        true
                    } else {
                        // Não deu para encher o item: tenta esvaziá-lo no contêiner e, se nem isso, manda-o para a saída.
                        self.set_last_transfer_direction(
                            LastTransferDirection::FillFromItem,
                            Some(&sub),
                        );
                        self.try_fill_or_move(container, handler, output, &sub)
                    }
                }
                LastTransferDirection::FillFromItem => {
                    self.try_fill_or_move(container, handler, output, &sub)
                }
                // Aqui o item não sai: o contêiner pode só estar esperando mais conteúdo para encher o item.
                LastTransferDirection::DrainIntoItem => {
                    drain_into_handler(container, handler, &sub)
                }
            },
        };
        if commit {
            sub.commit();
        }
    }

    /// `tryFillOrMove`.
    fn try_fill_or_move<R: Resource + Eq + Hash>(
        &self,
        container: &dyn ResourceContainer<R>,
        handler: &dyn ItemResourceHandler<R>,
        output: &InventorySlot,
        transaction: &Transaction,
    ) -> bool {
        if fill_from_handler(container, handler, transaction) {
            return true;
        }
        let invalid = match container.resource() {
            None => true,
            Some(stored) => {
                let simulation = Transaction::open(Some(transaction));
                handler.extract(
                    &stored,
                    container.capacity_as_int(Some(&stored)),
                    &simulation,
                ) == 0
            }
        };
        if !invalid {
            return true;
        }
        // O item não enche o contêiner agora nem nunca: vai para o slot de saída.
        let sub = Transaction::open(Some(transaction));
        let Some(stored_type) = self.slot.resource() else {
            return false;
        };
        let stored = self.slot.amount();
        let extracted = self
            .slot
            .extract(&stored_type, stored, &sub, AutomationType::Internal);
        if extracted == stored
            && output.insert(&stored_type, stored, &sub, AutomationType::Internal) == stored
        {
            sub.commit();
            return true;
        }
        false
    }

    /// `drainContainerIntoSlot(resourceContainer, containerType, transaction)`: enche o item do slot, sem movê-lo depois.
    pub fn drain_container_into_slot<R: Resource + Eq + Hash>(
        &self,
        container: &dyn ResourceContainer<R>,
        lookup: &ItemHandlerLookup<R>,
        transaction: Option<&Transaction>,
    ) -> bool {
        self.own_slot_transfer(
            lookup,
            transaction,
            LastTransferDirection::DrainIntoItem,
            |handler, sub| drain_into_handler(container, handler, sub),
        )
    }

    /// `fillContainerFromSlot(resourceContainer, containerType, transaction)`: esvazia o item do slot, sem movê-lo depois.
    pub fn fill_container_from_slot<R: Resource + Eq + Hash>(
        &self,
        container: &dyn ResourceContainer<R>,
        lookup: &ItemHandlerLookup<R>,
        transaction: Option<&Transaction>,
    ) -> bool {
        self.own_slot_transfer(
            lookup,
            transaction,
            LastTransferDirection::FillFromItem,
            |handler, sub| fill_from_handler(container, handler, sub),
        )
    }

    /// O que as duas variantes sem slot de saída têm em comum: a capability pelo `asItemAccess` do próprio slot, sem o um por um.
    fn own_slot_transfer<R>(
        &self,
        lookup: &ItemHandlerLookup<R>,
        transaction: Option<&Transaction>,
        direction: LastTransferDirection,
        transfer: impl FnOnce(&dyn ItemResourceHandler<R>, &Transaction) -> bool,
    ) -> bool {
        if self.slot.is_empty() {
            return false;
        }
        let access = SlotAccess {
            slot: self.slot.clone(),
            automation: AutomationType::Internal,
        };
        let Some(handler) = lookup(&access) else {
            return false;
        };
        let sub = Transaction::open(transaction);
        self.set_last_transfer_direction(direction, Some(&sub));
        if transfer(handler.as_ref(), &sub) {
            sub.commit();
            return true;
        }
        false
    }
}

/// `fillContainerFromSlot(resourceContainer, handler, transaction)`.
fn fill_from_handler<R: Resource + Eq + Hash>(
    container: &dyn ResourceContainer<R>,
    handler: &dyn ItemResourceHandler<R>,
    transaction: &Transaction,
) -> bool {
    if let Some(resource) = container.resource() {
        let needed = container.needed_as_int(&resource);
        return needed > 0
            && fill_with_resource(container, handler, &resource, needed, transaction);
    }
    let mut known: HashSet<R> = HashSet::new();
    for index in 0..handler.size() {
        let Some(resource) = handler.resource(index) else {
            continue;
        };
        if known.insert(resource.clone())
            && container.is_valid_for_insertion(&resource, AutomationType::Internal)
            && fill_with_resource(
                container,
                handler,
                &resource,
                container.capacity_as_int(Some(&resource)),
                transaction,
            )
        {
            return true;
        }
    }
    false
}

/// `fillContainerFromSlot(resourceContainer, handler, resource, amountNeeded, transaction)`.
fn fill_with_resource<R: Resource>(
    container: &dyn ResourceContainer<R>,
    handler: &dyn ItemResourceHandler<R>,
    resource: &R,
    amount_needed: i32,
    transaction: &Transaction,
) -> bool {
    if amount_needed == 0 {
        return false;
    }
    let room_for = {
        let simulation = Transaction::open(Some(transaction));
        container.insert(
            resource,
            amount_needed,
            &simulation,
            AutomationType::Internal,
        )
    };
    if room_for == 0 {
        return false;
    }
    let sub = Transaction::open(Some(transaction));
    let extracted = handler.extract(resource, room_for, &sub);
    if extracted > 0
        && container.insert(resource, extracted, &sub, AutomationType::Internal) == extracted
    {
        sub.commit();
        return true;
    }
    false
}

/// `drainContainerIntoSlot(resourceContainer, handler, transaction)`.
fn drain_into_handler<R: Resource>(
    container: &dyn ResourceContainer<R>,
    handler: &dyn ItemResourceHandler<R>,
    transaction: &Transaction,
) -> bool {
    let Some(resource) = container.resource() else {
        return false;
    };
    let available = {
        let simulation = Transaction::open(Some(transaction));
        container.extract(
            &resource,
            container.amount_as_int(),
            &simulation,
            AutomationType::Internal,
        )
    };
    if available == 0 {
        return false;
    }
    let sub = Transaction::open(Some(transaction));
    let inserted = handler.insert(&resource, available, &sub);
    if inserted > 0
        && container.extract(&resource, inserted, &sub, AutomationType::Internal) == inserted
    {
        sub.commit();
        return true;
    }
    false
}

/// `simulateCanInsert(resourceContainer, resource)`.
fn simulate_can_insert<R: Resource>(container: &dyn ResourceContainer<R>, resource: &R) -> bool {
    container.is_valid_for_insertion(resource, AutomationType::Internal)
        && container.resource().is_none_or(|held| &held == resource)
}

/// `canInsertNonEmpty`: o item aceita o que o contêiner guarda.
fn can_insert_non_empty<R: Resource>(
    container: &dyn ResourceContainer<R>,
    handler: &dyn ItemResourceHandler<R>,
) -> bool {
    let Some(resource) = container.resource() else {
        return false;
    };
    let simulation = Transaction::open_safe();
    handler.insert(
        &resource,
        container.capacity_as_int(Some(&resource)),
        &simulation,
    ) > 0
}

/// `ResourceHandlerSlot.canInput(resourceContainer, itemAccess, capability)`, com o item numa pilha solta: o que o
/// `FluidInventorySlot.input` pergunta para deixar um item entrar por fora e para não deixá-lo sair.
pub fn can_input<R: Resource>(
    container: &dyn ResourceContainer<R>,
    item: &ItemResource,
    lookup: &ItemHandlerLookup<R>,
) -> bool {
    let loose = StackAccess::of(item);
    let Some(handler) = lookup(&loose) else {
        return false;
    };
    let mut has_empty = false;
    for index in 0..handler.size() {
        match handler.resource(index) {
            None => has_empty = true,
            Some(resource) => {
                if simulate_can_insert(container, &resource) {
                    return true;
                }
            }
        }
    }
    if container.is_empty() {
        return has_empty;
    }
    can_insert_non_empty(container, handler.as_ref())
}

/// `ResourceHandlerSlot.canFill`: o item tem um recurso que o contêiner aceita.
pub fn can_fill<R: Resource>(
    container: &dyn ResourceContainer<R>,
    item: &ItemResource,
    lookup: &ItemHandlerLookup<R>,
) -> bool {
    let loose = StackAccess::of(item);
    let Some(handler) = lookup(&loose) else {
        return false;
    };
    (0..handler.size()).any(|index| {
        handler
            .resource(index)
            .is_some_and(|resource| simulate_can_insert(container, &resource))
    })
}

/// `ResourceHandlerSlot.canDrain`: o item pode receber o que o contêiner guarda, ou, com o contêiner vazio, não está cheio.
pub fn can_drain<R: Resource>(
    container: &dyn ResourceContainer<R>,
    item: &ItemResource,
    lookup: &ItemHandlerLookup<R>,
) -> bool {
    let loose = StackAccess::of(item);
    let Some(handler) = lookup(&loose) else {
        return false;
    };
    if container.is_empty() {
        return !handler.is_full();
    }
    can_insert_non_empty(container, handler.as_ref())
}
