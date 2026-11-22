//! A ordem de iteração do `java.util.HashSet`, que o jogo usa onde grava o chunk: `ChunkAccess.getBlockEntitiesPos` monta um
//! `Sets.newHashSet(...)`, e `SerializableChunkData.copyOf` chama `getBlockEntityNbtForSaving` nessa ordem, que não é a do mapa
//! do chunk. O rastro do `gameTestServer` mostra: `saveAdditional` dos cabos de um chunk sai na ordem da tabela do `HashMap`, e
//! os block entities voltam do disco nessa mesma ordem.
//!
//! Porta o `HashMap` do OpenJDK 21 no que muda a ordem: o índice `(h ^ h >>> 16) & (n - 1)`, a lista de cada balde em ordem de
//! inserção, o `resize` que parte cada balde em dois sem trocar a ordem e o `treeifyBin` que, com tabela menor que 64, só
//! dobra a tabela. Balde com nove nós numa tabela de 64 ou mais vira árvore, e a ordem da árvore não é portada: o `panic`
//! avisa, em vez de devolver uma ordem que o jogo não tem.

/// `HashMap.MIN_TREEIFY_CAPACITY`.
const MIN_TREEIFY_CAPACITY: usize = 64;
/// `HashMap.TREEIFY_THRESHOLD`.
const TREEIFY_THRESHOLD: usize = 8;
/// A capacidade que `new HashSet<>(collection)` pede: `max((int) (size / .75f) + 1, 16)`.
pub fn initial_capacity_for(collection_len: usize) -> usize {
    ((collection_len as f32 / 0.75_f32) as usize + 1).max(16)
}

/// `HashMap.hash`: `h ^ (h >>> 16)`.
fn spread(hash_code: i32) -> u32 {
    let h = hash_code as u32;
    h ^ (h >> 16)
}

/// `HashMap.tableSizeFor`.
fn table_size_for(capacity: usize) -> usize {
    capacity.max(1).next_power_of_two()
}

struct Table<K> {
    buckets: Vec<Vec<(u32, K)>>,
    size: usize,
}

impl<K> Table<K> {
    /// `HashMap.resize` com a tabela já criada: cada balde se parte em dois, o de baixo no mesmo índice e o de cima `n` depois.
    fn double(&mut self) {
        let old = self.buckets.len();
        let mut next: Vec<Vec<(u32, K)>> = (0..old * 2).map(|_| Vec::new()).collect();
        for (index, bucket) in std::mem::take(&mut self.buckets).into_iter().enumerate() {
            for (hash, key) in bucket {
                let target = if hash as usize & old == 0 {
                    index
                } else {
                    index + old
                };
                next[target].push((hash, key));
            }
        }
        self.buckets = next;
    }
}

/// As chaves, distintas, na ordem em que um `HashSet` criado com `initial_capacity` as devolve depois de `add` em `keys`.
pub fn hash_set_order<K: Copy>(
    keys: impl IntoIterator<Item = K>,
    hash_code: impl Fn(K) -> i32,
    initial_capacity: usize,
) -> Vec<K> {
    hash_set_table(keys, hash_code, initial_capacity).0
}

/// [`hash_set_order`] e o tamanho que a tabela ficou. `HashSet.clear()` esvazia a tabela sem encolhê-la, então quem limpa e volta a
/// inserir parte desse tamanho.
pub fn hash_set_table<K: Copy>(
    keys: impl IntoIterator<Item = K>,
    hash_code: impl Fn(K) -> i32,
    initial_capacity: usize,
) -> (Vec<K>, usize) {
    let capacity = table_size_for(initial_capacity);
    let mut table = Table {
        buckets: (0..capacity).map(|_| Vec::new()).collect(),
        size: 0,
    };
    for key in keys {
        let hash = spread(hash_code(key));
        let index = hash as usize & (table.buckets.len() - 1);
        table.buckets[index].push((hash, key));
        // `putVal`: `binCount >= TREEIFY_THRESHOLD - 1` ao ligar o nono nó.
        if table.buckets[index].len() > TREEIFY_THRESHOLD {
            assert!(
                table.buckets.len() < MIN_TREEIFY_CAPACITY,
                "HashSet com um balde de {} nós numa tabela de {}: a ordem da árvore não é portada",
                table.buckets[index].len(),
                table.buckets.len()
            );
            table.double();
        }
        table.size += 1;
        // `++size > threshold`, com o limite em 0,75 da tabela.
        if table.size > table.buckets.len() * 3 / 4 {
            table.double();
        }
    }
    let length = table.buckets.len();
    let order = table
        .buckets
        .into_iter()
        .flatten()
        .map(|(_, key)| key)
        .collect();
    (order, length)
}

