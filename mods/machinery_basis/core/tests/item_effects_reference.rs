//! O item solto parado na célula de um bloco com colisão contra o rastro do original (`dev/referencia/rastros/fluido_item_parado.jsonl`, `FluidoItemParadoTraceTest.java`: o Heat
//! Generator com lava, com água e sem fluido, o cabo, o cacto, tick a tick, com o `noPhysics`, o fogo e a vida de cada tick). O módulo `item-effects` da `machinery_basis`
//! (`item_effects.rs`) é compilado como está: a injeção `item_effects` só lê o mundo e aplica o que ele decide, e a `item_no_physics` guarda o `noPhysics` do `tick_item`.
//!
//! O que o original faz (decisão 100):
//! - `Entity.isAffectedByBlocks` é `!isRemoved() && !noPhysics` (`Entity.java:1030`), e `applyEffectsFromBlocks` (de onde saem a lava que acende e fere, a água que apaga e o cacto) só
//!   roda com ele. O item preso numa colisão (o `moveTowardsClosestSpace` o empurra para fora a 0,1 a 0,3 por tick, ao acaso: a posição não se repete entre execuções) não pega fogo na
//!   lava, não apaga na água e o cacto não o fere; no tick em que sai, tudo isso vale de novo;
//! - `isInLava` é `!firstTick && ...` (`Entity.java:1782`): no primeiro tick o `baseTick` fere 1 do fogo mesmo dentro da lava;
//! - o `baseTick` vem antes dos efeitos dos blocos: o item em chamas dentro da água leva o ferimento de 1 e só então a água o apaga.

#[allow(dead_code)]
mod effects {
    include!("../../item_effects.rs");
}

use effects::{Input, tick};
use serde_json::Value;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/fluido_item_parado.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha do rastro"))
        .collect()
}

/// As linhas de cada caso, na ordem do rastro.
fn cases() -> Vec<(String, Vec<Value>)> {
    let mut out: Vec<(String, Vec<Value>)> = Vec::new();
    for row in rows() {
        let name = row["case"].as_str().unwrap().to_owned();
        match out.last_mut() {
            Some((last, list)) if *last == name => list.push(row),
            _ => out.push((name, vec![row])),
        }
    }
    out
}

fn case(name: &str) -> Vec<Value> {
    cases()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("sem o caso {name}"))
        .1
}

fn flag(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} em {row}"))
}

fn int(row: &Value, key: &str) -> i64 {
    row[key]
        .as_i64()
        .unwrap_or_else(|| panic!("{key} em {row}"))
}

/// O fogo inicial do caso: o `burning` do nome põe 100.
fn initial_fire(name: &str) -> i32 {
    if name.contains("burning") { 100 } else { 0 }
}

/// A entrada de um tick do rastro: o `noPhysics`, a lava e a água que o original deu a esse tick (o cacto fica de fora: nenhum tick sem `noPhysics` o toca).
fn input(row: &Value, fire_ticks: i32) -> Input {
    Input {
        fire_ticks,
        no_physics: flag(row, "np"),
        first_tick: int(row, "t") == 1,
        in_water: flag(row, "water"),
        in_lava: flag(row, "lava"),
        ..Input::default()
    }
}

/// O rastro todo, tick a tick: o fogo, a vida e se o item foi removido, com o `noPhysics`, a lava e a água que o original deu a cada tick.
#[test]
fn the_rule_reproduces_every_row_of_the_trace() {
    let mut checked = 0;
    for (name, list) in cases() {
        let (mut fire, mut health) = (initial_fire(&name), 5);
        for row in &list {
            let out = tick(input(row, fire));
            fire = out.fire_ticks;
            for hurt in &out.hurts {
                health -= hurt;
            }
            assert_eq!(
                flag(row, "removed"),
                health <= 0,
                "{name} t={}: removido",
                row["t"]
            );
            assert_eq!(
                int(row, "health"),
                i64::from(health),
                "{name} t={}: vida",
                row["t"]
            );
            if health <= 0 {
                break;
            }
            // O `fire` do original é o `remainingFireTicks`: 0 e negativo (o `fireImmuneTicks`) são "sem fogo".
            assert_eq!(
                int(row, "fire").max(0),
                i64::from(fire),
                "{name} t={}: fogo",
                row["t"]
            );
            checked += 1;
        }
    }
    assert!(checked >= 130, "só {checked} linhas conferidas");
}

