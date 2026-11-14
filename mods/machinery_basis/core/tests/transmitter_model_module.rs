//! O módulo `transmitter-model` que entra em `crates/horizon-client-render/src/models/catalog.rs` (`mods/machinery_basis/transmitter_model.rs`): o `TransmitterModelData` do original (o
//! estado visual de cada lado, `getVisualStatus`) e as partes que o `TransmitterBlockStateModel.collectParts` junta. O arquivo é código só de `std`, então o teste o compila como está e o
//! confere contra o original: `rastros/transmitter_model_visual.jsonl` (`testes/TransmitterModelTraceTest.java`) tem, para as 4096 combinações dos quatro `ConnectionType` nos seis lados, o
//! que `setConnectionData` guarda no dado comum e no do desviador.
#[path = "../../transmitter_model.rs"]
mod model;

use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::client::{
    ClientKind, ClientTransmitters, Op, TagMessage, decode_batch, encode_batches,
};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::transmitter::{Compound, Tag, keys};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

fn rastros() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros")
}

fn digits(text: &str) -> [u8; 6] {
    let list: Vec<u8> = text.bytes().map(|digit| digit - b'0').collect();
    list.try_into().unwrap()
}

#[test]
fn the_visual_status_of_every_connection_combination_is_the_one_of_the_original() {
    let text = std::fs::read_to_string(rastros().join("transmitter_model_visual.jsonl")).unwrap();
    let mut counts = [0usize; 6];
    let mut lines = 0;
    for line in text.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let connections = digits(row["c"].as_str().unwrap());
        let want = digits(row["v"].as_str().unwrap());
        let diversion = row["d"].as_u64().unwrap() == 1;
        assert_eq!(
            model::visual_statuses(connections, diversion),
            want,
            "{line}"
        );
        for status in want {
            counts[usize::from(status)] += 1;
        }
        lines += 1;
    }
    // 4^6 combinações, no dado comum e no do desviador; e todo estado visual aparece (o teste pode falhar).
    assert_eq!(lines, 2 * 4096);
    assert!(counts.iter().all(|count| *count > 0), "{counts:?}");
    // O desviador não tem trecho reto.
    let straight = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["d"] == 1)
        .any(|row| row["v"].as_str().unwrap().bytes().any(|d| d >= b'0' + 4));
    assert!(!straight);
}

#[test]
fn the_straight_run_hides_the_joint_only_with_one_pair_of_opposite_arms() {
    use model::{NONE, NONE_CONTIGUOUS, NONE_CONTIGUOUS_ROTATED, NORMAL};
    const N: u8 = model::CONNECTION_NONE;
    // Norte e sul ligados: o centro de baixo e de cima segue a linha; o de oeste e leste a vê de lado (girado).
    assert_eq!(
        model::visual_statuses([N, N, 0, 0, N, N], false),
        [
            NONE_CONTIGUOUS,
            NONE_CONTIGUOUS,
            NORMAL,
            NORMAL,
            NONE_CONTIGUOUS_ROTATED,
            NONE_CONTIGUOUS_ROTATED
        ]
    );
    // Um braço só: o centro aparece inteiro.
    assert_eq!(
        model::visual_statuses([N, N, 0, N, N, N], false),
        [NONE, NONE, NORMAL, NONE, NONE, NONE]
    );
    // O centro sozinho.
    assert_eq!(model::visual_statuses([N; 6], false), [NONE; 6]);
    // Um canto (norte e leste): nenhum par de lados opostos.
    assert_eq!(
        model::visual_statuses([N, N, 0, N, N, 0], false),
        [NONE, NONE, NORMAL, NONE, NONE, NORMAL]
    );
    // O desviador mostra o centro inteiro mesmo na linha reta.
    assert_eq!(
        model::visual_statuses([N, N, 0, 0, N, N], true),
        [NONE, NONE, NORMAL, NORMAL, NONE, NONE]
    );
}