/// Um `HashMap<K, V>` do Java que vive e muda: a chave é um inteiro cujo `hashCode` é ele mesmo (`Integer`, ou um record de um `int`
/// como `RecipeDisplayId`). Guarda o que decide a ordem de `values()`: a tabela nasce com 16 no primeiro `put`, dobra quando o tamanho
/// passa de 0,75 dela, nunca encolhe (`remove` tira o nó do balde e `clear` esvazia os baldes sem trocar a tabela), e cada balde fica
/// na ordem em que as chaves chegaram. Trocar o valor de uma chave que já está não muda o lugar dela.
#[derive(Clone, Debug)]
pub struct JavaIntMap<V> {
    buckets: Vec<Vec<(i32, V)>>,
    size: usize,
    treeified: bool,
}

impl<V> Default for JavaIntMap<V> {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            size: 0,
            treeified: false,
        }
    }
}

impl JavaIntMap<()> {
    /// Uma tabela nova com `keys` postas nessa ordem.
    pub fn from_keys(keys: impl IntoIterator<Item = i32>) -> Self {
        let mut map = Self::default();
        for key in keys {
            map.put(key, ());
        }
        map
    }
}

impl<V> JavaIntMap<V> {
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `table.length` (0 antes do primeiro `put`).
    pub fn capacity(&self) -> usize {
        self.buckets.len()
    }

    fn index(&self, key: i32) -> usize {
        spread(key) as usize & (self.buckets.len() - 1)
    }

    pub fn get(&self, key: i32) -> Option<&V> {
        if self.buckets.is_empty() {
            return None;
        }
        self.buckets[self.index(key)]
            .iter()
            .find(|(other, _)| *other == key)
            .map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: i32) -> bool {
        self.get(key).is_some()
    }

    /// `HashMap.resize` com a tabela criada: cada balde se parte em dois sem trocar a ordem.
    fn double(&mut self) {
        let old = self.buckets.len();
        let mut next: Vec<Vec<(i32, V)>> = (0..old * 2).map(|_| Vec::new()).collect();
        for (index, bucket) in std::mem::take(&mut self.buckets).into_iter().enumerate() {
            for (key, value) in bucket {
                let target = if spread(key) as usize & old == 0 {
                    index
                } else {
                    index + old
                };
                next[target].push((key, value));
            }
        }
        self.buckets = next;
    }

    /// `HashMap.put`: devolve o valor que a chave tinha.
    pub fn put(&mut self, key: i32, value: V) -> Option<V> {
        if self.buckets.is_empty() {
            self.buckets = (0..16).map(|_| Vec::new()).collect();
        }
        let index = self.index(key);
        if let Some(slot) = self.buckets[index]
            .iter_mut()
            .find(|(other, _)| *other == key)
        {
            return Some(std::mem::replace(&mut slot.1, value));
        }
        self.buckets[index].push((key, value));
        // `putVal`: `binCount >= TREEIFY_THRESHOLD - 1` ao ligar o nono nó. Com a tabela menor que 64 o `treeifyBin` só a dobra. Com 64 ou
        // mais o balde vira árvore no Java, e a ordem dela (a raiz na frente, o nó novo depois do pai) não é portada: aqui o balde segue
        // na ordem de chegada. Este mapa vive no cliente de quem joga, então não derruba o jogo como o `hash_set_order` faz num teste;
        // [`JavaIntMap::treeified`] diz que aconteceu.
        if self.buckets[index].len() > TREEIFY_THRESHOLD {
            if self.buckets.len() < MIN_TREEIFY_CAPACITY {
                self.double();
            } else {
                self.treeified = true;
            }
        }
        self.size += 1;
        if self.size > self.buckets.len() * 3 / 4 {
            self.double();
        }
        None
    }

