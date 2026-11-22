//! Os mapas de dados do NeoForge (`data/<ns>/data_maps/<registro>/<tipo>.json`), como `DataMapLoader` os monta: os arquivos de um tipo empilhados pela ordem dos
//! pacotes, as chaves de `values` na ordem do documento, a chave de tag (`#ns:tag`) que vale para cada membro da tag, e `remove` depois dos valores do mesmo arquivo.
//! A lib não sabe de registro nem de tipo: quem a usa entrega os arquivos de um tipo e quem são os membros de uma tag.
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

/// `DataMapFile`: um arquivo de um tipo, de um pacote. `values` fica na ordem do documento (`ExtraCodecs.strictUnboundedMap` guarda a ordem), que decide quem
/// vence quando uma tag e um id dão valor ao mesmo objeto.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataMapFile {
    /// `replace` do arquivo: larga o que os pacotes de baixo deram.
    pub replace: bool,
    /// A chave (um id, ou `#` e uma tag) e o valor cru (com o invólucro `{value, replace}` se o arquivo o usa).
    pub values: Vec<(String, Value)>,
    /// As chaves de `remove`, ids ou tags.
    pub removals: Vec<String>,
}

/// As chaves de um objeto na ordem do documento: o `Value` do `serde_json` as ordena pelo nome.
struct Ordered(Vec<(String, Value)>);

impl<'de> serde::Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = Ordered;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("um objeto")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    entries.push((key, value));
                }
                Ok(Ordered(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

impl<'de> serde::Deserialize<'de> for DataMapFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct File;
        impl<'de> Visitor<'de> for File {
            type Value = DataMapFile;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("um arquivo de mapa de dados")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<DataMapFile, A::Error> {
                let mut file = DataMapFile::default();
                let mut values = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "replace" => file.replace = map.next_value()?,
                        "values" => values = Some(map.next_value::<Ordered>()?.0),
                        "remove" => file.removals = removals(&map.next_value::<Value>()?),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                // `fieldOf("values")`: o arquivo sem ela não decodifica.
                file.values = values.ok_or_else(|| serde::de::Error::missing_field("values"))?;
                Ok(file)
            }
        }
        deserializer.deserialize_map(File)
    }
}

