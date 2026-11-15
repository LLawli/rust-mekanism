//! O clique sobre as abas laterais das quatro máquinas contra o que o cliente oficial responde: `GuiMekanism.hasClickedOutside` numa grade de
//! pixels de GUI inteiros ao redor da tela (`rastros/fora.jsonl`, gravado pela `CenaTelas` com `fora=1`), em cada estado (sem e com o upgrade
//! de xp, sem e com a aba de avisos). Cada linha do rastro é um estado e um modo:
//! - `completo`: o que o original tem, com as abas que o port não desenha (configuração, transporte, segurança, livro de receitas e redstone);
//! - `oculto`: as abas que o port não tem escondidas, como a cena de pixel;
//! - `sem_upgrades`: o mesmo sem a aba de upgrades, que a janela de upgrades da lib responde (`windows.rs`), então o que sobra é o que
//!   `screen_elements` declara.
mod common;

use common::Live;
use machinery_basis_core::container_sync::Mirror;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::screen_elements;
use mekanism_rules::{
    ELECTRIC_SCREEN, ELECTRIC_TRACKED_ERRORS, INFUSER_SCREEN, INFUSER_TRACKED_ERRORS, ScreenSpec,
    screen_elements as machine_elements,
};
use serde_json::Value;
use std::path::Path;

/// `GuiMekanism.imageWidth` e `imageHeight` das quatro telas.
const IMAGE: [i32; 2] = [176, 166];
/// A aba de upgrades (`GuiUpgradeWindowTab`: 176, 6, 26 por 26) tem o botão de 18 por 18 em (179, 10).
const UPGRADE_BUTTON: [i32; 4] = [179, 10, 18, 18];

struct Grid {
    case: String,
    mode: String,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    /// Por linha, os intervalos `[a, b)` de x em que o clique NÃO é "fora".
    rows: Vec<Vec<[i32; 2]>>,
}

impl Grid {
    /// O clique em (x, y) não é "fora" no original.
    fn over(&self, x: i32, y: i32) -> bool {
        let row = &self.rows[(y - self.y0) as usize];
        row.iter().any(|[a, b]| (*a..*b).contains(&x))
    }

    fn points(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (self.y0..self.y1).flat_map(|y| (self.x0..self.x1).map(move |x| (x, y)))
    }
}

fn trace() -> Vec<Grid> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/fora.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| {
            let value: Value = serde_json::from_str(line).unwrap();
            let int = |key: &str| value[key].as_i64().unwrap() as i32;
            Grid {
                case: value["case"].as_str().unwrap().to_owned(),
                mode: value["mode"].as_str().unwrap().to_owned(),
                x0: int("x0"),
                y0: int("y0"),
                x1: int("x1"),
                y1: int("y1"),
                rows: value["rows"]
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
                    .collect(),
            }
        })
        .collect()
}

/// O botão da aba de segurança (179, 38), 18 por 18.
fn security_button(x: i32, y: i32) -> bool {
    (179..197).contains(&x) && (38..56).contains(&y)
}

/// O botão da aba de redstone (`GuiRedstoneControlTab`: 176, 137; o botão em (179, 141)), 18 por 18.
fn redstone_button(x: i32, y: i32) -> bool {
    (179..197).contains(&x) && (141..159).contains(&y)
}

fn inside_image(x: i32, y: i32) -> bool {
    (0..IMAGE[0]).contains(&x) && (0..IMAGE[1]).contains(&y)
}

/// A máquina de um caso (`chamber_fora_xp_aviso`) e o estado dele. O aviso é a falta de energia (`energia=0` com entrada), que o servidor
/// sincroniza no dado dos erros rastreados; o xp é o upgrade instalado.
fn machine_of(case: &str) -> Option<(ScreenSpec, &'static [RecipeError], bool, bool)> {
    let (spec, tracked) = match case.split('_').next()? {
        "chamber" | "crusher" | "smelter" => (ELECTRIC_SCREEN, &ELECTRIC_TRACKED_ERRORS[..]),
        "infuser" => (INFUSER_SCREEN, &INFUSER_TRACKED_ERRORS[..]),
        _ => return None,
    };
    Some((spec, tracked, case.contains("_xp"), case.contains("_aviso")))
}

