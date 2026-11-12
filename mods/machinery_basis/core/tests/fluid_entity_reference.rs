//! A entidade dentro de um fluido de mod contra o rastro do original (`dev/referencia/rastros/fluids_entity.jsonl`, `FluidsEntityTrace` no
//! mekanism-ref: coluna de 4 fontes de `brine`, 40 ticks, e o fluxo de nível 3). As regras são as de `fluid_entity`; o que o Horizon faz
//! com elas (parar o `travel`, a queda, o ar) é a injeção `fluid_entity`, e o teste em jogo repete as mesmas colunas.

use machinery_basis_core::fluid_entity::{self, MOTION_SCALE};
use serde_json::Value;
use std::path::Path;

fn read_trace(env: &str, entity: &str, start: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros/fluids_entity.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rows: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("linha do rastro"))
        .filter(|r: &Value| r["env"] == env && r["entity"] == entity && r["start"] == start)
        .collect();
    assert!(rows.len() >= 21, "rastro curto: {env} {entity} {start}: {}", rows.len());
    rows
}

fn f(row: &Value, key: &str) -> f64 {
    row[key].as_f64().unwrap_or_else(|| panic!("{key} em {row}"))
}

/// A coluna de 4 fontes: o jogador e o porco ficam parados, sem gravidade, e perdem 1 de ar por tick dentro do fluido.
#[test]
fn player_and_pig_stand_still_in_a_source_column() {
    for (entity, first_tick) in [("player", 1usize), ("pig", 2)] {
        let rows = read_trace("brine", entity, "inside");
        for row in &rows {
            assert_eq!(f(row, "y"), 3.0, "{entity} tick {}", row["tick"]);
            assert_eq!(f(row, "dy"), 0.0, "{entity} tick {}", row["tick"]);
        }
        // `holds_still` é a regra que o travel do Horizon usa: a célula dos pés é de mod e alcança a caixa.
        let height = fluid_entity::height(0, true);
        assert!(fluid_entity::holds_still(
            false,
            false,
            fluid_entity::reaches_box(3, height, 3.0),
            true,
            false
        ));
        // O ar: 300 antes de entrar, -1 por tick até o fim.
        let mut air = f(&rows[first_tick - 1], "airSupply") as i32;
        assert_eq!(air, 300);
        for row in &rows[first_tick..] {
            air -= 1;
            assert_eq!(f(row, "airSupply") as i32, air, "{entity} tick {}", row["tick"]);
        }
    }
}

/// O fluxo de nível 3 soma `motionScale` por tick à velocidade e a posição não anda (o jogador e o porco).
#[test]
fn current_adds_the_motion_scale_to_a_held_entity() {
    for (env, entity, axis, sign) in [
        ("flowing_flat", "player", "dx", 1.0),
        ("flowing_flat", "pig", "dz", -1.0),
        ("flowing_steps", "player", "dx", 1.0),
        ("flowing_steps", "pig", "dz", -1.0),
    ] {
        let rows = read_trace(env, entity, "flowing_3");
        let (x0, z0) = (f(&rows[0], "x"), f(&rows[0], "z"));
        let mut velocity = 0.0;
        for row in &rows[1..] {
            velocity += sign * MOTION_SCALE;
            assert!((f(row, axis) - velocity).abs() < 1e-9, "{env} {entity} tick {}", row["tick"]);
            assert_eq!(f(row, "x"), x0);
            assert_eq!(f(row, "z"), z0);
        }
    }
}

/// A queda do porco que cai na coluna anda pelo `fallDistanceModifier`: metade a cada tick dentro do fluido.
#[test]
fn fall_distance_halves_every_tick_in_the_fluid() {
    let rows = read_trace("brine", "pig", "above");
    let mut previous = f(&rows[38], "fallDistance");
    for row in &rows[39..] {
        let expected = fluid_entity::fall_distance_in_fluid(previous);
        assert!((f(row, "fallDistance") - expected).abs() < 1e-12, "tick {}", row["tick"]);
        previous = f(row, "fallDistance");
    }
}

/// O item solto na coluna sobe pelo `setFluidMovement(0,99)` e o arrasto do ar, tick a tick, igual ao original.
#[test]
fn item_rises_in_the_column_like_the_original() {
    for start in ["inside", "above"] {
        let rows = read_trace("brine", "item", start);
        // O item que cai atravessa o fluido (a subida de 5.0E-4 só freia a queda) e pousa no fundo: a subida começa no primeiro tick com `dy > 0`.
        let first = rows.iter().position(|r| f(r, "dy") > 0.0).expect("o item sobe");
        // A coluna de 4 fontes: o topo do fluido fica 1 acima do centro do último bloco de fonte (`fluidTypeHeight` + `y`).
        let top = f(&rows[first], "y") + f(&rows[first], "fluidTypeHeight");
        let mut y = f(&rows[first - 1], "y");
        let mut vy = f(&rows[first - 1], "dy");
        for row in &rows[first..] {
            let height = (top - y) as f32;
            let [_, ny, _] = fluid_entity::item_movement(height, [0.0, vy, 0.0]).expect("o item flutua");
            y += ny;
            // `move` e depois o arrasto do ar (`getAirDrag`, `0.98F`).
            vy = ny * f64::from(0.98_f32);
            assert!((f(row, "y") - y).abs() < 1e-5, "{start} tick {}: {} contra {y}", row["tick"], f(row, "y"));
            assert!((f(row, "dy") - vy).abs() < 1e-9, "{start} tick {}", row["tick"]);
        }
    }
}
