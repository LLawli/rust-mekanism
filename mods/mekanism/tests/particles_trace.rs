//! As partículas das quatro máquinas acesas (`Machine.java:23`, `BlockTile.animateTick`): o rastro `particulas_rastro.jsonl` do cliente oficial
//! (`CenaParticulas`, etapa `rastro`) chama o `animateTick` do bloco direto, com uma `RandomSource.create(12345)` por máquina, direção, bloco
//! ativo ou não e `machineEffects` ligado ou não, seis vezes seguidas, e grava cada partícula que o `ClientLevel.addParticle` entregou ao motor
//! (classe e posição). Aqui a mesma regra, com o `java.util.Random` da mesma semente, tem de dar as mesmas partículas nas mesmas posições.
use machinery_basis_core::active_fx::{
    Facing, Particle, ParticleLevel, Spawn, animate_tick, bit_random_double, center_of,
};
use machinery_basis_core::java_random::JavaRandom;
use mekanism_rules::MACHINE_FX;
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
    .collect()
}

fn facing_of(name: &str) -> Facing {
    Facing::from_property(name)
}

/// A classe do `Particle` que o `ParticleEngine` cria para cada `ParticleOptions`.
fn class_of(particle: Particle) -> &'static str {
    match particle {
        Particle::Smoke => "SmokeParticle",
        Particle::Flame => "FlameParticle",
        Particle::Dust { .. } => "DustParticle",
    }
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

#[test]
fn the_four_machines_drop_the_particles_of_the_original_at_the_same_positions() {
    let mut replays: HashMap<String, Replay> = HashMap::new();
    let (mut compared, mut with_particles) = (0, 0);
    for row in rows() {
        let machine = row["maquina"].as_str().unwrap();
        if machine == "heat" {
            continue;
        }
        let key = format!(
            "{machine} {} {} {}",
            row["facing"], row["ativo"], row["efeitos"]
        );
        let seed = row["semente"].as_i64().unwrap();
        let replay = replays.entry(key).or_insert_with(|| Replay {
            random: JavaRandom::new(seed),
            spawns: Vec::new(),
        });
        replay.spawns.clear();
        let center = center_of([
            row["x"].as_i64().unwrap() as i32,
            row["y"].as_i64().unwrap() as i32,
            row["z"].as_i64().unwrap() as i32,
        ]);
        animate_tick(
            &MACHINE_FX,
            facing_of(row["facing"].as_str().unwrap()),
            center,
            row["ativo"].as_bool().unwrap(),
            row["efeitos"].as_bool().unwrap(),
            replay,
        );
        let expected = row["particulas"].as_array().unwrap();
        assert_eq!(replay.spawns.len(), expected.len(), "{row}");
        for (spawn, java) in replay.spawns.iter().zip(expected) {
            assert_eq!(
                class_of(spawn.particle),
                java["classe"].as_str().unwrap(),
                "{row}"
            );
            for (axis, name) in ["x", "y", "z"].into_iter().enumerate() {
                assert_eq!(spawn.pos[axis], number(&java[name]), "{name} de {row}");
            }
            with_particles += 1;
        }
        compared += 1;
    }
    // 4 máquinas, 4 direções, 2 estados de atividade, 2 de `machineEffects`, 6 chamadas.
    assert_eq!(compared, 4 * 4 * 2 * 2 * 6);
    // Só o bloco ativo com os efeitos ligados solta (2 partículas por chamada).
    assert_eq!(with_particles, 4 * 4 * 6 * 2);
}

#[test]
fn a_block_that_is_off_or_without_effects_does_not_draw_from_the_generator() {
    // O Java não sorteia nada nesses casos: a primeira chamada com tudo ligado, depois de uma sem efeitos, dá as mesmas partículas da primeira vez.
    let mut fresh = Replay {
        random: JavaRandom::new(7),
        spawns: Vec::new(),
    };
    animate_tick(&MACHINE_FX, Facing::South, [0.5; 3], true, true, &mut fresh);
    let mut skipped = Replay {
        random: JavaRandom::new(7),
        spawns: Vec::new(),
    };
    animate_tick(
        &MACHINE_FX,
        Facing::South,
        [0.5; 3],
        false,
        true,
        &mut skipped,
    );
    animate_tick(
        &MACHINE_FX,
        Facing::South,
        [0.5; 3],
        true,
        false,
        &mut skipped,
    );
    assert!(skipped.spawns.is_empty());
    animate_tick(
        &MACHINE_FX,
        Facing::South,
        [0.5; 3],
        true,
        true,
        &mut skipped,
    );
    assert_eq!(fresh.spawns, skipped.spawns);
}
