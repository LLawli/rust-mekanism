//! Porte de `TwoInputCachedRecipe` com um item e um químico de entrada e um item de saída (a receita do Metallurgic
//! Infuser, `ItemStackChemicalToItemStackRecipe`), e de `CachedRecipeHelper.twoInputCalculateOperationsThisTick`.
use crate::cached_recipe::{ItemIngredient, ItemOutput, RecipeLogic};
use crate::chemical::{
    ChemicalIngredient, ChemicalInputHandler, ChemicalResource, ChemicalTank, TankContents,
};
use crate::containers::{InventorySlot, ItemResource, SlotContents};
use crate::operation_tracker::{OperationTracker, RecipeError};
use crate::recipe_handlers::{ItemInputHandler, ItemOutputHandler, RecipeStack};
use crate::recipe_monitor::RecipeId;
use crate::transaction::Transaction;
use std::cell::RefCell;
use std::rc::Rc;

/// Uma receita de um item e um químico para um item. Os valores e os ids vêm do mod.
///
/// `per_tick_usage` não existe aqui: a receita que gasta o químico a cada tick pede
/// `ItemStackConstantChemicalToObjectCachedRecipe`, que a lib ainda não porta, e quem monta as receitas deixa essas de fora.
#[derive(Clone, Debug)]
pub struct ItemChemicalToItemRecipe {
    pub id: String,
    pub item_input: ItemIngredient,
    pub item_count: u32,
    pub chemical_input: ChemicalIngredient,
    pub chemical_amount: u32,
    pub output: ItemOutput,
}

impl ItemChemicalToItemRecipe {
    /// `TwoInputMekRecipe.test`: o item e o químico casam, com a quantidade que a receita pede de cada um.
    pub fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
        item.resource
            .as_ref()
            .is_some_and(|resource| self.item_input.matches(resource))
            && item.amount >= self.item_count
            && chemical
                .resource
                .as_ref()
                .is_some_and(|resource| self.chemical_input.matches(resource))
            && chemical.amount >= u64::from(self.chemical_amount)
    }
}

impl RecipeId for ItemChemicalToItemRecipe {
    fn recipe_id(&self) -> &str {
        &self.id
    }
}

/// As receitas de um tile, com as perguntas de `DoubleInputRecipeCache`. Compartilhado: o tile troca a lista quando os dados
/// recarregam, e quem pergunta (o tanque, a receita em cache) vê a nova.
pub struct ItemChemicalRecipes {
    recipes: RefCell<Rc<Vec<Rc<ItemChemicalToItemRecipe>>>>,
}

impl ItemChemicalRecipes {
    pub fn new(recipes: Vec<Rc<ItemChemicalToItemRecipe>>) -> Rc<Self> {
        Rc::new(Self {
            recipes: RefCell::new(Rc::new(recipes)),
        })
    }

    pub fn set(&self, recipes: Vec<Rc<ItemChemicalToItemRecipe>>) {
        *self.recipes.borrow_mut() = Rc::new(recipes);
    }

    fn all(&self) -> Rc<Vec<Rc<ItemChemicalToItemRecipe>>> {
        self.recipes.borrow().clone()
    }

    /// `containsInputA`: o tipo do item entra em alguma receita.
    pub fn contains_item(&self, item: &ItemResource) -> bool {
        self.all()
            .iter()
            .any(|recipe| recipe.item_input.matches(item))
    }

    /// `containsInputB`: o tipo do químico entra em alguma receita.
    pub fn contains_chemical(&self, chemical: &ChemicalResource) -> bool {
        self.all()
            .iter()
            .any(|recipe| recipe.chemical_input.matches(chemical))
    }

    /// `containsInputBA`, o que o tanque pergunta para inserir `chemical` com `item` no slot de entrada: sem item, vale
    /// qualquer químico que alguma receita use (a validação do tanque já olhou isso); com item, o par tem de existir.
    pub fn contains_pairing_ba(
        &self,
        item: Option<&ItemResource>,
        chemical: &ChemicalResource,
    ) -> bool {
        let Some(item) = item else {
            return true;
        };
        self.all().iter().any(|recipe| {
            recipe.chemical_input.matches(chemical) && recipe.item_input.matches(item)
        })
    }

    /// `findFirstRecipe(inputA, inputB)`: a primeira receita que as duas entradas pagam por inteiro.
    pub fn find_first(
        &self,
        item: &SlotContents,
        chemical: &TankContents,
    ) -> Option<Rc<ItemChemicalToItemRecipe>> {
        if item.is_empty() || chemical.is_empty() {
            return None;
        }
        self.all()
            .iter()
            .find(|recipe| recipe.test(item, chemical))
            .cloned()
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
        })
    }

    fn book() -> Rc<ItemChemicalRecipes> {
        ItemChemicalRecipes::new(vec![
            recipe("copper", "redstone", 10),
            recipe("sand", "bio", 10),
        ])
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
        assert!(
            book.find_first(&item("copper", 1), &chemical("redstone", 10))
                .is_some()
        );
        assert!(
            book.find_first(&item("copper", 1), &chemical("redstone", 9))
                .is_none()
        );
        assert!(
            book.find_first(&item("copper", 1), &chemical("bio", 10))
                .is_none()
        );
        assert!(
            book.find_first(&item("copper", 1), &TankContents::default())
                .is_none()
        );
        assert!(
            book.find_first(&SlotContents::default(), &chemical("redstone", 10))
                .is_none()
        );
    }

    #[test]
    fn the_tank_asks_only_about_the_pairing_with_the_input_slot() {
        let book = book();
        let copper = ItemResource::new("copper", 64);
        let redstone = ChemicalResource::new("redstone");
        let bio = ChemicalResource::new("bio");
        assert!(book.contains_pairing_ba(Some(&copper), &redstone));
        assert!(!book.contains_pairing_ba(Some(&copper), &bio));
        assert!(book.contains_pairing_ba(None, &bio));
        assert!(book.contains_chemical(&bio));
        assert!(!book.contains_chemical(&ChemicalResource::new("tin")));
        assert!(book.contains_item(&copper));
    }
}
