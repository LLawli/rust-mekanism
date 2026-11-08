//! `java.util.Random` e `Math.random()`. O Mekanism sorteia a parte fracionária da experiência com `Math.random()`
//! (`AbstractFurnaceBlockEntity.createExperience`), que é o `nextDouble()` de um `Random` global semeado uma vez na primeira
//! chamada. Não há semente a reproduzir (ela vem de `System.nanoTime()`), mas a distribuição é a de um gerador congruencial
//! de 48 bits: uniforme em `[0, 1)` com 53 bits. Este é o mesmo gerador, para a distribuição ser a do original e não a de uma
//! fração de relógio.
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Mutex;

/// `Random.multiplier`, `Random.addend` e `Random.mask` (2^48 - 1).
const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const ADDEND: u64 = 0xB;
const MASK: u64 = (1 << 48) - 1;
/// `DOUBLE_UNIT`: `0x1.0p-53`.
const DOUBLE_UNIT: f64 = 1.0 / (1_u64 << 53) as f64;

/// `java.util.Random`.
#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    /// `new Random(seed)`: `initialScramble`.
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed as u64 ^ MULTIPLIER) & MASK,
        }
    }

    /// `Random.next(bits)`, as `bits` mais altas dos 48 do estado, com sinal como o `int` do Java.
    pub fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.seed >> (48 - bits)) as i64 as i32
    }

    /// `Random.nextDouble()`: `(((long) next(26) << 27) + next(27)) * DOUBLE_UNIT`.
    pub fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26)) << 27;
        let low = i64::from(self.next(27));
        (high + low) as f64 * DOUBLE_UNIT
    }
}

/// `Math.RandomNumberGeneratorHolder.randomNumberGenerator`: um só para o processo, criado na primeira chamada.
static MATH_RANDOM: Mutex<Option<JavaRandom>> = Mutex::new(None);

/// Uma semente que muda a cada execução, como `seedUniquifier() ^ System.nanoTime()`: a entropia do `RandomState` da biblioteca
/// padrão (sorteada pelo sistema operacional) somada ao relógio.
fn entropy_seed() -> i64 {
    let mut hasher = RandomState::new().build_hasher();
    if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(since.as_nanos());
    }
    hasher.finish() as i64
}

/// `Math.random()`: uniforme em `[0, 1)`.
pub fn math_random() -> f64 {
    let mut generator = MATH_RANDOM.lock().unwrap_or_else(|e| e.into_inner());
    generator
        .get_or_insert_with(|| JavaRandom::new(entropy_seed()))
        .next_double()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saídas do `java.util.Random` do JDK 21 (`jshell`), com a semente dada.
    #[test]
    fn matches_the_jdk_sequences() {
        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_double(), 0.7275636800328681);
        assert_eq!(random.next_double(), 0.6832234717598454);
        assert_eq!(JavaRandom::new(0).next_double(), 0.730967787376657);
        let mut random = JavaRandom::new(-8_765_432_123);
        assert_eq!(random.next_double(), 0.1152330064905468);
        assert_eq!(random.next_double(), 0.5066467026661889);
        assert_eq!(random.next_double(), 0.24980736622565713);
    }

    /// `Math.random()` é uniforme em `[0, 1)`: 200.000 sorteios em dez faixas, cada uma perto de 10%, e a média perto de 0,5.
    /// Com 20.000 esperados por faixa o desvio é de cerca de 134; o teto de 600 (4,5 desvios) só estoura com um gerador torto.
    #[test]
    fn math_random_is_uniform() {
        let mut bins = [0_u32; 10];
        let mut sum = 0.0;
        for _ in 0..200_000 {
            let value = math_random();
            assert!((0.0..1.0).contains(&value));
            bins[(value * 10.0) as usize] += 1;
            sum += value;
        }
        for (bin, count) in bins.iter().enumerate() {
            assert!(count.abs_diff(20_000) < 600, "faixa {bin}: {count}");
        }
        assert!((sum / 200_000.0 - 0.5).abs() < 0.005, "média {}", sum / 200_000.0);
    }

    /// Sorteios seguidos são independentes: a diferença média entre dois seguidos de dois uniformes independentes é 1/3. Uma fração do
    /// relógio (o que o port usava) anda uns poucos nanossegundos entre duas chamadas e daria cerca de 0,0001.
    #[test]
    fn consecutive_draws_are_independent() {
        let mut previous = math_random();
        let mut total = 0.0;
        for _ in 0..100_000 {
            let value = math_random();
            total += (value - previous).abs();
            previous = value;
        }
        let mean = total / 100_000.0;
        assert!((mean - 1.0 / 3.0).abs() < 0.005, "diferença média {mean}");
    }

    /// A chance do ponto a mais de `createExperience` é a fração: 0,3 de experiência dá 1 ponto em 30% das remessas.
    #[test]
    fn the_extra_point_follows_the_fraction() {
        let drawn = (0..100_000)
            .filter(|_| crate::experience::experience_points(1, 0.3, math_random()) == 1)
            .count();
        assert!(drawn.abs_diff(30_000) < 700, "{drawn}");
    }
}
