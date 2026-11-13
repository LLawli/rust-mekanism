//! O rastro do jogo (não o do original): `dev/referencia/rastros/jogo_calor.jsonl`, gravado por `h2_condutor_jogo.py serie` com a cópia de teste do port no Horizon. A linha de cinco
//! condutores básicos com o Heat Generator em cima do último, o calor do gerador posto por `data merge` e `tick step 1`: cada linha é o calor dos cinco condutores no
//! `ServerTickEvent.Post` do passo. O mundo simulado é o de `heat_owner_world`, que os rastros do original conferem (`heat_owner_trace.rs`); repetir o jogo nele, com os mesmos bits,
//! mostra que a cola (o ar do bioma, o capacitor do gerador lido e gravado no NBT dele, a ordem do tick) não muda nada do que a lib faz.
mod heat_owner_world;

use heat_owner_world::{Ambient, World};
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::thermodynamic_conductor::ConductorTier;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_game_repeats_the_line_with_the_generator_on_top_bit_for_bit() {
    let rows = read("jogo_calor.jsonl");
    let setup = &rows[0];
    let origin = setup["origin"].as_array().unwrap();
    let origin = BlockPos::new(
        origin[0].as_i64().unwrap() as i32,
        origin[1].as_i64().unwrap() as i32,
        origin[2].as_i64().unwrap() as i32,
    );
    // O ar que o jogo leu, por posição: o do bioma com o ajuste de altura (o rastro do original tem 300 K em toda parte).
    let reads: HashMap<[i32; 3], f64> = setup["ambient_reads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|read| {
            let read = read.as_array().unwrap();
            (
                [
                    read[0].as_i64().unwrap() as i32,
                    read[1].as_i64().unwrap() as i32,
                    read[2].as_i64().unwrap() as i32,
                ],
                read[3].as_f64().unwrap(),
            )
        })
        .collect();
    assert!(
        reads.len() >= 20,
        "o jogo leu o ar de {} posições",
        reads.len()
    );
    let h0 = setup["gen_heat0"].as_f64().unwrap();
    let mut world = World::with_ambient(
        origin,
        Ambient::from_positions(origin, reads),
        (1..40).map(|n| n * 7919).collect(),
        (1..40).map(|n| n * 31).collect(),
    );
    for x in 0..5 {
        world.place_conductor(x, ConductorTier::Basic);
    }
    world.place_generator(4);
    for _ in 0..4 {
        world.tick();
    }
    // O jogo parou com os condutores onde estavam (`rest`) e pôs o calor do gerador de uma vez (`data merge`).
    for (x, heat) in setup["rest"].as_array().unwrap().iter().enumerate() {
        world.set_conductor_heat(x as i32, heat.as_f64().unwrap());
    }
    world.set_generator_heat(h0);
    let mut steps = 0;
    for row in &rows[1..] {
        world.tick();
        for (x, want) in row["cond"].as_array().unwrap().iter().enumerate() {
            let got = world
                .owner()
                .state_at(world.pos(x as i32, 0))
                .unwrap()
                .heat
                .unwrap();
            let want = want.as_f64().unwrap();
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "passo {} condutor {x}: {got:?} contra {want:?}",
                row["i"]
            );
        }
        steps += 1;
    }
    assert!(steps >= 30, "só {steps} passos conferidos");
}
