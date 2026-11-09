//! O estado em que o bloco é posto, contra o original em jogo: `dev/referencia/rastros/blocos_colocacao.jsonl` (`testes/BlocosTraceTest.java`) grava, por bloco do Mekanism, o `facing`
//! que `BlockStateHelper.getStateForPlacement` dá a um jogador a cada guinada (de -720 a 720 graus, de 3 em 3), a cada inclinação (de -90 a 90) e a cada face clicada, e o
//! `fluid_logged` com água, lava e ar no lugar. As regras são as de `data/mekanism/machinery_basis/placement/<bloco>.json`, lidas do conteúdo.

use machinery_basis_core::placement::{self, Rule};
use serde_json::Value;
use std::path::PathBuf;

/// Os blocos que o port tinha antes dos blocos de máquina: as quatro máquinas usam o gabarito do forno, que se vira sozinho, e os cabos ligam os lados pela energia.
const EARLIER: [&str; 8] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
    "basic_universal_cable",
    "advanced_universal_cable",
    "elite_universal_cable",
    "ultimate_universal_cable",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn rastro() -> Vec<Value> {
    let path = root().join("dev/referencia/rastros/blocos_colocacao.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Se o bloco está no port: o rastro tem todos os blocos do original, inclusive os das frentes que ainda não chegaram.
fn ported(block: &str) -> bool {
    root()
        .join(format!(
            "mods/mekanism/content/data/mekanism/horizon/block/{block}.json"
        ))
        .is_file()
}

fn rule(block: &str) -> Option<Rule> {
    let path = root().join(format!(
        "mods/mekanism/content/data/mekanism/machinery_basis/placement/{block}.json"
    ));
    let text = std::fs::read_to_string(path).ok()?;
    Some(Rule::parse(&serde_json::from_str(&text).unwrap()))
}

/// Os blocos do Mekanism com `facing` no original, todos com regra no conteúdo, e o que a regra dá é o que o original dá em cada ponto da varredura.
#[test]
fn the_facing_is_the_one_the_original_gives_at_every_yaw_pitch_and_face() {
    let mut checked = 0;
    let mut points = 0;
    for row in rastro() {
        let Some(placement) = row["placement"].as_object() else {
            continue;
        };
        if !placement.contains_key("yaw") {
            continue;
        }
        let id = row["id"].as_str().unwrap();
        let name = id.strip_prefix("mekanism:").unwrap();
        if !ported(name) {
            continue;
        }
        let Some(rule) = rule(name) else {
            assert!(
                EARLIER.contains(&name),
                "{id} tem facing no original e não tem regra de colocação"
            );
            continue;
        };
        let facing = rule
            .facing
            .as_ref()
            .unwrap_or_else(|| panic!("{id}: sem facing na regra"));
        let from = placement["yaw_from"].as_i64().unwrap();
        let step = placement["yaw_step"].as_i64().unwrap();
        for (index, want) in placement["yaw"].as_array().unwrap().iter().enumerate() {
            let yaw = (from + step * index as i64) as f32;
            let got = facing.direction(yaw, 0.0, "up");
            assert_eq!(got, want.as_str(), "{id}: guinada {yaw}");
            points += 1;
        }
        for (index, want) in placement["pitch"].as_array().unwrap().iter().enumerate() {
            let pitch = (placement["pitch_from"].as_i64().unwrap() + index as i64) as f32;
            let got = facing.direction(0.0, pitch, "up");
            assert_eq!(got, want.as_str(), "{id}: inclinação {pitch}");
            points += 1;
        }
        let faces = ["down", "up", "north", "south", "west", "east"];
        for (face, want) in faces.iter().zip(placement["face"].as_array().unwrap()) {
            let got = facing.direction(0.0, 0.0, face);
            assert_eq!(got, want.as_str(), "{id}: face {face}");
            points += 1;
        }
        checked += 1;
    }
    assert!(checked >= 70, "{checked}");
    assert!(points >= 70 * 500, "{points}");
}

/// O fluido do lugar: água e lava (fontes) enchem o bloco, o ar o deixa vazio.
#[test]
fn the_fluid_logged_value_is_the_one_the_original_gives() {
    let mut checked = 0;
    for row in rastro() {
        let Some(fluids) = row["placement"]["fluid"].as_object() else {
            continue;
        };
        let id = row["id"].as_str().unwrap();
        let name = id.strip_prefix("mekanism:").unwrap();
        if !ported(name) {
            continue;
        }
        let Some(rule) = rule(name) else {
            assert!(
                EARLIER.contains(&name),
                "{id} tem fluido no original e não tem regra de colocação"
            );
            continue;
        };
        assert_eq!(rule.fluid_logged.as_deref(), Some("fluid_logged"), "{id}");
        for (fluid, want) in fluids {
            assert_eq!(
                placement::fluid_logged_value(fluid),
                want.as_str().unwrap(),
                "{id}: {fluid}"
            );
        }
        checked += 1;
    }
    assert!(checked >= 60, "{checked}");
}

/// Mutação conhecida: a rodada de 90 graus deve estar certa nas quatro direções (a guinada 0 olha o sul e vira o bloco para o norte).
#[test]
fn the_yaw_zero_faces_the_block_north() {
    let rule = rule("combiner").unwrap();
    assert_eq!(
        rule.facing.unwrap().direction(0.0, 0.0, "up"),
        Some("north")
    );
}
