//! Porte de `RecipeCacheLookupMonitor` e `getUpdatedCache`. O monitor é também o `IContentsListener` dos slots, e por isso
//! o que os listeners tocam fica em [`MonitorFlags`], compartilhado: um aviso no commit da raiz nunca pede emprestado o
//! monitor que está processando.
use crate::cached_recipe::CachedRecipe;
use crate::experience::ExperienceDrop;
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

    /// `resetLastRecipe`.
    pub fn reset_last_recipe(&self) -> bool {
        self.reset_last_recipe.get()
    }
}

/// O que o monitor precisa saber de uma receita: o id que ele conta em `recipesUsed`. O resto é do tile.
pub trait RecipeId {
    fn recipe_id(&self) -> &str;
}

/// Quem acha e cria as receitas do monitor: o `IRecipeLookupHandler` do tile, para as receitas `R` dele.
pub trait RecipeLookupHandler<R> {
    /// `getRecipe`: ignora o que está em cache.
    fn get_recipe(&self) -> Option<Rc<R>>;
    /// `createNewCachedRecipe`.
    fn create_new_cached_recipe(&self, recipe: Rc<R>) -> CachedRecipe;
    fn saved_operating_ticks(&self) -> i32 {
        0
    }
    /// `ConstantUsageRecipeLookupHandler.getSavedUsedSoFar`: `None` se o tile não é um `ConstantUsageRecipeLookupHandler`.
    fn saved_used_so_far(&self) -> Option<i32> {
        None
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
    /// `hadContainment`: se o upgrade de contenção de xp estava instalado na última vez que o monitor soube.
    had_containment: bool,
    /// Sobe a cada mudança, para quem salva saber sem comparar o mapa.
    version: u32,
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
        self.version = self.version.wrapping_add(1);
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

/// O que o Java chama de `awardUsedRecipesAndPopExperience`: as receitas a premiar (as que ainda existem) e a experiência a soltar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Awarded {
    pub recipes: Vec<String>,
    pub drops: Vec<ExperienceDrop>,
}

/// O estado de `recipesUsed`, da última receita e de `hadContainment`, que o tile alcança sem passar pelo monitor: o
/// `recalculateUpgrades` roda dentro do componente de upgrades.
#[derive(Clone, Debug, Default)]
pub struct UsedHandle(Rc<RefCell<UsedRecipes>>);

impl UsedHandle {
    pub fn had_containment(&self) -> bool {
        self.0.borrow().had_containment
    }

    /// `loadAdditional`: `HAS_CONTAINMENT`.
    pub fn set_had_containment(&self, had_containment: bool) {
        self.0.borrow_mut().had_containment = had_containment;
    }

    /// A última receita e quantas operações dela saíram em sequência.
    pub fn last_recipe(&self) -> (Option<String>, i32) {
        let used = self.0.borrow();
        (
            used.last_recipe_used.clone(),
            used.last_recipe_crafted_amount,
        )
    }

    /// `getRecipesToAwardAndPopExperience(level, position, recipesUsed, dropXp)`: as receitas que ainda existem
    /// (`experience` devolve `None` para as que sumiram) e, com `drop_xp`, a experiência de cada uma.
    fn recipes_to_award(
        used: &UsedRecipes,
        drop_xp: bool,
        experience: &dyn Fn(&str) -> Option<f32>,
    ) -> Awarded {
        let mut awarded = Awarded::default();
        for (recipe, crafted) in &used.recipes_used {
            let Some(experience) = experience(recipe) else {
                continue;
            };
            awarded.recipes.push(recipe.clone());
            if drop_xp {
                awarded.drops.push(ExperienceDrop {
                    recipe: recipe.clone(),
                    crafted: *crafted,
                    experience,
                });
            }
        }
        awarded
    }

    /// `getRecipesToAwardAndPopExperience(level, position)`: o que o tile solta ao ser quebrado. `drop_xp` é
    /// `hasXpContainment()` na hora da chamada.
    pub fn recipes_to_award_and_pop(
        &self,
        drop_xp: bool,
        experience: &dyn Fn(&str) -> Option<f32>,
    ) -> Awarded {
        Self::recipes_to_award(&self.0.borrow(), drop_xp, experience)
    }

    /// `xpContainmentChanged`. `drop_xp` é `hasXpContainment()` na hora: dentro do `recalculateUpgrades`, depois de o upgrade
    /// já ter saído, ele é falso, e a experiência guardada se perde.
    pub fn xp_containment_changed(
        &self,
        flags: &MonitorFlags,
        has_containment: bool,
        drop_xp: bool,
        experience: &dyn Fn(&str) -> Option<f32>,
    ) -> Awarded {
        let mut used = self.0.borrow_mut();
        if has_containment == used.had_containment {
            return Awarded::default();
        }
        let mut awarded = Awarded::default();
        if used.had_containment {
            awarded = Self::recipes_to_award(&used, drop_xp, experience);
        }
        used.recipes_used.clear();
        if !used.had_containment
            && let Some(last) = used.last_recipe_used.clone()
            && used.last_recipe_crafted_amount > 0
        {
            let amount = used.last_recipe_crafted_amount;
            used.recipes_used.insert(last, amount);
        }
        used.last_recipe_used = None;
        used.last_recipe_crafted_amount = 0;
        used.had_containment = has_containment;
        used.version = used.version.wrapping_add(1);
        let _ = flags;
        awarded
    }

