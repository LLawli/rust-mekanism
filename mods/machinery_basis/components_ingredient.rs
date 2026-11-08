// Módulo de `crafting.rs`: a resolução de `items` de um `neoforge:components` (`DataComponentIngredient`). O Horizon manda o tipo de ingrediente
// que não é dele ao hook `ingredient` (`kind`, o JSON e uma lista de itens para preencher) e descarta o arquivo se a lista volta vazia, mas o hook
// não recebe o `Items`, e `items` do ingrediente é um `HolderSet` que pode ser `#tag`. `stash` guarda, só para este tipo e só se há `#tag`, a cópia
// das tags de item que `Ingredient::parse_object` tem à mão, e `resolve` (o hook `ingredient`) preenche a lista com ela e a larga.
use std::cell::RefCell;
use std::collections::HashMap;

const KIND: &str = "neoforge:components";

thread_local! {
    static TAGS: RefCell<Option<HashMap<String, Vec<String>>>> = const { RefCell::new(None) };
}

fn kind_of(object: &serde_json::Map<String, serde_json::Value>) -> Option<&str> {
    object.get("neoforge:ingredient_type").or_else(|| object.get("type")).and_then(serde_json::Value::as_str)
}

/// Os itens que `items` nomeia, na ordem do arquivo: ids, `#tag` e listas, com o `neoforge:or` (a união de `HolderSet`s) achatado.
fn named(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(list) => list.iter().for_each(|child| named(child, out)),
        serde_json::Value::Object(map) if map.get("type").and_then(serde_json::Value::as_str) == Some("neoforge:or") => {
            if let Some(values) = map.get("values") {
                named(values, out);
            }
        }
        _ => {}
    }
}

/// `Ingredient::parse_object` vai chamar o hook para este objeto: guarda as tags se ele precisa delas.
pub fn stash(object: &serde_json::Map<String, serde_json::Value>, tags: &HashMap<String, Vec<String>>) {
    if kind_of(object) != Some(KIND) {
        return;
    }
    let mut names = Vec::new();
    if let Some(items) = object.get("items") {
        named(items, &mut names);
    }
    if names.iter().any(|name| name.starts_with('#')) {
        TAGS.with(|held| *held.borrow_mut() = Some(tags.clone()));
    }
}

/// O hook `ingredient` para o tipo `neoforge:components`: a lista de itens do `HolderSet`, sem repetir, na ordem do `HolderSet`.
pub fn resolve(kind: &str, data: &serde_json::Map<String, serde_json::Value>, items: &mut Vec<String>) {
    if kind != KIND {
        return;
    }
    let tags = TAGS.with(|held| held.borrow_mut().take());
    let mut names = Vec::new();
    if let Some(listed) = data.get("items") {
        named(listed, &mut names);
    }
    for name in names {
        let found: Vec<String> = match name.strip_prefix('#') {
            Some(tag) => tags.as_ref().and_then(|tags| tags.get(tag)).cloned().unwrap_or_default(),
            None => vec![name],
        };
        for item in found {
            if !items.contains(&item) {
                items.push(item);
            }
        }
    }
}
