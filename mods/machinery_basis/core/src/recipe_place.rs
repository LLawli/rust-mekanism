//! A colocação de uma receita pelo livro de receitas, no servidor (`mekanism.common.lib.recipetransfer`: `PlaceMekanismRecipe` e
//! `RecipeIngredientResolver`, chamados por `MekanismContainer.handlePlacement`, que é o que o `handlePlaceRecipe` do vanilla chama).
//!
//! Portado linha a linha, com as transações do NeoForge como cópias do estado: o que o original faz dentro de uma transação que ele
//! confirma fica, e o que faz numa que ele abandona some. A transação raiz é confirmada sempre (`handlePlacement`), também quando a
//! colocação falha no meio: é por isso que itens somem no original (ver "Comportamento do original que parece bug e fica" no plano), e
//! aqui somem igual.
//!
//! A lib não conhece nenhum item: um recurso é uma chave de texto (o id do item, mais `#` e o que distingue os componentes quando a
//! pilha os tem), e as regras dos slots chegam por [`Rules`].

use crate::identity_hash::identity_hash;
use crate::sim::java_hash::hash_set_order;
use crate::sim::open_hash::{OpenHashMap, OpenKey, mix_int};

/// Uma pilha: o recurso e a quantidade.
pub type Stack = Option<(String, i32)>;

/// `Inventory.INVENTORY_SIZE`: os slots principais do jogador (`PlayerInventoryWrapper.getMainSlots`).
pub const MAIN_SLOTS: usize = 36;
/// `Item.ABSOLUTE_MAX_STACK_SIZE`, a capacidade de um `BasicInventorySlot`.
pub const SLOT_CAPACITY: i32 = 99;

/// O botão de menu que leva o `ServerboundPlaceRecipePacket(containerId, recipe, useMaxItems)` ao servidor: o contêiner é o menu
/// aberto, e o id do botão guarda o display e o shift acima desta base (os botões das telas ficam bem abaixo dela).
pub const PLACE_BUTTON_BASE: i32 = 0x1000_0000;

/// O id do botão de menu que pede a colocação do display `id`.
pub fn place_button(id: i32, use_max: bool) -> i32 {
    PLACE_BUTTON_BASE + ((id << 1) | i32::from(use_max))
}

/// O display e o shift de um botão de menu de colocação; `None` para os outros botões.
pub fn place_button_request(button: i32) -> Option<(i32, bool)> {
    let packed = button
        .checked_sub(PLACE_BUTTON_BASE)
        .filter(|packed| (0..PLACE_BUTTON_BASE).contains(packed))?;
    Some((packed >> 1, packed & 1 != 0))
}

/// O id do item de um recurso (a chave sem os componentes).
pub fn item_of(resource: &str) -> &str {
    resource.split('#').next().unwrap_or(resource)
}

/// `ItemResource.hashCode()` (`ItemStack.hashItemAndComponents`: `31 * (31 + item.hashCode()) + components.hashCode()`). O `hashCode`
/// do item é o de identidade do JVM e o dos componentes soma hashes de identidade (as chaves dos componentes, enums, textos): o valor é
/// um inteiro de 32 bits que muda a cada execução e é o mesmo durante ela. Aqui cada recurso sorteia o seu uma vez por processo.
pub fn item_resource_hash(resource: &str) -> i32 {
    let item = identity_hash(&format!("item:{}", item_of(resource)));
    let high = identity_hash(&format!("item-components-a:{resource}"));
    let low = identity_hash(&format!("item-components-b:{resource}"));
    let components = (high << 1) ^ low;
    31_i32
        .wrapping_mul(31_i32.wrapping_add(item))
        .wrapping_add(components)
}

/// O que a colocação lê e muda: o inventário do jogador, os slots de item da máquina e o que cai no chão.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// Os 36 slots principais.
    pub main: Vec<Stack>,
    /// A outra mão (`Inventory.SLOT_OFFHAND`).
    pub offhand: Stack,
    /// O slot escolhido da barra (`Inventory.getSelectedSlot`).
    pub selected: usize,
    /// Os slots de item da máquina.
    pub slots: Vec<Stack>,
    /// O que o jogador largou (`PlayerInventoryWrapper.drop`), na ordem.
    pub dropped: Vec<(String, i32)>,
}

