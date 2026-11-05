//! Porte de `MachineEnergyContainer` e `BasicInventorySlot`: guardam o conteúdo dentro das transações e dizem quando mudou.
use crate::transaction::{SnapshotJournal, Transaction};

/// Quem está mexendo no contêiner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationType {
    External,
    Internal,
    Manual,
}

/// O que o contêiner avisa ao mudar, no lugar do `IContentsListener` do Java.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentsListener {
    /// `onContentsChanged` do tile.
    Tile,
    /// `RecipeCacheLookupMonitor`: avisa o tile, volta a procurar receita e despausa.
    RecipeCache,
    /// `getRecipeCacheUnpauseListener(this)`: avisa o tile e despausa.
    RecipeCacheUnpause,
    /// `getRecipeCacheUnpauseListener` com o listener de salvar: só salva e despausa.
    RecipeCacheUnpauseSaveOnly,
}

/// Energia em `i64`, com capacidade e consumo que mudam com os upgrades.
#[derive(Clone, Debug)]
pub struct MachineEnergyContainer {
    stored: i64,
    journal: SnapshotJournal<i64>,
    base_max_energy: i64,
    current_max_energy: i64,
    base_energy_per_tick: i32,
    current_energy_per_tick: i32,
    listener: ContentsListener,
    changed: bool,
}

impl MachineEnergyContainer {
    pub fn new(max_energy: i64, energy_per_tick: i32, listener: ContentsListener) -> Self {
        Self {
            stored: 0,
            journal: SnapshotJournal::default(),
            base_max_energy: max_energy,
            current_max_energy: max_energy,
            base_energy_per_tick: energy_per_tick,
            current_energy_per_tick: energy_per_tick,
            listener,
            changed: false,
        }
    }

    pub fn amount(&self) -> i64 {
        self.stored
    }

    /// Satura como `getAmountAsInt`.
    pub fn amount_as_int(&self) -> i32 {
        saturated_cast(self.stored)
    }

    /// Nunca abaixo do guardado: é o `Math.max` do Java.
    pub fn capacity(&self) -> i64 {
        self.current_max_energy.max(self.stored)
    }

    pub fn base_max_energy(&self) -> i64 {
        self.base_max_energy
    }

    /// Como a capacidade nunca fica abaixo do guardado, o clamp do Java não corta nada aqui.
    pub fn set_max_energy(&mut self, max_energy: i64) {
        self.current_max_energy = max_energy;
    }

    pub fn energy_per_tick(&self) -> i32 {
        self.current_energy_per_tick
    }

    pub fn base_energy_per_tick(&self) -> i32 {
        self.base_energy_per_tick
    }

    pub fn set_energy_per_tick(&mut self, energy_per_tick: i32) {
        self.current_energy_per_tick = energy_per_tick;
    }

    /// Fora de transação, como `setEnergy(energy, null)`.
    pub fn set_energy(&mut self, energy: i64) {
        if self.stored != energy {
            self.stored = energy;
            self.changed = true;
        }
    }

    pub fn extract(
        &mut self,
        amount: i32,
        transaction: Transaction,
        automation: AutomationType,
    ) -> i32 {
        // `notExternal`: quem está de fora não tira energia da máquina.
        if self.stored == 0 || amount == 0 || automation == AutomationType::External {
            return 0;
        }
        let to_remove = amount.min(saturated_cast(self.stored));
        if to_remove > 0 {
            self.journal.update_snapshots(transaction, &self.stored);
            self.stored -= i64::from(to_remove);
        }
        to_remove
    }

    pub fn insert(&mut self, amount: i32, transaction: Transaction) -> i32 {
        let needed = saturated_cast(self.capacity() - self.stored);
        if amount == 0 || needed <= 0 {
            return 0;
        }
        let to_add = amount.min(needed);
        self.journal.update_snapshots(transaction, &self.stored);
        self.stored += i64::from(to_add);
        to_add
    }

    pub fn close(&mut self, transaction: Transaction, commit: bool) {
        if let Some(original) = self.journal.close(transaction, commit, &mut self.stored)
            && original != self.stored
        {
            self.changed = true;
        }
    }

    /// O listener a chamar, se o conteúdo mudou desde a última vez.
    pub fn take_change(&mut self) -> Option<ContentsListener> {
        std::mem::take(&mut self.changed).then_some(self.listener)
    }
}

fn saturated_cast(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `Item.ABSOLUTE_MAX_STACK_SIZE`: a capacidade do slot, antes do limite da pilha do item.
const ABSOLUTE_MAX_STACK_SIZE: u32 = 99;

/// Um item, com o limite da pilha dele. A lib não conhece ids: quem usa diz qual é.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemResource {
    pub id: String,
    pub max_stack_size: u32,
}

