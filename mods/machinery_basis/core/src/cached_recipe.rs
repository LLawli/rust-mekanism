//! Porte de `CachedRecipe` e `OneInputCachedRecipe`. Como no Java, a receita guarda os callbacks do tile (`setActive`,
//! `setEnergyRequirements`, `setOnFinish`...) como closures, e a parte abstrata é a [`RecipeLogic`].
use crate::containers::{
    AutomationType, EnergyContainer, InventorySlot, ItemResource, SlotContents,
};
use crate::operation_tracker::{OperationTracker, RecipeError, RecipeErrors};
use crate::recipe_handlers::{ItemInputHandler, ItemOutputHandler, RecipeStack};
use crate::recipe_monitor::RecipeId;
use crate::transaction::Transaction;
use std::fmt;
use std::rc::Rc;

/// O que muda de uma receita para outra: o que o Java deixa abstrato em `CachedRecipe`.
pub trait RecipeLogic {
    /// `setupVariableValues`.
    fn setup_variable_values(&mut self) {}
    /// `calculateOperationsThisTick`, depois do cálculo da energia, que é comum a todas.
    fn calculate_operations_this_tick(
        &mut self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
    );
    fn is_input_valid(&self) -> bool;
    fn finish_processing(&mut self, operations: i32, transaction: &Transaction) -> bool;
    fn use_resources(&mut self, _operations: i32, _transaction: &Transaction) -> bool {
        true
    }
    fn reset_cache(&mut self) {}
}

/// `CachedRecipe`.
pub struct CachedRecipe {
    logic: Box<dyn RecipeLogic>,
    errors: RecipeErrors,
    paused_for_errors: bool,
    recheck_all_errors: Box<dyn Fn() -> bool>,
    can_holder_function: Box<dyn Fn() -> bool>,
    set_active: Box<dyn Fn(bool)>,
    required_ticks: Box<dyn Fn() -> i32>,
    on_finish: Box<dyn Fn(i32)>,
    per_tick_energy: Box<dyn Fn() -> i32>,
    energy_container: Option<EnergyContainer>,
    baseline_max_operations: Box<dyn Fn() -> i32>,
    post_process_operations: Box<dyn Fn(&mut OperationTracker)>,
    on_errors_change: Box<dyn Fn(RecipeErrors)>,
    operating_ticks: i32,
    operating_ticks_changed: Box<dyn Fn(i32)>,
    multiple_operations_cost: bool,
}

impl fmt::Debug for CachedRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedRecipe")
            .field("errors", &self.errors)
            .field("paused_for_errors", &self.paused_for_errors)
            .field("operating_ticks", &self.operating_ticks)
            .finish_non_exhaustive()
    }
}

impl CachedRecipe {
    /// Os padrões são os do `CachedRecipe`.
    pub fn new(logic: Box<dyn RecipeLogic>, recheck_all_errors: Box<dyn Fn() -> bool>) -> Self {
        Self {
            logic,
            errors: RecipeErrors::EMPTY,
            paused_for_errors: false,
            recheck_all_errors,
            can_holder_function: Box::new(|| true),
            set_active: Box::new(|_| {}),
            required_ticks: Box::new(|| 1),
            on_finish: Box::new(|_| {}),
            per_tick_energy: Box::new(|| 0),
            energy_container: None,
            baseline_max_operations: Box::new(|| 1),
            post_process_operations: Box::new(|_| {}),
            on_errors_change: Box::new(|_| {}),
            operating_ticks: 0,
            operating_ticks_changed: Box::new(|_| {}),
            multiple_operations_cost: false,
        }
    }

    pub fn set_can_holder_function(mut self, f: impl Fn() -> bool + 'static) -> Self {
        self.can_holder_function = Box::new(f);
        self
    }

    pub fn set_active(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.set_active = Box::new(f);
        self
    }

    /// `setEnergyRequirements`: o consumo por tick e o contêiner de onde sai a energia.
    pub fn set_energy_requirements(
        mut self,
        per_tick_energy: impl Fn() -> i32 + 'static,
        energy_container: EnergyContainer,
    ) -> Self {
        self.per_tick_energy = Box::new(per_tick_energy);
        self.energy_container = Some(energy_container);
        self
    }

    pub fn set_required_ticks(mut self, f: impl Fn() -> i32 + 'static) -> Self {
        self.required_ticks = Box::new(f);
        self
    }

    pub fn set_operating_ticks_changed(mut self, f: impl Fn(i32) + 'static) -> Self {
        self.operating_ticks_changed = Box::new(f);
        self
    }

