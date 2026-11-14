//! A ordem das linhas de upgrade no tooltip do item de uma máquina (decisão 225). `registerComponentAppenderAfter(UPGRADES, ...)` percorre o `Object2IntMap<Holder<Upgrade>>` do
//! `UpgradeAware` com `Object2IntMaps.fastIterator`, e o mapa é um `Object2IntOpenHashMap` de `Holder<Upgrade>`, que o NeoForge faz ter o `hashCode` da `ResourceKey`; a
//! `ResourceKey` não o sobrescreve e vale o `System.identityHashCode`, que o HotSpot sorteia a cada execução na primeira vez que o objeto o pede. Dentro de uma execução cada
//! tipo tem um hash só, então a ordem não muda; de uma execução para outra, muda (medido: `rastros/item_upgrade_order_<n>.jsonl`, duas execuções do original).
//!
//! `Holder.Reference.hashCode` é `key.hashCode()` (o acréscimo do NeoForge, `Holder.java:281`), e `ResourceKey.create` interna as chaves: o servidor e o cliente do mesmo
//! processo veem a mesma chave, com o mesmo hash.
//!
//! O mapa que o tooltip percorre passa por três tabelas, e a ordem de cada uma é a da tabela do fastutil (`sim::open_hash`), com sondagem linear (a ordem de inserção só
//! importa quando duas chaves caem na mesma posição):
//!
//! 1. o do tile: o item da máquina posta traz `UpgradeAware.EMPTY` (o padrão do bloco), e `applyImplicitComponents` faz `new Object2IntOpenHashMap<>(upgradeAware.upgrades())`, um
//!    mapa de 0 entradas (2 posições) que cresce a cada instalação (`TileComponentUpgrade.setUpgrades`, cada `rehash` reordena);
//! 2. o do item, `new Object2IntOpenHashMap<>(upgrades)` (`collectImplicitComponents`, `TileComponentUpgrade.java:226`): a tabela do tamanho do mapa, as entradas na ordem
//!    do anterior;
//! 3. o que o cliente monta ao decodificar o item (`UpgradeAware.STREAM_CODEC`, `ByteBufCodecs.map(_ -> new Object2IntOpenHashMap<>(), ...)`): 32 posições, as entradas na
//!    ordem do item.
//!
//! O que o port não sabe é a ordem das instalações do tile (o histórico não fica no NBT, e no original também só vale na sessão em que se instalou): ele usa a do registro.
//! Só importa quando duas chaves colidem na tabela de 32.
use crate::sim::open_hash::{OpenHashMap, OpenKey, mix_int};

/// Um `Holder<Upgrade>` na tabela: o hash de identidade dele e a posição na lista de quem chama (dois `Holder` com o mesmo hash continuam dois).
#[derive(Clone, Copy, PartialEq, Eq)]
struct HolderKey {
    hash: i32,
    index: usize,
}

impl OpenKey for HolderKey {
    fn mixed(self) -> i32 {
        mix_int(self.hash)
    }
}

/// A ordem em que a tabela devolve as chaves depois de inseri-las na ordem dada (`keys` são índices de `0..hashes.len()`).
fn table_order(mut map: OpenHashMap<HolderKey, ()>, keys: &[usize], hashes: &[i32]) -> Vec<usize> {
    for &index in keys {
        map.insert(
            HolderKey {
                hash: hashes[index],
                index,
            },
            (),
        );
    }
    map.keys_in_order()
        .into_iter()
        .map(|key| key.index)
        .collect()
}

/// A ordem de cada etapa (o tile, o item e o que o cliente decodifica): os índices de `0..hashes.len()` (os upgrades instalados, na ordem de instalação que o tile viu), com
/// `hashes[i]` o `hashCode` do `Holder` do upgrade `i`.
pub fn stages(hashes: &[i32]) -> [Vec<usize>; 3] {
    let installed: Vec<usize> = (0..hashes.len()).collect();
    let tile = table_order(OpenHashMap::with_expected(0), &installed, hashes);
    let item = table_order(OpenHashMap::with_expected(tile.len()), &tile, hashes);
    let decoded = table_order(OpenHashMap::new(), &item, hashes);
    [tile, item, decoded]
}

/// A ordem das linhas de upgrade do tooltip: a do mapa que o cliente decodifica.
pub fn tooltip_order(hashes: &[i32]) -> Vec<usize> {
    let [_, _, decoded] = stages(hashes);
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_is_a_permutation_and_stays_put() {
        let hashes = [11, 22, 33, 44];
        let order = tooltip_order(&hashes);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3]);
        assert_eq!(order, tooltip_order(&hashes));
    }

    #[test]
    fn another_draw_of_hashes_gives_another_order() {
        let orders: Vec<Vec<usize>> = (1..64)
            .map(|seed| tooltip_order(&[seed * 7919, seed * 104_729 + 3]))
            .collect();
        assert!(orders.contains(&vec![0, 1]) && orders.contains(&vec![1, 0]));
    }
}
