//! Porte de `TwoInputCachedRecipe` com um item e um químico de entrada e um item de saída (a receita do Metallurgic
//! Infuser, `ItemStackChemicalToItemStackRecipe`), e de `CachedRecipeHelper.twoInputCalculateOperationsThisTick`.
use crate::cached_recipe::{ItemIngredient, ItemOutput, RecipeLogic};
use crate::chemical::{
    ChemicalIngredient, ChemicalInputHandler, ChemicalResource, ChemicalTank, TankContents,
};
use crate::containers::{InventorySlot, ItemResource, SlotContents};
use crate::operation_tracker::{OperationTracker, RecipeError};
use crate::recipe_cache::{
    ChemicalInputCache, ComponentSensitiveInputCache, DoubleInputRecipe, DoubleInputRecipeCache,
    InputIngredient,
};
use crate::recipe_handlers::{ItemInputHandler, ItemOutputHandler, RecipeStack};
use crate::recipe_monitor::RecipeId;
use crate::transaction::Transaction;
use std::rc::Rc;

/// Uma receita de um item e um químico para um item. Os valores e os ids vêm do mod.
///
/// `per_tick_usage` escolhe a receita em cache, como o `createNewCachedRecipe` do Java: falso é `TwoInputCachedRecipe`
/// ([`TwoInputItemChemicalRecipe`]), verdadeiro gasta o químico a cada tick
/// ([`crate::item_chemical_constant_recipe::ItemChemicalConstantUsageRecipe`]).
#[derive(Clone, Debug)]
pub struct ItemChemicalToItemRecipe {
    pub id: String,
    pub item_input: ItemIngredient,
    pub item_count: u32,
    pub chemical_input: ChemicalIngredient,
    pub chemical_amount: u32,
    pub output: ItemOutput,
    /// `perTickUsage()`.
    pub per_tick_usage: bool,
}

impl ItemChemicalToItemRecipe {
    /// `TwoInputMekRecipe.test`: o item e o químico casam, com a quantidade que a receita pede de cada um.
    pub fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
        item.resource.is_some()
            && self.item_input.test_type(item)
            && item.amount >= self.item_count
            && chemical.resource.is_some()
            && self.chemical_input.test_type(chemical)
            && chemical.amount >= u64::from(self.chemical_amount)
    }

    /// O índice de `MekanismRecipeType.getInputCache` (`InputRecipeCache.ItemChemical`) das receitas que `source` devolve, lido
    /// na primeira pergunta depois de um `clear`. `empty_chemical` é o `ChemicalIds.EMPTY`, que o índice do químico não guarda.
    pub fn cache(
        source: impl Fn() -> Vec<Rc<Self>> + 'static,
        empty_chemical: &str,
    ) -> Rc<ItemChemicalCache> {
        Rc::new(DoubleInputRecipeCache::new(
            source,
            |recipe: &Self| &recipe.item_input,
            ComponentSensitiveInputCache::new(),
            |recipe: &Self| &recipe.chemical_input,
            ChemicalInputCache::new(Some(empty_chemical)),
        ))
    }
}

impl ItemChemicalToItemRecipe {
    /// O índice de uma lista que não muda.
    pub fn cache_of(recipes: Vec<Rc<Self>>, empty_chemical: &str) -> Rc<ItemChemicalCache> {
        Self::cache(move || recipes.clone(), empty_chemical)
    }
}

impl DoubleInputRecipe<SlotContents, TankContents> for ItemChemicalToItemRecipe {
    fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
        Self::test(self, item, chemical)
    }
}

/// O índice das receitas de um item e um químico.
pub type ItemChemicalCache = DoubleInputRecipeCache<
    ItemChemicalToItemRecipe,
    ItemIngredient,
    SlotContents,
    ComponentSensitiveInputCache,
    ChemicalIngredient,
    TankContents,
    ChemicalInputCache,
>;

impl RecipeId for ItemChemicalToItemRecipe {
    fn recipe_id(&self) -> &str {
        &self.id
    }
}

/// `TwoInputCachedRecipe`: o item é a entrada principal e o químico a secundária.
pub struct TwoInputItemChemicalRecipe {
    recipe: Rc<ItemChemicalToItemRecipe>,
    item_handler: ItemInputHandler,
    chemical_handler: ChemicalInputHandler,
    output_handler: ItemOutputHandler,
    /// O que `calculateOperationsThisTick` guardou para o `finishProcessing`.
    input: Option<RecipeStack<ItemResource>>,
    secondary_input: Option<RecipeStack<ChemicalResource>>,
}

