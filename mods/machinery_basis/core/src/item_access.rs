//! Porte do `ItemAccess` do NeoForge (o lugar onde um item está, visto por uma capability de item), dos acessos que o Mekanism usa
//! (`InventorySlotItemAccess`, `InOutSlotResourceItemAccess`, `OneByOneItemAccess`, `SideEffectFreeItemAccess`) e do
//! `ItemAccessResourceHandler`, a base do handler de recurso de um item que troca de item ao encher e ao esvaziar (o balde:
//! `BucketResourceHandler`).
//!
//! A lib não conhece nenhum item: quem usa diz, por [`PerItemRules`], o que cada item guarda e em que item ele vira.
use crate::containers::{AutomationType, InventorySlot, ItemResource};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::rc::Rc;

/// `ItemAccess`.
pub trait ItemAccess {
    /// `getResource`: `None` é o vazio.
    fn resource(&self) -> Option<ItemResource>;
    /// `getAmount`.
    fn amount(&self) -> i32;
    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32;
    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32;

    /// `exchange`: troca `amount` do item de agora por `new`, ou nada.
    fn exchange(&self, new: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        let Some(current) = self.resource() else {
            return 0;
        };
        let sub_transaction = Transaction::open(Some(transaction));
        let extracted = self.extract(&current, amount, &sub_transaction);
        if extracted > 0 && self.insert(new, extracted, &sub_transaction) == extracted {
            sub_transaction.commit();
            return extracted;
        }
        0
    }
}

fn to_i32(amount: u32) -> i32 {
    i32::try_from(amount).unwrap_or(i32::MAX)
}

fn to_u32(amount: i32) -> u32 {
    u32::try_from(amount).unwrap_or(0)
}

/// `InventorySlotItemAccess`: o slot, com o tipo de automação de quem mexe.
#[derive(Clone)]
pub struct SlotAccess {
    pub slot: InventorySlot,
    pub automation: AutomationType,
}

impl ItemAccess for SlotAccess {
    fn resource(&self) -> Option<ItemResource> {
        self.slot.resource()
    }

    fn amount(&self) -> i32 {
        to_i32(self.slot.amount())
    }

    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        to_i32(
            self.slot
                .insert(resource, to_u32(amount), transaction, self.automation),
        )
    }

    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        to_i32(
            self.slot
                .extract(resource, to_u32(amount), transaction, self.automation),
        )
    }
}

/// `OneByOneItemAccess`: o mesmo lugar, um item por vez.
pub struct OneByOne<'a>(pub &'a dyn ItemAccess);

impl ItemAccess for OneByOne<'_> {
    fn resource(&self) -> Option<ItemResource> {
        self.0.resource()
    }

    fn amount(&self) -> i32 {
        self.0.amount().min(1)
    }

    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        self.0.insert(resource, amount.min(1), transaction)
    }

    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        self.0.extract(resource, amount.min(1), transaction)
    }
}

struct StackState {
    resource: Option<ItemResource>,
    stored: i32,
}

impl Journaled for StackState {
    type Snapshot = (Option<ItemResource>, i32);

    fn revert_to_snapshot(&mut self, snapshot: Self::Snapshot) {
        (self.resource, self.stored) = snapshot;
    }

    fn on_root_commit(&mut self, _original: Self::Snapshot) -> Vec<Callback> {
        Vec::new()
    }
}

/// `SideEffectFreeItemAccess`: uma pilha solta, que não é de ninguém.
pub struct StackAccess {
    cell: Rc<JournalCell<StackState>>,
}

impl StackAccess {
    /// `ItemAccessUtils.sideEffectFreeAccess(item)`: um item só.
    pub fn of(resource: &ItemResource) -> Self {
        Self::with_amount(resource, 1)
    }

    pub fn with_amount(resource: &ItemResource, amount: i32) -> Self {
        Self {
            cell: JournalCell::new(StackState {
                resource: Some(resource.clone()),
                stored: amount,
            }),
        }
    }

    fn snapshot(&self) -> (Option<ItemResource>, i32) {
        let state = self.cell.borrow();
        (state.resource.clone(), state.stored)
    }
}

impl ItemAccess for StackAccess {
    fn resource(&self) -> Option<ItemResource> {
        self.cell.borrow().resource.clone()
    }

    fn amount(&self) -> i32 {
        self.cell.borrow().stored
    }

    fn insert(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        let (current, _) = self.snapshot();
        if amount == 0 || current.as_ref().is_some_and(|current| current != resource) {
            return 0;
        }
        self.cell.update_snapshots(transaction, || self.snapshot());
        let mut state = self.cell.borrow_mut();
        state.resource = Some(resource.clone());
        state.stored += amount;
        amount
    }

    fn extract(&self, resource: &ItemResource, amount: i32, transaction: &Transaction) -> i32 {
        let (current, stored) = self.snapshot();
        if amount == 0 || current.as_ref() != Some(resource) || stored == 0 {
            return 0;
        }
        let removed = amount.min(stored);
        self.cell.update_snapshots(transaction, || self.snapshot());
        let mut state = self.cell.borrow_mut();
        state.stored -= removed;
        if state.stored == 0 {
            state.resource = None;
        }
        removed
    }
}

