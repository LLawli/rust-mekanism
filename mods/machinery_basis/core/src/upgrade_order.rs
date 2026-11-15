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
//! A tabela do tile é guardada em memória, a mesma que o original guarda (decisão 233): [`UpgradeKey`] é a chave, e o componente (`upgrade.rs`) insere nela na ordem em que os
//! tipos entram na sessão. Depois de recarregar do disco o original a refaz na ordem do `HashMap<String, Tag>` do NBT (`UPGRADES_CODEC`: `Codec.unboundedMap` e
//! `new Object2IntOpenHashMap<>(mapa)`), que o port reproduz com [`file_order`] e o `String.hashCode` das chaves de texto.
use crate::identity_hash::identity_hash;
use crate::sim::java_hash::hash_set_order;
use crate::sim::open_hash::{OpenHashMap, OpenKey, mix_int};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

/// `HashMap` do `CompoundTag`: `Maps.newHashMap()`, 16 posições.
const COMPOUND_CAPACITY: usize = 16;

/// `String.hashCode`: `s[0]*31^(n-1) + ... + s[n-1]`, sobre as unidades UTF-16.
pub fn string_hash(text: &str) -> i32 {
    text.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

thread_local! {
    /// Os hashes que um teste fixa para os tipos desta thread (os que o original deu, gravados no rastro), no lugar do sorteio.
    static FIXED: RefCell<HashMap<String, i32>> = RefCell::new(HashMap::new());
}

/// Os tipos dados passam a ter, nesta thread, o hash dado (os do rastro do original). `fixed_hashes(&[])` volta ao sorteio.
pub fn fixed_hashes(hashes: &[(String, i32)]) {
    FIXED.with_borrow_mut(|fixed| *fixed = hashes.iter().cloned().collect());
}

/// O `hashCode` do `Holder<Upgrade>` de um tipo (o `System.identityHashCode` da `ResourceKey`, sorteado uma vez por processo, decisão 225).
pub fn upgrade_hash(id: &str) -> i32 {
    FIXED
        .with_borrow(|fixed| fixed.get(id).copied())
        .unwrap_or_else(|| identity_hash(&format!("upgrade:{id}")))
}

static IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Um tipo de upgrade na tabela do tile: o hash de identidade dele e um número de ordem (dois tipos com o mesmo hash continuam dois, como dois objetos).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UpgradeKey {
    hash: i32,
    ordinal: u32,
}

impl OpenKey for UpgradeKey {
    fn mixed(self) -> i32 {
        mix_int(self.hash)
    }
}

impl UpgradeKey {
    pub fn of(id: &str) -> Self {
        let mut ids = IDS.lock().unwrap_or_else(|e| e.into_inner());
        let ordinal = ids.iter().position(|known| known == id).unwrap_or_else(|| {
            ids.push(id.to_owned());
            ids.len() - 1
        });
        Self {
            hash: upgrade_hash(id),
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
        }
    }

    pub fn id(self) -> String {
        let ids = IDS.lock().unwrap_or_else(|e| e.into_inner());
        ids[self.ordinal as usize].clone()
    }
}

/// A ordem em que o `HashMap` do NBT devolve as chaves de texto depois de inseri-las na ordem dada (o que o arquivo guarda e o que a leitura reinsere).
pub fn file_order(ids: &[String]) -> Vec<String> {
    hash_set_order(
        ids.iter().map(String::as_str),
        string_hash,
        COMPOUND_CAPACITY,
    )
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// O mapa que `UPGRADES_CODEC` monta ao ler: `new Object2IntOpenHashMap<>(mapa)`, a tabela do tamanho do mapa e as entradas na ordem do `HashMap` do NBT.
pub fn loaded_map(entries_in_file_order: &[(String, i32)]) -> OpenHashMap<UpgradeKey, i32> {
    let mut map = OpenHashMap::with_expected(entries_in_file_order.len());
    for (id, total) in entries_in_file_order {
        map.insert(UpgradeKey::of(id), *total);
    }
    map
}

/// As entradas de um conjunto de ids lido do disco, na ordem em que o original as insere: a do `HashMap` do NBT. O NBT do port é ordenado e não guarda a ordem do arquivo, então
/// a inserção no `HashMap` parte da ordem alfabética; só difere quando duas chaves caem no mesmo balde.
pub fn entries_from_disk(sorted: &[(String, i32)]) -> Vec<(String, i32)> {
    let ids: Vec<String> = sorted.iter().map(|(id, _)| id.clone()).collect();
    file_order(&ids)
        .into_iter()
        .filter_map(|id| sorted.iter().find(|(known, _)| *known == id).cloned())
        .collect()
}

/// As entradas de um conjunto de ids lido do disco quando o tile guardou a ordem em que o original escreveria o arquivo (`file`, a de [`file_order`] sobre a tabela da sessão que o
/// gravou): o `HashMap` do NBT relê as chaves nessa ordem e a devolve igual, então ela é a ordem das entradas (decisão 233). Vale só se `file` tem exatamente os ids de `known`
/// (um NBT editado à mão, ou um que outro mod mexeu, cai na ordem alfabética de [`entries_from_disk`]).
pub fn entries_from_file(known: &[(String, i32)], file: &[String]) -> Vec<(String, i32)> {
    let same_set = file.len() == known.len()
        && file
            .iter()
            .all(|id| known.iter().any(|(known, _)| known == id))
        && file
            .iter()
            .enumerate()
            .all(|(index, id)| !file[..index].contains(id));
    if !same_set {
        return entries_from_disk(known);
    }
    file.iter()
        .filter_map(|id| known.iter().find(|(known, _)| known == id).cloned())
        .collect()
}

/// O mapa que o cliente decodifica da janela de upgrades (`SyncableStreamCodec.upgradeMap`: `Object2IntOpenHashMap::new` com o tamanho, as entradas na ordem do mapa do tile):
/// a ordem de `getInstalledTypes()`.
pub fn window_order(tile_order: &[String]) -> Vec<String> {
    let mut map: OpenHashMap<UpgradeKey, ()> = OpenHashMap::with_expected(tile_order.len());
    for id in tile_order {
        map.insert(UpgradeKey::of(id), ());
    }
    map.keys_in_order()
        .into_iter()
        .map(UpgradeKey::id)
        .collect()
}

/// A ordem das linhas do tooltip de um item lido do disco: o mapa do item (`new Object2IntOpenHashMap<>(upgrades)`, na ordem do `HashMap` do NBT) e o que o cliente decodifica
/// (`UpgradeAware.STREAM_CODEC`, 32 posições).
pub fn item_tooltip_order(sorted: &[(String, i32)]) -> Vec<String> {
    let item = loaded_map(&entries_from_disk(sorted));
    let mut decoded: OpenHashMap<UpgradeKey, ()> = OpenHashMap::new();
    for key in item.keys_in_order() {
        decoded.insert(key, ());
    }
    decoded
        .keys_in_order()
        .into_iter()
        .map(UpgradeKey::id)
        .collect()
}

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
