//! Os avisos de vizinhança do mundo simulado (`sim::chunks`): `Level.setBlock` com e sem a flag 1, `updateNeighborsAt` na ordem de
//! `NeighborUpdater.UPDATE_ORDER`, o `onNeighborChange` do NeoForge na ordem de `Level.NEIGHBOR_UPDATE_LIST`, e `setPlacedBy`.
//! As duas ordens vêm do jogo (`dev/referencia/rastros/neighbors_*.jsonl`, primeira linha, lida por reflexão em `NeighborTraceTest`).
use machinery_basis_core::neighbors::{OUTPUT_SIGNAL_ORDER, UPDATE_ORDER, flags};
use machinery_basis_core::sim::chunks::{
    BlockPos, ChunkHooks, ChunkPos, ENTITY_TICKING_LEVEL, LevelView, NeighborCall, SimLevel,
};

const MACHINE: &str = "test:machine";
const LEVER: &str = "test:lever_on";
const STONE: &str = "test:stone";

/// Quem recebeu o quê, com o sinal que o aviso entregou.
#[derive(Default)]
struct Probe {
    calls: Vec<String>,
}

impl ChunkHooks for Probe {
    type BlockEntity = BlockPos;
    type Saved = ();

    fn new_block_entity(&mut self, pos: BlockPos, block: &str, _: Option<&()>) -> Option<BlockPos> {
        (block == MACHINE).then_some(pos)
    }

    fn save_block_entity(&mut self, _: BlockPos, _: &mut BlockPos) {}

    fn block_signal(&self, block: &str) -> i32 {
        if block == LEVER { 15 } else { 0 }
    }

    fn has_analog_output(&self, block: &str) -> bool {
        block == MACHINE
    }

    fn neighbor_changed(
        &mut self,
        _: &LevelView<BlockPos>,
        pos: BlockPos,
        _: &mut BlockPos,
        powered: bool,
    ) {
        self.calls
            .push(format!("neighbor_changed {} {} {powered}", pos.x, pos.z));
    }

    fn on_neighbor_change(
        &mut self,
        _: &LevelView<BlockPos>,
        pos: BlockPos,
        _: &mut BlockPos,
        powered: bool,
        neighbor: BlockPos,
    ) {
        self.calls.push(format!(
            "on_neighbor_change {} {} {powered} from {} {}",
            pos.x, pos.z, neighbor.x, neighbor.z
        ));
    }

    fn placed_by(
        &mut self,
        _: &LevelView<BlockPos>,
        pos: BlockPos,
        _: &mut BlockPos,
        powered: bool,
    ) {
        self.calls
            .push(format!("placed_by {} {} {powered}", pos.x, pos.z));
    }
}

fn level() -> SimLevel<Probe> {
    let mut level = SimLevel::new(Probe::default());
    for x in -1..=1 {
        for z in -1..=1 {
            level.set_ticket_level(ChunkPos::new(x, z), ENTITY_TICKING_LEVEL);
        }
    }
    level.tick();
    level
}

/// O centro e as seis casas ao redor dele, todas com uma máquina (uma só por face, em y = 64).
fn around() -> (BlockPos, [BlockPos; 6]) {
    let center = BlockPos::new(0, 64, 0);
    let faces = machinery_basis_core::sides::Face::ALL.map(|face| center.relative(face));
    (center, faces)
}

fn face_names(order: [machinery_basis_core::sides::Face; 6]) -> Vec<BlockPos> {
    let (center, _) = around();
    order
        .into_iter()
        .map(|face| center.relative(face))
        .collect()
}

fn neighbor_positions(calls: Vec<NeighborCall>) -> Vec<BlockPos> {
    calls
        .into_iter()
        .filter_map(|call| match call {
            NeighborCall::NeighborChanged { pos, .. } => Some(pos),
            _ => None,
        })
        .collect()
}

fn ring_of_machines(level: &mut SimLevel<Probe>) -> BlockPos {
    let (center, faces) = around();
    for pos in faces {
        level.set_block_flags(pos, Some(MACHINE), flags::UPDATE_CLIENTS);
    }
    level.take_neighbor_calls();
    level.hooks.calls.clear();
    center
}

#[test]
fn set_block_with_the_first_flag_notifies_the_six_neighbours_in_the_update_order() {
    let mut level = level();
    let center = ring_of_machines(&mut level);
    level.set_block(center, Some(STONE));
    assert_eq!(
        neighbor_positions(level.take_neighbor_calls()),
        face_names(UPDATE_ORDER)
    );
}

#[test]
fn the_update_order_is_west_east_down_up_north_south() {
    let mut level = level();
    let center = ring_of_machines(&mut level);
    level.set_block(center, Some(STONE));
    let seen: Vec<(i32, i32, i32)> = level
        .hooks
        .calls
        .iter()
        .map(|call| {
            let words: Vec<&str> = call.split(' ').collect();
            (words[1].parse().unwrap(), 0, words[2].parse().unwrap())
        })
        .collect();
    // West (-1, 0), east (1, 0), down e up caem em (0, 0), north (0, -1), south (0, 1).
    assert_eq!(
        seen,
        vec![
            (-1, 0, 0),
            (1, 0, 0),
            (0, 0, 0),
            (0, 0, 0),
            (0, 0, -1),
            (0, 0, 1)
        ]
    );
}

#[test]
fn set_block_without_the_first_flag_notifies_nobody() {
    let mut level = level();
    let center = ring_of_machines(&mut level);
    level.set_block_flags(center, Some(STONE), flags::UPDATE_CLIENTS);
    assert!(level.take_neighbor_calls().is_empty());
    assert!(level.hooks.calls.is_empty());
    // `/setblock ... strict`: 2 | 816.
    level.set_block_flags(center, Some(LEVER), flags::UPDATE_CLIENTS | 816);
    assert!(level.take_neighbor_calls().is_empty());
}