    pub fn set_on_finish(mut self, f: impl Fn(i32) + 'static) -> Self {
        self.on_finish = Box::new(f);
        self
    }

    pub fn set_baseline_max_operations(mut self, f: impl Fn() -> i32 + 'static) -> Self {
        self.baseline_max_operations = Box::new(f);
        self
    }

    pub fn set_post_process_operations(
        mut self,
        f: impl Fn(&mut OperationTracker) + 'static,
    ) -> Self {
        self.post_process_operations = Box::new(f);
        self
    }

    pub fn set_operations_cost(mut self, value: bool) -> Self {
        self.multiple_operations_cost = value;
        self
    }

    pub fn set_errors_changed(mut self, f: impl Fn(RecipeErrors) + 'static) -> Self {
        self.on_errors_change = Box::new(f);
        self
    }

    pub fn operating_ticks(&self) -> i32 {
        self.operating_ticks
    }

    pub fn paused_for_errors(&self) -> bool {
        self.paused_for_errors
    }

    pub fn errors(&self) -> RecipeErrors {
        self.errors
    }

    pub fn unpause_errors(&mut self) {
        self.paused_for_errors = false;
    }

    pub fn load_saved_operating_ticks(&mut self, operating_ticks: i32) {
        if operating_ticks > 0 && operating_ticks < (self.required_ticks)() {
            self.operating_ticks = operating_ticks;
        }
    }

    pub fn is_input_valid(&self) -> bool {
        self.logic.is_input_valid()
    }

    fn update_errors(&mut self, errors: RecipeErrors) {
        if self.errors != errors {
            self.errors = errors;
            self.paused_for_errors = self.errors.len() > 1
                || !self
                    .errors
                    .contains(RecipeError::NOT_ENOUGH_ENERGY_REDUCED_RATE);
            (self.on_errors_change)(errors);
        }
    }

    /// `energyUsage.useEnergy`: tira do contêiner como quem é interno.
    fn use_energy_from_container(&self, energy: i32, transaction: &Transaction) -> i32 {
        self.energy_container.as_ref().map_or(0, |container| {
            container.extract(energy, transaction, AutomationType::Internal)
        })
    }

    fn use_energy(&self, operations: i32, transaction: &Transaction) -> bool {
        let mut energy = (self.per_tick_energy)();
        if self.multiple_operations_cost {
            energy *= operations;
        }
        self.use_energy_from_container(energy, transaction) == energy
    }

    fn calculate_energy(&self, tracker: &mut OperationTracker, transaction: &Transaction) {
        if tracker.should_continue_checking() {
            let energy_per_tick = (self.per_tick_energy)();
            if energy_per_tick > 0 {
                let stored = self
                    .energy_container
                    .as_ref()
                    .map_or(0, EnergyContainer::amount_as_int);
                let operations = {
                    let simulation = Transaction::open(Some(transaction));
                    self.use_energy_from_container(stored, &simulation) / energy_per_tick
                };
                tracker.max_for_energy = operations;
                if operations == 0 {
                    tracker.update_operations(operations);
                    tracker.add_error(RecipeError::NOT_ENOUGH_ENERGY);
                }
            }
        }
    }

    /// `process`. Cada saída fecha a raiz: com commit se a operação andou, e sem, a transação volta tudo ao sair.
    pub fn process(&mut self) {
        if self.paused_for_errors {
            (self.set_active)(false);
            return;
        }
        let transaction = Transaction::open_root();
        let operations = if (self.can_holder_function)() {
            self.logic.setup_variable_values();
            let mut tracker = OperationTracker::new(
                self.errors,
                (self.recheck_all_errors)(),
                (self.baseline_max_operations)(),
            );
            self.calculate_energy(&mut tracker, &transaction);
            self.logic
                .calculate_operations_this_tick(&mut tracker, &transaction);
            if tracker.should_continue_checking() {
                (self.post_process_operations)(&mut tracker);
                if tracker.should_continue_checking()
                    && self.multiple_operations_cost
                    && tracker.cap_at_max_for_energy()
                {
                    tracker.add_error(RecipeError::NOT_ENOUGH_ENERGY_REDUCED_RATE);
                }
            }
            let operations = tracker.current_max();
            if tracker.has_errors_to_copy() {
                self.update_errors(tracker.errors());
            }
            operations
        } else {
            if !self.errors.is_empty() {
                self.update_errors(RecipeErrors::EMPTY);
            }
            0
        };
        if operations > 0 && self.use_energy(operations, &transaction) {
            let ticks_required = (self.required_ticks)();
            if self.operating_ticks >= ticks_required - 1 {
                if self.logic.finish_processing(operations, &transaction) {
                    (self.set_active)(true);
                    transaction.commit();
                    (self.on_finish)(operations);
                    self.logic.reset_cache();
                    if ticks_required > 1 {
                        self.operating_ticks = 0;
                        (self.operating_ticks_changed)(0);
                    }
                    return;
                }
            } else if self.logic.use_resources(operations, &transaction) {
                (self.set_active)(true);
                transaction.commit();
                if ticks_required > 1 {
                    self.operating_ticks += 1;
                    (self.operating_ticks_changed)(self.operating_ticks);
                }
                return;
            }
        }
        (self.set_active)(false);
        if operations < 0 {
            self.operating_ticks = 0;
            (self.operating_ticks_changed)(0);
            self.logic.reset_cache();
        }
    }
}

