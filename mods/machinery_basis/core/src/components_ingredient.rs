//! `DataComponentIngredient` do NeoForge (`neoforge:components`): os itens, um `DataComponentPatch` e `strict`. A lib não conhece
//! componente nenhum: quem usa dá o valor de cada um na forma canônica de texto que escolher (dois valores iguais dão o mesmo texto),
//! e a pilha chega como o mapa de todos os componentes dela, os padrão do item e o patch (`ItemStack.getComponents()`).
//!
//! | Rust | Java |
//! |---|---|
//! | [`ComponentsIngredient::test`] | `DataComponentIngredient.test` (sem a parte dos itens, que é do jogo) |
//! | [`ComponentsIngredient::exact_components`] | `PatchedDataComponentMap.fromPatch(item.components(), patch)`, a chave de `addComponentInputCache` |
use crate::components::Components;
use crate::recipe_cache::IngredientShape;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::rc::Rc;

/// O que o patch diz de um componente: um valor, ou `!tipo` (tirar o componente).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchEntry {
    Set(String),
    Removed,
}

/// `DataComponentIngredient`, sem os itens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentsIngredient {
    patch: BTreeMap<String, PatchEntry>,
    strict: bool,
}

/// O tipo do ingrediente, como a receita o escreve (`neoforge:ingredient_type`).
pub const KIND: &str = "neoforge:components";

impl ComponentsIngredient {
    /// O ingrediente de um arquivo de receita: `components` (o formato do `DataComponentPatch.CODEC`: `tipo: valor` ou `"!tipo": {}`)
    /// e `strict` (`Codec.BOOL.optionalFieldOf("strict", false)`). `canon` dá o texto canônico de um valor. `None` se o codec
    /// recusaria o arquivo.
    pub fn parse(object: &Map<String, Value>, canon: impl Fn(&Value) -> String) -> Option<Self> {
        let strict = match object.get("strict") {
            None => false,
            Some(Value::Bool(strict)) => *strict,
            Some(_) => return None,
        };
        let mut patch = BTreeMap::new();
        for (key, value) in object.get("components")?.as_object()? {
            match key.strip_prefix('!') {
                // `DataComponentPatch.CODEC`: tirar um componente é `"!tipo": {}`.
                Some(removed) => {
                    if !value.as_object().is_some_and(Map::is_empty) {
                        return None;
                    }
                    patch.insert(removed.to_owned(), PatchEntry::Removed);
                }
                None => {
                    patch.insert(key.clone(), PatchEntry::Set(canon(value)));
                }
            }
        }
        Some(Self { patch, strict })
    }

    /// `componentsExhaustive`.
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    /// `DataComponentIngredient.test` sem os itens: cada entrada do patch tem de bater com os componentes da pilha (`testComponents`:
    /// um valor pede o mesmo valor, `!tipo` pede que a pilha não tenha o tipo), e com `strict` a pilha não pode ter tipo nenhum
    /// que o patch não cite (`isPatched`: valor ou remoção).
    pub fn test(&self, stack: &BTreeMap<String, String>) -> bool {
        for (kind, entry) in &self.patch {
            let held = stack.get(kind);
            match entry {
                PatchEntry::Removed if held.is_some() => return false,
                PatchEntry::Set(value) if held != Some(value) => return false,
                _ => {}
            }
        }
        !self.strict || stack.keys().all(|kind| self.patch.contains_key(kind))
    }

    /// `PatchedDataComponentMap.fromPatch(prototype, patch)`: o patch contra os componentes padrão do item, sem as entradas que
    /// repetem o padrão e sem a remoção do que o item não tem (`applyPatch` passa por `set` e `remove`). É o patch que uma pilha de
    /// verdade com esses componentes tem, e por isso a chave do índice de receitas de componentes exatos. As remoções entram como
    /// `!tipo`, que nenhum patch de pilha do Horizon tem.
    pub fn exact_components(&self, defaults: &BTreeMap<String, String>) -> Components {
        let mut entries: Vec<(String, String)> = Vec::new();
        for (kind, entry) in &self.patch {
            match entry {
                PatchEntry::Set(value) if defaults.get(kind) != Some(value) => {
                    entries.push((kind.clone(), value.clone()));
                }
                PatchEntry::Removed if defaults.contains_key(kind) => {
                    entries.push((format!("!{kind}"), String::new()));
                }
                _ => {}
            }
        }
        Components::new(
            entries
                .iter()
                .map(|(kind, value)| (kind.as_str(), value.as_str())),
        )
    }
}

/// Um `neoforge:components` com os itens que ele nomeia (as tags já resolvidas) e os componentes padrão de cada um, em texto canônico.
pub struct Leaf {
    pub ingredient: ComponentsIngredient,
    pub items: Vec<String>,
    pub defaults: BTreeMap<String, BTreeMap<String, String>>,
}

/// Um ingrediente de item que tem um `neoforge:components` em algum ramo, em árvore: `CompoundIngredient`, `IntersectionIngredient` e
/// `DifferenceIngredient` em volta. O ramo sem componente em lugar nenhum é a lista de itens dele (`Plain`, na ordem do `HolderSet`).
pub enum Tree {
    Plain(Vec<String>),
    Components(Leaf),
    /// Um ingrediente de mod que não é de componentes: só quem o conhece sabe testá-lo, pelo item.
    Opaque(Box<dyn Fn(&str) -> bool + Send + Sync>),
    Compound(Vec<Tree>),
    Intersection(Vec<Tree>),
    Difference(Box<Tree>, Box<Tree>),
}

