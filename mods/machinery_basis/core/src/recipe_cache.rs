//! Porte de `mekanism.common.recipe.lookup.cache`: o índice de receitas por tipo de entrada que `findFirstRecipe`,
//! `containsInput` e as regras de inserção dos slots consultam.
//!
//! | Rust | Java |
//! |---|---|
//! | [`InputCache`] | `IInputCache` |
//! | [`BaseInputCache`] | `BaseInputCache` |
//! | [`ComponentSensitiveInputCache`] | `ComponentSensitiveInputCache` (e o `mapInputs` de `ItemInputCache` e `FluidInputCache`) |
//! | [`ChemicalInputCache`] | `ChemicalInputCache` |
//! | [`SingleInputRecipeCache`] | `SingleInputRecipeCache` (`InputRecipeCache.SingleItem`, `SingleFluid`, `SingleChemical`) |
//! | [`DoubleInputRecipeCache`] | `DoubleInputRecipeCache` e `AbstractInputRecipeCache.containsInput`/`containsPairing` |
//!
//! A lib não conhece id nenhum: o tipo de uma entrada é um texto que o mod escolhe, e o que o ingrediente aceita chega como
//! [`IngredientShape`], que é a classificação que `ItemInputCache.mapIngredient` faz do ingrediente do jogo (vanilla ou
//! simples, composto, de componentes exatos, ou qualquer outro).
//!
//! `RecipeHolder` é a posição da receita na lista que a fonte devolve: o `Set` de receitas complexas e as listas dos mapas
//! guardam essa posição. O `HashSet` do Java itera em ordem de hash, que não se reproduz (o `hashCode` de uma receita depende
//! do ingrediente); aqui as receitas complexas iteram na ordem da lista.
use crate::chemical::{ChemicalResource, TankContents};
use crate::containers::{ItemResource, SlotContents};
use std::cell::RefCell;
use std::collections::HashMap;
use std::iter::Copied;
use std::marker::PhantomData;
use std::rc::Rc;
use std::slice;

/// Como o `getComponents()` de uma entrada se compara com a chave do mapa de componentes. O Java compara `DataComponentMap`
/// com `equals`, e o `ItemStack` e o `ItemResource` não devolvem a mesma classe de mapa quando não há patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentsKey<'a> {
    /// O `PatchedDataComponentMap` sem patch, o do `ItemStack` com os componentes padrão: igual à chave de um ingrediente
    /// exato com os componentes padrão.
    Default,
    /// Os componentes do tipo com este patch.
    Patch(&'a str),
    /// O mapa do protótipo, sem patch, que o `ItemResource` devolve (`innerStack.immutableComponents()`): não é igual a
    /// nenhuma chave do índice.
    Unmatchable,
}

/// `TypedInstance`: o tipo de uma entrada, que serve de chave nos mapas, e os componentes que a distinguem dentro do tipo.
pub trait TypedInstance {
    /// `typeHolder().value()`.
    fn type_id(&self) -> &str;
    /// `DataComponentHolder.getComponents()`, como chave.
    fn components_key(&self) -> ComponentsKey<'_> {
        ComponentsKey::Default
    }
    /// `stack.isEmpty()` e `resource.isEmpty()`.
    fn is_empty(&self) -> bool;
}

/// Sem entrada (`null`) é vazio.
impl<T: TypedInstance> TypedInstance for Option<T> {
    fn type_id(&self) -> &str {
        self.as_ref().map_or("", T::type_id)
    }

    fn components_key(&self) -> ComponentsKey<'_> {
        self.as_ref()
            .map_or(ComponentsKey::Unmatchable, T::components_key)
    }

    fn is_empty(&self) -> bool {
        self.as_ref().is_none_or(T::is_empty)
    }
}

impl TypedInstance for ItemResource {
    fn type_id(&self) -> &str {
        &self.id
    }

    fn components_key(&self) -> ComponentsKey<'_> {
        if self.components.is_empty() {
            ComponentsKey::Unmatchable
        } else {
            ComponentsKey::Patch(self.components.key())
        }
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl TypedInstance for SlotContents {
    fn type_id(&self) -> &str {
        self.resource.as_ref().map_or("", |resource| &resource.id)
    }

    fn components_key(&self) -> ComponentsKey<'_> {
        match self
            .resource
            .as_ref()
            .filter(|resource| !resource.components.is_empty())
        {
            Some(resource) => ComponentsKey::Patch(resource.components.key()),
            None => ComponentsKey::Default,
        }
    }

    fn is_empty(&self) -> bool {
        self.resource.is_none() || self.amount == 0
    }
}

impl TypedInstance for ChemicalResource {
    fn type_id(&self) -> &str {
        &self.id
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl TypedInstance for TankContents {
    fn type_id(&self) -> &str {
        self.resource.as_ref().map_or("", |resource| &resource.id)
    }

    fn is_empty(&self) -> bool {
        self.resource.is_none() || self.amount == 0
    }
}

/// O que o `mapInputs` de um cache faz com um ingrediente. Quem monta a receita classifica o ingrediente do jogo, como o
/// `ItemInputCache.mapIngredient` faz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngredientShape {
    /// Os tipos que o ingrediente aceita, sem olhar componentes: ingrediente vanilla, `isSimple`, ou a lista de um químico.
    Types(Vec<Rc<str>>),
    /// `DataComponentIngredient` exato: os tipos, e os componentes que a entrada precisa ter, iguais.
    Exact {
        types: Vec<Rc<str>>,
        components: Option<Rc<str>>,
    },
    /// `CompoundIngredient`: cada filho se mapeia à parte.
    Compound(Vec<IngredientShape>),
    /// Qualquer outro: não se indexa, e a receita fica na lista das complexas.
    Custom,
}

impl IngredientShape {
    pub fn types(types: &[&str]) -> Self {
        Self::Types(types.iter().map(|id| Rc::from(*id)).collect())
    }