    /// `HashMap.remove`: o nó sai do balde e a tabela fica do tamanho que estava.
    pub fn remove(&mut self, key: i32) -> Option<V> {
        if self.buckets.is_empty() {
            return None;
        }
        let index = self.index(key);
        let at = self.buckets[index]
            .iter()
            .position(|(other, _)| *other == key)?;
        self.size -= 1;
        Some(self.buckets[index].remove(at).1)
    }

    /// `HashMap.clear`: os baldes ficam vazios e a tabela do tamanho que estava.
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.size = 0;
    }

    /// A mesma tabela (o tamanho, os baldes e a ordem de cada um) com outro valor por chave.
    pub fn map<W>(&self, value: impl Fn(i32, &V) -> W) -> JavaIntMap<W> {
        JavaIntMap {
            buckets: self
                .buckets
                .iter()
                .map(|bucket| {
                    bucket
                        .iter()
                        .map(|(key, old)| (*key, value(*key, old)))
                        .collect()
                })
                .collect(),
            size: self.size,
            treeified: self.treeified,
        }
    }

    /// Algum balde chegou a nove nós numa tabela de 64 ou mais: no Java ele virou árvore, e a ordem deste mapa pode não ser a dele.
    pub fn treeified(&self) -> bool {
        self.treeified
    }

    /// `entrySet()` na ordem da tabela.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &V)> {
        self.buckets
            .iter()
            .flatten()
            .map(|(key, value)| (*key, value))
    }
}

/// A ordem em que um `Sets.newIdentityHashSet()` (um `IdentityHashMap` com a capacidade padrão) devolve as chaves depois de `add` em
/// `keys`, com `identity` o `System.identityHashCode` de cada uma. Porta o `IdentityHashMap` do OpenJDK 21 no que decide a ordem: a
/// tabela de 64 posições (32 chaves, uma a cada duas posições), o índice `((h << 1) - (h << 8)) & (length - 1)`, a sondagem de duas em
/// duas posições com volta ao começo, e o `resize` que dobra a tabela quando `3 * (size + 1)` passa do comprimento dela e reinsere
/// as chaves na ordem da tabela velha. Só `add`: quem tira chaves (`closeDeletion`) não é portado.
pub fn identity_set_order<K: Copy + PartialEq>(
    keys: impl IntoIterator<Item = K>,
    identity: impl Fn(K) -> i32,
) -> Vec<K> {
    fn slot(hash: i32, length: usize) -> usize {
        (hash.wrapping_shl(1).wrapping_sub(hash.wrapping_shl(8))) as u32 as usize & (length - 1)
    }
    fn place<K: Copy + PartialEq>(table: &mut [Option<(i32, K)>], hash: i32, key: K) -> bool {
        let length = table.len();
        let mut index = slot(hash, length);
        while let Some((_, other)) = table[index] {
            if other == key {
                return false;
            }
            index = if index + 2 < length { index + 2 } else { 0 };
        }
        table[index] = Some((hash, key));
        true
    }
    let mut table: Vec<Option<(i32, K)>> = vec![None; 64];
    let mut size = 0_usize;
    for key in keys {
        let hash = identity(key);
        // `put`: a chave que já está não muda nada; senão `s + (s << 1) > len` dobra a tabela antes de pôr.
        let length = table.len();
        let mut index = slot(hash, length);
        let mut present = false;
        while let Some((_, other)) = table[index] {
            if other == key {
                present = true;
                break;
            }
            index = if index + 2 < length { index + 2 } else { 0 };
        }
        if present {
            continue;
        }
        if 3 * (size + 1) > length {
            let mut next: Vec<Option<(i32, K)>> = vec![None; length * 2];
            for (old_hash, old_key) in table.iter().step_by(2).flatten() {
                place(&mut next, *old_hash, *old_key);
            }
            table = next;
        }
        place(&mut table, hash, key);
        size += 1;
    }
    table
        .into_iter()
        .step_by(2)
        .flatten()
        .map(|(_, key)| key)
        .collect()
}

