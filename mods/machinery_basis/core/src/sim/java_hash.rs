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
    table
        .buckets
        .into_iter()
        .flatten()
        .map(|(_, key)| key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