    pub fn exact(types: &[&str], components: Option<&str>) -> Self {
        Self::Exact {
            types: types.iter().map(|id| Rc::from(*id)).collect(),
            components: components.map(Rc::from),
        }
    }
}

/// `InputIngredient`: o que o cache precisa de um ingrediente.
pub trait InputIngredient {
    /// `testType`: o tipo da entrada casa, sem olhar a quantidade.
    fn test_type(&self, input: &dyn TypedInstance) -> bool;

    /// A classificação que o `mapInputs` do cache lê.
    fn shape(&self) -> &IngredientShape;
}

/// O `testType` de um ingrediente do jogo.
type TypeTest = dyn Fn(&dyn TypedInstance) -> bool;

/// Um ingrediente com a classificação que o cache pede e o predicado de tipo do jogo.
#[derive(Clone)]
pub struct ShapedIngredient {
    shape: IngredientShape,
    test_type: Rc<TypeTest>,
}

impl ShapedIngredient {
    pub fn new(
        shape: IngredientShape,
        test_type: impl Fn(&dyn TypedInstance) -> bool + 'static,
    ) -> Self {
        Self {
            shape,
            test_type: Rc::new(test_type),
        }
    }
}

impl ShapedIngredient {
    /// Um tipo só, pelo id: o ingrediente vanilla de um item ou de um químico, sem componentes.
    pub fn of_type(id: &str) -> Self {
        let wanted: Rc<str> = Rc::from(id);
        Self::new(IngredientShape::types(&[id]), move |input| {
            input.type_id() == &*wanted
        })
    }
}

impl InputIngredient for ShapedIngredient {
    fn test_type(&self, input: &dyn TypedInstance) -> bool {
        (self.test_type)(input)
    }

    fn shape(&self) -> &IngredientShape {
        &self.shape
    }
}

/// `SingleInputRecipe`/`Predicate<STACK>`: a receita casa com a entrada, quantidade incluída.
pub trait SingleInputRecipe<S> {
    fn test(&self, input: &S) -> bool;
}

/// `BiPredicate<INPUT_A, INPUT_B>`.
pub trait DoubleInputRecipe<A, B> {
    fn test(&self, input_a: &A, input_b: &B) -> bool;
}

/// As posições de receita de uma chave: as de componentes exatos primeiro, depois as do tipo (`Iterables.concat`).
pub type RecipeIter<'a> =
    std::iter::Chain<Copied<slice::Iter<'a, usize>>, Copied<slice::Iter<'a, usize>>>;

fn recipe_iter<'a>(first: &'a [usize], second: &'a [usize]) -> RecipeIter<'a> {
    first.iter().copied().chain(second.iter().copied())
}

/// `IInputCache`.
pub trait InputCache {
    /// `contains(input)`.
    fn contains(&self, input: &dyn TypedInstance) -> bool;

    /// `getRecipes(input)`.
    fn get_recipes<'a>(&'a self, input: &dyn TypedInstance) -> RecipeIter<'a>;

    /// `mapInputs`: `true` se o ingrediente é complexo e o `IInputRecipeCache` tem de tratar a receita à parte.
    fn map_inputs(&mut self, recipe: usize, shape: &IngredientShape) -> bool;

    fn clear(&mut self);

    /// `isEmpty`: sem entrada (`null`) é vazio.
    fn is_empty(&self, input: &dyn TypedInstance) -> bool;
}

/// `IInputCache.contains(input, matchCriteria)`.
pub fn contains_matching<R>(
    cache: &impl InputCache,
    recipes: &[Rc<R>],
    input: &dyn TypedInstance,
    match_criteria: impl Fn(&R) -> bool,
) -> bool {
    find_first_matching(cache, recipes, input, match_criteria).is_some()
}

/// `IInputCache.findFirstRecipe(input, matchCriteria)`: a posição da receita.
pub fn find_first_matching<R>(
    cache: &impl InputCache,
    recipes: &[Rc<R>],
    input: &dyn TypedInstance,
    match_criteria: impl Fn(&R) -> bool,
) -> Option<usize> {
    cache
        .get_recipes(input)
        .find(|&recipe| match_criteria(&recipes[recipe]))
}

/// `BaseInputCache`: o mapa de tipo para as receitas que o têm, na ordem em que entraram.
#[derive(Default)]
pub struct BaseInputCache {
    input_cache: HashMap<Rc<str>, Vec<usize>>,
}

impl BaseInputCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `addInputCache`.
    fn add_input_cache(&mut self, input: &Rc<str>, recipe: usize) {
        self.input_cache
            .entry(input.clone())
            .or_default()
            .push(recipe);
    }

    fn recipes_of(&self, input: &dyn TypedInstance) -> &[usize] {
        self.input_cache
            .get(input.type_id())
            .map_or(&[], Vec::as_slice)
    }
}

impl InputCache for BaseInputCache {
    fn contains(&self, input: &dyn TypedInstance) -> bool {
        self.input_cache.contains_key(input.type_id())
    }