/// `removalsCodec`: a lista de chaves, a lista de `{key}` e o objeto de chaves (as formas com `remover` são dos tipos avançados, e aqui só a chave conta).
fn removals(json: &Value) -> Vec<String> {
    match json {
        Value::Array(list) => list
            .iter()
            .filter_map(|entry| match entry {
                Value::String(key) => Some(key.clone()),
                other => other.get("key").and_then(Value::as_str).map(str::to_owned),
            })
            .collect(),
        Value::Object(map) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

impl DataMapFile {
    /// O arquivo do texto, com `values` na ordem do documento. `None` para o que `DataMapFile.codec` recusa (o `DataMapLoader` registra o erro e pula o arquivo).
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// O arquivo de um `Value` já lido: as chaves de `values` vêm na ordem em que o `Value` as guarda (pelo nome, sem `preserve_order`).
    pub fn of_value(json: &Value) -> Option<Self> {
        let values = json.get("values")?.as_object()?;
        Some(Self {
            replace: json
                .get("replace")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            values: values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            removals: json.get("remove").map(removals).unwrap_or_default(),
        })
    }
}

/// `DataMapEntry.codec`: o valor sozinho ou `{value, replace}` (o `Either` tenta primeiro a forma com `value`).
pub fn entry_value(raw: &Value) -> &Value {
    match raw.get("value") {
        Some(value) if raw.is_object() => value,
        _ => raw,
    }
}

/// `DataMapLoader.buildDataMap` com o `DataMapValueMerger.defaultMerger` (o valor novo troca o velho): os arquivos de um tipo, o pacote de baixo primeiro. Cada
/// arquivo com `replace` larga o que veio antes; os valores entram na ordem do documento, a chave de tag para cada membro que `members` devolve
/// (`registry.getTagOrEmpty`); e as chaves de `remove`, tags inclusive, saem depois dos valores do mesmo arquivo. O resultado é o valor de cada id, sem o invólucro.
pub fn build(
    files: &[DataMapFile],
    members: &dyn Fn(&str) -> Vec<String>,
) -> HashMap<String, Value> {
    let resolve = |key: &str| match key.strip_prefix('#') {
        Some(tag) => members(tag),
        None => vec![key.to_owned()],
    };
    let mut result = HashMap::new();
    for file in files {
        if file.replace {
            result.clear();
        }
        for (key, raw) in &file.values {
            for id in resolve(key) {
                result.insert(id, entry_value(raw).clone());
            }
        }
        for key in &file.removals {
            for id in resolve(key) {
                result.remove(&id);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tags(key: &str) -> Vec<String> {
        match key {
            "mod:pair" => vec!["mod:a".to_owned(), "mod:b".to_owned()],
            _ => Vec::new(),
        }
    }

    #[test]
    fn the_values_keep_the_order_of_the_document() {
        let file = DataMapFile::parse(r##"{"values": {"mod:z": 1, "#mod:pair": 2, "mod:a": 3}, "remove": ["mod:z"], "other": {"x": [1]}}"##).unwrap();
        let keys: Vec<&str> = file.values.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["mod:z", "#mod:pair", "mod:a"]);
        assert_eq!(file.removals, ["mod:z"]);
        assert!(!file.replace);
        // Sem `values` o codec recusa o arquivo.
        assert_eq!(DataMapFile::parse(r#"{"replace": true}"#), None);
        assert_eq!(DataMapFile::parse("[]"), None);
    }

    #[test]
    fn a_tag_key_gives_the_value_to_each_member_and_the_later_key_wins() {
        // A tag depois do id: o valor da tag fica.
        let after = DataMapFile::parse(r##"{"values": {"mod:a": 1, "#mod:pair": 2}}"##).unwrap();
        let built = build(&[after], &tags);
        assert_eq!(built["mod:a"], json!(2));
        assert_eq!(built["mod:b"], json!(2));
        // O id depois da tag: o valor do id fica.
        let before = DataMapFile::parse(r##"{"values": {"#mod:pair": 2, "mod:a": 1}}"##).unwrap();
        let built = build(&[before], &tags);
        assert_eq!(built["mod:a"], json!(1));
        assert_eq!(built["mod:b"], json!(2));
        // Uma tag sem membros não dá nada.
        let empty = DataMapFile::parse(r##"{"values": {"#mod:none": 2}}"##).unwrap();
        assert!(build(&[empty], &tags).is_empty());
    }

    #[test]
    fn the_files_stack_and_replace_and_remove_follow_the_loader() {
        let bottom =
            DataMapFile::parse(r#"{"values": {"mod:a": 1, "mod:b": 1, "mod:c": 1}}"#).unwrap();
        // O de cima troca `a`, tira a tag (`a` e `b`: o `remove` vem depois dos valores do mesmo arquivo) e deixa `c`.
        let top = DataMapFile::parse(r##"{"values": {"mod:a": 5, "mod:d": {"value": 7, "replace": true}}, "remove": ["#mod:pair"]}"##).unwrap();
        let built = build(&[bottom.clone(), top], &tags);
        assert_eq!(built.len(), 2);
        assert_eq!(built["mod:c"], json!(1));
        assert_eq!(built["mod:d"], json!(7));
        // `replace` no arquivo larga os de baixo.
        let replacing = DataMapFile::parse(r#"{"replace": true, "values": {"mod:d": 9}}"#).unwrap();
        let built = build(&[bottom, replacing], &tags);
        assert_eq!(built.len(), 1);
        assert_eq!(built["mod:d"], json!(9));
    }

    #[test]
    fn the_removals_take_the_three_forms_of_the_codec() {
        assert_eq!(
            removals(&json!(["mod:a", {"key": "#mod:pair"}])),
            ["mod:a", "#mod:pair"]
        );
        assert_eq!(removals(&json!({"mod:a": {}})), ["mod:a"]);
        let file = DataMapFile::of_value(
            &json!({"values": {"mod:b": 1, "mod:a": 2}, "remove": ["mod:b"], "replace": true}),
        )
        .unwrap();
        assert!(file.replace);
        assert_eq!(file.values.len(), 2);
        assert_eq!(file.removals, ["mod:b"]);
        assert_eq!(DataMapFile::of_value(&json!({"remove": []})), None);
    }
}
