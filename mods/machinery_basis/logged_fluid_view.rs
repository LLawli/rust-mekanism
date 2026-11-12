// Entra em `crates/horizon-world/src/registry.rs` como `pub mod machinery_logged_fluid` (`visibility = "pub"`). O modo "montando a tabela do mesher" de uma thread: enquanto ele vale,
// `view` devolve o estado da fonte de água ou de lava no lugar do estado de um bloco com `fluid_logged=water` ou `lava`, e é isso que `FluidState::from_block` lê.
use crate::chunk::BlockState;
use std::cell::Cell;
use std::sync::OnceLock;

thread_local! {
    /// Quantos estados a tabela em montagem tem; 0 fora da montagem.
    static REMAINING: Cell<usize> = const { Cell::new(0) };
}

static SOURCES: OnceLock<[BlockState; 2]> = OnceLock::new();

/// Liga o modo para uma tabela de `states` estados (o laço do mesher pede o estado de cada um, em ordem, uma vez).
pub fn begin(states: usize) {
    REMAINING.with(|remaining| remaining.set(states));
}

fn sources() -> &'static [BlockState; 2] {
    SOURCES.get_or_init(|| {
        ["minecraft:water", "minecraft:lava"].map(|name| {
            let mut properties = crate::block_state_properties::BlockProperties::new();
            properties.insert("level".to_owned(), "0".to_owned());
            BlockState {
                name: name.to_owned(),
                properties,
            }
        })
    })
}

/// O estado que a tabela do mesher lê no lugar de `state` (o estado de número `id`).
pub fn view(state: Option<&BlockState>, id: u32) -> Option<&BlockState> {
    let states = REMAINING.with(Cell::get);
    if states == 0 {
        return state;
    }
    // O laço do mesher pede os estados de 0 até `states - 1`: o último desliga o modo.
    if id as usize + 1 >= states {
        REMAINING.with(|cell| cell.set(0));
    }
    let found = state?;
    match found.properties.get("fluid_logged").map(String::as_str) {
        Some("water") => Some(&sources()[0]),
        Some("lava") => Some(&sources()[1]),
        _ => state,
    }
}
