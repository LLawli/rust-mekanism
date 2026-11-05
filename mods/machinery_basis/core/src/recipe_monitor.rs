//! Porte de `RecipeCacheLookupMonitor` e `getUpdatedCache`. O monitor é também o `IContentsListener` dos slots, e por isso
//! o que os listeners tocam fica em [`MonitorFlags`], compartilhado: um aviso no commit da raiz nunca pede emprestado o
//! monitor que está processando.
use crate::cached_recipe::{CachedRecipe, ItemToItemRecipe};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// Prefixo das receitas sintéticas que embrulham uma receita do jogo.
const GENERATED_PREFIX: &str = "/mekanism_generated/";
const SYNTHETIC_PREFIX: char = '/';

/// O que os listeners do monitor mexem.
#[derive(Debug, Default)]
pub struct MonitorFlags {
    has_no_recipe: Cell<bool>,
    should_unpause: Cell<bool>,
    reset_last_recipe: Cell<bool>,
}

impl MonitorFlags {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// `onChange`: pode haver receita de novo.
    pub fn on_change(&self) {
        self.has_no_recipe.set(false);
        self.unpause();
    }

    pub fn unpause(&self) {
        self.should_unpause.set(true);
    }

    pub fn output_decreased(&self) {
        self.reset_last_recipe.set(true);
    }
}

/// Quem acha e cria as receitas do monitor: o `IRecipeLookupHandler` do tile.
pub trait RecipeLookupHandler {
    /// `getRecipe`: ignora o que está em cache.
    fn get_recipe(&self) -> Option<Rc<ItemToItemRecipe>>;
    /// `createNewCachedRecipe`.
    fn create_new_cached_recipe(&self, recipe: Rc<ItemToItemRecipe>) -> CachedRecipe;
    fn saved_operating_ticks(&self) -> i32 {
        0
    }
    /// `invalidateCache`: as tags ou as receitas recarregaram.
    fn invalidate_cache(&self) -> bool {
        false
    }
    /// `onCachedRecipeChanged`: `clearRecipeErrors`.
    fn on_cached_recipe_changed(&self) {}
    /// `markForSave`.
    fn mark_for_save(&self) {}
}

#[derive(Debug, Default)]
struct UsedRecipes {
    recipes_used: BTreeMap<String, i32>,
    last_recipe_used: Option<String>,
    last_recipe_crafted_amount: i32,
}

impl UsedRecipes {
    /// `setRecipeUsed`, com as receitas sintéticas filtradas como no Java.
    fn set_recipe_used(
        &mut self,
        id: &str,
        operations: i32,
        has_item_output: bool,
        flags: &MonitorFlags,
    ) {
        let (namespace, path) = id.split_once(':').unwrap_or(("", id));
        let id = if let Some(path) = path.strip_prefix(GENERATED_PREFIX) {
            format!("{namespace}:{path}")
        } else if path.starts_with(SYNTHETIC_PREFIX) {
            return;
        } else {
            id.to_owned()
        };
        *self.recipes_used.entry(id.clone()).or_default() += operations;
        if has_item_output {
            if !flags.reset_last_recipe.get() && self.last_recipe_used.as_ref() == Some(&id) {
                self.last_recipe_crafted_amount += operations;
            } else {
                self.last_recipe_used = Some(id);
                self.last_recipe_crafted_amount = operations;
                flags.reset_last_recipe.set(false);
            }
        }
    }
}

pub struct RecipeCacheLookupMonitor {
    handler: Rc<dyn RecipeLookupHandler>,
    flags: Rc<MonitorFlags>,
    used: Rc<RefCell<UsedRecipes>>,
    has_item_output: bool,
    cached_recipe: Option<CachedRecipe>,
}

impl RecipeCacheLookupMonitor {
    pub fn new(
        handler: Rc<dyn RecipeLookupHandler>,
        flags: Rc<MonitorFlags>,
        has_item_output: bool,
    ) -> Self {
        Self {
            handler,
            flags,
            used: Rc::default(),
            has_item_output,
            cached_recipe: None,
        }
    }

    pub fn flags(&self) -> &Rc<MonitorFlags> {
        &self.flags
    }

    pub fn cached_recipe(&self) -> Option<&CachedRecipe> {
        self.cached_recipe.as_ref()
    }

    /// `getRecipesUsed`.
    pub fn recipes_used(&self) -> BTreeMap<String, i32> {
        self.used.borrow().recipes_used.clone()
    }

    /// `getUpdatedCache`. Devolve se o cache mudou, que é quando o Java compara a referência.
    fn updated_cache(&mut self) -> bool {
        let cache_invalid = self.handler.invalidate_cache();
        let mut changed = false;
        if cache_invalid {
            changed = self.cached_recipe.take().is_some();
        }
        let valid = self
            .cached_recipe
            .as_ref()
            .is_some_and(CachedRecipe::is_input_valid);
        if valid || !(cache_invalid || !self.flags.has_no_recipe.get()) {
            return changed;
        }
        let Some(recipe) = self.handler.get_recipe() else {
            self.flags.has_no_recipe.set(true);
            return changed;
        };
        let id = recipe.id.clone();
        let handler = self.handler.clone();
        let used = self.used.clone();
        let flags = self.flags.clone();
        let has_item_output = self.has_item_output;
        let mut cached =
            self.handler
                .create_new_cached_recipe(recipe)
                .set_on_finish(move |operations| {
                    handler.mark_for_save();
                    used.borrow_mut()
                        .set_recipe_used(&id, operations, has_item_output, &flags);
                });
        if self.cached_recipe.is_none() {
            cached.load_saved_operating_ticks(self.handler.saved_operating_ticks());
        }
        self.cached_recipe = Some(cached);
        true
    }

    /// `updateAndProcess`. Devolve se havia receita para rodar.
    pub fn update_and_process(&mut self) -> bool {
        if self.updated_cache() {
            self.handler.on_cached_recipe_changed();
        }
        let Some(cached) = &mut self.cached_recipe else {
            return false;
        };
        if self.flags.should_unpause.take() {
            cached.unpause_errors();
        }
        cached.process();
        true
    }
}
