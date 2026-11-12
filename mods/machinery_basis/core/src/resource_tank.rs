//! Porte de `BasicResourceContainer` (a base de `BasicChemicalTank` e `BasicFluidTank`), de `IResourceContainer` e do que
//! `ResourceHandlerSlot` faz com um item que tem capability de recurso (`fillContainerFromSlot`). O contêiner é um objeto
//! compartilhado, com diário: registra-se sozinho na transação que o muda e avisa quem escuta no commit da raiz.
//!
//! A lib não conhece nenhum recurso: o tipo `R` diz qual é (químico, fluido), e quem usa dá as regras como dados.
use crate::containers::{AutomationType, InventorySlot};
use crate::recipe_cache::{IngredientShape, InputIngredient, ShapedIngredient, TypedInstance};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction, fire};
use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;

/// O que um recurso precisa ter para um contêiner guardá-lo: o `Resource` do NeoForge, comparável e copiável.
pub trait Resource: Clone + PartialEq + fmt::Debug + TypedInstance + 'static {}

impl<T: Clone + PartialEq + fmt::Debug + TypedInstance + 'static> Resource for T {}

/// `LargeResourceStack<R>`: o que um contêiner guarda. Vazio é sem recurso e com zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackContents<R> {
    pub resource: Option<R>,
    pub amount: u64,
}

impl<R> Default for StackContents<R> {
    fn default() -> Self {
        Self {
            resource: None,
            amount: 0,
        }
    }
}

impl<R> StackContents<R> {
    /// `StackHelper.createStack`: sem recurso ou sem quantidade é o vazio.
    pub fn new(resource: Option<R>, amount: u64) -> Self {
        if amount == 0 || resource.is_none() {
            return Self::default();
        }
        Self { resource, amount }
    }

    pub fn is_empty(&self) -> bool {
        self.resource.is_none()
    }
}

pub(crate) fn saturated_cast(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `Ints.saturatedCast` de uma quantidade `long`.
pub(crate) fn saturated_amount(amount: u64) -> i32 {
    i32::try_from(amount).unwrap_or(i32::MAX)
}

/// `BiPredicate<R, AutomationType>`.
pub type Rule<R> = Rc<dyn Fn(&R, AutomationType) -> bool>;
/// `Predicate<R>`.
pub type Validator<R> = Rc<dyn Fn(&R) -> bool>;

struct TankState<R: Resource> {
    contents: StackContents<R>,
    capacity: u64,
    can_extract: Rule<R>,
    can_insert: Rule<R>,
    validator: Validator<R>,
    listener: Option<Callback>,
}

impl<R: Resource> Journaled for TankState<R> {
    type Snapshot = StackContents<R>;

    fn revert_to_snapshot(&mut self, snapshot: StackContents<R>) {
        // Sem avisar: o `revertToSnapshot` do Java ignora a checagem de mudança.
        self.contents = snapshot;
    }

    fn on_root_commit(&mut self, original: StackContents<R>) -> Vec<Callback> {
        if original == self.contents {
            return Vec::new();
        }
        self.listener.iter().cloned().collect()
    }
}

/// `BasicResourceContainer<R>`. Compartilhado: quem o guarda e a transação que o muda falam com o mesmo objeto.
pub struct ResourceTank<R: Resource> {
    cell: Rc<JournalCell<TankState<R>>>,
}

impl<R: Resource> Clone for ResourceTank<R> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
        }
    }
}

impl<R: Resource> fmt::Debug for ResourceTank<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ResourceTank({:?})", self.cell.borrow().contents)
    }
}

/// Uma referência fraca a um [`ResourceTank`].
pub struct WeakTank<R: Resource> {
    cell: std::rc::Weak<JournalCell<TankState<R>>>,
}

impl<R: Resource> WeakTank<R> {
    pub fn upgrade(&self) -> Option<ResourceTank<R>> {
        self.cell.upgrade().map(|cell| ResourceTank { cell })
    }
}

