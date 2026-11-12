//! O pedaço de NBT que a pilha do transportador usa: números, texto e compostos, com o texto (`toString`) do Minecraft.
use std::collections::BTreeMap;

/// Uma etiqueta do NBT.
#[derive(Clone, Debug, PartialEq)]
pub enum Nbt {
    Int(i32),
    Long(i64),
    String(String),
    Compound(Compound),
}

/// Um `CompoundTag`: as chaves em ordem alfabética, como o `toString` as mostra.
pub type Compound = BTreeMap<String, Nbt>;

impl Nbt {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Nbt::Int(v) => Some(*v),
            Nbt::Long(v) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn as_long(&self) -> Option<i64> {
        match self {
            Nbt::Int(v) => Some(i64::from(*v)),
            Nbt::Long(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Nbt::String(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Nbt::Compound(v) => Some(v),
            _ => None,
        }
    }
}

/// `SnbtGrammar.NON_QUOTED`: uma chave sem aspas só tem estes caracteres.
fn is_simple_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
}

/// `StringTagVisitor.quoteAndEscape`: aspas duplas, a menos que o texto tenha aspas duplas e nenhuma simples.
fn quote(text: &str) -> String {
    let quote = if text.contains('"') && !text.contains('\'') {
        '\''
    } else {
        '"'
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        if c == '\\' || c == quote {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(quote);
    out
}

/// O `toString` de uma etiqueta.
pub fn to_snbt(tag: &Nbt) -> String {
    match tag {
        Nbt::Int(v) => v.to_string(),
        Nbt::Long(v) => format!("{v}L"),
        Nbt::String(v) => quote(v),
        Nbt::Compound(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(key, value)| {
                    let key = if is_simple_key(key) {
                        key.clone()
                    } else {
                        quote(key)
                    };
                    format!("{key}:{}", to_snbt(value))
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_like_the_game() {
        let mut item = Compound::new();
        item.insert("id".into(), Nbt::String("minecraft:stone".into()));
        let mut stack = Compound::new();
        stack.insert("progress".into(), Nbt::Int(0));
        stack.insert("type".into(), Nbt::Compound(item));
        stack.insert("amount".into(), Nbt::Long(10));
        assert_eq!(
            to_snbt(&Nbt::Compound(stack)),
            r#"{amount:10L,progress:0,type:{id:"minecraft:stone"}}"#
        );
        assert_eq!(to_snbt(&Nbt::String("a\"b".into())), "'a\"b'");
    }
}
