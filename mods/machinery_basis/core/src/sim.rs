//! Mundo simulado dos testes: o relógio, o sinal de redstone e os blocos vizinhos que expõem itens, na ordem em que o jogo
//! roda um tick. Os chunks (carga, descarga, blocos e block entities que somem e voltam) estão em `chunks`.
pub mod chunks;
pub mod java_hash;
pub mod open_hash;

use crate::item_handler::{ItemCapabilities, ItemHandler};
use crate::sides::Face;
use std::rc::Rc;

/// Bloco que o mundo simulado faz andar.
pub trait SimBlockEntity {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities);
    /// `onNeighborChange`, já com o sinal que os vizinhos dão.
    fn neighbor_changed(&mut self, powered: bool);
}

/// Os blocos ao redor, vistos pelas capabilities de item.
#[derive(Default)]
pub struct SimNeighbours {
    handlers: Vec<(Face, Rc<dyn ItemHandler>)>,
}

impl SimNeighbours {
    /// Põe um bloco com itens do lado `face` do bloco que anda.
    pub fn set(&mut self, face: Face, handler: Rc<dyn ItemHandler>) {
        self.handlers.retain(|(placed, _)| *placed != face);
        self.handlers.push((face, handler));
    }
}

impl ItemCapabilities for SimNeighbours {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        self.handlers
            .iter()
            .find(|(placed, _)| *placed == face)
            .map(|(_, handler)| handler.clone())
    }
}

/// Um bloco, com o sinal e os vizinhos ao lado dele.
pub struct SimWorld<T> {
    pub block_entity: T,
    pub neighbours: SimNeighbours,
    game_time: u64,
    signal: bool,
}

impl<T: SimBlockEntity> SimWorld<T> {
    pub fn new(block_entity: T) -> Self {
        Self {
            block_entity,
            neighbours: SimNeighbours::default(),
            game_time: 0,
            signal: false,
        }
    }

    pub fn game_time(&self) -> u64 {
        self.game_time
    }

    /// O bloco anda primeiro. O que o teste faz depois deste tick só aparece no próximo, como num `gameTest`.
    pub fn tick(&mut self) {
        self.block_entity
            .tick_server(self.game_time, &self.neighbours);
        self.game_time += 1;
    }

    pub fn set_signal(&mut self, signal: bool) {
        if self.signal != signal {
            self.signal = signal;
            self.block_entity.neighbor_changed(signal);
        }
    }
}
