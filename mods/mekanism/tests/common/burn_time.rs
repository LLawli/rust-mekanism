//! O tempo de queima de um item como o Horizon o lê: o componente `minecraft:cooking_fuel` do JSON do item e o `Providers::eval` de
//! `horizon-client-render/src/providers.rs` (pin `f319701`), que avalia o provider com o contexto `fast_cooking` do bloco que queima. Os
//! providers do vanilla que o `cooking_fuel` cita (`minecraft:cooking/normal_burn_time_reduction_factor` e `fast_burn_time_reduction_factor`) valem 1 e
//! 2 (`ContextIntProviders.COOKING_NORMAL_BURN_TIME_REDUCTION_FACTOR` e `COOKING_FAST_BURN_TIME_REDUCTION_FACTOR`, `MekanismDatapackRegistries.java:225-226`).
//! Os testes que o usam incluem este arquivo por `#[path]`.
#![allow(dead_code)]
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `ContextIntProviders.COOKING_NORMAL_BURN_TIME_REDUCTION_FACTOR` e `COOKING_FAST_BURN_TIME_REDUCTION_FACTOR`: os dois valores que o `div` do tempo de
/// queima divide por, no bloco comum e no que cozinha mais depressa (o alto-forno e o defumador).
const NORMAL_FACTOR: f64 = 1.0;
const FAST_FACTOR: f64 = 2.0;

/// O JSON de um item do Mekanism, de `content/data/mekanism/horizon/item`.
pub fn item_json(name: &str) -> Value {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/data/mekanism/horizon/item")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// O `burn_time` do componente `minecraft:cooking_fuel` do item, se ele tem o componente.
pub fn burn_time_provider(item: &Value) -> Option<Value> {
    item["components"]["minecraft:cooking_fuel"]
        .get("burn_time")
        .cloned()
}

/// `Providers::eval` do Horizon, para os tipos que o `cooking_fuel` usa. Uma referência a um provider do vanilla que o Horizon lê de
/// `data/minecraft/context_int_provider` vale o que o vanilla dá a ele.
fn eval(value: &Value, fast: bool) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap_or(0.0),
        Value::String(name) => match name.as_str() {
            "minecraft:cooking/normal_burn_time_reduction_factor" => NORMAL_FACTOR,
            "minecraft:cooking/fast_burn_time_reduction_factor" => FAST_FACTOR,
            other => panic!("provider que o teste não conhece: {other}"),
        },
        Value::Object(object) => {
            let field = |key: &str| eval(object.get(key).unwrap_or(&Value::Null), fast);
            match object.get("type").and_then(Value::as_str).unwrap_or("") {
                "minecraft:div" => {
                    let right = field("right");
                    if right == 0.0 {
                        0.0
                    } else {
                        (field("left") / right).trunc()
                    }
                }
                "minecraft:conditional" => {
                    let condition = object.get("condition").and_then(Value::as_str)
                        == Some("minecraft:block/fast_cooking");
                    if condition && fast {
                        field("on_true")
                    } else {
                        field("on_false")
                    }
                }
                other => panic!("tipo de provider que o teste não conhece: {other}"),
            }
        }
        _ => 0.0,
    }
}

/// `Providers::burn_time`: o tempo de queima do item (0 sem o componente), com o contexto `fast_cooking` do bloco.
pub fn burn_time(item: &Value, fast: bool) -> i32 {
    burn_time_provider(item).map_or(0, |provider| eval(&provider, fast) as i32)
}
