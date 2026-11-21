//! As linhas que os atributos de um químico acrescentam ao tooltip (`ChemicalStack.getTooltipLines`), contra o rastro do cliente do original
//! (`dev/referencia/rastros/chemical_tooltips.jsonl`, gravado pela `CenaTelas`: todo químico do registro, sem e com os tooltips avançados, cada linha
//! como trechos `[texto, cor]`). O port monta as linhas com os mapas de dados e o idioma do próprio conteúdo.

use machinery_basis_core::temperature::TemperatureUnit;
use mekanism_rules::{
    AttributeTooltip, CHEMICAL_ATTRIBUTE_FOLDER, Chat, ChemicalAttributeData, DECAY_BLACKLIST_TAG,
    chemical_attribute_data, chemical_attribute_lines, chemical_registry_line,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn json(path: PathBuf) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")),
    )
    .unwrap()
}

fn data() -> HashMap<String, ChemicalAttributeData> {
    let folder = root().join(format!("content/data/mekanism/{CHEMICAL_ATTRIBUTE_FOLDER}"));
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        files.push((format!("mekanism:{name}"), json(path)));
    }
    assert_eq!(files.len(), 4, "os quatro mapas de dados dos químicos");
    chemical_attribute_data(&files)
}

fn lang() -> HashMap<String, String> {
    let lang = json(root().join("content/assets/mekanism/lang/en_us.json"));
    lang.as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
        .collect()
}

fn decay_immune(chemical: &str) -> bool {
    let (namespace, path) = DECAY_BLACKLIST_TAG.split_once(':').unwrap();
    let tag = json(root().join(format!(
        "content/data/{namespace}/tags/mekanism/chemical/{path}.json"
    )));
    tag["values"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str() == Some(chemical))
}

type Piece = (String, Option<u32>);

/// Um `Chat` nos trechos que `Component.visit` entrega: o modelo de idioma na cor do componente (ou na herdada) e cada argumento na dele, com os vizinhos de
/// mesma cor juntos.
fn pieces(
    chat: &Chat,
    inherited: Option<u32>,
    lang: &HashMap<String, String>,
    out: &mut Vec<Piece>,
) {
    let color = chat.color.or(inherited);
    let push = |text: &str, color: Option<u32>, out: &mut Vec<Piece>| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.1 == color => last.0.push_str(text),
            _ => out.push((text.to_owned(), color)),
        }
    };
    if let Some(literal) = &chat.literal {
        push(literal, color, out);
        return;
    }
    let template = lang
        .get(&chat.translate)
        .unwrap_or_else(|| panic!("sem a chave {}", chat.translate));
    let mut rest = template.as_str();
    while let Some(at) = rest.find('%') {
        push(&rest[..at], color, out);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("%%") {
            push("%", color, out);
            rest = after;
        } else if let Some(after) = tail.strip_prefix("%1$s") {
            pieces(&chat.with[0], color, lang, out);
            rest = after;
        } else {
            panic!("modelo {template:?}");
        }
    }
    push(rest, color, out);
}

fn tip<'a>(chemical: &str, translate: &'a dyn Fn(&str) -> String) -> AttributeTooltip<'a> {
    AttributeTooltip {
        radiation_enabled: true,
        decay_immune: decay_immune(chemical),
        temperature_unit: TemperatureUnit::Kelvin,
        compact_energy: true,
        translate,
    }
}

fn recorded(line: &Value) -> Vec<Piece> {
    line.as_array()
        .unwrap()
        .iter()
        .map(|piece| {
            let color = piece[1]
                .as_str()
                .map(|hex| u32::from_str_radix(&hex[1..], 16).unwrap());
            (piece[0].as_str().unwrap().to_owned(), color)
        })
        .collect()
}

/// Todo químico do registro do original: as linhas depois da primeira (o nome) são as dos atributos, e com os tooltips avançados a última é o nome de registro.
#[test]
fn every_chemical_gets_the_attribute_lines_of_the_original() {
    let trace = std::fs::read_to_string(
        root().join("../../dev/referencia/rastros/chemical_tooltips.jsonl"),
    )
    .expect("o rastro dos tooltips dos químicos");
    let data = data();
    let lang = lang();
    let translate = |key: &str| lang[key].clone();
    let mut seen = 0;
    let mut with_attributes = 0;
    for row in trace.lines() {
        let row: Value = serde_json::from_str(row).unwrap();
        let id = row["id"].as_str().unwrap();
        let chats = data
            .get(id)
            .map(|data| chemical_attribute_lines(data, &tip(id, &translate)))
            .unwrap_or_default();
        let mut ours: Vec<Vec<Piece>> = chats
            .iter()
            .map(|chat| {
                let mut out = Vec::new();
                pieces(chat, None, &lang, &mut out);
                out
            })
            .collect();
        let normal: Vec<Vec<Piece>> = row["normal"].as_array().unwrap()[1..]
            .iter()
            .map(recorded)
            .collect();
        assert_eq!(ours, normal, "{id}");
        let mut registry = Vec::new();
        pieces(&chemical_registry_line(id), None, &lang, &mut registry);
        ours.push(registry);
        let advanced: Vec<Vec<Piece>> = row["advanced"].as_array().unwrap()[1..]
            .iter()
            .map(recorded)
            .collect();
        assert_eq!(ours, advanced, "{id}, avançado");
        seen += 1;
        with_attributes += usize::from(!normal.is_empty());
    }
    assert!(
        seen >= 60,
        "o registro do original tem dezenas de químicos: {seen}"
    );
    assert_eq!(
        with_attributes,
        data.len(),
        "todo químico com atributo no conteúdo aparece no rastro com linhas"
    );
}