/// As regras dos slots da máquina e dos itens, que são do mod.
pub trait Rules {
    /// `ItemResource.getMaxStackSize`.
    fn max_stack(&self, resource: &str) -> i32;
    /// `isValid` do slot (o validador: a máquina tem receita com o item).
    fn valid(&self, slot: usize, resource: &str) -> bool;
    /// `canInsert` do slot com `AutomationType.MANUAL`.
    fn can_insert(&self, slot: usize, resource: &str) -> bool;
    /// `canExtract` do slot com `AutomationType.MANUAL`.
    fn can_extract(&self, slot: usize, resource: &str) -> bool;
    /// `ItemResource.hashCode()`: a ordem do `HashMap` do `RecipeIngredientResolver` sai dele.
    fn hash(&self, resource: &str) -> i32;
}

/// Um `ItemStackIngredient`: os itens que ele aceita (`testType`) e a quantidade (`ingredient().count()`).
#[derive(Clone, Copy, Debug)]
pub struct Ingredient<'a> {
    pub items: &'a [&'a str],
    pub count: i32,
}

impl Ingredient<'_> {
    fn test(&self, resource: &str) -> bool {
        self.items.contains(&item_of(resource))
    }
}

/// O tanque de químico de uma `ItemStackChemicalToObjectRecipe` na hora da colocação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tank {
    /// `inputTank.isEmpty()`.
    Empty,
    /// `!icRecipe.getChemicalInput().test(chemicalStack)`: outro químico, ou menos do que a receita pede.
    Mismatch,
    Matches,
}

/// A receita, pelo ramo do `switch` de `PlaceMekanismRecipe.placeRecipe`.
#[derive(Clone, Copy, Debug)]
pub enum Recipe<'a> {
    /// `ItemInputRecipe` numa máquina com um slot de entrada.
    Item(Ingredient<'a>),
    /// `ItemStackChemicalToObjectRecipe` numa máquina com um slot e um tanque.
    ItemChemical(Ingredient<'a>, Tank),
    /// Uma receita que não é `MekanismRecipe` (a `SmeltingRecipe` do vanilla que o livro do Energized Smelter lista):
    /// `MekanismContainer.handlePlacement` faz `(RECIPE) recipe.value()` e o JVM lança `ClassCastException`, que o servidor registra
    /// e engole (`ServerPacketListener.onPacketError`): nada muda e nenhum fantasma vai.
    Foreign,
}

/// `RecipeBookMenu.PostPlaceAction`, mais a exceção.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    PlaceGhostRecipe,
    /// A transação raiz fecha sem confirmar: nada muda.
    ClassCast,
}

/// Os slots de item dos `RecipeContainers`, por índice em [`State::slots`]: as entradas (na ordem dos ingredientes) e as saídas.
#[derive(Clone, Copy, Debug)]
pub struct Containers<'a> {
    pub inputs: &'a [usize],
    pub outputs: &'a [usize],
}

fn amount(stack: &Stack) -> i32 {
    stack.as_ref().map_or(0, |(_, count)| *count)
}

/// `BasicInventorySlot.capacityAsLong(resource)`: zero para o que o slot não aceita, senão o menor entre a capacidade e a pilha do item.
fn capacity(rules: &dyn Rules, slot: usize, resource: &str) -> i32 {
    if rules.valid(slot, resource) {
        SLOT_CAPACITY.min(rules.max_stack(resource))
    } else {
        0
    }
}

/// `IResourceContainer.getNeededAsInt(resource)` de um slot que guarda `resource`.
fn needed(state: &State, rules: &dyn Rules, slot: usize, resource: &str) -> i32 {
    (capacity(rules, slot, resource) - amount(&state.slots[slot])).max(0)
}

/// `BasicResourceContainer.insert` com `AutomationType.MANUAL`.
fn slot_insert(
    state: &mut State,
    rules: &dyn Rules,
    slot: usize,
    resource: &str,
    count: i32,
) -> i32 {
    if count == 0 {
        return 0;
    }
    if let Some((current, _)) = &state.slots[slot]
        && current != resource
    {
        return 0;
    }
    let stored = amount(&state.slots[slot]);
    let room = capacity(rules, slot, resource) - stored;
    if room <= 0 || !rules.can_insert(slot, resource) {
        return 0;
    }
    let added = count.min(room);
    state.slots[slot] = Some((resource.to_owned(), stored + added));
    added
}