/// O espelho do menu que o cliente da máquina tem no estado do caso.
fn mirror_of(spec: ScreenSpec, tracked: &[RecipeError], xp: bool, warning: bool) -> Mirror {
    let errors = tracked
        .iter()
        .map(|error| warning && *error == RecipeError::NOT_ENOUGH_ENERGY)
        .collect();
    Live {
        xp_containment: xp,
        errors,
        ..Live::default()
    }
    .mirror(&spec)
}

fn grid<'a>(grids: &'a [Grid], case: &str, mode: &str) -> &'a Grid {
    grids
        .iter()
        .find(|grid| grid.case == case && grid.mode == mode)
        .unwrap_or_else(|| panic!("sem {case} {mode}"))
}

/// O port responde igual ao original em cada ponto da grade, em cada estado das quatro máquinas: o que `screen_elements` declara é, fora da
/// imagem, exatamente o que o original acha sob o mouse sem a aba de upgrades.
#[test]
fn the_declared_elements_are_the_ones_the_original_finds_under_the_mouse() {
    let grids = trace();
    let mut cases = 0;
    for original in grids.iter().filter(|grid| grid.mode == "sem_upgrades") {
        let Some((spec, tracked, xp, warning)) = machine_of(&original.case) else {
            continue;
        };
        let menu = format!("test:screen_elements:{}", original.case);
        screen_elements::declare(&menu, machine_elements(spec));
        let mirror = mirror_of(spec, tracked, xp, warning);
        let mut outside_hits = 0;
        for (x, y) in original.points() {
            // O rastro foi gravado com as abas de segurança e de redstone ocultas (a cena as esconde): os botões delas, que o port tem, ficam fora da comparação
            // (o modo `completo`, que as mostra, os confere em `the_redstone_button_is_what_the_original_finds`).
            if security_button(x, y) || redstone_button(x, y) {
                continue;
            }
            let port = inside_image(x, y)
                || screen_elements::over_element(&menu, [x as f32, y as f32], &mirror);
            assert_eq!(port, original.over(x, y), "{} em ({x}, {y})", original.case);
            outside_hits += usize::from(!inside_image(x, y) && port);
        }
        // A grade não é vazia: fora da imagem sobram a aba de energia (26 por 26), a de avisos (26 por 26) e o botão da aba de xp (18 por 18).
        let expected = 26 * 26 + if warning { 26 * 26 } else { 0 } + if xp { 18 * 18 } else { 0 };
        assert_eq!(outside_hits, expected, "{}", original.case);
        cases += 1;
    }
    // 4 máquinas em 4 estados.
    assert_eq!(cases, 16);
}

/// O botão da aba de redstone no original (o modo `completo`, com todas as abas visíveis): os 18 por 18 de dentro respondem, e o suporte de 26 por 26 em volta, não.
/// O port declara o mesmo botão, sempre ativo e visível.
#[test]
fn the_redstone_button_is_what_the_original_finds() {
    let grids = trace();
    for machine in ["chamber", "crusher", "smelter", "infuser"] {
        for state in ["base", "xp", "aviso", "xp_aviso"] {
            let case = format!("{machine}_fora_{state}");
            let full = grid(&grids, &case, "completo");
            let (spec, tracked, xp, warning) = machine_of(&case).unwrap();
            let menu = format!("test:screen_elements:redstone:{case}");
            screen_elements::declare(&menu, machine_elements(spec));
            let mirror = mirror_of(spec, tracked, xp, warning);
            for (x, y) in full.points().filter(|(x, y)| (176..210).contains(x) && (132..166).contains(y)) {
                assert_eq!(
                    full.over(x, y) && redstone_button(x, y),
                    redstone_button(x, y),
                    "{case} em ({x}, {y}): o original responde no botão inteiro"
                );
                if redstone_button(x, y) {
                    assert!(screen_elements::over_element(&menu, [x as f32, y as f32], &mirror), "{case} em ({x}, {y})");
                }
            }
            // O suporte em volta (3 pixels à esquerda, 5 à direita, 4 em cima e embaixo) não é elemento.
            for (x, y) in [(177, 150), (198, 150), (188, 139), (188, 160)] {
                assert!(!full.over(x, y), "{case} em ({x}, {y})");
            }
        }
    }
}

