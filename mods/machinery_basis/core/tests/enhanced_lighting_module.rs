//! O módulo `enhanced-lighting` que entra em `crates/horizon-client-render/src/mesher.rs` (`mods/machinery_basis/enhanced_lighting.rs`): o sombreamento de vértice do `EnhancedBlockModelLighter`
//! do NeoForge. O arquivo é código só de `std`, então o teste o compila como está e o confere contra o NeoForge: `rastros/lighting_enhanced.jsonl` (`testes/CenaLuz.java`) tem, para cada quad de
//! cada bloco, a cor e a luz dos quatro vértices pelo vanilla e pelo NeoForge, e as 27 células em volta do bloco (a luz, o brilho de sombra, se deixa a luz passar).
#[path = "../../enhanced_lighting.rs"]
mod enhanced;

use enhanced::{Cell, Face, Quad, ambient_occlusion, before_directional, gray};
use serde_json::Value;
use std::path::Path;

fn rastro() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/lighting_enhanced.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    match name {
        "down" => Face::Down,
        "up" => Face::Up,
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        "east" => Face::East,
        other => panic!("{other}"),
    }
}

/// `CardinalLighting.DEFAULT`.
fn cardinal(face: Face) -> f32 {
    match face {
        Face::Down => 0.5,
        Face::Up => 1.0,
        Face::North | Face::South => 0.8,
        Face::West | Face::East => 0.6,
    }
}

fn cells(world: &Value) -> Vec<Cell> {
    let list = world["celulas"].as_array().unwrap();
    assert_eq!(list.len(), 27);
    list.iter()
        .map(|cell| Cell {
            light: cell["luz"].as_i64().unwrap() as u32,
            shade: cell["sombra"].as_f64().unwrap() as f32,
            permeable: cell["passa"].as_bool().unwrap(),
        })
        .collect()
}

fn positions(quad: &Value) -> [[f32; 3]; 4] {
    std::array::from_fn(|v| {
        std::array::from_fn(|axis| quad["posicoes"][v][axis].as_f64().unwrap() as f32)
    })
}

fn ints(value: &Value) -> [i64; 4] {
    std::array::from_fn(|i| value[i].as_i64().unwrap())
}

struct Counts {
    quads: usize,
    irregular: usize,
    differ_from_vanilla: usize,
    /// Os quads com `shadeDirectionOverride` (os LEDs do Mekanism: `shade_direction_override: up`) e, entre eles, os que o módulo dá como o NeoForge deu.
    overridden: usize,
    overridden_equal: usize,
}

/// Cada quad do rastro, com o mundo do bloco dele: `ambient_occlusion` do módulo dá a cor e a luz que o NeoForge deu, e a luz suave sai nos mesmos bits (0..8 e 16..24).
fn check(only: Option<&str>) -> Counts {
    let mut counts = Counts {
        quads: 0,
        irregular: 0,
        differ_from_vanilla: 0,
        overridden: 0,
        overridden_equal: 0,
    };
    let mut world: Option<(Vec<Cell>, bool)> = None;
    for row in rastro() {
        if row["tipo"] == "mundo" {
            world = Some((cells(&row), row["cheio"].as_bool().unwrap()));
            continue;
        }
        if !row["ao"].as_bool().unwrap() {
            continue;
        }
        if only.is_some_and(|only| row["bloco"] != only) {
            continue;
        }
        let (cells, full) = world
            .as_ref()
            .expect("a linha `mundo` vem antes dos quads do bloco");
        let cell = |d: [i32; 3]| cells[(((d[1] + 1) * 3 + (d[2] + 1)) * 3 + (d[0] + 1)) as usize];
        let positions = positions(&row);
        let direction = face(row["direcao"].as_str().unwrap());
        let shade_override = Some(row["sombra_forcada"].as_str().unwrap())
            .filter(|name| !name.is_empty())
            .map(face);
        let quad = Quad {
            positions: &positions,
            direction,
            shade_override,
        };
        let (colors, lights) = ambient_occlusion(&cell, *full, &cardinal, &quad);
        let expected = &row["neoforge"];
        let (java_colors, java_lights) = (ints(&expected["cor"]), ints(&expected["luz"]));
        let java_grays: [i64; 4] = std::array::from_fn(|v| java_colors[v] & 0xFF);
        let mine_colors: [i64; 4] = std::array::from_fn(|v| i64::from(colors[v]));
        let mine_lights: [i64; 4] = std::array::from_fn(|v| i64::from(lights[v]));
        let equal = mine_colors == java_grays && mine_lights == java_lights;
        if shade_override.is_some() {
            // O NeoForge guarda as faces calculadas num `int` de bits indexado por `direção + 6 se de fora + (sombra forçada + 1) * 12`, até 83: `1 << índice` passa de 31 e o bit cai em outro
            // índice (`FullFaceCalculator.calculatedAoFaces`), então um quad com sombra forçada pode receber o resultado, velho ou zerado, de outra configuração. Isso depende do que a thread de
            // malha calculou antes, não se reproduz: o módulo calcula a face de novo, e o teste só conta quantos desses quads o NeoForge deu como a conta limpa.
            counts.overridden += 1;
            counts.overridden_equal += usize::from(equal);
        } else {
            assert_eq!(
                mine_colors, java_grays,
                "{} {} {:?}: cor",
                row["bloco"], row["direcao"], positions
            );
            assert_eq!(
                mine_lights, java_lights,
                "{} {} {:?}: luz",
                row["bloco"], row["direcao"], positions
            );
        }
        counts.quads += 1;
        let vanilla = &row["vanilla"];
        let (vanilla_colors, vanilla_lights) = (ints(&vanilla["cor"]), ints(&vanilla["luz"]));
        if vanilla_colors != java_colors || vanilla_lights != java_lights {
            counts.differ_from_vanilla += 1;
        }
        let min = |axis: usize| positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
        let max = |axis: usize| positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
        let flat = match direction {
            Face::Down | Face::Up => min(1) == max(1),
            Face::North | Face::South => min(2) == max(2),
            Face::West | Face::East => min(0) == max(0),
        };
        counts.irregular += usize::from(!flat);
    }
    counts
}

