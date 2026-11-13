//! Os eventos do jogo que a rede de transmissores recebe (`transmitter_events.rs` da `machinery_basis`, compilado como está), na ordem do original
//! (`dev/referencia/rastros/transmitter_events_*.jsonl`, `TransmitterEventsTraceTest.java`):
//!
//! - o `clearRemoved` de um cabo vem antes das capabilities que ele invalida nos cabos vizinhos (`BlockEntity.clearRemoved` chama `invalidateCapabilities` dentro da chamada);
//! - uma máquina ao lado só invalida a capability dos cabos vizinhos: não é transmissor, não tem evento próprio;
//! - o chunk que sai: o ticket, a invalidação do chunk inteiro (`ChunkEvent.Unload`), o `onChunkUnloaded` de todos e só depois o `setRemoved` de todos, na ordem do mapa do chunk;
//! - o chunk que volta: o ticket, o `clearRemoved` de cada um na ordem em que o jogo os recebeu e a invalidação do chunk inteiro (`ChunkEvent.Load`);
//! - o tick e o aviso do vizinho só chegam a quem está no chunk (o transmissor que já entrou e ainda não saiu).

#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}

use events::{Event, Kind, Raw, Tracker};

fn cable(dim: &str, pos: [i32; 3], kind: Kind) -> Event {
    Event::Transmitter {
        dim: dim.to_owned(),
        pos,
        kind,
    }
}

fn run(tracker: &mut Tracker, dim: &str, raws: Vec<Raw>) -> Vec<Event> {
    let mut out = Vec::new();
    for raw in raws {
        tracker.apply(dim, raw, &mut out);
    }
    out
}

fn added(pos: [i32; 3], id: &str) -> Raw {
    Raw::Added {
        pos,
        id: Some(id.to_owned()),
    }
}

const OW: &str = "minecraft:overworld";

#[test]
fn cable_placed_next_to_a_cable_tells_the_first_one_its_capability_changed() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    let first = run(&mut tracker, OW, vec![added([3, 1, 2], "cable")]);
    assert_eq!(first, vec![cable(OW, [3, 1, 2], Kind::ClearRemoved)]);
    // O vizinho a oeste do cabo A é o cabo B: do lado de A, B está a oeste (índice 4).
    let second = run(&mut tracker, OW, vec![added([2, 1, 2], "cable")]);
    assert_eq!(
        second,
        vec![
            cable(OW, [2, 1, 2], Kind::ClearRemoved),
            cable(OW, [3, 1, 2], Kind::Capability(4))
        ]
    );
}

#[test]
fn machine_next_to_a_cable_only_invalidates_the_capability() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(&mut tracker, OW, vec![added([3, 1, 2], "cable")]);
    // A máquina a leste do cabo: o cabo a vê no leste (5). Quebrá-la invalida de novo (`setRemoved`).
    let placed = run(&mut tracker, OW, vec![added([4, 1, 2], "machine")]);
    assert_eq!(placed, vec![cable(OW, [3, 1, 2], Kind::Capability(5))]);
    let broken = run(
        &mut tracker,
        OW,
        vec![Raw::Removed {
            pos: [4, 1, 2],
            id: Some("machine".to_owned()),
        }],
    );
    assert_eq!(broken, vec![cable(OW, [3, 1, 2], Kind::Capability(5))]);
}

#[test]
fn removed_cable_leaves_and_stops_hearing() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(
        &mut tracker,
        OW,
        vec![added([3, 1, 2], "cable"), added([2, 1, 2], "cable")],
    );
    let removed = run(
        &mut tracker,
        OW,
        vec![Raw::Removed {
            pos: [2, 1, 2],
            id: Some("cable".to_owned()),
        }],
    );
    assert_eq!(
        removed,
        vec![
            cable(OW, [2, 1, 2], Kind::Removed),
            cable(OW, [3, 1, 2], Kind::Capability(4))
        ]
    );
    let mut out = Vec::new();
    tracker.simple(OW, [2, 1, 2], Kind::Update, &mut out);
    tracker.simple(OW, [3, 1, 2], Kind::Update, &mut out);
    assert_eq!(out, vec![cable(OW, [3, 1, 2], Kind::Update)]);
}