impl<R: Resource> ResourceTank<R> {
    /// `create(capacity, canExtract, canInsert, validator, listener)`.
    pub fn create(
        capacity: u64,
        can_extract: Rule<R>,
        can_insert: Rule<R>,
        validator: Validator<R>,
        listener: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(TankState {
                contents: StackContents::default(),
                capacity,
                can_extract,
                can_insert,
                validator,
                listener,
            }),
        }
    }

    /// Um contêiner que aceita tudo, de qualquer um (`create(capacity, listener)`).
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

    /// O valor novo da fonte de capacidade de um contêiner de capacidade variável (`VariableCapacityFluidTank`, que lê
    /// `capacity.getAsLong()` a cada consulta): quem o guarda chama isto sempre que o valor da fonte muda.
    pub fn set_capacity(&self, capacity: u64) {
        self.cell.borrow_mut().capacity = capacity;
    }

    /// Uma referência fraca ao contêiner, para um ouvinte do próprio contêiner (que o guardaria por inteiro e faria um ciclo).
    pub fn downgrade(&self) -> WeakTank<R> {
        WeakTank {
            cell: Rc::downgrade(&self.cell),
        }
    }

    /// `asStack`.
    pub fn contents(&self) -> StackContents<R> {
        self.cell.borrow().contents.clone()
    }

    /// `resource`: `None` é o `EMPTY`.
    pub fn resource(&self) -> Option<R> {
        self.cell.borrow().contents.resource.clone()
    }

    /// `amountAsLong`.
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

    /// `isValid`: o validador do contêiner.
    pub fn is_valid(&self, resource: &R) -> bool {
        let validator = self.cell.borrow().validator.clone();
        validator(resource)
    }

    /// `isValidForInsertion`.
    pub fn is_valid_for_insertion(&self, resource: &R, automation: AutomationType) -> bool {
        let can_insert = self.cell.borrow().can_insert.clone();
        self.is_valid(resource) && can_insert(resource, automation)
    }

    /// `capacityAsLong`: zero para o que o validador recusa. O recurso vazio (`None`) tem a capacidade inteira.
    pub fn capacity_for(&self, resource: Option<&R>) -> u64 {
        if resource.is_none_or(|resource| self.is_valid(resource)) {
            self.cell.borrow().capacity
        } else {
            0
        }
    }

    /// `capacityAsLong` de um recurso.
    pub fn capacity(&self, resource: &R) -> u64 {
        self.capacity_for(Some(resource))
    }

    /// `capacityAsInt`.
    pub fn capacity_as_int(&self, resource: &R) -> i32 {
        saturated_amount(self.capacity(resource))
    }

    /// `getNeededAsLong`: o que falta para encher. Com outro recurso guardado, nada cabe; com `None`, vale o guardado.
    pub fn needed(&self, resource: Option<&R>) -> u64 {
        let current = self.resource();
        let capacity = match resource {
            None => self.capacity_for(current.as_ref()),
            Some(resource) => {
                if current.as_ref().is_some_and(|held| held != resource) {
                    return 0;
                }
                self.capacity(resource)
            }
        };
        capacity.saturating_sub(self.amount())
    }

    /// `getNeededAsInt`.
    pub fn needed_as_int(&self, resource: Option<&R>) -> i32 {
        saturated_amount(self.needed(resource))
    }

    /// `isFull`.
    pub fn is_full(&self) -> bool {
        self.amount() >= self.capacity_for(self.resource().as_ref())
    }

    /// `setContents`: sem transação, o aviso dispara na hora, e só se o conteúdo mudou.
    pub fn set_contents(
        &self,
        resource: Option<R>,
        amount: u64,
        transaction: Option<&Transaction>,
    ) {
        let new = StackContents::new(resource, amount);
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
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 {
            return 0;
        }
        let stored = {
            let state = self.cell.borrow();
            // Outro tipo que o que o contêiner guarda não entra.
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
        resource: &R,
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
        // `isCurrentValidForExtraction`: com o contêiner vazio a conta já não passa pelo `matches`.
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

/// Um recurso que casa com o ingrediente de uma receita (`InputIngredient`). A lib não conhece ids: quem usa diz o que casa, e
/// como o `IInputCache` indexa o ingrediente (a [`IngredientShape`]).
pub struct Ingredient<R> {
    shaped: ShapedIngredient,
    resource: PhantomData<fn(&R)>,
}

impl<R> Clone for Ingredient<R> {
    fn clone(&self) -> Self {
        Self {
            shaped: self.shaped.clone(),
            resource: PhantomData,
        }
    }
}

impl<R> Ingredient<R> {
    /// Um ingrediente com a classificação que o cache de receitas lê e o teste de tipo do jogo.
    pub fn shaped(shaped: ShapedIngredient) -> Self {
        Self {
            shaped,
            resource: PhantomData,
        }
    }

    pub fn as_shaped(&self) -> &ShapedIngredient {
        &self.shaped
    }
}

impl<R: Resource> Ingredient<R> {
    pub fn matches(&self, resource: &R) -> bool {
        self.shaped.test_type(resource)
    }
}

impl<R> InputIngredient for Ingredient<R> {
    fn test_type(&self, input: &dyn TypedInstance) -> bool {
        self.shaped.test_type(input)
    }

    fn shape(&self) -> &IngredientShape {
        self.shaped.shape()
    }
}

impl<R> fmt::Debug for Ingredient<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ingredient")
    }
}