/// A pergunta da frente: o item parado na célula com lava do gerador não pega fogo enquanto está com `noPhysics`, e pega no tick em que sai com a lava ainda nele.
#[test]
fn a_stuck_item_in_lava_stays_alive_and_cold_until_it_leaves() {
    let mut stuck_ticks = 0;
    let mut ignited_after_leaving = 0;
    for (name, list) in cases().into_iter().filter(|(name, _)| {
        name.starts_with("heat_generator_lava_stone")
            || name.starts_with("heat_generator_lava_coal")
    }) {
        let mut left = false;
        for row in &list {
            let np = flag(row, "np");
            if !left && np {
                assert_eq!(
                    row["fire"], 0,
                    "{name} t={}: o item com noPhysics pegou fogo",
                    row["t"]
                );
                assert_eq!(
                    row["health"], 5,
                    "{name} t={}: o item com noPhysics se feriu",
                    row["t"]
                );
                assert!(!flag(row, "removed"));
                stuck_ticks += 1;
            }
            if !np {
                left = true;
            }
            if left && flag(row, "lava") && row["fire"] == 300 {
                ignited_after_leaving += 1;
                break;
            }
        }
    }
    assert!(stuck_ticks >= 20, "{stuck_ticks}");
    assert!(
        ignited_after_leaving >= 1,
        "nenhum item pegou fogo ao sair da colisão"
    );
}

/// A água não apaga o item com `noPhysics` (`WaterFluid.entityInside` é um efeito de bloco): o fogo só desce 1 por tick, como no ar; ao sair, a água o apaga.
#[test]
fn water_does_not_put_out_a_stuck_item() {
    let list = case("heat_generator_water_burning");
    let stuck: Vec<&Value> = list.iter().take_while(|row| flag(row, "np")).collect();
    assert!(stuck.len() >= 3, "{}", stuck.len());
    for (index, row) in stuck.iter().enumerate() {
        assert_eq!(int(row, "fire"), 99 - index as i64, "t={}", row["t"]);
    }
    assert!(
        list.iter()
            .skip(stuck.len())
            .any(|row| flag(row, "water") && row["fire"] == 0),
        "ao sair, a água apaga"
    );
}

/// O controle do cabo: sem colisão no vão o item não tem `noPhysics`; a água o apaga já no primeiro tick, e só depois de o `baseTick` o ferir em 1.
#[test]
fn water_puts_out_a_free_item_after_the_base_tick_hurt_it() {
    let list = case("cable_water_burning");
    assert!(
        list.iter()
            .all(|row| !flag(row, "np") && row["fire"] == 0 && row["health"] == 4)
    );
    let out = tick(Input {
        fire_ticks: 100,
        first_tick: true,
        in_water: true,
        ..Input::default()
    });
    assert_eq!((out.fire_ticks, out.hurts), (0, vec![1]));
}

/// O cacto não fere o item com `noPhysics`: ele fica vivo e com a vida cheia todos os ticks em que está preso.
#[test]
fn a_cactus_does_not_hurt_a_stuck_item() {
    let list = case("cactus_stone");
    let stuck = list.iter().take_while(|row| flag(row, "np")).count();
    assert!(stuck >= 3, "{stuck}");
    assert!(
        list.iter()
            .take(stuck)
            .all(|row| !flag(row, "removed") && row["health"] == 5)
    );
    let touching = Input {
        cactus: true,
        ..Input::default()
    };
    assert_eq!(tick(touching).hurts, vec![1]);
    assert!(
        tick(Input {
            no_physics: true,
            ..touching
        })
        .hurts
        .is_empty()
    );
}

/// O controle do cabo com lava: sem `noPhysics` a lava acende e fere 4 já no primeiro tick (a pedra sobra com 1 de vida e morre no segundo); o item em chamas morre no primeiro, porque
/// `isInLava` é falso no primeiro tick e o `baseTick` fere 1 antes.
#[test]
fn the_cable_cell_has_no_no_physics_and_lava_kills_at_the_second_tick() {
    let list = case("cable_lava_stone");
    assert!(list.iter().all(|row| !flag(row, "np") && flag(row, "lava")));
    assert_eq!((int(&list[0], "fire"), int(&list[0], "health")), (300, 1));
    assert!(!flag(&list[0], "removed") && flag(&list[1], "removed"));
    let burning = case("cable_lava_burning");
    assert!(flag(&burning[0], "removed"));
    assert_eq!(int(&burning[0], "health"), 0);
}

