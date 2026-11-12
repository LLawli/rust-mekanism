//! Os contêineres de energia da rede e do cabo: `BasicEnergyContainer` (o buffer do `UniversalCable`) e `VariableCapacityEnergyContainer` (o
//! `energyContainer` do `EnergyNetwork`, cuja capacidade é a da rede), sem limite de taxa. O diário de transação é o do `JournalCell`: o
//! aviso de mudança dispara no commit da raiz, ou na hora quando a mudança vem sem transação (`setEnergy(.., null)`).
use crate::containers::AutomationType;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction, fire};
use std::rc::Rc;

/// `Ints.saturatedCast`.
pub fn saturated(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `MathUtils.addClamped(long, long)`.
pub fn add_clamped(x: i64, y: i64) -> i64 {
    x.saturating_add(y)
}

/// `BasicEnergyContainer.notExternal`.
pub fn not_external(automation: AutomationType) -> bool {
    automation != AutomationType::External
}

/// `ConstantPredicates.alwaysTrue`.
pub fn always(_: AutomationType) -> bool {
    true
}

struct State {
    stored: i64,
    capacity: i64,
    can_extract: fn(AutomationType) -> bool,
    can_insert: fn(AutomationType) -> bool,
    listener: Option<Callback>,
}

impl Journaled for State {
    type Snapshot = i64;

    fn revert_to_snapshot(&mut self, snapshot: i64) {
        self.stored = snapshot;
    }

    fn on_root_commit(&mut self, original: i64) -> Vec<Callback> {
        if self.stored == original {
            return Vec::new();
        }
        self.listener.iter().cloned().collect()
    }
}

/// Um contêiner de energia com diário.
#[derive(Clone)]
pub struct BufferEnergyContainer {
    cell: Rc<JournalCell<State>>,
}

impl BufferEnergyContainer {
    /// `BasicEnergyContainer.create(maxEnergy, canExtract, canInsert, listener)`. Para o de capacidade variável, quem o guarda chama
    /// [`BufferEnergyContainer::set_capacity`] sempre que o valor da fonte (`this::getCapacity`) muda.
    pub fn new(
        capacity: i64,
        can_extract: fn(AutomationType) -> bool,
        can_insert: fn(AutomationType) -> bool,
        listener: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(State {
                stored: 0,
                capacity,
                can_extract,
                can_insert,
                listener,
            }),
        }
    }

    /// `getAmountAsLong`.
    pub fn amount(&self) -> i64 {
        self.cell.borrow().stored
    }

    /// `getCapacityAsLong`.
    pub fn capacity(&self) -> i64 {
        self.cell.borrow().capacity
    }

    /// O valor novo da fonte de capacidade de um `VariableCapacityEnergyContainer`.
    pub fn set_capacity(&self, capacity: i64) {
        self.cell.borrow_mut().capacity = capacity;
    }

    /// `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.amount() == 0
    }

    /// `isValidForInsertion`.
    pub fn is_valid_for_insertion(&self, automation: AutomationType) -> bool {
        (self.cell.borrow().can_insert)(automation)
    }

    /// `isValidForExtraction`.
    pub fn is_valid_for_extraction(&self, automation: AutomationType) -> bool {
        (self.cell.borrow().can_extract)(automation)
    }

    /// `setEnergy`: o Java recusa energia negativa (`MekanismPreconditions.checkNonNegative`).
    pub fn set_energy(&self, energy: i64, transaction: Option<&Transaction>) {
        assert!(energy >= 0, "a energia não pode ser negativa: {energy}");
        let callbacks = {
            let mut state = self.cell.borrow_mut();
            if state.stored == energy {
                return;
            }
            match transaction {
                Some(transaction) => {
                    let stored = state.stored;
                    drop(state);
                    self.cell.update_snapshots(transaction, || stored);
                    self.cell.borrow_mut().stored = energy;
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

    /// `insert`.
    pub fn insert(
        &self,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 || !self.is_valid_for_insertion(automation) {
            return 0;
        }
        let (stored, capacity) = (self.amount(), self.capacity());
        let needed = saturated(capacity - stored);
        if needed <= 0 {
            return 0;
        }
        let to_add = amount.min(needed);
        self.set_energy(stored + i64::from(to_add), Some(transaction));
        to_add
    }

    /// `extract`.
    pub fn extract(
        &self,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if self.is_empty() || amount == 0 || !self.is_valid_for_extraction(automation) {
            return 0;
        }
        let stored = self.amount();
        let to_remove = amount.min(saturated(stored));
        if to_remove > 0 {
            self.set_energy(stored - i64::from(to_remove), Some(transaction));
        }
        to_remove
    }

    /// `ContainerType.ENERGY.clampContents(container, transaction)`: com capacidade zero, um contêiner de capacidade variável não
    /// é cortado (`variable`), porque a rede finge capacidade zero até terminar de se montar.
    pub fn clamp_contents(&self, variable: bool, transaction: Option<&Transaction>) {
        if !self.is_empty() {
            let capacity = self.capacity();
            if capacity == 0 && variable {
                return;
            }
            if self.amount() > capacity {
                self.set_energy(capacity, transaction);
            }
        }
    }

    /// `MathUtils.divideToLevel(amount, capacity)`: 1 se a capacidade é 0 ou a energia a alcança.
    pub fn divide_to_level(&self) -> f64 {
        let (numerator, divisor) = (self.amount() as f64, self.capacity() as f64);
        if divisor == 0.0 || numerator >= divisor {
            1.0
        } else {
            numerator / divisor
        }
    }
}
