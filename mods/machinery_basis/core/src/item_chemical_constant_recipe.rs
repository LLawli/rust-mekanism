//! Porte de `ItemStackConstantChemicalToObjectCachedRecipe`: uma receita de um item e um químico em que o químico não sai de
//! uma vez no fim, e sim aos poucos, a cada tick (`per_tick_usage`). É a variante de `TwoInputCachedRecipe` das máquinas que
//! usam químico por tick (Osmium Compressor, Purification Chamber, Chemical Injection Chamber), e o `chemicalUsedSoFar` dela é
//! o que o tile guarda como `usedSoFar`.
use crate::cached_recipe::{ItemIngredient, ItemOutput, RecipeLogic};
use crate::chemical::{ChemicalIngredient, ChemicalInputHandler, ChemicalResource, TankContents};
use crate::containers::{ItemResource, SlotContents};
use crate::operation_tracker::OperationTracker;
use crate::recipe_handlers::{ItemInputHandler, OutputHandler, RecipeStack};
use crate::resource_tank::saturated_amount;
use crate::transaction::Transaction;
use crate::two_input_recipe::ItemChemicalToItemRecipe;
use std::rc::Rc;

/// `ItemStackChemicalToObjectRecipe<OUTPUT, ..>`: o que a receita de uso constante pede à receita.
pub trait ItemChemicalToObjectRecipe {
    /// O `OUTPUT` do Java.
    type Output;

    /// `getItemInput`, com a quantidade que ela pede.
    fn item_input(&self) -> &ItemIngredient;
    fn item_count(&self) -> u32;
    /// `getChemicalInput`, com a quantidade que ela pede.
    fn chemical_input(&self) -> &ChemicalIngredient;
    fn chemical_amount(&self) -> u32;
    /// `test(item, chemical)`.
    fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool;
    /// `getOutput(item, chemical)`.
    fn output(
        &self,
        item: &RecipeStack<ItemResource>,
        chemical: &RecipeStack<ChemicalResource>,
    ) -> Self::Output;
}

impl ItemChemicalToObjectRecipe for ItemChemicalToItemRecipe {
    type Output = ItemOutput;

    fn item_input(&self) -> &ItemIngredient {
        &self.item_input
    }

    fn item_count(&self) -> u32 {
        self.item_count
    }

    fn chemical_input(&self) -> &ChemicalIngredient {
        &self.chemical_input
    }

    fn chemical_amount(&self) -> u32 {
        self.chemical_amount
    }

    fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
        ItemChemicalToItemRecipe::test(self, item, chemical)
    }

    fn output(
        &self,
        _item: &RecipeStack<ItemResource>,
        _chemical: &RecipeStack<ChemicalResource>,
    ) -> ItemOutput {
        self.output.clone()
    }
}

/// `ChemicalUsageMultiplier`: quanto químico gastar neste tick, dado o que já se gastou e os ticks de operação.
#[derive(Clone)]
pub struct ChemicalUsageMultiplier(Rc<dyn Fn(i32, i32) -> i32>);

impl ChemicalUsageMultiplier {
    /// Uma regra qualquer, como a do `inversePoisson` das máquinas de mecânica estatística: `(usedSoFar, operatingTicks)`.
    pub fn new(to_use: impl Fn(i32, i32) -> i32 + 'static) -> Self {
        Self(Rc::new(to_use))
    }

    /// `ChemicalUsageMultiplier.constantUse(baseTotalUsage, ticksRequired)`: reparte o que falta gastar pelos ticks que
    /// faltam, de modo que a operação termine com o total.
    pub fn constant_use(
        base_total_usage: impl Fn() -> i32 + 'static,
        ticks_required: impl Fn() -> i32 + 'static,
    ) -> Self {
        Self::new(move |used_so_far, operating_ticks| {
            let base_remaining = base_total_usage().wrapping_sub(used_so_far);
            let remaining_ticks = ticks_required().wrapping_sub(operating_ticks);
            if base_remaining < remaining_ticks {
                // Se já se gastou mais do que o preciso (por tirar upgrades de velocidade ou pôr de gás), não gasta
                // nada neste tick.
                0
            } else if base_remaining == remaining_ticks {
                1
            } else {
                clamp_to_int(f64::from(base_remaining) / f64::from(remaining_ticks)).max(0)
            }
        })
    }

