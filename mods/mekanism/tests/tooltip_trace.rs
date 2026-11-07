//! Repete os rastros de `upgrade_tooltip`: as linhas que o item de upgrade tem no original, tipo a tipo.
mod common;

use mekanism_rules::{ItemTooltipLine, TooltipArg, upgrade_tooltip};
use serde_json::Value;
use std::path::Path;

/// `ComponentSerialization.CODEC` de uma cor `#RRGGBB`.
fn hex(color: u32) -> String {
    format!("#{color:06X}")
}

/// A linha em JSON de componente, como o Java a grava.
fn component(line: &ItemTooltipLine) -> Value {
    let args: Vec<Value> = line
        .args
        .iter()
        .map(|arg| match arg {
            TooltipArg::Translate { key, color } => {
                serde_json::json!({"translate": key, "color": hex(*color)})
            }
            TooltipArg::Number { value, color } => {
                serde_json::json!({"text": value.to_string(), "color": hex(*color)})
            }
        })
        .collect();
    let mut json = serde_json::json!({"translate": line.translate});
    if !args.is_empty() {
        json["with"] = Value::Array(args);
    }
    if let Some(color) = line.color {
        json["color"] = Value::String(hex(color));
    }
    json
}

#[test]
fn every_upgrade_type_has_the_lines_of_the_original() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/upgrade_tooltip.jsonl");
    let types = common::upgrade_types();
    let mut checked = 0;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let expected: Value = serde_json::from_str(line).unwrap();
        let name = expected["marks"][0].as_str().unwrap();
        // A primeira linha é o nome do item, que o jogo põe; as outras são do appender.
        let extra: Vec<Value> = expected["lines"].as_array().unwrap()[1..].to_vec();
        if name == "no type" {
            assert!(extra.is_empty(), "um upgrade sem tipo só tem o nome");
            continue;
        }
        let id = format!("mekanism:{name}");
        let kind = types.get(&id).unwrap_or_else(|| panic!("tipo {id}"));
        let actual: Vec<Value> = upgrade_tooltip(&id, kind).iter().map(component).collect();
        assert_eq!(actual, extra, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 8);
}