/// Um `HashMap<K, V>` (ou o `HashSet<K>` que mora num, com `V = ()`) do Java para chaves quaisquer: quem chama dá o `hashCode` de cada
/// chave, e a igualdade é a do `equals` (o `PartialEq`). As regras da ordem são as de [`JavaIntMap`]: a tabela nasce com 16 no primeiro
/// `put`, dobra quando o tamanho passa de 0,75 dela e nunca encolhe, e cada balde fica na ordem de chegada.
#[derive(Clone, Debug)]
pub struct JavaHashMap<K, V> {
    buckets: Vec<Vec<(i32, K, V)>>,
    size: usize,
    treeified: bool,
}

impl<K, V> Default for JavaHashMap<K, V> {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            size: 0,
            treeified: false,
        }
    }
}

impl<K: PartialEq, V> JavaHashMap<K, V> {
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `table.length` (0 antes do primeiro `put`).
    pub fn capacity(&self) -> usize {
        self.buckets.len()
    }

    fn index(&self, hash: i32) -> usize {
        spread(hash) as usize & (self.buckets.len() - 1)
    }

    pub fn get(&self, hash: i32, key: &K) -> Option<&V> {
        if self.buckets.is_empty() {
            return None;
        }
        self.buckets[self.index(hash)]
            .iter()
            .find(|(h, k, _)| *h == hash && k == key)
            .map(|(_, _, value)| value)
    }

    pub fn get_mut(&mut self, hash: i32, key: &K) -> Option<&mut V> {
        if self.buckets.is_empty() {
            return None;
        }
        let index = self.index(hash);
        self.buckets[index]
            .iter_mut()
            .find(|(h, k, _)| *h == hash && k == key)
            .map(|(_, _, value)| value)
    }

    fn double(&mut self) {
        let old = self.buckets.len();
        let mut next: Vec<Vec<(i32, K, V)>> = (0..old * 2).map(|_| Vec::new()).collect();
        for (index, bucket) in std::mem::take(&mut self.buckets).into_iter().enumerate() {
            for (hash, key, value) in bucket {
                let target = if spread(hash) as usize & old == 0 {
                    index
                } else {
                    index + old
                };
                next[target].push((hash, key, value));
            }
        }
        self.buckets = next;
    }

    /// `HashMap.put`: devolve o valor que a chave tinha (a chave que já estava fica, como no Java).
    pub fn put(&mut self, hash: i32, key: K, value: V) -> Option<V> {
        if self.buckets.is_empty() {
            self.buckets = (0..16).map(|_| Vec::new()).collect();
        }
        let index = self.index(hash);
        if let Some(slot) = self.buckets[index]
            .iter_mut()
            .find(|(h, k, _)| *h == hash && *k == key)
        {
            return Some(std::mem::replace(&mut slot.2, value));
        }
        self.buckets[index].push((hash, key, value));
        if self.buckets[index].len() > TREEIFY_THRESHOLD {
            if self.buckets.len() < MIN_TREEIFY_CAPACITY {
                self.double();
            } else {
                self.treeified = true;
            }
        }
        self.size += 1;
        if self.size > self.buckets.len() * 3 / 4 {
            self.double();
        }
        None
    }

    /// `HashMap.remove`: o nó sai do balde e a tabela fica do tamanho que estava.
    pub fn remove(&mut self, hash: i32, key: &K) -> Option<V> {
        if self.buckets.is_empty() {
            return None;
        }
        let index = self.index(hash);
        let at = self.buckets[index]
            .iter()
            .position(|(h, k, _)| *h == hash && k == key)?;
        self.size -= 1;
        Some(self.buckets[index].remove(at).2)
    }

    /// `entrySet().removeIf` pelo iterador do `HashMap`: percorre na ordem da tabela, e quem sai não muda o lugar dos outros. Devolve as
    /// chaves que saíram, na ordem em que saíram.
    pub fn remove_if(&mut self, mut test: impl FnMut(&K, &mut V) -> bool) -> Vec<(i32, K)> {
        let mut removed = Vec::new();
        for bucket in &mut self.buckets {
            let mut kept = Vec::with_capacity(bucket.len());
            for (hash, key, mut value) in bucket.drain(..) {
                if test(&key, &mut value) {
                    removed.push((hash, key));
                } else {
                    kept.push((hash, key, value));
                }
            }
            *bucket = kept;
        }
        self.size -= removed.len();
        removed
    }

