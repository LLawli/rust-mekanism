//! Porte de `MachineEnergyContainer` e `BasicInventorySlot` (com `OutputInventorySlot`): objetos compartilhados, que se
//! registram sozinhos na transação que os muda e avisam quem escuta no commit da raiz.
use crate::components::Components;
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
    /// `manualOnly`.
    ManualOnly,
}

impl Access {
    pub fn allows(self, automation: AutomationType) -> bool {
        match self {
            Self::Any => true,
            Self::NotExternal => automation != AutomationType::External,
            Self::InternalOnly => automation == AutomationType::Internal,
            Self::ManualOnly => automation == AutomationType::Manual,
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

    /// `isValidForInsertion(AutomationType)`.
    pub fn is_valid_for_insertion(&self, automation: AutomationType) -> bool {
        self.cell.borrow().can_insert.allows(automation)
    }

    /// `isValidForExtraction(AutomationType)`.
    pub fn is_valid_for_extraction(&self, automation: AutomationType) -> bool {
        self.cell.borrow().can_extract.allows(automation)
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

/// Um item, com o limite da pilha dele e os componentes que o distinguem (`ItemResource`): dois recursos são o mesmo só se o item e
/// os componentes são iguais. A lib não conhece ids nem componentes: quem usa diz quais são.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemResource {
    pub id: Rc<str>,
    pub max_stack_size: u32,
    /// O `DataComponentPatch`. Sem ele, dois itens do mesmo id com componentes diferentes seriam o mesmo recurso.
    pub components: Components,
}

impl ItemResource {
    pub fn new(id: &str, max_stack_size: u32) -> Self {
        Self {
            id: Rc::from(id),
            max_stack_size,
            components: Components::none(),
        }
    }

    /// O item com os componentes dados.
    pub fn with_components(id: &str, max_stack_size: u32, components: Components) -> Self {
        Self {
            components,
            ..Self::new(id, max_stack_size)
        }
    }

    /// O item com um componente só, de id `component` e valor `value`.
    pub fn with_component(id: &str, max_stack_size: u32, component: &str, value: &str) -> Self {
        Self::with_components(id, max_stack_size, Components::new([(component, value)]))
    }

    /// O item com um componente que o distingue dos outros de mesmo id, sem importar qual: para quem só precisa de dois itens
    /// diferentes.
    pub fn with_variant(id: &str, max_stack_size: u32, variant: &str) -> Self {
        Self::with_components(id, max_stack_size, Components::variant(variant))
    }
}

/// O `Predicate<ItemResource>` e o `BiPredicate<ItemResource, AutomationType>` de um slot, já sem o tipo de automação.
pub type ItemTest = Rc<dyn Fn(&ItemResource) -> bool>;

/// O `BiPredicate<ItemResource, AutomationType>` de um slot que decide pelo item e por quem mexe, como o `fillOrConvert`.
pub type AutomationItemTest = Rc<dyn Fn(&ItemResource, AutomationType) -> bool>;

/// `Item.ABSOLUTE_MAX_STACK_SIZE`: a capacidade do slot, antes do limite da pilha do item.
const ABSOLUTE_MAX_STACK_SIZE: u32 = 99;

impl ItemResource {
    /// A capacidade que um slot sem regra de aceitar dá a este recurso: `min(ABSOLUTE_MAX_STACK_SIZE, getMaxStackSize())`.
    pub fn slot_capacity(&self) -> u32 {
        ABSOLUTE_MAX_STACK_SIZE.min(self.max_stack_size)
    }
}

/// Quanto cabe de `incoming` num slot que guarda `held`, com a capacidade `capacity` que o slot dá a ele
/// (`BasicResourceContainer.insert`): nada se o slot guarda outro recurso, que para um item é outro id ou outros componentes, e
/// senão o que falta para a capacidade. É a conta do `InventorySlot::insert`, para quem mexe nos itens sem passar por um slot
/// (o hopper e o shift-clique do jogo).
pub fn room_for(held: &SlotContents, incoming: &ItemResource, capacity: u32) -> u32 {
    if held
        .resource
        .as_ref()
        .is_some_and(|resource| resource != incoming)
    {
        return 0;
    }
    capacity.saturating_sub(held.amount)
}

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

/// O que um slot recusa além do `AutomationType`: o `validator` (`isValid`, que também zera a capacidade) e o item que o
/// `canInsert` aceita.
#[derive(Clone, Default)]
pub struct SlotChecks {
    pub validator: Option<ItemTest>,
    pub can_insert: Option<ItemTest>,
    /// O `canExtract` que olha o item junto com o tipo de automação (`ChemicalInventorySlot.fillOrConvert`).
    pub extract_by: Option<AutomationItemTest>,
    /// O `canInsert` que olha o item junto com o tipo de automação.
    pub insert_by: Option<AutomationItemTest>,
}

impl SlotChecks {
    /// `ChemicalInventorySlot.fillOrConvert` e `EnergyInventorySlot.fillOrConvert`: quem está de fora só tira o que o slot não
    /// consegue encher ou converter (`!external || !canFillOrConvert`), e só põe o que ele consegue (`internal || canFillOrConvert`).
    pub fn fill_or_convert(can_fill_or_convert: ItemTest) -> Self {
        let extract = can_fill_or_convert.clone();
        Self {
            extract_by: Some(Rc::new(move |item, automation| {
                automation != AutomationType::External || !extract(item)
            })),
            insert_by: Some(Rc::new(move |item, automation| {
                automation == AutomationType::Internal || can_fill_or_convert(item)
            })),
            ..Self::default()
        }
    }
}

struct SlotState {
    contents: SlotContents,
    can_extract: Access,
    can_insert: Access,
    checks: SlotChecks,
    listener: Option<Callback>,
    /// `OutputInventorySlot.onDecrease`.
    on_decrease: Option<Callback>,
}

impl SlotState {
    /// `BasicInventorySlot.capacityAsLong`: um item que o `isValid` recusa tem capacidade zero.
    fn capacity_for(&self, resource: Option<&ItemResource>) -> u32 {
        let Some(resource) = resource else {
            return ABSOLUTE_MAX_STACK_SIZE;
        };
        let valid = self
            .checks
            .validator
            .as_ref()
            .is_none_or(|validator| validator(resource));
        if valid { resource.slot_capacity() } else { 0 }
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
        Self::with_checks(
            can_extract,
            can_insert,
            SlotChecks::default(),
            listener,
            on_decrease,
        )
    }

    /// `BasicInventorySlot.at(canExtract, canInsert, validator, ..)` com o item no `canInsert` e no `validator`.
    pub fn with_checks(
        can_extract: Access,
        can_insert: Access,
        checks: SlotChecks,
        listener: Option<Callback>,
        on_decrease: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(SlotState {
                contents: SlotContents::default(),
                can_extract,
                can_insert,
                checks,
                listener,
                on_decrease,
            }),
        }
    }

    pub fn same_slot(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }

    /// Uma referência que não segura o slot, para o contêiner que pergunta por ele e que o slot também alcança (o tanque e o
    /// slot de entrada do Infuser): sem ela os dois se seguram para sempre.
    pub fn downgrade(&self) -> WeakInventorySlot {
        WeakInventorySlot {
            cell: Rc::downgrade(&self.cell),
        }
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
        let state = self.cell.borrow();
        state.capacity_for(state.contents.resource.as_ref())
    }

    pub fn is_full(&self) -> bool {
        let state = self.cell.borrow();
        state.contents.amount >= state.capacity_for(state.contents.resource.as_ref())
    }

    /// `isValid`.
    pub fn is_valid(&self, resource: &ItemResource) -> bool {
        self.cell
            .borrow()
            .checks
            .validator
            .as_ref()
            .is_none_or(|validator| validator(resource))
    }

    /// `isValidForInsertion`: o `isValid` do slot e o `canInsert` dele, com o tipo de automação de quem insere.
    pub fn is_valid_for_insertion(
        &self,
        resource: &ItemResource,
        automation: AutomationType,
    ) -> bool {
        let state = self.cell.borrow();
        let valid = state
            .checks
            .validator
            .as_ref()
            .is_none_or(|validator| validator(resource));
        let accepted = state
            .checks
            .can_insert
            .as_ref()
            .is_none_or(|accepts| accepts(resource))
            && state
                .checks
                .insert_by
                .as_ref()
                .is_none_or(|accepts| accepts(resource, automation));
        valid && accepted && state.can_insert.allows(automation)
    }

    /// `isCurrentValidForExtraction` para o item que o slot guarda: o `canExtract` do slot, com o tipo de automação de quem tira.
    pub fn is_valid_for_extraction(
        &self,
        resource: &ItemResource,
        automation: AutomationType,
    ) -> bool {
        let state = self.cell.borrow();
        state.can_extract.allows(automation)
            && state
                .checks
                .extract_by
                .as_ref()
                .is_none_or(|rule| rule(resource, automation))
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
            let stored = state.contents.amount;
            let needed = room_for(
                &state.contents,
                resource,
                state.capacity_for(Some(resource)),
            );
            // O `canInsert` é checado direto, e não pela capacidade, que já zera o que o `isValid` recusa.
            let item_accepted = state
                .checks
                .can_insert
                .as_ref()
                .is_none_or(|accepts| accepts(resource))
                && state
                    .checks
                    .insert_by
                    .as_ref()
                    .is_none_or(|accepts| accepts(resource, automation));
            if needed == 0 || !state.can_insert.allows(automation) || !item_accepted {
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
                || !state
                    .checks
                    .extract_by
                    .as_ref()
                    .is_none_or(|rule| rule(resource, automation))
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

/// [`InventorySlot::downgrade`].
#[derive(Clone)]
pub struct WeakInventorySlot {
    cell: std::rc::Weak<JournalCell<SlotState>>,
}

impl WeakInventorySlot {
    pub fn upgrade(&self) -> Option<InventorySlot> {
        self.cell.upgrade().map(|cell| InventorySlot { cell })
    }
}

/// `ResourceContainerType.getRedstoneSignalFromContainers`, em `f32` como o Java.
pub fn redstone_signal(slots: &[&InventorySlot]) -> i32 {
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
    lerp_discrete(proportion)
}

/// `ResourceContainerType.getRedstoneSignalFromContainer`: um contêiner só (o tanque de lava do Heat Generator). Vazio, ou sem capacidade, dá 0; o
/// que passa da capacidade conta como cheio.
pub fn redstone_signal_of_container(amount: u64, capacity: u64) -> i32 {
    if amount > 0 && capacity > 0 {
        lerp_discrete((amount as f32 / capacity as f32).min(1.0))
    } else {
        0
    }
}

/// `Mth.lerpDiscrete(proportion, SIGNAL_NONE, SIGNAL_MAX)`.
pub fn lerp_discrete(proportion: f32) -> i32 {
    const SIGNAL_MAX: i32 = 15;
    (proportion * (SIGNAL_MAX - 1) as f32).floor() as i32 + i32::from(proportion > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn clay() -> ItemResource {
        ItemResource::new("clay", 64)
    }

    fn named(name: &str) -> ItemResource {
        ItemResource::with_component(
            "clay",
            64,
            "minecraft:custom_name",
            &Components::text_value(name),
        )
    }

    #[test]
    fn a_resource_is_the_item_and_its_components() {
        assert_eq!(named("A"), named("A"));
        assert_ne!(named("A"), named("B"));
        assert_ne!(named("A"), clay());
        let damaged = ItemResource::with_component("clay", 64, "minecraft:damage", "5");
        assert_ne!(damaged, clay());
        assert_ne!(
            damaged,
            ItemResource::with_component("clay", 64, "minecraft:damage", "6")
        );
    }

    #[test]
    fn a_slot_only_stacks_and_gives_the_same_item_and_components() {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(named("A")), 3, None);
        let root = Transaction::open_root();
        for other in [named("B"), clay()] {
            assert_eq!(slot.insert(&other, 1, &root, AutomationType::Manual), 0);
            assert_eq!(slot.extract(&other, 1, &root, AutomationType::Manual), 0);
        }
        assert_eq!(
            slot.insert(&named("A"), 2, &root, AutomationType::Manual),
            2
        );
        assert_eq!(
            slot.extract(&named("A"), 4, &root, AutomationType::Manual),
            4
        );
        root.commit();
        assert_eq!(slot.contents(), SlotContents::new(Some(named("A")), 1));
    }

    #[test]
    fn room_for_is_nothing_for_another_resource_and_the_rest_for_the_same() {
        let held = SlotContents::new(Some(named("A")), 60);
        assert_eq!(room_for(&held, &named("B"), 64), 0);
        assert_eq!(room_for(&held, &clay(), 64), 0);
        assert_eq!(room_for(&held, &named("A"), 64), 4);
        assert_eq!(room_for(&held, &named("A"), 60), 0);
        assert_eq!(room_for(&SlotContents::default(), &named("B"), 64), 64);
        assert_eq!(named("A").slot_capacity(), 64);
        assert_eq!(ItemResource::new("x", 200).slot_capacity(), 99);
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
