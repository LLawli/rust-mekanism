//! A posição das pilhas em trânsito que `RenderLogisticalTransporter` desenha, contra o original: `rastros/transporter_render_position.jsonl` (`testes/TransporterRenderTraceTest.java`)
//! guarda, para cada tier, cada par de lados (de onde a pilha veio e para onde vai), cada progresso e cada `partialTick`, o `TransporterStack.getSide` e o
//! `TransporterUtils.getStackPosition` do original com os bits dos `float`.
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::client::ClientStack;
use machinery_basis_core::transporter::pos::{NO_POS, relative_pos};
use machinery_basis_core::transporter::render::{client_side, drawn_stacks, stack_position};
use std::path::Path;

fn rastro() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../dev/referencia/rastros/transporter_render_position.jsonl"),
    )
    .unwrap()
}

fn stack(index: i32, item: Option<&str>, progress: i32, color: Option<u8>) -> ClientStack {
    ClientStack {
        index,
        item: item.map(|id| (id.to_owned(), 1)),
        components: None,
        color,
        progress,
        original_location: 0x1_0000_0007,
        path_type: None,
        next: None,
        previous: None,
    }
}

#[test]
fn the_position_of_a_stack_is_the_one_of_the_original_for_every_side_pair() {
    let at = BlockPos::new(10, 64, -3).as_long();
    let mut lines = 0;
    let mut sides = [0usize; 6];
    for line in rastro().lines() {
        let row: Vec<i64> = serde_json::from_str(line).unwrap();
        let [_, prev, next, progress, partial, want_side, x, y, z] = row[..] else {
            panic!("{line}");
        };
        let relative = |side: i64| match side {
            -1 => None,
            side => Some(relative_pos(at, Face::ALL[side as usize])),
        };
        let progress = progress as i32;
        let side = client_side(progress, relative(prev), relative(next), at);
        assert_eq!(side as usize, want_side as usize, "{line}");
        sides[side as usize] += 1;
        let position = stack_position(side, progress, f32::from_bits(partial as u32));
        let want = [x, y, z].map(|bits| f32::from_bits(bits as u32));
        assert_eq!(position.map(f32::to_bits), want.map(f32::to_bits), "{line}");
        lines += 1;
    }
    // 4 tiers, 7 origens, 7 destinos, 10 progressos, 5 `partialTick`.
    assert_eq!(lines, 4 * 7 * 7 * 10 * 5);
    assert!(sides.iter().all(|count| *count > 0), "{sides:?}");
}

#[test]
fn a_missing_position_falls_back_to_down() {
    assert_eq!(client_side(10, None, Some(5), 7), Face::Down);
    assert_eq!(client_side(60, Some(5), None, 7), Face::Down);
    assert_eq!(client_side(10, Some(NO_POS), None, 7), Face::Down);
}

#[test]
fn identical_stacks_in_the_same_spot_are_drawn_once_and_empty_ones_not_at_all() {
    let a = stack(1, Some("minecraft:stick"), 40, None);
    let same = stack(2, Some("minecraft:stick"), 40, None);
    let other_color = stack(3, Some("minecraft:stick"), 40, Some(2));
    let other_progress = stack(4, Some("minecraft:stick"), 41, None);
    let other_item = stack(5, Some("minecraft:coal"), 40, None);
    let empty = stack(6, None, 40, None);
    let transit = [
        &a,
        &same,
        &other_color,
        &other_progress,
        &other_item,
        &empty,
    ];
    let drawn = drawn_stacks(&transit, 0, 5, 0.5);
    let indexes: Vec<i32> = drawn.iter().map(|d| d.stack.index).collect();
    assert_eq!(indexes, vec![1, 3, 4, 5]);
    // A semente é a metade de baixo do `originalLocation`, `(int)`.
    assert_eq!(drawn[0].seed, 7);
    assert_eq!(drawn[1].color, Some(2));
}

#[test]
fn partial_tick_moves_the_stack_by_the_speed_of_the_tier() {
    let a = stack(1, Some("minecraft:stick"), 0, None);
    let slow = drawn_stacks(&[&a], 0, 5, 1.0)[0].position;
    let fast = drawn_stacks(&[&a], 0, 20, 1.0)[0].position;
    // `DOWN`: só o y anda, de -0,5 em direção a +0,5 do passo, a 0,25.
    assert_eq!(slow, [0.5, 0.25 - (5.0f32 / 100.0 - 0.5), 0.5]);
    assert!(fast[1] < slow[1]);
}
