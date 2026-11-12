//! O item solto parado na célula de um bloco com colisão contra o rastro do original (`dev/referencia/rastros/fluido_item_parado.jsonl`, `FluidoItemParadoTraceTest.java`: o Heat
//! Generator com lava, com água e sem fluido, o cabo, o cacto, tick a tick, com o `noPhysics` de cada tick). O módulo `item-effects` da `machinery_basis` (`item_effects.rs`) é compilado
//! como está: as injeções `item_no_physics` e `item_effects` só ligam o `noPhysics` do `tick_item` ao `environment_tick` do Horizon, e a regra é a deste arquivo.
//!
//! O que o original faz (`Entity.java:1030`): `isAffectedByBlocks` é `!isRemoved() && !noPhysics`, e `applyEffectsFromBlocks` (de onde saem a lava que acende e fere, a água que
//! apaga e o cacto) só roda com ele. O item preso numa colisão (o `moveTowardsClosestSpace` o empurra para fora a 0,1 a 0,3 por tick, ao acaso: a posição não se repete entre
//! execuções) não pega fogo na lava, não apaga na água e o cacto não o fere; no tick em que sai, tudo isso vale de novo.

#[allow(dead_code)]
mod effects {
    include!("../../item_effects.rs");
}

use serde_json::Value;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros/fluido_item_parado.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines().map(|line| serde_json::from_str(line).expect("linha do rastro")).collect()
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

fn flag(row: &Value, key: &str) -> bool {
    row[key].as_bool().unwrap_or_else(|| panic!("{key} em {row}"))
}

/// O fogo inicial do caso: o `burning` do nome põe 100.
fn initial_fire(name: &str) -> i32 {
    if name.contains("burning") { 100 } else { 0 }
}

/// O `Entity.baseTick` e os efeitos dos blocos do original (que é o `ItemEntity.environment_tick` do Horizon, `crates/horizon-client/src/entities.rs`, menos o `firstTick`) para um item sem resistência a fogo (`crates/horizon-client/src/entities.rs`), com a lava e a água que o original dá pelo `isInLava` e
/// `isInWater` do tick (o cacto fica de fora: sem o `noPhysics` não se sabe se a caixa ainda o toca). É o que roda quando o item NÃO está com `noPhysics`.
fn horizon(fire: &mut i32, health: &mut i32, in_lava: bool, in_water: bool, first_tick: bool) {
    if in_water {
        *fire = 0;
    }
    if *fire > 0 {
        if *fire % 20 == 0 && !(in_lava && !first_tick) {
            *health -= 1;
        }
        *fire -= 1;
    }
    if in_lava {
        *fire = (*fire).max(300);
        *health -= 4;
    }
}

/// Um tick do item com a regra do port: com `noPhysics` só o `baseTick` do fogo (`effects::burn`), sem ele o `environment_tick` do Horizon.
fn step(fire: &mut i32, health: &mut i32, np: bool, in_lava: bool, in_water: bool, first_tick: bool) {
    if effects::affected_by_blocks(np) {
        horizon(fire, health, in_lava, in_water, first_tick);
    } else {
        let burn = effects::burn(*fire, in_lava, first_tick, false, false);
        *fire = burn.fire_ticks;
        if burn.hurt {
            *health -= 1;
        }
    }
}

