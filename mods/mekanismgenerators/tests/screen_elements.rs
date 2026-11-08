//! As abas do Heat Generator fora da imagem contra o que o cliente oficial responde: `GuiMekanism.hasClickedOutside` numa grade de pixels de GUI
//! inteiros (`rastros/fora.jsonl`, `generator_fora_*`, modo `sem_upgrades`: a aba de upgrades é da janela da lib). Os cinco slots da barra de
//! armadura não contam para o original (`GuiSlot` está inativo): quem os acha sob o mouse é o NeoForge, e o port os põe em `armor_bar::over_slot`.
mod common;

use machinery_basis_core::{armor_bar, screen_elements};
use mekanismgenerators_rules as rules;
use serde_json::Value;
use std::path::Path;

const IMAGE: [i32; 2] = [176, 166];

/// A linha `case`/`mode` do rastro: a grade e, por linha, os intervalos `[a, b)` de x em que o clique NÃO é "fora".
fn grid(case: &str, mode: &str) -> (i32, i32, i32, i32, Vec<Vec<[i32; 2]>>) {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/fora.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let value: Value = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|value| value["case"] == case && value["mode"] == mode)
        .unwrap_or_else(|| panic!("sem {case} {mode}"));
    let int = |key: &str| value[key].as_i64().unwrap() as i32;
    let rows = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|run| {
                    let run = run.as_array().unwrap();
                    [
                        run[0].as_i64().unwrap() as i32,
                        run[1].as_i64().unwrap() as i32,
                    ]
                })
                .collect()
        })
        .collect();
    (int("x0"), int("y0"), int("x1"), int("y1"), rows)
}

#[test]
fn the_generator_tabs_are_what_the_original_finds_under_the_mouse() {
    screen_elements::declare_once(rules::MENU, rules::screen_elements);
    armor_bar::declare_once(rules::MENU, rules::armor_bar_declaration);
    // Os elementos não dependem de dado nenhum do servidor: o espelho é o do menu recém-aberto.
    let (client, _) = common::client_of(&common::Server::new(0.0), false);
    for case in ["generator_fora_base", "generator_fora_lava"] {
        let (x0, y0, x1, y1, rows) = grid(case, "sem_upgrades");
        assert_eq!((x0, y0, x1, y1), (-32, -6, 240, 176));
        let mut outside_hits = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let java = rows[(y - y0) as usize]
                    .iter()
                    .any(|[a, b]| (*a..*b).contains(&x));
                let inside = (0..IMAGE[0]).contains(&x) && (0..IMAGE[1]).contains(&y);
                let at = [x as f32, y as f32];
                let port = inside || screen_elements::over_element(rules::MENU, at, &client.mirror);
                assert_eq!(port, java, "{case} em ({x}, {y})");
                outside_hits += usize::from(!inside && java);
                // Os slots da barra ficam de fora do original e dentro do jogo: o clique sobre eles nunca é "fora" no NeoForge.
                if armor_bar::over_slot(rules::MENU, at) {
                    assert!(!java, "{case}: um slot da barra em ({x}, {y})");
                }
            }
        }
        // A aba de calor e a de energia, 26 por 26 cada.
        assert_eq!(outside_hits, 2 * 26 * 26, "{case}");
    }
}
