//! A conexão de um cabo com o cabo vizinho depois de o mundo ser salvo e reaberto (frente `rede-reabrir`, decisão 235): a ordem em que o port entrega os eventos do chunk que carrega.
//!
//! O original mantém `currentTransmitterConnections` do NBT e, no primeiro `onUpdateServer` de cada cabo, `refreshConnections` lê o que o nível tem em volta (`getTransmitterTile` é o
//! `getBlockEntity` do vizinho: todo block entity do chunk já está lá quando o primeiro tick chega). O port só vê os cabos de que o dono já recebeu o `ClearRemoved`.
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::host_acceptor::{BufferSnapshot, SnapshotAcceptor};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::transmitter::{Compound, Overlay};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::HashMap;
use std::rc::Rc;

const CAPACITY: i64 = 8_000;

#[derive(Default)]
struct Level {
    /// Os block entities de cabo que o nível tem agora, com o NBT que o save deu.
    cables: HashMap<BlockPos, Option<Compound>>,
    published: Vec<(BlockPos, CableView)>,
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, _pos: BlockPos) -> bool {
        true
    }
    fn is_getting_powered(&self, _pos: BlockPos) -> bool {
        false
    }
    /// O Horizon dá a capability de energia a todo block entity de cabo que está no mundo (`Game::native_energy`), também ao que o dono ainda não criou.
    fn energy_handler(&self, pos: BlockPos, _side: Face) -> Option<Acceptor> {
        self.cables.get(&pos)?;
        Some(Rc::new(SnapshotAcceptor::new(BufferSnapshot {
            amount: 0,
            capacity: CAPACITY as i32,
            max_insert: CAPACITY as i32,
            max_extract: CAPACITY as i32,
            can_insert: true,
            can_extract: true,
        })))
    }
    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        let saved = self.cables.get(&pos)?.clone();
        Some(CableInfo {
            capacity: CAPACITY,
            saved,
        })
    }
    fn publish(&mut self, pos: BlockPos, view: &CableView) {
        self.published.push((pos, view.clone()));
    }
    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {}
}

fn owner() -> EnergyTransmitters {
    let mut uuid = 0i32;
    let mut tile = 0i32;
    EnergyTransmitters::new(
        DimensionId(0),
        move || {
            uuid += 1;
            Uuid::with_hash_code(uuid.wrapping_mul(0x1234_5679))
        },
        move |_| {
            tile += 1;
            tile.wrapping_mul(0x2545_F491)
        },
    )
}

const MIDDLE: BlockPos = BlockPos::new(0, 150, 0);
const SOUTH: BlockPos = BlockPos::new(0, 150, 1);
const EAST: BlockPos = BlockPos::new(1, 150, 0);

fn connections(view: &CableView) -> [bool; 6] {
    view.connected
}

/// Um mundo com os cabos postos e já andado, salvo: o NBT de cada um.
fn saved_world(cables: &[BlockPos]) -> HashMap<BlockPos, Compound> {
    let mut level = Level::default();
    let mut owner = owner();
    for pos in cables {
        level.cables.insert(*pos, None);
        owner.handle(&mut level, Event::ClearRemoved(*pos));
        // O vizinho avisado na ordem do jogo.
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if owner.cable_at(neighbor).is_some() {
                owner.handle(&mut level, Event::Neighbor(neighbor));
            }
        }
    }
    for time in 0..4 {
        for pos in owner.positions() {
            owner.handle(&mut level, Event::Update(pos));
        }
        owner.set_game_time(time);
        owner.handle(
            &mut level,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
    }
    cables
        .iter()
        .map(|pos| (*pos, owner.write(*pos).expect("o cabo grava")))
        .collect()
}

fn mask(owner: &EnergyTransmitters, level: &Level, pos: BlockPos) -> [bool; 6] {
    connections(&owner.view_at(level, pos).expect("o cabo está no dono"))
}

fn south_of(pos: BlockPos) -> usize {
    let _ = pos;
    Face::ALL
        .iter()
        .position(|face| *face == Face::South)
        .unwrap()
}

/// A capability dos vizinhos de `pos` que o block entity que entrou invalida (o `capabilities_at` do rastreador), entregue aos cabos que o dono tem.
fn capabilities_around(owner: &mut EnergyTransmitters, level: &mut Level, pos: BlockPos) {
    for side in Face::ALL {
        let neighbor = pos.relative(side);
        if owner.cable_at(neighbor).is_some() {
            owner.handle(level, Event::Capability(neighbor, side.opposite()));
        }
    }
}

/// O mundo reaberto, com a ordem de chegada do port: o `ClearRemoved` do cabo do sul chega antes de o block entity dele estar no mundo (`cable_info` vazio), o do meio já tem o dele, e a
/// espera entrega o do sul de novo no evento seguinte. O primeiro tick anda o sul (o ticker do `(0, 150, 1)` vem antes do `(0, 150, 0)` no jogo) e depois o meio.
fn reopen(saved: &HashMap<BlockPos, Compound>, late: &[BlockPos]) -> (EnergyTransmitters, Level) {
    let mut level = Level::default();
    let mut owner = owner();
    owner.handle(
        &mut level,
        Event::Ticket {
            chunk: ChunkPos::new(0, 0),
            loaded: true,
        },
    );
    let mut order: Vec<BlockPos> = saved.keys().copied().collect();
    order.sort_by_key(|pos| (pos.x, pos.y, pos.z));
    // Os que esperam entram no mundo depois do `ClearRemoved` e são entregues de novo.
    for pos in &order {
        if !late.contains(pos) {
            level.cables.insert(*pos, Some(saved[pos].clone()));
        }
        owner.handle(&mut level, Event::ClearRemoved(*pos));
        capabilities_around(&mut owner, &mut level, *pos);
    }
    for pos in late {
        level.cables.insert(*pos, Some(saved[pos].clone()));
        owner.handle(&mut level, Event::ClearRemoved(*pos));
        capabilities_around(&mut owner, &mut level, *pos);
    }
    for pos in [SOUTH, MIDDLE, EAST] {
        if owner.cable_at(pos).is_some() {
            owner.handle(&mut level, Event::Update(pos));
        }
    }
    owner.set_game_time(0);
    owner.handle(
        &mut level,
        Event::ServerTickPost {
            runs_normally: true,
        },
    );
    (owner, level)
}

#[test]
fn a_cable_keeps_the_connection_to_its_neighbour_when_the_neighbours_clear_removed_arrives_late() {
    let saved = saved_world(&[MIDDLE, SOUTH]);
    let south = south_of(MIDDLE);
    let (owner, level) = reopen(&saved, &[SOUTH]);
    assert!(
        mask(&owner, &level, MIDDLE)[south],
        "o meio mantém o braço ao sul"
    );
    assert!(
        mask(&owner, &level, SOUTH)[2],
        "o sul mantém o braço ao norte"
    );
    let _ = ConnectionType::Normal;
}

#[test]
fn the_order_of_arrival_does_not_change_the_connections() {
    let saved = saved_world(&[MIDDLE, SOUTH, EAST]);
    let (reference, reference_level) = reopen(&saved, &[]);
    for late in [
        vec![SOUTH],
        vec![MIDDLE],
        vec![EAST],
        vec![SOUTH, EAST],
        vec![MIDDLE, SOUTH, EAST],
    ] {
        let (owner, level) = reopen(&saved, &late);
        for pos in [MIDDLE, SOUTH, EAST] {
            assert_eq!(
                mask(&owner, &level, pos),
                mask(&reference, &reference_level, pos),
                "{pos:?} com {late:?} chegando depois"
            );
        }
    }
}
