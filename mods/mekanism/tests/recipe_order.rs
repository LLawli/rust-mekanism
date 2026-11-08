//! Repete, nas máquinas de verdade, as perguntas do índice de receitas que o Mekanism original respondeu
//! (`dev/referencia/rastros/recipe_cache.jsonl`, escrito por `recipeCacheOrder` em `MachineTraceTest.java`).
//!
//! O teste do índice sozinho (`machinery_basis_core/tests/recipe_cache_reference.rs`) confere o que ele devolve. Aqui o que se
//! confere é o que a máquina faz com isso: a Enrichment Chamber e o Metallurgic Infuser montados como o jogo os monta
//! (`electric_config`, `infuser_config`), com a receita escolhida pelo `getRecipe` do tile, e as regras de inserção dos slots e do
//! tanque. Em vários casos a primeira receita da lista que casa não é a que o mapa do Java devolve: o teste só vale se a lista
//! daria outra receita, e confere isso.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{
    ChemicalConversion, ChemicalIngredient, ChemicalResource, TankContents, can_fill_or_convert,
};
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{AutomationType, ItemResource, SlotContents};
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::recipe_cache::{
    ComponentsKey, IngredientShape, InputIngredient, ShapedIngredient, TypedInstance,
};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use mekanism_rules::{
    ENRICHMENT_USAGE, INFUSER_USAGE, electric_config, infuser_config, infusing_cache_of,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

/// Menos do que isto é um rastro cortado, ou um rastro sem os casos que a ordem da lista erra.
const MINIMUM_RUNS: usize = 7;
const MINIMUM_TYPE_BASED: usize = 4;
/// Uma operação leva `TICKS_REQUIRED` ticks: o resto é folga.
const EXTRA_TICKS: u32 = 10;
const STACK: u32 = 64;

fn components(repair_cost: &Value) -> Option<String> {
    repair_cost
        .as_i64()
        .map(|cost| format!("repair_cost={cost}"))
}

/// A chave de componentes do item de `repair_cost`: a que o `ItemResource` dá ao patch que `with_variant` monta.
fn patch_key(repair_cost: &Value) -> Option<String> {
    components(repair_cost).map(|variant| Components::variant(&variant).key().to_owned())
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
        "exact" => IngredientShape::exact(&items, patch_key(&spec["rc"]).as_deref()),
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

/// `ItemStackIngredient.testType`: o ingrediente testado com o item e os componentes, sem a quantidade.
fn accepts(spec: &Value, input: &dyn TypedInstance) -> bool {
    let listed = names(spec).iter().any(|item| item == input.type_id());
    let patch = patch_key(&spec["rc"]);
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

fn ingredient(spec: &Value) -> ItemIngredient {
    let spec_for_test = spec.clone();
    ItemIngredient::shaped(ShapedIngredient::new(
        shape(spec),
        move |input: &dyn TypedInstance| accepts(&spec_for_test, input),
    ))
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, STACK)
}

/// O item da pergunta, com o componente que o distingue (`repair_cost`). Sem id é a ausência dele.
fn resource(stack: &Value) -> Option<ItemResource> {
    let id = stack["id"].as_str().unwrap();
    if id.is_empty() {
        return None;
    }
    Some(match components(&stack["rc"]) {
        Some(variant) => ItemResource::with_variant(id, STACK, &variant),
        None => item(id),
    })
}

fn contents(stack: &Value) -> SlotContents {
    SlotContents::new(resource(stack), stack["amount"].as_u64().unwrap() as u32)
}

fn chemical(stack: &Value) -> Option<ChemicalResource> {
    let id = stack["id"].as_str().unwrap();
    (!id.is_empty()).then(|| ChemicalResource::new(id))
}

fn out(id: &str) -> ItemResource {
    item(&format!("out_{id}"))
}

fn output(id: &str) -> ItemOutput {
    ItemOutput {
        resource: out(id),
        count: 1,
    }
}

fn expected(query: &Value) -> Option<&str> {
    query["result"].as_str()
}

fn boolean(case: &str, query: &Value) -> bool {
    query["result"]
        .as_bool()
        .unwrap_or_else(|| panic!("{case}: {query}"))
}

/// Dá energia cheia e anda a máquina até a operação acabar.
fn run(machine: &mut ProgressMachine) {
    let ticks = machine.ticks_required() as u32 + EXTRA_TICKS;
    for tick in 0..u64::from(ticks) {
        machine
            .energy()
            .set_energy(machine.energy().capacity(), None);
        machine.tick_server(tick, &NoNeighbours);
    }
}

fn made(machine: &ProgressMachine) -> Option<String> {
    machine
        .output()
        .resource()
        .map(|resource| resource.id.to_string())
}

fn accepted_by_the_slot(machine: &ProgressMachine, item: &ItemResource) -> bool {
    machine
        .input()
        .is_valid_for_insertion(item, AutomationType::External)
}

#[derive(Default)]
struct Tally {
    /// Quantas vezes a máquina andou com uma receita que a lista não escolheria.
    runs_against_the_list: usize,
    /// O mesmo para a pergunta só pelo tipo, que a regra do slot de infusão faz.
    type_based_against_the_list: usize,
    asked: usize,
}

struct ItemRecipe {
    id: String,
    input: ItemIngredient,
    count: u32,
}

fn item_recipes(case: &Value) -> Vec<ItemRecipe> {
    case["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|recipe| ItemRecipe {
            id: recipe["id"].as_str().unwrap().to_owned(),
            input: ingredient(&recipe["input"]),
            count: recipe["count"].as_u64().unwrap() as u32,
        })
        .collect()
}

/// A Enrichment Chamber com as receitas do cenário, cada uma dando um item que diz qual foi.
fn chamber(recipes: &[ItemRecipe]) -> ProgressMachine {
    let recipes = recipes
        .iter()
        .map(|recipe| {
            Rc::new(ItemToItemRecipe {
                id: recipe.id.clone(),
                input: recipe.input.clone(),
                input_count: recipe.count,
                output: output(&recipe.id),
            })
        })
        .collect();
    ProgressMachine::electric(electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(recipes),
        0,
        common::upgrade_data(),
    ))
}

