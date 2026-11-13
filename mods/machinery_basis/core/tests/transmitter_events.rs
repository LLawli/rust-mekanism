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
use std::sync::Mutex;

/// Os testes que usam a fila global (`take`, `push_from`) não andam juntos.
static GLOBAL: Mutex<()> = Mutex::new(());

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
    let _only = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    events::register("rewrite");
    let before = events::take();
    assert!(before.is_empty() || events::is_active());
    // `note_insert` com o mesmo tipo no lugar (`/data merge block`): nada para a rede.
    events::note_insert(0, [1, 1, 1], Some("rewrite"), Some(Some("rewrite")));
    assert!(events::take().is_empty());
    // De outro tipo (o bloco trocado, `LevelChunk.setBlockState`): o velho sai e o novo entra, nessa ordem; o rastreio por posição não perde o novo.
    events::note_insert(0, [1, 1, 1], Some("rewrite"), Some(None));
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

/// `ServerStoppedEvent`: um `clearRemoved` que esperava o block entity quando o mundo fechou não vale no mundo seguinte. O relógio do mundo novo recomeça perto de 0, e a espera, medida
/// contra ele (`now - since < PENDING_TICKS`), nunca acabaria: o `clearRemoved` velho ficava na fila e entrava no mundo novo com o primeiro block entity que aparecesse na posição.
#[test]
fn a_world_that_closes_drops_the_clears_that_were_still_waiting() {
    let at = [7, 64, 7];
    let mut pending = events::PendingClears::new();
    pending.wait("overworld", at, 90_000);
    // O mundo novo, com o relógio perto de 0: a entrada velha segue esperando para sempre.
    assert!(pending.retry("overworld", 10, |_| false).is_empty());
    assert_eq!(
        pending.len(),
        1,
        "sem o `clear`, a espera do mundo velho fica"
    );
    pending.clear();
    assert!(
        pending.is_empty(),
        "o mundo que fechou esquece o que esperava"
    );
    // E o block entity que aparece depois na mesma posição não recebe um `clearRemoved` do mundo velho.
    assert!(pending.retry("overworld", 20, |_| true).is_empty());
}

/// O espelho do cliente da janela do anfitrião (outro `BlockEntities`, que recebe os blocos do servidor e passa pelas mesmas injeções) não anota nada depois de o do servidor se
/// apresentar: o bloco trocado pela liga voltava um tick depois como `setRemoved` e `clearRemoved` do cliente, e o dono dos transportadores largava as pilhas do que ele acabara de
/// receber. Antes de o servidor se apresentar (os chunks do começo do mundo) qualquer um vale.
#[test]
fn the_client_mirror_of_the_window_notes_nothing_once_the_server_block_entities_are_known() {
    let _only = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    events::register("rewrite");
    let _ = events::take();
    const SERVER: usize = 0x7fb3_d517_41d0;
    const CLIENT: usize = 0x7fff_a19c_64d0;
    events::reset();
    events::push_from(CLIENT, Raw::AddedLater { pos: [1, 1, 1] });
    assert_eq!(
        events::take().len(),
        1,
        "sem servidor conhecido, qualquer um vale"
    );
    events::set_server_source(SERVER);
    events::note_insert(CLIENT, [2, 1, 1], Some("rewrite"), Some(None));
    events::push_from(
        CLIENT,
        Raw::Removed {
            pos: [3, 1, 1],
            id: None,
        },
    );
    assert!(events::take().is_empty(), "o espelho do cliente não entra");
    events::note_insert(SERVER, [2, 1, 1], Some("rewrite"), None);
    events::push_from(
        SERVER,
        Raw::Removed {
            pos: [3, 1, 1],
            id: None,
        },
    );
    assert_eq!(events::take().len(), 2, "o do servidor entra");
    // O mundo fechou: o próximo servidor é outro objeto.
    let _ = events::reset();
    events::push_from(CLIENT, Raw::AddedLater { pos: [4, 1, 1] });
    assert_eq!(events::take().len(), 1);
}

fn ticket(chunk: [i32; 2], loaded: bool) -> Event {
    Event::Ticket {
        dim: OW.to_owned(),
        chunk,
        loaded,
    }
}

