//! A ordem dos block entities de um chunk que sai contra o original (`transmitter_events_chunks_mixed.jsonl`, `TransmitterEventsTraceTest.chunks_mixed`): dois chunks com cabos e
//! baús postos fora de ordem, três tirados e dois postos de novo no segundo, e cada chunk descarregado. O `onChunkUnloaded` de cada cabo sai na ordem do `Object2ObjectOpenHashMap`
//! do chunk, que guarda todos os block entities (os baús também), e não na ordem dos cabos sozinhos.
#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}

use events::{Event, Kind, Raw, Tracker};
use machinery_basis_core::sim::block_entity_order::BlockEntityOrder;
use machinery_basis_core::sim::chunks::BlockPos;
use serde_json::Value;

const OW: &str = "minecraft:overworld";

fn rows() -> Vec<Value> {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/transmitter_events_chunks_mixed.jsonl");
    std::fs::read_to_string(file)
        .expect("o rastro")
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha do rastro"))
        .collect()
}

fn int(row: &Value, key: &str) -> i32 {
    row[key].as_i64().unwrap_or_else(|| panic!("{key}")) as i32
}

struct Scene {
    origin: BlockPos,
    border: i32,
}

impl Scene {
    fn read(rows: &[Value]) -> Self {
        let origin = rows.iter().find(|r| r["e"] == "origin").unwrap();
        let border = rows
            .iter()
            .filter(|r| r["e"] == "mark")
            .find_map(|r| r["m"].as_str()?.strip_prefix("border x ")?.parse().ok())
            .unwrap();
        Self {
            origin: BlockPos::new(int(origin, "x"), int(origin, "y"), int(origin, "z")),
            border,
        }
    }

    fn at(&self, x: i32, z: i32) -> BlockPos {
        BlockPos::new(self.origin.x + x, self.origin.y + 1, self.origin.z + z)
    }

    /// `mixed(helper, level, fromX, count, step, chestEvery)` do Java: as posições na ordem em que são postas, e se cada uma é um baú.
    fn mixed(&self, from_x: i32, count: i32, step: i32, chest_every: i32) -> Vec<(BlockPos, bool)> {
        (0..count)
            .map(|i| {
                let k = (i * step) % 48;
                (
                    self.at(from_x + k % 16, k / 16),
                    i % chest_every == chest_every - 1,
                )
            })
            .collect()
    }

    fn rel(&self, pos: BlockPos) -> [i32; 3] {
        [
            pos.x - self.origin.x,
            pos.y - self.origin.y,
            pos.z - self.origin.z,
        ]
    }
}

/// O que o original chamou: o `onChunkUnloaded` de cada cabo depois da marca `after`, na ordem.
fn unloaded_after(rows: &[Value], after: &str) -> Vec<[i32; 3]> {
    let mut on = false;
    let mut out = Vec::new();
    for row in rows {
        if row["e"] == "mark" {
            on = row["m"] == after;
        } else if on && row["e"] == "onChunkUnloaded" {
            let p = &row["p"];
            out.push([int_at(p, 0), int_at(p, 1), int_at(p, 2)]);
        }
    }
    out
}

fn int_at(array: &Value, i: usize) -> i32 {
    array[i].as_i64().unwrap() as i32
}

fn chunk_of(pos: BlockPos) -> (i32, i32) {
    (pos.x >> 4, pos.z >> 4)
}

