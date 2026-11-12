//! O módulo `logged-fluid-facts` da `machinery_basis` (`mods/machinery_basis/logged_fluid_facts.rs`), que o launcher enxerta em `game.rs`: o fluido que o valor de `fluid_logged` põe no
//! `StateFacts` de um estado (`IStateFluidLoggable.getFluid`: a fonte, `getSource(false)`, do fluido do `FluidLogType`). O arquivo só usa `std`; o teste o compila como está e o confere
//! contra `FluidLogType.java` (`empty`, `water`, `lava`, na ordem do enum) e contra o que os blocos do port declaram. Quem põe o valor na tabela do Horizon é o HEAD
//! `logged_fluid_facts` (conferido em jogo: `h2_fluido_no_bloco.py mob`, contra `rastros/fluido_no_bloco_entity.jsonl`).
#[allow(dead_code)]
mod logged {
    include!("../../logged_fluid_facts.rs");
}

use logged::{SOURCE_LEVEL, Source, source};
use std::path::{Path, PathBuf};

fn mods() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn water_and_lava_are_sources_and_empty_is_nothing() {
    assert_eq!(source(Some("water")), Some(Source::Water));
    assert_eq!(source(Some("lava")), Some(Source::Lava));
    assert_eq!(source(Some("empty")), None);
    // O estado de um bloco sem `fluid_logged` (a propriedade falta) e um valor que o original não tem não dão fluido.
    assert_eq!(source(None), None);
    assert_eq!(source(Some("Water")), None);
    assert_eq!(source(Some("waterlogged")), None);
}

/// `FlowingFluid.getSource(false)`: a fonte tem a quantidade 8 e não cai, a mesma que o Horizon dá a `waterlogged` e ao nível 0 do bloco de líquido (`(Fluid::Water, 8, false)`).
#[test]
fn a_source_has_the_full_amount() {
    assert_eq!(SOURCE_LEVEL, 8);
}

/// Todo valor de `fluid_logged` que um bloco do port declara é um valor do `FluidLogType` do original, e o módulo trata cada um como o original: `empty` sem fluido, `water` e `lava`
/// com a fonte.
#[test]
fn every_declared_value_is_a_fluid_log_type() {
    let java = std::fs::read_to_string(
        "/mnt/dados/caches/rust-mekanism-ref/mekanism/src/main/java/mekanism/common/block/states/FluidLogType.java",
    );
    // O original só existe na máquina que o clonou; sem ele o teste confere o que os blocos declaram contra a lista fixa do enum (`empty`, `water`, `lava`).
    let names: Vec<String> = match &java {
        Ok(text) => text
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let (_, rest) = line.split_once('(')?;
                let name = rest.strip_prefix('"')?.split('"').next()?;
                (line.starts_with(|c: char| c.is_ascii_uppercase())).then(|| name.to_owned())
            })
            .collect(),
        Err(_) => ["empty", "water", "lava"].map(str::to_owned).to_vec(),
    };
    assert_eq!(names, ["empty", "water", "lava"]);
    let mut declared = std::collections::BTreeSet::new();
    let mut blocks = 0;
    for content in ["mekanism/content", "mekanismgenerators/content"] {
        let dir = mods().join(content).join("data");
        for entry in walk(&dir) {
            let Ok(text) = std::fs::read_to_string(&entry) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(values) = json["state_properties"]["fluid_logged"]["values"].as_array() else {
                continue;
            };
            blocks += 1;
            for value in values {
                declared.insert(value.as_str().unwrap().to_owned());
            }
        }
    }
    assert!(blocks >= 20, "só {blocks} blocos declaram fluid_logged");
    for value in &declared {
        assert!(names.contains(value), "{value} não é um FluidLogType");
    }
    assert!(
        declared.contains("water") && declared.contains("lava") && declared.contains("empty"),
        "{declared:?}"
    );
    for name in &names {
        let got = source(Some(name));
        assert_eq!(got.is_some(), name != "empty", "{name}");
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
    out
}
