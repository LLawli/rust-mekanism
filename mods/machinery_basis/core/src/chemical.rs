//! Porte de `BasicChemicalTank` (sobre `BasicResourceContainer`), do handler de entrada de químico e de
//! `ChemicalInventorySlot.fillTankOrConvert`. Como os outros contêineres da base, o tanque é um objeto compartilhado, com
//! diário: registra-se sozinho na transação que o muda e avisa quem escuta no commit da raiz.
//!
//! A lib não conhece nenhum químico: quem usa dá o id, as regras de inserção e as conversões como dados.
use crate::cached_recipe::ItemIngredient;
use crate::containers::{Access, AutomationType, InventorySlot};
use crate::operation_tracker::{OperationTracker, RecipeError};
use crate::recipe_handlers::{RecipeStack, calculate_operations_can_support};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction, fire};
use std::fmt;
use std::rc::Rc;

/// Um químico, pelo id. Vazio é a ausência dele (`ChemicalResource.EMPTY`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemicalResource {
    pub id: Rc<str>,
}

impl ChemicalResource {
    pub fn new(id: &str) -> Self {
        Self { id: Rc::from(id) }
    }
}

/// `LargeResourceStack<ChemicalResource>`: o que um tanque guarda. Vazio é sem recurso e com zero.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TankContents {
    pub resource: Option<ChemicalResource>,
    pub amount: u64,
}

impl TankContents {
    /// `StackHelper.createStack`: sem recurso ou sem quantidade é o vazio.
    pub fn new(resource: Option<ChemicalResource>, amount: u64) -> Self {
        if amount == 0 || resource.is_none() {
            return Self::default();
        }
        Self { resource, amount }
    }

    pub fn is_empty(&self) -> bool {
        self.resource.is_none()
    }
}

fn saturated_cast(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `Ints.saturatedCast` de uma quantidade `long`.
fn saturated_amount(amount: u64) -> i32 {
    i32::try_from(amount).unwrap_or(i32::MAX)
}

/// `BiPredicate<ChemicalResource, AutomationType>`.
pub type ChemicalRule = Rc<dyn Fn(&ChemicalResource, AutomationType) -> bool>;
/// `Predicate<ChemicalResource>`.
pub type ChemicalValidator = Rc<dyn Fn(&ChemicalResource) -> bool>;

struct TankState {
    contents: TankContents,
    capacity: u64,
    can_extract: ChemicalRule,
    can_insert: ChemicalRule,
    validator: ChemicalValidator,
    listener: Option<Callback>,
}

impl Journaled for TankState {
    type Snapshot = TankContents;

    fn revert_to_snapshot(&mut self, snapshot: TankContents) {
        // Sem avisar: o `revertToSnapshot` do Java ignora a checagem de mudança.
        self.contents = snapshot;
    }

    fn on_root_commit(&mut self, original: TankContents) -> Vec<Callback> {
        if original == self.contents {
            return Vec::new();
        }
        self.listener.iter().cloned().collect()
    }
}

/// `BasicChemicalTank`. Compartilhado: quem o guarda e a transação que o muda falam com o mesmo objeto.
///
/// O validador de atributos (`ChemicalAttributeValidator`) não existe aqui: ele recusa químicos com atributos que pedem
/// validação (radiação, por exemplo), e nenhum químico dos tanques portados tem atributo.
#[derive(Clone)]
pub struct ChemicalTank {
    cell: Rc<JournalCell<TankState>>,
}

impl fmt::Debug for ChemicalTank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChemicalTank({:?})", self.cell.borrow().contents)
    }
}