/// `BasicResourceContainer.extract` com `AutomationType.MANUAL`.
fn slot_extract(
    state: &mut State,
    rules: &dyn Rules,
    slot: usize,
    resource: &str,
    count: i32,
) -> i32 {
    let Some((current, stored)) = state.slots[slot].clone() else {
        return 0;
    };
    if count == 0 || current != resource || !rules.can_extract(slot, resource) {
        return 0;
    }
    let removed = count.min(stored);
    state.slots[slot] = (stored > removed).then(|| (current, stored - removed));
    removed
}

/// `ResourceHandler.extract(resource, amount, transaction)` dos slots principais: do primeiro ao último.
fn main_extract(state: &mut State, resource: &str, count: i32) -> i32 {
    let mut extracted = 0;
    for stack in state.main.iter_mut() {
        if extracted == count {
            break;
        }
        if let Some((current, stored)) = stack
            && current == resource
        {
            let take = (count - extracted).min(*stored);
            *stored -= take;
            extracted += take;
            if *stored == 0 {
                *stack = None;
            }
        }
    }
    extracted
}

/// O `insert` de um slot do jogador: só no vazio ou no que guarda o mesmo recurso, até a pilha do item.
fn stack_insert(stack: &mut Stack, limit: i32, resource: &str, count: i32) -> i32 {
    match stack {
        Some((current, stored)) if current == resource => {
            let added = count.min((limit - *stored).max(0));
            *stored += added;
            added
        }
        Some(_) => 0,
        None => {
            let added = count.min(limit);
            if added > 0 {
                *stack = Some((resource.to_owned(), added));
            }
            added
        }
    }
}

/// `ResourceHandlerUtil.insertStacking` nos slots principais: primeiro os que já têm algo, depois os vazios. O que coube fica, mesmo
/// quando não coube tudo (a transação dele é confirmada).
fn main_insert_stacking(state: &mut State, rules: &dyn Rules, resource: &str, count: i32) -> i32 {
    if count == 0 {
        return 0;
    }
    let limit = rules.max_stack(resource);
    let mut inserted = 0;
    for empty in [false, true] {
        if inserted == count {
            break;
        }
        for stack in state.main.iter_mut() {
            if stack.is_none() == empty {
                inserted += stack_insert(stack, limit, resource, count - inserted);
                if inserted == count {
                    break;
                }
            }
        }
    }
    inserted
}

/// `RecipeBookPlacementHelper.sendItemsToPlayerInventory`: no criativo, `PlayerInventoryWrapper.placeItemBackInInventory` (a mão
/// principal e a outra mão quando guardam o recurso, depois os slots principais, e o resto cai no chão), que sempre dá certo; fora
/// dele, o `insertStacking` nos slots principais, que só dá certo quando tudo coube.
fn send_to_player(
    state: &mut State,
    rules: &dyn Rules,
    creative: bool,
    resource: &str,
    count: i32,
) -> bool {
    if !creative {
        return main_insert_stacking(state, rules, resource, count) == count;
    }
    let limit = rules.max_stack(resource);
    let mut inserted = 0;
    let selected = state.selected;
    if let Some(Some((current, _))) = state.main.get(selected)
        && current == resource
    {
        inserted += stack_insert(&mut state.main[selected], limit, resource, count);
    }
    if inserted < count
        && let Some((current, _)) = &state.offhand
        && current == resource
    {
        inserted += stack_insert(&mut state.offhand, limit, resource, count - inserted);
    }
    if inserted < count {
        inserted += main_insert_stacking(state, rules, resource, count - inserted);
    }
    if inserted < count {
        state.dropped.push((resource.to_owned(), count - inserted));
    }
    true
}

/// Uma chave das tabelas do fastutil (`Object2IntOpenHashMap<ItemResource>`, `Int2ObjectOpenHashMap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    index: usize,
    hash: i32,
}

impl OpenKey for Key {
    fn mixed(self) -> i32 {
        mix_int(self.hash)
    }
}

/// O que cada slot de entrada deve guardar (`Int2ObjectMap<ResourceStack<ItemResource>>`), na ordem de inserção.
type Solved = Vec<(usize, String, i32)>;