    /// `HashMap.clear`: os baldes ficam vazios e a tabela do tamanho que estava.
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.size = 0;
    }

    /// Algum balde chegou a nove nós numa tabela de 64 ou mais: no Java ele virou árvore, e a ordem deste mapa pode não ser a dele.
    pub fn treeified(&self) -> bool {
        self.treeified
    }

    /// `entrySet()` na ordem da tabela, com o `hashCode` de cada chave.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &K, &V)> {
        self.buckets
            .iter()
            .flatten()
            .map(|(hash, key, value)| (*hash, key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `HashMap` do Java com chaves `Integer`: 1, 17, 33 e 49 dividem o balde 1 de uma tabela de 16 na ordem em que chegam; tirar uma
    /// deixa as outras como estavam, trocar o valor não muda o lugar, e a tabela dobra ao passar de 12 (33 e 49 ficam no balde 1 de 32
    /// só se o bit 16 está apagado: 1 e 33 ficam, 17 e 49 vão para o 17).
    #[test]
    fn the_int_map_keeps_the_order_of_arrival_in_a_bucket() {
        let mut map = JavaIntMap::default();
        for key in [49, 1, 33, 17, 2] {
            map.put(key, key * 10);
        }
        assert_eq!(
            map.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            [49, 1, 33, 17, 2]
        );
        assert_eq!(map.put(1, 7), Some(10));
        assert_eq!(map.remove(1), Some(7));
        assert_eq!(
            map.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            [49, 33, 17, 2]
        );
        assert_eq!(map.remove(49), Some(490));
        assert_eq!(
            map.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            [33, 17, 2]
        );
        for key in 100..110 {
            map.put(key, 0);
        }
        // 13 chaves: a tabela dobrou para 32.
        assert_eq!(map.capacity(), 32);
        let order: Vec<i32> = map.iter().map(|(key, _)| key).collect();
        assert!(order.iter().position(|&key| key == 33) < order.iter().position(|&key| key == 17));
        map.clear();
        assert_eq!((map.len(), map.capacity()), (0, 32));
        assert!(!map.treeified());
    }

    // Saídas do `RefHash.java` da verificação: o mesmo `HashSet` no JDK 21 (Temurin 21.0.12), com chaves `Vec3i` e a semente
    // dada. Os casos 2, 3 e 5 passam pelo `resize` e por baldes com mais de um nó.
    const SEED_1: &str = "-4,61,2 -1,60,-1 0,60,-1 -7,60,-2 0,61,2 3,62,0 3,60,0 8,61,1 10,62,0 11,60,-1 15,61,-2 14,60,-1 14,61,0 -8,61,1 -7,62,2 -5,62,1";
    const SEED_2: &str = "0,62,-2 -3,60,-2 -4,60,1 -2,60,-1 -1,62,0 -6,60,-2 3,62,-1 4,62,-2 2,62,0 2,62,1 1,62,2 1,60,1 0,60,2 3,60,0 8,62,-1 6,62,1 8,62,1 10,62,0 9,60,1 12,60,0 15,62,-1 11,61,-2 5,61,-2 -6,61,2 -4,61,0 -1,61,-2 -2,61,-1 -5,61,2 -1,61,-1 -3,61,-2 1,61,1 4,61,-1 7,61,0 10,61,-1 11,61,0 10,61,1 13,60,-2 -7,62,-1 -8,60,0 -7,60,0";
    const SEED_3: &str = "-1,62,-2 0,62,-2 2,62,-2 -1,62,2 3,62,-2 0,62,2 -1,60,1 -6,62,-2 1,62,2 0,60,1 -7,62,-2 1,60,1 2,60,1 8,62,-2 4,60,1 5,62,2 10,62,-2 6,60,1 7,60,1 9,62,2 13,62,-2 14,62,-2 15,62,-2 12,62,2 14,61,-2 13,60,1 6,61,-2 4,61,-2 3,61,-2 -7,61,1 -2,61,-2 -4,61,1 -3,61,1 -3,61,-2 0,61,1 2,61,1 3,61,1 4,61,1 14,60,-2 12,60,-2 14,61,1 -1,60,-2 -5,62,1 -2,60,-2 -1,62,1 -6,60,-2 1,62,1 1,60,0 2,60,0 7,62,1 8,62,1 10,60,0 12,60,0 14,60,0 -7,61,0 -2,61,0 3,61,0 7,61,0 8,61,0 9,61,0 10,61,0 11,61,0 -7,62,0 -5,62,0 -6,60,-1 -4,60,-1 0,60,-1 4,60,-1 5,60,-1 6,60,-1 10,62,0 11,60,-1 12,60,-1 14,60,-1 -6,61,-1 -1,61,-1 1,61,-1 4,61,-1 8,61,-1 9,61,-1 10,61,-1 13,61,-1 -4,62,-1 -3,60,2 2,62,-1 -2,60,2 7,62,-1 4,60,2 6,60,2 14,62,-1 12,60,2 13,60,2 14,60,2 -7,61,2 -4,61,2 10,61,2 11,61,2 14,61,2 -7,62,2 -6,62,2";
    const SEED_4: &str = "12,60,0 -4,61,-2 0,61,-1 -4,62,-2 9,60,-2 2,61,1 5,61,-1 -8,62,-2 5,61,2 11,62,0 -3,61,-1 -6,60,1";
    const SEED_5: &str = "3,62,-2 4,62,-2 1,60,-1 3,62,0 0,60,2 3,60,0 6,62,0 3,60,1 9,62,-2 6,62,1 10,62,-2 7,60,-1 7,60,0 6,60,1 9,62,0 7,60,1 12,62,1 13,61,-2 -8,61,-1 -1,61,-2 -3,61,0 -2,61,-1 2,61,1 1,61,2 6,61,-1 8,61,1 10,61,2 14,61,-1 -8,62,0 -6,60,0";

    type Key = (i32, i32, i32);

    /// O `hashCode` do `Vec3i`: `(y + z * 31) * 31 + x`.
    fn vec3i_hash((x, y, z): Key) -> i32 {
        y.wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x)
    }

    /// O gerador do `RefHash.java`: `state = state * 6364136223846793005 + 1442695040888963407`.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % bound
        }
    }

    fn keys(seed: u64, count: usize) -> Vec<Key> {
        let mut lcg = Lcg(seed * 11);
        let mut keys: Vec<Key> = Vec::new();
        while keys.len() < count {
            let key = (
                lcg.next(24) as i32 - 8,
                60 + lcg.next(3) as i32,
                lcg.next(5) as i32 - 2,
            );
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    fn text(order: &[Key]) -> String {
        order
            .iter()
            .map(|(x, y, z)| format!("{x},{y},{z}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_ordem_confere_com_o_jdk() {
        for (seed, count, capacity, expected) in [
            (1, 16, 16, SEED_1),
            (2, 40, 16, SEED_2),
            (3, 100, 16, SEED_3),
            (4, 12, 16, SEED_4),
            (5, 30, 17, SEED_5),
        ] {
            let order = hash_set_order(keys(seed, count), vec3i_hash, capacity);
            assert_eq!(text(&order), expected, "semente {seed}");
        }
    }

    #[test]
    fn a_capacidade_inicial_segue_o_construtor_de_colecao() {
        assert_eq!(initial_capacity_for(0), 16);
        assert_eq!(initial_capacity_for(12), 17);
        assert_eq!(initial_capacity_for(48), 65);
    }

    /// Dezesseis cabos seguidos, como os do chunk do rastro `chunk_reload_intermediary`: `saveAdditional` sai de dois em dois,
    /// trocados, porque `h ^ h >>> 16` mexe nos bits baixos.
    #[test]
    fn cabos_em_linha_saem_trocados_de_dois_em_dois() {
        let order = hash_set_order(
            (0..16).map(|i| (-14_138_339 + 19 + i, 5, 12_913_922)),
            vec3i_hash,
            16,
        );
        let xs: Vec<i32> = order.iter().map(|p| p.0 + 14_138_339).collect();
        assert_eq!(
            xs,
            vec![
                34, 32, 33, 30, 31, 28, 29, 26, 27, 24, 25, 22, 23, 20, 21, 19
            ]
        );
    }
}