/// O que o jogo faz com os dois chunks do cenário, em ordem; devolve o mapa e as posições de cada tipo.
fn play(scene: &Scene, only_cables: bool) -> (BlockEntityOrder, Vec<BlockPos>, Vec<BlockPos>) {
    let first = scene.mixed(scene.border, 14, 7, 4);
    let second = scene.mixed(scene.border + 16, 36, 7, 5);
    let mut order = BlockEntityOrder::new();
    let mut cables_a = Vec::new();
    let mut cables_b = Vec::new();
    let put = |order: &mut BlockEntityOrder, pos: BlockPos, chest: bool| {
        if !(only_cables && chest) {
            order.put(OW, pos);
        }
    };
    for (pos, chest) in &first {
        put(&mut order, *pos, *chest);
        if !chest {
            cables_a.push(*pos);
        }
    }
    for (pos, chest) in &second {
        put(&mut order, *pos, *chest);
        if !chest {
            cables_b.push(*pos);
        }
    }
    // Tirar os de k = 14, 21, 35 e pôr os de k = 35, 14 (cabos) de novo.
    let at = |k: i32| scene.at(scene.border + 16 + k % 16, k / 16);
    for k in [14, 21, 35] {
        order.remove(OW, at(k));
        cables_b.retain(|pos| *pos != at(k));
    }
    for k in [35, 14] {
        order.put(OW, at(k));
        cables_b.push(at(k));
    }
    (order, cables_a, cables_b)
}

fn present(
    scene: &Scene,
    from_x: i32,
    count: i32,
    step: i32,
    chest_every: i32,
    removed: &[i32],
    back: &[i32],
) -> Vec<BlockPos> {
    let mut list: Vec<BlockPos> = scene
        .mixed(from_x, count, step, chest_every)
        .into_iter()
        .map(|(pos, _)| pos)
        .collect();
    let at = |k: i32| scene.at(from_x + k % 16, k / 16);
    list.retain(|pos| !removed.iter().any(|k| at(*k) == *pos));
    for k in back {
        if !list.contains(&at(*k)) {
            list.push(at(*k));
        }
    }
    list
}

fn only(list: Vec<BlockPos>, keep: &[BlockPos]) -> Vec<BlockPos> {
    list.into_iter().filter(|pos| keep.contains(pos)).collect()
}

#[test]
fn the_chunk_that_leaves_visits_its_cables_in_the_order_of_the_map_of_all_its_block_entities() {
    let rows = rows();
    let scene = Scene::read(&rows);
    let (mut order, cables_a, cables_b) = play(&scene, false);
    let first = present(&scene, scene.border, 14, 7, 4, &[], &[]);
    let second = present(
        &scene,
        scene.border + 16,
        36,
        7,
        5,
        &[14, 21, 35],
        &[35, 14],
    );
    let java_a = unloaded_after(&rows, "unload the first chunk");
    let java_b = unloaded_after(&rows, "unload the second chunk");
    assert_eq!(java_a.len(), 11);
    assert_eq!(java_b.len(), 28);
    let got_a = only(order.unload(OW, chunk_of(first[0]), &first), &cables_a);
    let got_b = only(order.unload(OW, chunk_of(second[0]), &second), &cables_b);
    assert_eq!(
        got_a.iter().map(|p| scene.rel(*p)).collect::<Vec<_>>(),
        java_a
    );
    assert_eq!(
        got_b.iter().map(|p| scene.rel(*p)).collect::<Vec<_>>(),
        java_b
    );
}

/// A mutação: contar só os cabos (o que o port fazia, com os de mod) erra o segundo chunk, que tem 36 block entities e a tabela do original cresce com os baús.
#[test]
fn counting_only_the_cables_gets_the_second_chunk_wrong() {
    let rows = rows();
    let scene = Scene::read(&rows);
    let (mut order, _, cables_b) = play(&scene, true);
    let second = present(
        &scene,
        scene.border + 16,
        36,
        7,
        5,
        &[14, 21, 35],
        &[35, 14],
    );
    let java_b = unloaded_after(&rows, "unload the second chunk");
    let cables: Vec<BlockPos> = only(second.clone(), &cables_b);
    let got = order.unload(OW, chunk_of(second[0]), &cables);
    assert_ne!(
        got.iter().map(|p| scene.rel(*p)).collect::<Vec<_>>(),
        java_b
    );
}