/// Uma conversão por receita do cenário, que dá o químico `conv_<id>`: o químico diz qual receita o índice escolheu.
fn conversions(recipes: &[ItemRecipe]) -> Vec<ChemicalConversion> {
    recipes
        .iter()
        .map(|recipe| ChemicalConversion {
            id: String::new(),
            input: recipe.input.clone(),
            input_count: recipe.count,
            output: ChemicalResource::new(&format!("conv_{}", recipe.id)),
            amount: 1,
        })
        .collect()
}

/// O Metallurgic Infuser: as receitas de item e químico, e as conversões do slot de infusão. Todo `conv_<id>` entra numa receita
/// de infusão, porque o tanque só aceita o que alguma receita usa.
fn infuser(
    pairs: Vec<Rc<ItemChemicalToItemRecipe>>,
    conversions: Vec<ChemicalConversion>,
) -> ProgressMachine {
    ProgressMachine::chemical(infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(pairs),
        ChemicalConversion::cache_of(conversions),
        0,
        common::upgrade_data(),
    ))
}

fn pair_recipe(
    id: &str,
    input: ItemIngredient,
    count: u32,
    chemical: &str,
    amount: u32,
) -> Rc<ItemChemicalToItemRecipe> {
    Rc::new(ItemChemicalToItemRecipe {
        id: id.to_owned(),
        item_input: input,
        item_count: count,
        chemical_input: ChemicalIngredient::chemical(chemical),
        chemical_amount: amount,
        output: output(id),
        per_tick_usage: false,
    })
}

