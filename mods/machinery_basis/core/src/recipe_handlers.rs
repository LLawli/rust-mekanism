//! Porte de `InputResourceHandler` (o que `InputHelper.getInputHandler` devolve) e do `OutputHelper` para slot de item. As
//! receitas de uma ou de duas entradas usam os mesmos, cada um com o erro que o tile escolhe para "falta entrada".
use crate::cached_recipe::{ItemIngredient, ItemOutput};
use crate::containers::{AutomationType, InventorySlot, ItemResource, SlotContents};
use crate::operation_tracker::{OperationTracker, RecipeError};
use crate::resource_tank::{Ingredient, Resource, ResourceTank};
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

/// `IOutputHandler<OUTPUT>`: o que o `OutputHelper.getOutputHandler` devolve para cada tipo de saída.
pub trait OutputHandler<O> {
    /// `calculateOperationsCanSupport`.
    fn calculate_operations_can_support(
        &self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
        output: &O,
    );

    /// `handleOutput`.
    fn handle_output(&self, output: &O, operations: i32, transaction: &Transaction) -> bool;
}

/// `OutputHelper.calculateOperationsCanSupport(tracker, notEnoughSpace, container, toOutput, outputSize, maxStackSize)`:
/// `insert_max` insere, dentro da simulação, a quantidade máxima da saída (`maxStackSize`).
pub(crate) fn calculate_output_operations(
    tracker: &mut OperationTracker,
    transaction: &Transaction,
    not_enough_space: RecipeError,
    output_size: u32,
    insert_max: impl FnOnce(&Transaction) -> u32,
    is_full: impl FnOnce() -> bool,
) {
    let simulation = Transaction::open(Some(transaction));
    // Tenta inserir a quantidade máxima da saída, e divide o que coube pelo que uma operação dá.
    let amount_used = insert_max(&simulation);
    let operations = (amount_used / output_size) as i32;
    tracker.update_operations(operations);
    if operations == 0 {
        if amount_used == 0 && !is_full() {
            tracker.add_error(RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT);
        } else {
            tracker.add_error(not_enough_space);
        }
    }
}

/// `OutputHelper.handleOutput(container, toOutput, amount, operations, transaction)`.
pub(crate) fn handle_output_amount(
    amount: u32,
    operations: i32,
    insert: impl FnOnce(u32) -> u32,
) -> bool {
    if operations == 0 {
        return true;
    }
    let scaled = amount * operations as u32;
    insert(scaled) == scaled
}

/// `InputHelper.getInputHandler(IChemicalTank, ..)` e `getInputHandler(IFluidTank, ..)`: o `InputResourceHandler` sobre um
/// contêiner de recurso grande.
#[derive(Clone)]
pub struct TankInputHandler<R: Resource> {
    tank: ResourceTank<R>,
    not_enough_error: RecipeError,
}

impl<R: Resource> TankInputHandler<R> {
    pub fn new(tank: ResourceTank<R>, not_enough_error: RecipeError) -> Self {
        Self {
            tank,
            not_enough_error,
        }
    }

    /// `getInput`.
    pub fn input(&self) -> crate::resource_tank::StackContents<R> {
        self.tank.contents()
    }

    /// `getRecipeInput`: o que o ingrediente aceita do contêiner, com a quantidade que a receita pede. Contêiner vazio, outro
    /// recurso ou menos do que ela pede não casam.
    pub fn recipe_input(&self, ingredient: &Ingredient<R>, amount: u32) -> Option<RecipeStack<R>> {
        let contents = self.tank.contents();
        let resource = contents.resource.filter(|resource| {
            contents.amount >= u64::from(amount) && ingredient.matches(resource)
        })?;
        Some(RecipeStack { resource, amount })
    }

    /// `use`.
    pub fn use_input(
        &self,
        input: Option<&RecipeStack<R>>,
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
        input: &RecipeStack<R>,
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

/// `ChemicalStackTemplate`, `FluidStackTemplate`: o que uma receita entrega a um contêiner de recurso grande.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceOutput<R> {
    pub resource: R,
    pub amount: u32,
}

/// `OutputHelper.getOutputHandler(IChemicalTank, ..)` e `getOutputHandler(IFluidTank, ..)`.
#[derive(Clone)]
pub struct TankOutputHandler<R: Resource> {
    tank: ResourceTank<R>,
    not_enough_space_error: RecipeError,
}

impl<R: Resource> TankOutputHandler<R> {
    pub fn new(tank: ResourceTank<R>, not_enough_space_error: RecipeError) -> Self {
        Self {
            tank,
            not_enough_space_error,
        }
    }
}

impl<R: Resource> OutputHandler<ResourceOutput<R>> for TankOutputHandler<R> {
    fn calculate_operations_can_support(
        &self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
        output: &ResourceOutput<R>,
    ) {
        let tank = &self.tank;
        calculate_output_operations(
            tracker,
            transaction,
            self.not_enough_space_error,
            output.amount,
            |simulation| {
                // O limite de quem não tem pilha é `Integer.MAX_VALUE`.
                tank.insert(
                    &output.resource,
                    i32::MAX,
                    simulation,
                    AutomationType::Internal,
                ) as u32
            },
            || tank.is_full(),
        );
    }