    /// `awardUsedRecipesAndPopExperience(player, recipesUsed, dropXp, monitor)` de um monitor com saída de item: o jogador
    /// tirou o resultado, ou apertou a aba de xp. Sem contenção, a última receita dá a experiência dela (como o forno).
    pub fn award_used_recipes_and_pop(
        &self,
        flags: &MonitorFlags,
        has_item_output: bool,
        drop_xp: bool,
        experience: &dyn Fn(&str) -> Option<f32>,
    ) -> Awarded {
        let mut used = self.0.borrow_mut();
        let mut awarded = Self::recipes_to_award(&used, drop_xp, experience);
        if has_item_output && !drop_xp {
            if let Some(last) = used.last_recipe_used.clone()
                && used.last_recipe_crafted_amount > 0
                && let Some(experience) = experience(&last)
            {
                awarded.drops.push(ExperienceDrop {
                    recipe: last,
                    crafted: used.last_recipe_crafted_amount,
                    experience,
                });
            }
            flags.reset_last_recipe.set(false);
            used.last_recipe_used = None;
            used.last_recipe_crafted_amount = 0;
        }
        used.recipes_used.clear();
        used.version = used.version.wrapping_add(1);
        awarded
    }
}

pub struct RecipeCacheLookupMonitor<R> {
    handler: Rc<dyn RecipeLookupHandler<R>>,
    flags: Rc<MonitorFlags>,
    used: UsedHandle,
    has_item_output: bool,
    cached_recipe: Option<CachedRecipe>,
}

impl<R: RecipeId + 'static> RecipeCacheLookupMonitor<R> {
    pub fn new(
        handler: Rc<dyn RecipeLookupHandler<R>>,
        flags: Rc<MonitorFlags>,
        has_item_output: bool,
    ) -> Self {
        Self::sharing(handler, flags, UsedHandle::default(), has_item_output)
    }

    /// O monitor de um tile que também mexe nas receitas usadas por fora dele (o upgrade de contenção de xp).
    pub fn sharing(
        handler: Rc<dyn RecipeLookupHandler<R>>,
        flags: Rc<MonitorFlags>,
        used: UsedHandle,
        has_item_output: bool,
    ) -> Self {
        Self {
            handler,
            flags,
            used,
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

    /// `saveAdditional`: as receitas usadas, a última e quanto dela saiu em sequência.
    pub fn used_snapshot(&self) -> (BTreeMap<String, i32>, Option<String>, i32) {
        let used = self.used.0.borrow();
        (
            used.recipes_used.clone(),
            used.last_recipe_used.clone(),
            used.last_recipe_crafted_amount,
        )
    }

    /// `usedRecipesLoaded`.
    pub fn restore_used(
        &self,
        recipes_used: BTreeMap<String, i32>,
        last_recipe_used: Option<String>,
        last_recipe_amount: i32,
    ) {
        let mut used = self.used.0.borrow_mut();
        used.recipes_used = recipes_used;
        self.flags.reset_last_recipe.set(false);
        used.last_recipe_crafted_amount = if last_recipe_used.is_some() {
            last_recipe_amount
        } else {
            0
        };
        used.last_recipe_used = last_recipe_used;
        used.version = used.version.wrapping_add(1);
    }

    pub fn used_version(&self) -> u32 {
        self.used.0.borrow().version
    }

    /// `getRecipesUsed`.
    pub fn recipes_used(&self) -> BTreeMap<String, i32> {
        self.used.0.borrow().recipes_used.clone()
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
        let id = recipe.recipe_id().to_owned();
        let handler = self.handler.clone();
        let used = self.used.clone();
        let flags = self.flags.clone();
        let has_item_output = self.has_item_output;
        let mut cached =
            self.handler
                .create_new_cached_recipe(recipe)
                .set_on_finish(move |operations| {
                    handler.mark_for_save();
                    used.0
                        .borrow_mut()
                        .set_recipe_used(&id, operations, has_item_output, &flags);
                });
        if self.cached_recipe.is_none() {
            cached.load_saved_operating_ticks(self.handler.saved_operating_ticks());
            // `loadSavedData` do monitor: depois dos ticks, o quanto do químico de uso constante já foi gasto.
            if let Some(used_so_far) = self.handler.saved_used_so_far() {
                cached.load_saved_usage_so_far(used_so_far);
            }
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