/// As contas que o conteúdo do original não exercita: sem a radiação nada sai do atributo de radioatividade, a energia por extenso, a outra unidade de
/// temperatura, a condutividade arredondada (`Math.round`) e o produto do combustível preso ao `int` (`MathUtils.multiplyClamped`).
#[test]
fn the_lines_follow_the_configuration_and_the_values() {
    let lang = lang();
    let translate = |key: &str| lang[key].clone();
    let plain = |data: &ChemicalAttributeData, tip: &AttributeTooltip<'_>| -> Vec<String> {
        chemical_attribute_lines(data, tip)
            .iter()
            .map(|chat| {
                let mut out = Vec::new();
                pieces(chat, None, &lang, &mut out);
                out.into_iter().map(|piece| piece.0).collect()
            })
            .collect()
    };
    let waste = ChemicalAttributeData {
        radioactivity: Some(0.02),
        ..Default::default()
    };
    let mut config = tip("mekanism:plutonium", &translate);
    assert_eq!(
        plain(&waste, &config),
        [
            " - Radioactivity: 20 mSv/h",
            "Will not decay inside a Radioactive Waste Barrel"
        ]
    );
    config.decay_immune = false;
    assert_eq!(plain(&waste, &config), [" - Radioactivity: 20 mSv/h"]);
    config.radiation_enabled = false;
    assert!(plain(&waste, &config).is_empty());

    let fuel = ChemicalAttributeData {
        fuel: Some((2_000, 1_500_000)),
        ..Default::default()
    };
    let mut config = tip("mekanism:hydrogen", &translate);
    assert_eq!(
        plain(&fuel, &config),
        [
            " - Maximum Burn: 2,000/t",
            " - Energy Density: 1.5 MFE/mB",
            " - Maximum Output: 2.14 GFE/t"
        ]
    );
    config.compact_energy = false;
    assert_eq!(
        plain(&fuel, &config),
        [
            " - Maximum Burn: 2,000/t",
            " - Energy Density: 1,500,000 FE/mB",
            " - Maximum Output: 2,147,483,647 FE/t"
        ]
    );

    let coolant = ChemicalAttributeData {
        cooled: Some((7.9, 0.125)),
        heated: Some((5.0, 0.4, 100_000.0)),
        ..Default::default()
    };
    let mut config = tip("mekanism:sodium", &translate);
    assert_eq!(
        plain(&coolant, &config),
        [
            " - Coolant Efficiency: 13%",
            " - Thermal Enthalpy: 7 FE/mB",
            " - Coolant Efficiency: 40%",
            " - Thermal Enthalpy: 5 FE/mB",
            " - Temperature: 100kK"
        ]
    );
    config.temperature_unit = TemperatureUnit::Celsius;
    assert_eq!(plain(&coolant, &config)[4], " - Temperature: 99.72k°C");
}

/// Os arquivos: o número sozinho da radioatividade, os padrões dos refrigerantes, `remove` e as chaves de tag.
#[test]
fn the_data_maps_give_the_values_with_the_defaults_of_the_codecs() {
    let data = data();
    assert_eq!(data["mekanism:hydrogen"].fuel, Some((10, 2)));
    assert_eq!(data["mekanism:polonium"].radioactivity, Some(0.05));
    assert_eq!(data["mekanism:sodium"].cooled, Some((5.0, 1.0)));
    assert_eq!(
        data["mekanism:superheated_sodium"].heated,
        Some((5.0, 0.4, 100_000.0))
    );
    let files = vec![
        (
            "mekanism:chemical_attribute_radioactivity".to_owned(),
            serde_json::json!({"values": {"a:x": 0.5, "a:y": {"radioactivity": 2.0}, "#a:tag": 1.0}}),
        ),
        (
            "mekanism:chemical_attribute_fuel".to_owned(),
            serde_json::json!({"values": {"a:x": {"energy": 3, "maxBurnPerTick": 4}, "a:z": {"energy": 1, "maxBurnPerTick": 1}}, "remove": ["a:z"]}),
        ),
        (
            "mekanism:other".to_owned(),
            serde_json::json!({"values": {"a:w": 1.0}}),
        ),
    ];
    let data = chemical_attribute_data(&files);
    assert_eq!(data.len(), 2);
    assert_eq!(
        data["a:x"],
        ChemicalAttributeData {
            radioactivity: Some(0.5),
            fuel: Some((4, 3)),
            ..Default::default()
        }
    );
    assert_eq!(data["a:y"].radioactivity, Some(2.0));
}