impl TwoInputItemChemicalRecipe {
    /// Os erros de cada handler são do tile: no Metallurgic Infuser o do item é `NOT_ENOUGH_SECONDARY_INPUT` e o do químico
    /// `NOT_ENOUGH_INPUT`.
    pub fn new(
        recipe: Rc<ItemChemicalToItemRecipe>,
        item_input: InventorySlot,
        item_error: RecipeError,
        chemical_input: ChemicalTank,
        chemical_error: RecipeError,
        output: InventorySlot,
        output_error: RecipeError,
    ) -> Self {
        Self {
            recipe,
            item_handler: ItemInputHandler::new(item_input, item_error),
            chemical_handler: ChemicalInputHandler::new(chemical_input, chemical_error),
            output_handler: ItemOutputHandler::new(output, output_error),
            input: None,
            secondary_input: None,
        }
    }
}

impl RecipeLogic for TwoInputItemChemicalRecipe {
    /// `CachedRecipeHelper.twoInputCalculateOperationsThisTick`.
    fn calculate_operations_this_tick(
        &mut self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
    ) {
        if !tracker.should_continue_checking() {
            return;
        }
        // Sem entrada não se sabe se a receita casa: trata como se não casasse.
        let Some(input) = self
            .item_handler
            .recipe_input(&self.recipe.item_input, self.recipe.item_count)
        else {
            tracker.mismatched_recipe();
            return;
        };
        let Some(secondary) = self
            .chemical_handler
            .recipe_input(&self.recipe.chemical_input, self.recipe.chemical_amount)
        else {
            tracker.mismatched_recipe();
            return;
        };
        self.item_handler
            .calculate_operations_can_support(tracker, transaction, &input, false, 1);
        if tracker.should_continue_checking() {
            self.chemical_handler.calculate_operations_can_support(
                tracker,
                transaction,
                &secondary,
                false,
                1,
            );
            if tracker.should_continue_checking() {
                self.output_handler.calculate_operations_can_support(
                    tracker,
                    transaction,
                    &self.recipe.output,
                );
            }
        }
        self.input = Some(input);
        self.secondary_input = Some(secondary);
    }

    fn is_input_valid(&self) -> bool {
        let input = self.item_handler.input();
        if input.is_empty() {
            return false;
        }
        let secondary = self.chemical_handler.input();
        !secondary.is_empty() && self.recipe.test(&input, &secondary)
    }

    fn finish_processing(&mut self, operations: i32, transaction: &Transaction) -> bool {
        self.item_handler
            .use_input(self.input.as_ref(), operations, transaction)
            && self.chemical_handler.use_input(
                self.secondary_input.as_ref(),
                operations,
                transaction,
            )
            && self
                .output_handler
                .handle_output(&self.recipe.output, operations, transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(item: &str, chemical: &str, amount: u32) -> Rc<ItemChemicalToItemRecipe> {
        Rc::new(ItemChemicalToItemRecipe {
            id: format!("{item}+{chemical}"),
            item_input: ItemIngredient::item(item),
            item_count: 1,
            chemical_input: ChemicalIngredient::chemical(chemical),
            chemical_amount: amount,
            output: ItemOutput {
                resource: ItemResource::new("out", 64),
                count: 1,
            },
            per_tick_usage: false,
        })
    }

    fn book() -> Rc<ItemChemicalCache> {
        let recipes = vec![recipe("copper", "redstone", 10), recipe("sand", "bio", 10)];
        ItemChemicalToItemRecipe::cache(move || recipes.clone(), "empty")
    }

    fn item(id: &str, amount: u32) -> SlotContents {
        SlotContents::new(Some(ItemResource::new(id, 64)), amount)
    }

    fn chemical(id: &str, amount: u64) -> TankContents {
        TankContents::new(Some(ChemicalResource::new(id)), amount)
    }

    #[test]
    fn the_recipe_needs_both_inputs_in_full() {
        let book = book();
        let find =
            |item: &SlotContents, chemical: &TankContents| book.find_first_recipe(item, chemical);
        assert!(find(&item("copper", 1), &chemical("redstone", 10)).is_some());
        assert!(find(&item("copper", 1), &chemical("redstone", 9)).is_none());
        assert!(find(&item("copper", 1), &chemical("bio", 10)).is_none());
        assert!(find(&item("copper", 1), &TankContents::default()).is_none());
        assert!(find(&SlotContents::default(), &chemical("redstone", 10)).is_none());
    }

    #[test]
    fn the_tank_asks_only_about_the_pairing_with_the_input_slot() {
        let book = book();
        let copper = Some(ItemResource::new("copper", 64));
        let redstone = Some(ChemicalResource::new("redstone"));
        let bio = Some(ChemicalResource::new("bio"));
        assert!(book.contains_input_ba(&copper, &redstone));
        assert!(!book.contains_input_ba(&copper, &bio));
        assert!(book.contains_input_ba(&None::<ItemResource>, &bio));
        assert!(book.contains_input_b(&bio));
        assert!(!book.contains_input_b(&Some(ChemicalResource::new("tin"))));
        assert!(book.contains_input_a(&copper));
    }
}
