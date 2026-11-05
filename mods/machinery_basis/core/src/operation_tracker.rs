//! Porte de `CachedRecipe.OperationTracker`: quantas operações a receita faz neste tick e o que a impede.

/// Marca de um erro de receita. Um mod cria os seus com [`RecipeError::custom`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeError(u8);

impl RecipeError {
    pub const INPUT_DOESNT_PRODUCE_OUTPUT: Self = Self(0);
    pub const NOT_ENOUGH_ENERGY: Self = Self(1);
    pub const NOT_ENOUGH_ENERGY_REDUCED_RATE: Self = Self(2);
    pub const NOT_ENOUGH_INPUT: Self = Self(3);
    pub const NOT_ENOUGH_SECONDARY_INPUT: Self = Self(4);
    pub const NOT_ENOUGH_LEFT_INPUT: Self = Self(5);
    pub const NOT_ENOUGH_RIGHT_INPUT: Self = Self(6);
    pub const NOT_ENOUGH_OUTPUT_SPACE: Self = Self(7);
    const FIRST_CUSTOM: u8 = 8;

    /// `index` vai de 0 a 23: o conjunto de erros guarda um bit por erro.
    pub const fn custom(index: u8) -> Self {
        assert!(index < u32::BITS as u8 - Self::FIRST_CUSTOM);
        Self(Self::FIRST_CUSTOM + index)
    }

    const fn bit(self) -> u32 {
        1 << self.0
    }
}

/// Conjunto de erros, no lugar do `ObjectArraySet` do Java.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecipeErrors(u32);

impl RecipeErrors {
    pub const EMPTY: Self = Self(0);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub fn contains(self, error: RecipeError) -> bool {
        self.0 & error.bit() != 0
    }

    pub fn contains_all(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Devolve se o erro é novo no conjunto.
    fn add(&mut self, error: RecipeError) -> bool {
        let new = !self.contains(error);
        self.0 |= error.bit();
        new
    }
}

/// Número mágico para quando a receita perde o progresso.
const RESET_PROGRESS: i32 = -1;
/// Número mágico para quando a receita não confere: perde o progresso e limpa os erros.
const MISMATCHED_RECIPE: i32 = -2;

#[derive(Clone, Debug)]
pub struct OperationTracker {
    last_errors: RecipeErrors,
    errors: RecipeErrors,
    check_all: bool,
    /// Começa `true`: `last_errors` sempre contém o conjunto vazio.
    checked_errors: bool,
    current_max: i32,
    pub max_for_energy: i32,
}

impl OperationTracker {
    pub fn new(last_errors: RecipeErrors, check_all: bool, starting_max: i32) -> Self {
        Self {
            last_errors,
            errors: RecipeErrors::EMPTY,
            check_all,
            checked_errors: true,
            current_max: starting_max,
            max_for_energy: starting_max,
        }
    }

    pub fn current_max(&self) -> i32 {
        self.current_max
    }

    pub fn errors(&self) -> RecipeErrors {
        self.errors
    }

    /// Se os erros daqui devem ir para a receita. Com receita que não confere, eles são limpos antes.
    pub fn has_errors_to_copy(&mut self) -> bool {
        if self.current_max == MISMATCHED_RECIPE {
            self.errors = RecipeErrors::EMPTY;
            return true;
        }
        if self.check_all || self.current_max > 0 {
            return true;
        }
        !self.checked_errors && !self.last_errors.contains_all(self.errors)
    }

    pub fn should_continue_checking(&mut self) -> bool {
        if self.current_max > 0 {
            return true;
        }
        if self.current_max == 0 {
            if self.check_all {
                return true;
            }
            if !self.checked_errors {
                if !self.last_errors.contains_all(self.errors) {
                    // Há erro novo: daqui em diante coleta todos.
                    self.check_all = true;
                    return true;
                }
                self.checked_errors = true;
            }
        }
        false
    }