    /// `getToUse`.
    pub fn to_use(&self, used_so_far: i32, operating_ticks: i32) -> i32 {
        (self.0)(used_so_far, operating_ticks)
    }
}

/// `MathUtils.clampToInt`.
fn clamp_to_int(value: f64) -> i32 {
    if value > f64::from(i32::MAX) {
        i32::MAX
    } else if value < f64::from(i32::MIN) {
        i32::MIN
    } else {
        // O `(int) d` do Java corta para zero, e NaN dá zero.
        value as i32
    }
}

/// `ItemStackConstantChemicalToObjectCachedRecipe`.
pub struct ItemChemicalConstantUsageRecipe<R: ItemChemicalToObjectRecipe, H> {
    recipe: Rc<R>,
    item_handler: ItemInputHandler,
    chemical_handler: ChemicalInputHandler,
    output_handler: H,
    chemical_usage: ChemicalUsageMultiplier,
    chemical_used_so_far_changed: Box<dyn Fn(i32)>,
    chemical_usage_multiplier: i32,
    chemical_used_so_far: i32,
    /// O que `calculateOperationsThisTick` guardou para `useResources` e `finishProcessing`. Como no Java, os campos ficam com
    /// o último valor quando um tick sai antes de calculá-los.
    recipe_item: Option<RecipeStack<ItemResource>>,
    recipe_chemical: Option<RecipeStack<ChemicalResource>>,
    output: Option<R::Output>,
}

impl<R: ItemChemicalToObjectRecipe, H: OutputHandler<R::Output>>
    ItemChemicalConstantUsageRecipe<R, H>
{
    /// `create(recipe, recheckAllErrors, itemInputHandler, chemicalInputHandler, chemicalUsage, chemicalUsedSoFarChanged,
    /// outputHandler)`.
    pub fn new(
        recipe: Rc<R>,
        item_handler: ItemInputHandler,
        chemical_handler: ChemicalInputHandler,
        chemical_usage: ChemicalUsageMultiplier,
        chemical_used_so_far_changed: impl Fn(i32) + 'static,
        output_handler: H,
    ) -> Self {
        Self {
            recipe,
            item_handler,
            chemical_handler,
            output_handler,
            chemical_usage,
            chemical_used_so_far_changed: Box::new(chemical_used_so_far_changed),
            chemical_usage_multiplier: 0,
            chemical_used_so_far: 0,
            recipe_item: None,
            recipe_chemical: None,
            output: None,
        }
    }

    pub fn chemical_used_so_far(&self) -> i32 {
        self.chemical_used_so_far
    }
}