#[test]
fn dimensions_do_not_mix() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(&mut tracker, OW, vec![added([3, 1, 2], "cable")]);
    let nether = run(
        &mut tracker,
        "minecraft:the_nether",
        vec![added([4, 1, 2], "cable")],
    );
    assert_eq!(
        nether,
        vec![cable("minecraft:the_nether", [4, 1, 2], Kind::ClearRemoved)]
    );
}

#[test]
fn chunk_leaving_tells_unloaded_for_all_before_removed_for_all() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    // Três cabos em linha, os dois primeiros no chunk (0, 0) e o terceiro no (1, 0).
    run(
        &mut tracker,
        OW,
        vec![
            added([14, 1, 1], "cable"),
            added([15, 1, 1], "cable"),
            added([16, 1, 1], "cable"),
            added([17, 1, 1], "machine"),
        ],
    );
    // O chunk (1, 0) sai com o cabo e uma máquina: o cabo do chunk vizinho vê o chunk invalidar (a leste dele), e a ordem é a dada.
    let blocks = vec![
        ([17, 1, 1], Some("machine".to_owned())),
        ([16, 1, 1], Some("cable".to_owned())),
    ];
    let mut out = Vec::new();
    tracker.chunk_unloading(OW, [1, 0], &blocks, &mut out);
    assert_eq!(
        out,
        vec![
            Event::Ticket {
                dim: OW.to_owned(),
                chunk: [1, 0],
                loaded: false
            },
            // O chunk inteiro (`ChunkEvent.Unload`): o cabo 15 vigia o leste (16, do chunk 1: sim); o cabo 16 vigia baixo, cima, norte, sul e leste (17), mas não o oeste (15, do chunk 0).
            cable(OW, [15, 1, 1], Kind::Capability(5)),
            cable(OW, [16, 1, 1], Kind::Capability(0)),
            cable(OW, [16, 1, 1], Kind::Capability(1)),
            cable(OW, [16, 1, 1], Kind::Capability(2)),
            cable(OW, [16, 1, 1], Kind::Capability(3)),
            cable(OW, [16, 1, 1], Kind::Capability(5)),
            cable(OW, [16, 1, 1], Kind::ChunkUnloaded),
            // setRemoved da máquina em 17: o cabo em 16 a vê no leste. Depois o cabo em 16: o cabo em 15 o vê no leste.
            cable(OW, [16, 1, 1], Kind::Capability(5)),
            cable(OW, [16, 1, 1], Kind::Removed),
            cable(OW, [15, 1, 1], Kind::Capability(5)),
        ]
    );
}

