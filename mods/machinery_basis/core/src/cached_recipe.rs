//! Porte de `CachedRecipe` e `OneInputCachedRecipe`. Os callbacks do Java viram [`CachedRecipeHolder`], e a parte abstrata
//! vira [`RecipeLogic`].
use crate::containers::{InventorySlot, ItemResource};
use crate::operation_tracker::{OperationTracker, RecipeError, RecipeErrors};
use crate::transaction::Transaction;

/// Quem roda a receita. Os padrões são os do `CachedRecipe`.
pub trait CachedRecipeHolder {
    fn can_function(&self) -> bool {
        true
    }
    fn set_active(&mut self, _active: bool) {}
    fn required_ticks(&self) -> i32 {
        1
    }
    fn on_finish(&mut self, _operations: i32) {}
    fn per_tick_energy(&self) -> i32 {
        0
    }
    fn stored_energy(&self) -> i32 {
        0
    }
    fn use_energy(&mut self, _energy: i32, _transaction: Transaction) -> i32 {
        0
    }
    fn baseline_max_operations(&self) -> i32 {
        1
    }
    fn post_process_operations(&mut self, _tracker: &mut OperationTracker) {}
    fn operations_cost(&self) -> bool {
        false
    }
    fn on_errors_change(&mut self, _errors: RecipeErrors) {}
    fn operating_ticks_changed(&mut self, _operating_ticks: i32) {}
    fn recheck_all_errors(&self) -> bool;
    /// Fecha a transação em todos os contêineres do holder.
    fn close_transaction(&mut self, transaction: Transaction, commit: bool);
}

/// O que muda de uma receita para outra.
pub trait RecipeLogic<H> {
    fn setup_variable_values(&mut self, _holder: &mut H) {}
    /// Roda depois do cálculo da energia, que é comum a todas.
    fn calculate_operations_this_tick(
        &mut self,
        holder: &mut H,
        tracker: &mut OperationTracker,
        transaction: Transaction,
    );
    fn is_input_valid(&self, holder: &H) -> bool;
    fn finish_processing(
        &mut self,
        holder: &mut H,
        operations: i32,
        transaction: Transaction,
    ) -> bool;
    fn use_resources(
        &mut self,
        _holder: &mut H,
        _operations: i32,
        _transaction: Transaction,
    ) -> bool {
        true
    }
    fn reset_cache(&mut self) {}
}

#[derive(Clone, Debug)]
pub struct CachedRecipe<L> {
    logic: L,
    errors: RecipeErrors,
    paused_for_errors: bool,
    operating_ticks: i32,
}

impl<L> CachedRecipe<L> {
    pub fn new(logic: L) -> Self {
        Self {
            logic,
            errors: RecipeErrors::EMPTY,
            paused_for_errors: false,
            operating_ticks: 0,
        }
    }

    pub fn logic(&self) -> &L {
        &self.logic
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

    pub fn load_saved_operating_ticks<H: CachedRecipeHolder>(
        &mut self,
        holder: &H,
        operating_ticks: i32,
    ) {
        if operating_ticks > 0 && operating_ticks < holder.required_ticks() {
            self.operating_ticks = operating_ticks;
        }
    }

    fn update_errors<H: CachedRecipeHolder>(&mut self, holder: &mut H, errors: RecipeErrors) {
        if self.errors != errors {
            self.errors = errors;
            self.paused_for_errors = self.errors.len() > 1
                || !self
                    .errors
                    .contains(RecipeError::NOT_ENOUGH_ENERGY_REDUCED_RATE);
            holder.on_errors_change(errors);
        }
    }

    fn use_energy<H: CachedRecipeHolder>(
        holder: &mut H,
        operations: i32,
        transaction: Transaction,
    ) -> bool {
        let mut energy = holder.per_tick_energy();
        if holder.operations_cost() {
            energy *= operations;
        }
        holder.use_energy(energy, transaction) == energy
    }

    fn calculate_energy<H: CachedRecipeHolder>(
        holder: &mut H,
        tracker: &mut OperationTracker,
        transaction: Transaction,
    ) {
        if tracker.should_continue_checking() {
            let energy_per_tick = holder.per_tick_energy();
            if energy_per_tick > 0 {
                let simulation = transaction.open_nested();
                let stored = holder.stored_energy();
                let available = holder.use_energy(stored, simulation);
                let operations = available / energy_per_tick;
                holder.close_transaction(simulation, false);
                tracker.max_for_energy = operations;
                if operations == 0 {
                    tracker.update_operations(operations);
                    tracker.add_error(RecipeError::NOT_ENOUGH_ENERGY);
                }
            }
        }
    }
}

impl<L> CachedRecipe<L> {
    pub fn is_input_valid<H>(&self, holder: &H) -> bool
    where
        L: RecipeLogic<H>,
    {
        self.logic.is_input_valid(holder)
    }