impl ChemicalTank {
    /// `BasicChemicalTank.create(capacity, canExtract, canInsert, validator, listener)`.
    pub fn create(
        capacity: u64,
        can_extract: ChemicalRule,
        can_insert: ChemicalRule,
        validator: ChemicalValidator,
        listener: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(TankState {
                contents: TankContents::default(),
                capacity,
                can_extract,
                can_insert,
                validator,
                listener,
            }),
        }
    }

    /// Um tanque que aceita tudo, de qualquer um (`BasicChemicalTank.create(capacity, listener)`).
    pub fn open(capacity: u64, listener: Option<Callback>) -> Self {
        Self::create(
            capacity,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(|_| true),
            listener,
        )
    }

    pub fn same_tank(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }

    pub fn contents(&self) -> TankContents {
        self.cell.borrow().contents.clone()
    }

    pub fn resource(&self) -> Option<ChemicalResource> {
        self.cell.borrow().contents.resource.clone()
    }

    pub fn amount(&self) -> u64 {
        self.cell.borrow().contents.amount
    }

    /// `amountAsInt`.
    pub fn amount_as_int(&self) -> i32 {
        saturated_amount(self.amount())
    }

    pub fn is_empty(&self) -> bool {
        self.cell.borrow().contents.is_empty()
    }

    /// `isValid`: o validador do tanque.
    pub fn is_valid(&self, resource: &ChemicalResource) -> bool {
        let validator = self.cell.borrow().validator.clone();
        validator(resource)
    }

    /// `isValidForInsertion`.
    pub fn is_valid_for_insertion(
        &self,
        resource: &ChemicalResource,
        automation: AutomationType,
    ) -> bool {
        let can_insert = self.cell.borrow().can_insert.clone();
        self.is_valid(resource) && can_insert(resource, automation)
    }

    /// `capacityAsLong`: zero para o que o validador recusa.
    pub fn capacity(&self, resource: &ChemicalResource) -> u64 {
        if self.is_valid(resource) {
            self.cell.borrow().capacity
        } else {
            0
        }
    }

    /// `setContents`: sem transação, o aviso dispara na hora, e só se o conteúdo mudou.
    pub fn set_contents(
        &self,
        resource: Option<ChemicalResource>,
        amount: u64,
        transaction: Option<&Transaction>,
    ) {
        let new = TankContents::new(resource, amount);
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
                    state.contents = new;
                    state.listener.iter().cloned().collect()
                }
            }
        };
        fire(callbacks);
    }

    /// `insert`.
    pub fn insert(
        &self,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 {
            return 0;
        }
        let stored = {
            let state = self.cell.borrow();
            // Outro tipo que o que o tanque guarda não entra.
            if state
                .contents
                .resource
                .as_ref()
                .is_some_and(|held| held != resource)
            {
                return 0;
            }
            state.contents.amount
        };
        let needed = saturated_cast(self.capacity(resource) as i64 - stored as i64);
        let can_insert = self.cell.borrow().can_insert.clone();
        if needed <= 0 || !can_insert(resource, automation) {
            return 0;
        }
        let to_add = amount.min(needed);
        self.set_contents(
            Some(resource.clone()),
            stored + to_add as u64,
            Some(transaction),
        );
        to_add
    }

    /// `extract`.
    pub fn extract(
        &self,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        let (stored, can_extract, current) = {
            let state = self.cell.borrow();
            (
                state.contents.amount,
                state.can_extract.clone(),
                state.contents.resource.clone(),
            )
        };
        // `isCurrentValidForExtraction`: com tanque vazio a conta já não passa pelo `matches`.
        if amount == 0 || current.as_ref() != Some(resource) || !can_extract(resource, automation) {
            return 0;
        }
        let to_remove = amount.min(saturated_amount(stored));
        if to_remove > 0 {
            self.set_contents(
                Some(resource.clone()),
                stored - to_remove as u64,
                Some(transaction),
            );
        }
        to_remove
    }
}

/// Os predicados de `AutomationType` que o Mekanism passa aos tanques, para quem quer o padrão sem escrever a closure.
pub fn access_rule(access: Access) -> ChemicalRule {
    Rc::new(move |_, automation| access.allows(automation))
}

/// `InputHelper.getInputHandler(IChemicalTank, RecipeError)`.
#[derive(Clone)]
pub struct ChemicalInputHandler {
    tank: ChemicalTank,
    not_enough_error: RecipeError,
}

/// Um químico que casa com o ingrediente de uma receita.
#[derive(Clone)]
pub struct ChemicalIngredient {
    test: Rc<dyn Fn(&ChemicalResource) -> bool>,
}

impl ChemicalIngredient {
    /// Um químico só, pelo id.
    pub fn chemical(id: &str) -> Self {
        let id: Rc<str> = Rc::from(id);
        Self::matching(move |resource| resource.id == id)
    }

    /// Qualquer químico que o predicado aceite, como uma tag.
    pub fn matching(test: impl Fn(&ChemicalResource) -> bool + 'static) -> Self {
        Self {
            test: Rc::new(test),
        }
    }

