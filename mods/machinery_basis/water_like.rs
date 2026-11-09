// Entra em `crates/horizon-client/src/motion.rs` como `pub mod machinery_water_like` (`visibility = "pub"`). Os estados de bloco de líquido de mod, por número de
// estado, para a física de entidade (`FluidResources::fluid`, só água e lava): o nível mais 1 onde o estado é de um bloco de líquido declarado, 0 no resto. O
// `Game` (crate de cima) escreve a tabela a cada carga de dados e o RETURN de `FluidResources::fluid` (crate da biblioteca) a lê.
use std::sync::RwLock;

static TABLE: RwLock<Vec<u8>> = RwLock::new(Vec::new());

/// Troca a tabela: a cada carga de dados.
pub fn set(table: Vec<u8>) {
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = table;
}

/// O nível (0 a 15) do estado `state`, se ele é de um bloco de líquido de mod.
pub fn get(state: u32) -> Option<u8> {
    match TABLE.read().unwrap_or_else(|e| e.into_inner()).get(state as usize) {
        Some(&entry) if entry > 0 => Some(entry - 1),
        _ => None,
    }
}