/// Um lado ligado vale o tipo dele (`VisualConnectionStatus.from`), `PUSH` e `PULL` inclusive, e não conta como lado sem conexão para os trechos retos.
#[test]
fn a_connected_side_keeps_its_own_type_and_counts_as_an_arm_for_the_straight_runs() {
    use model::{NONE, NONE_CONTIGUOUS, NORMAL, PULL, PUSH};
    const N: u8 = model::CONNECTION_NONE;
    // Em cima e embaixo ligados, o sul sem conexão é trecho reto; o oeste e o leste, com um braço do outro par, mostram o centro inteiro.
    assert_eq!(
        model::visual_statuses([PUSH, PULL, NORMAL, N, N, N], false),
        [PUSH, PULL, NORMAL, NONE_CONTIGUOUS, NONE, NONE]
    );
    // Um braço `PUSH` e um `PULL` em lados opostos também fazem o trecho reto.
    assert_eq!(
        model::visual_statuses([N, N, PUSH, PULL, N, N], false)[..2],
        [NONE_CONTIGUOUS, NONE_CONTIGUOUS]
    );
}

#[test]
fn the_names_of_the_parts_follow_the_obj_groups_of_the_original() {
    let parts: Value = serde_json::from_str(
        &std::fs::read_to_string(rastros().join("transmitter_model_parts.json")).unwrap(),
    )
    .unwrap();
    // Os nomes dos grupos do OBJ (`ALL_PART_GROUPS`) são o lado e o `partName()` de cada estado.
    let mut groups: Vec<String> = Vec::new();
    for name in model::SIDES {
        for entry in parts["statuses"].as_array().unwrap() {
            groups.push(format!("{name}{}", entry["part"].as_str().unwrap()));
        }
    }
    groups.sort();
    groups.dedup();
    let want: Vec<&str> = parts["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| group.as_str().unwrap())
        .collect();
    assert_eq!(groups, want);
    assert_eq!(
        parts["statuses"].as_array().unwrap().len(),
        model::STATUS_NAMES.len()
    );
    for (index, entry) in parts["statuses"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            entry["name"].as_str().unwrap().to_lowercase(),
            model::STATUS_NAMES[index]
        );
    }
}

/// O índice de cada parte no `multipart` do blockstate: lado, depois estado visual; o desviador não tem as duas partes do trecho reto.
#[test]
fn the_selection_indexes_the_multipart_of_the_blockstate_side_by_side() {
    use model::{NONE, NONE_CONTIGUOUS, NORMAL, PULL, PUSH};
    assert_eq!(model::parts_per_side(false), 6);
    assert_eq!(model::parts_per_side(true), 4);
    assert_eq!(model::selection([NONE; 6], false, None), [3, 9, 15, 21, 27, 33]);
    assert_eq!(
        model::selection([NORMAL, PUSH, PULL, NONE, NONE_CONTIGUOUS, NORMAL], false, None),
        [0, 7, 14, 21, 28, 30]
    );
    assert_eq!(model::selection([NONE; 6], true, None), [3, 7, 11, 15, 19, 23]);
    assert_eq!(
        model::selection([NORMAL, PUSH, PULL, NONE, NORMAL, PUSH], true, None),
        [0, 5, 10, 15, 16, 21]
    );
    // Com o vidro, cada lado leva o corpo e depois o vidro da cor (as partes do vidro de cada cor vêm depois das 36 do corpo, de cor em cor, na mesma ordem).
    assert_eq!(
        model::selection([NORMAL, PUSH, PULL, NONE, NONE_CONTIGUOUS, NORMAL], false, Some(0)),
        [0, 36, 7, 43, 14, 50, 21, 57, 28, 64, 30, 66]
    );
    assert_eq!(
        model::selection([NONE; 6], false, Some(17)),
        [3, 36 + 17 * 36 + 3, 9, 36 + 17 * 36 + 9, 15, 36 + 17 * 36 + 15, 21, 36 + 17 * 36 + 21, 27, 36 + 17 * 36 + 27, 33, 36 + 17 * 36 + 33]
    );
    assert_eq!(model::GLASS_COLORS, 18);
}

fn data(connections: [u8; 6]) -> model::ModelData {
    model::ModelData { connections, color: None }
}

/// O módulo guarda estado global: os testes que o usam andam um de cada vez.
static GLOBAL: Mutex<()> = Mutex::new(());