#[test]
fn chunk_leaving_orders_unloaded_and_removed_like_the_game() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    let line: Vec<[i32; 3]> = (0..5).map(|x| [x, 1, 1]).collect();
    run(
        &mut tracker,
        OW,
        line.iter().map(|pos| added(*pos, "cable")).collect(),
    );
    // A ordem do mapa do chunk não é a do eixo: o rastro do Java tinha 28, 27, 26, ...
    let order = [[3, 1, 1], [1, 1, 1], [4, 1, 1], [0, 1, 1], [2, 1, 1]];
    let blocks: Vec<([i32; 3], Option<String>)> = order
        .iter()
        .map(|pos| (*pos, Some("cable".to_owned())))
        .collect();
    let mut out = Vec::new();
    tracker.chunk_unloading(OW, [0, 0], &blocks, &mut out);
    let names: Vec<(Kind, [i32; 3])> = out
        .iter()
        .filter_map(|event| match event {
            Event::Transmitter { pos, kind, .. }
                if matches!(kind, Kind::ChunkUnloaded | Kind::Removed) =>
            {
                Some((*kind, *pos))
            }
            _ => None,
        })
        .collect();
    let unloaded: Vec<[i32; 3]> = names
        .iter()
        .filter(|(kind, _)| *kind == Kind::ChunkUnloaded)
        .map(|(_, pos)| *pos)
        .collect();
    let removed: Vec<[i32; 3]> = names
        .iter()
        .filter(|(kind, _)| *kind == Kind::Removed)
        .map(|(_, pos)| *pos)
        .collect();
    assert_eq!(unloaded, order);
    assert_eq!(removed, order);
    let last_unloaded = names
        .iter()
        .rposition(|(kind, _)| *kind == Kind::ChunkUnloaded)
        .unwrap();
    let first_removed = names
        .iter()
        .position(|(kind, _)| *kind == Kind::Removed)
        .unwrap();
    assert!(
        last_unloaded < first_removed,
        "todos os onChunkUnloaded antes do primeiro setRemoved"
    );
    assert!(
        tracker.simple_is_empty(OW),
        "o chunk descarregado não deixa transmissor seguido"
    );
}

#[test]
fn chunk_coming_back_tells_ticket_then_added_then_the_chunk() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    // Um cabo no chunk vizinho (0, 0), vigiando o chunk (1, 0).
    run(&mut tracker, OW, vec![added([15, 1, 1], "cable")]);
    run(
        &mut tracker,
        OW,
        vec![Raw::LoadChunk(vec![
            ([16, 1, 1], Some("cable".to_owned())),
            ([17, 1, 1], Some("cable".to_owned())),
        ])],
    );
    let mut out = Vec::new();
    tracker.chunk_loaded(OW, [1, 0], &mut out);
    assert_eq!(
        out,
        vec![
            Event::Ticket {
                dim: OW.to_owned(),
                chunk: [1, 0],
                loaded: true
            },
            cable(OW, [16, 1, 1], Kind::ClearRemoved),
            cable(OW, [15, 1, 1], Kind::Capability(5)),
            cable(OW, [17, 1, 1], Kind::ClearRemoved),
            cable(OW, [16, 1, 1], Kind::Capability(5)),
            // ChunkEvent.Load: o cabo 15 vigia o 16, o 16 vigia baixo, cima, norte, sul e o 17 (o 15 é de outro chunk), e o 17 vigia os seis.
            cable(OW, [15, 1, 1], Kind::Capability(5)),
            cable(OW, [16, 1, 1], Kind::Capability(0)),
            cable(OW, [16, 1, 1], Kind::Capability(1)),
            cable(OW, [16, 1, 1], Kind::Capability(2)),
            cable(OW, [16, 1, 1], Kind::Capability(3)),
            cable(OW, [16, 1, 1], Kind::Capability(5)),
            cable(OW, [17, 1, 1], Kind::Capability(0)),
            cable(OW, [17, 1, 1], Kind::Capability(1)),
            cable(OW, [17, 1, 1], Kind::Capability(2)),
            cable(OW, [17, 1, 1], Kind::Capability(3)),
            cable(OW, [17, 1, 1], Kind::Capability(4)),
            cable(OW, [17, 1, 1], Kind::Capability(5)),
        ]
    );
}

#[test]
fn replaced_block_entity_of_the_same_kind_is_not_a_new_one() {
    events::register("rewrite");
    let before = events::take();
    assert!(before.is_empty() || events::is_active());
    // `note_insert` com o mesmo tipo no lugar (`/data merge block`): nada para a rede.
    events::note_insert([1, 1, 1], Some("rewrite"), Some(Some("rewrite")));
    assert!(events::take().is_empty());
    // De outro tipo (o bloco trocado, `LevelChunk.setBlockState`): o velho sai e o novo entra, nessa ordem; o rastreio por posição não perde o novo.
    events::note_insert([1, 1, 1], Some("rewrite"), Some(None));
    assert_eq!(
        events::take(),
        vec![
            Raw::Removed {
                pos: [1, 1, 1],
                id: None
            },
            Raw::Added {
                pos: [1, 1, 1],
                id: Some("rewrite".to_owned())
            },
        ]
    );
}