/// Um item que uma receita aceita. A lib não conhece ids: quem usa diz o que casa.
#[derive(Clone)]
pub struct ItemIngredient {
    test: Rc<dyn Fn(&ItemResource) -> bool>,
}

impl ItemIngredient {
    /// Um item só, pelo id.
    pub fn item(id: &str) -> Self {
        let id: Rc<str> = Rc::from(id);
        Self::matching(move |resource| resource.id == id)
    }

    /// Qualquer item que o predicado aceite, como uma tag.
    pub fn matching(test: impl Fn(&ItemResource) -> bool + 'static) -> Self {
        Self {
            test: Rc::new(test),
        }
    }

    pub fn matches(&self, resource: &ItemResource) -> bool {
        (self.test)(resource)
    }
}

impl fmt::Debug for ItemIngredient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ItemIngredient")
    }
}

/// Saída de uma receita de item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemOutput {
    pub resource: ItemResource,
    pub count: u32,
}

/// Uma receita de um item para um item.
#[derive(Clone, Debug)]
pub struct ItemToItemRecipe {
    pub id: String,
    pub input: ItemIngredient,
    pub input_count: u32,
    pub output: ItemOutput,
}

impl ItemToItemRecipe {
    /// `getMatchingInstance`: item diferente, ou menos do que a receita pede, não casa.
    pub fn test(&self, contents: &SlotContents) -> bool {
        contents
            .resource
            .as_ref()
            .is_some_and(|resource| self.input.matches(resource))
            && contents.amount >= self.input_count
    }
}

impl RecipeId for ItemToItemRecipe {
    fn recipe_id(&self) -> &str {
        &self.id
    }
}

/// `OneInputCachedRecipe` com o `ItemInputHandler` e o handler de saída de slot.
pub struct OneInputItemRecipe {
    recipe: Rc<ItemToItemRecipe>,
    input: ItemInputHandler,
    output: ItemOutputHandler,
    /// O que `calculateOperationsThisTick` guardou para o `finishProcessing`.
    recipe_input: Option<RecipeStack<ItemResource>>,
}

impl OneInputItemRecipe {
    pub fn new(recipe: Rc<ItemToItemRecipe>, input: InventorySlot, output: InventorySlot) -> Self {
        Self {
            recipe,
            input: ItemInputHandler::new(input, RecipeError::NOT_ENOUGH_INPUT),
            output: ItemOutputHandler::new(output, RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
            recipe_input: None,
        }
    }
}

impl RecipeLogic for OneInputItemRecipe {
    fn calculate_operations_this_tick(
        &mut self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
    ) {
        if !tracker.should_continue_checking() {
            return;
        }
        let Some(input) = self
            .input
            .recipe_input(&self.recipe.input, self.recipe.input_count)
        else {
            tracker.mismatched_recipe();
            return;
        };
        self.input
            .calculate_operations_can_support(tracker, transaction, &input, false, 1);
        self.recipe_input = Some(input);
        if tracker.should_continue_checking() {
            self.output
                .calculate_operations_can_support(tracker, transaction, &self.recipe.output);
        }
    }

    fn is_input_valid(&self) -> bool {
        self.recipe.test(&self.input.input())
    }

    fn finish_processing(&mut self, operations: i32, transaction: &Transaction) -> bool {
        self.input
            .use_input(self.recipe_input.as_ref(), operations, transaction)
            && self
                .output
                .handle_output(&self.recipe.output, operations, transaction)
    }
}
