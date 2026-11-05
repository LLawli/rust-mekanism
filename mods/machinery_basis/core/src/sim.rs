//! Mundo simulado dos testes: o relógio e o sinal de redstone, na ordem em que o jogo roda um tick.

/// Bloco que o mundo simulado faz andar.
pub trait SimBlockEntity {
    fn tick_server(&mut self, game_time: u64);
    /// `onNeighborChange`, já com o sinal que os vizinhos dão.
    fn neighbor_changed(&mut self, powered: bool);
}

/// Um bloco, com o sinal ao lado dele.
#[derive(Clone, Debug)]
pub struct SimWorld<T> {
    pub block_entity: T,
    game_time: u64,
    signal: bool,
}

impl<T: SimBlockEntity> SimWorld<T> {
    pub fn new(block_entity: T) -> Self {
        Self {
            block_entity,
            game_time: 0,
            signal: false,
        }
    }

    pub fn game_time(&self) -> u64 {
        self.game_time
    }

    /// O bloco anda primeiro. O que o teste faz depois deste tick só aparece no próximo, como num `gameTest`.
    pub fn tick(&mut self) {
        self.block_entity.tick_server(self.game_time);
        self.game_time += 1;
    }

    pub fn set_signal(&mut self, signal: bool) {
        if self.signal != signal {
            self.signal = signal;
            self.block_entity.neighbor_changed(signal);
        }
    }
}
