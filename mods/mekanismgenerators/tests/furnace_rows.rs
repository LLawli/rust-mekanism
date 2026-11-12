//! O `heat_generator` segue com o gabarito `minecraft:furnace` (Fase 5), e a injeção `block_rows` (`native/block_rows.rs`) lhe aplica a reação ao pistão e a oclusão do original (a tabela
//! `FURNACE` de `native/block_rows_table.rs`): o gabarito de cubo cheio dá `isSolidRender` verdadeiro, amortecimento 15 e a luz do céu barrada, e o `BlockTileModel` do original é de forma menor que
//! o cubo (`isSolidRender` falso, amortecimento 0, a luz do céu passa; com lava dentro, amortecimento 1 e a luz barrada). O teste roda o helper de verdade sobre a linha de jogo do gabarito, estado
//! a estado, e compara com o original em jogo (`rastros/blocos_generators.jsonl`).
#[path = "common/generators.rs"]
mod generators;

use generators::blocos;
use serde_json::Value;

/// O `J` de `mod_content.rs` do Horizon (JSON com as chaves na ordem do arquivo), só com o que a injeção `block_rows` toca.
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
}

struct Decl {
    id: String,
}

struct NewBlock<'a> {
    decl: &'a Decl,
}

#[path = "../native/block_rows_table.rs"]
#[allow(dead_code)]
mod mekanismgenerators_rows;

/// O helper da injeção, como mora em `mod_content.rs`.
mod block_rows {
    use super::{Decl, J, NewBlock, mekanismgenerators_rows};
    include!("../native/block_rows.rs");
    pub fn run(id: &str, combo: &[(String, String)], row: &mut J) {
        let decl = Decl { id: id.to_owned() };
        mekanismgenerators_block_rows(&NewBlock { decl: &decl }, combo, row);
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

fn combo(lit: bool, fluid: &str) -> Vec<(String, String)> {
    vec![
        ("facing".to_owned(), "north".to_owned()),
        ("lit".to_owned(), lit.to_string()),
        ("fluid_logged".to_owned(), fluid.to_owned()),
    ]
}

/// O helper dá ao `heat_generator`, em cada estado (seco e com lava dentro), o `push`, o `canOcclude`, o `isSolidRender`, o amortecimento e a luz do céu do original, e não toca na luz do gabarito.
#[test]
fn the_heat_generator_gets_the_original_piston_reaction_and_occlusion() {
    let row = blocos()
        .into_iter()
        .find(|row| row["id"] == "mekanismgenerators:heat_generator")
        .unwrap();
    for lit in [false, true] {
        for fluid in ["empty", "lava"] {
            let state = row["states"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| {
                    s["props"]["active"] == if lit { "true" } else { "false" }
                        && s["props"]["facing"] == "north"
                        && s["props"]["fluid_logged"] == fluid
                })
                .unwrap();
            let mut got = furnace_row(lit);
            block_rows::run(
                "mekanismgenerators:heat_generator",
                &combo(lit, fluid),
                &mut got,
            );
            let text = |key: &str| match got.get(key) {
                Some(J::Str(s)) => Value::from(s.as_str()),
                Some(J::Bool(b)) => Value::from(*b),
                Some(J::Num(n)) => Value::from(*n as u64),
                other => panic!("{key}: {other:?}"),
            };
            for key in [
                "push",
                "can_occlude",
                "solid_render",
                "dampening",
                "skylight_down",
            ] {
                assert_eq!(text(key), state[key], "{key} lit={lit} {fluid}");
            }
            // A luz é a do gabarito (13 aceso): a do estado ativo e a da lava são de outras linhas (`logged-fluid-row` e o gabarito).
            assert_eq!(
                got.get("emission"),
                Some(&J::Num(if lit { 13.0 } else { 0.0 }))
            );
        }
    }
}

/// Só o `heat_generator`: outro bloco do Generators fora das duas tabelas fica como está.
#[test]
fn another_block_keeps_the_template_row() {
    for id in [
        "minecraft:furnace",
        "mekanismgenerators:heat_generator_extra",
    ] {
        let mut row = furnace_row(true);
        block_rows::run(id, &combo(true, "empty"), &mut row);
        assert_eq!(row, furnace_row(true), "{id}");
    }
}