#[test]
fn the_parts_of_a_transmitter_are_asked_by_state_and_position_of_the_block_the_mesher_draws() {
    let _guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    model::clear_data();
    // Dois transmissores (o desviador é a família 1), no estado 7 e 9; o 8 não é de transmissor.
    model::set_families(vec![false, true], vec![false, false], vec![(7, 0), (9, 1)]);
    let make = |choice: usize, parts: Vec<usize>| (choice, parts);
    // Sem bloco anotado (outra thread, ou o catálogo chamado fora do mesher) não há o que escolher.
    assert!(
        std::thread::spawn(|| model::select(7, Some(4), |choice, parts| (choice, parts)).is_none())
            .join()
            .unwrap()
    );
    // O mesher desenha a seção de origem (16, 0, 32) e o bloco local (1, 2, 3): a posição é (17, 2, 35).
    model::note_section([16, 0, 32]);
    model::note_block(1, 2, 3);
    // Sem o dado da block entity: o modelo do blockstate segue.
    assert!(model::select(7, Some(4), make).is_none());
    const N: u8 = model::CONNECTION_NONE;
    model::set_data([17, 2, 35], Some(data([N, N, 0, 1, N, N])));
    // Um estado que não é de transmissor, ou sem escolha de blockstate, também deixa o blockstate.
    assert!(model::select(8, Some(4), make).is_none());
    assert!(model::select(7, None, make).is_none());
    // Norte e sul ligados (`NORMAL` e `PUSH`), o resto sem conexão: o trecho reto, e as partes são as dos índices do `multipart`.
    let cable = model::select(7, Some(4), make).unwrap();
    assert_eq!(*cable, (4, vec![4, 10, 12, 19, 29, 35]));
    // O desviador dá o centro inteiro (`NONE`) onde o cabo dá o trecho reto, com quatro partes por lado.
    let diversion = model::select(9, Some(2), make).unwrap();
    assert_eq!(*diversion, (2, vec![3, 7, 8, 13, 19, 23]));
    // Cada conjunto de partes é montado uma vez e fica: o mesmo pedido devolve o mesmo endereço.
    assert!(std::ptr::eq(
        cable,
        model::select(7, Some(4), make).unwrap()
    ));
    // Outro bloco da seção, sem dado, deixa o blockstate; e o dado de outra posição não vale.
    model::note_block(1, 2, 4);
    assert!(model::select(7, Some(4), make).is_none());
    // O dado sai com a block entity.
    model::note_block(1, 2, 3);
    assert!(model::set_data([17, 2, 35], None));
    assert!(!model::set_data([17, 2, 35], None));
    assert!(model::select(7, Some(4), make).is_none());
    assert_eq!(model::data_count(), 0);
}

/// O vidro do transportador logístico pintado (`TransmitterBlockStateModel.collectParts`): com cor as partes têm o vidro da cor de cada lado, sem cor ou em outro bloco, não.
#[test]
fn a_painted_transporter_adds_the_glass_parts_of_its_color() {
    let _guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    model::clear_data();
    // A família 0 é um transportador (com vidro), a 1 o desviador (sem).
    model::set_families(vec![false, true], vec![true, false], vec![(7, 0), (9, 1)]);
    let make = |choice: usize, parts: Vec<usize>| (choice, parts);
    model::note_section([0, 0, 0]);
    model::note_block(1, 1, 1);
    const N: u8 = model::CONNECTION_NONE;
    // Sem cor: o corpo só.
    model::set_data([1, 1, 1], Some(data([N; 6])));
    assert_eq!(*model::select(7, Some(0), make).unwrap(), (0, vec![3, 9, 15, 21, 27, 33]));
    // Pintado de vermelho (`EnumColor.RED`, 12): o corpo e o vidro da cor por lado.
    model::set_data([1, 1, 1], Some(model::ModelData { connections: [N; 6], color: Some(12) }));
    let base = 36 + 12 * 36;
    assert_eq!(
        *model::select(7, Some(0), make).unwrap(),
        (0, vec![3, base + 3, 9, base + 9, 15, base + 15, 21, base + 21, 27, base + 27, 33, base + 33])
    );
    // Outra cor é outro conjunto de partes.
    model::set_data([1, 1, 1], Some(model::ModelData { connections: [N; 6], color: Some(1) }));
    assert_eq!(model::select(7, Some(0), make).unwrap().1[1], 36 + 36 + 3);
    // O desviador não tem vidro, mesmo com cor no dado: só o corpo.
    assert_eq!(*model::select(9, Some(0), make).unwrap(), (0, vec![3, 7, 11, 15, 19, 23]));
}