/// O rastro todo, tick a tick: o fogo e se o item foi removido, com o `noPhysics`, a lava e a água que o original deu a cada tick.
#[test]
fn the_rule_reproduces_every_row_of_the_trace() {
    let mut checked = 0;
    for (name, list) in cases() {
        let (mut fire, mut health) = (initial_fire(&name), 5);
        for row in &list {
            let np = flag(row, "np");
            step(&mut fire, &mut health, np, flag(row, "lava"), flag(row, "water"), row["t"] == 1);
            if health <= 0 {
                assert!(flag(row, "removed"), "{name} t={}: o item devia ter morrido", row["t"]);
                break;
            }
            assert!(!flag(row, "removed"), "{name} t={}: o item não devia ter morrido", row["t"]);
            // O `fire` do original é o `remainingFireTicks`: 0 e negativo (o `fireImmuneTicks`) são "sem fogo".
            assert_eq!(row["fire"].as_i64().unwrap().max(0), i64::from(fire), "{name} t={}", row["t"]);
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
    for (name, list) in cases().into_iter().filter(|(name, _)| name.starts_with("heat_generator_lava_stone") || name.starts_with("heat_generator_lava_coal")) {
        let mut left = false;
        for row in &list {
            let np = flag(row, "np");
            if !left && np {
                assert_eq!(row["fire"], 0, "{name} t={}: o item com noPhysics pegou fogo", row["t"]);
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
    assert!(ignited_after_leaving >= 1, "nenhum item pegou fogo ao sair da colisão");
}

/// A água não apaga o item com `noPhysics` (`WaterFluid.entityInside` é um efeito de bloco): o fogo só desce 1 por tick, como no ar; ao sair, a água o apaga.
#[test]
fn water_does_not_put_out_a_stuck_item() {
    let (_, list) = cases().into_iter().find(|(name, _)| name == "heat_generator_water_burning").unwrap();
    let stuck: Vec<&Value> = list.iter().take_while(|row| flag(row, "np")).collect();
    assert!(stuck.len() >= 3, "{}", stuck.len());
    for (index, row) in stuck.iter().enumerate() {
        assert_eq!(row["fire"].as_i64().unwrap(), 99 - index as i64, "t={}", row["t"]);
    }
    assert!(list.iter().skip(stuck.len()).any(|row| flag(row, "water") && row["fire"] == 0), "ao sair, a água apaga");
    // O controle: no cabo (sem colisão no vão) a água apaga no primeiro tick.
    let (_, cable) = cases().into_iter().find(|(name, _)| name == "cable_water_burning").unwrap();
    assert!(cable.iter().all(|row| !flag(row, "np") && row["fire"] == 0));
}

/// O cacto não fere o item com `noPhysics`: ele fica vivo todos os ticks em que está preso (cinco golpes de 1 o matariam).
#[test]
fn a_cactus_does_not_hurt_a_stuck_item() {
    let (_, list) = cases().into_iter().find(|(name, _)| name == "cactus_stone").unwrap();
    let stuck = list.iter().take_while(|row| flag(row, "np")).count();
    assert!(stuck >= 5, "{stuck}");
    assert!(list.iter().take(stuck).all(|row| !flag(row, "removed")));
}

/// O controle do cabo: sem colisão no vão o item não tem `noPhysics`, a lava o acende no primeiro tick e ele morre no segundo (as 44 conferências da decisão 88).
#[test]
fn the_cable_cell_has_no_no_physics_and_lava_kills_at_the_second_tick() {
    let (_, list) = cases().into_iter().find(|(name, _)| name == "cable_lava_stone").unwrap();
    assert!(list.iter().all(|row| !flag(row, "np") && flag(row, "lava")));
    assert_eq!(list[0]["fire"], 300);
    assert!(!flag(&list[0], "removed") && flag(&list[1], "removed"));
}

/// `Entity.baseTick` (`Entity.java:561-575`): com fogo, fere 1 quando `remainingFireTicks % 20 == 0 && !isInLava` e o fogo desce 1. A linha do Java é conferida onde o original está clonado.
#[test]
fn burn_follows_the_base_tick() {
    use effects::{Burn, burn};
    assert_eq!(burn(100, false, false, false, false), Burn { fire_ticks: 99, hurt: true });
    assert_eq!(burn(100, true, false, false, false), Burn { fire_ticks: 99, hurt: false });
    // `isInLava` é falso no primeiro tick (`!this.firstTick && ...`, `Entity.java:1782`): na lava, o item em chamas fere 1 nele.
    assert_eq!(burn(100, true, true, false, false), Burn { fire_ticks: 99, hurt: true });
    assert_eq!(burn(99, false, false, false, false), Burn { fire_ticks: 98, hurt: false });
    assert_eq!(burn(90, false, false, false, false), Burn { fire_ticks: 89, hurt: false });
    assert_eq!(burn(40, false, false, false, false), Burn { fire_ticks: 39, hurt: true });
    assert_eq!(burn(80, false, false, false, true), Burn { fire_ticks: 79, hurt: false });
    assert_eq!(burn(20, false, false, true, false), Burn { fire_ticks: 0, hurt: false });
    assert_eq!(burn(0, false, false, false, false), Burn { fire_ticks: 0, hurt: false });
    assert_eq!(burn(-1, false, false, false, false), Burn { fire_ticks: -1, hurt: false });
    let java = "/mnt/dados/caches/rust-mekanism-ref/mc-sources/mc/net/minecraft/world/entity/Entity.java";
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
