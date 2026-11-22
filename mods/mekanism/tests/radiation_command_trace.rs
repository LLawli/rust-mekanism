//! As mensagens de `/mek radiation` contra o rastro do original (`RadiationTraceTest`, `radiation_command.jsonl`): cada resposta de sucesso que o jogador
//! leu, em JSON (`ComponentSerialization.CODEC`), com as cores, o texto da radiação e a cor fora do lugar de `addEntity <alvos>`. As fontes que `get` lê saem
//! da mesma conta do port (`RadiationLevelData`), com os `add` do rastro na ordem.
use machinery_basis_core::radiation::{
    BASELINE, RadiationLevelData, radiate_dose, reduce_dose, set_dose,
};
use mekanism_rules::network_reader::{Arg, Body, Comp};
use mekanism_rules::radiation_command as messages;
use serde_json::Value;
use std::path::Path;

/// `RadiationLevelData.MAX_BLOCK_RANGE` com o `chunkCheckRadius` padrão.
const BLOCK_RANGE: i32 = 80;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/radiation_command.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn block(value: &Value) -> [i32; 3] {
    let list = value.as_array().unwrap();
    [0, 1, 2].map(|i| list[i].as_i64().unwrap() as i32)
}

/// `Vec3Argument.vec3()` (inteiros no centro do bloco) e `BlockPos.containing`.
fn location(words: &[&str]) -> [i32; 3] {
    [0, 1, 2].map(|i| {
        let word = words[i];
        let value: f64 = word.parse().unwrap();
        let centered = if word.contains('.') {
            value
        } else {
            value + 0.5
        };
        centered.floor() as i32
    })
}

/// O nome que o rastro gravou (o jogador de teste ou a vaca `Mimosa`), sem a cor que a mensagem põe.
fn name(json: &Value) -> Arg {
    let mut raw = json.clone();
    raw.as_object_mut().unwrap().remove("color");
    Arg::Comp(Comp {
        body: Body::Raw(raw),
        color: None,
    })
}

#[test]
fn every_answer_of_the_command_is_the_component_of_the_original() {
    let rows = rows();
    let player = block(&rows[0]["player"]);
    let mut level = RadiationLevelData::new();
    let mut dose = BASELINE;
    let mut cow = BASELINE;
    let mut checked = 0;
    for row in rows.iter().filter(|row| row["success"] == true) {
        let command = row["command"].as_str().unwrap();
        let words: Vec<&str> = command.split(' ').collect();
        let messages_json = row["messages"].as_array().unwrap();
        let target = words.get(3).is_some_and(|word| word.starts_with('@'));
        let cow_only = command.contains("type=minecraft:cow");
        let expected: Vec<Comp> = match words[2] {
            "add" => {
                let magnitude: f64 = words[3].parse().unwrap();
                let pos = if words.len() > 4 {
                    location(&words[4..7])
                } else {
                    player
                };
                level.radiate(pos, magnitude, BLOCK_RANGE);
                vec![messages::add(magnitude, pos, "minecraft:overworld")]
            }
            "get" => {
                let pos = if words.len() > 3 {
                    location(&words[3..6])
                } else {
                    player
                };
                vec![messages::get(
                    pos,
                    "minecraft:overworld",
                    level.level_and_max(pos).level,
                )]
            }
            "addEntity" if !target => {
                let magnitude: f64 = words[3].parse().unwrap();
                dose = radiate_dose(dose, magnitude);
                vec![messages::add_entity(magnitude)]
            }
            "addEntity" => {
                let magnitude: f64 = words[4].parse().unwrap();
                cow = radiate_dose(cow, magnitude);
                if cow_only {
                    let with = messages_json[0]["with"].as_array().unwrap();
                    vec![messages::add_entity_targets(
                        magnitude,
                        1,
                        Some(name(&with[2])),
                    )]
                } else {
                    dose = radiate_dose(dose, magnitude);
                    // Quem roda dá sempre o nome da primeira entidade (a cola); só a contagem escolhe a mensagem.
                    let first = Arg::Str("primeira".to_owned());
                    vec![messages::add_entity_targets(magnitude, 2, Some(first))]
                }
            }
            "reduce" if !target => {
                let magnitude: f64 = words[3].parse().unwrap();
                let (after, reduced) = reduce_dose(dose, magnitude);
                dose = after;
                vec![messages::reduce(reduced)]
            }
            "reduce" => {
                // Uma mensagem por entidade, na ordem do seletor (a do rastro: o nome sai dele).
                let magnitude: f64 = words[4].parse().unwrap();
                messages_json
                    .iter()
                    .map(|message| {
                        let who = &message["with"].as_array().unwrap()[0];
                        let is_cow = who["text"] == "Mimosa";
                        let current = if is_cow { &mut cow } else { &mut dose };
                        let (after, reduced) = reduce_dose(*current, magnitude);
                        *current = after;
                        messages::reduce_target(name(who), reduced)
                    })
                    .collect()
            }
            "heal" if !target => {
                dose = set_dose(BASELINE);
                vec![messages::clear()]
            }
            "heal" => {
                cow = BASELINE;
                if cow_only {
                    let with = messages_json[0]["with"].as_array().unwrap();
                    vec![messages::clear_targets(1, Some(name(&with[0])))]
                } else {
                    dose = BASELINE;
                    vec![messages::clear_targets(
                        2,
                        Some(Arg::Str("primeira".to_owned())),
                    )]
                }
            }
            "removeAll" => {
                level.clear();
                vec![messages::remove_all()]
            }
            other => panic!("{other}"),
        };
        let got: Vec<Value> = expected.iter().map(Comp::to_json).collect();
        assert_eq!(&got, messages_json, "{command}");
        assert_eq!(
            row["dose"].as_f64().unwrap().to_bits(),
            dose.to_bits(),
            "{command}"
        );
        checked += 1;
    }
    assert_eq!(checked, 23);
}

