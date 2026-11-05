//! Porte de `RecipeCacheLookupMonitor`, `ProgressRecipeCacheLookupMonitor` e `getUpdatedCache`.
use crate::cached_recipe::{CachedRecipe, CachedRecipeHolder, RecipeLogic};
use std::collections::BTreeMap;

/// Prefixo das receitas sintéticas que embrulham uma receita do jogo.
const GENERATED_PREFIX: &str = "/mekanism_generated/";
const SYNTHETIC_PREFIX: char = '/';

/// Quem acha e cria as receitas do monitor.
pub trait RecipeLookupHandler: CachedRecipeHolder {
    type Recipe;
    type Logic: RecipeLogic<Self>
    where
        Self: Sized;

    /// `getRecipe`: ignora o que está em cache, e devolve o id junto.
    fn find_recipe(&self) -> Option<(String, Self::Recipe)>;
    fn create_new_cached_recipe(&mut self, recipe: Self::Recipe) -> Self::Logic
    where
        Self: Sized;
    fn saved_operating_ticks(&self) -> i32 {
        0
    }
    /// `invalidateCache`: as tags ou as receitas recarregaram.
    fn invalidate_cache(&self) -> bool {
        false
    }
    /// `clearRecipeErrors`.
    fn on_cached_recipe_changed(&mut self) {}
}

#[derive(Clone, Debug)]
pub struct RecipeCacheLookupMonitor<L> {
    cached_recipe: Option<CachedRecipe<L>>,
    recipe_id: Option<String>,
    has_no_recipe: bool,
    should_unpause: bool,
    has_item_output: bool,
    recipes_used: BTreeMap<String, i32>,
    last_recipe_used: Option<String>,
    last_recipe_crafted_amount: i32,
    reset_last_recipe: bool,
}

impl<L> RecipeCacheLookupMonitor<L> {
    pub fn new(has_item_output: bool) -> Self {
        Self {
            cached_recipe: None,
            recipe_id: None,
            has_no_recipe: false,
            should_unpause: false,
            has_item_output,
            recipes_used: BTreeMap::new(),
            last_recipe_used: None,
            last_recipe_crafted_amount: 0,
            reset_last_recipe: false,
        }
    }

    pub fn cached_recipe(&self) -> Option<&CachedRecipe<L>> {
        self.cached_recipe.as_ref()
    }

    pub fn recipes_used(&self) -> &BTreeMap<String, i32> {
        &self.recipes_used
    }

    /// `onChange`: pode haver receita de novo.
    pub fn on_change(&mut self) {
        self.has_no_recipe = false;
        self.unpause();
    }

    pub fn unpause(&mut self) {
        self.should_unpause = true;
    }

    pub fn output_decreased(&mut self) {
        self.reset_last_recipe = true;
    }

    /// `setRecipeUsed`, com as receitas sintéticas filtradas como no Java.
    pub fn set_recipe_used(&mut self, operations: i32) {
        let Some(id) = self.recipe_id.clone() else {
            return;
        };
        let (namespace, path) = id.split_once(':').unwrap_or(("", &id));
        let id = if let Some(path) = path.strip_prefix(GENERATED_PREFIX) {
            format!("{namespace}:{path}")
        } else if path.starts_with(SYNTHETIC_PREFIX) {
            return;
        } else {
            id.clone()
        };
        *self.recipes_used.entry(id.clone()).or_default() += operations;
        if self.has_item_output {
            if !self.reset_last_recipe && self.last_recipe_used.as_ref() == Some(&id) {
                self.last_recipe_crafted_amount += operations;
            } else {
                self.last_recipe_used = Some(id);
                self.last_recipe_crafted_amount = operations;
                self.reset_last_recipe = false;
            }
        }
    }

    /// `getUpdatedCache`. Devolve se o cache mudou.
    fn updated_cache<H>(&mut self, holder: &mut H) -> bool
    where
        H: RecipeLookupHandler<Logic = L>,
        L: RecipeLogic<H>,
    {
        let cache_invalid = holder.invalidate_cache();
        let mut changed = false;
        if cache_invalid {
            changed = self.cached_recipe.take().is_some();
        }
        let valid = self
            .cached_recipe
            .as_ref()
            .is_some_and(|cached| cached.is_input_valid(holder));
        if valid || !(cache_invalid || !self.has_no_recipe) {
            return changed;
        }
        let Some((id, recipe)) = holder.find_recipe() else {
            self.has_no_recipe = true;
            return changed;
        };
        let mut cached = CachedRecipe::new(holder.create_new_cached_recipe(recipe));
        if self.cached_recipe.is_none() {
            cached.load_saved_operating_ticks(holder, holder.saved_operating_ticks());
        }
        self.cached_recipe = Some(cached);
        self.recipe_id = Some(id);
        true
    }

    /// `updateAndProcess`. Devolve se havia receita para rodar.
    pub fn update_and_process<H>(&mut self, holder: &mut H) -> bool
    where
        H: RecipeLookupHandler<Logic = L>,
        L: RecipeLogic<H>,
    {
        if self.updated_cache(holder) {
            holder.on_cached_recipe_changed();
        }
        let Some(cached) = &mut self.cached_recipe else {
            return false;
        };
        if std::mem::take(&mut self.should_unpause) {
            cached.unpause_errors();
        }
        cached.process(holder);
        true
    }
}
