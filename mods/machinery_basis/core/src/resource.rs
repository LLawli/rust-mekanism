//! Recursos tipados: o que um tanque guarda. A lib não conhece nenhum: cada mod os declara em
//! `data/<ns>/machinery_basis/resource/<nome>.json`, e o id vira `<ns>:<nome>`. Um mod que já tem
//! pasta e formato próprios lê os dele e entrega o resultado como outra fonte, por [`set_source`].
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

/// A pasta que o mod escuta, dentro de `data/<ns>/`.
pub const DATA_FOLDER: &str = "machinery_basis/resource";

/// A pasta das tags de recursos, dentro de `data/<ns>/`. O caminho é o de uma tag de registro.
pub const TAGS_FOLDER: &str = "tags/machinery_basis/resource";

/// O que marca uma tag num ingrediente.
pub const TAG_PREFIX: char = '#';

/// Um recurso declarado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub id: String,
    /// A família, livre para cada mod: `gas`, `infusion`, `mana`. Tubos só levam o que aceitam.
    pub kind: String,
    /// Cor `0xRRGGBB`, para medidores e tubos.
    pub color: u32,
    /// O sprite que a tela desenha dentro de um tanque dele, e a cor `0xAARRGGBB` que o multiplica (`Chemical.icon` e `tint`).
    /// Sem `icon`, quem desenha usa a cor lisa.
    pub icon: Option<String>,
    pub tint: u32,
}

/// `tint` sem valor: o sprite sai como é.
pub const NO_TINT: u32 = 0xFF_FF_FF_FF;

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
        let icon = json.get("icon").and_then(Value::as_str).map(str::to_owned);
        let tint = match json.get("tint") {
            None => NO_TINT,
            Some(tint) => tint
                .as_str()
                .and_then(parse_color)
                .map(|rgb| 0xFF_00_00_00 | rgb)
                .ok_or(ResourceError::BadColor)?,
        };
        Ok(Self {
            id: id.to_owned(),
            kind: kind.to_owned(),
            color,
            icon,
            tint,
        })
    }
}

/// Os recursos de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    resources: BTreeMap<String, Resource>,
}

impl Registry {
    /// Um registro já lido por quem conhece o formato.
    pub fn from_resources(resources: impl IntoIterator<Item = Resource>) -> Self {
        Self {
            resources: resources
                .into_iter()
                .map(|resource| (resource.id.clone(), resource))
                .collect(),
        }
    }

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

/// As tags de recursos de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    tags: BTreeMap<String, BTreeSet<String>>,
}

impl Tags {
    /// Monta as tags com o que o jogo leu. Um arquivo sem `values` não acrescenta nada.
    pub fn load(files: &[(String, Value)]) -> Self {
        let mut tags = Self::default();
        for (id, json) in files {
            let values = json.get("values").and_then(Value::as_array);
            let members = values.into_iter().flatten().filter_map(Value::as_str);
            tags.tags
                .entry(id.clone())
                .or_default()
                .extend(members.map(str::to_owned));
        }
        tags
    }

    pub fn contains(&self, tag: &str, resource: &str) -> bool {
        self.tags.get(tag).is_some_and(|tag| tag.contains(resource))
    }
}

static CURRENT: RwLock<Option<Arc<Registry>>> = RwLock::new(None);
static SOURCES: Mutex<BTreeMap<String, Registry>> = Mutex::new(BTreeMap::new());
static TAG_SOURCES: Mutex<BTreeMap<String, Tags>> = Mutex::new(BTreeMap::new());
static CURRENT_TAGS: RwLock<Option<Arc<Tags>>> = RwLock::new(None);

/// Troca o registro em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_source(source: &str, registry: Registry) {
    let mut sources = SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    sources.insert(source.to_owned(), registry);
    let mut merged = Registry::default();
    for registry in sources.values() {
        merged.resources.extend(registry.resources.clone());
    }
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(merged));
}

/// O registro em uso: vazio antes da primeira carga.
pub fn current() -> Arc<Registry> {
    let current = CURRENT.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

/// Troca as tags em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_tags_source(source: &str, tags: Tags) {
    let mut sources = TAG_SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    sources.insert(source.to_owned(), tags);
    let mut merged = Tags::default();
    for tags in sources.values() {
        for (tag, members) in &tags.tags {
            merged
                .tags
                .entry(tag.clone())
                .or_default()
                .extend(members.iter().cloned());
        }
    }
    let mut current = CURRENT_TAGS.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(merged));
}

/// As tags em uso: vazias antes da primeira carga.
pub fn current_tags() -> Arc<Tags> {
    let current = CURRENT_TAGS.read().unwrap_or_else(|e| e.into_inner());
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
                icon: None,
                tint: NO_TINT,
            })
        );
    }

    #[test]
    fn reads_the_icon_and_the_tint_when_there_are_any() {
        let parsed = Resource::parse(
            "a:carbon",
            &json!({"kind": "infusion", "color": "2c2c2c", "icon": "a:infuse/base", "tint": "2c2c2c"}),
        )
        .unwrap();
        assert_eq!(parsed.icon.as_deref(), Some("a:infuse/base"));
        assert_eq!(parsed.tint, 0xFF_2C_2C_2C);
        assert_eq!(
            Resource::parse(
                "a:b",
                &json!({"kind": "gas", "color": "ffffff", "tint": "xyz"})
            ),
            Err(ResourceError::BadColor)
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
        set_source("teste", first);
        assert!(current().get("a:x").is_some());
        let (second, _) = Registry::load(&[file("a:y", json!({"kind": "g", "color": "000000"}))]);
        set_source("teste", second);
        assert!(current().get("a:x").is_none());
        assert!(current().get("a:y").is_some());
    }

    #[test]
    fn a_tag_holds_what_its_file_lists() {
        let tags = Tags::load(&[file("a:infuse", json!({"values": ["a:carbon", "a:gold"]}))]);
        assert!(tags.contains("a:infuse", "a:carbon"));
        assert!(!tags.contains("a:infuse", "a:tin"));
        assert!(!tags.contains("a:outra", "a:carbon"));
    }

    #[test]
    fn a_tag_file_with_no_values_holds_nothing() {
        let tags = Tags::load(&[file("a:vazia", json!({}))]);
        assert!(!tags.contains("a:vazia", "a:carbon"));
    }

    #[test]
    fn sources_add_up_and_a_reload_replaces_only_its_own() {
        let one = |id: &str| {
            Registry::from_resources([Resource {
                id: id.into(),
                kind: "gas".into(),
                color: 0,
                icon: None,
                tint: NO_TINT,
            }])
        };
        set_source("fonte_a", one("a:um"));
        set_source("fonte_b", one("b:dois"));
        assert!(current().get("a:um").is_some() && current().get("b:dois").is_some());
        set_source("fonte_a", one("a:tres"));
        assert!(current().get("a:um").is_none());
        assert!(current().get("a:tres").is_some() && current().get("b:dois").is_some());
    }
}