    pub fn matches(&self, resource: &ChemicalResource) -> bool {
        (self.test)(resource)
    }
}

impl fmt::Debug for ChemicalIngredient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChemicalIngredient")
    }
}

impl ChemicalInputHandler {
    pub fn new(tank: ChemicalTank, not_enough_error: RecipeError) -> Self {
        Self {
            tank,
            not_enough_error,
        }
    }

    /// `getInput`.
    pub fn input(&self) -> TankContents {
        self.tank.contents()
    }

    /// `getRecipeInput`: o que o ingrediente aceita do tanque, com a quantidade que a receita pede. Tanque vazio, outro
    /// químico ou menos do que ela pede não casam.
    pub fn recipe_input(
        &self,
        ingredient: &ChemicalIngredient,
        amount: u32,
    ) -> Option<RecipeStack<ChemicalResource>> {
        let contents = self.tank.contents();
        let resource = contents.resource.filter(|resource| {
            contents.amount >= u64::from(amount) && ingredient.matches(resource)
        })?;
        Some(RecipeStack { resource, amount })
    }

    /// `use`.
    pub fn use_input(
        &self,
        input: Option<&RecipeStack<ChemicalResource>>,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        let Some(input) = input else {
            return false;
        };
        if operations == 0 {
            return true;
        }
        let amount = input.amount as i32 * operations;
        self.tank.extract(
            &input.resource,
            amount,
            transaction,
            AutomationType::Internal,
        ) == amount
    }

    /// `calculateOperationsCanSupport(tracker, recipeInput, isConstantUsage, usageMultiplier)`.
    pub fn calculate_operations_can_support(
        &self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
        input: &RecipeStack<ChemicalResource>,
        is_constant_usage: bool,
        usage_multiplier: i32,
    ) {
        let tank = &self.tank;
        calculate_operations_can_support(
            tracker,
            transaction,
            self.not_enough_error,
            input.amount,
            is_constant_usage,
            usage_multiplier,
            |simulation| {
                tank.extract(
                    &input.resource,
                    tank.amount_as_int(),
                    simulation,
                    AutomationType::Internal,
                )
            },
        );
    }
}

/// Uma receita de `chemical_conversion` (`ItemStackToChemicalRecipe`): um item que vira químico.
#[derive(Clone, Debug)]
pub struct ChemicalConversion {
    pub input: ItemIngredient,
    pub input_count: u32,
    pub output: ChemicalResource,
    pub amount: i32,
}

