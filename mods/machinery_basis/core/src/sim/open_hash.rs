//! A ordem de iteração do `Object2ObjectOpenHashMap` e do `LongOpenHashSet` do fastutil 8.5.18, o que o Minecraft usa em
//! `ChunkAccess.blockEntities` e em `ChunkMap.toDrop`. Quem percorre esses mapas vê as chaves na ordem da tabela, não na da
//! inserção, e o jogo registra os tickers e chama `onChunkUnloaded` nessa ordem.
//!
//! Porta `Object2ObjectOpenHashMap` (`find`, `insert`, `removeEntry`, `shiftKeys`, `rehash`) e o iterador sem remoção, que
//! percorre a tabela do fim para o começo e devolve antes a chave nula (a chave `0` das tabelas de `long`).
//! `ordem_confere_com_o_fastutil` compara o resultado com a saída do próprio fastutil 8.5.18.

/// Uma chave de tabela do fastutil: o `HashCommon.mix` do seu `hashCode` (objeto) ou da própria chave (`long`).
pub trait OpenKey: Copy + Eq {
    /// Os 32 bits baixos de `HashCommon.mix`: o índice da tabela é esse valor com a máscara.
    fn mixed(self) -> i32;
    /// A chave que o fastutil guarda à parte (`containsNullKey`): a `0` das tabelas de `long`. Os objetos não têm.
    const NULL_KEY: Option<Self> = None;
}

/// `HashCommon.INT_PHI`.
const INT_PHI: i32 = 0x9E37_79B9_u32 as i32;
/// `HashCommon.LONG_PHI`.
const LONG_PHI: i64 = 0x9E37_79B9_7F4A_7C15_u64 as i64;
/// `Hash.DEFAULT_INITIAL_SIZE`.
const DEFAULT_INITIAL_SIZE: usize = 16;

/// `HashCommon.mix(int)`, sobre o `hashCode` do objeto.
pub fn mix_int(hash_code: i32) -> i32 {
    let h = hash_code.wrapping_mul(INT_PHI);
    h ^ (((h as u32) >> 16) as i32)
}

/// `HashCommon.mix(long)`, truncado aos 32 bits que o índice usa.
pub fn mix_long(key: i64) -> i32 {
    let mut h = key.wrapping_mul(LONG_PHI);
    h ^= ((h as u64) >> 32) as i64;
    (h ^ (((h as u64) >> 16) as i64)) as i32
}

/// `HashCommon.arraySize(expected, 0.75f)`.
fn array_size(expected: usize) -> usize {
    (expected * 4).div_ceil(3).next_power_of_two().max(2)
}

/// `HashCommon.maxFill(n, 0.75f)`.
fn max_fill(n: usize) -> usize {
    (n * 3).div_ceil(4).min(n - 1)
}

/// O mapa, só com o que muda a ordem: a tabela de chaves e a de valores, com sondagem linear.
#[derive(Debug, Clone)]
pub struct OpenHashMap<K: OpenKey, V> {
    keys: Vec<Option<K>>,
    values: Vec<Option<V>>,
    /// O valor da chave nula, que o fastutil guarda na posição `n` da tabela.
    null_value: Option<V>,
    mask: usize,
    max_fill: usize,
    min_n: usize,
    /// `size`, que conta a chave nula.
    size: usize,
}