fn run_single(case: &Value, tally: &mut Tally) {
    let name = case["case"].as_str().unwrap();
    let recipes = item_recipes(case);
    for query in case["queries"].as_array().unwrap() {
        let op = query["op"].as_str().unwrap();
        let stack = &query["stack"];
        match op {
            // `getRecipe`: a máquina anda com a receita que o índice achou.
            "find_first" => {
                tally.asked += 1;
                let mut machine = chamber(&recipes);
                machine.input().set_contents(
                    resource(stack),
                    stack["amount"].as_u64().unwrap() as u32,
                    None,
                );
                run(&mut machine);
                let result = made(&machine);
                let wanted = expected(query).map(|id| format!("out_{id}"));
                assert_eq!(result, wanted, "{name}: {query}");
                // A ordem da lista daria outra receita: é o que este teste existe para pegar.
                let contents = contents(stack);
                let by_list = recipes
                    .iter()
                    .find(|recipe| {
                        contents.resource.is_some()
                            && recipe.input.test_type(&contents)
                            && contents.amount >= recipe.count
                    })
                    .map(|recipe| format!("out_{}", recipe.id));
                if by_list != wanted {
                    tally.runs_against_the_list += 1;
                }
            }
            // `InputInventorySlot.at(this::containsRecipe, ..)`: o item só entra se alguma receita o usa.
            "contains" => {
                tally.asked += 1;
                let machine = chamber(&recipes);
                let answer = boolean(name, query);
                match resource(stack) {
                    Some(resource) => {
                        assert_eq!(
                            accepted_by_the_slot(&machine, &resource),
                            answer,
                            "{name}: {query}"
                        );
                    }
                    None => assert!(!answer, "{name}: {query}"),
                }
            }
            // `ChemicalInventorySlot.canFillOrConvert`: a primeira conversão do tipo (`findTypeBasedRecipe`), sem a quantidade,
            // dá o químico que o tanque aceita. O tanque guarda o químico de cada receita por vez: só o da que o Java achou
            // deixa o item entrar, e sem receita nenhum deixa.
            "type_based" => {
                tally.asked += 1;
                let all = conversions(&recipes);
                let machine = infuser(registered(&all), all);
                let side = machine.chemical_side().unwrap();
                let Some(resource) = resource(stack) else {
                    continue;
                };
                for recipe in &recipes {
                    let held = ChemicalResource::new(&format!("conv_{}", recipe.id));
                    side.tank.set_contents(Some(held), 1, None);
                    let can = can_fill_or_convert(&side.tank, &side.conversions, &resource);
                    assert_eq!(
                        can,
                        expected(query) == Some(recipe.id.as_str()),
                        "{name} {}: {query}",
                        recipe.id
                    );
                }
                let by_list = recipes
                    .iter()
                    .find(|recipe| recipe.input.matches(&resource));
                if by_list.map(|recipe| recipe.id.as_str()) != expected(query) {
                    tally.type_based_against_the_list += 1;
                }
            }
            // As perguntas só com a pilha (`ItemStack`) não passam por slot nenhum: o índice já as responde no teste dele.
            _ => {}
        }
    }
    // O infuser também converte: o item no slot de infusão vira o químico da primeira receita que o índice acha (`findFirstRecipe`).
    for query in case["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| query["op"] == "find_first")
    {
        let all = conversions(&recipes);
        let mut machine = infuser(registered(&all), all);
        let stack = &query["stack"];
        let side = machine.chemical_side().unwrap();
        side.slot.set_contents(
            resource(stack),
            stack["amount"].as_u64().unwrap() as u32,
            None,
        );
        machine.tick_server(0, &NoNeighbours);
        let side = machine.chemical_side().unwrap();
        let converted = side.tank.resource().map(|resource| resource.id.to_string());
        let wanted = expected(query).map(|id| format!("conv_{id}"));
        assert_eq!(converted, wanted, "{name} (infusão): {query}");
    }
}

/// As receitas de infusão que registram os químicos `conv_<id>` no tanque: o item é um que nenhum teste usa.
fn registered(conversions: &[ChemicalConversion]) -> Vec<Rc<ItemChemicalToItemRecipe>> {
    conversions
        .iter()
        .map(|conversion| {
            pair_recipe(
                &format!("registers_{}", conversion.output.id),
                ItemIngredient::item("unused"),
                1,
                &conversion.output.id,
                1,
            )
        })
        .collect()
}

struct PairRecipe {
    id: String,
    item: ItemIngredient,
    count: u32,
    chemical: String,
    amount: u32,
}

fn run_double(case: &Value, tally: &mut Tally) {
    let name = case["case"].as_str().unwrap();
    let recipes: Vec<PairRecipe> = case["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|recipe| PairRecipe {
            id: recipe["id"].as_str().unwrap().to_owned(),
            item: ingredient(&recipe["item"]),
            count: recipe["count"].as_u64().unwrap() as u32,
            chemical: recipe["chemical"].as_str().unwrap().to_owned(),
            amount: recipe["amount"].as_u64().unwrap() as u32,
        })
        .collect();
    let machine_with = || {
        infuser(
            recipes
                .iter()
                .map(|recipe| {
                    pair_recipe(
                        &recipe.id,
                        recipe.item.clone(),
                        recipe.count,
                        &recipe.chemical,
                        recipe.amount,
                    )
                })
                .collect(),
            Vec::new(),
        )
    };
    let nothing = serde_json::json!({"id": "", "rc": null, "amount": 0});
    for query in case["queries"].as_array().unwrap() {
        let op = query["op"].as_str().unwrap();
        let side = |key: &str| {
            if query[key].is_null() {
                nothing.clone()
            } else {
                query[key].clone()
            }
        };
        let (a, b) = (side("a"), side("b"));
        let tank_amount = b["amount"].as_u64().unwrap();
        match op {
            "find_first" => {
                tally.asked += 1;
                let mut machine = machine_with();
                machine.input().set_contents(
                    resource(&a),
                    a["amount"].as_u64().unwrap() as u32,
                    None,
                );
                let side = machine.chemical_side().unwrap();
                side.tank.set_contents(chemical(&b), tank_amount, None);
                run(&mut machine);
                let wanted = expected(query).map(|id| format!("out_{id}"));
                assert_eq!(made(&machine), wanted, "{name}: {query}");
                let stack = contents(&a);
                let held = TankContents::new(chemical(&b), tank_amount);
                let by_list = recipes
                    .iter()
                    .find(|recipe| {
                        stack.resource.is_some()
                            && recipe.item.test_type(&stack)
                            && stack.amount >= recipe.count
                            && held
                                .resource
                                .as_ref()
                                .is_some_and(|held| *held.id == *recipe.chemical)
                            && held.amount >= u64::from(recipe.amount)
                    })
                    .map(|recipe| format!("out_{}", recipe.id));
                if by_list != wanted {
                    tally.runs_against_the_list += 1;
                }
            }
            // `InputInventorySlot.at((itemType, _) -> containsRecipeAB(itemType, infusionTank.resource()), this::containsRecipeA,
            // ..)`: com o tanque no estado da pergunta. Um item que nenhuma receita usa (`containsRecipeA`) não entra, qualquer
            // que seja o resto.
            "contains_a" => {
                tally.asked += 1;
                let machine = machine_with();
                let Some(resource) = resource(&a) else {
                    assert!(!boolean(name, query), "{name}: {query}");
                    continue;
                };
                assert_eq!(
                    machine.input().is_valid(&resource),
                    boolean(name, query),
                    "{name}: {query}"
                );
            }
            "contains_ab" => {
                tally.asked += 1;
                let machine = machine_with();
                let side = machine.chemical_side().unwrap();
                side.tank.set_contents(chemical(&b), tank_amount, None);
                let Some(resource) = resource(&a) else {
                    continue;
                };
                // Só vale onde o `containsRecipeA` deixa: sem ele o item não entra por regra nenhuma.
                if machine.input().is_valid(&resource) {
                    assert_eq!(
                        accepted_by_the_slot(&machine, &resource),
                        boolean(name, query),
                        "{name}: {query}"
                    );
                }
            }
            // O tanque: `(chemicalType, _) -> containsRecipeBA(inputSlot.resource(), chemicalType)` e `containsRecipeB`.
            "contains_b" => {
                tally.asked += 1;
                let machine = machine_with();
                let Some(chemical) = chemical(&b) else {
                    assert!(!boolean(name, query), "{name}: {query}");
                    continue;
                };
                let side = machine.chemical_side().unwrap();
                assert_eq!(
                    side.tank.is_valid(&chemical),
                    boolean(name, query),
                    "{name}: {query}"
                );
            }
            "contains_ba" => {
                tally.asked += 1;
                let machine = machine_with();
                machine.input().set_contents(
                    resource(&a),
                    a["amount"].as_u64().unwrap() as u32,
                    None,
                );
                let Some(chemical) = chemical(&b) else {
                    continue;
                };
                let side = machine.chemical_side().unwrap();
                if side.tank.is_valid(&chemical) {
                    assert_eq!(
                        side.tank
                            .is_valid_for_insertion(&chemical, AutomationType::External),
                        boolean(name, query),
                        "{name}: {query}"
                    );
                }
            }
            _ => {}
        }
    }
}

