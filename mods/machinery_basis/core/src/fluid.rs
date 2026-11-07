//! O lado de fluido de `BasicFluidTank` (um `BasicResourceContainer<FluidResource>`, em `resource_tank`) e dos handlers de
//! receita que `InputHelper.getInputHandler(IFluidTank, ..)` e `OutputHelper.getOutputHandler(IFluidTank, ..)` devolvem.
//!
//! A lib não conhece nenhum fluido: quem usa dá o id e as regras como dados. O `FluidResource` do NeoForge também carrega
//! `DataComponentPatch`; aqui ele é só o id, como o `ChemicalResource`.
use crate::containers::Access;
use crate::recipe_cache::{ShapedIngredient, TypedInstance};
use crate::recipe_handlers::{ResourceOutput, TankInputHandler, TankOutputHandler};
use crate::resource_tank::{Ingredient, ResourceTank, Rule, StackContents, Validator};
use std::rc::Rc;

/// Um fluido, pelo id. Vazio é a ausência dele (`FluidResource.EMPTY`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FluidResource {
    pub id: Rc<str>,
}

impl FluidResource {
    pub fn new(id: &str) -> Self {
        Self { id: Rc::from(id) }
    }
}

/// `LargeResourceStack<FluidResource>`.
pub type FluidContents = StackContents<FluidResource>;
/// `BiPredicate<FluidResource, AutomationType>`.
pub type FluidRule = Rule<FluidResource>;
/// `Predicate<FluidResource>`.
pub type FluidValidator = Validator<FluidResource>;
/// `BasicFluidTank`.
pub type FluidTank = ResourceTank<FluidResource>;

/// Os predicados de `AutomationType` que o Mekanism passa aos tanques de fluido.
pub fn fluid_access_rule(access: Access) -> FluidRule {
    Rc::new(move |_, automation| access.allows(automation))
}

/// Um fluido que casa com o ingrediente de uma receita.
pub type FluidIngredient = Ingredient<FluidResource>;

impl TypedInstance for FluidResource {
    fn type_id(&self) -> &str {
        &self.id
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl FluidIngredient {
    /// Um fluido só, pelo id.
    pub fn fluid(id: &str) -> Self {
        Self::shaped(ShapedIngredient::of_type(id))
    }
}

/// `InputHelper.getInputHandler(IFluidTank, RecipeError)`.
pub type FluidInputHandler = TankInputHandler<FluidResource>;

/// `FluidStackTemplate`: o que uma receita entrega a um tanque de fluido.
pub type FluidOutput = ResourceOutput<FluidResource>;

/// `OutputHelper.getOutputHandler(IFluidTank, RecipeError)`.
pub type FluidOutputHandler = TankOutputHandler<FluidResource>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::AutomationType;
    use crate::operation_tracker::{OperationTracker, RecipeError, RecipeErrors};
    use crate::recipe_handlers::OutputHandler;
    use crate::transaction::Transaction;

    fn water() -> FluidResource {
        FluidResource::new("water")
    }

    fn tracker() -> OperationTracker {
        OperationTracker::new(RecipeErrors::EMPTY, true, 100)
    }

    #[test]
    fn a_fluid_tank_holds_one_fluid_up_to_its_capacity() {
        let tank = FluidTank::open(1_000, None);
        let root = Transaction::open_root();
        assert_eq!(
            tank.insert(&water(), 700, &root, AutomationType::Internal),
            700
        );
        assert_eq!(
            tank.insert(&water(), 700, &root, AutomationType::Internal),
            300
        );
        assert_eq!(
            tank.insert(
                &FluidResource::new("lava"),
                1,
                &root,
                AutomationType::Internal
            ),
            0
        );
        assert_eq!(
            tank.extract(&water(), 50, &root, AutomationType::Internal),
            50
        );
        assert_eq!(tank.amount(), 950);
    }

    #[test]
    fn the_input_handler_gives_the_recipe_amount_and_keeps_the_tank_when_it_is_short() {
        let tank = FluidTank::open(1_000, None);
        tank.set_contents(Some(water()), 25, None);
        let handler = FluidInputHandler::new(tank.clone(), RecipeError::NOT_ENOUGH_INPUT);
        assert!(
            handler
                .recipe_input(&FluidIngredient::fluid("lava"), 10)
                .is_none()
        );
        assert!(
            handler
                .recipe_input(&FluidIngredient::fluid("water"), 30)
                .is_none()
        );
        let input = handler
            .recipe_input(&FluidIngredient::fluid("water"), 10)
            .unwrap();
        let root = Transaction::open_root();
        let mut tracker = tracker();
        handler.calculate_operations_can_support(&mut tracker, &root, &input, false, 1);
        assert_eq!(tracker.current_max(), 2, "25 de água, 10 por operação");
        assert!(handler.use_input(Some(&input), 2, &root));
        assert_eq!(tank.amount(), 5);
        assert!(!handler.use_input(Some(&input), 1, &root), "só restam 5");
        assert!(!handler.use_input(None, 0, &root));
    }

    #[test]
    fn a_fluid_input_that_is_not_enough_resets_the_progress_unless_the_use_is_constant() {
        let tank = FluidTank::open(1_000, None);
        tank.set_contents(Some(water()), 5, None);
        let handler = FluidInputHandler::new(tank, RecipeError::NOT_ENOUGH_SECONDARY_INPUT);
        let input = crate::recipe_handlers::RecipeStack {
            resource: water(),
            amount: 10,
        };
        let root = Transaction::open_root();
        let mut tracker = tracker();
        handler.calculate_operations_can_support(&mut tracker, &root, &input, false, 1);
        assert!(
            tracker
                .errors()
                .contains(RecipeError::NOT_ENOUGH_SECONDARY_INPUT)
        );
        assert!(tracker.current_max() < 0);
        let mut constant = self::tracker();
        handler.calculate_operations_can_support(&mut constant, &root, &input, true, 1);
        assert_eq!(constant.current_max(), 0);
        assert!(constant.errors().is_empty());
        // Sem nada a gastar (`usageMultiplier` zero), nem calcula.
        let mut idle = self::tracker();
        handler.calculate_operations_can_support(&mut idle, &root, &input, false, 0);
        assert_eq!(idle.current_max(), 100);
    }

    #[test]
    fn the_output_handler_fills_the_fluid_tank_by_operations() {
        let tank = FluidTank::open(100, None);
        tank.set_contents(Some(water()), 70, None);
        let handler = FluidOutputHandler::new(tank.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE);
        let output = FluidOutput {
            resource: water(),
            amount: 10,
        };
        let root = Transaction::open_root();
        let mut tracker = tracker();
        handler.calculate_operations_can_support(&mut tracker, &root, &output);
        assert_eq!(tracker.current_max(), 3);
        assert!(handler.handle_output(&output, 3, &root));
        assert_eq!(tank.amount(), 100);
        assert!(!handler.handle_output(&output, 1, &root));
        let mut full = self::tracker();
        handler.calculate_operations_can_support(&mut full, &root, &output);
        assert_eq!(full.current_max(), 0);
        assert!(full.errors().contains(RecipeError::NOT_ENOUGH_OUTPUT_SPACE));
    }
}