/// `ResourceHandler<R>` do NeoForge, o que um item expõe pela capability: uma lista de recursos que ele guarda, e a troca
/// deles dentro de uma transação. Quem usa liga isto ao item que está no slot: o que o `extract` tira tem de mudar o item do
/// slot na mesma transação (o `ItemAccess` do NeoForge), para que desfazer a transação o devolva.
pub trait ResourceHandler<R> {
    /// `size`.
    fn size(&self) -> usize;
    /// `getResource`: `None` é o vazio.
    fn resource(&self, index: usize) -> Option<R>;
    /// `extract`.
    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32;
}

/// `ResourceHandlerSlot.fillContainerFromSlot(container, handler, resource, amountNeeded, transaction)`.
fn fill_with_resource<R: Resource>(
    container: &ResourceTank<R>,
    handler: &dyn ResourceHandler<R>,
    resource: &R,
    amount_needed: i32,
    transaction: &Transaction,
) -> bool {
    if amount_needed == 0 {
        return false;
    }
    let room_for = {
        // Confere quanto o contêiner aceita, caso tenha limite de taxa e não receba tudo de uma vez.
        let simulation = Transaction::open(Some(transaction));
        let room_for = container.insert(
            resource,
            amount_needed,
            &simulation,
            AutomationType::Internal,
        );
        if room_for == 0 {
            return false;
        }
        room_for
    };
    let sub_transaction = Transaction::open(Some(transaction));
    // Tira do item o que a simulação disse que cabe, antes de pôr no contêiner: um item empilhado pode só dar em passos.
    let extracted = handler.extract(resource, room_for, &sub_transaction);
    if extracted > 0
        && container.insert(
            resource,
            extracted,
            &sub_transaction,
            AutomationType::Internal,
        ) == extracted
    {
        sub_transaction.commit();
        return true;
    }
    false
}

/// `ResourceHandlerSlot.fillContainerFromSlot(container, handler, transaction)`.
fn fill_from_handler<R: Resource + Eq + Hash>(
    container: &ResourceTank<R>,
    handler: &dyn ResourceHandler<R>,
    transaction: &Transaction,
) -> bool {
    if !container.is_empty() {
        let resource = container.resource();
        let amount_needed = container.needed_as_int(resource.as_ref());
        return match resource {
            Some(resource) if amount_needed > 0 => {
                fill_with_resource(container, handler, &resource, amount_needed, transaction)
            }
            _ => false,
        };
    }
    // Junta tudo o que o item guarda e o contêiner aceita.
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
                container.capacity_as_int(&resource),
                transaction,
            )
        {
            // Com um contêiner de um recurso só, o que entrou impede que outro tipo entre.
            return true;
        }
    }
    false
}

