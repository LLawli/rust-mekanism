//! Porte de `MachineEnergyContainer` e `BasicInventorySlot` (com `OutputInventorySlot`): objetos compartilhados, que se
//! registram sozinhos na transação que os muda e avisam quem escuta no commit da raiz.
use crate::transaction::{Callback, JournalCell, Journaled, Transaction, fire};
use std::fmt;
use std::rc::Rc;

/// Quem está mexendo no contêiner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationType {
    External,
    Internal,
    Manual,
}

/// Os predicados de `AutomationType` que o Mekanism passa aos contêineres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// `alwaysTrue`.
    Any,
    /// `notExternal`.
    NotExternal,
    /// `internalOnly`.
    InternalOnly,
}

impl Access {
    pub fn allows(self, automation: AutomationType) -> bool {
        match self {
            Self::Any => true,
            Self::NotExternal => automation != AutomationType::External,
            Self::InternalOnly => automation == AutomationType::Internal,
        }
    }
}

fn saturated_cast(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

struct EnergyState {
    stored: i64,
    base_max_energy: i64,
    current_max_energy: i64,
    base_energy_per_tick: i32,
    current_energy_per_tick: i32,
    can_extract: Access,
    can_insert: Access,
    listener: Option<Callback>,
}

impl Journaled for EnergyState {
    type Snapshot = i64;

    fn revert_to_snapshot(&mut self, snapshot: i64) {
        // Sem avisar: o `revertToSnapshot` do Java ignora a checagem de mudança.
        self.stored = snapshot;
    }

    fn on_root_commit(&mut self, original: i64) -> Vec<Callback> {
        if self.stored == original {
            return Vec::new();
        }
        self.listener.iter().cloned().collect()
    }
}

/// `MachineEnergyContainer`: energia em `i64`, com capacidade e consumo que mudam com os upgrades.
#[derive(Clone)]
pub struct EnergyContainer {
    cell: Rc<JournalCell<EnergyState>>,
}

impl fmt::Debug for EnergyContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EnergyContainer({})", self.amount())
    }
}

impl EnergyContainer {
    /// `MachineEnergyContainer.input`: quem está de fora não tira energia da máquina.
    pub fn input(max_energy: i64, energy_per_tick: i32, listener: Option<Callback>) -> Self {
        Self::new(
            max_energy,
            energy_per_tick,
            Access::NotExternal,
            Access::Any,
            listener,
        )
    }

    pub fn new(
        max_energy: i64,
        energy_per_tick: i32,
        can_extract: Access,
        can_insert: Access,
        listener: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(EnergyState {
                stored: 0,
                base_max_energy: max_energy,
                current_max_energy: max_energy,
                base_energy_per_tick: energy_per_tick,
                current_energy_per_tick: energy_per_tick,
                can_extract,
                can_insert,
                listener,
            }),
        }
    }

    pub fn amount(&self) -> i64 {
        self.cell.borrow().stored
    }

    /// Satura como `getAmountAsInt`.
    pub fn amount_as_int(&self) -> i32 {
        saturated_cast(self.amount())
    }

    /// Nunca abaixo do guardado: é o `Math.max` do Java.
    pub fn capacity(&self) -> i64 {
        let state = self.cell.borrow();
        state.current_max_energy.max(state.stored)
    }

    pub fn base_max_energy(&self) -> i64 {
        self.cell.borrow().base_max_energy
    }

    /// Como a capacidade nunca fica abaixo do guardado, o clamp do Java não corta nada aqui.
    pub fn set_max_energy(&self, max_energy: i64) {
        self.cell.borrow_mut().current_max_energy = max_energy;
    }

    pub fn energy_per_tick(&self) -> i32 {
        self.cell.borrow().current_energy_per_tick
    }

    pub fn base_energy_per_tick(&self) -> i32 {
        self.cell.borrow().base_energy_per_tick
    }

    pub fn set_energy_per_tick(&self, energy_per_tick: i32) {
        self.cell.borrow_mut().current_energy_per_tick = energy_per_tick;
    }

    /// `setEnergy`: sem transação, o aviso dispara na hora.
    pub fn set_energy(&self, energy: i64, transaction: Option<&Transaction>) {
        let callbacks = {
            let mut state = self.cell.borrow_mut();
            if state.stored == energy {
                return;
            }
            match transaction {
                Some(transaction) => {
                    self.cell.update_snapshots(transaction, || state.stored);
                    state.stored = energy;
                    Vec::new()
                }
                None => {
                    state.stored = energy;
                    state.listener.iter().cloned().collect()
                }
            }
        };
        fire(callbacks);
    }

    pub fn extract(
        &self,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        let (stored, to_remove) = {
            let state = self.cell.borrow();
            if state.stored == 0 || amount == 0 || !state.can_extract.allows(automation) {
                return 0;
            }
            (state.stored, amount.min(saturated_cast(state.stored)))
        };
        if to_remove > 0 {
            self.set_energy(stored - i64::from(to_remove), Some(transaction));
        }
        to_remove
    }

    pub fn insert(
        &self,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 || !self.cell.borrow().can_insert.allows(automation) {
            return 0;
        }
        let stored = self.amount();
        let needed = saturated_cast(self.capacity() - stored);
        if needed <= 0 {
            return 0;
        }
        let to_add = amount.min(needed);
        self.set_energy(stored + i64::from(to_add), Some(transaction));
        to_add
    }
}