#[test]
fn the_count_of_the_tracker_is_single_only_for_one_entity() {
    // `CommandResponseTracker.dispatch`: sem elemento (nenhuma entidade viva achada) é a mensagem de várias, com 0.
    let none = messages::clear_targets(0, None).to_json();
    assert_eq!(
        none["translate"],
        "command.mekanism.radiation.clear.entity.multiple"
    );
    assert_eq!(none["with"][0]["text"], "0");
    let one = messages::add_entity_targets(0.5, 1, Some(Arg::Str("x".to_owned()))).to_json();
    assert_eq!(
        one["translate"],
        "command.mekanism.radiation.add_entity.target"
    );
}

#[test]
fn the_radiation_node_needs_an_operator_and_hands_its_arguments_to_the_runner() {
    use machinery_basis_core::commands::{Parsed, parse};
    let runs = mekanism_rules::command::RadiationRuns {
        add: 1,
        add_entity: 2,
        get: 3,
        heal: 4,
        reduce: 5,
        remove_all: 6,
    };
    let tree = mekanism_rules::command::tree(0, runs);
    let run = |line: &'static str, level: u8| match parse(&tree, line, level) {
        Some(Parsed::Run(run)) => Some((*run, "")),
        Some(Parsed::RunArgs(run, rest)) => Some((*run, rest)),
        _ => None,
    };
    assert_eq!(run("mek radiation get", 2), Some((3, "")));
    assert_eq!(
        run("mek radiation get ~ ~ ~5 minecraft:the_nether", 2),
        Some((3, "~ ~ ~5 minecraft:the_nether"))
    );
    assert_eq!(run("mek radiation add 0.5", 2), Some((1, "0.5")));
    assert_eq!(
        run("mek radiation addEntity @e[type=cow] 1", 2),
        Some((2, "@e[type=cow] 1"))
    );
    assert_eq!(run("mek radiation heal", 2), Some((4, "")));
    assert_eq!(run("mek radiation reduce 5", 2), Some((5, "5")));
    assert_eq!(run("mek radiation removeAll", 2), Some((6, "")));
    // `nodeOpCommand`: nível 2; abaixo dele o `radiation` não existe para quem digita.
    assert_eq!(run("mek radiation get", 0), None);
    // `add` sem magnitude não roda.
    assert_eq!(run("mek radiation add", 2), None);
}

#[test]
fn the_bounds_of_the_magnitude_are_written_as_java_writes_them() {
    // As linhas `erro` do rastro: `argument.double.low` com `[4.9E-324, 0.0]` e `argument.double.big` com `[10000.0, 20000.0]`.
    use machinery_basis_core::units::java_double_to_string as java;
    assert_eq!(java(f64::from_bits(1)), "4.9E-324");
    assert_eq!(java(0.0), "0.0");
    assert_eq!(java(10_000.0), "10000.0");
    assert_eq!(java(20_000.0), "20000.0");
}
