//! Porte de `InputResourceHandler` (o que `InputHelper.getInputHandler` devolve) e do `OutputHelper` para slot de item. As
//! receitas de uma ou de duas entradas usam os mesmos, cada um com o erro que o tile escolhe para "falta entrada".
use crate::cached_recipe::{ItemIngredient, ItemOutput};
use crate::containers::{AutomationType, InventorySlot, ItemResource, SlotContents};
use crate::operation_tracker::{OperationTracker, RecipeError};
use crate::transaction::Transaction;

/// O que uma receita pede de uma entrada: o recurso que casou e quanto ela gasta por operação.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipeStack<R> {
    pub resource: R,
    pub amount: u32,
}

/// `InputResourceHandler.calculateOperationsCanSupport`. `extract_available` tira do contêiner, dentro da simulação, tudo o
/// que ele tem do recurso da receita.
pub(crate) fn calculate_operations_can_support(
    tracker: &mut OperationTracker,
    transaction: &Transaction,
    not_enough_error: RecipeError,
    amount_per_operation: u32,
    is_constant_usage: bool,
    usage_multiplier: i32,
    extract_available: impl FnOnce(&Transaction) -> i32,
) {
    // Só calcula se for usar alguma coisa.
    if usage_multiplier <= 0 {
        return;
    }
    let available = {
        let simulation = Transaction::open(Some(transaction));
        extract_available(&simulation)
    };
    if available > 0 {
        let operations = available / (amount_per_operation as i32 * usage_multiplier);
        if operations > 0 {
            tracker.update_operations(operations);
            return;
        }
    }
    // Entrada que não basta para a receita: com uso constante a receita só pausa, e sem ele o progresso volta a zero.
    if is_constant_usage {
        tracker.update_operations(0);
    } else {
        tracker.reset_progress(not_enough_error);
    }
}

/// `InputHelper.getInputHandler(IInventorySlot, RecipeError)`.
#[derive(Clone)]
pub struct ItemInputHandler {
    slot: InventorySlot,
    not_enough_error: RecipeError,
}

impl ItemInputHandler {
    pub fn new(slot: InventorySlot, not_enough_error: RecipeError) -> Self {
        Self {
            slot,
            not_enough_error,
        }
    }

    /// `getInput`.
    pub fn input(&self) -> SlotContents {
        self.slot.contents()
    }

    /// `getRecipeInput`: o que o ingrediente aceita do slot, com a quantidade que a receita pede. Slot vazio, item de outro
    /// tipo ou menos do que ela pede não casam.
    pub fn recipe_input(
        &self,
        ingredient: &ItemIngredient,
        count: u32,
    ) -> Option<RecipeStack<ItemResource>> {
        let contents = self.slot.contents();
        let resource = contents
            .resource
            .filter(|resource| contents.amount >= count && ingredient.matches(resource))?;
        Some(RecipeStack {
            resource,
            amount: count,
        })
    }

    /// `use`.
    pub fn use_input(
        &self,
        input: Option<&RecipeStack<ItemResource>>,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        let Some(input) = input else {
            return false;
        };
        if operations == 0 {
            return true;
        }
        let amount = input.amount * operations as u32;
        self.slot.extract(
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
        input: &RecipeStack<ItemResource>,
        is_constant_usage: bool,
        usage_multiplier: i32,
    ) {
        let slot = &self.slot;
        calculate_operations_can_support(
            tracker,
            transaction,
            self.not_enough_error,
            input.amount,
            is_constant_usage,
            usage_multiplier,
            |simulation| {
                slot.extract(
                    &input.resource,
                    slot.amount(),
                    simulation,
                    AutomationType::Internal,
                ) as i32
            },
        );
    }
}

/// `OutputHelper.getOutputHandler(IInventorySlot, RecipeError)`.
#[derive(Clone)]
pub struct ItemOutputHandler {
    slot: InventorySlot,
    not_enough_space_error: RecipeError,
}

impl ItemOutputHandler {
    pub fn new(slot: InventorySlot, not_enough_space_error: RecipeError) -> Self {
        Self {
            slot,
            not_enough_space_error,
        }
    }

    /// `OutputHelper.calculateOperationsCanSupport`: quantas operações cabem no slot.
    pub fn calculate_operations_can_support(
        &self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
        output: &ItemOutput,
    ) {
        let simulation = Transaction::open(Some(transaction));
        let amount_used = self.slot.insert(
            &output.resource,
            output.resource.max_stack_size,
            &simulation,
            AutomationType::Internal,
        );
        let operations = (amount_used / output.count) as i32;
        tracker.update_operations(operations);
        if operations == 0 {
            if amount_used == 0 && !self.slot.is_full() {
                tracker.add_error(RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT);
            } else {
                tracker.add_error(self.not_enough_space_error);
            }
        }
    }

    /// `handleOutput`.
    pub fn handle_output(
        &self,
        output: &ItemOutput,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        if operations == 0 {
            return true;
        }
        let amount = output.count * operations as u32;
        self.slot.insert(
            &output.resource,
            amount,
            transaction,
            AutomationType::Internal,
        ) == amount
    }
}