#[test]
fn a_chunk_that_leaves_the_accessible_ring_tells_the_ticket_while_it_stays_loaded() {
    // O jogador se afasta: o chunk (0, 0) fica carregado (o nível 33 do original), mas o ticket passa de 32 e os cabos dele saem da rede (decisão 196, medido: 47 cabos de diferença).
    let mut tracker = Tracker::with_ids(&["cable"]);
    let mut out = Vec::new();
    tracker.chunk_loaded_with(OW, [0, 0], true, &mut out);
    tracker.chunk_loaded_with(OW, [1, 0], true, &mut out);
    assert_eq!(out, vec![ticket([0, 0], true), ticket([1, 0], true)]);
    // Nada mudou: nenhum ticket.
    let mut out = Vec::new();
    tracker.accessibility(OW, |_| true, &mut out);
    assert!(
        out.is_empty(),
        "sem mudança de nível não há ticket: {out:?}"
    );
    // O (0, 0) sai do anel, o (1, 0) fica.
    tracker.accessibility(OW, |chunk| chunk[0] != 0, &mut out);
    assert_eq!(out, vec![ticket([0, 0], false)]);
    // Na chamada seguinte o estado já é o novo: nada se repete.
    let mut out = Vec::new();
    tracker.accessibility(OW, |chunk| chunk[0] != 0, &mut out);
    assert!(out.is_empty(), "o ticket não se repete: {out:?}");
    // E volta ao anel.
    tracker.accessibility(OW, |_| true, &mut out);
    assert_eq!(out, vec![ticket([0, 0], true)]);
}

#[test]
fn the_chunks_that_change_side_come_in_position_order_and_only_for_the_dimension() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    let mut out = Vec::new();
    for chunk in [[3, 0], [-1, 2], [0, 0], [3, -4]] {
        tracker.chunk_loaded_with(OW, chunk, true, &mut out);
    }
    tracker.chunk_loaded_with("minecraft:the_nether", [9, 9], true, &mut out);
    let mut out = Vec::new();
    tracker.accessibility(OW, |_| false, &mut out);
    assert_eq!(
        out,
        vec![
            ticket([-1, 2], false),
            ticket([0, 0], false),
            ticket([3, -4], false),
            ticket([3, 0], false)
        ]
    );
}

#[test]
fn a_chunk_that_loads_outside_the_ring_gives_the_ticket_only_when_it_enters() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    let mut out = Vec::new();
    tracker.chunk_loaded_with(OW, [5, 0], false, &mut out);
    assert!(
        out.is_empty(),
        "o chunk que carrega no nível 33 não dá ticket: {out:?}"
    );
    tracker.accessibility(OW, |_| true, &mut out);
    assert_eq!(out, vec![ticket([5, 0], true)]);
}

#[test]
fn a_chunk_that_already_left_the_ring_does_not_tell_the_ticket_again_when_it_unloads() {
    let mut tracker = Tracker::with_ids(&["cable"]);
    let mut out = Vec::new();
    tracker.chunk_loaded_with(OW, [0, 0], true, &mut out);
    tracker.accessibility(OW, |_| false, &mut out);
    let mut out = Vec::new();
    tracker.chunk_unloading(OW, [0, 0], &[], &mut out);
    assert!(
        out.is_empty(),
        "o nível já passou de 32 quando o chunk sai: {out:?}"
    );
    // Um chunk sem registro (o do começo do mundo) segue dando o ticket de sempre.
    let mut out = Vec::new();
    tracker.chunk_unloading(OW, [7, 7], &[], &mut out);
    assert_eq!(out, vec![ticket([7, 7], false)]);
}

#[test]
fn a_reset_forgets_which_side_of_the_ring_each_chunk_was() {
    let _only = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    events::register("ring");
    let _ = events::chunk_loaded_with(OW, [0, 0], false);
    assert!(events::accessibility(OW, |_| true).len() == 1);
    let _ = events::reset();
    assert!(
        events::accessibility(OW, |_| false).is_empty(),
        "depois do reset nenhum chunk é seguido"
    );
}

#[test]
fn the_client_mirror_does_not_drain_the_queue_the_server_filled() {
    // Decisão 198, medido em jogo: o tick e o fim de cada operação do espelho do cliente da janela esvaziavam a fila global, e levavam o `load_chunk` que o servidor acabara de anotar.
    // O chunk sem `clearRemoved` deixava os cabos dele fora da rede (uma rede partida em cerca de metade das primeiras aberturas de um mundo recém-construído).
    let _only = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    let _ = events::reset();
    events::register("cable");
    let blocks = vec![([33, 1, 1], Some("cable".to_owned()))];
    events::push(Raw::LoadChunk(blocks.clone()));
    assert!(
        events::take_for(false).is_empty(),
        "o espelho do cliente não é dono da fila"
    );
    assert_eq!(
        events::take_for(true),
        vec![Raw::LoadChunk(blocks)],
        "o que o servidor anotou segue na fila para quem o entrega"
    );
}