impl Tree {
    /// `ItemInputCache.mapIngredient`: o composto mapeia cada filho; o de componentes exatos (`strict`) indexa a chave de cada item
    /// (`addComponentInputCache`); o resto é complexo.
    pub fn shape(&self) -> IngredientShape {
        match self {
            Self::Plain(items) => {
                IngredientShape::Types(items.iter().map(|item| Rc::from(item.as_str())).collect())
            }
            Self::Components(leaf) if !leaf.ingredient.is_strict() => IngredientShape::Custom,
            Self::Components(leaf) => {
                let none = BTreeMap::new();
                IngredientShape::Compound(
                    leaf.items
                        .iter()
                        .map(|item| {
                            let key = leaf
                                .ingredient
                                .exact_components(leaf.defaults.get(item).unwrap_or(&none));
                            IngredientShape::exact(&[item], (!key.is_empty()).then(|| key.key()))
                        })
                        .collect(),
                )
            }
            Self::Compound(kids) => {
                IngredientShape::Compound(kids.iter().map(Self::shape).collect())
            }
            Self::Opaque(_) | Self::Intersection(_) | Self::Difference(..) => {
                IngredientShape::Custom
            }
        }
    }

    /// `Ingredient.test` de um item cujo patch de componentes é `patch` (texto canônico por tipo), por cima dos componentes padrão dele.
    pub fn accepts(&self, item: &str, patch: &[(String, String)]) -> bool {
        match self {
            Self::Plain(items) => items.iter().any(|listed| listed == item),
            Self::Components(leaf) => {
                if !leaf.items.iter().any(|listed| listed == item) {
                    return false;
                }
                let mut full = leaf.defaults.get(item).cloned().unwrap_or_default();
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
                leaf.ingredient.test(&full)
            }
            Self::Opaque(test) => test(item),
            Self::Compound(kids) => kids.iter().any(|kid| kid.accepts(item, patch)),
            Self::Intersection(kids) => kids.iter().all(|kid| kid.accepts(item, patch)),
            Self::Difference(base, subtracted) => {
                base.accepts(item, patch) && !subtracted.accepts(item, patch)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn canon(value: &Value) -> String {
        value.to_string()
    }

    fn ingredient(components: Value, strict: bool) -> ComponentsIngredient {
        let object = json!({"components": components, "strict": strict});
        ComponentsIngredient::parse(object.as_object().unwrap(), canon).unwrap()
    }

    fn stack(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn an_empty_patch_matches_everything_unless_strict() {
        let everything = stack(&[("a", "1")]);
        assert!(ingredient(json!({}), false).test(&everything));
        assert!(!ingredient(json!({}), true).test(&everything));
        assert!(ingredient(json!({}), true).test(&stack(&[])));
    }

    #[test]
    fn a_value_asks_for_the_same_value_and_a_removal_for_no_component() {
        let held = stack(&[("a", "1"), ("b", "2")]);
        assert!(ingredient(json!({"a": 1}), false).test(&held));
        assert!(!ingredient(json!({"a": 2}), false).test(&held));
        assert!(!ingredient(json!({"c": 1}), false).test(&held));
        assert!(ingredient(json!({"!c": {}}), false).test(&held));
        assert!(!ingredient(json!({"!a": {}}), false).test(&held));
    }

    #[test]
    fn strict_wants_every_component_of_the_stack_listed() {
        let held = stack(&[("a", "1"), ("b", "2")]);
        assert!(ingredient(json!({"a": 1, "b": 2}), true).test(&held));
        assert!(!ingredient(json!({"a": 1}), true).test(&held));
        // Citar o tipo basta para `isPatched`, e a remoção de um tipo ausente também o cita.
        assert!(ingredient(json!({"a": 1, "b": 2, "!c": {}}), true).test(&held));
    }

    #[test]
    fn the_codec_refuses_a_bad_file() {
        for bad in [
            json!({"components": 3}),
            json!({"components": {"!a": 1}}),
            json!({"components": {}, "strict": "yes"}),
            json!({"strict": true}),
        ] {
            assert_eq!(
                ComponentsIngredient::parse(bad.as_object().unwrap(), canon),
                None,
                "{bad}"
            );
        }
    }

    #[test]
    fn the_exact_key_is_the_patch_against_the_item_defaults() {
        let defaults = stack(&[("a", "1"), ("b", "2")]);
        // Igual ao padrão: sai; diferente: fica; remover o que o item tem: fica; remover o que ele não tem: sai.
        let patch = ingredient(json!({"a": 1, "b": 3, "c": 4, "!b2": {}, "!z": {}}), true);
        let key = patch.exact_components(&defaults);
        assert_eq!(key.iter().collect::<Vec<_>>(), vec![("b", "3"), ("c", "4")]);
        let defaults_only = ingredient(json!({"a": 1, "b": 2}), true).exact_components(&defaults);
        assert!(defaults_only.is_empty());
        let removal = ingredient(json!({"!a": {}}), true).exact_components(&defaults);
        assert_eq!(removal.iter().collect::<Vec<_>>(), vec![("!a", "")]);
    }
}
