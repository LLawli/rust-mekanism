//! Repete, tick a tick, os rastros do Heat Generator original em `dev/referencia/rastros`: `heat_generator_lava_run` (a lava que
//! acaba, o calor guardado que segue gerando, o resto de lava menor que uma queima e o buffer cheio) e `heat_generator_fuel_run`
//! (o slot que converte item em lava, o balde de lava e o item que não cabe). Cada linha do rastro é um tick, gravado depois do
//! tick do tile e das marcas do cenário, como os rastros das máquinas. `transfer_loss` e `environment_loss` são o `lastTransferLoss` e o
//! `lastEnvironmentLoss` que a aba de calor mostra. `active` é o `currentActive` do tile e `block_active` o estado
//! `lit` do bloco, que o atraso de `blockDeactivationDelay` segura aceso depois que o gerador para.
use mekanismgenerators_rules::{
    ENERGY_CAPACITY, FuelItem, Produced, State, Surroundings, fuel_to_tank, step, trace_line,
};
use serde_json::Value;
use std::path::Path;

/// `Items.COAL` e `Items.LAVA_BUCKET` no `FuelValues` do jogo.
const COAL_BURN_TIME: i32 = 1_600;
const LAVA_BUCKET_BURN_TIME: i32 = 20_000;

/// O slot de combustível, como o rastro o grava: o caminho do item e a quantidade.
#[derive(Clone, Debug, PartialEq)]
struct Slot {
    item: String,
    count: u64,
}

impl Slot {
    fn empty() -> Self {
        Self {
            item: String::new(),
            count: 0,
        }
    }

    fn fuel(&self) -> Option<FuelItem> {
        match self.item.as_str() {
            "lava_bucket" => Some(FuelItem::LavaBucket),
            "coal" => Some(FuelItem::Solid {
                burn_time: COAL_BURN_TIME,
                lava_bucket_burn_time: LAVA_BUCKET_BURN_TIME,
            }),
            _ => None,
        }
    }
}

/// `FluidFuelInventorySlot.fillOrBurn`, antes da regra do tick, como o `onUpdateServer` do tile.
fn fill_or_burn(state: &mut State, slot: &mut Slot) {
    let Some(item) = slot.fuel() else {
        return;
    };
    let room = mekanismgenerators_rules::TANK_CAPACITY - state.lava;
    let Some(lava) = fuel_to_tank(item, room) else {
        return;
    };
    state.lava += lava;
    if item == FuelItem::LavaBucket {
        slot.item = "bucket".to_owned();
    } else {
        slot.count -= 1;
        if slot.count == 0 {
            *slot = Slot::empty();
        }
    }
}

fn apply(state: &mut State, slot: &mut Slot, action: &str) {
    let words: Vec<&str> = action.split(' ').collect();
    match words.as_slice() {
        ["lava", amount] => state.lava = amount.parse().unwrap(),
        ["energy", amount] => state.energy = amount.parse().unwrap(),
        // O bloco muda o mundo, e as faces com lava vêm do rastro.
        ["lava_block" | "air", ..] => {}
        ["fuel", item, count] => {
            *slot = Slot {
                item: (*item).to_owned(),
                count: count.parse().unwrap(),
            }
        }
        _ => panic!("marca desconhecida: {action}"),
    }
}

fn replay(scenario: &str, minimum_lines: usize) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= minimum_lines,
        "rastro curto demais: {}",
        lines.len()
    );
    // O capacitor chega ao primeiro tick gravado depois dos ticks de preparação do jogo, com o ar de cada face: o calor dele semeia o
    // estado.
    assert_eq!(
        lines[0]["epsilon"].as_f64().unwrap(),
        machinery_basis_core::heat::EPSILON,
        "o limite de troca de calor"
    );
    let mut state = State::new(lines[0]["ambient"].as_f64().unwrap());
    state.heat.heat = lines[0]["heat_before"].as_f64().unwrap();
    let mut slot = Slot::empty();
    let mut total_produced = 0;
    let mut lava_faces = 0;
    for expected in &lines {
        let t = expected["t"].as_u64().unwrap();
        let faces: Vec<f64> = expected["ambient_faces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|face| face.as_f64().unwrap())
            .collect();
        // As faces com lava que o tile viu neste tick. O rastro grava depois das marcas, que mexem nos blocos depois do tick do tile:
        // no tick de uma marca de bloco vale o que o tick anterior gravou.
        let moves_blocks = expected["marks"].as_array().unwrap().iter().any(|mark| {
            let mark = mark.as_str().unwrap();
            mark.starts_with("lava_block") || mark.starts_with("air")
        });
        if !moves_blocks {
            lava_faces = expected["lava_faces"].as_u64().unwrap() as u32;
        }
        let around = Surroundings {
            lava_faces,
            in_nether: false,
            ambient: expected["ambient"].as_f64().unwrap(),
            face_ambient: faces.try_into().unwrap(),
        };
        // O primeiro tick gravado já aconteceu quando o cenário começa: o estado dele é o semeado, e a regra roda a partir do seguinte.
        let produced: Produced = if t == 0 {
            // As perdas do tick que já aconteceu vêm do rastro, como o calor.
            Produced {
                transfer_loss: expected["transfer_loss"].as_f64().unwrap(),
                environment_loss: expected["environment_loss"].as_f64().unwrap(),
                ..Produced::default()
            }
        } else {
            fill_or_burn(&mut state, &mut slot);
            step(&mut state, &around)
        };
        total_produced += produced.energy;
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&mut state, &mut slot, action);
            }
        }
        let actual = serde_json::json!({
            "heat": state.heat.heat,
            "temperature": state.heat.temperature(),
            "energy": state.energy,
            "capacity": ENERGY_CAPACITY,
            "lava": state.lava,
            "produced": produced.energy,
            "transfer_loss": produced.transfer_loss,
            "environment_loss": produced.environment_loss,
            "active": state.tile.active(),
            "block_active": state.tile.block_active(),
            "fuel": [slot.item, slot.count],
        });
        // O `MEKTRACE` do jogo tem os mesmos campos do rastro, que a regra calcula.
        let traced: Value = serde_json::from_str(&trace_line(
            t as i64,
            &state,
            &around,
            &produced,
            (slot.item.as_str(), slot.count),
        ))
        .unwrap();
        for (key, value) in actual.as_object().unwrap() {
            assert_eq!(&traced[key], value, "{scenario} t {t}: MEKTRACE {key}");
        }
        assert_eq!(
            traced["ambient_faces"], expected["ambient_faces"],
            "{scenario} t {t}: MEKTRACE ambient_faces"
        );
        assert_eq!(
            traced["ambient"], expected["ambient"],
            "{scenario} t {t}: MEKTRACE ambient"
        );
        assert_eq!(
            traced["lava_faces"].as_u64().unwrap(),
            u64::from(around.lava_faces)
        );
        for (key, value) in actual.as_object().unwrap() {
            assert_eq!(
                &expected[key], value,
                "{scenario} t {t}: {key}, esperado {}, obtido {value}",
                expected[key]
            );
        }
    }
    assert!(total_produced > 0, "o cenário não gerou nada");
}

#[test]
fn lava_run_matches_the_original() {
    replay("heat_generator_lava_run", 300);
}

#[test]
fn lava_sides_match_the_original() {
    replay("heat_generator_lava_sides", 400);
}

#[test]
fn fuel_run_matches_the_original() {
    replay("heat_generator_fuel_run", 240);
}