#[test]
fn a_block_entity_the_map_never_saw_comes_in_by_position_and_one_that_is_gone_leaves() {
    let mut order = BlockEntityOrder::new();
    let a = BlockPos::new(1, 64, 1);
    let b = BlockPos::new(2, 64, 1);
    let c = BlockPos::new(3, 64, 1);
    order.put(OW, a);
    order.put(OW, b);
    // `b` já não existe no mundo, `c` é novo.
    let got = order.unload(OW, (0, 0), &[a, c]);
    assert_eq!(got.len(), 2);
    assert!(got.contains(&a) && got.contains(&c));
    // O mapa do chunk que saiu não fica: o chunk que volta começa de novo.
    assert!(order.unload(OW, (0, 0), &[]).is_empty());
}

/// O `setRemoved` de cada cabo depois da marca `after`, na ordem.
fn removed_after(rows: &[Value], after: &str) -> Vec<[i32; 3]> {
    let mut on = false;
    let mut out = Vec::new();
    for row in rows {
        if row["e"] == "mark" {
            on = row["m"] == after;
        } else if on && row["e"] == "setRemoved" {
            let p = &row["p"];
            out.push([int_at(p, 0), int_at(p, 1), int_at(p, 2)]);
        }
    }
    out
}

/// O chunk que sai pelo `Tracker` do jogo, com os block entities na ordem do mapa de todos eles: o `onChunkUnloaded` e o `setRemoved` de cada cabo saem na ordem do original, os baús
/// (que não são transmissores) entre eles só invalidam capabilities.
#[test]
fn the_tracker_unloads_the_chunk_in_the_order_of_the_original_with_the_chests_in_it() {
    let rows = rows();
    let scene = Scene::read(&rows);
    let (mut order, _, _) = play(&scene, false);
    let second = present(
        &scene,
        scene.border + 16,
        36,
        7,
        5,
        &[14, 21, 35],
        &[35, 14],
    );
    let mut tracker = Tracker::with_ids(&["cable"]);
    // Os block entities do chunk entram como o jogo os anota: cabos e baús, na ordem em que o cenário os põe.
    let chests: Vec<BlockPos> = scene
        .mixed(scene.border + 16, 36, 7, 5)
        .into_iter()
        .filter(|(_, chest)| *chest)
        .map(|(pos, _)| pos)
        .collect();
    let mut out = Vec::new();
    for pos in &second {
        let id = if chests.contains(pos) {
            None
        } else {
            Some("cable".to_owned())
        };
        tracker.apply(
            OW,
            Raw::Added {
                pos: [pos.x, pos.y, pos.z],
                id,
            },
            &mut out,
        );
    }
    let chunk = chunk_of(second[0]);
    let blocks: Vec<([i32; 3], Option<String>)> = order
        .unload(OW, chunk, &second)
        .into_iter()
        .map(|pos| {
            let id = if chests.contains(&pos) {
                None
            } else {
                Some("cable".to_owned())
            };
            ([pos.x, pos.y, pos.z], id)
        })
        .collect();
    let mut events = Vec::new();
    tracker.chunk_unloading(OW, [chunk.0, chunk.1], &blocks, &mut events);
    let rel = |pos: [i32; 3]| scene.rel(BlockPos::new(pos[0], pos[1], pos[2]));
    let unloaded: Vec<[i32; 3]> = events
        .iter()
        .filter_map(|event| match event {
            Event::Transmitter {
                pos,
                kind: Kind::ChunkUnloaded,
                ..
            } => Some(rel(*pos)),
            _ => None,
        })
        .collect();
    let removed: Vec<[i32; 3]> = events
        .iter()
        .filter_map(|event| match event {
            Event::Transmitter {
                pos,
                kind: Kind::Removed,
                ..
            } => Some(rel(*pos)),
            _ => None,
        })
        .collect();
    assert_eq!(unloaded, unloaded_after(&rows, "unload the second chunk"));
    assert_eq!(removed, removed_after(&rows, "unload the second chunk"));
    assert!(tracker.simple_is_empty(OW));
}
