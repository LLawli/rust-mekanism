//! Quem avisa um bloco quando um vizinho muda, e em que ordem. O `redstone` de um `TileEntityMekanism` só muda quando o jogo o avisa
//! (`updatePower`, nos três pontos abaixo): ler o sinal a cada tick da máquina é outro comportamento.
//!
//! # Os três pontos que chamam `updatePower` (`TileEntityMekanism.java:1109`)
//!
//! 1. `BlockTile.neighborChanged` (`BlockTile.java:115-124`): um vizinho avisou por `Level.updateNeighborsAt`
//!    (`ServerLevel.java:1206-1213`), que roda a fila de `CollectingNeighborUpdater` (`CollectingNeighborUpdater.java:59-117`) com as
//!    seis faces em [`UPDATE_ORDER`] (`NeighborUpdater.java:18`). O `Level.setBlock` avisa quando a flag [`flags::UPDATE_NEIGHBORS`] está
//!    ligada (`Level.markAndNotifyBlock`, `Level.java:271`); o bloco que mudou NÃO avisa a si mesmo.
//! 2. `BlockTile.onNeighborChange` (`BlockTile.java:105-113`) -> `TileEntityMekanism.onNeighborChange` (`:993-997`): o gancho do NeoForge
//!    (`IBlockExtension.onNeighborChange`), chamado por `Level.updateNeighbourForOutputSignal` (`Level.java:992-1006`) nas faces de
//!    [`OUTPUT_SIGNAL_ORDER`] (`Level.java:990`), com o vizinho (qualquer block entity que mudou: `BlockEntity.setChanged`,
//!    `BlockEntity.java:216-221`; `Level.removeBlockEntity`, `Level.java:686`; um bloco com sinal analógico posto, `Level.java:273`;
//!    o comparador de uma máquina do Mekanism, `TileEntityMekanism.notifyComparatorChange`, `:508-510`).
//! 3. `TileEntityMekanism.onAdded` (`:1000-1008`), de `BlockMekanism.setPlacedBy` (`BlockMekanism.java:179-196`): só o bloco posto por
//!    quem tem item (um jogador, um dispenser). `/setblock`, o gabarito de teste e o carregamento do chunk não passam por ele.
//!
//! O valor lido é `level.hasNeighborSignal(pos)` na hora do aviso. Uma mudança de sinal que não avisa (`setBlock` sem a flag 1,
//! `/setblock ... strict`) deixa o `redstone` velho até o próximo aviso, e o `redstone` salvo no NBT (`TileEntityMekanism.java:750,775`)
//! volta velho com o chunk.
use crate::sides::Face;
use std::collections::BTreeMap;

/// `NeighborUpdater.UPDATE_ORDER` (`NeighborUpdater.java:18`): a ordem de `Level.updateNeighborsAt`.
pub const UPDATE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

/// `Level.NEIGHBOR_UPDATE_LIST` (NeoForge, `Level.java:990`): `Direction.Plane.HORIZONTAL` (norte, leste, sul, oeste) e depois
/// `Direction.Plane.VERTICAL` (cima, baixo). A ordem de `Level.updateNeighbourForOutputSignal`.
pub const OUTPUT_SIGNAL_ORDER: [Face; 6] = [
    Face::North,
    Face::East,
    Face::South,
    Face::West,
    Face::Up,
    Face::Down,
];

/// `Block.UPDATE_*`, as flags de `Level.setBlock`.
pub mod flags {
    /// `Block.UPDATE_NEIGHBORS`: `Level.updateNeighborsAt` (e o comparador do bloco novo).
    pub const UPDATE_NEIGHBORS: i32 = 1;
    /// `Block.UPDATE_CLIENTS`.
    pub const UPDATE_CLIENTS: i32 = 2;
    /// `Block.UPDATE_ALL`.
    pub const UPDATE_ALL: i32 = UPDATE_NEIGHBORS | UPDATE_CLIENTS;
    /// `Block.UPDATE_KNOWN_SHAPE`.
    pub const UPDATE_KNOWN_SHAPE: i32 = 16;

    /// O `setBlock` avisa os vizinhos (`Level.markAndNotifyBlock`, `Level.java:270`).
    pub const fn notifies_neighbors(flags: i32) -> bool {
        flags & UPDATE_NEIGHBORS != 0
    }
}

/// O sinal que os avisos de vizinhança entregaram a uma posição e que o dono ainda não leu. O dono é um tile que o mod guarda em
/// outro lugar (e que pode nem existir ainda): quem avisa grava, quem lê pega antes de ler o `redstone`. Só vale o último valor, como
/// o campo `redstone` do Java, que cada `updatePower` sobrescreve.
#[derive(Debug)]
pub struct PendingPower<K: Ord> {
    powers: BTreeMap<K, bool>,
}

impl<K: Ord> Default for PendingPower<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord> PendingPower<K> {
    pub fn new() -> Self {
        Self {
            powers: BTreeMap::new(),
        }
    }

    /// `updatePower` com o sinal lido agora.
    pub fn record(&mut self, key: K, power: bool) {
        self.powers.insert(key, power);
    }

    /// O que o dono ainda não leu, uma vez só.
    pub fn take(&mut self, key: &K) -> Option<bool> {
        self.powers.remove(key)
    }

    /// O tile saiu do mundo com avisos por ler: eles não valem para o próximo da mesma posição.
    pub fn forget(&mut self, key: &K) {
        self.powers.remove(key);
    }

    pub fn is_empty(&self) -> bool {
        self.powers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(order: [Face; 6]) -> Vec<&'static str> {
        order.iter().map(|face| face.name()).collect()
    }

    #[test]
    fn the_orders_are_the_ones_of_the_game() {
        assert_eq!(
            names(UPDATE_ORDER),
            ["west", "east", "down", "up", "north", "south"]
        );
        assert_eq!(
            names(OUTPUT_SIGNAL_ORDER),
            ["north", "east", "south", "west", "up", "down"]
        );
    }

    #[test]
    fn each_order_has_every_face_once() {
        for order in [UPDATE_ORDER, OUTPUT_SIGNAL_ORDER] {
            for face in Face::ALL {
                assert_eq!(order.iter().filter(|&&other| other == face).count(), 1);
            }
        }
    }

    #[test]
    fn only_the_first_flag_notifies_the_neighbours() {
        assert!(flags::notifies_neighbors(flags::UPDATE_ALL));
        assert!(flags::notifies_neighbors(flags::UPDATE_NEIGHBORS));
        assert!(!flags::notifies_neighbors(flags::UPDATE_CLIENTS));
        assert!(!flags::notifies_neighbors(
            flags::UPDATE_CLIENTS | flags::UPDATE_KNOWN_SHAPE | 32 | 256 | 512
        ));
    }

    #[test]
    fn the_last_recorded_power_is_the_one_that_counts_and_is_read_once() {
        let mut pending = PendingPower::new();
        pending.record((0, 1), true);
        pending.record((0, 1), false);
        assert_eq!(pending.take(&(0, 1)), Some(false));
        assert_eq!(pending.take(&(0, 1)), None);
        assert!(pending.is_empty());
    }

    #[test]
    fn forgetting_a_position_drops_what_was_not_read() {
        let mut pending = PendingPower::new();
        pending.record(1, true);
        pending.forget(&1);
        assert_eq!(pending.take(&1), None);
    }
}
