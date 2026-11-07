//! `DataComponentPatch` de um `ItemResource`: o que distingue dois itens do mesmo id. O NeoForge compara um recurso de item pelo
//! item e pelos componentes (`ItemResource.equals`, `ItemStack.isSameItemSameComponents`), e é isso que decide se dois itens
//! empilham, se um slot aceita o item de um hopper e se uma retirada acha o que pede.
//!
//! A lib não conhece nenhum componente: quem usa diz o id de cada um e o valor na forma canônica de texto que escolher, desde que
//! dois valores iguais deem o mesmo texto e dois diferentes deem textos diferentes. O dano (`minecraft:damage`) é um componente
//! como os outros.
use std::rc::Rc;

/// Os componentes de um item, em ordem de id. Vazio é um item sem patch (`isComponentsPatchEmpty`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Components {
    entries: Rc<[(Rc<str>, Rc<str>)]>,
    /// As entradas num texto só, para comparar e para servir de chave: igual para os mesmos componentes, qualquer que seja a ordem
    /// em que foram dados.
    key: Rc<str>,
}

impl Components {
    /// Sem componentes.
    pub fn none() -> Self {
        Self::default()
    }

    /// De `(id, valor)`. Um id repetido vale pelo último.
    pub fn new<'a>(entries: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut sorted: Vec<(Rc<str>, Rc<str>)> = Vec::new();
        for (id, value) in entries {
            sorted.retain(|(held, _)| **held != *id);
            sorted.push((Rc::from(id), Rc::from(value)));
        }
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut key = String::new();
        for (id, value) in &sorted {
            key.push_str(&format!("{}:{id}{}:{value}", id.len(), value.len()));
        }
        Self {
            entries: sorted.into(),
            key: Rc::from(key),
        }
    }

    /// Um componente só, de id `variant`: para os testes que só precisam de dois itens diferentes, sem importar qual componente.
    pub fn variant(value: &str) -> Self {
        Self::new([("variant", value)])
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// O valor de um componente, se o patch o tem.
    pub fn get(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(held, _)| **held == *id)
            .map(|(_, value)| &**value)
    }

    /// Os componentes, em ordem de id.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(id, value)| (&**id, &**value))
    }

    /// O texto que os identifica: vazio sem componentes.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// O valor de um componente de texto na forma canônica: entre aspas, com `\` e `"` escapados.
    pub fn text_value(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }

    /// O inverso de [`Components::text_value`]: `None` se o valor não é um texto.
    pub fn parse_text_value(value: &str) -> Option<String> {
        let inner = value.strip_prefix('"')?.strip_suffix('"')?;
        let mut text = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                text.push(chars.next()?);
            } else {
                text.push(c);
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_the_components_come_in_does_not_matter() {
        let a = Components::new([("a", "1"), ("b", "2")]);
        let b = Components::new([("b", "2"), ("a", "1")]);
        assert_eq!(a, b);
        assert_eq!(a.key(), b.key());
    }

    #[test]
    fn different_components_are_different_keys() {
        // Os tamanhos na chave impedem que `a` com `bc` e `ab` com `c` se confundam.
        let a = Components::new([("a", "bc")]);
        let b = Components::new([("ab", "c")]);
        assert_ne!(a, b);
        assert_ne!(a.key(), b.key());
        assert_ne!(Components::none(), a);
        assert_ne!(Components::new([("a", "1")]), Components::new([("a", "2")]));
    }

    #[test]
    fn none_is_empty_and_has_no_key() {
        assert!(Components::none().is_empty());
        assert_eq!(Components::none().key(), "");
        assert_eq!(Components::new([]), Components::none());
    }

    #[test]
    fn a_repeated_id_keeps_the_last_value() {
        let components = Components::new([("a", "1"), ("a", "2")]);
        assert_eq!(components.get("a"), Some("2"));
        assert_eq!(components.iter().count(), 1);
    }

    #[test]
    fn a_text_value_round_trips() {
        for text in ["", "speed", "mekanism:speed", "a\"b", "a\\b", "\\\""] {
            let value = Components::text_value(text);
            assert_eq!(Components::parse_text_value(&value).as_deref(), Some(text));
        }
        assert_eq!(
            Components::text_value("mekanism:speed"),
            "\"mekanism:speed\""
        );
        assert_eq!(Components::parse_text_value("5"), None);
        assert_eq!(Components::parse_text_value("\"a"), None);
    }
}