/// `ResourceHandlerSlot.fillContainerFromSlot(container, containerType, transaction)`, o que `fillTankFromSlot` chama.
///
/// `handler` é a capability do item que está no slot (`containerType.capability().getCapability(asItemAccess())`), ou `None`
/// se o item não tem. Devolve se encheu o contêiner.
///
/// Não porta o `setLastTransferDirection(FILL_FROM_ITEM)` que o Java grava no slot dentro da subtransação: só o
/// `handleContainer` lê esse valor, e nenhum slot portado o chama.
pub fn fill_container_from_slot<R: Resource + Eq + Hash>(
    slot: &InventorySlot,
    handler: Option<&dyn ResourceHandler<R>>,
    container: &ResourceTank<R>,
    transaction: Option<&Transaction>,
) -> bool {
    if slot.is_empty() {
        return false;
    }
    let Some(handler) = handler else {
        return false;
    };
    let sub_transaction = Transaction::open(transaction);
    if fill_from_handler(container, handler, &sub_transaction) {
        sub_transaction.commit();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct Kind(&'static str);

    impl TypedInstance for Kind {
        fn type_id(&self) -> &str {
            self.0
        }

        fn is_empty(&self) -> bool {
            false
        }
    }

    fn counting() -> (Callback, Rc<Cell<u32>>) {
        let count = Rc::new(Cell::new(0));
        let inner = count.clone();
        (Rc::new(move || inner.set(inner.get() + 1)), count)
    }

    #[test]
    fn what_is_needed_follows_the_stored_resource() {
        let tank = ResourceTank::<Kind>::open(100, None);
        assert_eq!(tank.needed(None), 100);
        assert_eq!(tank.needed(Some(&Kind("a"))), 100);
        tank.set_contents(Some(Kind("a")), 30, None);
        assert_eq!(tank.needed(None), 70);
        assert_eq!(tank.needed(Some(&Kind("a"))), 70);
        assert_eq!(tank.needed(Some(&Kind("b"))), 0);
        assert!(!tank.is_full());
        tank.set_contents(Some(Kind("a")), 100, None);
        assert!(tank.is_full());
    }

    #[test]
    fn a_refused_resource_has_no_capacity_and_the_empty_one_has_all() {
        let tank = ResourceTank::<Kind>::create(
            100,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(|kind| kind.0 == "a"),
            None,
        );
        assert_eq!(tank.capacity_for(None), 100);
        assert_eq!(tank.capacity(&Kind("b")), 0);
        assert_eq!(tank.needed_as_int(Some(&Kind("b"))), 0);
        assert!(!tank.is_valid_for_insertion(&Kind("b"), AutomationType::Internal));
    }

    /// Um item com tanques, para o que o Java chama de `ResourceHandler`.
    struct Bottle {
        stored: RefCell<Vec<Option<(Kind, i32)>>>,
        journal: Rc<JournalCell<BottleState>>,
    }

    struct BottleState {
        taken: i32,
    }

    impl Journaled for BottleState {
        type Snapshot = i32;

        fn revert_to_snapshot(&mut self, snapshot: i32) {
            self.taken = snapshot;
        }

        fn on_root_commit(&mut self, _original: i32) -> Vec<Callback> {
            Vec::new()
        }
    }

    impl Bottle {
        fn new(tanks: Vec<Option<(Kind, i32)>>) -> Self {
            Self {
                stored: RefCell::new(tanks),
                journal: JournalCell::new(BottleState { taken: 0 }),
            }
        }

        fn taken(&self) -> i32 {
            self.journal.borrow().taken
        }
    }

    impl ResourceHandler<Kind> for Bottle {
        fn size(&self) -> usize {
            self.stored.borrow().len()
        }

        fn resource(&self, index: usize) -> Option<Kind> {
            self.stored.borrow()[index]
                .as_ref()
                .map(|(kind, _)| kind.clone())
        }

        fn extract(&self, resource: &Kind, amount: i32, transaction: &Transaction) -> i32 {
            let stored = self.stored.borrow();
            let available: i32 = stored
                .iter()
                .flatten()
                .filter(|(kind, _)| kind == resource)
                .map(|(_, held)| *held)
                .sum();
            let to_take = amount.min(available - self.taken());
            if to_take > 0 {
                let mut state = self.journal.borrow_mut();
                self.journal.update_snapshots(transaction, || state.taken);
                state.taken += to_take;
            }
            to_take.max(0)
        }
    }

    fn slot_with_item() -> InventorySlot {
        let slot = InventorySlot::new(None);
        slot.set_contents(
            Some(crate::containers::ItemResource::new("bottle", 1)),
            1,
            None,
        );
        slot
    }

    #[test]
    fn an_empty_container_takes_the_first_resource_of_the_item_that_it_accepts() {
        let slot = slot_with_item();
        let bottle = Bottle::new(vec![None, Some((Kind("b"), 40)), Some((Kind("a"), 25))]);
        let tank = ResourceTank::<Kind>::create(
            100,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(|kind| kind.0 == "a"),
            None,
        );
        assert!(fill_container_from_slot(&slot, Some(&bottle), &tank, None));
        assert_eq!(tank.contents(), StackContents::new(Some(Kind("a")), 25));
        assert_eq!(bottle.taken(), 25);
    }

    #[test]
    fn a_container_with_something_takes_only_the_same_resource_up_to_what_it_needs() {
        let slot = slot_with_item();
        let bottle = Bottle::new(vec![Some((Kind("b"), 40)), Some((Kind("a"), 90))]);
        let tank = ResourceTank::<Kind>::open(100, None);
        tank.set_contents(Some(Kind("a")), 60, None);
        assert!(fill_container_from_slot(&slot, Some(&bottle), &tank, None));
        assert_eq!(tank.amount(), 100);
        assert_eq!(bottle.taken(), 40);
    }

    #[test]
    fn a_full_container_or_a_missing_handler_or_an_empty_slot_fills_nothing() {
        let slot = slot_with_item();
        let bottle = Bottle::new(vec![Some((Kind("a"), 90))]);
        let full = ResourceTank::<Kind>::open(100, None);
        full.set_contents(Some(Kind("a")), 100, None);
        assert!(!fill_container_from_slot(&slot, Some(&bottle), &full, None));
        let tank = ResourceTank::<Kind>::open(100, None);
        assert!(!fill_container_from_slot(&slot, None, &tank, None));
        let empty = InventorySlot::new(None);
        assert!(!fill_container_from_slot(
            &empty,
            Some(&bottle),
            &tank,
            None
        ));
        assert_eq!((tank.amount(), bottle.taken()), (0, 0));
    }

    #[test]
    fn a_failed_fill_leaves_both_sides_as_they_were() {
        let slot = slot_with_item();
        let bottle = Bottle::new(vec![Some((Kind("a"), 90))]);
        // O contêiner recusa a inserção de verdade, depois de a simulação aceitar a primeira vez.
        let calls = Rc::new(Cell::new(0));
        let counter = calls.clone();
        let tank = ResourceTank::<Kind>::create(
            100,
            Rc::new(|_, _| true),
            Rc::new(move |_, _| {
                counter.set(counter.get() + 1);
                counter.get() <= 2
            }),
            Rc::new(|_| true),
            None,
        );
        assert!(!fill_container_from_slot(&slot, Some(&bottle), &tank, None));
        assert_eq!((tank.amount(), bottle.taken()), (0, 0));
    }

    #[test]
    fn a_filled_container_says_it_once_at_the_root() {
        let (listener, count) = counting();
        let slot = slot_with_item();
        let bottle = Bottle::new(vec![Some((Kind("a"), 10))]);
        let tank = ResourceTank::<Kind>::open(100, Some(listener));
        assert!(fill_container_from_slot(&slot, Some(&bottle), &tank, None));
        assert_eq!(count.get(), 1);
    }
}