    fn get_recipes<'a>(&'a self, input: &dyn TypedInstance) -> RecipeIter<'a> {
        recipe_iter(&[], self.recipes_of(input))
    }

    /// Só `Types` se indexa aqui. Os outros não existem para este cache; ficam como complexos, para o teste de tipo decidir.
    fn map_inputs(&mut self, recipe: usize, shape: &IngredientShape) -> bool {
        match shape {
            IngredientShape::Types(types) => {
                for input in types {
                    self.add_input_cache(input, recipe);
                }
                false
            }
            _ => true,
        }
    }

    fn clear(&mut self) {
        self.input_cache.clear();
    }

    fn is_empty(&self, input: &dyn TypedInstance) -> bool {
        input.is_empty()
    }
}

/// `ChemicalInputCache`: o `BaseInputCache` que não indexa o químico vazio e conta como vazio o tipo dele.
pub struct ChemicalInputCache {
    base: BaseInputCache,
    /// `ChemicalIds.EMPTY`.
    empty: Option<Rc<str>>,
}

impl ChemicalInputCache {
    pub fn new(empty: Option<&str>) -> Self {
        Self {
            base: BaseInputCache::new(),
            empty: empty.map(Rc::from),
        }
    }
}

impl InputCache for ChemicalInputCache {
    fn contains(&self, input: &dyn TypedInstance) -> bool {
        self.base.contains(input)
    }

    fn get_recipes<'a>(&'a self, input: &dyn TypedInstance) -> RecipeIter<'a> {
        self.base.get_recipes(input)
    }

    fn map_inputs(&mut self, recipe: usize, shape: &IngredientShape) -> bool {
        match shape {
            IngredientShape::Types(types) => {
                for input in types {
                    // Ignora o vazio: alguns mods têm ingredientes com stacks vazios.
                    if self.empty.as_ref() != Some(input) {
                        self.base.add_input_cache(input, recipe);
                    }
                }
                false
            }
            _ => true,
        }
    }

    fn clear(&mut self) {
        self.base.clear();
    }

    fn is_empty(&self, input: &dyn TypedInstance) -> bool {
        input.is_empty() || self.empty.as_deref() == Some(input.type_id())
    }
}

/// Componentes (`None` são os padrão) para as receitas que os pedem exatos.
type ComponentRecipes = HashMap<Option<Rc<str>>, Vec<usize>>;

/// `ComponentSensitiveInputCache` com o `mapInputs` de `ItemInputCache` e de `FluidInputCache`: além do mapa por tipo, um por
/// tipo e componentes exatos.
#[derive(Default)]
pub struct ComponentSensitiveInputCache {
    base: BaseInputCache,
    /// Tipo para componentes para receitas.
    component_input_cache: HashMap<Rc<str>, ComponentRecipes>,
}

impl ComponentSensitiveInputCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `addComponentInputCache`: só para casamento exato.
    fn add_component_input_cache(
        &mut self,
        input: &Rc<str>,
        components: &Option<Rc<str>>,
        recipe: usize,
    ) {
        self.component_input_cache
            .entry(input.clone())
            .or_default()
            .entry(components.clone())
            .or_default()
            .push(recipe);
    }

    /// `getComponentMatches`.
    fn component_matches(&self, input: &dyn TypedInstance) -> Option<&[usize]> {
        let components = match input.components_key() {
            ComponentsKey::Default => None,
            ComponentsKey::Patch(patch) => Some(Rc::<str>::from(patch)),
            ComponentsKey::Unmatchable => return None,
        };
        self.component_input_cache
            .get(input.type_id())?
            .get(&components)
            .map(Vec::as_slice)
    }
}

impl InputCache for ComponentSensitiveInputCache {
    /// Olha o mapa de componentes, mais específico, e depois o do tipo.
    fn contains(&self, input: &dyn TypedInstance) -> bool {
        self.base.contains(input)
            || (!self.component_input_cache.is_empty() && self.component_matches(input).is_some())
    }

    fn get_recipes<'a>(&'a self, input: &dyn TypedInstance) -> RecipeIter<'a> {
        let basic = self.base.recipes_of(input);
        if self.component_input_cache.is_empty() {
            return recipe_iter(&[], basic);
        }
        match self.component_matches(input) {
            None => recipe_iter(&[], basic),
            Some(component_recipes) => recipe_iter(component_recipes, basic),
        }
    }

    fn map_inputs(&mut self, recipe: usize, shape: &IngredientShape) -> bool {
        match shape {
            IngredientShape::Types(types) => {
                for input in types {
                    self.base.add_input_cache(input, recipe);
                }
                false
            }
            IngredientShape::Compound(children) => {
                let mut result = false;
                for child in children {
                    result |= self.map_inputs(recipe, child);
                }
                result
            }
            IngredientShape::Exact { types, components } => {
                for input in types {
                    self.add_component_input_cache(input, components, recipe);
                }
                false
            }
            IngredientShape::Custom => true,
        }
    }

    fn clear(&mut self) {
        self.base.clear();
        self.component_input_cache.clear();
    }

    fn is_empty(&self, input: &dyn TypedInstance) -> bool {
        self.base.is_empty(input)
    }
}

/// Um lado de uma receita: o cache de uma entrada, o que o extrator tira da receita e as receitas em que esse ingrediente
/// é complexo (`complexRecipes` do `SingleInputRecipeCache`, `complexIngredientA` e `complexIngredientB`).
struct Side<R, G, C> {
    extractor: fn(&R) -> &G,
    cache: C,
    complex: Vec<usize>,
}

impl<R, G: InputIngredient, C: InputCache> Side<R, G, C> {
    fn new(extractor: fn(&R) -> &G, cache: C) -> Self {
        Self {
            extractor,
            cache,
            complex: Vec::new(),
        }
    }