impl ItemResource {
    pub fn new(id: impl Into<String>, max_stack_size: u32) -> Self {
        Self {
            id: id.into(),
            max_stack_size,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SlotContents {
    resource: Option<ItemResource>,
    amount: u32,
}

/// Slot de item. `onDecrease` do slot de saída vira [`InventorySlot::take_decrease`].
#[derive(Clone, Debug)]
pub struct InventorySlot {
    contents: SlotContents,
    journal: SnapshotJournal<SlotContents>,
    listener: ContentsListener,
    changed: bool,
    decreased: bool,
}

impl InventorySlot {
    pub fn new(listener: ContentsListener) -> Self {
        Self {
            contents: SlotContents::default(),
            journal: SnapshotJournal::default(),
            listener,
            changed: false,
            decreased: false,
        }
    }

    pub fn resource(&self) -> Option<&ItemResource> {
        self.contents.resource.as_ref()
    }

    pub fn amount(&self) -> u32 {
        self.contents.amount
    }

    pub fn is_empty(&self) -> bool {
        self.contents.resource.is_none()
    }

    /// Um slot vazio tem capacidade, e é por isso que ele entra na média do comparador.
    pub fn capacity(&self) -> u32 {
        self.capacity_for(self.contents.resource.as_ref())
    }

    fn capacity_for(&self, resource: Option<&ItemResource>) -> u32 {
        resource.map_or(ABSOLUTE_MAX_STACK_SIZE, |resource| {
            ABSOLUTE_MAX_STACK_SIZE.min(resource.max_stack_size)
        })
    }

    pub fn is_full(&self) -> bool {
        self.contents.amount >= self.capacity()
    }

    /// Fora de transação, como `setContents(type, amount, null)`.
    pub fn set_contents(&mut self, resource: Option<ItemResource>, amount: u32) {
        let new = SlotContents {
            amount: if resource.is_some() { amount } else { 0 },
            resource: resource.filter(|_| amount > 0),
        };
        if self.contents != new {
            let original = std::mem::replace(&mut self.contents, new);
            self.note_change(&original);
        }
    }

    pub fn insert(
        &mut self,
        resource: &ItemResource,
        amount: u32,
        transaction: Transaction,
    ) -> u32 {
        if self.resource().is_some_and(|stored| stored != resource) {
            return 0;
        }
        let room = self
            .capacity_for(Some(resource))
            .saturating_sub(self.contents.amount);
        let to_add = amount.min(room);
        if to_add > 0 {
            self.journal.update_snapshots(transaction, &self.contents);
            self.contents.resource = Some(resource.clone());
            self.contents.amount += to_add;
        }
        to_add
    }

    pub fn extract(
        &mut self,
        resource: &ItemResource,
        amount: u32,
        transaction: Transaction,
    ) -> u32 {
        if self.resource() != Some(resource) {
            return 0;
        }
        let to_remove = amount.min(self.contents.amount);
        if to_remove > 0 {
            self.journal.update_snapshots(transaction, &self.contents);
            self.contents.amount -= to_remove;
            if self.contents.amount == 0 {
                self.contents.resource = None;
            }
        }
        to_remove
    }

    pub fn close(&mut self, transaction: Transaction, commit: bool) {
        if let Some(original) = self.journal.close(transaction, commit, &mut self.contents)
            && original != self.contents
        {
            self.note_change(&original);
        }
    }

    fn note_change(&mut self, original: &SlotContents) {
        self.changed = true;
        if original.resource.is_some()
            && (original.resource != self.contents.resource
                || self.contents.amount < original.amount)
        {
            self.decreased = true;
        }
    }

    pub fn take_change(&mut self) -> Option<ContentsListener> {
        std::mem::take(&mut self.changed).then_some(self.listener)
    }

    /// Se a quantidade caiu desde a última vez, o `onDecrease` do `OutputInventorySlot`.
    pub fn take_decrease(&mut self) -> bool {
        std::mem::take(&mut self.decreased)
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

    fn clay() -> ItemResource {
        ItemResource::new("clay", 64)
    }

    #[test]
    fn capacity_follows_what_is_stored() {
        let mut energy = MachineEnergyContainer::new(20, 5, ContentsListener::Tile);
        energy.set_energy(1_200);
        assert_eq!(energy.capacity(), 1_200);
        energy.set_energy(3);
        assert_eq!(energy.capacity(), 20);
    }

    #[test]
    fn a_rolled_back_change_says_nothing() {
        let root = Transaction::open_root();
        let mut slot = InventorySlot::new(ContentsListener::Tile);
        slot.insert(&clay(), 3, root);
        slot.close(root, false);
        assert!(slot.is_empty());
        assert_eq!(slot.take_change(), None);
    }

    #[test]
    fn a_commit_back_to_the_original_says_nothing() {
        let root = Transaction::open_root();
        let mut energy = MachineEnergyContainer::new(100, 5, ContentsListener::Tile);
        energy.set_energy(50);
        energy.take_change();
        energy.extract(10, root, AutomationType::Internal);
        energy.insert(10, root);
        energy.close(root, true);
        assert_eq!(energy.take_change(), None);
    }

    #[test]
    fn a_slot_holds_what_the_item_stacks_to() {
        let root = Transaction::open_root();
        for (max_stack_size, expected) in [(16, 16), (64, 64), (99, 99)] {
            let mut slot = InventorySlot::new(ContentsListener::Tile);
            let item = ItemResource::new("item", max_stack_size);
            assert_eq!(slot.insert(&item, 200, root), expected);
            assert!(slot.is_full());
        }
    }

    #[test]
    fn the_signal_matches_the_recorded_levels() {
        let empty = InventorySlot::new(ContentsListener::Tile);
        let mut input = InventorySlot::new(ContentsListener::Tile);
        let mut output = InventorySlot::new(ContentsListener::Tile);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 0);
        input.set_contents(Some(clay()), 3);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 1);
        input.set_contents(Some(clay()), 64);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 5);
        input.set_contents(Some(clay()), 62);
        output.set_contents(Some(ItemResource::new("clay_ball", 64)), 8);
        assert_eq!(redstone_signal(&[&input, &output, &empty]), 6);
    }
}
