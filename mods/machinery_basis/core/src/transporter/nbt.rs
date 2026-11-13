//! O NBT que a pilha do transportador usa: todas as etiquetas do Minecraft (os componentes de um item podem ter qualquer uma), com o texto (`toString`) delas.
use std::collections::BTreeMap;

/// Uma etiqueta do NBT.
#[derive(Clone, Debug, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    /// `FloatTag`, pelos bits (`f32::to_bits`): o `NaN` fica igual a si mesmo, como o `FloatTag.equals` do Java (`Float.compare`).
    Float(u32),
    /// `DoubleTag`, pelos bits (`f64::to_bits`).
    Double(u64),
    String(String),
    Compound(Compound),
    /// `ListTag`: os elementos, na ordem.
    List(Vec<Nbt>),
    /// `ByteArrayTag`.
    ByteArray(Vec<u8>),
    /// `IntArrayTag`.
    IntArray(Vec<i32>),
    /// `LongArrayTag`.
    LongArray(Vec<i64>),
}

/// Um `CompoundTag`: as chaves em ordem alfabética, como o `toString` as mostra.
pub type Compound = BTreeMap<String, Nbt>;

impl Nbt {
    /// `NumericTag.intValue`: qualquer número, e só número.
    pub fn as_int(&self) -> Option<i32> {
        match self {
            Nbt::Byte(v) => Some(i32::from(*v)),
            Nbt::Short(v) => Some(i32::from(*v)),
            Nbt::Int(v) => Some(*v),
            Nbt::Long(v) => Some(*v as i32),
            Nbt::Float(v) => Some(f32::from_bits(*v) as i32),
            Nbt::Double(v) => Some(f64::from_bits(*v) as i32),
            _ => None,
        }
    }

    /// `NumericTag.longValue`.
    pub fn as_long(&self) -> Option<i64> {
        match self {
            Nbt::Byte(v) => Some(i64::from(*v)),
            Nbt::Short(v) => Some(i64::from(*v)),
            Nbt::Int(v) => Some(i64::from(*v)),
            Nbt::Long(v) => Some(*v),
            Nbt::Float(v) => Some(f32::from_bits(*v) as i64),
            Nbt::Double(v) => Some(f64::from_bits(*v) as i64),
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

    pub fn as_list(&self) -> Option<&[Nbt]> {
        match self {
            Nbt::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_int_array(&self) -> Option<&[i32]> {
        match self {
            Nbt::IntArray(v) => Some(v),
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

/// `Float.toString` e `Double.toString` do Java: o decimal mais curto que volta ao mesmo número, sempre com a parte fracionária; de `10^-3` (inclusive) a `10^7` (exclusive) sem
/// expoente, fora disso a notação científica (`1.0E10`).
fn java_float_text(value: f64, shortest: &str, scientific: &str) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let magnitude = value.abs();
    if magnitude == 0.0 || (1e-3..1e7).contains(&magnitude) {
        if shortest.contains('.') {
            shortest.to_owned()
        } else {
            format!("{shortest}.0")
        }
    } else {
        let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific, "0"));
        let mantissa = if mantissa.contains('.') { mantissa.to_owned() } else { format!("{mantissa}.0") };
        format!("{mantissa}E{exponent}")
    }
}

/// O `toString` de uma etiqueta.
pub fn to_snbt(tag: &Nbt) -> String {
    match tag {
        Nbt::Byte(v) => format!("{v}b"),
        Nbt::Short(v) => format!("{v}s"),
        Nbt::Int(v) => v.to_string(),
        Nbt::Long(v) => format!("{v}L"),
        Nbt::Float(bits) => {
            let value = f32::from_bits(*bits);
            format!("{}f", java_float_text(f64::from(value), &format!("{value}"), &format!("{value:e}")))
        }
        Nbt::Double(bits) => {
            let value = f64::from_bits(*bits);
            format!("{}d", java_float_text(value, &format!("{value}"), &format!("{value:e}")))
        }
        Nbt::ByteArray(values) => {
            let parts: Vec<String> = values.iter().map(|v| format!("{}B", *v as i8)).collect();
            format!("[B;{}]", parts.join(","))
        }
        Nbt::LongArray(values) => {
            let parts: Vec<String> = values.iter().map(|v| format!("{v}L")).collect();
            format!("[L;{}]", parts.join(","))
        }
        Nbt::String(v) => quote(v),
        Nbt::List(items) => {
            let parts: Vec<String> = items.iter().map(to_snbt).collect();
            format!("[{}]", parts.join(","))
        }
        Nbt::IntArray(values) => {
            let parts: Vec<String> = values.iter().map(i32::to_string).collect();
            format!("[I;{}]", parts.join(","))
        }
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

    #[test]
    fn renders_every_tag_like_the_game() {
        assert_eq!(to_snbt(&Nbt::Byte(-3)), "-3b");
        assert_eq!(to_snbt(&Nbt::Short(300)), "300s");
        assert_eq!(to_snbt(&Nbt::Float(1.5f32.to_bits())), "1.5f");
        assert_eq!(to_snbt(&Nbt::Float(2.0f32.to_bits())), "2.0f");
        assert_eq!(to_snbt(&Nbt::Double(0.25f64.to_bits())), "0.25d");
        assert_eq!(to_snbt(&Nbt::Double(3.0f64.to_bits())), "3.0d");
        assert_eq!(to_snbt(&Nbt::Double(1.0e10f64.to_bits())), "1.0E10d");
        assert_eq!(to_snbt(&Nbt::Double(1.5e-5f64.to_bits())), "1.5E-5d");
        assert_eq!(to_snbt(&Nbt::ByteArray(vec![1, 255])), "[B;1B,-1B]");
        assert_eq!(to_snbt(&Nbt::LongArray(vec![1, -2])), "[L;1L,-2L]");
    }

    #[test]
    fn numbers_read_as_the_game_reads_them() {
        // `NumericTag.intValue`/`longValue`: qualquer número serve, um texto não.
        assert_eq!(Nbt::Byte(7).as_int(), Some(7));
        assert_eq!(Nbt::Short(-7).as_long(), Some(-7));
        assert_eq!(Nbt::Double(2.9f64.to_bits()).as_int(), Some(2));
        assert_eq!(Nbt::String("7".into()).as_int(), None);
    }
}