    /// O resto de `AbstractInputRecipeCache.containsInput` depois de o cache iniciar: o mapa, e as receitas complexas.
    fn contains_initialized(&self, recipes: &[Rc<R>], input: &dyn TypedInstance) -> bool {
        self.cache.contains(input)
            || self
                .complex
                .iter()
                .any(|&recipe| (self.extractor)(&recipes[recipe]).test_type(input))
    }

    fn map(&mut self, recipes: &[Rc<R>], index: usize) -> bool {
        let complex = self
            .cache
            .map_inputs(index, (self.extractor)(&recipes[index]).shape());
        if complex {
            self.complex.push(index);
        }
        complex
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.complex.clear();
    }
}

/// `SingleInputRecipeCache`. `source` é o `recipeType.getRecipes(world)`, chamado na primeira pergunta depois de um `clear`.
///
/// `S` é a pilha que `findFirstRecipe` recebe. As outras perguntas recebem qualquer `TypedInstance`: o `ItemStack` e o
/// `ItemResource` do Java, que se comparam de modo diferente nos componentes (ver [`ComponentsKey`]).
pub struct SingleInputRecipeCache<R, G, S, C> {
    source: Box<dyn Fn() -> Vec<Rc<R>>>,
    state: RefCell<SingleState<R, G, C>>,
    stack: PhantomData<fn(&S)>,
}

struct SingleState<R, G, C> {
    initialized: bool,
    recipes: Vec<Rc<R>>,
    side: Side<R, G, C>,
}

