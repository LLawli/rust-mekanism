//! Repete as perguntas gravadas de `InputRecipeCache.SingleItem` e `InputRecipeCache.ItemChemical` do Mekanism original
//! (`dev/referencia/rastros/recipe_cache.jsonl`, escrito por `recipeCacheOrder` em `MachineTraceTest.java`) contra o porte.
//!
//! Cada linha é um cenário: as receitas na ordem da lista e as perguntas com a resposta do Java. Em vários cenários a
//! primeira receita da lista que casa não é a que o mapa por item devolve: o teste confere as duas coisas, a resposta do
//! Java e que a ordem da lista (o que o `find_first` antigo fazia) erraria.
use machinery_basis_core::chemical::{ChemicalResource, TankContents};
use machinery_basis_core::containers::{ItemResource, SlotContents};
use machinery_basis_core::recipe_cache::{
    ChemicalInputCache, ComponentSensitiveInputCache, ComponentsKey, DoubleInputRecipe,
    DoubleInputRecipeCache, IngredientShape, InputIngredient, ShapedIngredient, SingleInputRecipe,
    SingleInputRecipeCache, TypedInstance,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

/// O `Chemical` vazio do Mekanism, que a `ChemicalInputCache` não indexa.
const EMPTY_CHEMICAL: &str = "empty";
/// Menos linhas do que isto é um rastro cortado.
const MINIMUM_CASES: usize = 10;
const MINIMUM_QUERIES: usize = 100;
/// Menos divergências do que isto é um rastro que não prova nada sobre a ordem.
const MINIMUM_DIVERGENCES: usize = 8;

fn components(repair_cost: &Value) -> Option<String> {
    repair_cost
        .as_i64()
        .map(|cost| format!("repair_cost={cost}"))
}

fn names(json: &Value) -> Vec<String> {
    json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

/// A forma que `ItemInputCache.mapIngredient` dá ao ingrediente de cada tipo de `Spec` do Java.
fn shape(spec: &Value) -> IngredientShape {
    let items = names(spec);
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    match spec["kind"].as_str().unwrap() {
        "simple" => IngredientShape::types(&items),
        "exact" => IngredientShape::exact(&items, components(&spec["rc"]).as_deref()),
        "partial" => IngredientShape::Custom,
        "compound" => IngredientShape::Compound(
            spec["children"]
                .as_array()
                .unwrap()
                .iter()
                .map(shape)
                .collect(),
        ),
        kind => panic!("ingrediente {kind}"),
    }
}

/// `ItemStackIngredient.testType`: o ingrediente do jogo testado com o item e os componentes, sem a quantidade. A entrada
/// pode ser pilha ou recurso: o `ItemStackIngredient` do Java testa a pilha que cria dela, e só os componentes contam.
fn accepts(spec: &Value, input: &dyn TypedInstance) -> bool {
    let listed = names(spec).iter().any(|item| item == input.type_id());
    let patch = components(&spec["rc"]);
    let has_patch = match input.components_key() {
        ComponentsKey::Patch(found) => Some(found) == patch.as_deref(),
        ComponentsKey::Default | ComponentsKey::Unmatchable => patch.is_none(),
    };
    match spec["kind"].as_str().unwrap() {
        "simple" => listed,
        "exact" | "partial" => listed && has_patch,
        "compound" => spec["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| accepts(child, input)),
        kind => panic!("ingrediente {kind}"),
    }
}

fn ingredient(spec: &Value) -> ShapedIngredient {
    let spec_for_test = spec.clone();
    ShapedIngredient::new(shape(spec), move |input: &dyn TypedInstance| {
        accepts(&spec_for_test, input)
    })
}

fn resource(stack: &Value) -> Option<ItemResource> {
    let id = stack["id"].as_str().unwrap();
    if id.is_empty() {
        return None;
    }
    Some(match components(&stack["rc"]) {
        Some(variant) => ItemResource::with_variant(id, 64, &variant),
        None => ItemResource::new(id, 64),
    })
}

fn contents(stack: &Value) -> SlotContents {
    SlotContents::new(resource(stack), stack["amount"].as_u64().unwrap() as u32)
}

fn chemical(stack: &Value) -> Option<ChemicalResource> {
    let id = stack["id"].as_str().unwrap();
    (!id.is_empty()).then(|| ChemicalResource::new(id))
}

fn tank(stack: &Value) -> TankContents {
    TankContents::new(chemical(stack), stack["amount"].as_u64().unwrap())
}

struct ItemRecipe {
    id: String,
    input: ShapedIngredient,
    count: u32,
}

impl SingleInputRecipe<SlotContents> for ItemRecipe {
    fn test(&self, input: &SlotContents) -> bool {
        self.input.test_type(input) && input.amount >= self.count
    }
}

struct PairRecipe {
    id: String,
    item: ShapedIngredient,
    count: u32,
    chemical: ShapedIngredient,
    amount: u64,
}

impl DoubleInputRecipe<SlotContents, TankContents> for PairRecipe {
    fn test(&self, item: &SlotContents, chemical: &TankContents) -> bool {
        self.item.test_type(item)
            && item.amount >= self.count
            && self.chemical.test_type(chemical)
            && chemical.amount >= self.amount
    }
}

fn result(query: &Value) -> Option<&str> {
    query["result"].as_str()
}

/// O que a lista de receitas daria: a primeira que casa, sem índice nenhum.
fn by_list<R>(
    recipes: &[Rc<R>],
    matches: impl Fn(&R) -> bool,
    id: impl Fn(&R) -> &str,
) -> Option<String> {
    recipes
        .iter()
        .find(|recipe| matches(recipe))
        .map(|recipe| id(recipe).to_owned())
}

#[derive(Default)]
struct Tally {
    queries: usize,
    divergences: Vec<String>,
}

fn boolean(case: &str, query: &Value, found: bool) {
    assert_eq!(Some(found), query["result"].as_bool(), "{case}: {query}");
}

type ItemCache = SingleInputRecipeCache<
    ItemRecipe,
    ShapedIngredient,
    SlotContents,
    ComponentSensitiveInputCache,
>;

fn run_single(case: &Value, tally: &mut Tally) {
    let recipes: Vec<Rc<ItemRecipe>> = case["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|recipe| {
            Rc::new(ItemRecipe {
                id: recipe["id"].as_str().unwrap().to_owned(),
                input: ingredient(&recipe["input"]),
                count: recipe["count"].as_u64().unwrap() as u32,
            })
        })
        .collect();
    let listed = recipes.clone();
    let cache: ItemCache = SingleInputRecipeCache::new(
        move || recipes.clone(),
        |recipe: &ItemRecipe| &recipe.input,
        ComponentSensitiveInputCache::new(),
    );
    let name = case["case"].as_str().unwrap();
    for query in case["queries"].as_array().unwrap() {
        tally.queries += 1;
        let stack = &query["stack"];
        let op = query["op"].as_str().unwrap();
        // `ItemStack` e `ItemResource` são as duas formas de `TypedInstance` que o Java testa.
        let as_stack = contents(stack);
        let as_resource = resource(stack);
        let rust: Option<String> = match op {
            "find_first" => cache
                .find_first_recipe(&as_stack)
                .map(|recipe| recipe.id.clone()),
            "contains" => {
                boolean(name, query, cache.contains_input(&as_resource));
                continue;
            }
            "contains_stack" => {
                boolean(name, query, cache.contains_input(&as_stack));
                continue;
            }
            "type_based" => cache
                .find_type_based_recipe(&as_resource)
                .map(|recipe| recipe.id.clone()),
            "type_based_stack" => cache
                .find_type_based_recipe(&as_stack)
                .map(|recipe| recipe.id.clone()),
            op => panic!("pergunta {op}"),
        };
        assert_eq!(rust.as_deref(), result(query), "{name}: {query}");
        // A ordem da lista: com a quantidade para `find_first`, só com o tipo para o resto.
        let list = match op {
            "find_first" => by_list(
                &listed,
                |recipe| recipe.test(&as_stack),
                |recipe| &recipe.id,
            ),
            _ => by_list(
                &listed,
                |recipe| recipe.input.test_type(&as_stack),
                |recipe| &recipe.id,
            ),
        };
        if list.as_deref() != result(query) {
            tally.divergences.push(format!("{name} {op}: {query}"));
        }
    }
}

type PairCache = DoubleInputRecipeCache<
    PairRecipe,
    ShapedIngredient,
    SlotContents,
    ComponentSensitiveInputCache,
    ShapedIngredient,
    TankContents,
    ChemicalInputCache,
>;

fn run_double(case: &Value, tally: &mut Tally) {
    let recipes: Vec<Rc<PairRecipe>> = case["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|recipe| {
            let chemical_id = recipe["chemical"].as_str().unwrap().to_owned();
            let for_test = chemical_id.clone();
            Rc::new(PairRecipe {
                id: recipe["id"].as_str().unwrap().to_owned(),
                item: ingredient(&recipe["item"]),
                count: recipe["count"].as_u64().unwrap() as u32,
                chemical: ShapedIngredient::new(
                    IngredientShape::types(&[&chemical_id]),
                    move |input: &dyn TypedInstance| input.type_id() == for_test,
                ),
                amount: recipe["amount"].as_u64().unwrap(),
            })
        })
        .collect();
    let listed = recipes.clone();
    let cache: PairCache = DoubleInputRecipeCache::new(
        move || recipes.clone(),
        |recipe: &PairRecipe| &recipe.item,
        ComponentSensitiveInputCache::new(),
        |recipe: &PairRecipe| &recipe.chemical,
        ChemicalInputCache::new(Some(EMPTY_CHEMICAL)),
    );
    let name = case["case"].as_str().unwrap();
    let nothing = serde_json::json!({"id": "", "rc": null, "amount": 0});
    for query in case["queries"].as_array().unwrap() {
        tally.queries += 1;
        let op = query["op"].as_str().unwrap();
        // Cada pergunta traz só as entradas de que precisa.
        let side = |key: &str| {
            if query[key].is_null() {
                nothing.clone()
            } else {
                query[key].clone()
            }
        };
        let (a, b) = (side("a"), side("b"));
        let (stack_a, stack_b) = (contents(&a), tank(&b));
        let (resource_a, resource_b) = (resource(&a), chemical(&b));
        let rust: Option<String> = match op {
            "contains_a" => {
                boolean(name, query, cache.contains_input_a(&resource_a));
                continue;
            }
            "contains_a_stack" => {
                boolean(name, query, cache.contains_input_a(&stack_a));
                continue;
            }
            "contains_b" => {
                boolean(name, query, cache.contains_input_b(&resource_b));
                continue;
            }
            "contains_ab" => {
                boolean(
                    name,
                    query,
                    cache.contains_input_ab(&resource_a, &resource_b),
                );
                continue;
            }
            "contains_ba" => {
                boolean(
                    name,
                    query,
                    cache.contains_input_ba(&resource_a, &resource_b),
                );
                continue;
            }
            "contains_ab_stack" => {
                boolean(name, query, cache.contains_input_ab(&stack_a, &stack_b));
                continue;
            }
            "contains_ba_stack" => {
                boolean(name, query, cache.contains_input_ba(&stack_a, &stack_b));
                continue;
            }
            "find_first" => cache
                .find_first_recipe(&stack_a, &stack_b)
                .map(|recipe| recipe.id.clone()),
            "find_first_b" => cache
                .find_first_recipe_by(&stack_a, &stack_b, false)
                .map(|recipe| recipe.id.clone()),
            "type_based" => cache
                .find_type_based_recipe(&resource_a, &resource_b, |_| true)
                .map(|recipe| recipe.id.clone()),
            "type_based_stack" => cache
                .find_type_based_recipe(&stack_a, &stack_b, |_| true)
                .map(|recipe| recipe.id.clone()),
            op => panic!("pergunta {op}"),
        };
        assert_eq!(rust.as_deref(), result(query), "{name}: {query}");
        if op == "find_first" {
            let list = by_list(
                &listed,
                |recipe| recipe.test(&stack_a, &stack_b),
                |recipe| &recipe.id,
            );
            if list.as_deref() != result(query) {
                tally.divergences.push(format!("{name} {op}: {query}"));
            }
        }
    }
}

#[test]
fn the_caches_answer_like_the_original() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/recipe_cache.jsonl");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let mut tally = Tally::default();
    let mut cases = 0;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let case: Value = serde_json::from_str(line).unwrap();
        cases += 1;
        match case["kind"].as_str().unwrap() {
            "single" => run_single(&case, &mut tally),
            "double" => run_double(&case, &mut tally),
            kind => panic!("tipo {kind}"),
        }
    }
    assert!(cases >= MINIMUM_CASES, "só {cases} cenários");
    assert!(
        tally.queries >= MINIMUM_QUERIES,
        "só {} perguntas",
        tally.queries
    );
    // O ponto do porte: nestes casos a ordem da lista daria outra receita do que o mapa do Java.
    assert!(
        tally.divergences.len() >= MINIMUM_DIVERGENCES,
        "só {} divergências entre a lista e o mapa:\n{}",
        tally.divergences.len(),
        tally.divergences.join("\n")
    );
    eprintln!(
        "{cases} cenários, {} perguntas, {} em que a ordem da lista erra:\n{}",
        tally.queries,
        tally.divergences.len(),
        tally.divergences.join("\n")
    );
}