/// Cada aba responde só quando o original a mostra: o mesmo ponto sobre a aba de xp ou a de avisos muda com o estado, no original e no port.
#[test]
fn the_xp_tab_and_the_warning_tab_follow_their_state() {
    let grids = trace();
    for machine in ["chamber", "crusher", "smelter", "infuser"] {
        let at = |case: &str, x: i32, y: i32| grid(&grids, case, "sem_upgrades").over(x, y);
        // O botão da aba de xp (179, 94, 18, 18) e a aba de avisos (-26, 109, 26, 26).
        assert!(!at(&format!("{machine}_fora_base"), 188, 103));
        assert!(at(&format!("{machine}_fora_xp"), 188, 103));
        assert!(!at(&format!("{machine}_fora_base"), -13, 120));
        assert!(at(&format!("{machine}_fora_aviso"), -13, 120));
        assert!(at(&format!("{machine}_fora_xp_aviso"), 188, 103));
        assert!(at(&format!("{machine}_fora_xp_aviso"), -13, 120));
        // A aba de energia, sempre.
        for state in ["base", "xp", "aviso", "xp_aviso"] {
            assert!(at(&format!("{machine}_fora_{state}"), -13, 150));
        }
        // O suporte de 26 por 26 da aba de xp não é elemento: só o botão de dentro (179, 94, 18, 18). A borda de 3 pixels e a de 5 soltam o item.
        assert!(!at(&format!("{machine}_fora_xp"), 177, 103));
        assert!(!at(&format!("{machine}_fora_xp"), 198, 103));
        assert!(!at(&format!("{machine}_fora_xp"), 188, 92));
        assert!(!at(&format!("{machine}_fora_xp"), 188, 113));
    }
}

/// O que o port não desenha: o original tem mais elementos (`completo`), e tudo o que o port responde o original responde também. O que
/// sobra no original e falta no port é a lista de faltas (configuração, transporte, livro de receitas).
#[test]
fn the_port_never_claims_more_than_the_original_has() {
    let grids = trace();
    for hidden in grids.iter().filter(|grid| grid.mode == "oculto") {
        let Some((spec, tracked, xp, warning)) = machine_of(&hidden.case) else {
            continue;
        };
        let full = grid(&grids, &hidden.case, "completo");
        let menu = format!("test:screen_elements:full:{}", hidden.case);
        screen_elements::declare(&menu, machine_elements(spec));
        let mirror = mirror_of(spec, tracked, xp, warning);
        let mut missing = 0;
        for (x, y) in full.points() {
            let port = screen_elements::over_element(&menu, [x as f32, y as f32], &mirror);
            assert!(!port || full.over(x, y), "{} em ({x}, {y})", hidden.case);
            missing += usize::from(!inside_image(x, y) && full.over(x, y) && !port);
        }
        // Falta de verdade: a configuração, o transporte, o livro e a aba de upgrades (que é da janela da lib).
        assert!(
            missing >= 4 * 18 * 18,
            "{} faltas em {}",
            missing,
            hidden.case
        );
    }
}

/// A única diferença entre `oculto` e `sem_upgrades` é o botão da aba de upgrades, em (179, 10, 18, 18).
#[test]
fn the_upgrade_tab_is_the_only_thing_the_window_of_the_lib_answers() {
    let grids = trace();
    let mut checked = 0;
    for hidden in grids.iter().filter(|grid| grid.mode == "oculto") {
        let without = grid(&grids, &hidden.case, "sem_upgrades");
        let [bx, by, bw, bh] = UPGRADE_BUTTON;
        for (x, y) in hidden.points() {
            let button = (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y);
            assert_eq!(
                hidden.over(x, y),
                without.over(x, y) || button,
                "{} em ({x}, {y})",
                hidden.case
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
}

/// A grade cobre as margens de todas as abas e o rastro tem os 18 estados nos 3 modos (4 máquinas em 4 estados e o gerador em 2).
#[test]
fn the_trace_covers_every_state_in_every_mode() {
    let grids = trace();
    assert_eq!(grids.len(), 18 * 3);
    for grid in &grids {
        assert_eq!(
            (grid.x0, grid.y0, grid.x1, grid.y1),
            (-32, -6, 240, 176),
            "{}",
            grid.case
        );
        assert_eq!(grid.rows.len(), (grid.y1 - grid.y0) as usize);
        assert!(["completo", "oculto", "sem_upgrades"].contains(&grid.mode.as_str()));
    }
}
