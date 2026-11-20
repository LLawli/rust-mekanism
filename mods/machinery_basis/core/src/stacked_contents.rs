//! `StackedContents` do vanilla (`net.minecraft.world.entity.player.StackedContents`, com o `StackedItemContents` por cima): os itens
//! que o jogador e a máquina têm, contados por tipo, e a pergunta "dá para tirar um item para cada ingrediente?", que o livro de
//! receitas faz para pintar o botão de vermelho e para o filtro. O `RecipePicker` é portado linha a linha (os caminhos de aumento em
//! profundidade, na ordem dos índices), porque a ordem dele decide qual item sai primeiro quando a receita é colocada.

/// Os itens contados (`amounts`), na ordem em que entraram. No Java a tabela é um `Reference2IntOpenHashMap` (a ordem é a do hash de
/// identidade): a ordem só decide qual item o `Output` recebe primeiro, não a resposta de [`StackedContents::try_pick`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackedContents {
    amounts: Vec<(String, i32)>,
}

impl StackedContents {
    /// `clear`.
    pub fn clear(&mut self) {
        self.amounts.clear();
    }

    /// `StackedItemContents.accountStack(stack, maxCount)`: a pilha vazia não conta e a contagem para no `maxCount`
    /// (`getMaxStackSize` do item).
    pub fn account_stack(&mut self, item: &str, count: i32, max_count: i32) {
        if count > 0 {
            self.account(item, count.min(max_count));
        }
    }

    /// `account(item, count)` (`put`, um `addTo`).
    pub fn account(&mut self, item: &str, count: i32) {
        match self.amounts.iter_mut().find(|(name, _)| name == item) {
            Some((_, amount)) => *amount += count,
            None => self.amounts.push((item.to_owned(), count)),
        }
    }

    /// Quanto há do item.
    pub fn amount(&self, item: &str) -> i32 {
        self.amounts
            .iter()
            .find(|(name, _)| name == item)
            .map_or(0, |(_, amount)| *amount)
    }

    /// `tryPick(ingredients, capacity, output)`: se cada ingrediente (a lista dos itens que ele aceita) recebe `capacity` itens de um
    /// tipo que ele aceita. Devolve os itens escolhidos, um por ingrediente satisfeito na ordem dos ingredientes (o `Output`), ou `None`.
    pub fn pick(&self, ingredients: &[&[&str]], capacity: i32) -> Option<Vec<String>> {
        Picker::new(self, ingredients).try_pick(capacity)
    }

    /// `StackedItemContents.canCraft(contents, null)`.
    pub fn try_pick(&self, ingredients: &[&[&str]], capacity: i32) -> bool {
        self.pick(ingredients, capacity).is_some()
    }
}

/// `StackedContents.RecipePicker`.
struct Picker {
    /// `items` (`getUniqueAvailableIngredientItems`): os que há e que algum ingrediente aceita, com a quantidade que sobra.
    items: Vec<(String, i32)>,
    ingredient_count: usize,
    connection: Vec<bool>,
    residual: Vec<bool>,
    satisfied: Vec<bool>,
    visited_ingredient: Vec<bool>,
    visited_item: Vec<bool>,
}

impl Picker {
    fn new(contents: &StackedContents, ingredients: &[&[&str]]) -> Self {
        let items: Vec<(String, i32)> = contents
            .amounts
            .iter()
            .filter(|(name, amount)| {
                *amount > 0
                    && ingredients
                        .iter()
                        .any(|accepted| accepted.contains(&name.as_str()))
            })
            .cloned()
            .collect();
        let ingredient_count = ingredients.len();
        let mut connection = vec![false; ingredient_count * items.len()];
        // `setInitialConnections`.
        for (ingredient, accepted) in ingredients.iter().enumerate() {
            for (item, (name, _)) in items.iter().enumerate() {
                if accepted.contains(&name.as_str()) {
                    connection[ingredient * items.len() + item] = true;
                }
            }
        }
        Self {
            residual: vec![false; connection.len()],
            satisfied: vec![false; ingredient_count],
            visited_ingredient: vec![false; ingredient_count],
            visited_item: vec![false; items.len()],
            items,
            ingredient_count,
            connection,
        }
    }

    fn at(&self, item: usize, ingredient: usize) -> usize {
        ingredient * self.items.len() + item
    }