/// Um item, com o limite da pilha dele. A lib não conhece ids: quem usa diz qual é.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemResource {
    pub id: Rc<str>,
    pub max_stack_size: u32,
}

impl ItemResource {
    pub fn new(id: &str, max_stack_size: u32) -> Self {
        Self {
            id: Rc::from(id),
            max_stack_size,
        }
    }
}

/// `Item.ABSOLUTE_MAX_STACK_SIZE`: a capacidade do slot, antes do limite da pilha do item.
const ABSOLUTE_MAX_STACK_SIZE: u32 = 99;

/// `LargeResourceStack`: o que um slot guarda. Vazio é sem recurso e com zero.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotContents {
    pub resource: Option<ItemResource>,
    pub amount: u32,
}

impl SlotContents {
    pub fn new(resource: Option<ItemResource>, amount: u32) -> Self {
        if amount == 0 || resource.is_none() {
            return Self::default();
        }
        Self { resource, amount }
    }

    pub fn is_empty(&self) -> bool {
        self.resource.is_none()
    }
}

struct SlotState {
    contents: SlotContents,
    can_extract: Access,
    can_insert: Access,
    listener: Option<Callback>,
    /// `OutputInventorySlot.onDecrease`.
    on_decrease: Option<Callback>,
}

impl SlotState {
    fn capacity_for(resource: Option<&ItemResource>) -> u32 {
        resource.map_or(ABSOLUTE_MAX_STACK_SIZE, |resource| {
            ABSOLUTE_MAX_STACK_SIZE.min(resource.max_stack_size)
        })
    }

    /// `onContentsChanged(originalState)`, do slot e do slot de saída: primeiro o aviso, depois o `onDecrease`.
    fn contents_changed(&self, original: &SlotContents) -> Vec<Callback> {
        let mut callbacks: Vec<Callback> = self.listener.iter().cloned().collect();
        if let Some(on_decrease) = &self.on_decrease
            && !original.is_empty()
            && (original.resource != self.contents.resource
                || self.contents.amount < original.amount)
        {
            callbacks.push(on_decrease.clone());
        }
        callbacks
    }
}

impl Journaled for SlotState {
    type Snapshot = SlotContents;

    fn revert_to_snapshot(&mut self, snapshot: SlotContents) {
        self.contents = snapshot;
    }

    fn on_root_commit(&mut self, original: SlotContents) -> Vec<Callback> {
        if original == self.contents {
            return Vec::new();
        }
        self.contents_changed(&original)
    }
}

/// Slot de item. Compartilhado: quem o guarda e a transação que o muda falam com o mesmo objeto.
#[derive(Clone)]
pub struct InventorySlot {
    cell: Rc<JournalCell<SlotState>>,
}

