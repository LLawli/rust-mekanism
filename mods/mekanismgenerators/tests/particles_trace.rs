//! As partículas do Heat Generator aceso (`GeneratorsBlockTypes.java:69`, `BlockTile.animateTick`): o mesmo rastro do cliente oficial que as
//! máquinas (`particulas_rastro.jsonl`, máquina `heat`). A fumaça tem a posição pedida, byte a byte; a chama (`FlameParticle`) soma à posição pedida um
//! deslocamento sorteado de até 0,05 em cada eixo no construtor (`(nextFloat - nextFloat) * 0.05`), com o gerador do próprio jogo, que não tem semente a
//! reproduzir: a posição pedida tem de ficar a menos de 0,05 da que o rastro viu.
use machinery_basis_core::active_fx::{
    Facing, Particle, ParticleLevel, Spawn, animate_tick, bit_random_double, center_of,
};
use machinery_basis_core::java_random::JavaRandom;
use mekanismgenerators_rules::HEAT_GENERATOR_FX;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

struct Replay {
    random: JavaRandom,
    spawns: Vec<Spawn>,
}

impl ParticleLevel for Replay {
    fn next_double(&mut self) -> f64 {
        let high = self.random.next(26);
        bit_random_double(high, self.random.next(27))
    }

    fn add_particle(&mut self, spawn: Spawn) {
        self.spawns.push(spawn);
    }
}

fn rows() -> Vec<Value> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/rastros/particulas_rastro.jsonl"),
    )
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .filter(|row: &Value| row["maquina"] == "heat")
    .collect()
}

#[test]
fn the_heat_generator_drops_smoke_and_flame_at_the_positions_of_the_original() {
    let mut replays: HashMap<String, Replay> = HashMap::new();
    let (mut compared, mut smoke, mut flame) = (0, 0, 0);
    for row in rows() {
        let key = format!("{} {} {}", row["facing"], row["ativo"], row["efeitos"]);
        let replay = replays.entry(key).or_insert_with(|| Replay {
            random: JavaRandom::new(row["semente"].as_i64().unwrap()),
            spawns: Vec::new(),
        });
        replay.spawns.clear();
        let center = center_of(["x", "y", "z"].map(|axis| row[axis].as_i64().unwrap() as i32));
        animate_tick(
            &HEAT_GENERATOR_FX,
            Facing::from_property(row["facing"].as_str().unwrap()),
            center,
            row["ativo"].as_bool().unwrap(),
            row["efeitos"].as_bool().unwrap(),
            replay,
        );
        let expected = row["particulas"].as_array().unwrap();
        assert_eq!(replay.spawns.len(), expected.len(), "{row}");
        for (spawn, java) in replay.spawns.iter().zip(expected) {
            let class = java["classe"].as_str().unwrap();
            let position = ["x", "y", "z"].map(|axis| java[axis].as_f64().unwrap());
            match spawn.particle {
                Particle::Smoke => {
                    assert_eq!(class, "SmokeParticle", "{row}");
                    assert_eq!(spawn.pos, position, "{row}");
                    smoke += 1;
                }
                Particle::Flame => {
                    assert_eq!(class, "FlameParticle", "{row}");
                    for (axis, (ours, java)) in spawn.pos.iter().zip(position).enumerate() {
                        assert!((ours - java).abs() < 0.05, "{axis} de {row}");
                    }
                    flame += 1;
                }
                Particle::Dust { .. } => panic!("o gerador não solta pó de redstone"),
            }
        }
        compared += 1;
    }
    assert_eq!(compared, 4 * 2 * 2 * 6);
    assert_eq!((smoke, flame), (4 * 6, 4 * 6));
}