/// `RecipeIngredientResolver.solveFor`: tudo dentro de uma simulação, que é abandonada no fim.
fn solve_for(
    state: &State,
    rules: &dyn Rules,
    ingredients: &[Ingredient<'_>],
    slots: &[usize],
    use_max: bool,
) -> Solved {
    let count = ingredients.len();
    let mut sim = state.clone();
    let mut previous: Vec<(String, i32)> = Vec::new();
    for (&slot, ingredient) in slots.iter().zip(ingredients) {
        if let Some((current, stored)) = sim.slots[slot].clone() {
            let mut sub = sim.clone();
            let extractable = slot_extract(&mut sub, rules, slot, &current, stored);
            if extractable < stored || sub.slots[slot].is_some() {
                if ingredient.test(&current) {
                    continue;
                }
                return Vec::new();
            }
            match previous
                .iter_mut()
                .find(|(resource, _)| *resource == current)
            {
                Some((_, had)) => *had += stored,
                None => previous.push((current, stored)),
            }
            sim = sub;
        }
    }
    // `validIngredients`: um `HashMap<ItemResource, ResourceInfo>`, percorrido na ordem da tabela.
    let mut valid: Vec<(String, i32, Vec<usize>)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for player_slot in 0..sim.main.len().min(MAIN_SLOTS) {
        let Some((resource, _)) = sim.main[player_slot].clone() else {
            continue;
        };
        if seen.contains(&resource) {
            continue;
        }
        seen.push(resource.clone());
        let mut max = 0;
        let mut valid_for = Vec::new();
        for (i, (&slot, ingredient)) in slots.iter().zip(ingredients).enumerate() {
            if let Some((current, _)) = &sim.slots[slot] {
                if *current == resource {
                    valid_for.push(i);
                    max += needed(&sim, rules, slot, &resource);
                }
            } else if ingredient.test(&resource) {
                valid_for.push(i);
                max += capacity(rules, slot, &resource);
            }
        }
        if max > 0 {
            let had = previous
                .iter()
                .find(|(name, _)| *name == resource)
                .map_or(0, |(_, had)| *had);
            if had >= max {
                valid.push((resource.clone(), had, valid_for));
            } else {
                let extractable = main_extract(&mut sim, &resource, max - had) + had;
                if extractable > 0 {
                    valid.push((resource.clone(), extractable, valid_for));
                }
            }
            previous.retain(|(name, _)| *name != resource);
        }
    }
    let order = hash_set_order(0..valid.len(), |index| rules.hash(&valid[index].0), 16);
    let mut removed = vec![false; valid.len()];
    let mut remaining: Vec<usize> = (0..count).collect();
    let mut result: Solved = Vec::new();
    let put = |result: &mut Solved, index: usize, resource: &str, stored: i32| match result
        .iter_mut()
        .find(|(at, _, _)| *at == index)
    {
        Some(entry) => *entry = (index, resource.to_owned(), stored),
        None => result.push((index, resource.to_owned(), stored)),
    };
    loop {
        let mut removed_single = false;
        for &entry in &order {
            if removed[entry] {
                continue;
            }
            let (resource, available, valid_for) = valid[entry].clone();
            // O laço que tiraria de `elements` os índices já tratados percorre o próprio `remainingIndices` e nunca tira nada.
            let elements = valid_for.len();
            if elements == 0 {
                removed[entry] = true;
            } else if elements == 1 {
                let index = valid_for[0];
                let slot = slots[index];
                let ingredient = ingredients[index];
                let to_transfer;
                if use_max {
                    to_transfer = available;
                } else {
                    let stored = amount(&sim.slots[slot]);
                    to_transfer = ingredient.count - stored;
                    if to_transfer <= 0 {
                        put(&mut result, index, &resource, stored);
                        remaining.retain(|at| *at != index);
                        removed[entry] = true;
                        removed_single = true;
                        continue;
                    }
                }
                let mut sub = sim.clone();
                let inserted = slot_insert(&mut sub, rules, slot, &resource, to_transfer);
                let stored = amount(&sub.slots[slot]);
                if inserted < to_transfer && stored < ingredient.count {
                    continue;
                }
                sim = sub;
                put(&mut result, index, &resource, stored);
                remaining.retain(|at| *at != index);
                removed[entry] = true;
                removed_single = true;
            }
            if remaining.is_empty() {
                break;
            }
        }
        if !(removed_single && !remaining.is_empty() && removed.iter().any(|gone| !gone)) {
            break;
        }
    }
    let mut error = false;
    for &index in &remaining {
        let slot = slots[index];
        let stored = amount(&sim.slots[slot]);
        if stored < ingredients[index].count {
            error = true;
        } else if let Some((resource, _)) = sim.slots[slot].clone() {
            put(&mut result, index, &resource, stored);
        }
    }
    if error { Vec::new() } else { result }
}

/// A ordem do `Int2ObjectOpenHashMap` com as chaves postas na ordem dada (a chave 0 é a "nula" do fastutil e sai primeiro).
fn solved_order(solved: &Solved) -> Vec<usize> {
    let mut zero = None;
    let mut table: OpenHashMap<Key, usize> = OpenHashMap::new();
    for (at, (index, _, _)) in solved.iter().enumerate() {
        if *index == 0 {
            zero = Some(at);
        } else {
            table.insert(
                Key {
                    index: *index,
                    hash: *index as i32,
                },
                at,
            );
        }
    }
    zero.into_iter()
        .chain(table.entries_in_order().into_iter().map(|(_, at)| *at))
        .collect()
}

/// `PlaceMekanismRecipe.tryTransfer`, na transação raiz: devolve se deu certo. O que mudou antes de um erro fica.
fn try_transfer(
    state: &mut State,
    rules: &dyn Rules,
    slots: &[usize],
    solved: &Solved,
    creative: bool,
) -> bool {
    // `resourcesToReturn`: um `Object2IntOpenHashMap<ItemResource>`.
    let mut names: Vec<String> = Vec::new();
    let mut limbo: OpenHashMap<Key, i32> = OpenHashMap::new();
    let key_of = |names: &mut Vec<String>, resource: &str| {
        let index = names
            .iter()
            .position(|name| name == resource)
            .unwrap_or_else(|| {
                names.push(resource.to_owned());
                names.len() - 1
            });
        Key {
            index,
            hash: rules.hash(resource),
        }
    };
    let order = solved_order(solved);
    let mut kept: Vec<usize> = Vec::new();
    for &at in &order {
        let (index, resource, target) = &solved[at];
        let Some(&slot) = slots.get(*index) else {
            return false;
        };
        let mut keep = true;
        if let Some((current, stored)) = state.slots[slot].clone() {
            if current == *resource {
                if stored == *target {
                    keep = false;
                } else if stored > *target {
                    // `int toExtract = target - stored; if (toExtract <= 0) { warn; return helper.error(); }`: sempre.
                    return false;
                }
            } else {
                if slot_extract(state, rules, slot, &current, stored) < stored
                    || state.slots[slot].is_some()
                {
                    return false;
                }
                let key = key_of(&mut names, &current);
                let had = limbo.get(key).copied().unwrap_or(0);
                limbo.insert(key, had + stored);
            }
        }
        if keep {
            kept.push(at);
        }
    }
    for &at in &kept {
        let (index, resource, target) = &solved[at];
        let slot = slots[*index];
        let mut to_transfer = *target - amount(&state.slots[slot]);
        let key = key_of(&mut names, resource);
        if let Some(&in_limbo) = limbo.get(key) {
            let available = in_limbo.min(to_transfer);
            if slot_insert(state, rules, slot, resource, available) != available {
                return false;
            } else if to_transfer < in_limbo {
                limbo.insert(key, in_limbo - to_transfer);
                continue;
            }
            limbo.remove(key);
            if to_transfer == in_limbo {
                continue;
            }
            to_transfer -= in_limbo;
        }
        if main_extract(state, resource, to_transfer) < to_transfer
            || slot_insert(state, rules, slot, resource, to_transfer) != to_transfer
        {
            return false;
        }
    }
    let back: Vec<(String, i32)> = limbo
        .entries_in_order()
        .into_iter()
        .map(|(key, count)| (names[key.index].clone(), *count))
        .collect();
    for (resource, count) in back {
        if !send_to_player(state, rules, creative, &resource, count) {
            return false;
        }
    }
    true
}

/// `MekanismContainer.handlePlacement` com `PlaceMekanismRecipe.placeRecipe`: põe a receita nos slots de entrada com o que o jogador
/// tem (`use_max`: o shift) e, quando não dá, esvazia os slots de entrada e de saída para o fantasma aparecer. `creative` é o
/// `allowDroppingItemsToClear` (`player.isCreative()`).
pub fn place_recipe(
    state: &mut State,
    rules: &dyn Rules,
    recipe: &Recipe<'_>,
    containers: Containers<'_>,
    use_max: bool,
    creative: bool,
) -> Action {
    let ingredient = match recipe {
        Recipe::Foreign => return Action::ClassCast,
        Recipe::Item(ingredient) => Some(*ingredient),
        Recipe::ItemChemical(ingredient, Tank::Matches) => Some(*ingredient),
        // O tanque vazio ou com outro químico: `DEFAULT_ACTION`, o fantasma.
        Recipe::ItemChemical(..) => None,
    };
    let placed = ingredient.is_some_and(|ingredient| {
        let slots = &containers.inputs[..1];
        let solved = solve_for(state, rules, &[ingredient], slots, use_max);
        !solved.is_empty() && try_transfer(state, rules, slots, &solved, creative)
    });
    if placed {
        return Action::Nothing;
    }
    // O fantasma: os slots de item de entrada e de saída vão para o inventário do jogador, tudo ou nada.
    let mut sub = state.clone();
    for &slot in containers.inputs.iter().chain(containers.outputs) {
        let Some((current, stored)) = sub.slots[slot].clone() else {
            continue;
        };
        if slot_extract(&mut sub, rules, slot, &current, stored) != stored
            || sub.slots[slot].is_some()
        {
            return Action::Nothing;
        }
        if !send_to_player(&mut sub, rules, creative, &current, stored) {
            return Action::Nothing;
        }
    }
    *state = sub;
    Action::PlaceGhostRecipe
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plain;

    impl Rules for Plain {
        fn max_stack(&self, _: &str) -> i32 {
            64
        }
        fn valid(&self, slot: usize, _: &str) -> bool {
            slot == 0
        }
        fn can_insert(&self, slot: usize, _: &str) -> bool {
            slot == 0
        }
        fn can_extract(&self, _: usize, _: &str) -> bool {
            true
        }
        fn hash(&self, resource: &str) -> i32 {
            resource.len() as i32
        }
    }

    fn state(main: &[(usize, &str, i32)], input: Stack) -> State {
        let mut slots = vec![None; MAIN_SLOTS];
        for (at, item, count) in main {
            slots[*at] = Some(((*item).to_owned(), *count));
        }
        State {
            main: slots,
            offhand: None,
            selected: 0,
            slots: vec![input, None],
            dropped: Vec::new(),
        }
    }

    const BOTH: Containers<'static> = Containers {
        inputs: &[0],
        outputs: &[1],
    };

    #[test]
    fn the_resource_hash_is_stable_within_a_process_and_differs_between_resources() {
        assert_eq!(item_resource_hash("a:b"), item_resource_hash("a:b"));
        assert_ne!(item_resource_hash("a:b"), item_resource_hash("a:c"));
        assert_eq!(item_of("a:b#x"), "a:b");
    }

    #[test]
    fn the_place_button_carries_the_display_and_the_shift() {
        assert_eq!(
            place_button_request(place_button(3913, true)),
            Some((3913, true))
        );
        assert_eq!(
            place_button_request(place_button(0, false)),
            Some((0, false))
        );
        assert_eq!(place_button_request(103), None);
        assert_eq!(place_button_request(-1), None);
    }

    #[test]
    fn a_foreign_recipe_changes_nothing() {
        let mut now = state(&[(0, "a", 5)], Some(("b".to_owned(), 2)));
        let before = now.clone();
        assert_eq!(
            place_recipe(&mut now, &Plain, &Recipe::Foreign, BOTH, false, false),
            Action::ClassCast
        );
        assert_eq!(now, before);
    }

    #[test]
    fn too_few_items_without_shift_are_lost_as_in_the_original() {
        let mut now = state(&[(3, "a", 2)], None);
        let recipe = Recipe::Item(Ingredient {
            items: &["a"],
            count: 3,
        });
        assert_eq!(
            place_recipe(&mut now, &Plain, &recipe, BOTH, false, false),
            Action::PlaceGhostRecipe
        );
        assert!(now.main.iter().all(Option::is_none));
        assert_eq!(now.slots, vec![None, None]);
    }
}