#[test]
fn the_machines_choose_and_accept_like_the_original() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/recipe_cache.jsonl");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let mut tally = Tally::default();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let case: Value = serde_json::from_str(line).unwrap();
        match case["kind"].as_str().unwrap() {
            "single" => run_single(&case, &mut tally),
            "double" => run_double(&case, &mut tally),
            kind => panic!("tipo {kind}"),
        }
    }
    assert!(tally.asked >= 100, "só {} perguntas", tally.asked);
    // O ponto: nestes casos a lista daria outra receita do que o Java escolheu, e a máquina escolhe a do Java.
    assert!(
        tally.runs_against_the_list >= MINIMUM_RUNS,
        "só {} execuções em que a lista erra",
        tally.runs_against_the_list
    );
    assert!(
        tally.type_based_against_the_list >= MINIMUM_TYPE_BASED,
        "só {} perguntas por tipo em que a lista erra",
        tally.type_based_against_the_list
    );
    eprintln!(
        "{} perguntas, {} execuções e {} perguntas por tipo em que a ordem da lista erra",
        tally.asked, tally.runs_against_the_list, tally.type_based_against_the_list
    );
}

/// Um Infuser com uma receita de item e químico, a de 1 de químico por operação.
fn single_pair_infuser(per_tick_usage: bool) -> ProgressMachine {
    let recipe = Rc::new(ItemChemicalToItemRecipe {
        per_tick_usage,
        ..Rc::unwrap_or_clone(pair_recipe(
            "mekanism:metallurgic_infusing/test",
            ItemIngredient::item("dirt"),
            1,
            "fungi",
            1,
        ))
    });
    infuser(vec![recipe], Vec::new())
}

