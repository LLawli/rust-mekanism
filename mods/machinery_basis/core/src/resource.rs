//! Recursos tipados: o que um tanque guarda. A lib não conhece nenhum: cada mod os declara em
//! `data/<ns>/machinery_basis/resource/<nome>.json`, e o id vira `<ns>:<nome>`.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// A pasta que o mod escuta, dentro de `data/<ns>/`.
pub const DATA_FOLDER: &str = "machinery_basis/resource";

/// Um recurso declarado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub id: String,
    /// A família, livre para cada mod: `gas`, `infusion`, `mana`. Tubos só levam o que aceitam.
    pub kind: String,
    /// Cor `0xRRGGBB`, para medidores e tubos.
    pub color: u32,
}

/// Por que um arquivo de recurso foi recusado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    BadId,
    MissingKind,
    BadColor,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadId => "o id precisa ser <ns>:<nome>",
            Self::MissingKind => "falta `kind`, um texto não vazio",
            Self::BadColor => "`color` precisa ser RRGGBB em hexadecimal",
        })
    }
}

fn valid_id(id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(namespace, path)| !namespace.is_empty() && !path.is_empty())
}

fn parse_color(text: &str) -> Option<u32> {
    if text.len() != 6 {
        return None;
    }
    u32::from_str_radix(text, 16).ok()
}

impl Resource {
    pub fn parse(id: &str, json: &Value) -> Result<Self, ResourceError> {
        if !valid_id(id) {
            return Err(ResourceError::BadId);
        }
        let kind = json
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| !kind.is_empty())
            .ok_or(ResourceError::MissingKind)?;
        let color = json
            .get("color")
            .and_then(Value::as_str)
            .and_then(parse_color)
            .ok_or(ResourceError::BadColor)?;
        Ok(Self {
            id: id.to_owned(),
            kind: kind.to_owned(),
            color,
        })
    }
}

/// Os recursos de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    resources: BTreeMap<String, Resource>,
}

impl Registry {
    /// Monta o registro com o que o jogo leu. Um arquivo ruim fica de fora e volta na lista de
    /// erros: como nos dados do jogo, ele não derruba os outros.
    pub fn load(files: &[(String, Value)]) -> (Self, Vec<(String, ResourceError)>) {
        let mut registry = Self::default();
        let mut errors = Vec::new();
        for (id, json) in files {
            match Resource::parse(id, json) {
                Ok(resource) => {
                    registry.resources.insert(id.clone(), resource);
                }
                Err(error) => errors.push((id.clone(), error)),
            }
        }
        (registry, errors)
    }

    pub fn get(&self, id: &str) -> Option<&Resource> {
        self.resources.get(id)
    }

    pub fn len(&self) -> usize {
        self.resources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Resource> {
        self.resources.values().filter(move |r| r.kind == kind)
    }
}

static CURRENT: RwLock<Option<Arc<Registry>>> = RwLock::new(None);

/// Troca o registro em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_current(registry: Registry) {
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(registry));
}

/// O registro em uso: vazio antes da primeira carga.
pub fn current() -> Arc<Registry> {
    let current = CURRENT.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(id: &str, json: Value) -> (String, Value) {
        (id.to_owned(), json)
    }

    #[test]
    fn reads_a_resource() {
        let parsed = Resource::parse(
            "mekanism:hydrogen",
            &json!({"kind": "gas", "color": "ffffff"}),
        );
        assert_eq!(
            parsed,
            Ok(Resource {
                id: "mekanism:hydrogen".into(),
                kind: "gas".into(),
                color: 0xFF_FFFF,
            })
        );
    }

    #[test]
    fn refuses_what_it_cannot_use() {
        let good = json!({"kind": "gas", "color": "ffffff"});
        assert_eq!(Resource::parse("sem_ns", &good), Err(ResourceError::BadId));
        assert_eq!(Resource::parse(":x", &good), Err(ResourceError::BadId));
        assert_eq!(
            Resource::parse("a:b", &json!({"color": "ffffff"})),
            Err(ResourceError::MissingKind)
        );
        for color in ["fff", "gggggg", "ffffff00"] {
            assert_eq!(
                Resource::parse("a:b", &json!({"kind": "gas", "color": color})),
                Err(ResourceError::BadColor)
            );
        }
    }

    #[test]
    fn a_bad_file_does_not_take_the_others_down() {
        let (registry, errors) = Registry::load(&[
            file("a:bom", json!({"kind": "gas", "color": "000000"})),
            file("a:ruim", json!({"kind": "gas"})),
        ]);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("a:bom").is_some());
        assert_eq!(errors, [("a:ruim".to_owned(), ResourceError::BadColor)]);
    }

    #[test]
    fn filters_by_kind() {
        let (registry, _) = Registry::load(&[
            file("a:gas", json!({"kind": "gas", "color": "000000"})),
            file("a:mana", json!({"kind": "mana", "color": "000000"})),
        ]);
        let gases: Vec<_> = registry.of_kind("gas").map(|r| r.id.as_str()).collect();
        assert_eq!(gases, ["a:gas"]);
    }

    #[test]
    fn a_reload_replaces_the_registry() {
        let (first, _) = Registry::load(&[file("a:x", json!({"kind": "g", "color": "000000"}))]);
        set_current(first);
        assert!(current().get("a:x").is_some());
        let (second, _) = Registry::load(&[file("a:y", json!({"kind": "g", "color": "000000"}))]);
        set_current(second);
        assert!(current().get("a:x").is_none());
        assert!(current().get("a:y").is_some());
    }
}
