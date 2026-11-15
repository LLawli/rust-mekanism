//! Porte de `ItemStackToEnergyRecipe` (o tipo de receita `energy_conversion`) e de `EnergyInventorySlot.fillContainerOrConvert`:
//! um item que vira energia no contêiner da máquina (a conversão que `EnergyInventorySlot.fillContainerOrConvert` tenta depois de o item não dar energia).
//!
//! A lib não conhece nenhum item: quem usa traz as receitas como dados. O índice é o `getInputCache` do tipo
//! (`InputRecipeCache.SingleItem`), o mesmo das conversões de item em químico.
use crate::cached_recipe::ItemIngredient;
use crate::containers::{AutomationType, InventorySlot, SlotContents};
use crate::energy_cube::SlotEnergy;
use crate::recipe_cache::{
    ComponentSensitiveInputCache, InputIngredient, SingleInputRecipe, SingleInputRecipeCache,
};
use crate::recipe_monitor::RecipeId;
use crate::transaction::Transaction;
use std::rc::Rc;

/// Uma receita `energy_conversion` (`ItemStackToEnergyRecipe`): um item que dá uma quantidade de energia.
#[derive(Clone, Debug)]
pub struct EnergyConversion {
    /// O id da receita (`RecipeHolder.id`), de onde vem o hash de identidade da ordem das receitas complexas.
    pub id: String,
    pub input: ItemIngredient,
    pub input_count: u32,
    /// `getOutput`: a energia, constante (`@Range(from = 1)`).
    pub output: i32,
}

impl RecipeId for EnergyConversion {
    fn recipe_id(&self) -> &str {
        &self.id
    }
}

impl SingleInputRecipe<SlotContents> for EnergyConversion {
    /// `ItemStackToEnergyRecipe.test(ItemStack)` (`ItemInputRecipe`): o ingrediente casa e a pilha tem o que ele pede.
    fn test(&self, input: &SlotContents) -> bool {
        input.resource.is_some() && self.input.test_type(input) && input.amount >= self.input_count
    }
}

/// O índice de `MekanismRecipeType.ENERGY_CONVERSION` (`InputRecipeCache.SingleItem`).
pub type EnergyConversionCache = SingleInputRecipeCache<
    EnergyConversion,
    ItemIngredient,
    SlotContents,
    ComponentSensitiveInputCache,
>;

impl EnergyConversion {
    /// O índice das conversões que `source` devolve, como o `getRecipes(world)` do tipo: lido na primeira pergunta depois de um
    /// `clear`.
    pub fn cache(source: impl Fn() -> Vec<Rc<Self>> + 'static) -> Rc<EnergyConversionCache> {
        Rc::new(SingleInputRecipeCache::new(
            source,
            |conversion: &Self| &conversion.input,
            ComponentSensitiveInputCache::new(),
        ))
    }

    /// O índice de uma lista que não muda.
    pub fn cache_of(conversions: Vec<Self>) -> Rc<EnergyConversionCache> {
        let conversions: Vec<Rc<Self>> = conversions.into_iter().map(Rc::new).collect();
        Self::cache(move || conversions.clone())
    }
}