#[test]
fn set_data_tells_whether_the_connections_changed_and_clear_forgets_them_all() {
    let _guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    model::clear_data();
    assert!(model::set_data([0, 0, 0], Some(data([0, 1, 2, 3, 0, 1]))));
    assert!(!model::set_data([0, 0, 0], Some(data([0, 1, 2, 3, 0, 1]))));
    assert!(model::set_data([0, 0, 0], Some(data([0, 1, 2, 3, 0, 2]))));
    assert!(model::set_data([5, 0, 0], Some(data([3; 6]))));
    assert_eq!(model::data_at([0, 0, 0]), Some(data([0, 1, 2, 3, 0, 2])));
    // Mudar só a cor muda o dado (`refreshModelData`).
    assert!(model::set_data([0, 0, 0], Some(model::ModelData { connections: [0, 1, 2, 3, 0, 2], color: Some(4) })));
    assert_eq!(model::data_count(), 2);
    model::clear_data();
    assert_eq!(model::data_count(), 0);
    assert_eq!(model::data_at([0, 0, 0]), None);
}

fn tag(raw: [i32; 6], transmitter_connections: i8, acceptors: i8) -> Compound {
    let mut tag = Compound::new();
    tag.insert(
        keys::CURRENT_CONNECTIONS.into(),
        Tag::Byte(transmitter_connections),
    );
    tag.insert(keys::CURRENT_ACCEPTORS.into(), Tag::Byte(acceptors));
    tag.insert(keys::CONNECTION.into(), Tag::IntArray(raw.to_vec()));
    tag
}

fn message(pos: BlockPos, raw: [i32; 6], transmitter_connections: i8, acceptors: i8) -> Op {
    Op::Tag(TagMessage {
        dimension: "minecraft:overworld".to_owned(),
        pos,
        kind: ClientKind::Energy,
        capacity: 8_000,
        tag: tag(raw, transmitter_connections, acceptors),
    })
}

/// O cliente do convidado: o pacote de atualização leva `PUSH` e `PULL` pelo fio, o tile novo e a mudança de conexão pedem o dado do modelo de novo, e o pacote que não muda
/// nada não pede.
#[test]
fn the_client_asks_for_the_model_data_when_a_tile_comes_changes_or_leaves() {
    let mut client = ClientTransmitters::new();
    let here = BlockPos::new(3, 64, 4);
    let there = BlockPos::new(30, 70, -4);
    // Os tipos crus: DOWN normal, UP push, NORTH pull, o resto sem conexão (`ConnectionType.ordinal`); os braços de aceitador em DOWN, UP e NORTH.
    let raw = [0, 1, 2, 3, 3, 3];
    let acceptors = 0b0000_0111;
    let sent = vec![
        message(here, raw, 0, acceptors),
        message(there, [3; 6], 0, 0),
    ];
    // Pelo fio, como o convidado recebe.
    let received: Vec<Op> = encode_batches(&sent)
        .iter()
        .flat_map(|batch| decode_batch(batch).unwrap())
        .collect();
    assert_eq!(received.len(), 2);
    for op in &received {
        client.apply(op);
    }
    let view = client.view_at(here).unwrap();
    assert_eq!(
        view.kind,
        [
            ConnectionType::Normal,
            ConnectionType::Push,
            ConnectionType::Pull,
            ConnectionType::None,
            ConnectionType::None,
            ConnectionType::None
        ]
    );
    assert_eq!(client.take_model_changes(), vec![here, there]);
    assert!(client.take_model_changes().is_empty());
    // O mesmo pacote de novo não muda as conexões.
    client.apply(&received[0]);
    assert!(client.take_model_changes().is_empty());
    // O Configurator troca o braço do norte de `PULL` para `NONE` cru: o dado muda.
    client.apply(&message(here, [0, 1, 3, 3, 3, 3], 0, 0b0000_0011));
    assert_eq!(client.take_model_changes(), vec![here]);
    // O tile sai: o dado sai.
    client.apply(&Op::Gone {
        dimension: "minecraft:overworld".to_owned(),
        pos: there,
    });
    assert_eq!(client.take_model_changes(), vec![there]);
    assert!(client.view_at(there).is_none());
    // O chunk some do cliente: os tiles dele saem.
    client.apply(&Op::Forget {
        dimension: "minecraft:overworld".to_owned(),
        chunks: None,
    });
    assert_eq!(client.take_model_changes(), vec![here]);
    assert!(client.tile_positions().is_empty());
}