impl<R: ItemChemicalToObjectRecipe, H: OutputHandler<R::Output>> RecipeLogic
    for ItemChemicalConstantUsageRecipe<R, H>
{
    fn load_saved_usage_so_far(&mut self, used_so_far: i32) {
        if used_so_far > 0 {
            self.chemical_used_so_far = used_so_far;
        }
    }

    fn setup_variable_values(&mut self, operating_ticks: i32) {
        self.chemical_usage_multiplier = self
            .chemical_usage
            .to_use(self.chemical_used_so_far, operating_ticks)
            .max(0);
    }

    fn calculate_operations_this_tick(
        &mut self,
        tracker: &mut OperationTracker,
        transaction: &Transaction,
    ) {
        if !tracker.should_continue_checking() {
            return;
        }
        self.recipe_item = self
            .item_handler
            .recipe_input(self.recipe.item_input(), self.recipe.item_count());
        // Sem item que pague uma operação, não se sabe se a receita casa: trata como se não casasse.
        let Some(recipe_item) = self.recipe_item.clone() else {
            tracker.mismatched_recipe();
            return;
        };
        self.recipe_chemical = self
            .chemical_handler
            .recipe_input(self.recipe.chemical_input(), self.recipe.chemical_amount());
        if self.recipe_chemical.is_none() {
            // Não se força o reset por falta do químico de uso por tick: a receita só pausa.
            tracker.update_operations(0);
            if !tracker.should_continue_checking() {
                // Sem químico também não dá para checar os erros da saída.
                return;
            }
        }
        self.item_handler.calculate_operations_can_support(
            tracker,
            transaction,
            &recipe_item,
            false,
            1,
        );
        let Some(recipe_chemical) = self.recipe_chemical.clone() else {
            return;
        };
        if tracker.should_continue_checking() {
            // O químico é de uso constante: sem o bastante a receita pausa em vez de voltar ao zero.
            self.chemical_handler.calculate_operations_can_support(
                tracker,
                transaction,
                &recipe_chemical,
                true,
                self.chemical_usage_multiplier,
            );
            if tracker.should_continue_checking() {
                let output = self.recipe.output(&recipe_item, &recipe_chemical);
                self.output_handler
                    .calculate_operations_can_support(tracker, transaction, &output);
                self.output = Some(output);
            }
        }
    }

    fn is_input_valid(&self) -> bool {
        let item = self.item_handler.input();
        if item.is_empty() {
            return false;
        }
        // O `ChemicalStack` do Java tem a quantidade como `int`.
        let chemical = self.chemical_handler.input();
        let chemical =
            TankContents::new(chemical.resource, saturated_amount(chemical.amount) as u64);
        if chemical.is_empty() || !self.recipe.test(&item, &chemical) {
            return false;
        }
        // Confere que o recipe casa *e* que há o bastante para o que ele precisa gastar.
        self.chemical_handler
            .recipe_input(self.recipe.chemical_input(), self.recipe.chemical_amount())
            .is_some_and(|needed| chemical.amount >= u64::from(needed.amount))
    }

    fn use_resources(&mut self, operations: i32, transaction: &Transaction) -> bool {
        // Deveria haver o bastante: `calculateOperationsThisTick` já reduziu as operações pelas quantidades.
        let to_use = operations.wrapping_mul(self.chemical_usage_multiplier);
        if !self
            .chemical_handler
            .use_input(self.recipe_chemical.as_ref(), to_use, transaction)
        {
            return false;
        }
        if to_use > 0 {
            self.chemical_used_so_far = self.chemical_used_so_far.wrapping_add(to_use);
            (self.chemical_used_so_far_changed)(self.chemical_used_so_far);
        }
        true
    }

    fn reset_cache(&mut self) {
        self.chemical_used_so_far = 0;
        (self.chemical_used_so_far_changed)(self.chemical_used_so_far);
    }

    fn finish_processing(&mut self, operations: i32, transaction: &Transaction) -> bool {
        self.item_handler
            .use_input(self.recipe_item.as_ref(), operations, transaction)
            // Com `chemicalUsageMultiplier` zero este `use` devolve `true`.
            && self.chemical_handler.use_input(
                self.recipe_chemical.as_ref(),
                operations.wrapping_mul(self.chemical_usage_multiplier),
                transaction,
            )
            && self.output.as_ref().is_some_and(|output| {
                self.output_handler
                    .handle_output(output, operations, transaction)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cached_recipe::CachedRecipe;
    use crate::chemical::{ChemicalOutput, ChemicalOutputHandler, ChemicalTank};
    use crate::containers::{EnergyContainer, InventorySlot};
    use crate::operation_tracker::RecipeError;
    use crate::recipe_handlers::ItemOutputHandler;
    use std::cell::Cell;

    fn use_now(base_total: i32, ticks_required: i32, used: i32, ticks: i32) -> i32 {
        ChemicalUsageMultiplier::constant_use(move || base_total, move || ticks_required)
            .to_use(used, ticks)
    }

    #[test]
    fn the_usage_spreads_what_is_left_over_the_ticks_that_are_left() {
        // 200 em 200 ticks: um por tick, do primeiro ao último, e o `==` do Java devolve 1 sem dividir.
        assert_eq!(use_now(200, 200, 0, 0), 1);
        assert_eq!(use_now(200, 200, 137, 137), 1);
        assert_eq!(use_now(200, 200, 199, 199), 1);
        // 200 em 63 ticks (velocidade 4): 3 por tick, e o último tick paga o resto.
        assert_eq!(use_now(200, 63, 0, 0), 3);
        assert_eq!(use_now(200, 63, 186, 62), 14);
        // O total mais que dobra o tempo: dá 2 por tick, truncado.
        assert_eq!(use_now(10, 4, 0, 0), 2);
    }

    #[test]
    fn a_machine_that_already_used_too_much_uses_nothing() {
        // Gastou mais do que precisaria para os ticks que faltam (tirou velocidade ou pôs upgrade de gás).
        assert_eq!(use_now(200, 200, 150, 10), 0);
        // O tempo caiu abaixo dos ticks já feitos: o `double` dá negativo, e o `Math.max` zera.
        assert_eq!(use_now(200, 63, 110, 110), 0);
    }

    #[test]
    fn no_ticks_left_with_something_to_spend_saturates_like_the_double_division() {
        // `baseRemaining / (double) 0` é infinito, e `clampToInt` o prende no maior `int`.
        assert_eq!(use_now(200, 63, 100, 63), i32::MAX);
        // E sem nada a gastar nem ticks, o `==` devolve 1.
        assert_eq!(use_now(200, 63, 200, 63), 1);
        assert_eq!(clamp_to_int(f64::NAN), 0);
        assert_eq!(clamp_to_int(-1e12), i32::MIN);
    }

    /// Uma máquina mínima: o que o tile liga na `CachedRecipe`, sem tile.
    struct Rig {
        recipe: CachedRecipe,
        input: InventorySlot,
        output: InventorySlot,
        tank: ChemicalTank,
        energy: EnergyContainer,
        used: Rc<Cell<i32>>,
        changes: Rc<Cell<u32>>,
        ticks_required: Rc<Cell<i32>>,
    }

    fn item(id: &str) -> ItemResource {
        ItemResource::new(id, 64)
    }

    fn osmium() -> ChemicalResource {
        ChemicalResource::new("osmium")
    }

    fn rig(base_total: i32, ticks_required: i32) -> Rig {
        let item_recipe = Rc::new(ItemChemicalToItemRecipe {
            id: "compress".to_owned(),
            item_input: ItemIngredient::item("dust"),
            item_count: 1,
            chemical_input: ChemicalIngredient::chemical("osmium"),
            chemical_amount: 1,
            output: ItemOutput {
                resource: item("ingot"),
                count: 1,
            },
        });
        let input = InventorySlot::new(None);
        let output = InventorySlot::output(None, None);
        let tank = ChemicalTank::open(1_000, None);
        let energy = EnergyContainer::input(10_000, 10, None);
        energy.set_energy(10_000, None);
        let used = Rc::new(Cell::new(0));
        let changes = Rc::new(Cell::new(0));
        let ticks = Rc::new(Cell::new(ticks_required));
        let usage = {
            let ticks = ticks.clone();
            ChemicalUsageMultiplier::constant_use(move || base_total, move || ticks.get())
        };
        let logic = ItemChemicalConstantUsageRecipe::new(
            item_recipe,
            ItemInputHandler::new(input.clone(), RecipeError::NOT_ENOUGH_INPUT),
            ChemicalInputHandler::new(tank.clone(), RecipeError::NOT_ENOUGH_SECONDARY_INPUT),
            usage,
            {
                let (used, changes) = (used.clone(), changes.clone());
                move |value| {
                    used.set(value);
                    changes.set(changes.get() + 1);
                }
            },
            ItemOutputHandler::new(output.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
        );
        let recipe = CachedRecipe::new(Box::new(logic), Box::new(|| false))
            .set_energy_requirements(
                {
                    let energy = energy.clone();
                    move || energy.energy_per_tick()
                },
                energy.clone(),
            )
            .set_required_ticks({
                let ticks = ticks.clone();
                move || ticks.get()
            });
        Rig {
            recipe,
            input,
            output,
            tank,
            energy,
            used,
            changes,
            ticks_required: ticks,
        }
    }

    #[test]
    fn the_chemical_goes_a_little_at_a_time_and_the_total_is_the_base_usage() {
        let mut rig = rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 2, None);
        rig.tank.set_contents(Some(osmium()), 100, None);
        for tick in 1..=19 {
            rig.recipe.process();
            assert_eq!(rig.tank.amount(), 100 - tick, "tick {tick}");
            assert_eq!(rig.used.get(), tick as i32);
            assert_eq!(rig.output.amount(), 0);
        }
        // O último tick gasta o resto e termina: o gasto volta a zero e o item sai.
        rig.recipe.process();
        assert_eq!(rig.tank.amount(), 80);
        assert_eq!(rig.used.get(), 0);
        assert_eq!((rig.input.amount(), rig.output.amount()), (1, 1));
        assert_eq!(rig.recipe.operating_ticks(), 0);
    }

    #[test]
    fn without_the_chemical_it_pauses_and_neither_resets_the_progress_nor_flags_an_error() {
        let mut rig = rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 5, None);
        for _ in 0..5 {
            rig.recipe.process();
        }
        assert_eq!(rig.recipe.operating_ticks(), 5);
        assert!(rig.tank.is_empty());
        for _ in 0..10 {
            rig.recipe.process();
        }
        assert_eq!(rig.recipe.operating_ticks(), 5, "pausou, não voltou a zero");
        assert_eq!(rig.used.get(), 5);
        assert!(
            rig.recipe.errors().is_empty(),
            "uso constante não marca erro"
        );
        // Voltou o químico: segue de onde parou.
        rig.tank.set_contents(Some(osmium()), 100, None);
        rig.recipe.process();
        assert_eq!(rig.recipe.operating_ticks(), 6);
        assert_eq!(rig.used.get(), 6);
    }

    #[test]
    fn a_missing_item_is_a_mismatch_that_resets_the_progress_and_the_spent_chemical() {
        let mut rig = rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 100, None);
        for _ in 0..4 {
            rig.recipe.process();
        }
        assert_eq!((rig.recipe.operating_ticks(), rig.used.get()), (4, 4));
        rig.input.set_contents(None, 0, None);
        rig.recipe.process();
        assert_eq!((rig.recipe.operating_ticks(), rig.used.get()), (0, 0));
        // O reset avisa o tile do `usedSoFar` zerado, como o `resetCache` do Java.
        assert_eq!(rig.changes.get(), 5);
    }

    #[test]
    fn a_speed_upgrade_that_cuts_the_ticks_below_the_ones_done_finishes_with_what_was_spent() {
        let mut rig = rig(200, 200);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 500, None);
        for _ in 0..110 {
            rig.recipe.process();
        }
        assert_eq!(rig.used.get(), 110);
        rig.ticks_required.set(63);
        rig.recipe.process();
        // 110 ticks feitos, 63 exigidos: o uso é zero e a operação termina sem gastar o resto.
        assert_eq!(rig.output.amount(), 1);
        assert_eq!(rig.tank.amount(), 500 - 110);
        assert_eq!(rig.used.get(), 0);
    }

    #[test]
    fn a_larger_usage_per_tick_reduces_the_operations_by_what_the_tank_holds() {
        let mut rig = rig(200, 63);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 2, None);
        // 3 por tick e só 2 no tanque: nenhuma operação cabe, e a receita pausa sem gastar.
        rig.recipe.process();
        assert_eq!(rig.tank.amount(), 2);
        assert_eq!(rig.recipe.operating_ticks(), 0);
        rig.tank.set_contents(Some(osmium()), 10, None);
        rig.recipe.process();
        assert_eq!(rig.tank.amount(), 7);
        assert_eq!(rig.used.get(), 3);
    }

    #[test]
    fn energy_is_spent_each_tick_and_a_short_tank_keeps_the_energy() {
        let mut rig = rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 100, None);
        rig.recipe.process();
        assert_eq!(rig.energy.amount(), 10_000 - 10);
        rig.tank.set_contents(None, 0, None);
        rig.recipe.process();
        assert_eq!(rig.energy.amount(), 10_000 - 10, "pausada, não gasta");
    }

    #[test]
    fn the_saved_usage_is_loaded_only_when_positive() {
        let mut rig = rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 100, None);
        rig.recipe.load_saved_usage_so_far(0);
        rig.recipe.load_saved_usage_so_far(-3);
        rig.recipe.process();
        assert_eq!(rig.used.get(), 1);
        let mut rig = self::rig(20, 20);
        rig.input.set_contents(Some(item("dust")), 1, None);
        rig.tank.set_contents(Some(osmium()), 100, None);
        rig.recipe.load_saved_usage_so_far(7);
        rig.recipe.load_saved_operating_ticks(7);
        rig.recipe.process();
        // 7 gastos em 7 ticks: o ritmo segue em um por tick, a partir do que foi salvo.
        assert_eq!((rig.used.get(), rig.recipe.operating_ticks()), (8, 8));
        assert_eq!(rig.tank.amount(), 99);
    }

    #[test]
    fn an_input_is_valid_only_with_the_item_and_enough_chemical_for_the_recipe() {
        let rig = rig(20, 20);
        assert!(!rig.recipe.is_input_valid());
        rig.input.set_contents(Some(item("dust")), 1, None);
        assert!(!rig.recipe.is_input_valid());
        rig.tank.set_contents(Some(osmium()), 1, None);
        assert!(rig.recipe.is_input_valid());
        rig.tank
            .set_contents(Some(ChemicalResource::new("carbon")), 5, None);
        assert!(!rig.recipe.is_input_valid());
    }

    /// A receita serve a qualquer saída: aqui o item vira químico num tanque, com o `ChemicalOutputHandler`.
    #[test]
    fn the_output_can_be_a_chemical_in_a_tank() {
        struct Dissolve(ChemicalIngredient, ItemIngredient);
        impl ItemChemicalToObjectRecipe for Dissolve {
            type Output = ChemicalOutput;
            fn item_input(&self) -> &ItemIngredient {
                &self.1
            }
            fn item_count(&self) -> u32 {
                1
            }
            fn chemical_input(&self) -> &ChemicalIngredient {
                &self.0
            }
            fn chemical_amount(&self) -> u32 {
                1
            }
            fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
                !item.is_empty() && !chemical.is_empty()
            }
            fn output(
                &self,
                _item: &RecipeStack<ItemResource>,
                _chemical: &RecipeStack<ChemicalResource>,
            ) -> ChemicalOutput {
                ChemicalOutput {
                    resource: ChemicalResource::new("slurry"),
                    amount: 10,
                }
            }
        }
        let input = InventorySlot::new(None);
        input.set_contents(Some(item("ore")), 1, None);
        let acid = ChemicalTank::open(1_000, None);
        acid.set_contents(Some(ChemicalResource::new("acid")), 100, None);
        let slurry = ChemicalTank::open(25, None);
        let energy = EnergyContainer::input(1_000, 1, None);
        energy.set_energy(1_000, None);
        let logic = ItemChemicalConstantUsageRecipe::new(
            Rc::new(Dissolve(
                ChemicalIngredient::chemical("acid"),
                ItemIngredient::item("ore"),
            )),
            ItemInputHandler::new(input.clone(), RecipeError::NOT_ENOUGH_INPUT),
            ChemicalInputHandler::new(acid.clone(), RecipeError::NOT_ENOUGH_SECONDARY_INPUT),
            ChemicalUsageMultiplier::new(|_, _| 1),
            |_| {},
            ChemicalOutputHandler::new(slurry.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
        );
        let mut recipe = CachedRecipe::new(Box::new(logic), Box::new(|| false))
            .set_energy_requirements(|| 1, energy);
        // Uma operação por tick e um tick por operação: gasta 1 de ácido e produz 10 de lama, até a lama não caber.
        recipe.process();
        assert_eq!((acid.amount(), slurry.amount()), (99, 10));
        input.set_contents(Some(item("ore")), 1, None);
        recipe.process();
        assert_eq!((acid.amount(), slurry.amount()), (98, 20));
        input.set_contents(Some(item("ore")), 1, None);
        recipe.process();
        assert_eq!(
            (acid.amount(), slurry.amount()),
            (98, 20),
            "25 não comporta mais 10"
        );
        assert!(
            recipe
                .errors()
                .contains(RecipeError::NOT_ENOUGH_OUTPUT_SPACE)
        );
    }
}