/// A parte de conversão de `EnergyInventorySlot.fillContainerOrConvert`, que `ItemEnergy::fill_container_or_convert` chama quando o item não deu energia
/// (`fillContainerFromSlot` falhou): `findFirstRecipe` acha a primeira receita para o que o slot tem, quantidade incluída, e só se o slot dá o que ela pede e o contêiner aceita **tudo**
/// o que ela produz (`insert(output, .., MANUAL) == output`) a troca vale. `Manual` passa por cima do limite de taxa de quem insere, mas o `canInsert` do contêiner ainda vale.
pub fn convert_to_energy(
    slot: &InventorySlot,
    container: &dyn SlotEnergy,
    conversions: &EnergyConversionCache,
    transaction: Option<&Transaction>,
) {
    let contents = slot.contents();
    let Some(resource) = contents.resource.clone() else {
        return;
    };
    let Some(conversion) = conversions.find_first_recipe(&contents) else {
        return;
    };
    let sub_transaction = Transaction::open(transaction);
    let needed = conversion.input_count;
    let output = conversion.output;
    if slot.extract(
        &resource,
        needed,
        &sub_transaction,
        AutomationType::Internal,
    ) == needed
        && container.insert(output, &sub_transaction, AutomationType::Manual) == output
    {
        sub_transaction.commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::{Access, EnergyContainer, ItemResource};

    fn conversions() -> Rc<EnergyConversionCache> {
        EnergyConversion::cache_of(vec![
            EnergyConversion {
                id: String::new(),
                input: ItemIngredient::item("redstone_block"),
                input_count: 1,
                output: 90_000,
            },
            EnergyConversion {
                id: String::new(),
                input: ItemIngredient::item("redstone"),
                input_count: 1,
                output: 10_000,
            },
        ])
    }

    fn container(max: i64) -> EnergyContainer {
        EnergyContainer::new(max, 5, Access::NotExternal, Access::Any, None)
    }

    fn slot_with(id: &str, amount: u32) -> InventorySlot {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(ItemResource::new(id, 64)), amount, None);
        slot
    }

    #[test]
    fn one_item_becomes_its_energy_each_call() {
        let (slot, energy) = (slot_with("redstone", 3), container(100_000));
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (2, 10_000));
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (1, 20_000));
    }

    #[test]
    fn the_item_stays_when_the_container_cannot_take_all_of_it() {
        let (slot, energy) = (slot_with("redstone", 3), container(9_999));
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (3, 0));
        let (slot, energy) = (slot_with("redstone", 3), container(20_000));
        energy.set_energy(10_001, None);
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (3, 10_001));
    }

    #[test]
    fn it_takes_exactly_the_room_there_is() {
        let (slot, energy) = (slot_with("redstone", 3), container(20_000));
        energy.set_energy(10_000, None);
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (2, 20_000));
    }

    #[test]
    fn a_block_gives_nine_times_as_much() {
        let (slot, energy) = (slot_with("redstone_block", 2), container(100_000));
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (1, 90_000));
    }

    #[test]
    fn an_item_with_no_recipe_or_an_empty_slot_changes_nothing() {
        let (slot, energy) = (slot_with("stick", 2), container(100_000));
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (2, 0));
        let empty = InventorySlot::new(None);
        convert_to_energy(&empty, &energy, &conversions(), None);
        assert_eq!(energy.amount(), 0);
    }

    #[test]
    fn a_recipe_that_needs_more_than_the_slot_has_is_not_found() {
        let cache = EnergyConversion::cache_of(vec![EnergyConversion {
            id: String::new(),
            input: ItemIngredient::item("redstone"),
            input_count: 2,
            output: 7,
        }]);
        let (slot, energy) = (slot_with("redstone", 1), container(100));
        convert_to_energy(&slot, &energy, &cache, None);
        assert_eq!((slot.amount(), energy.amount()), (1, 0));
        let slot = slot_with("redstone", 3);
        convert_to_energy(&slot, &energy, &cache, None);
        assert_eq!((slot.amount(), energy.amount()), (1, 7));
    }

    #[test]
    fn a_container_that_refuses_manual_insertion_gets_nothing() {
        let energy =
            EnergyContainer::new(100_000, 5, Access::NotExternal, Access::InternalOnly, None);
        let slot = slot_with("redstone", 1);
        convert_to_energy(&slot, &energy, &conversions(), None);
        assert_eq!((slot.amount(), energy.amount()), (1, 0));
    }

    #[test]
    fn a_rolled_back_outer_transaction_undoes_the_conversion() {
        let (slot, energy) = (slot_with("redstone", 2), container(100_000));
        let outer = Transaction::open_root();
        convert_to_energy(&slot, &energy, &conversions(), Some(&outer));
        assert_eq!((slot.amount(), energy.amount()), (1, 10_000));
        drop(outer);
        assert_eq!((slot.amount(), energy.amount()), (2, 0));
    }
}