#[test]
fn every_quad_of_the_trace_gets_the_color_and_the_light_the_neoforge_gave() {
    let counts = check(None);
    // O rastro tem os blocos de `cenas/luz.txt`: 1044 quads com AO (dos blocos do Mekanism e seis do vanilla), 65 deles de elemento girado.
    assert!(counts.quads >= 1000, "só {} quads no rastro", counts.quads);
    assert!(
        counts.irregular >= 60,
        "só {} quads de elemento girado no rastro",
        counts.irregular
    );
    // Os de sombra forçada: o módulo dá o resultado limpo, e a maior parte deles o NeoForge também deu (34 de 41).
    assert!(
        counts.overridden >= 40,
        "só {} quads com sombra forçada",
        counts.overridden
    );
    assert!(
        counts.overridden_equal * 5 >= counts.overridden * 4,
        "{} de {} quads com sombra forçada",
        counts.overridden_equal,
        counts.overridden
    );
}

/// O que o teste prova só vale se o NeoForge e o vanilla dão resultados diferentes no rastro: sem a diferença, trocar o `BlockModelLighter` pelo do NeoForge não mudaria nada.
#[test]
fn the_neoforge_differs_from_the_vanilla_lighter_in_this_scene() {
    let counts = check(None);
    assert!(
        counts.differ_from_vanilla >= 10,
        "só {} quads diferem do vanilla",
        counts.differ_from_vanilla
    );
    // E a caixa de papelão aberta, a diferença que o print mostrou, está entre eles.
    let cardboard = check(Some("mekanism:cardboard_box"));
    assert!(
        cardboard.irregular >= 4,
        "as quatro faces de cada aba são de elemento girado"
    );
}

/// O mesher do Horizon multiplica o brilho pelo sombreamento direcional (`scale(gray(b), directional)`): `before_directional` devolve o brilho que dá o canal do NeoForge, para todo canal que
/// algum brilho consegue dar (`floor(g * directional)`), e o `gray` do Horizon o lê de volta como o `g` escolhido.
#[test]
fn the_brightness_before_the_directional_shade_lands_on_the_neoforge_channel() {
    let scale = |channel: i32, by: f32| ((channel as f32 * by) as i32).clamp(0, 255);
    for directional in [0.5f32, 0.6, 0.8, 0.9, 1.0] {
        let reachable: Vec<i32> = (0..=255).map(|g| scale(g, directional)).collect();
        for target in 0..=255 {
            if !reachable.contains(&target) {
                continue;
            }
            let b = before_directional(target, directional);
            assert_eq!(
                scale(gray(b), directional),
                target,
                "alvo {target} com {directional}"
            );
        }
    }
}

#[test]
fn the_state_table_is_per_thread_and_follows_the_last_set() {
    assert!(!enhanced::uses(3));
    enhanced::set(vec![false, true, false, true]);
    assert!(enhanced::uses(1) && enhanced::uses(3));
    assert!(!enhanced::uses(0) && !enhanced::uses(2) && !enhanced::uses(99));
    enhanced::set(vec![true]);
    assert!(enhanced::uses(0) && !enhanced::uses(1));
}