impl<R, G, S, C> SingleInputRecipeCache<R, G, S, C>
where
    G: InputIngredient,
    S: TypedInstance,
    C: InputCache,
    R: SingleInputRecipe<S>,
{
    pub fn new(
        source: impl Fn() -> Vec<Rc<R>> + 'static,
        input_extractor: fn(&R) -> &G,
        cache: C,
    ) -> Self {
        Self {
            source: Box::new(source),
            state: RefCell::new(SingleState {
                initialized: false,
                recipes: Vec::new(),
                side: Side::new(input_extractor, cache),
            }),
            stack: PhantomData,
        }
    }

    /// `clear`: o cache volta a ser montado na próxima pergunta.
    pub fn clear(&self) {
        let mut state = self.state.borrow_mut();
        state.initialized = false;
        state.recipes.clear();
        state.side.clear();
    }

    /// `initCacheIfNeeded` e `initCache`.
    fn init_cache_if_needed(&self) {
        if self.state.borrow().initialized {
            return;
        }
        let recipes = (self.source)();
        let mut state = self.state.borrow_mut();
        state.initialized = true;
        for index in 0..recipes.len() {
            state.side.map(&recipes, index);
        }
        state.recipes = recipes;
    }

    /// `true` se a entrada é vazia, sem montar o cache: é a checagem que as perguntas fazem antes de `initCacheIfNeeded`.
    fn is_empty(&self, input: &dyn TypedInstance) -> bool {
        self.state.borrow().side.cache.is_empty(input)
    }

    /// `containsInput`.
    pub fn contains_input(&self, input: &dyn TypedInstance) -> bool {
        // Não deixa entrada vazia.
        if self.is_empty(input) {
            return false;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        state.side.contains_initialized(&state.recipes, input)
    }

    /// `findFirstRecipe(world, input)`: primeiro as receitas que o mapa tem para a entrada, depois as complexas.
    pub fn find_first_recipe(&self, input: &S) -> Option<Rc<R>> {
        if self.is_empty(input) {
            return None;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        let from_cache = state
            .side
            .cache
            .get_recipes(input)
            .find(|&recipe| state.recipes[recipe].test(input));
        let found = from_cache.or_else(|| {
            state
                .side
                .complex
                .iter()
                .copied()
                .find(|&recipe| state.recipes[recipe].test(input))
        });
        found.map(|recipe| state.recipes[recipe].clone())
    }

    /// `findTypeBasedRecipeHolder`: a primeira receita do tipo, sem olhar a quantidade.
    pub fn find_type_based_recipe(&self, input: &dyn TypedInstance) -> Option<Rc<R>> {
        self.find_type_based_matching(input, |_| true)
    }

    /// As duas sobrecargas de `findTypeBasedRecipe` com critério (`TriPredicate` e `CheckRecipeType`): o mesmo corpo, com os
    /// dados que o critério usa capturados no fecho. Sem critério (`findTypeBasedRecipeHolder`), o mapa decide só pelo tipo
    /// e a lista complexa também.
    pub fn find_type_based_matching(
        &self,
        input: &dyn TypedInstance,
        match_criteria: impl Fn(&R) -> bool,
    ) -> Option<Rc<R>> {
        if self.is_empty(input) {
            return None;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        let from_cache = state
            .side
            .cache
            .get_recipes(input)
            .find(|&recipe| match_criteria(&state.recipes[recipe]));
        let found = from_cache.or_else(|| {
            state.side.complex.iter().copied().find(|&recipe| {
                let candidate = &state.recipes[recipe];
                (state.side.extractor)(candidate).test_type(input) && match_criteria(candidate)
            })
        });
        found.map(|recipe| state.recipes[recipe].clone())
    }
}

/// Escolhe o lado A ou o B do estado.
type Selector<State, Part> = fn(&State) -> &Part;

/// `DoubleInputRecipeCache` (e `DoubleSameInputRecipeCache`, que só repete o tipo dos dois lados).
pub struct DoubleInputRecipeCache<R, GA, SA, CA, GB, SB, CB> {
    source: Box<dyn Fn() -> Vec<Rc<R>>>,
    state: RefCell<DoubleState<R, GA, CA, GB, CB>>,
    stacks: PhantomData<fn(&SA, &SB)>,
}

struct DoubleState<R, GA, CA, GB, CB> {
    initialized: bool,
    recipes: Vec<Rc<R>>,
    a: Side<R, GA, CA>,
    b: Side<R, GB, CB>,
    /// `complexRecipes`: qualquer um dos dois ingredientes é complexo.
    complex_recipes: Vec<usize>,
}

impl<R, GA, SA, CA, GB, SB, CB> DoubleInputRecipeCache<R, GA, SA, CA, GB, SB, CB>
where
    GA: InputIngredient,
    GB: InputIngredient,
    SA: TypedInstance,
    SB: TypedInstance,
    CA: InputCache,
    CB: InputCache,
    R: DoubleInputRecipe<SA, SB>,
{
    pub fn new(
        source: impl Fn() -> Vec<Rc<R>> + 'static,
        input_a_extractor: fn(&R) -> &GA,
        cache_a: CA,
        input_b_extractor: fn(&R) -> &GB,
        cache_b: CB,
    ) -> Self {
        Self {
            source: Box::new(source),
            state: RefCell::new(DoubleState {
                initialized: false,
                recipes: Vec::new(),
                a: Side::new(input_a_extractor, cache_a),
                b: Side::new(input_b_extractor, cache_b),
                complex_recipes: Vec::new(),
            }),
            stacks: PhantomData,
        }
    }

    pub fn clear(&self) {
        let mut state = self.state.borrow_mut();
        state.initialized = false;
        state.recipes.clear();
        state.a.clear();
        state.b.clear();
        state.complex_recipes.clear();
    }

    /// `initCache`: cada ingrediente se mapeia no cache dele, e a receita é complexa se algum deles for.
    fn init_cache_if_needed(&self) {
        if self.state.borrow().initialized {
            return;
        }
        let recipes = (self.source)();
        let mut state = self.state.borrow_mut();
        state.initialized = true;
        for index in 0..recipes.len() {
            let complex_a = state.a.map(&recipes, index);
            let complex_b = state.b.map(&recipes, index);
            if complex_a || complex_b {
                state.complex_recipes.push(index);
            }
        }
        state.recipes = recipes;
    }

    /// `containsInputA`.
    pub fn contains_input_a(&self, input: &dyn TypedInstance) -> bool {
        if self.state.borrow().a.cache.is_empty(input) {
            return false;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        state.a.contains_initialized(&state.recipes, input)
    }

    /// `containsInputB`.
    pub fn contains_input_b(&self, input: &dyn TypedInstance) -> bool {
        if self.state.borrow().b.cache.is_empty(input) {
            return false;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        state.b.contains_initialized(&state.recipes, input)
    }

    /// `containsInputAB`, o que o slot de A pergunta para inserir `input_a` com `input_b` já na máquina: sem B devolve `true`
    /// sem olhar A, e sem A vale qualquer A que alguma receita use.
    pub fn contains_input_ab(
        &self,
        input_a: &dyn TypedInstance,
        input_b: &dyn TypedInstance,
    ) -> bool {
        self.contains_pairing(|s| &s.a, input_a, |s| &s.b, input_b)
    }

    /// `containsInputBA`, o que o tanque de B pergunta para inserir `input_b` com `input_a` já na máquina.
    pub fn contains_input_ba(
        &self,
        input_a: &dyn TypedInstance,
        input_b: &dyn TypedInstance,
    ) -> bool {
        self.contains_pairing(|s| &s.b, input_b, |s| &s.a, input_a)
    }

    /// `AbstractInputRecipeCache.containsPairing`: `input1` é o que se quer inserir e `input2` o que já está lá. Os seletores
    /// escolhem qual lado, A ou B, faz o papel de 1 e de 2.
    #[allow(clippy::type_complexity)]
    fn contains_pairing<G1, C1, G2, C2>(
        &self,
        side1: Selector<DoubleState<R, GA, CA, GB, CB>, Side<R, G1, C1>>,
        input1: &dyn TypedInstance,
        side2: Selector<DoubleState<R, GA, CA, GB, CB>, Side<R, G2, C2>>,
        input2: &dyn TypedInstance,
    ) -> bool
    where
        G1: InputIngredient,
        G2: InputIngredient,
        C1: InputCache,
        C2: InputCache,
    {
        let (first_empty, second_empty) = {
            let state = self.state.borrow();
            (
                side1(&state).cache.is_empty(input1),
                side2(&state).cache.is_empty(input2),
            )
        };
        if first_empty {
            // Se o 2 está vazio isso se verifica no `containsInput`.
            if second_empty {
                return false;
            }
            self.init_cache_if_needed();
            let state = self.state.borrow();
            return side2(&state).contains_initialized(&state.recipes, input2);
        } else if second_empty {
            return true;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        let (first, second) = (side1(&state), side2(&state));
        // Se o cache do 1 tem o 1, só falta o tipo do 2: o 1 já casa.
        for recipe in first.cache.get_recipes(input1) {
            if (second.extractor)(&state.recipes[recipe]).test_type(input2) {
                return true;
            }
        }
        // O cache rápido do 1 não tem: olha as receitas em que o ingrediente 1 é complexo.
        first.complex.iter().any(|&recipe| {
            let candidate = &state.recipes[recipe];
            (first.extractor)(candidate).test_type(input1)
                && (second.extractor)(candidate).test_type(input2)
        })
    }

    /// `findFirstRecipe(world, a, b)`: procura pelo mapa de A.
    pub fn find_first_recipe(&self, input_a: &SA, input_b: &SB) -> Option<Rc<R>> {
        self.find_first_recipe_by(input_a, input_b, true)
    }

    /// `findFirstRecipe(world, a, b, useCacheA)`: `false` procura pelo mapa de B, para quando ele é bem maior.
    pub fn find_first_recipe_by(
        &self,
        input_a: &SA,
        input_b: &SB,
        use_cache_a: bool,
    ) -> Option<Rc<R>> {
        {
            let state = self.state.borrow();
            if state.a.cache.is_empty(input_a) || state.b.cache.is_empty(input_b) {
                return None;
            }
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        let tests = |recipe: &usize| state.recipes[*recipe].test(input_a, input_b);
        // Pelo mapa escolhido; se não há, as complexas (A ou B complexo).
        let found = if use_cache_a {
            state.a.cache.get_recipes(input_a).find(tests)
        } else {
            state.b.cache.get_recipes(input_b).find(tests)
        };
        let found = found.or_else(|| state.complex_recipes.iter().copied().find(tests));
        found.map(|recipe| state.recipes[recipe].clone())
    }

    /// `findTypeBasedRecipeHolder`: o tipo das duas entradas, sem a quantidade, e o critério. É para as fábricas: se B está
    /// vazio ele não entra na conta.
    pub fn find_type_based_recipe(
        &self,
        input_a: &dyn TypedInstance,
        input_b: &dyn TypedInstance,
        match_criteria: impl Fn(&R) -> bool,
    ) -> Option<Rc<R>> {
        if self.state.borrow().a.cache.is_empty(input_a) {
            // Não deixa a entrada principal vazia.
            return None;
        }
        self.init_cache_if_needed();
        let state = self.state.borrow();
        let found = if state.b.cache.is_empty(input_b) {
            // Sem B, procura por A e pelo critério.
            state
                .a
                .cache
                .get_recipes(input_a)
                .find(|&recipe| match_criteria(&state.recipes[recipe]))
                .or_else(|| {
                    state.complex_recipes.iter().copied().find(|&recipe| {
                        let candidate = &state.recipes[recipe];
                        (state.a.extractor)(candidate).test_type(input_a)
                            && match_criteria(candidate)
                    })
                })
        } else {
            state
                .a
                .cache
                .get_recipes(input_a)
                .find(|&recipe| {
                    let candidate = &state.recipes[recipe];
                    (state.b.extractor)(candidate).test_type(input_b) && match_criteria(candidate)
                })
                .or_else(|| {
                    state.complex_recipes.iter().copied().find(|&recipe| {
                        let candidate = &state.recipes[recipe];
                        (state.a.extractor)(candidate).test_type(input_a)
                            && (state.b.extractor)(candidate).test_type(input_b)
                            && match_criteria(candidate)
                    })
                })
        };
        found.map(|recipe| state.recipes[recipe].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Single {
        id: &'static str,
        input: ShapedIngredient,
        count: u32,
    }

    impl SingleInputRecipe<SlotContents> for Single {
        fn test(&self, input: &SlotContents) -> bool {
            self.input.test_type(input) && input.amount >= self.count
        }
    }

    fn item(id: &str) -> ItemResource {
        ItemResource::new(id, 64)
    }

    /// A chave de componentes de um item que só tem o componente `variant`.
    fn patch_key(variant: &str) -> String {
        crate::components::Components::variant(variant)
            .key()
            .to_owned()
    }

    fn variant(id: &str, components: &str) -> ItemResource {
        ItemResource::with_variant(id, 64, components)
    }

    fn stack(resource: ItemResource, amount: u32) -> SlotContents {
        SlotContents::new(Some(resource), amount)
    }

    /// Aceita o tipo, qualquer componente.
    fn simple(id: &'static str, count: u32, name: &'static str) -> Rc<Single> {
        Rc::new(Single {
            id: name,
            input: ShapedIngredient::new(IngredientShape::types(&[id]), move |input| {
                input.type_id() == id
            }),
            count,
        })
    }

    fn with_components(
        shape: IngredientShape,
        id: &'static str,
        components: &'static str,
        name: &'static str,
    ) -> Rc<Single> {
        let key = patch_key(components);
        Rc::new(Single {
            id: name,
            input: ShapedIngredient::new(shape, move |input| {
                input.type_id() == id && input.components_key() == ComponentsKey::Patch(&key)
            }),
            count: 1,
        })
    }

    /// Só o tipo com estes componentes, e fora do índice por tipo.
    fn custom(id: &'static str, components: &'static str, name: &'static str) -> Rc<Single> {
        with_components(IngredientShape::Custom, id, components, name)
    }

    fn exact(id: &'static str, components: &'static str, name: &'static str) -> Rc<Single> {
        with_components(
            IngredientShape::exact(&[id], Some(&patch_key(components))),
            id,
            components,
            name,
        )
    }

    type Cache = SingleInputRecipeCache<
        Single,
        ShapedIngredient,
        SlotContents,
        ComponentSensitiveInputCache,
    >;

    fn cache(recipes: Vec<Rc<Single>>) -> Cache {
        SingleInputRecipeCache::new(
            move || recipes.clone(),
            |recipe: &Single| &recipe.input,
            ComponentSensitiveInputCache::new(),
        )
    }

    fn name(found: Option<Rc<Single>>) -> Option<&'static str> {
        found.map(|recipe| recipe.id)
    }

    #[test]
    fn the_map_answers_before_the_complex_recipes_even_when_the_list_says_otherwise() {
        // Java: `SingleInputRecipeCache.findFirstRecipe` olha `cache.getRecipes(input)` e só depois `complexRecipes`.
        let cache = cache(vec![custom("coal", "rc5", "c0"), simple("coal", 1, "s1")]);
        let input = stack(variant("coal", "rc5"), 1);
        assert_eq!(name(cache.find_first_recipe(&input)), Some("s1"));
    }

    #[test]
    fn the_exact_component_recipes_come_before_the_ones_of_the_type() {
        // Java: `ComponentSensitiveInputCache.getRecipes` devolve `Iterables.concat(nbtRecipes, basicRecipes)`.
        let cache = cache(vec![simple("sand", 3, "s0"), exact("sand", "rc5", "e1")]);
        assert_eq!(
            name(cache.find_first_recipe(&stack(variant("sand", "rc5"), 5))),
            Some("e1")
        );
        assert_eq!(
            name(cache.find_first_recipe(&stack(variant("sand", "rc6"), 5))),
            Some("s0")
        );
        assert_eq!(
            name(cache.find_first_recipe(&stack(item("sand"), 5))),
            Some("s0")
        );
    }

    #[test]
    fn a_stack_and_a_resource_without_a_patch_do_not_compare_the_same() {
        // `ItemStack.getComponents()` é um `PatchedDataComponentMap`, e `ItemResource.getComponents()` sem patch é o mapa do
        // protótipo: só o primeiro é igual à chave de um ingrediente exato com os componentes padrão.
        let default_exact = Rc::new(Single {
            id: "e",
            input: ShapedIngredient::new(
                IngredientShape::Exact {
                    types: vec![Rc::from("sand")],
                    components: None,
                },
                |input| {
                    input.type_id() == "sand"
                        && input.components_key() != ComponentsKey::Patch(&patch_key("rc5"))
                },
            ),
            count: 1,
        });
        let cache = cache(vec![simple("sand", 1, "s"), default_exact]);
        assert_eq!(
            name(cache.find_first_recipe(&stack(item("sand"), 1))),
            Some("e")
        );
        assert_eq!(
            name(cache.find_type_based_recipe(&stack(item("sand"), 1))),
            Some("e")
        );
        assert_eq!(name(cache.find_type_based_recipe(&item("sand"))), Some("s"));
        assert_eq!(
            name(cache.find_type_based_recipe(&variant("sand", "rc5"))),
            Some("s")
        );
    }

    #[test]
    fn an_empty_input_finds_nothing_and_does_not_build_the_cache() {
        let built = Rc::new(Cell::new(0));
        let counter = built.clone();
        let cache: Cache = SingleInputRecipeCache::new(
            move || {
                counter.set(counter.get() + 1);
                vec![simple("coal", 1, "s")]
            },
            |recipe: &Single| &recipe.input,
            ComponentSensitiveInputCache::new(),
        );
        assert!(cache.find_first_recipe(&SlotContents::default()).is_none());
        assert!(!cache.contains_input(&None::<ItemResource>));
        assert_eq!(built.get(), 0);
        assert!(cache.find_first_recipe(&stack(item("coal"), 1)).is_some());
        assert!(cache.find_first_recipe(&stack(item("coal"), 1)).is_some());
        assert_eq!(built.get(), 1);
    }

    #[test]
    fn clear_makes_the_next_question_read_the_list_again() {
        let recipes: Rc<RefCell<Vec<Rc<Single>>>> =
            Rc::new(RefCell::new(vec![simple("coal", 1, "old")]));
        let source = recipes.clone();
        let cache: Cache = SingleInputRecipeCache::new(
            move || source.borrow().clone(),
            |recipe: &Single| &recipe.input,
            ComponentSensitiveInputCache::new(),
        );
        let coal = stack(item("coal"), 1);
        assert_eq!(name(cache.find_first_recipe(&coal)), Some("old"));
        *recipes.borrow_mut() = vec![simple("coal", 1, "new")];
        // Sem `clear` o índice velho segue valendo, como no Java.
        assert_eq!(name(cache.find_first_recipe(&coal)), Some("old"));
        cache.clear();
        assert_eq!(name(cache.find_first_recipe(&coal)), Some("new"));
    }

    #[test]
    fn contains_input_asks_the_map_and_then_the_complex_recipes() {
        let cache = cache(vec![simple("coal", 1, "s"), custom("diamond", "rc5", "c")]);
        assert!(cache.contains_input(&item("coal")));
        assert!(!cache.contains_input(&item("stone")));
        // O tipo só chega por uma receita complexa: o `testType` dela decide, componentes incluídos.
        assert!(!cache.contains_input(&item("diamond")));
        assert!(cache.contains_input(&variant("diamond", "rc5")));
        // Os componentes exatos contam como conhecidos mesmo sem o tipo no mapa.
        let exact_only = self::cache(vec![exact("sand", "rc5", "e")]);
        assert!(exact_only.contains_input(&variant("sand", "rc5")));
        assert!(!exact_only.contains_input(&item("sand")));
    }

    #[test]
    fn a_compound_ingredient_is_complex_when_any_child_is() {
        let compound = Rc::new(Single {
            id: "m",
            input: ShapedIngredient::new(
                IngredientShape::Compound(vec![
                    IngredientShape::types(&["charcoal"]),
                    IngredientShape::Custom,
                ]),
                |input| input.type_id() == "charcoal",
            ),
            count: 1,
        });
        let cache = cache(vec![compound]);
        let charcoal = stack(item("charcoal"), 1);
        // Está no mapa pelo filho simples, e também na lista das complexas.
        assert_eq!(name(cache.find_first_recipe(&charcoal)), Some("m"));
        let state = cache.state.borrow();
        assert_eq!(state.side.complex, vec![0]);
    }

    #[test]
    fn the_type_based_lookup_ignores_the_amount_and_checks_the_criteria() {
        let cache = cache(vec![simple("coal", 5, "five"), simple("coal", 1, "one")]);
        let coal = item("coal");
        assert_eq!(name(cache.find_type_based_recipe(&coal)), Some("five"));
        assert_eq!(
            name(cache.find_type_based_matching(&coal, |recipe| recipe.id == "one")),
            Some("one")
        );
        assert!(
            cache
                .find_type_based_recipe(&None::<ItemResource>)
                .is_none()
        );
    }

    #[test]
    fn the_chemical_cache_skips_the_empty_chemical() {
        let mut cache = ChemicalInputCache::new(Some("empty"));
        assert!(!cache.map_inputs(0, &IngredientShape::types(&["empty", "redstone"])));
        let empty = ChemicalResource::new("empty");
        let redstone = ChemicalResource::new("redstone");
        assert!(!cache.contains(&empty));
        assert!(cache.contains(&redstone));
        assert!(cache.is_empty(&empty));
        assert!(!cache.is_empty(&redstone));
        assert!(cache.is_empty(&None::<ChemicalResource>));
        // Só `Types` se indexa: qualquer outra forma vira receita complexa.
        assert!(cache.map_inputs(1, &IngredientShape::Custom));
    }

    struct Pair {
        id: &'static str,
        item: ShapedIngredient,
        chemical: ShapedIngredient,
    }

    impl DoubleInputRecipe<SlotContents, TankContents> for Pair {
        fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
            self.item.test_type(item) && self.chemical.test_type(chemical)
        }
    }

    fn pair(item_id: &'static str, chemical_id: &'static str, name: &'static str) -> Rc<Pair> {
        Rc::new(Pair {
            id: name,
            item: ShapedIngredient::new(IngredientShape::types(&[item_id]), move |input| {
                input.type_id() == item_id
            }),
            chemical: ShapedIngredient::new(IngredientShape::types(&[chemical_id]), move |input| {
                input.type_id() == chemical_id
            }),
        })
    }

    type Pairs = DoubleInputRecipeCache<
        Pair,
        ShapedIngredient,
        SlotContents,
        ComponentSensitiveInputCache,
        ShapedIngredient,
        TankContents,
        ChemicalInputCache,
    >;

    fn pairs(recipes: Vec<Rc<Pair>>) -> Pairs {
        DoubleInputRecipeCache::new(
            move || recipes.clone(),
            |recipe: &Pair| &recipe.item,
            ComponentSensitiveInputCache::new(),
            |recipe: &Pair| &recipe.chemical,
            ChemicalInputCache::new(Some("empty")),
        )
    }

    #[test]
    fn the_insertion_rules_of_a_pair_follow_contains_pairing() {
        let cache = pairs(vec![
            pair("coal", "redstone", "d0"),
            pair("sand", "carbon", "d1"),
        ]);
        let (coal, sand) = (item("coal"), item("sand"));
        let (redstone, carbon) = (
            ChemicalResource::new("redstone"),
            ChemicalResource::new("carbon"),
        );
        let (no_item, no_chemical) = (None::<ItemResource>, None::<ChemicalResource>);
        // `containsInputAB`: o item que entra, com o químico que já está.
        assert!(cache.contains_input_ab(&coal, &redstone));
        assert!(!cache.contains_input_ab(&coal, &carbon));
        assert!(cache.contains_input_ab(&coal, &no_chemical));
        assert!(cache.contains_input_ab(&no_item, &carbon));
        assert!(!cache.contains_input_ab(&no_item, &no_chemical));
        // `containsInputBA`: o químico que entra, com o item que já está.
        assert!(cache.contains_input_ba(&sand, &carbon));
        assert!(!cache.contains_input_ba(&coal, &carbon));
        assert!(cache.contains_input_ba(&no_item, &carbon));
        assert!(cache.contains_input_ba(&coal, &no_chemical));
        assert!(cache.contains_input_a(&sand));
        assert!(!cache.contains_input_a(&item("stone")));
        assert!(cache.contains_input_b(&redstone));
        assert!(!cache.contains_input_b(&ChemicalResource::new("empty")));
    }

    #[test]
    fn a_pair_is_found_by_either_map() {
        let cache = pairs(vec![
            pair("coal", "redstone", "d0"),
            pair("sand", "redstone", "d1"),
        ]);
        let sand = stack(item("sand"), 1);
        let redstone = TankContents::new(Some(ChemicalResource::new("redstone")), 10);
        let found = |by_a: bool| {
            cache
                .find_first_recipe_by(&sand, &redstone, by_a)
                .map(|r| r.id)
        };
        assert_eq!(found(true), Some("d1"));
        assert_eq!(found(false), Some("d1"));
        assert!(
            cache
                .find_first_recipe(&SlotContents::default(), &redstone)
                .is_none()
        );
        let by_type = |a: &dyn TypedInstance, b: &dyn TypedInstance| {
            cache
                .find_type_based_recipe(a, b, |_| true)
                .map(|recipe| recipe.id)
        };
        assert_eq!(
            by_type(&item("coal"), &None::<ChemicalResource>),
            Some("d0")
        );
        assert_eq!(
            by_type(&None::<ItemResource>, &ChemicalResource::new("redstone")),
            None
        );
    }
}