#[test]
fn the_block_that_changed_does_not_notify_itself() {
    let mut level = level();
    let center = BlockPos::new(0, 64, 0);
    level.set_block(center, Some(MACHINE));
    assert!(
        neighbor_positions(level.take_neighbor_calls()).is_empty(),
        "o bloco posto não recebe o próprio aviso"
    );
}

#[test]
fn the_signal_is_read_when_the_notification_arrives() {
    let mut level = level();
    let center = ring_of_machines(&mut level);
    level.set_block(center, Some(LEVER));
    // Cada máquina encosta na alavanca: todas leem sinal.
    assert_eq!(level.hooks.calls.len(), 6, "{:?}", level.hooks.calls);
    assert!(
        level.hooks.calls.iter().all(|call| call.contains("true")),
        "{:?}",
        level.hooks.calls
    );
    level.hooks.calls.clear();
    // Sem a flag o sinal muda e ninguém sabe; o próximo aviso, de qualquer bloco, lê o sinal de agora.
    level.set_block_flags(center, Some(STONE), flags::UPDATE_CLIENTS);
    assert!(level.hooks.calls.is_empty());
    level.update_neighbors_at(center);
    assert_eq!(level.hooks.calls.len(), 6, "{:?}", level.hooks.calls);
    assert!(
        level.hooks.calls.iter().all(|call| call.ends_with("false")),
        "{:?}",
        level.hooks.calls
    );
}

#[test]
fn a_block_with_analog_output_also_sends_on_neighbor_change_after_the_neighbours() {
    let mut level = level();
    let (center, faces) = around();
    for pos in faces {
        level.set_block_flags(pos, Some(MACHINE), flags::UPDATE_CLIENTS);
    }
    level.take_neighbor_calls();
    level.set_block(center, Some(MACHINE));
    let calls = level.take_neighbor_calls();
    let kinds: Vec<bool> = calls
        .iter()
        .map(|call| matches!(call, NeighborCall::NeighborChanged { .. }))
        .collect();
    assert_eq!(kinds, [[true; 6], [false; 6]].concat());
    let output: Vec<BlockPos> = calls
        .into_iter()
        .filter_map(|call| match call {
            NeighborCall::OnNeighborChange { pos, neighbor } => {
                assert_eq!(neighbor, center);
                Some(pos)
            }
            _ => None,
        })
        .collect();
    assert_eq!(output, face_names(OUTPUT_SIGNAL_ORDER));
}

#[test]
fn the_output_signal_order_is_north_east_south_west_up_down() {
    let mut level = level();
    let center = ring_of_machines(&mut level);
    level.update_neighbour_for_output_signal(center);
    let (north, east, south, west) = (
        BlockPos::new(0, 64, -1),
        BlockPos::new(1, 64, 0),
        BlockPos::new(0, 64, 1),
        BlockPos::new(-1, 64, 0),
    );
    let (up, down) = (BlockPos::new(0, 65, 0), BlockPos::new(0, 63, 0));
    let order: Vec<BlockPos> = level
        .take_neighbor_calls()
        .into_iter()
        .map(|call| match call {
            NeighborCall::OnNeighborChange { pos, .. } => pos,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(order, [north, east, south, west, up, down]);
}

#[test]
fn only_neighbours_with_a_block_entity_and_a_loaded_chunk_are_notified() {
    let mut level = level();
    let center = BlockPos::new(0, 64, 0);
    level.set_block_flags(
        center.relative(machinery_basis_core::sides::Face::East),
        Some(STONE),
        flags::UPDATE_CLIENTS,
    );
    level.set_block_flags(
        center.relative(machinery_basis_core::sides::Face::West),
        Some(MACHINE),
        flags::UPDATE_CLIENTS,
    );
    level.take_neighbor_calls();
    level.update_neighbors_at(center);
    assert_eq!(
        neighbor_positions(level.take_neighbor_calls()),
        vec![BlockPos::new(-1, 64, 0)]
    );
    // Um vizinho em chunk descarregado não é avisado (o jogo carregaria o chunk; o simulador o ignora).
    level.unload_chunk(ChunkPos::new(-1, 0));
    level.update_neighbors_at(center);
    assert!(neighbor_positions(level.take_neighbor_calls()).is_empty());
}

#[test]
fn a_block_placed_by_a_player_reads_the_signal_and_a_set_block_does_not() {
    let mut level = level();
    let (center, faces) = around();
    level.set_block_flags(faces[0], Some(LEVER), flags::UPDATE_CLIENTS);
    level.take_neighbor_calls();
    level.set_block_flags(center, Some(MACHINE), flags::UPDATE_ALL);
    // `/setblock`: o bloco novo não é avisado, mesmo encostado numa alavanca ligada.
    assert!(
        !level
            .hooks
            .calls
            .iter()
            .any(|call| call.starts_with("placed_by")),
        "{:?}",
        level.hooks.calls
    );
    level.hooks.calls.clear();
    level.set_block_flags(center, None, flags::UPDATE_ALL);
    level.take_neighbor_calls();
    level.hooks.calls.clear();
    // `BlockItem.place`: `setPlacedBy` lê o sinal.
    assert!(level.place_by_player(center, MACHINE));
    assert_eq!(
        level.hooks.calls.last().map(String::as_str),
        Some("placed_by 0 0 true")
    );
    assert!(matches!(
        level.take_neighbor_calls().last(),
        Some(NeighborCall::PlacedBy { pos }) if *pos == center
    ));
}
