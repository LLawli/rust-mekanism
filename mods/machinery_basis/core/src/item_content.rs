//! O conteúdo que a máquina quebrada leva no item e que o item posto devolve ao tile (`ContainerType`, `TileComponentUpgrade.collectImplicitComponents` e
//! `applyImplicitComponents`, `TileEntityMekanism.collectImplicitComponents`). A lib não conhece nenhum item: as formas são as do NBT do tile e as do componente do item,
//! em JSON, e quem usa diz quais máquinas guardam o quê.
//!
//! Cada conta é a do original e vem de um rastro (`dev/referencia/rastros/item_content_<máquina>.jsonl`, `ItemContentTraceTest.java`):
//!
//! - o que a máquina guarda e **é igual ao padrão do item não vira componente** (o `DataComponentPatch` omite o valor padrão): os upgrades vazios (`UpgradeAware.EMPTY`),
//!   os slots todos vazios (`ItemContainerType`: `attachedCopyOf` devolve `null`), o modo de redstone `DISABLED` e o calor na temperatura ambiente;
//! - uma lista de contêineres (`AttachedResources`) guarda todos os contêineres, os vazios como `{}`, e só é aplicada de volta se tem a mesma contagem que o tile
//!   (`ResourceContainerType.copyToContainers`: "if size == containers.size()").
use serde_json::{Map, Value, json};

/// `SerializationConstants.UPGRADES`, `INPUT` e `OUTPUT`: as chaves do `UpgradeAware.CODEC`.
const UPGRADES: &str = "upgrades";
const INPUT: &str = "input";
const OUTPUT: &str = "output";
/// `TileComponentUpgrade.serialize`: o mapa de totais e a lista `items` dos dois slots (`ContainerType.ITEM.saveTo`: `container` e `stored`).
const ITEMS: &str = "items";
const CONTAINER: &str = "container";
const STORED: &str = "stored";
/// `AttachedResources`: a lista dos contêineres.
const CONTAINERS: &str = "containers";
/// `HeatCapacitorData.CODEC`: o calor guardado e a capacidade.
const HEAT_STORED: &str = "heat";
const HEAT_CAPACITY: &str = "heat_capacity";
/// `RedstoneControl.DISABLED`, o padrão do item e do tile.
pub const REDSTONE_DISABLED: &str = "disabled";

fn is_empty_stack(stored: &Value) -> bool {
    stored.as_object().is_none_or(Map::is_empty)
}

/// `TileComponentUpgrade.collectImplicitComponents`: o `UpgradeAware` do tile, a partir do filho `component_upgrade` do NBT. `None` sem upgrade e sem item nos dois slots
/// (igual a `UpgradeAware.EMPTY`, o padrão do item de bloco).
pub fn upgrades_collect(component_upgrade: Option<&Value>) -> Option<Value> {
    let component = component_upgrade?;
    let totals = component
        .get(UPGRADES)
        .filter(|totals| totals.as_object().is_some_and(|map| !map.is_empty()))
        .cloned();
    let mut input = json!({});
    let mut output = json!({});
    for entry in component
        .get(ITEMS)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let stored = entry.get(STORED).cloned().unwrap_or_else(|| json!({}));
        match entry.get(CONTAINER).and_then(Value::as_i64) {
            Some(0) => input = stored,
            Some(1) => output = stored,
            _ => {}
        }
    }
    if totals.is_none() && is_empty_stack(&input) && is_empty_stack(&output) {
        return None;
    }
    Some(json!({ UPGRADES: totals.unwrap_or_else(|| json!({})), INPUT: input, OUTPUT: output }))
}

