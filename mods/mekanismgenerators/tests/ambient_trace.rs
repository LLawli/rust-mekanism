//! Repete, tick a tick, os rastros do Heat Generator em alturas, biomas e colunas diferentes (`dev/referencia/rastros/heat_ambient_*.jsonl`,
//! do `HeatAmbientTraceTest`): o ar de cada gerador vem do bioma, da posição e do nível do mar pela `machinery_basis_core`
//! (`biome_temperature`), guardado como o `CachedAmbientTemperature` faz (`heat::AmbientCache`), e a regra do tick do gerador
//! (`step`) parte dele. Tudo igual ao bit: o ar do bloco e de cada face, o calor, a temperatura, a produção, a perda para o ambiente.
//!
//! Cada linha é um tick, gravado depois do tick do tile e das marcas do cenário. Uma marca `biome` troca o bioma debaixo do gerador
//! (o rastro mostra o bioma novo, e o tile segue lendo o ar guardado) e `reload` recria o tile do NBT (um cache novo).
use machinery_basis_core::biome_temperature::{Climate, sea_level};
use machinery_basis_core::heat::AmbientCache;
use machinery_basis_core::sides::Face;
use mekanismgenerators_rules::{State, Surroundings, step};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

struct Generator {
    pos: [i32; 3],
    state: State,
    cache: AmbientCache,
}