/// O `baseTick` e os efeitos dos blocos como o original, valor a valor.
#[test]
fn tick_follows_the_original() {
    let burning = Input {
        fire_ticks: 100,
        ..Input::default()
    };
    // `remainingFireTicks % 20 == 0 && !isInLava`: fere 1 e o fogo desce 1.
    let out = tick(burning);
    assert_eq!(
        (out.fire_ticks, out.hurts.clone(), out.lava_hurt),
        (99, vec![1], false)
    );
    assert!(
        tick(Input {
            fire_ticks: 99,
            ..burning
        })
        .hurts
        .is_empty()
    );
    assert!(
        tick(Input {
            fire_ticks: 90,
            ..burning
        })
        .hurts
        .is_empty()
    );
    assert_eq!(
        tick(Input {
            fire_ticks: 40,
            ..burning
        })
        .hurts,
        vec![1]
    );
    // Na lava (e não no primeiro tick) não fere por estar em chamas; a lava acende e fere 4. No primeiro tick fere 1 do fogo e 4 da lava.
    let lava = Input {
        in_lava: true,
        ..burning
    };
    assert_eq!(
        (
            tick(lava).fire_ticks,
            tick(lava).hurts,
            tick(lava).lava_hurt
        ),
        (300, vec![4], true)
    );
    assert_eq!(
        tick(Input {
            first_tick: true,
            ..lava
        })
        .hurts,
        vec![1, 4]
    );
    // O item que resiste a `on_fire` não leva o 1; o que resiste a `lava` leva o fogo e não o 4; o imune a fogo apaga e não acende.
    assert!(
        tick(Input {
            resists_on_fire: true,
            ..burning
        })
        .hurts
        .is_empty()
    );
    assert_eq!(
        (
            tick(Input {
                resists_lava: true,
                ..lava
            })
            .fire_ticks,
            tick(Input {
                resists_lava: true,
                ..lava
            })
            .hurts
        ),
        (300, vec![])
    );
    let immune = tick(Input {
        fire_immune: true,
        ..lava
    });
    assert_eq!((immune.fire_ticks, immune.hurts), (0, vec![]));
    // O fogo de bloco: acende a 160 e fere pelo dano dele; o cacto fere 1; a resistência dele não vale para os outros.
    let fire = Input {
        fire_damage: Some(3),
        ..Input::default()
    };
    let out = tick(fire);
    assert_eq!((out.fire_ticks, out.hurts), (160, vec![3]));
    assert_eq!(
        tick(Input {
            resists_in_fire: true,
            ..fire
        })
        .hurts,
        Vec::<i32>::new()
    );
    assert_eq!(
        tick(Input {
            cactus: true,
            resists_cactus: true,
            ..Input::default()
        })
        .hurts,
        Vec::<i32>::new()
    );
    // Sem fogo e sem nada, nada acontece; o fogo negativo (o `fireImmuneTicks`) segue como está.
    assert_eq!(
        tick(Input::default()),
        effects::Outcome {
            fire_ticks: 0,
            hurts: vec![],
            lava_hurt: false
        }
    );
    assert_eq!(
        tick(Input {
            fire_ticks: -1,
            ..Input::default()
        })
        .fire_ticks,
        -1
    );
    // Com `noPhysics` só o `baseTick` vale: a lava, a água, o fogo e o cacto ficam de fora.
    let stuck = Input {
        no_physics: true,
        in_lava: true,
        in_water: true,
        fire_damage: Some(1),
        cactus: true,
        ..burning
    };
    let out = tick(stuck);
    assert_eq!(
        (out.fire_ticks, out.hurts, out.lava_hurt),
        (99, vec![], false)
    );
}

/// A linha do Java de cada regra, conferida onde o original está clonado.
#[test]
fn the_java_lines_are_the_ones_the_rule_follows() {
    let java =
        "/mnt/dados/caches/rust-mekanism-ref/mc-sources/mc/net/minecraft/world/entity/Entity.java";
    if let Ok(text) = std::fs::read_to_string(java) {
        assert!(text.contains("return !this.isRemoved() && !this.noPhysics;"));
        assert!(text.contains("if (this.remainingFireTicks % 20 == 0 && !this.isInLava()) {"));
        assert!(text.contains("return !this.firstTick && this.fluidInteraction.isInFluid("));
    }
}

#[test]
fn the_no_physics_flag_is_kept_for_the_next_call() {
    effects::set_no_physics(true);
    assert!(effects::no_physics());
    effects::set_no_physics(false);
    assert!(!effects::no_physics());
}