impl<K: OpenKey, V> Default for OpenHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: OpenKey, V> OpenHashMap<K, V> {
    /// O construtor sem argumentos: `n = arraySize(16, .75f)`, que dá 32.
    pub fn new() -> Self {
        let n = array_size(DEFAULT_INITIAL_SIZE);
        Self {
            keys: vec![None; n],
            values: (0..n).map(|_| None).collect(),
            null_value: None,
            mask: n - 1,
            max_fill: max_fill(n),
            min_n: n,
            size: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn n(&self) -> usize {
        self.keys.len()
    }

    fn slot(&self, key: K) -> usize {
        (key.mixed() as u32 as usize) & self.mask
    }

    /// `find`: a posição da chave, ou a posição vazia onde ela entraria.
    fn find(&self, key: K) -> Result<usize, usize> {
        let mut pos = self.slot(key);
        loop {
            match self.keys[pos] {
                None => return Err(pos),
                Some(found) if found == key => return Ok(pos),
                Some(_) => pos = (pos + 1) & self.mask,
            }
        }
    }

    pub fn contains_key(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    pub fn get(&self, key: K) -> Option<&V> {
        if Some(key) == K::NULL_KEY {
            return self.null_value.as_ref();
        }
        self.find(key)
            .ok()
            .and_then(|pos| self.values[pos].as_ref())
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        if Some(key) == K::NULL_KEY {
            return self.null_value.as_mut();
        }
        match self.find(key) {
            Ok(pos) => self.values[pos].as_mut(),
            Err(_) => None,
        }
    }

    /// `put`: troca o valor de uma chave que já está, sem mudar o lugar dela.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if Some(key) == K::NULL_KEY {
            let previous = self.null_value.replace(value);
            if previous.is_none() {
                self.grew();
            }
            return previous;
        }
        match self.find(key) {
            Ok(pos) => self.values[pos].replace(value),
            Err(pos) => {
                self.keys[pos] = Some(key);
                self.values[pos] = Some(value);
                self.grew();
                None
            }
        }
    }

    /// O fim de `insert`: `if (size++ >= maxFill) rehash(arraySize(size + 1, f))`.
    fn grew(&mut self) {
        let before = self.size;
        self.size += 1;
        if before >= self.max_fill {
            self.rehash(array_size(self.size + 1));
        }
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        let removed = if Some(key) == K::NULL_KEY {
            let value = self.null_value.take()?;
            self.size -= 1;
            value
        } else {
            let pos = self.find(key).ok()?;
            let value = self.values[pos].take();
            self.size -= 1;
            self.shift_keys(pos);
            value?
        };
        let n = self.n();
        if n > self.min_n && self.size < self.max_fill / 4 && n > DEFAULT_INITIAL_SIZE {
            self.rehash(n / 2);
        }
        Some(removed)
    }

    /// `shiftKeys`: a remoção por deslocamento que a sondagem linear pede.
    fn shift_keys(&mut self, mut pos: usize) {
        loop {
            let last = pos;
            pos = (last + 1) & self.mask;
            loop {
                let Some(current) = self.keys[pos] else {
                    self.keys[last] = None;
                    self.values[last] = None;
                    return;
                };
                let slot = self.slot(current);
                let stays = if last <= pos {
                    last >= slot || slot > pos
                } else {
                    last >= slot && slot > pos
                };
                if stays {
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            self.keys[last] = self.keys[pos];
            self.values[last] = self.values[pos].take();
        }
    }

    /// `rehash(newN)`: lê a tabela de trás para a frente e reinsere com sondagem linear.
    fn rehash(&mut self, new_n: usize) {
        let old_keys = std::mem::replace(&mut self.keys, vec![None; new_n]);
        let mut old_values =
            std::mem::replace(&mut self.values, (0..new_n).map(|_| None).collect());
        self.mask = new_n - 1;
        self.max_fill = max_fill(new_n);
        for i in (0..old_keys.len()).rev() {
            let Some(key) = old_keys[i] else { continue };
            let mut pos = self.slot(key);
            while self.keys[pos].is_some() {
                pos = (pos + 1) & self.mask;
            }
            self.keys[pos] = Some(key);
            self.values[pos] = old_values[i].take();
        }
    }

    /// As chaves na ordem do iterador do fastutil: a nula primeiro, depois a tabela do fim para o começo.
    pub fn keys_in_order(&self) -> Vec<K> {
        let mut keys = Vec::with_capacity(self.size);
        if self.null_value.is_some() {
            keys.extend(K::NULL_KEY);
        }
        keys.extend(self.keys.iter().rev().flatten().copied());
        keys
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::chunks::{BlockPos, ChunkPos};

    // Saídas do `Ref.java` da verificação: as mesmas operações no fastutil 8.5.18 real (`java -cp fastutil-8.5.18.jar`).
    const LINE_5: &str = "1 3 0 2 4";
    const LINE_30: &str =
        "3 28 7 24 15 16 19 23 20 0 2 6 4 12 14 13 8 9 11 10 1 26 5 17 18 22 21 29 25 27";
    const LINE_100: &str =
        "28 77 44 60 61 45 11 12 13 92 93 95 10 94 59 79 80 63 62 64 76 78 75 74 7 65 98 90 15 57 82 23 41 47 55 49 9
    96 17 43 39 21 35 33 37 29 25 27 31 3 69 36 85 19 86 20 2 4 51 53 54 1 52 67 71 5 88 84 87 83 68 66 70 72 73
    32 24 16 81 40 0 99 97 6 56 58 89 48 50 14 91 8 34 38 26 46 18 22 42 30";
    const RANDOM_1: &str =
        "10,61,-1 11,60,2 -8,60,-1 15,61,-2 6,61,0 -5,62,-1 5,61,0 12,60,-2 -3,61,2 -8,62,0 11,61,-1 3,62,1 9,60,-1
    7,60,-1 10,60,2 12,60,2 9,61,-1 14,61,-2 15,60,1 -1,62,2 -2,62,2 -2,60,-2 14,62,-1 -6,61,2 11,61,2 10,62,-1
    10,60,-2 -3,62,-1 7,61,0 1,60,1 13,62,-1 -4,60,1 14,61,2 -2,60,1 14,60,-2 -1,62,-1 9,61,2 6,61,-1 1,61,0
    -2,61,-1 -3,61,1 -6,61,-2 9,61,0 11,62,2 13,61,1 14,60,2 -5,62,2 4,60,1 -4,62,0 5,60,-1 -7,61,2 11,60,-1
    1,62,2 -3,60,-1 3,62,2 7,60,0 10,61,-2 12,61,-1 -1,61,1 -8,60,2 13,62,2 -7,60,0 9,62,2 -5,61,1 4,60,2 -2,62,0
    4,61,-1 1,62,-2 4,62,0 2,62,0 5,62,-2 -6,61,-1 10,62,0 -4,61,-1 1,61,1 8,62,0 -7,61,1 6,61,-2 12,60,0 14,61,0
    2,60,2 7,62,2 6,62,2 1,61,-1 9,62,-2 -8,61,1 -6,62,1 5,60,-2 15,61,0 8,60,1 13,60,0 -7,62,1 11,60,0 7,61,-2
    -8,62,2 -3,62,1 -2,60,-1 12,61,0 7,60,-2 5,61,2 7,62,-1 7,62,1 3,61,2 1,61,2 -6,60,1 6,61,2 3,62,-1 1,62,-1
    5,62,-1 0,61,0 -1,61,0 -2,61,0 -5,62,0 -7,61,0 3,60,1 12,60,-1 3,62,0 4,62,-2 12,62,-2 -8,62,-1 14,61,1
    -7,60,-2 6,60,0 11,60,1 2,60,-1 9,61,-2 -1,60,-2 -3,62,0 2,62,-2 6,62,-2 0,62,2 5,62,0 -6,62,-2 5,60,2 8,62,2
    14,62,-2 14,60,-1 12,61,1 10,62,-2 6,61,1 -6,60,0 4,61,1 -1,61,-1 15,62,0 1,60,2 -8,60,0 11,62,0 13,62,0
    7,62,0 -5,60,2 -3,60,2 0,62,-2 9,62,0";
    const RANDOM_3: &str =
        "10,61,-1 -2,61,-2 13,61,2 3,60,0 -4,62,-1 13,60,-2 6,61,0 2,62,1 11,61,-1 -5,60,-2 -3,60,-2 2,60,0 9,61,-1
    14,61,-2 15,60,1 10,60,-2 -2,62,2 -2,60,-2 1,60,1 14,62,-1 11,61,2 -5,61,2 15,61,2 -3,60,1 -2,61,2 15,60,-2
    1,62,1 -5,61,-2 -6,60,-1 4,62,1 -7,60,-1 -8,61,0 0,60,1 -7,62,-1 15,62,-1 13,62,-1 11,62,-1 6,62,1 -2,60,1
    4,61,0 12,61,2 6,61,-1 5,61,1 2,61,-2 1,61,0 -3,61,1 9,61,0 11,62,2 -5,60,-1 1,62,2 13,60,-1 2,60,1 -3,60,-1
    -7,61,2 5,60,-1 14,60,2 15,60,0 3,62,2 -2,62,1 -8,62,1 9,60,0 0,60,-2 10,61,-2 12,61,-1 -6,60,-2 0,61,-1
    8,61,-1 -8,60,2 -7,60,0 14,62,0 15,62,-2 15,60,-1 4,61,-1 5,62,2 -2,62,0 8,60,2 11,62,-2 -3,60,0 -3,62,-2
    -7,62,-2 -6,61,-1 -8,61,-1 12,62,0 0,60,2 -2,60,2 -4,61,-1 -6,60,2 1,61,1 3,61,1 9,61,1 0,60,-1 2,60,2 7,60,1
    3,60,2 6,62,2 1,61,-1 9,62,-2 -1,62,0 3,60,-2 15,61,0 5,61,-2 5,60,-2 13,61,0 -6,62,1 10,60,0 11,60,0 5,60,1
    9,60,1 14,60,0 -5,61,0 4,61,-2 -3,62,1 -2,60,-1 6,60,-2 12,61,0 7,60,-2 11,61,0 12,62,1 7,62,-1 2,62,-1
    -2,61,0 4,62,-1 6,61,2 -1,61,0 5,62,-1 -6,60,1 0,61,0 3,62,-1 -3,61,0 7,61,2 3,60,1 4,62,-2 -1,62,1 -8,61,2
    -4,60,-1 10,61,0 0,61,2 -8,62,-1 10,62,2 5,61,-1 14,61,1 7,60,2 -7,60,-2 13,61,-1 1,60,-2 3,61,-2 6,60,0
    13,60,1 2,60,-1 -1,60,-2 -2,62,-2 6,60,-1 13,60,2 -7,62,0 8,62,2 5,60,2 14,60,-1 4,61,1 -6,60,0 12,62,2
    -5,61,-1 -8,60,0 11,62,0 -1,60,2 7,62,0 9,62,0 -5,60,2 -3,60,2 -7,61,-1 8,60,-1";
    const LONG_1: &str =
        "0,4 -13,-14 -3,19 -4,6 12,14 -2,-19 -19,-2 1,10 12,-11 8,-7 7,19 15,16 18,11 19,-7 -20,7 -18,-7 -5,13 13,-10
    13,-6 0,15 3,-19 -6,-3 -15,-8 4,-16 12,17 15,-7 11,-2 14,-12 -15,-6 -15,18 -6,11 -1,16 -18,-15 12,-15 -7,5
    13,5 2,11 -14,15 12,-20 3,-4 -15,-19 14,-2";
    const LONG_2: &str =
        "9,-15 -8,7 15,-2 7,9 14,-15 -17,-19 -9,-16 12,-19 -14,-17 19,17 -8,-6 -13,-19 -13,-15 4,13 1,13 -3,-12 7,-9
    5,-12 4,3 -4,-4 -2,-3 0,3 -16,-15 -19,13 -14,-1 9,-10 19,10 -12,0 -10,15 19,2 -20,18 -1,-8 13,-13 10,-3 -7,-1
    10,15 3,14 -16,0 13,-5 13,-9 -20,-2 11,6 8,10 -18,8 -15,-11";

    /// O gerador do `Ref.java`: `state = state * 6364136223846793005 + 1442695040888963407`.
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

    /// As saídas longas vêm quebradas em linhas.
    fn squash(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn joined(items: impl Iterator<Item = String>) -> String {
        items.collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn cabos_em_linha_tem_a_ordem_do_fastutil() {
        for (count, expected) in [(5, LINE_5), (30, LINE_30), (100, LINE_100)] {
            let mut map = OpenHashMap::new();
            for x in 0..count {
                map.insert(BlockPos::new(x, 64, 0), x);
            }
            let order = map.keys_in_order();
            assert_eq!(
                joined(order.iter().map(|pos| pos.x.to_string())),
                squash(expected),
                "{count} cabos"
            );
            assert_eq!(map.len(), count as usize);
        }
    }

    #[test]
    fn ordem_confere_com_o_fastutil() {
        for (seed, expected) in [(1, RANDOM_1), (3, RANDOM_3)] {
            let mut lcg = Lcg(seed);
            let mut map = OpenHashMap::new();
            for i in 0..400 {
                let pos = BlockPos::new(
                    lcg.next(24) as i32 - 8,
                    60 + lcg.next(3) as i32,
                    lcg.next(5) as i32 - 2,
                );
                if lcg.next(3) == 0 {
                    map.remove(pos);
                } else {
                    map.insert(pos, i);
                }
            }
            let order = map.keys_in_order();
            assert_eq!(
                joined(order.iter().map(|p| format!("{},{},{}", p.x, p.y, p.z))),
                squash(expected),
                "semente {seed}"
            );
            assert_eq!(map.len(), order.len());
        }
    }

    #[test]
    fn conjunto_de_longs_tem_a_ordem_do_fastutil_e_a_chave_zero_vem_antes() {
        for (seed, expected) in [(1, LONG_1), (2, LONG_2)] {
            let mut lcg = Lcg(seed * 7);
            let mut set = OpenHashMap::new();
            for _ in 0..60 {
                let chunk = ChunkPos::new(lcg.next(40) as i32 - 20, lcg.next(40) as i32 - 20);
                if lcg.next(4) == 0 {
                    set.remove(chunk);
                } else {
                    set.insert(chunk, ());
                }
            }
            assert_eq!(
                joined(
                    set.keys_in_order()
                        .iter()
                        .map(|c| format!("{},{}", c.x, c.z))
                ),
                squash(expected),
                "semente {seed}"
            );
        }
        // `Ref2.java`: seis chunks com o `0,0` entre eles, e de novo sem ele.
        let mut set = OpenHashMap::new();
        for (x, z) in [(3, 4), (0, 0), (-1, 2), (7, 7), (0, 1), (1, 0)] {
            set.insert(ChunkPos::new(x, z), ());
        }
        let text = |set: &OpenHashMap<ChunkPos, ()>| {
            joined(
                set.keys_in_order()
                    .iter()
                    .map(|c| format!("{},{}", c.x, c.z)),
            )
        };
        assert_eq!(
            text(&set),
            "0,0 0,1 -1,2 7,7 1,0 3,4",
            "a chave nula é a primeira do iterador"
        );
        assert!(set.contains_key(ChunkPos::new(0, 0)));
        assert!(set.remove(ChunkPos::new(0, 0)).is_some());
        assert!(!set.contains_key(ChunkPos::new(0, 0)));
        assert_eq!(text(&set), "0,1 -1,2 7,7 1,0 3,4");
        assert_eq!(set.len(), 5);
    }
}
