//! As quatro máquinas da Fase 5 (`crusher`, `enrichment_chamber`, `energized_smelter` e `metallurgic_infuser`) seguem com o gabarito `minecraft:furnace`, e a injeção `active_light`
//! (`native/active_light.rs`) lhes aplica a reação ao pistão e a oclusão do original (a tabela `FURNACE` de `native/block_rows_table.rs`), que o gabarito de cubo cheio não dá ao Metallurgic
//! Infuser (`BlockFactoryMachineModel`: `isSolidRender` falso, amortecimento 0 e a luz do céu passa). O teste roda o helper de verdade sobre a linha de jogo do gabarito, estado a estado,
//! e compara com o original em jogo (`rastros/blocos.jsonl`, `testes/BlocosTraceTest.java`).
mod common;

use common::craft::blocos;
use serde_json::Value;

/// O `J` de `mod_content.rs` do Horizon (JSON com as chaves na ordem do arquivo), só com o que a injeção `active_light` toca.
#[derive(Clone, Debug, PartialEq)]
enum J {
    Bool(bool),
    Num(f64),
    Str(String),
    Obj(Vec<(String, J)>),
}

impl J {
    fn f64(value: f64) -> J {
        J::Num(value)
    }

    fn get(&self, key: &str) -> Option<&J> {
        let J::Obj(fields) = self else { return None };
        fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn set(&mut self, key: &str, value: J) {
        let J::Obj(fields) = self else { return };
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = value,
            None => fields.push((key.to_owned(), value)),
        }
    }

    fn as_u32(&self) -> Option<u32> {
        match self {
            J::Num(n) => Some(*n as u32),
            _ => None,
        }
    }
}

struct Decl {
    id: String,
}

struct NewBlock<'a> {
    decl: &'a Decl,
}

#[path = "../native/block_rows_table.rs"]
#[allow(dead_code)]
mod mekanism_rows;

/// O helper da injeção, como mora em `mod_content.rs`.
mod active_light {
    use super::{Decl, J, NewBlock, mekanism_rows};
    include!("../native/active_light.rs");
    pub fn run(id: &str, combo: &[(String, String)], row: &mut J) {
        let decl = Decl { id: id.to_owned() };
        mekanism_active_light(&NewBlock { decl: &decl }, combo, row);
    }
}

/// A linha de jogo do gabarito `minecraft:furnace` de um estado: `lit` acende com 13, e o resto é o do cubo cheio de pedra.
fn furnace_row(lit: bool) -> J {
    J::Obj(vec![
        ("emission".to_owned(), J::Num(if lit { 13.0 } else { 0.0 })),
        ("push".to_owned(), J::Str("NORMAL".to_owned())),
        ("can_occlude".to_owned(), J::Bool(true)),
        ("solid_render".to_owned(), J::Bool(true)),
        ("dampening".to_owned(), J::Num(15.0)),
        ("skylight_down".to_owned(), J::Bool(false)),
    ])
}

fn combo(lit: bool) -> Vec<(String, String)> {
    vec![
        ("facing".to_owned(), "north".to_owned()),
        ("lit".to_owned(), lit.to_string()),
    ]
}

/// O estado do original com `active` igual a `lit` (o port chama de `lit` o `active` do original) e o primeiro `facing`.
fn original_state(id: &str, active: bool) -> (Value, Value) {
    let row = blocos()
        .into_iter()
        .find(|row| row["id"] == format!("mekanism:{id}"))
        .unwrap();
    let state = row["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| {
            s["props"]["active"] == if active { "true" } else { "false" }
                && s["props"]["facing"] == "north"
                && s["props"].get("fluid_logged").is_none_or(|v| v == "empty")
        })
        .unwrap()
        .clone();
    (row["base"].clone(), state)
}

/// O helper dá a cada uma das quatro máquinas, em cada estado, o `push`, o `canOcclude`, o `isSolidRender`, o amortecimento e a luz do céu do original, e a luz do estado ativo é a do original.
#[test]
fn the_four_furnace_machines_get_the_original_piston_reaction_and_occlusion() {
    for id in [
        "crusher",
        "enrichment_chamber",
        "energized_smelter",
        "metallurgic_infuser",
    ] {
        for lit in [false, true] {
            let (base, state) = original_state(id, lit);
            let mut row = furnace_row(lit);
            active_light::run(&format!("mekanism:{id}"), &combo(lit), &mut row);
            let text = |key: &str| match row.get(key) {
                Some(J::Str(s)) => Value::from(s.as_str()),
                Some(J::Bool(b)) => Value::from(*b),
                Some(J::Num(n)) => Value::from(*n as u64),
                other => panic!("{id} {key}: {other:?}"),
            };
            assert_eq!(text("push"), base["push"], "{id}");
            assert_eq!(text("can_occlude"), base["can_occlude"], "{id}");
            assert_eq!(text("solid_render"), base["solid_render"], "{id}");
            assert_eq!(text("dampening"), base["dampening"], "{id}");
            assert_eq!(text("skylight_down"), base["skylight_down"], "{id}");
            assert_eq!(text("emission"), state["light"], "{id} lit={lit}");
        }
    }
}

/// O Metallurgic Infuser é o que muda no que se vê: o gabarito dá `solid_render` verdadeiro e o original, falso; as outras três máquinas são de cubo cheio no original e não mudam.
#[test]
fn only_the_metallurgic_infuser_stops_being_a_solid_render() {
    for (id, solid) in [
        ("crusher", true),
        ("enrichment_chamber", true),
        ("energized_smelter", true),
        ("metallurgic_infuser", false),
    ] {
        let mut row = furnace_row(false);
        active_light::run(&format!("mekanism:{id}"), &combo(false), &mut row);
        assert_eq!(row.get("solid_render"), Some(&J::Bool(solid)), "{id}");
    }
}

/// Só as quatro: um bloco que a tabela do forno não cita fica como está (os de `lodestone` têm a tabela `BLOCKS`).
#[test]
fn another_block_keeps_the_template_row() {
    for id in [
        "minecraft:furnace",
        "mekanism:block_salt",
        "mekanism:crusher_extra",
    ] {
        let mut row = furnace_row(true);
        active_light::run(id, &combo(true), &mut row);
        assert_eq!(row, furnace_row(true), "{id}");
    }
}
