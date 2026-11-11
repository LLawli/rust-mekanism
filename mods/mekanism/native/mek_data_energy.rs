// Módulo de `crafting.rs`: a energia que a `assemble` de `mekanism:mek_data` (`MekanismShapedRecipe`) copia das entradas para o resultado. O Horizon monta o resultado da mesa de
// `crafting_shaped` só com o modelo da receita (`Template::create`), sem olhar a grade, então o MODIFY_VARIABLE do `result` de `Recipes::find_with` (que atende a mesa do jogador, a do
// convidado de LAN e o crafter) chama `assemble` com o nome da receita, a grade e o resultado. A regra é a de `machinery_basis_core::recipe_data` (`EnergyRecipeData`: a soma saturada
// das energias das entradas que têm algo, e o que cabe na capacidade do resultado; o excesso se perde), repetida aqui porque este arquivo é do `horizon-client-lib` e não vê a `core`.
use crate::items::ItemStack;
use horizon_world::nbt::Tag;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

/// `MekanismDataComponents.ATTACHED_ENERGY`.
const ENERGY: &str = "mekanism:energy";

/// Os nomes das receitas `mekanism:mek_data`, como `from_sources` os lê (a troca de tipo da `mek_data_alias` os anota).
static RECIPES: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// A capacidade de energia de cada item que guarda energia (`ContainerType.ENERGY.getCapOrUnexposed`): quem tem entrada aqui guarda energia.
static CAPACITIES: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

thread_local! {
    /// A grade que o primeiro HEAD de `find_with` guardou, se ela tem algum item com energia.
    static GRID: std::cell::RefCell<Option<Vec<Option<ItemStack>>>> = const { std::cell::RefCell::new(None) };
}

/// Guarda a grade (para o segundo HEAD) se algum item dela guarda energia, e larga a de antes se não.
pub fn stash(grid: &[Option<ItemStack>]) {
    let keep = grid.iter().flatten().any(|stack| energy_of(stack) != 0);
    GRID.with_borrow_mut(|slot| *slot = keep.then(|| grid.to_vec()));
}

/// A grade guardada, uma vez.
pub fn take_stash() -> Option<Vec<Option<ItemStack>>> {
    GRID.with_borrow_mut(Option::take)
}

/// Uma carga nova das receitas esquece as anteriores.
pub fn forget() {
    RECIPES.lock().unwrap_or_else(|error| error.into_inner()).clear();
}

/// Anota que a receita `name` é do tipo `mekanism:mek_data`.
pub fn register(name: &str) {
    RECIPES.lock().unwrap_or_else(|error| error.into_inner()).insert(name.to_owned());
}

/// Diz que o item `item` guarda energia, até `capacity`.
pub fn set_capacity(item: &str, capacity: i64) {
    CAPACITIES.lock().unwrap_or_else(|error| error.into_inner()).get_or_insert_with(HashMap::new).insert(item.to_owned(), capacity);
}

/// `ContainerType.ENERGY.getOrZero`: o `long` do componente, ou zero.
fn energy_of(stack: &ItemStack) -> i64 {
    match stack.data().and_then(|data| data.other(ENERGY)) {
        Some(Tag::Long(value)) => (*value).max(0),
        Some(Tag::Int(value)) => i64::from(*value).max(0),
        _ => 0,
    }
}

/// `MekanismShapedRecipe.assemble` na parte da energia: se a receita é `mek_data`, soma a energia das entradas que guardam energia e a põe no resultado, se ele guarda, limitada
/// ao que cabe (`min(capacidade - guardado, soma)`, só se positivo). O resultado sai sempre, com o excesso perdido (`applyToStack` não falha por isso).
pub fn assemble(name: &str, grid: &[Option<ItemStack>], result: Option<ItemStack>) -> Option<ItemStack> {
    let mut result = result?;
    if !RECIPES.lock().unwrap_or_else(|error| error.into_inner()).contains(name) {
        return Some(result);
    }
    let capacities = CAPACITIES.lock().unwrap_or_else(|error| error.into_inner());
    let Some(capacities) = capacities.as_ref() else {
        return Some(result);
    };
    let Some(capacity) = capacities.get(&*result.item).copied() else {
        return Some(result);
    };
    let mut inputs = grid.iter().flatten().filter(|stack| capacities.contains_key(&*stack.item)).map(energy_of).filter(|energy| *energy != 0);
    let Some(first) = inputs.next() else {
        return Some(result);
    };
    let total = inputs.fold(first, |sum, energy| sum.checked_add(energy).unwrap_or(i64::MAX));
    let stored = energy_of(&result);
    let to_add = (capacity - stored).min(total);
    if to_add > 0 {
        result.data_mut().set_other(ENERGY, Some(Tag::Long(stored + to_add)));
        result.tidy();
    }
    Some(result)
}