fn tick_for(machine: &mut ProgressMachine, from: u64, ticks: u64) {
    for tick in from..from + ticks {
        machine
            .energy()
            .set_energy(machine.energy().capacity(), None);
        machine.tick_server(tick, &NoNeighbours);
    }
}

/// `createNewCachedRecipe`: `recipe.perTickUsage()` escolhe `ItemStackConstantChemicalToObjectCachedRecipe`, que gasta o
/// químico um pouco a cada tick e guarda o `usedSoFar` no tile; sem ele é o `TwoInputCachedRecipe`, que gasta tudo no fim. Não
/// há rastro do original para o Infuser com a opção ligada (os arquivos dele a deixam desligada): os números seguem a mesma
/// `constantUse` que o `compressor_trace` confere.
#[test]
fn per_tick_usage_picks_the_constant_usage_recipe() {
    for per_tick_usage in [true, false] {
        let mut machine = single_pair_infuser(per_tick_usage);
        machine.input().set_contents(Some(item("dirt")), 1, None);
        machine.chemical_side().unwrap().tank.set_contents(
            Some(ChemicalResource::new("fungi")),
            300,
            None,
        );
        let total = machine.ticks_required() as u64;
        tick_for(&mut machine, 0, 10);
        let tank = machine.chemical_side().unwrap().tank.amount();
        let used = machine.saved_state().used_so_far;
        if per_tick_usage {
            assert!(
                tank < 300 && tank > 280,
                "gastou {} em 10 ticks",
                300 - tank
            );
            assert!(used > 0, "usedSoFar {used}");
            assert_eq!(300 - tank, used as u64);
        } else {
            assert_eq!((tank, used), (300, 0));
        }
        tick_for(&mut machine, 10, total + 5);
        assert_eq!(
            made(&machine).as_deref(),
            Some("out_mekanism:metallurgic_infusing/test")
        );
        // `ChemicalUsageMultiplier.constantUse(() -> ticksRequired, ..)`: gasta `ticksRequired` vezes a quantidade da receita.
        let spent = if per_tick_usage { total } else { 1 };
        assert_eq!(machine.chemical_side().unwrap().tank.amount(), 300 - spent);
        assert_eq!(machine.saved_state().used_so_far, 0);
    }
}

/// O slot de entrada pergunta pelo tanque e o tanque pelo slot (`containsRecipeAB` e `containsRecipeBA`): se um segurasse o
/// outro com força, uma máquina que o jogo larga nunca seria liberada.
#[test]
fn a_dropped_infuser_frees_its_input_slot_and_tank() {
    let machine = single_pair_infuser(false);
    let input = machine.input().downgrade();
    assert!(input.upgrade().is_some());
    drop(machine);
    assert!(input.upgrade().is_none());
}
