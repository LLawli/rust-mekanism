//! A ordem em que o jogo percorre os block entities de um chunk que sai (`LevelChunk.clearAllBlockEntities`: o `onChunkUnloaded` de todos e depois o
//! `setRemoved` de todos). É a do mapa `ChunkAccess.blockEntities`, um `Object2ObjectOpenHashMap` que guarda **todos** os block entities do chunk, de
//! qualquer tipo, e a ordem de iteração dele depende de quantos são, da ordem em que entraram e do que já saiu (a tabela cresce e a remoção desloca chaves).
//! O Horizon guarda os block entities num mapa que não tem essa ordem, e só sabe a ordem dos de mod se contar só eles, o que erra quando o chunk tem um baú
//! ou um funil junto (medido: `transmitter_events_chunks_mixed.jsonl`, 28 cabos e 8 baús postos fora de ordem).
//!
//! Este mapa recebe o que o jogo faz com os block entities (`put` quando entram, `remove` quando saem, `load` quando o chunk carrega) e responde com a
//! ordem do original. `unload` reconcilia com o que o mundo tem de verdade antes de responder: o que o mundo tem e o mapa não viu (um block entity
//! posto antes de o dono dos transmissores existir) entra na ordem das posições, e o que o mapa tem e o mundo não, sai.
use super::chunks::BlockPos;
use super::open_hash::OpenHashMap;
use std::collections::HashMap;

/// O chunk de uma posição.
fn chunk_of(pos: BlockPos) -> (i32, i32) {
    (pos.x >> 4, pos.z >> 4)
}

/// Os mapas de block entities dos chunks, por dimensão e chunk.
#[derive(Default)]
pub struct BlockEntityOrder {
    chunks: HashMap<(String, i32, i32), OpenHashMap<BlockPos, ()>>,
}

impl BlockEntityOrder {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(dimension: &str, pos: BlockPos) -> (String, i32, i32) {
        let chunk = chunk_of(pos);
        (dimension.to_owned(), chunk.0, chunk.1)
    }

    /// `LevelChunk.setBlockEntity`: o block entity entra no mapa do chunk (se já está, fica no lugar: `put` não muda a posição de uma chave).
    pub fn put(&mut self, dimension: &str, pos: BlockPos) {
        self.chunks
            .entry(Self::key(dimension, pos))
            .or_default()
            .insert(pos, ());
    }

    /// `LevelChunk.removeBlockEntity`.
    pub fn remove(&mut self, dimension: &str, pos: BlockPos) {
        if let Some(map) = self.chunks.get_mut(&Self::key(dimension, pos)) {
            map.remove(pos);
        }
    }

    /// O chunk carregou: um mapa novo, com os block entities na ordem em que o jogo os recebeu (a da lista gravada).
    pub fn load(
        &mut self,
        dimension: &str,
        chunk: (i32, i32),
        list: impl IntoIterator<Item = BlockPos>,
    ) {
        let mut map = OpenHashMap::new();
        for pos in list {
            map.insert(pos, ());
        }
        self.chunks
            .insert((dimension.to_owned(), chunk.0, chunk.1), map);
    }

    /// O chunk vai sair: os block entities dele na ordem do mapa do original. `present` são as posições que o mundo tem no chunk agora.
    pub fn unload(
        &mut self,
        dimension: &str,
        chunk: (i32, i32),
        present: &[BlockPos],
    ) -> Vec<BlockPos> {
        let mut map = self
            .chunks
            .remove(&(dimension.to_owned(), chunk.0, chunk.1))
            .unwrap_or_default();
        let known = map.keys_in_order();
        for pos in &known {
            if !present.contains(pos) {
                map.remove(*pos);
            }
        }
        let mut unseen: Vec<BlockPos> = present
            .iter()
            .copied()
            .filter(|pos| !map.contains_key(*pos))
            .collect();
        unseen.sort_by_key(|pos| (pos.y, pos.z, pos.x));
        unseen.dedup();
        for pos in unseen {
            map.insert(pos, ());
        }
        map.keys_in_order()
    }

    /// O mundo fechou.
    pub fn clear(&mut self) {
        self.chunks.clear();
    }
}
