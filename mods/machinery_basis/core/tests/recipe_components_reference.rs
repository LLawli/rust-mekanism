//! Repete o rastro do `DataComponentIngredient` e da ordem das receitas complexas
//! (`dev/referencia/rastros/recipe_components.jsonl` e `recipe_components_2.jsonl`, escritos por `ComponentIngredientTraceTest.java`; os dois
//! arquivos são duas execuções do mesmo teste, com hashes de identidade diferentes) contra o porte.
//!
//! `matrix`: um ingrediente `neoforge:components` e uma pilha, com os componentes todos da pilha (`full`, os padrão do item e o patch) e as
//! respostas de `ItemStackIngredient.test` e `testType`. `cache`: as receitas num `InputRecipeCache.SingleItem`, com o hash de identidade de
//! cada id, e as respostas de `findFirstRecipe`, `containsInput` e `findTypeBasedRecipeHolder`.
use machinery_basis_core::components::Components;
use machinery_basis_core::components_ingredient::ComponentsIngredient;
use machinery_basis_core::containers::{ItemResource, SlotContents};
use machinery_basis_core::recipe_cache::{
    ComponentSensitiveInputCache, ComponentsKey, IngredientShape, InputIngredient,
    ShapedIngredient, SingleInputRecipe, SingleInputRecipeCache, TypedInstance,
};
use machinery_basis_core::recipe_monitor::RecipeId;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::Rc;