    fn handle_output(
        &self,
        output: &ResourceOutput<R>,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        handle_output_amount(output.amount, operations, |scaled| {
            self.tank.insert(
                &output.resource,
                scaled as i32,
                transaction,
                AutomationType::Internal,
            ) as u32
        })
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
        calculate_output_operations(
            tracker,
            transaction,
            self.not_enough_space_error,
            output.count,
            |simulation| {
                self.slot.insert(
                    &output.resource,
                    output.resource.max_stack_size,
                    simulation,
                    AutomationType::Internal,
                )
            },
            || self.slot.is_full(),
        );
    }

    /// `handleOutput`.
    pub fn handle_output(
        &self,
        output: &ItemOutput,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        handle_output_amount(output.count, operations, |amount| {
            self.slot.insert(
                &output.resource,
                amount,
                transaction,
                AutomationType::Internal,
            )
        })
    }
}

impl OutputHandler<ItemOutput> for ItemOutputHandler {
    fn calculate_operations_can_support(
        &self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
        output: &ItemOutput,
    ) {
        Self::calculate_operations_can_support(self, tracker, transaction, output);
    }

    fn handle_output(
        &self,
        output: &ItemOutput,
        operations: i32,
        transaction: &Transaction,
    ) -> bool {
        Self::handle_output(self, output, operations, transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_tracker::RecipeErrors;

    const COUNT: u32 = 3;

    fn handler(items: u32) -> ItemInputHandler {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(ItemResource::new("hdpe_pellet", 64)), items, None);
        ItemInputHandler::new(slot, RecipeError::NOT_ENOUGH_INPUT)
    }

    fn ingredient() -> ItemIngredient {
        ItemIngredient::item("hdpe_pellet")
    }

    /// Roda `calculate_operations_can_support` num tracker novo e devolve o máximo de operações e os erros que sobraram.
    fn run(items: u32, constant: bool, multiplier: i32) -> (i32, RecipeErrors) {
        let handler = handler(items);
        let input = RecipeStack {
            resource: ItemResource::new("hdpe_pellet", 64),
            amount: COUNT,
        };
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, false, 8);
        let root = Transaction::open_root();
        handler.calculate_operations_can_support(&mut tracker, &root, &input, constant, multiplier);
        (tracker.current_max(), tracker.errors())
    }

    /// `InputResourceHandler.getRecipeInput` + `SizedIngredient.test`: menos do que a receita pede nem chega ao
    /// `calculateOperationsCanSupport`, e a receita vira `mismatchedRecipe`, sem erro.
    #[test]
    fn less_than_the_recipe_asks_for_is_no_recipe_input_at_all() {
        assert!(
            handler(COUNT - 1)
                .recipe_input(&ingredient(), COUNT)
                .is_none()
        );
        assert!(handler(0).recipe_input(&ingredient(), COUNT).is_none());
        assert!(handler(COUNT).recipe_input(&ingredient(), COUNT).is_some());
    }

    /// O que o `recipe_input` entrega sempre dá ao menos uma operação a uma operação por tick: o erro de entrada não nasce.
    #[test]
    fn what_the_recipe_input_gives_always_supports_an_operation() {
        for items in COUNT..=COUNT * 4 {
            let (max, errors) = run(items, false, 1);
            assert_eq!(max, (items / COUNT) as i32, "{items}");
            assert!(errors.is_empty(), "{items}");
        }
    }

    /// A única entrada do Java para o erro: um multiplicador de uso maior que um, sem uso constante (nenhum `CachedRecipe` do Mekanism a usa).
    #[test]
    fn a_usage_multiplier_above_the_stock_is_the_only_way_to_the_error() {
        let (max, errors) = run(COUNT, false, 2);
        assert_eq!(max, -1, "o progresso volta a zero");
        assert!(errors.contains(RecipeError::NOT_ENOUGH_INPUT));
        // Com o estoque que cobre o multiplicador, nada.
        let (max, errors) = run(COUNT * 2, false, 2);
        assert_eq!((max, errors.is_empty()), (1, true));
    }

    /// Com uso constante a receita só pausa (`updateOperations(0)`): o erro não nasce, e é por isso que o Infuser sem químico para sem aviso.
    #[test]
    fn constant_usage_pauses_without_the_error() {
        let (max, errors) = run(COUNT, true, 2);
        assert_eq!(max, 0);
        assert!(errors.is_empty());
    }

    /// Multiplicador zero: o Java nem calcula (`usageMultiplier > 0`).
    #[test]
    fn a_zero_multiplier_does_not_touch_the_tracker() {
        let (max, errors) = run(0, false, 0);
        assert_eq!(max, 8);
        assert!(errors.is_empty());
    }
}