    /// Só reduz o máximo. Devolve se reduziu.
    pub fn update_operations(&mut self, max: i32) -> bool {
        if max < self.current_max {
            self.current_max = max;
            return true;
        }
        false
    }

    pub fn cap_at_max_for_energy(&mut self) -> bool {
        self.update_operations(self.max_for_energy)
    }

    pub fn mismatched_recipe(&mut self) {
        self.update_operations(MISMATCHED_RECIPE);
    }

    pub fn reset_progress(&mut self, error: RecipeError) {
        // Por `update_operations`, para não trocar `MISMATCHED_RECIPE` por `RESET_PROGRESS`.
        self.update_operations(RESET_PROGRESS);
        self.add_error(error);
    }

    pub fn add_error(&mut self, error: RecipeError) {
        if self.errors.add(error) {
            self.checked_errors = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(list: &[RecipeError]) -> RecipeErrors {
        let mut set = RecipeErrors::EMPTY;
        for &error in list {
            set.add(error);
        }
        set
    }

    #[test]
    fn operations_only_go_down() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, false, 4);
        assert!(!tracker.update_operations(9));
        assert!(tracker.update_operations(2));
        assert_eq!(tracker.current_max(), 2);
    }

    #[test]
    fn with_operations_left_it_keeps_checking_and_copies_the_errors() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, false, 1);
        assert!(tracker.should_continue_checking());
        assert!(tracker.has_errors_to_copy());
    }

    #[test]
    fn stopped_with_a_known_error_short_circuits() {
        let last = errors(&[RecipeError::NOT_ENOUGH_ENERGY]);
        let mut tracker = OperationTracker::new(last, false, 1);
        tracker.update_operations(0);
        tracker.add_error(RecipeError::NOT_ENOUGH_ENERGY);
        assert!(!tracker.should_continue_checking());
        assert!(!tracker.has_errors_to_copy());
    }

    #[test]
    fn a_new_error_turns_on_the_full_check() {
        let last = errors(&[RecipeError::NOT_ENOUGH_ENERGY]);
        let mut tracker = OperationTracker::new(last, false, 1);
        tracker.update_operations(0);
        tracker.add_error(RecipeError::NOT_ENOUGH_OUTPUT_SPACE);
        assert!(tracker.should_continue_checking());
        assert!(tracker.should_continue_checking());
        assert!(tracker.has_errors_to_copy());
    }

    #[test]
    fn a_full_check_goes_on_at_zero_operations() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, true, 1);
        tracker.update_operations(0);
        assert!(tracker.should_continue_checking());
    }

    #[test]
    fn a_reset_stops_even_a_full_check() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, true, 1);
        tracker.reset_progress(RecipeError::NOT_ENOUGH_INPUT);
        assert_eq!(tracker.current_max(), RESET_PROGRESS);
        assert!(!tracker.should_continue_checking());
        assert!(tracker.has_errors_to_copy());
        assert!(tracker.errors().contains(RecipeError::NOT_ENOUGH_INPUT));
    }

    #[test]
    fn a_mismatch_survives_a_reset_and_clears_the_errors() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, false, 1);
        tracker.mismatched_recipe();
        tracker.reset_progress(RecipeError::NOT_ENOUGH_INPUT);
        assert_eq!(tracker.current_max(), MISMATCHED_RECIPE);
        assert!(tracker.has_errors_to_copy());
        assert!(tracker.errors().is_empty());
    }

    #[test]
    fn the_energy_cap_lowers_the_maximum() {
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, false, 8);
        tracker.max_for_energy = 3;
        assert!(tracker.cap_at_max_for_energy());
        assert_eq!(tracker.current_max(), 3);
        assert!(!tracker.cap_at_max_for_energy());
    }

    #[test]
    fn a_custom_error_is_its_own() {
        let custom = RecipeError::custom(0);
        let set = errors(&[custom]);
        assert!(set.contains(custom));
        assert!(!set.contains(RecipeError::NOT_ENOUGH_OUTPUT_SPACE));
        assert_eq!(set.len(), 1);
    }
}
