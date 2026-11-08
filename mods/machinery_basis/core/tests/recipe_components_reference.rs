//! Repete o rastro do `DataComponentIngredient` e da ordem das receitas complexas
//! (`dev/referencia/rastros/recipe_components.jsonl` e `recipe_components_2.jsonl`, escritos por `ComponentIngredientTraceTest.java`; os dois
//! arquivos são duas execuções do mesmo teste, com hashes de identidade diferentes) contra o porte.
//!
//! `matrix`: um ingrediente `neoforge:components` e uma pilha, com os componentes todos da pilha (`full`, os padrão do item e o patch) e as
//! respostas de `ItemStackIngredient.test` e `testType`. `cache`: as receitas num `InputRecipeCache.SingleItem`, com o hash de identidade de
//! cada id, e as respostas de `findFirstRecipe`, `containsInput` e `findTypeBasedRecipeHolder`.
use machinery_basis_core::components::Components;
use machinery_basis_core::components_ingredient::{ComponentsIngredient, Leaf, Tree};
use machinery_basis_core::containers::{ItemResource, SlotContents};
use machinery_basis_core::recipe_cache::{
    ComponentSensitiveInputCache, ComponentsKey, InputIngredient, ShapedIngredient,
    SingleInputRecipe, SingleInputRecipeCache, TypedInstance,
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
    /// O patch que cada chave de componentes guarda, para o teste de tipo refazer a pilha (o `PATCHES` do glue).
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

fn kind_of(spec: &Value) -> &str {
    spec.get("neoforge:ingredient_type")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// A árvore que o glue monta do ingrediente (`build_node`): texto e lista são `Plain`, `neoforge:components` uma folha com os padrão de cada
/// item, e `compound`, `intersection` e `difference` guardam os filhos.
fn tree_of(spec: &Value, defaults: &Defaults) -> Tree {
    match spec {
        Value::String(item) => Tree::Plain(vec![item.clone()]),
        Value::Array(list) => Tree::Plain(
            list.iter()
                .map(|item| item.as_str().unwrap().to_owned())
                .collect(),
        ),
        Value::Object(object) => {
            let kids = |field: &str| -> Vec<Tree> {
                spec[field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|kid| tree_of(kid, defaults))
                    .collect()
            };
            match kind_of(spec) {
                "neoforge:components" => {
                    let items = items_of(spec);
                    Tree::Components(Leaf {
                        ingredient: ComponentsIngredient::parse(object, canon).unwrap(),
                        defaults: items
                            .iter()
                            .map(|item| (item.clone(), defaults_of(defaults, item)))
                            .collect(),
                        items,
                    })
                }
                "neoforge:compound" => Tree::Compound(kids("children")),
                "neoforge:intersection" => Tree::Intersection(kids("children")),
                "neoforge:difference" => Tree::Difference(
                    Box::new(tree_of(&spec["base"], defaults)),
                    Box::new(tree_of(&spec["subtracted"], defaults)),
                ),
                other => panic!("tipo {other}"),
            }
        }
        other => panic!("ingrediente {other}"),
    }
}

/// A classificação do rastro (`ItemInputCache.mapIngredient`) de uma árvore.
fn label(tree: &Tree) -> &'static str {
    match tree {
        Tree::Plain(_) => "types",
        Tree::Components(leaf) if leaf.ingredient.is_strict() => "exact",
        Tree::Compound(_) => "compound",
        _ => "complex",
    }
}

fn patch_key_of(input: &dyn TypedInstance) -> Vec<(String, String)> {
    match input.components_key() {
        ComponentsKey::Patch(key) => {
            PATCHES.with(|table| table.borrow().get(key).cloned().unwrap_or_default())
        }
        ComponentsKey::Default | ComponentsKey::Unmatchable => Vec::new(),
    }
}

fn ingredient(spec: &Value, defaults: &Rc<Defaults>) -> ShapedIngredient {
    let tree = Rc::new(tree_of(spec, defaults));
    let shape = tree.shape();
    ShapedIngredient::new(shape, move |input: &dyn TypedInstance| {
        tree.accepts(input.type_id(), &patch_key_of(input))
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
    let defaults = defaults_from(&lines);
    let (mut count, mut accepted, mut refused) = (0, 0, 0);
    for line in lines.iter().filter(|line| line["kind"] == "matrix") {
        count += 1;
        let tree = tree_of(&line["ingredient"], &defaults);
        let stack = &line["stack"];
        let full = full_map(&line["full"]);
        // A pilha tem os componentes dela por inteiro (`full`); o teste do porte recebe só o patch e põe os padrão por baixo.
        let base = defaults_of(&defaults, stack["id"].as_str().unwrap());
        let mut patch: Vec<(String, String)> = full
            .iter()
            .filter(|(kind, value)| base.get(*kind) != Some(*value))
            .map(|(kind, value)| (kind.clone(), value.clone()))
            .collect();
        patch.extend(
            base.keys()
                .filter(|kind| !full.contains_key(*kind))
                .map(|kind| (format!("!{kind}"), String::new())),
        );
        let rust = stack["count"].as_u64().unwrap() >= 1
            && tree.accepts(stack["id"].as_str().unwrap(), &patch);
        assert_eq!(rust, line["test"].as_bool().unwrap(), "{line}");
        assert_eq!(rust, line["test_type"].as_bool().unwrap(), "{line}");
        assert_eq!(label(&tree), line["shape"].as_str().unwrap(), "{line}");
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