/// `ChemicalInventorySlot.fillTankOrConvert`: o item do slot vira químico no tanque, se o tanque aceita tudo o que a
/// conversão dá.
///
/// Falta a primeira metade do Java, `fillTankFromSlot`, que enche o tanque de um item com a capability de químico (um
/// tanque portátil): nenhum item do jogo tem essa capability, então ela nunca acha o que fazer e cai na conversão.
pub fn fill_tank_or_convert(
    slot: &InventorySlot,
    tank: &ChemicalTank,
    conversions: &[ChemicalConversion],
    transaction: Option<&Transaction>,
) {
    let contents = slot.contents();
    let Some(resource) = contents.resource else {
        return;
    };
    // `findFirstRecipe`: a primeira conversão que paga o que o slot tem, quantidade incluída. O `getMatchingInstance` que
    // vem depois só devolve vazio onde o `test` já teria falhado.
    let Some(conversion) = conversions.iter().find(|conversion| {
        conversion.input.matches(&resource) && contents.amount >= conversion.input_count
    }) else {
        return;
    };
    let sub_transaction = Transaction::open(transaction);
    let needed = conversion.input_count;
    let produced = conversion.amount;
    // Tira do slot o que a receita pede e põe o que ela dá no tanque. `Manual` passa por cima do limite de taxa de quem
    // insere, que aqui não existe, mas o `canInsert` do tanque ainda vale.
    if slot.extract(
        &resource,
        needed,
        &sub_transaction,
        AutomationType::Internal,
    ) == needed
        && tank.insert(
            &conversion.output,
            produced,
            &sub_transaction,
            AutomationType::Manual,
        ) == produced
    {
        sub_transaction.commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::ItemResource;
    use std::cell::Cell;

    fn redstone() -> ChemicalResource {
        ChemicalResource::new("redstone")
    }

    fn counting() -> (Callback, Rc<Cell<u32>>) {
        let count = Rc::new(Cell::new(0));
        let inner = count.clone();
        (Rc::new(move || inner.set(inner.get() + 1)), count)
    }

    #[test]
    fn a_tank_holds_one_chemical_up_to_its_capacity() {
        let tank = ChemicalTank::open(1_000, None);
        let root = Transaction::open_root();
        assert_eq!(
            tank.insert(&redstone(), 600, &root, AutomationType::Internal),
            600
        );
        assert_eq!(
            tank.insert(&redstone(), 600, &root, AutomationType::Internal),
            400
        );
        let other = ChemicalResource::new("carbon");
        assert_eq!(tank.insert(&other, 1, &root, AutomationType::Internal), 0);
    }

    #[test]
    fn a_rolled_back_change_says_nothing() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        let root = Transaction::open_root();
        tank.insert(&redstone(), 5, &root, AutomationType::Internal);
        drop(root);
        assert!(tank.is_empty());
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn a_commit_says_it_once_at_the_root() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        let root = Transaction::open_root();
        let nested = Transaction::open(Some(&root));
        tank.insert(&redstone(), 5, &nested, AutomationType::Internal);
        nested.commit();
        assert_eq!(count.get(), 0);
        root.commit();
        assert_eq!(count.get(), 1);
        assert_eq!(tank.amount(), 5);
    }

    #[test]
    fn setting_outside_a_transaction_says_it_at_once() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        tank.set_contents(Some(redstone()), 15, None);
        assert_eq!(count.get(), 1);
        tank.set_contents(Some(redstone()), 15, None);
        assert_eq!(count.get(), 1, "o mesmo conteúdo não avisa");
        tank.set_contents(None, 0, None);
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn what_the_validator_refuses_has_no_room() {
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::Any),
            access_rule(Access::Any),
            Rc::new(|resource| &*resource.id == "redstone"),
            None,
        );
        let root = Transaction::open_root();
        let other = ChemicalResource::new("carbon");
        assert_eq!(tank.capacity(&other), 0);
        assert_eq!(tank.insert(&other, 5, &root, AutomationType::Internal), 0);
        assert_eq!(
            tank.insert(&redstone(), 5, &root, AutomationType::Internal),
            5
        );
    }

    #[test]
    fn extraction_follows_the_rule_for_who_asks() {
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::NotExternal),
            access_rule(Access::Any),
            Rc::new(|_| true),
            None,
        );
        tank.set_contents(Some(redstone()), 50, None);
        let root = Transaction::open_root();
        assert_eq!(
            tank.extract(&redstone(), 10, &root, AutomationType::External),
            0
        );
        assert_eq!(
            tank.extract(&redstone(), 10, &root, AutomationType::Internal),
            10
        );
        assert_eq!(
            tank.extract(&redstone(), 100, &root, AutomationType::Manual),
            40
        );
        assert!(tank.is_empty());
    }

    fn dust_to_redstone() -> ChemicalConversion {
        ChemicalConversion {
            input: ItemIngredient::item("redstone"),
            input_count: 1,
            output: redstone(),
            amount: 10,
        }
    }

    fn dust_slot(amount: u32) -> InventorySlot {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(ItemResource::new("redstone", 64)), amount, None);
        slot
    }

    #[test]
    fn an_item_becomes_the_chemical_one_at_a_time() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::open(1_000, None);
        fill_tank_or_convert(&slot, &tank, &[dust_to_redstone()], None);
        assert_eq!((slot.amount(), tank.amount()), (2, 10));
        fill_tank_or_convert(&slot, &tank, &[dust_to_redstone()], None);
        assert_eq!((slot.amount(), tank.amount()), (1, 20));
    }

    #[test]
    fn a_conversion_that_does_not_fit_in_the_tank_keeps_the_item() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::open(15, None);
        tank.set_contents(Some(redstone()), 10, None);
        fill_tank_or_convert(&slot, &tank, &[dust_to_redstone()], None);
        assert_eq!((slot.amount(), tank.amount()), (3, 10));
    }

    #[test]
    fn a_conversion_the_tank_refuses_keeps_the_item() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::Any),
            Rc::new(|_, _| false),
            Rc::new(|_| true),
            None,
        );
        fill_tank_or_convert(&slot, &tank, &[dust_to_redstone()], None);
        assert_eq!((slot.amount(), tank.amount()), (3, 0));
    }
}