/// Menos do que isto é um rastro cortado.
const MINIMUM_MATRIX: usize = 280;
const MINIMUM_CACHE_QUERIES: usize = 80;
/// Um valor canônico: números inteiros e decimais iguais dão o mesmo texto, e `true` e `false` valem 1 e 0 (o Horizon guarda o
/// booleano de um componente como byte).
fn canon(value: &Value) -> String {
    match value {
        Value::Number(number) => {
            let float = number.as_f64().unwrap();
            if float.fract() == 0.0 {
                format!("{}", float as i64)
            } else {
                format!("{float}")
            }
        }
        Value::Bool(flag) => (if *flag { "1" } else { "0" }).to_owned(),
        Value::String(text) => format!("{text:?}"),
        Value::Array(list) => format!("[{}]", list.iter().map(canon).collect::<Vec<_>>().join(",")),
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.iter().collect();
            let parts: Vec<String> = sorted
                .iter()
                .map(|(key, value)| format!("{key}:{}", canon(value)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Null => "null".to_owned(),
    }
}

fn full_map(full: &Value) -> BTreeMap<String, String> {
    full.as_object()
        .unwrap()
        .iter()
        .map(|(kind, value)| (kind.clone(), canon(value)))
        .collect()
}

/// Os componentes padrão de cada item que o rastro mostra (a pilha sem patch), e para os outros só o `repair_cost`.
type Defaults = HashMap<String, BTreeMap<String, String>>;

fn defaults_of(defaults: &Defaults, item: &str) -> BTreeMap<String, String> {
    defaults
        .get(item)
        .cloned()
        .unwrap_or_else(|| BTreeMap::from([("minecraft:repair_cost".to_owned(), "0".to_owned())]))
}

fn items_of(ingredient: &Value) -> Vec<String> {
    match &ingredient["items"] {
        Value::String(item) => vec![item.clone()],
        Value::Array(list) => list
            .iter()
            .map(|item| item.as_str().unwrap().to_owned())
            .collect(),
        other => panic!("items {other}"),
    }
}

thread_local! {
    /// O patch que cada chave de componentes guarda, para o teste de tipo refazer a pilha (o `EXTRAS` do glue).
    static PATCHES: RefCell<HashMap<String, Vec<(String, String)>>> = RefCell::new(HashMap::new());
}

/// O patch de uma pilha do rastro (`components`), com `!tipo` para o que ela tira.
fn patch_of(stack: &Value) -> Vec<(String, String)> {
    stack["components"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(kind, value)| match kind.strip_prefix('!') {
                    Some(_) => (kind.clone(), String::new()),
                    None => (kind.clone(), canon(value)),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Todos os componentes de uma entrada: os padrão do item e o patch por cima.
fn view(defaults: &Defaults, item: &str, patch: &[(String, String)]) -> BTreeMap<String, String> {
    let mut full = defaults_of(defaults, item);
    for (kind, value) in patch {
        match kind.strip_prefix('!') {
            Some(removed) => {
                full.remove(removed);
            }
            None => {
                full.insert(kind.clone(), value.clone());
            }
        }
    }
    full
}

fn components_of(patch: &[(String, String)]) -> Components {
    let components = Components::new(
        patch
            .iter()
            .map(|(kind, value)| (kind.as_str(), value.as_str())),
    );
    PATCHES.with(|table| {
        table
            .borrow_mut()
            .insert(components.key().to_owned(), patch.to_vec())
    });
    components
}

fn parse_ingredient(spec: &Value) -> Option<ComponentsIngredient> {
    ComponentsIngredient::parse(spec.as_object()?, canon)
}

/// A classificação que o glue faz do ingrediente (`ItemInputCache.mapIngredient`): um item simples indexa o tipo; `strict` indexa a chave
/// de cada item (`fromPatch` contra os padrão dele); o resto é complexo.
fn shape(spec: &Value, defaults: &Defaults) -> IngredientShape {
    if let Some(item) = spec.as_str() {
        return IngredientShape::types(&[item]);
    }
    let ingredient = parse_ingredient(spec).unwrap();
    if !ingredient.is_strict() {
        return IngredientShape::Custom;
    }
    IngredientShape::Compound(
        items_of(spec)
            .iter()
            .map(|item| {
                let key = ingredient.exact_components(&defaults_of(defaults, item));
                IngredientShape::exact(&[item], (!key.is_empty()).then(|| key.key()))
            })
            .collect(),
    )
}

/// `ItemStackIngredient.testType`: o item está na lista e o ingrediente aceita os componentes da entrada.
fn test_type(spec: &Value, defaults: &Defaults, input: &dyn TypedInstance) -> bool {
    if let Some(item) = spec.as_str() {
        return input.type_id() == item;
    }
    if !items_of(spec).iter().any(|item| item == input.type_id()) {
        return false;
    }
    let patch = match input.components_key() {
        ComponentsKey::Patch(key) => {
            PATCHES.with(|table| table.borrow().get(key).cloned().unwrap_or_default())
        }
        ComponentsKey::Default | ComponentsKey::Unmatchable => Vec::new(),
    };
    parse_ingredient(spec)
        .unwrap()
        .test(&view(defaults, input.type_id(), &patch))
}

fn ingredient(spec: &Value, defaults: &Rc<Defaults>) -> ShapedIngredient {
    let (own, table) = (spec.clone(), defaults.clone());
    ShapedIngredient::new(shape(spec, defaults), move |input: &dyn TypedInstance| {
        test_type(&own, &table, input)
    })
}

struct Recipe {
    id: String,
    input: ShapedIngredient,
    count: u32,
}

impl RecipeId for Recipe {
    fn recipe_id(&self) -> &str {
        &self.id
    }
}

impl SingleInputRecipe<SlotContents> for Recipe {
    fn test(&self, input: &SlotContents) -> bool {
        input.resource.is_some() && self.input.test_type(input) && input.amount >= self.count
    }
}

fn resource(stack: &Value) -> ItemResource {
    let id = stack["id"].as_str().unwrap();
    let patch = patch_of(stack);
    if patch.is_empty() {
        ItemResource::new(id, 64)
    } else {
        ItemResource::with_components(id, 64, components_of(&patch))
    }
}

fn contents(stack: &Value) -> SlotContents {
    SlotContents::new(
        Some(resource(stack)),
        stack["count"].as_u64().unwrap() as u32,
    )
}

type Cache =
    SingleInputRecipeCache<Recipe, ShapedIngredient, SlotContents, ComponentSensitiveInputCache>;

fn defaults_from(lines: &[Value]) -> Defaults {
    let mut defaults = Defaults::new();
    for line in lines.iter().filter(|line| line["kind"] == "matrix") {
        if line["stack"].get("components").is_none() {
            defaults.insert(
                line["stack"]["id"].as_str().unwrap().to_owned(),
                full_map(&line["full"]),
            );
        }
    }
    defaults
}

fn load(file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_component_ingredient_matches_like_the_original() {
    let lines = load("recipe_components.jsonl");
    let (mut count, mut accepted, mut refused) = (0, 0, 0);
    for line in lines.iter().filter(|line| line["kind"] == "matrix") {
        count += 1;
        let spec = &line["ingredient"];
        let ingredient = parse_ingredient(spec).unwrap();
        let stack = &line["stack"];
        let listed = items_of(spec)
            .iter()
            .any(|item| item == stack["id"].as_str().unwrap());
        let rust = listed
            && stack["count"].as_u64().unwrap() >= 1
            && ingredient.test(&full_map(&line["full"]));
        assert_eq!(rust, line["test"].as_bool().unwrap(), "{line}");
        assert_eq!(rust, line["test_type"].as_bool().unwrap(), "{line}");
        let wanted = if ingredient.is_strict() {
            "exact"
        } else {
            "complex"
        };
        assert_eq!(wanted, line["shape"].as_str().unwrap(), "{line}");
        if rust {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(count >= MINIMUM_MATRIX, "só {count} linhas");
    assert!(
        accepted >= 40 && refused >= 200,
        "{accepted} aceitas, {refused} recusadas"
    );
}

#[test]
fn the_caches_answer_like_the_original_with_component_ingredients() {
    let mut queries = 0;
    let mut hashed_cases = 0;
    for file in ["recipe_components.jsonl", "recipe_components_2.jsonl"] {
        let lines = load(file);
        let defaults = Rc::new(defaults_from(&lines));
        for case in lines.iter().filter(|line| line["kind"] == "cache") {
            let name = case["case"].as_str().unwrap();
            let hashes: HashMap<String, i32> = case["recipes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|recipe| {
                    (
                        recipe["id"].as_str().unwrap().to_owned(),
                        recipe["hash"].as_i64().unwrap() as i32,
                    )
                })
                .collect();
            let recipes: Vec<Rc<Recipe>> = case["recipes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|recipe| {
                    Rc::new(Recipe {
                        id: recipe["id"].as_str().unwrap().to_owned(),
                        input: ingredient(&recipe["ingredient"], &defaults),
                        count: recipe["count"].as_u64().unwrap() as u32,
                    })
                })
                .collect();
            let cache: Cache = SingleInputRecipeCache::new(
                move || recipes.clone(),
                |recipe: &Recipe| &recipe.input,
                ComponentSensitiveInputCache::new(),
            )
            .with_identity_hash(move |id| hashes[id]);
            for query in case["queries"].as_array().unwrap() {
                queries += 1;
                let stack = &query["stack"];
                let (as_stack, as_resource) = (contents(stack), resource(stack));
                let op = query["op"].as_str().unwrap();
                let wanted = query["result"].clone();
                let found = |recipe: Option<Rc<Recipe>>| {
                    recipe.map_or(Value::Null, |recipe| Value::from(recipe.id.clone()))
                };
                let rust = match op {
                    "find_first" => found(cache.find_first_recipe(&as_stack)),
                    "contains" => Value::from(cache.contains_input(&as_resource)),
                    "contains_stack" => Value::from(cache.contains_input(&as_stack)),
                    "type_based" => found(cache.find_type_based_recipe(&as_resource)),
                    "type_based_stack" => found(cache.find_type_based_recipe(&as_stack)),
                    op => panic!("pergunta {op}"),
                };
                assert_eq!(rust, wanted, "{file} {name} {op}: {query}");
            }
            if name.starts_with("complex_hash_order") {
                hashed_cases += 1;
            }
        }
    }
    assert!(queries >= MINIMUM_CACHE_QUERIES, "só {queries} perguntas");
    assert_eq!(
        hashed_cases, 4,
        "os dois casos de ordem, nas duas execuções"
    );
}
