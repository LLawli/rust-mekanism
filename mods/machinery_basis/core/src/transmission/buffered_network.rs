//! `DynamicBufferedNetwork` (`lib/transmitter/DynamicBufferedNetwork.java`), a parte que não depende do que a rede guarda: os chunks dela,
//! a capacidade, o `currentScale` e o `needsUpdate` que manda o `PacketNetworkScale`, e o relógio das duas validações por tick
//! (`validateSaveShares` e `markDirty`). A família (energia, fluido...) tem o buffer e implementa `absorbBuffer`, `clampBuffer` e
//! `updateSaveShares`; `energy_network` é a da energia.
use super::energy_container::add_clamped;
use crate::energy_cube::scale_changed;
use std::collections::BTreeSet;

/// Os campos de `DynamicBufferedNetwork`.
#[derive(Debug, Default)]
pub struct BufferedNetwork {
    /// `chunks`: `ChunkPos.pack` de cada transmissor (a ordem só decide a de `markChunkDirty`).
    chunks: BTreeSet<i64>,
    /// `capacity`.
    capacity: i64,
    /// `needsUpdate`.
    pub needs_update: bool,
    /// `forceScaleUpdate`.
    force_scale_update: bool,
    /// `lastSaveShareWriteTime`: começa em 0, como o `long` do Java.
    last_save_share_write_time: i64,
    /// `lastMarkDirtyTime`.
    last_mark_dirty_time: i64,
    /// `currentScale`.
    pub current_scale: f32,
}

/// `ChunkPos.pack(x >> 4, z >> 4)` de uma posição.
pub fn chunk_of(x: i32, z: i32) -> i64 {
    i64::from(x >> 4) & 0xFFFF_FFFF | (i64::from(z >> 4) & 0xFFFF_FFFF) << 32
}

impl BufferedNetwork {
    /// `getCapacity()`.
    pub fn capacity(&self) -> i64 {
        self.capacity
    }

    /// Os chunks dos transmissores da rede.
    pub fn chunks(&self) -> impl Iterator<Item = i64> + '_ {
        self.chunks.iter().copied()
    }

    /// `onUpdate()`, a parte do buffer: `scaleChanged(currentScale, scale)` marca `needsUpdate`.
    pub fn update_scale(&mut self, scale: f32) {
        if scale_changed(self.current_scale, scale) {
            self.current_scale = scale;
            self.needs_update = true;
        }
    }

    /// `addNewTransmitters`, depois do `super`: sem transmissores, a escala vai direto ao alvo depois da primeira adição.
    pub fn new_transmitters_added(&mut self, was_empty: bool) {
        if !self.force_scale_update {
            self.force_scale_update = was_empty;
        }
    }

    /// `addTransmitterFromCommit`, antes do `absorbBuffer`: o chunk do transmissor e a capacidade somada com teto.
    pub fn transmitter_committed(&mut self, chunk: i64, transmitter_capacity: i64) {
        self.chunks.insert(chunk);
        self.capacity = add_clamped(self.capacity, transmitter_capacity);
    }

    /// `validTransmittersAdded()`, depois do `clampBuffer`: devolve `true` quando a escala deve ser forçada ao alvo agora.
    pub fn valid_transmitters_added(&mut self) -> bool {
        let force = std::mem::replace(&mut self.force_scale_update, false);
        self.needs_update = true;
        force
    }

    /// `adoptTransmittersAndAcceptorsFrom`: os chunks da outra rede.
    pub fn adopt_chunks(&mut self, chunks: impl IntoIterator<Item = i64>) {
        self.chunks.extend(chunks);
    }

    /// `updateCapacity()`: a soma das capacidades dos transmissores, com teto. O teste de estouro compara com o `capacity` de antes e
    /// não com a soma parcial, como o Java.
    pub fn update_capacity(&mut self, capacities: impl IntoIterator<Item = i64>) {
        let mut sum: i64 = 0;
        for transmitter_capacity in capacities {
            if transmitter_capacity > i64::MAX - self.capacity {
                sum = i64::MAX;
                break;
            }
            sum += transmitter_capacity;
        }
        if self.capacity != sum {
            self.capacity = sum;
        }
    }

    /// `deregister()`, depois do `super`: esquece os chunks.
    pub fn deregistered(&mut self) {
        self.chunks.clear();
    }

    /// `validateSaveShares`: `true` se ainda não validou neste tick do jogo (e guarda o tick).
    pub fn should_validate_save_shares(&mut self, game_time: i64) -> bool {
        if game_time != self.last_save_share_write_time {
            self.last_save_share_write_time = game_time;
            true
        } else {
            false
        }
    }

    /// `markDirty()`: os chunks a marcar, uma vez por tick do jogo.
    pub fn chunks_to_mark_dirty(&mut self, game_time: i64) -> Vec<i64> {
        if game_time != self.last_mark_dirty_time {
            self.last_mark_dirty_time = game_time;
            self.chunks.iter().copied().collect()
        } else {
            Vec::new()
        }
    }

    /// Quando a escala é forçada: o valor novo.
    pub fn set_forced_scale(&mut self, scale: f32) {
        self.current_scale = scale;
    }
}