/// `TileComponentUpgrade.applyImplicitComponents` e depois `serialize`: o filho `component_upgrade` do tile que recebe o `UpgradeAware` do item. `None` quando não há nada a
/// guardar (o `write` do componente apaga o filho).
pub fn upgrades_apply(component: &Value) -> Option<Value> {
    let mut out = Map::new();
    if let Some(totals) = component
        .get(UPGRADES)
        .filter(|totals| totals.as_object().is_some_and(|map| !map.is_empty()))
    {
        out.insert(UPGRADES.to_owned(), totals.clone());
    }
    let mut items = Vec::new();
    for (index, key) in [INPUT, OUTPUT].into_iter().enumerate() {
        let stored = component.get(key).cloned().unwrap_or_else(|| json!({}));
        if !is_empty_stack(&stored) {
            items.push(json!({ CONTAINER: index, STORED: stored }));
        }
    }
    if !items.is_empty() {
        out.insert(ITEMS.to_owned(), Value::Array(items));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

/// `ResourceContainerType.attachedCopyOf`: a lista de contêineres, cada um `{}` (vazio) ou `{type, amount}`. `None` com todos vazios.
pub fn containers_collect(containers: &[Value]) -> Option<Value> {
    containers
        .iter()
        .any(|stored| !is_empty_stack(stored))
        .then(|| {
            let list = containers
                .iter()
                .map(|stored| {
                    if is_empty_stack(stored) {
                        json!({})
                    } else {
                        stored.clone()
                    }
                })
                .collect();
            json!({ CONTAINERS: Value::Array(list) })
        })
}

/// `ResourceContainerType.copyToContainers`: o conteúdo de cada um dos `size` contêineres do tile, ou `None` se a contagem do item não é a do tile (nada muda).
pub fn containers_apply(component: &Value, size: usize) -> Option<Vec<Value>> {
    let list = component.get(CONTAINERS)?.as_array()?;
    (list.len() == size).then(|| {
        list.iter()
            .map(|stored| {
                if is_empty_stack(stored) {
                    json!({})
                } else {
                    stored.clone()
                }
            })
            .collect()
    })
}

/// `ContainerType.saveTo`: a lista que o tile guarda (`[{container, stored}]`) vira a lista densa de `size` contêineres, os que faltam vazios.
pub fn containers_from_saved(saved: Option<&Value>, size: usize) -> Vec<Value> {
    let mut out = vec![json!({}); size];
    for entry in saved.and_then(Value::as_array).into_iter().flatten() {
        let (Some(index), Some(stored)) = (
            entry.get(CONTAINER).and_then(Value::as_u64),
            entry.get(STORED),
        ) else {
            continue;
        };
        if let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|index| out.get_mut(index))
        {
            *slot = stored.clone();
        }
    }
    out
}

/// `ContainerType.saveTo`: a lista densa vira a que o tile guarda, só com os contêineres que não estão vazios. `None` sem nenhum (o tile não escreve a chave).
pub fn containers_to_saved(containers: &[Value]) -> Option<Value> {
    let list: Vec<Value> = containers
        .iter()
        .enumerate()
        .filter(|(_, stored)| !is_empty_stack(stored))
        .map(|(index, stored)| json!({ CONTAINER: index, STORED: stored }))
        .collect();
    (!list.is_empty()).then_some(Value::Array(list))
}

/// `TileEntityMekanism.collectImplicitComponents`: `REDSTONE_CONTROL` com o modo do tile. `DISABLED` é o padrão do item e não vira componente.
pub fn redstone_collect(control: &str) -> Option<Value> {
    (control != REDSTONE_DISABLED).then(|| Value::String(control.to_owned()))
}

/// `HeatContainerType.attachedCopyOf`: um capacitor na temperatura ambiente dá só a capacidade (igual ao padrão do item, sem componente); senão o calor e a capacidade.
pub fn heat_collect(heat: f64, capacity: f64, ambient: bool) -> Option<Value> {
    (!ambient).then(|| json!({ HEAT_STORED: heat, HEAT_CAPACITY: capacity }))
}

/// `HeatContainerType.copyToContainer`: o calor e a capacidade do item (`None` no calor quando o item só traz a capacidade).
pub fn heat_apply(component: &Value) -> (Option<f64>, f64) {
    let heat = component.get(HEAT_STORED).and_then(Value::as_f64);
    let capacity = component
        .get(HEAT_CAPACITY)
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    (heat, capacity)
}

/// `ItemBlockTooltip`, o componente `ATTACHED_ITEMS` (`MekanismLang.HAS_INVENTORY`): se o item guarda alguma pilha. Sem o componente (o padrão, tudo vazio) é `false`.
pub fn has_inventory(items: Option<&Value>) -> bool {
    items
        .and_then(|component| component.get(CONTAINERS))
        .and_then(Value::as_array)
        .is_some_and(|list| list.iter().any(|stored| !is_empty_stack(stored)))
}