    /// `process`. Cada saída fecha a raiz: com commit, se a operação andou.
    pub fn process<H: CachedRecipeHolder>(&mut self, holder: &mut H)
    where
        L: RecipeLogic<H>,
    {
        if self.paused_for_errors {
            holder.set_active(false);
            return;
        }
        let transaction = Transaction::open_root();
        let operations = if holder.can_function() {
            self.logic.setup_variable_values(holder);
            let mut tracker = OperationTracker::new(
                self.errors,
                holder.recheck_all_errors(),
                holder.baseline_max_operations(),
            );
            Self::calculate_energy(holder, &mut tracker, transaction);
            self.logic
                .calculate_operations_this_tick(holder, &mut tracker, transaction);
            if tracker.should_continue_checking() {
                holder.post_process_operations(&mut tracker);
                if tracker.should_continue_checking()
                    && holder.operations_cost()
                    && tracker.cap_at_max_for_energy()
                {
                    tracker.add_error(RecipeError::NOT_ENOUGH_ENERGY_REDUCED_RATE);
                }
            }
            let operations = tracker.current_max();
            if tracker.has_errors_to_copy() {
                self.update_errors(holder, tracker.errors());
            }
            operations
        } else {
            if !self.errors.is_empty() {
                self.update_errors(holder, RecipeErrors::EMPTY);
            }
            0
        };
        if operations > 0 && Self::use_energy(holder, operations, transaction) {
            let ticks_required = holder.required_ticks();
            if self.operating_ticks >= ticks_required - 1 {
                if self
                    .logic
                    .finish_processing(holder, operations, transaction)
                {
                    holder.set_active(true);
                    holder.close_transaction(transaction, true);
                    holder.on_finish(operations);
                    self.logic.reset_cache();
                    if ticks_required > 1 {
                        self.operating_ticks = 0;
                        holder.operating_ticks_changed(0);
                    }
                    return;
                }
            } else if self.logic.use_resources(holder, operations, transaction) {
                holder.set_active(true);
                holder.close_transaction(transaction, true);
                if ticks_required > 1 {
                    self.operating_ticks += 1;
                    holder.operating_ticks_changed(self.operating_ticks);
                }
                return;
            }
        }
        holder.set_active(false);
        if operations < 0 {
            self.operating_ticks = 0;
            holder.operating_ticks_changed(0);
            self.logic.reset_cache();
        }
        holder.close_transaction(transaction, false);
    }
}

/// Saída de uma receita de item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemOutput {
    pub resource: ItemResource,
    pub count: u32,
}

/// Uma receita de um item para um item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemToItemRecipe {
    pub id: String,
    pub input: ItemResource,
    pub input_count: u32,
    pub output: ItemOutput,
}

impl ItemToItemRecipe {
    fn test(&self, slot: &InventorySlot) -> bool {
        slot.resource() == Some(&self.input) && slot.amount() >= self.input_count
    }
}

/// Holder com um slot de entrada e um de saída.
pub trait OneInputHolder: CachedRecipeHolder {
    fn input_slot(&self) -> &InventorySlot;
    fn input_slot_mut(&mut self) -> &mut InventorySlot;
    fn output_slot_mut(&mut self) -> &mut InventorySlot;
}

/// `OneInputCachedRecipe` com o `ItemInputHandler` e o handler de saída de slot.
#[derive(Clone, Debug)]
pub struct OneInputItemRecipe {
    pub recipe: ItemToItemRecipe,
}

impl OneInputItemRecipe {
    /// `OutputHelper.calculateOperationsCanSupport`.
    fn output_operations<H: OneInputHolder>(
        &self,
        holder: &mut H,
        tracker: &mut OperationTracker,
        transaction: Transaction,
    ) {
        let output = &self.recipe.output;
        let simulation = transaction.open_nested();
        let slot = holder.output_slot_mut();
        let amount_used = slot.insert(&output.resource, output.resource.max_stack_size, simulation);
        let operations = (amount_used / output.count) as i32;
        tracker.update_operations(operations);
        if operations == 0 {
            if amount_used == 0 && !slot.is_full() {
                tracker.add_error(RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT);
            } else {
                tracker.add_error(RecipeError::NOT_ENOUGH_OUTPUT_SPACE);
            }
        }
        holder.close_transaction(simulation, false);
    }
}

impl<H: OneInputHolder> RecipeLogic<H> for OneInputItemRecipe {
    fn calculate_operations_this_tick(
        &mut self,
        holder: &mut H,
        tracker: &mut OperationTracker,
        transaction: Transaction,
    ) {
        if !tracker.should_continue_checking() {
            return;
        }
        // `getMatchingInstance`: item diferente, ou menos do que a receita pede, volta vazio.
        if !self.recipe.test(holder.input_slot()) {
            tracker.mismatched_recipe();
            return;
        }
        let simulation = transaction.open_nested();
        let slot = holder.input_slot_mut();
        let available = slot.extract(&self.recipe.input, slot.amount(), simulation);
        holder.close_transaction(simulation, false);
        let operations = (available / self.recipe.input_count) as i32;
        if operations > 0 {
            tracker.update_operations(operations);
        } else {
            tracker.reset_progress(RecipeError::NOT_ENOUGH_INPUT);
        }
        if tracker.should_continue_checking() {
            self.output_operations(holder, tracker, transaction);
        }
    }

    fn is_input_valid(&self, holder: &H) -> bool {
        self.recipe.test(holder.input_slot())
    }

    fn finish_processing(
        &mut self,
        holder: &mut H,
        operations: i32,
        transaction: Transaction,
    ) -> bool {
        let operations = operations as u32;
        let amount = self.recipe.input_count * operations;
        if holder
            .input_slot_mut()
            .extract(&self.recipe.input, amount, transaction)
            != amount
        {
            return false;
        }
        let output = &self.recipe.output;
        let amount = output.count * operations;
        holder
            .output_slot_mut()
            .insert(&output.resource, amount, transaction)
            == amount
    }
}