#[test]
fn a_cable_swapped_for_another_tier_stays_followed() {
    // O upgrade pela liga troca o bloco do cabo por outro tipo de block entity: o novo continua seguido (recebe o `Update`), como o cabo novo do original.
    let mut tracker = Tracker::with_ids(&["basic", "advanced"]);
    let mut out = Vec::new();
    tracker.apply(
        OW,
        Raw::Added {
            pos: [4, 1, 1],
            id: Some("basic".to_owned()),
        },
        &mut out,
    );
    out.clear();
    tracker.apply(
        OW,
        Raw::Removed {
            pos: [4, 1, 1],
            id: Some("basic".to_owned()),
        },
        &mut out,
    );
    tracker.apply(
        OW,
        Raw::Added {
            pos: [4, 1, 1],
            id: Some("advanced".to_owned()),
        },
        &mut out,
    );
    let mut update = Vec::new();
    tracker.simple(OW, [4, 1, 1], Kind::Update, &mut update);
    assert_eq!(update.len(), 1, "o cabo novo recebe o Update");
    assert!(out.iter().any(|event| matches!(
        event,
        Event::Transmitter {
            kind: Kind::ClearRemoved,
            ..
        }
    )));
}

#[test]
fn same_cable_put_twice_enters_once() {
    // O Horizon passa por `insert` e por `get_or_insert_with` ao pôr o mesmo block entity.
    let mut tracker = Tracker::with_ids(&["cable"]);
    let events = run(
        &mut tracker,
        OW,
        vec![added([3, 1, 2], "cable"), added([3, 1, 2], "cable")],
    );
    assert_eq!(events, vec![cable(OW, [3, 1, 2], Kind::ClearRemoved)]);
}

#[test]
fn neighbor_updates_of_a_placement_come_after_on_added() {
    // O original: `clearRemoved` do cabo B, a capability do cabo A, `onAdded` de B e só então o aviso de A.
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(&mut tracker, OW, vec![added([3, 1, 2], "cable")]);
    let queue = vec![
        Raw::Neighbor {
            dim: OW.to_owned(),
            pos: [3, 1, 2],
        },
        added([2, 1, 2], "cable"),
    ];
    let (others, neighbors) = events::split_neighbors(queue);
    let mut out = run(&mut tracker, OW, others);
    tracker.simple(OW, [2, 1, 2], Kind::PlacedByItem, &mut out);
    out.extend(run(&mut tracker, OW, neighbors));
    assert_eq!(
        out,
        vec![
            cable(OW, [2, 1, 2], Kind::ClearRemoved),
            cable(OW, [3, 1, 2], Kind::Capability(4)),
            cable(OW, [2, 1, 2], Kind::PlacedByItem),
            cable(OW, [3, 1, 2], Kind::Neighbor),
        ]
    );
}

#[test]
fn same_cable_removed_twice_leaves_once() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(
        &mut tracker,
        OW,
        vec![added([3, 1, 2], "cable"), added([2, 1, 2], "cable")],
    );
    let gone = Raw::Removed {
        pos: [2, 1, 2],
        id: Some("cable".to_owned()),
    };
    let events = run(&mut tracker, OW, vec![gone.clone(), gone]);
    assert_eq!(
        events,
        vec![
            cable(OW, [2, 1, 2], Kind::Removed),
            cable(OW, [3, 1, 2], Kind::Capability(4))
        ]
    );
}

