//! `System.identityHashCode` da chave de um id de receita. `RecipeHolder.hashCode()` é `id.hashCode()` (`RecipeHolder.java:19-20`) e
//! `ResourceKey` não sobrescreve `hashCode`: vale o de identidade do JVM, que o HotSpot sorteia (31 bits, nunca zero) na primeira
//! vez que o objeto o pede. O `ResourceKey` de um id é internado (`ResourceKey.create`), então o mesmo id tem o mesmo hash por todas
//! as cargas de dados de uma execução, e outra execução sorteia outro. Aqui cada id sorteia o seu uma vez por processo.
use crate::java_random::{JavaRandom, entropy_seed};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// O gerador e os hashes já dados.
struct Identities {
    random: JavaRandom,
    given: HashMap<String, i32>,
}

static IDENTITIES: Mutex<Option<Identities>> = Mutex::new(None);

/// O `System.identityHashCode` da chave do id: sorteado na primeira pergunta e o mesmo depois.
pub fn identity_hash(id: &str) -> i32 {
    let mut guard = IDENTITIES.lock().unwrap_or_else(|e| e.into_inner());
    let identities = guard.get_or_insert_with(|| Identities {
        random: JavaRandom::new(entropy_seed()),
        given: HashMap::new(),
    });
    if let Some(hash) = identities.given.get(id) {
        return *hash;
    }
    // Os 31 bits do cabeçalho do objeto, sem o zero (que quer dizer "ainda sem hash").
    let hash = identities.random.next(31).max(1);
    identities.given.insert(id.to_owned(), hash);
    hash
}

thread_local! {
    /// Os hashes que um teste manda sortear a seguir, na ordem (os que o original deu, gravados no rastro).
    static FEED: RefCell<VecDeque<i32>> = const { RefCell::new(VecDeque::new()) };
}

/// Os próximos `next_identity_hash` desta thread devolvem estes valores, na ordem, antes de voltar a sortear.
pub fn feed_identity_hashes(hashes: impl IntoIterator<Item = i32>) {
    FEED.with_borrow_mut(|queue| queue.extend(hashes));
}

/// Quantos hashes dados ainda não foram usados.
pub fn pending_fed_identity_hashes() -> usize {
    FEED.with_borrow(VecDeque::len)
}

/// O `System.identityHashCode` de um objeto novo que não tem chave (uma pilha do transportador): sorteado como o HotSpot, 31 bits sem
/// o zero, e diferente a cada chamada.
pub fn next_identity_hash() -> i32 {
    if let Some(hash) = FEED.with_borrow_mut(VecDeque::pop_front) {
        return hash;
    }
    let mut guard = IDENTITIES.lock().unwrap_or_else(|e| e.into_inner());
    let identities = guard.get_or_insert_with(|| Identities {
        random: JavaRandom::new(entropy_seed()),
        given: HashMap::new(),
    });
    identities.random.next(31).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_keeps_its_hash_and_hashes_are_positive() {
        let first = identity_hash("identity_test:a");
        assert_eq!(first, identity_hash("identity_test:a"));
        assert!(first > 0);
    }

    /// 20.000 ids: o bit baixo e o do meio saem 50% cada, e os baldes de uma tabela de 16 ficam equilibrados.
    #[test]
    fn hashes_spread_over_the_buckets() {
        let mut buckets = [0_u32; 16];
        for i in 0..20_000 {
            let hash = identity_hash(&format!("identity_test:spread/{i}")) as u32;
            buckets[((hash ^ (hash >> 16)) & 15) as usize] += 1;
        }
        for (bucket, count) in buckets.iter().enumerate() {
            assert!(count.abs_diff(1250) < 200, "balde {bucket}: {count}");
        }
    }
}