fn read(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// As linhas guardam o bioma de cada posição como índice em `biome_ids` do cabeçalho: aqui voltam a ser o id.
fn expand_biomes(lines: &mut [Value]) {
    let ids = lines[0]["biome_ids"].as_array().unwrap().clone();
    for line in &mut lines[1..] {
        for entry in line["gens"].as_array_mut().unwrap() {
            let named: Vec<Value> = entry["biomes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|index| ids[index.as_u64().unwrap() as usize].clone())
                .collect();
            entry["biomes"] = Value::Array(named);
        }
    }
}

fn climates(header: &Value) -> HashMap<String, Climate> {
    header["biomes"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, biome)| {
            let bits = biome["temperature_bits"].as_i64().unwrap() as i32 as u32;
            (
                id.clone(),
                Climate {
                    temperature: f32::from_bits(bits),
                    frozen: biome["modifier"] == "frozen",
                },
            )
        })
        .collect()
}

/// O que o tile faz ao ler o ar: cada valor é calculado na primeira vez e guardado (`CachedAmbientTemperature.getTemperature`).
fn cached_ambient(
    generator: &mut Generator,
    biomes: &[Value],
    climates: &HashMap<String, Climate>,
    sea: i32,
) -> (f64, [f64; 6]) {
    let pos = generator.pos;
    let at =
        |index: usize, at: [i32; 3]| climates[biomes[index].as_str().unwrap()].ambient_at(at, sea);
    let ambient = generator
        .cache
        .get(AmbientCache::SELF_INDEX, || Some(at(0, pos)));
    let mut faces = [0.0; 6];
    for (index, face) in Face::ALL.into_iter().enumerate() {
        let [x, y, z] = face.offset();
        let neighbour = [pos[0] + x, pos[1] + y, pos[2] + z];
        faces[index] = generator
            .cache
            .get(index, || Some(at(index + 1, neighbour)));
    }
    (ambient, faces)
}

fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

fn replay(scenario: &str, ticks: usize) {
    let mut lines = read(scenario);
    expand_biomes(&mut lines);
    let header = &lines[0];
    assert!(header["header"].as_bool().unwrap());
    let sea = header["sea_level"].as_i64().unwrap() as i32;
    // O mundo de teste é plano: o nível do mar do overworld é -63, que é o que a regra dá.
    assert_eq!(sea, sea_level("minecraft:overworld", true), "{scenario}");
    assert_eq!(
        header["epsilon"].as_f64().unwrap(),
        machinery_basis_core::heat::EPSILON
    );
    let climates = climates(header);
    let mut generators: Vec<Generator> = header["gens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let pos: Vec<i32> = entry["pos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect();
            Generator {
                pos: [pos[0], pos[1], pos[2]],
                state: State::new(300.0),
                cache: AmbientCache::default(),
            }
        })
        .collect();
    let body = &lines[1..];
    assert_eq!(body.len(), ticks, "{scenario}: ticks gravados");
    let mut elevated = 0;
    let mut produced_total = 0;
    for line in body {
        let t = line["t"].as_u64().unwrap();
        let marks: Vec<&str> = line["marks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|mark| mark.as_str().unwrap().split(", "))
            .collect();
        for (index, generator) in generators.iter_mut().enumerate() {
            let expected = &line["gens"][index];
            let biomes = expected["biomes"].as_array().unwrap();
            if t == 0 {
                // O primeiro tick gravado já aconteceu quando o cenário começa: o tile tem o calor dele, o ar já foi lido e a lava e a
                // energia vêm do rastro.
                let (ambient, _) = cached_ambient(generator, biomes, &climates, sea);
                generator.state = State::new(ambient);
                generator.state.heat.heat = expected["heat_before"].as_f64().unwrap();
                generator.state.lava = expected["lava"].as_u64().unwrap();
            } else {
                // O bioma que o tile viu neste tick é o que o tick anterior gravou, depois das marcas: o cache já foi lido.
                let (ambient, face_ambient) = {
                    let previous = &body[(t - 1) as usize]["gens"][index];
                    cached_ambient(
                        generator,
                        previous["biomes"].as_array().unwrap(),
                        &climates,
                        sea,
                    )
                };
                let around = Surroundings {
                    lava_faces: 0,
                    in_nether: false,
                    ambient,
                    face_ambient,
                };
                let produced = step(&mut generator.state, &around);
                produced_total += produced.energy;
                let state = &generator.state;
                let key = format!("{scenario} t {t} gerador {index}");
                assert_eq!(
                    state.heat.heat,
                    expected["heat"].as_f64().unwrap(),
                    "{key}: calor"
                );
                assert_eq!(
                    state.heat.temperature(),
                    expected["temperature"].as_f64().unwrap(),
                    "{key}: temperatura"
                );
                assert_eq!(
                    state.energy,
                    expected["energy"].as_i64().unwrap() as i32,
                    "{key}: energia"
                );
                assert_eq!(
                    state.lava,
                    expected["lava"].as_u64().unwrap(),
                    "{key}: lava"
                );
                // Um tile recriado do NBT nasce com `producingEnergy`, `lastTransferLoss` e `lastEnvironmentLoss` zerados (não são salvos): o
                // rastro grava o tile novo depois da marca. Esses três só valem no tick seguinte; o port os refaz a cada tick.
                let renewed = marks.contains(&format!("reload {index}").as_str());
                if renewed {
                    assert_eq!(
                        expected["produced"].as_i64().unwrap(),
                        0,
                        "{key}: produção do tile novo"
                    );
                    assert_eq!(
                        expected["transfer_loss"].as_f64().unwrap(),
                        0.0,
                        "{key}: perda para a base do tile novo"
                    );
                    assert_eq!(
                        expected["environment_loss"].as_f64().unwrap(),
                        0.0,
                        "{key}: perda para o ambiente do tile novo"
                    );
                } else {
                    assert_eq!(
                        i64::from(produced.energy),
                        expected["produced"].as_i64().unwrap(),
                        "{key}: produção"
                    );
                    assert_eq!(
                        produced.transfer_loss,
                        expected["transfer_loss"].as_f64().unwrap(),
                        "{key}: perda para a base"
                    );
                    assert_eq!(
                        produced.environment_loss,
                        expected["environment_loss"].as_f64().unwrap(),
                        "{key}: perda para o ambiente"
                    );
                }
                assert_eq!(
                    state.tile.active(),
                    expected["active"].as_bool().unwrap(),
                    "{key}: ativo"
                );
            }
            // O buffer vazio depois de cada tick: o gerador produz no seguinte.
            generator.state.energy = 0;
            // As marcas mexem depois do tick do tile.
            for mark in &marks {
                let words: Vec<&str> = mark.split(' ').collect();
                match words.as_slice() {
                    ["biome", _, _] => {}
                    ["reload", which] if which.parse::<usize>().unwrap() == index => {
                        generator.cache = AmbientCache::default();
                    }
                    ["reload", _] => {}
                    _ => panic!("marca desconhecida: {mark}"),
                }
            }
            // Quem grava lê o ar do tile no fim do tick, com o mundo como ficou: um tile novo lê agora, um velho devolve o guardado.
            let (ambient, faces) = cached_ambient(generator, biomes, &climates, sea);
            let key = format!("{scenario} t {t} gerador {index}");
            assert_eq!(
                ambient,
                expected["ambient"].as_f64().unwrap(),
                "{key}: ar do bloco"
            );
            assert_eq!(
                faces.to_vec(),
                numbers(&expected["ambient_faces"]),
                "{key}: ar das faces"
            );
            // E o que o mundo daria agora, sem o cache.
            let direct = climates[biomes[0].as_str().unwrap()].ambient_at(generator.pos, sea);
            assert_eq!(
                direct,
                expected["ambient_direct"].as_f64().unwrap(),
                "{key}: ar direto"
            );
            if generator.pos[1] > sea + 17 {
                elevated += 1;
            }
        }
    }
    assert!(
        elevated > 0,
        "{scenario}: nenhum gerador acima de seaLevel + 17"
    );
    assert!(produced_total > 0, "{scenario}: nada foi produzido");
}

#[test]
fn heights_and_biomes_match_the_original() {
    replay("heat_ambient_height", 160);
}

#[test]
fn the_cached_air_changes_only_with_a_new_tile() {
    replay("heat_ambient_cache", 300);
}

#[test]
fn a_column_in_a_desert_matches_the_original() {
    replay("heat_ambient_game_desert", 200);
}

#[test]
fn a_column_in_a_frozen_ocean_matches_the_original() {
    replay("heat_ambient_game_frozen", 200);
}

/// O ar guardado de verdade fica velho: depois de trocar o bioma e antes do `reload`, o ar do tile difere do que o mundo daria.
#[test]
fn the_cache_is_stale_between_the_biome_change_and_the_reload() {
    let lines = read("heat_ambient_cache");
    let stale = lines[1..]
        .iter()
        .filter(|line| line["gens"][0]["ambient"] != line["gens"][0]["ambient_direct"])
        .count();
    // Do tick 61 (a marca) ao 120 (o último antes do reload), 60 ticks.
    assert_eq!(stale, 60);
}