/// `ResourceHandler<R>` de um item, o que a capability do item devolve: os recursos que ele guarda e a troca deles dentro de uma
/// transação. O `insert` e o `extract` sem índice são os do NeoForge para um handler de um contêiner só.
pub trait ItemResourceHandler<R> {
    /// `size`.
    fn size(&self) -> usize;
    /// `getResource`: `None` é o vazio.
    fn resource(&self, index: usize) -> Option<R>;
    /// `getAmountAsInt`.
    fn amount_as_int(&self, index: usize) -> i32;
    /// `insert(resource, amount, transaction)`.
    fn insert(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32;
    /// `extract(resource, amount, transaction)`.
    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32;
    /// `extract(index, resource, amount, transaction)`.
    fn extract_at(&self, index: usize, resource: &R, amount: i32, transaction: &Transaction)
    -> i32;
    /// `ResourceHandlerUtil.isFull`: todo contêiner está na capacidade.
    fn is_full(&self) -> bool;
}

/// A capability de recurso de um item (`ItemCapability<ResourceHandler<R>, ItemAccess>`): dado o lugar do item, o handler dele, ou
/// `None` se o item de agora não tem.
pub type ItemHandlerLookup<R> =
    Rc<dyn for<'a> Fn(&'a dyn ItemAccess) -> Option<Box<dyn ItemResourceHandler<R> + 'a>>>;

/// O que um `ItemAccessResourceHandler` de um contêiner só pergunta ao item: o que ele guarda e em que item ele vira.
pub trait PerItemRules<R> {
    /// `getResourceFrom`: o recurso que o item guarda, ou `None`.
    fn resource_from(&self, item: &ItemResource) -> Option<R>;
    /// `getAmountFrom`: quanto um item guarda.
    fn amount_from(&self, item: &ItemResource) -> i32;
    /// `getCapacity`.
    fn capacity(&self, resource: &R) -> i32;
    /// `update`: o item que guarda `amount` de `resource`. `None` é o `ItemResource.EMPTY` do Java (o item não existe com essa
    /// quantidade, e nada acontece).
    fn update(&self, item: &ItemResource, resource: &R, amount: i32) -> Option<ItemResource>;
}

/// `ItemAccessResourceHandler` com um contêiner (o `BucketResourceHandler`).
pub struct AccessResourceHandler<'a, R> {
    pub access: &'a dyn ItemAccess,
    pub rules: Rc<dyn PerItemRules<R>>,
}

impl<R: PartialEq> ItemResourceHandler<R> for AccessResourceHandler<'_, R> {
    fn size(&self) -> usize {
        1
    }

    fn resource(&self, _index: usize) -> Option<R> {
        self.rules.resource_from(&self.access.resource()?)
    }

    fn amount_as_int(&self, _index: usize) -> i32 {
        let Some(item) = self.access.resource() else {
            return 0;
        };
        self.access
            .amount()
            .saturating_mul(self.rules.amount_from(&item))
    }

    fn insert(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        let access_amount = self.access.amount();
        if access_amount == 0 {
            return 0;
        }
        let Some(item) = self.access.resource() else {
            return 0;
        };
        let amount_per_item = amount / access_amount;
        let current_per_item = self.rules.amount_from(&item);
        if current_per_item == 0 || self.rules.resource_from(&item).as_ref() == Some(resource) {
            let inserted_per_item =
                amount_per_item.min(self.rules.capacity(resource) - current_per_item);
            if inserted_per_item > 0
                && let Some(filled) =
                    self.rules
                        .update(&item, resource, inserted_per_item + current_per_item)
            {
                return inserted_per_item
                    * self.access.exchange(&filled, access_amount, transaction);
            }
        }
        0
    }

    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        self.extract_at(0, resource, amount, transaction)
    }

    fn extract_at(
        &self,
        _index: usize,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        let access_amount = self.access.amount();
        if access_amount == 0 {
            return 0;
        }
        let Some(item) = self.access.resource() else {
            return 0;
        };
        if self.rules.resource_from(&item).as_ref() == Some(resource) {
            let current_per_item = self.rules.amount_from(&item);
            let extracted_per_item = (amount / access_amount).min(current_per_item);
            if extracted_per_item > 0
                && let Some(emptied) =
                    self.rules
                        .update(&item, resource, current_per_item - extracted_per_item)
            {
                return extracted_per_item
                    * self.access.exchange(&emptied, access_amount, transaction);
            }
        }
        0
    }

    fn is_full(&self) -> bool {
        // `getAmountAsLong(i) >= getCapacityAsLong(i, getResource(i))`, e o vazio tem a capacidade de qualquer recurso.
        let Some(item) = self.access.resource() else {
            return true;
        };
        match self.rules.resource_from(&item) {
            Some(resource) => self.rules.amount_from(&item) >= self.rules.capacity(&resource),
            None => false,
        }
    }
}