    fn try_pick(mut self, capacity: i32) -> Option<Vec<String>> {
        if capacity <= 0 {
            return Some(Vec::new());
        }
        let mut satisfied_count = 0;
        loop {
            let Some(path) = self.try_assigning_new_item(capacity) else {
                if satisfied_count != self.ingredient_count {
                    return None;
                }
                let mut output = Vec::new();
                for ingredient in 0..self.ingredient_count {
                    for item in 0..self.items.len() {
                        if self.residual[self.at(item, ingredient)] {
                            output.push(self.items[item].0.clone());
                            break;
                        }
                    }
                }
                return Some(output);
            };
            // `take(items.get(assignedItem), capacity)`.
            self.items[path[0]].1 -= capacity;
            self.satisfied[path[path.len() - 1]] = true;
            satisfied_count += 1;
            for i in 0..path.len() - 1 {
                if i & 1 == 0 {
                    let at = self.at(path[i], path[i + 1]);
                    self.residual[at] = true;
                } else {
                    let at = self.at(path[i + 1], path[i]);
                    self.residual[at] = false;
                }
            }
        }
    }

    /// `tryAssigningNewItem`: os visitados são limpos uma vez, antes do laço dos itens.
    fn try_assigning_new_item(&mut self, capacity: i32) -> Option<Vec<usize>> {
        self.visited_ingredient.fill(false);
        self.visited_item.fill(false);
        for item in 0..self.items.len() {
            if self.items[item].1 >= capacity
                && let Some(path) = self.find_new_item_assignment_path(item)
            {
                return Some(path);
            }
        }
        None
    }

    /// `findNewItemAssignmentPath`: os índices pares do caminho são itens e os ímpares, ingredientes.
    fn find_new_item_assignment_path(&mut self, starting_item: usize) -> Option<Vec<usize>> {
        let mut path = vec![starting_item];
        self.visited_item[starting_item] = true;
        while !path.is_empty() {
            let length = path.len();
            let last = path[length - 1];
            if (length - 1) & 1 == 0 {
                for ingredient in 0..self.ingredient_count {
                    let at = self.at(last, ingredient);
                    if !self.visited_ingredient[ingredient]
                        && self.connection[at]
                        && !self.residual[at]
                    {
                        self.visited_ingredient[ingredient] = true;
                        path.push(ingredient);
                        break;
                    }
                }
            } else {
                if !self.satisfied[last] {
                    return Some(path);
                }
                for item in 0..self.items.len() {
                    if !self.visited_item[item] && self.residual[self.at(item, last)] {
                        self.visited_item[item] = true;
                        path.push(item);
                        break;
                    }
                }
            }
            if path.len() == length {
                path.pop();
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(stacks: &[(&str, i32)]) -> StackedContents {
        let mut contents = StackedContents::default();
        for (item, count) in stacks {
            contents.account_stack(item, *count, 64);
        }
        contents
    }

    #[test]
    fn one_item_feeds_as_many_ingredients_as_it_has_units() {
        let stone: &[&str] = &["a:stone"];
        assert!(contents(&[("a:stone", 3)]).try_pick(&[stone, stone, stone], 1));
        assert!(!contents(&[("a:stone", 2)]).try_pick(&[stone, stone, stone], 1));
        // Duas pilhas do mesmo item somam, e a pilha para no máximo do item.
        assert!(contents(&[("a:stone", 2), ("a:stone", 1)]).try_pick(&[stone, stone, stone], 1));
        let mut capped = StackedContents::default();
        capped.account_stack("a:pearl", 40, 16);
        assert_eq!(capped.amount("a:pearl"), 16);
    }

    #[test]
    fn an_assignment_moves_to_free_an_item_another_ingredient_needs() {
        // O primeiro ingrediente aceita os dois itens e o segundo só `a`: o caminho de aumento tira `a` do primeiro.
        let both: &[&str] = &["x:a", "x:b"];
        let only_a: &[&str] = &["x:a"];
        let picked = contents(&[("x:a", 1), ("x:b", 1)]).pick(&[both, only_a], 1);
        assert_eq!(picked, Some(vec!["x:b".to_owned(), "x:a".to_owned()]));
        assert!(!contents(&[("x:b", 2)]).try_pick(&[both, only_a], 1));
        // Vários tipos aceitos somam para o mesmo ingrediente repetido.
        assert!(contents(&[("x:a", 1), ("x:b", 1)]).try_pick(&[both, both], 1));
        assert!(!contents(&[("x:a", 1), ("x:b", 1)]).try_pick(&[both, both, both], 1));
    }

    #[test]
    fn no_ingredient_and_no_capacity_are_always_pickable() {
        assert!(contents(&[]).try_pick(&[], 1));
        assert!(contents(&[]).try_pick(&[&["x:a"]], 0));
        assert!(!contents(&[]).try_pick(&[&["x:a"]], 1));
    }
}