impl fmt::Debug for InventorySlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InventorySlot({:?})", self.cell.borrow().contents)
    }
}

impl InventorySlot {
    /// `BasicInventorySlot.at(listener, ..)`.
    pub fn new(listener: Option<Callback>) -> Self {
        Self::with(Access::Any, Access::Any, listener, None)
    }

    /// `OutputInventorySlot.at(listener, ..)`: só quem é interno insere, e `on_decrease` avisa quando a quantidade cai.
    pub fn output(listener: Option<Callback>, on_decrease: Option<Callback>) -> Self {
        Self::with(Access::Any, Access::InternalOnly, listener, on_decrease)
    }

    pub fn with(
        can_extract: Access,
        can_insert: Access,
        listener: Option<Callback>,
        on_decrease: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(SlotState {
                contents: SlotContents::default(),
                can_extract,
                can_insert,
                listener,
                on_decrease,
            }),
        }
    }

    pub fn same_slot(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }

    pub fn resource(&self) -> Option<ItemResource> {
        self.cell.borrow().contents.resource.clone()
    }

    pub fn amount(&self) -> u32 {
        self.cell.borrow().contents.amount
    }

    pub fn contents(&self) -> SlotContents {
        self.cell.borrow().contents.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.cell.borrow().contents.is_empty()
    }

    /// Um slot vazio tem capacidade, e é por isso que ele entra na média do comparador.
    pub fn capacity(&self) -> u32 {
        SlotState::capacity_for(self.cell.borrow().contents.resource.as_ref())
    }

    pub fn is_full(&self) -> bool {
        let state = self.cell.borrow();
        state.contents.amount >= SlotState::capacity_for(state.contents.resource.as_ref())
    }

    /// `setContents`: sem transação, o aviso dispara na hora, e só se o conteúdo mudou.
    pub fn set_contents(
        &self,
        resource: Option<ItemResource>,
        amount: u32,
        transaction: Option<&Transaction>,
    ) {
        let new = SlotContents::new(resource, amount);
        let callbacks = {
            let mut state = self.cell.borrow_mut();
            if state.contents == new {
                return;
            }
            match transaction {
                Some(transaction) => {
                    self.cell
                        .update_snapshots(transaction, || state.contents.clone());
                    state.contents = new;
                    Vec::new()
                }
                None => {
                    let original = std::mem::replace(&mut state.contents, new);
                    state.contents_changed(&original)
                }
            }
        };
        fire(callbacks);
    }

    pub fn insert(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> u32 {
        let (stored, to_add) = {
            let state = self.cell.borrow();
            if amount == 0 {
                return 0;
            }
            if state
                .contents
                .resource
                .as_ref()
                .is_some_and(|held| held != resource)
            {
                return 0;
            }
            let stored = state.contents.amount;
            let needed = SlotState::capacity_for(Some(resource)).saturating_sub(stored);
            if needed == 0 || !state.can_insert.allows(automation) {
                return 0;
            }
            (stored, amount.min(needed))
        };
        self.set_contents(Some(resource.clone()), stored + to_add, Some(transaction));
        to_add
    }

    pub fn extract(
        &self,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> u32 {
        let (stored, to_remove) = {
            let state = self.cell.borrow();
            if amount == 0
                || state.contents.resource.as_ref() != Some(resource)
                || !state.can_extract.allows(automation)
            {
                return 0;
            }
            let stored = state.contents.amount;
            (stored, amount.min(stored))
        };
        if to_remove > 0 {
            self.set_contents(
                Some(resource.clone()),
                stored - to_remove,
                Some(transaction),
            );
        }
        to_remove
    }
}

/// `ResourceContainerType.getRedstoneSignalFromContainers`, em `f32` como o Java.
pub fn redstone_signal(slots: &[&InventorySlot]) -> i32 {
    const SIGNAL_MAX: i32 = 15;
    let mut proportion = 0.0_f32;
    let mut sample_count = 0;
    for slot in slots {
        let capacity = slot.capacity();
        if capacity > 0 {
            if slot.amount() > 0 {
                proportion += (slot.amount() as f32 / capacity as f32).min(1.0);
            }
            sample_count += 1;
        }
    }
    if sample_count == 0 {
        return 0;
    }
    proportion /= sample_count as f32;
    // `Mth.lerpDiscrete`.
    (proportion * (SIGNAL_MAX - 1) as f32).floor() as i32 + i32::from(proportion > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn clay() -> ItemResource {
        ItemResource::new("clay", 64)
    }

    fn counting() -> (Callback, Rc<Cell<u32>>) {
        let count = Rc::new(Cell::new(0));
        let inner = count.clone();
        (Rc::new(move || inner.set(inner.get() + 1)), count)
    }

    #[test]
    fn capacity_follows_what_is_stored() {
        let energy = EnergyContainer::input(20, 5, None);
        energy.set_energy(1_200, None);
        assert_eq!(energy.capacity(), 1_200);
        energy.set_energy(3, None);
        assert_eq!(energy.capacity(), 20);
    }

    #[test]
    fn a_rolled_back_change_says_nothing() {
        let (listener, count) = counting();
        let slot = InventorySlot::new(Some(listener));
        let root = Transaction::open_root();
        slot.insert(&clay(), 3, &root, AutomationType::Internal);
        drop(root);
        assert!(slot.is_empty());
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn a_commit_back_to_the_original_says_nothing() {
        let (listener, count) = counting();
        let energy = EnergyContainer::input(100, 5, Some(listener));
        energy.set_energy(50, None);
        assert_eq!(count.get(), 1);
        let root = Transaction::open_root();
        energy.extract(10, &root, AutomationType::Internal);
        energy.insert(10, &root, AutomationType::Internal);
        root.commit();
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn a_committed_change_says_it_once_at_the_root() {
        let (listener, count) = counting();
        let slot = InventorySlot::new(Some(listener));
        let root = Transaction::open_root();
        let nested = Transaction::open(Some(&root));
        slot.insert(&clay(), 3, &nested, AutomationType::Internal);
        nested.commit();
        assert_eq!(count.get(), 0);
        root.commit();
        assert_eq!(count.get(), 1);
        assert_eq!(slot.amount(), 3);
    }

    #[test]
    fn a_slot_holds_what_the_item_stacks_to() {
        let root = Transaction::open_root();
        for (max_stack_size, expected) in [(16, 16), (64, 64), (99, 99)] {
            let slot = InventorySlot::new(None);
            let item = ItemResource::new("item", max_stack_size);
            assert_eq!(
                slot.insert(&item, 200, &root, AutomationType::Internal),
                expected
            );
            assert!(slot.is_full());
        }
    }

    #[test]
    fn the_output_slot_only_takes_internal_inserts() {
        let root = Transaction::open_root();
        let slot = InventorySlot::output(None, None);
        assert_eq!(slot.insert(&clay(), 3, &root, AutomationType::External), 0);
        assert_eq!(slot.insert(&clay(), 3, &root, AutomationType::Internal), 3);
        assert_eq!(slot.extract(&clay(), 2, &root, AutomationType::External), 2);
    }

    #[test]
    fn a_decrease_is_reported_to_the_output_slot() {
        let (decrease, count) = counting();
        let slot = InventorySlot::output(None, Some(decrease));
        slot.set_contents(Some(clay()), 5, None);
        assert_eq!(count.get(), 0);
        slot.set_contents(Some(clay()), 6, None);
        assert_eq!(count.get(), 0);
        slot.set_contents(Some(clay()), 2, None);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn the_signal_matches_the_recorded_levels() {
        let empty = InventorySlot::new(None);
        let input = InventorySlot::new(None);
        let output = InventorySlot::new(None);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 0);
        input.set_contents(Some(clay()), 3, None);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 1);
        input.set_contents(Some(clay()), 64, None);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 5);
        input.set_contents(Some(clay()), 62, None);
        output.set_contents(Some(ItemResource::new("clay_ball", 64)), 8, None);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 6);
    }
}