#[test]
fn same_machine_put_and_removed_twice_invalidates_once() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    run(&mut tracker, OW, vec![added([3, 1, 2], "cable")]);
    let put = added([4, 1, 2], "machine");
    let events = run(&mut tracker, OW, vec![put.clone(), put]);
    assert_eq!(events, vec![cable(OW, [3, 1, 2], Kind::Capability(5))]);
    let gone = Raw::Removed {
        pos: [4, 1, 2],
        id: Some("machine".to_owned()),
    };
    let events = run(&mut tracker, OW, vec![gone.clone(), gone]);
    assert_eq!(events, vec![cable(OW, [3, 1, 2], Kind::Capability(5))]);
}

// ---------------------------------------------------------------------------------------------------------------------------
// `get_or_insert_with`, o `clearRemoved` que espera o block entity e o fim do tick (as mutações da frente `cabo-ajustes`).

/// `get_or_insert_with` anota só a posição: o tipo vem do que está lá agora, e sem block entity a ocorrência cai (a inserção anota a sua).
#[test]
fn a_later_insert_takes_the_type_of_the_block_entity_that_is_there_and_falls_without_one() {
    let at = |x: i32| [x, 64, 0];
    let raws = vec![
        Raw::AddedLater { pos: at(1) },
        Raw::AddedLater { pos: at(2) },
        Raw::AddedLater { pos: at(3) },
        Raw::Neighbor {
            dim: "overworld".to_owned(),
            pos: at(9),
        },
    ];
    let resolved = events::resolve(raws, |pos| match pos[0] {
        1 => Some(Some("test:cable".to_owned())),
        2 => Some(None),
        _ => None,
    });
    assert_eq!(
        resolved,
        vec![
            Raw::Added {
                pos: at(1),
                id: Some("test:cable".to_owned())
            },
            Raw::Added {
                pos: at(2),
                id: None
            },
            Raw::Neighbor {
                dim: "overworld".to_owned(),
                pos: at(9)
            },
        ]
    );
    // A ocorrência sem block entity que não cai marcaria a posição como ocupada e esconderia o `clearRemoved` da inserção de verdade.
    let mut tracker = Tracker::with_ids(&["test:cable"]);
    let mut out = Vec::new();
    for raw in events::resolve(vec![Raw::AddedLater { pos: at(3) }], |_| None) {
        tracker.apply("overworld", raw, &mut out);
    }
    tracker.apply(
        "overworld",
        Raw::Added {
            pos: at(3),
            id: Some("test:cable".to_owned()),
        },
        &mut out,
    );
    assert!(
        out.iter().any(|event| matches!(event, events::Event::Transmitter { kind: events::Kind::ClearRemoved, pos, .. } if *pos == at(3))),
        "o `clearRemoved` da inserção de verdade sai: {out:?}"
    );
}

#[test]
fn a_clear_removed_waits_for_its_block_entity_and_gives_up_after_the_limit() {
    let at = |x: i32| [x, 64, 0];
    let mut pending = events::PendingClears::new();
    pending.wait("overworld", at(1), 100);
    pending.wait("overworld", at(2), 150);
    pending.wait("nether", at(3), 100);
    assert_eq!(pending.len(), 3);
    // Só o que já tem o block entity sai, na ordem em que esperou; os outros seguem esperando, e a outra dimensão não é tocada.
    let ready = pending.retry("overworld", 110, |pos| pos == at(2) || pos == at(3));
    assert_eq!(ready, vec![at(2)]);
    assert_eq!(pending.len(), 2);
    // Ainda dentro do limite: segue.
    assert!(
        pending
            .retry("overworld", 100 + events::PENDING_TICKS - 1, |_| false)
            .is_empty()
    );
    assert_eq!(pending.len(), 2);
    // No limite desiste (o bloco já não é um cabo).
    assert!(
        pending
            .retry("overworld", 100 + events::PENDING_TICKS, |_| false)
            .is_empty()
    );
    assert_eq!(pending.len(), 1, "só o da outra dimensão resta");
    // O da outra dimensão sai quando for a vez dela.
    assert_eq!(pending.retry("nether", 120, |_| true), vec![at(3)]);
    assert!(pending.is_empty());
}
