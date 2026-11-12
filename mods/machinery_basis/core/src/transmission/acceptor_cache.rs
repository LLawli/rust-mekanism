//! `AcceptorCache`: o cache, por lado, do aceitador que o vizinho oferece a um transmissor.
//!
//! No original cada lado tem um `BlockCapabilityCache` (a capability do vizinho vista pelo lado
//! oposto), que guarda o valor até a capability ser invalidada e o procura de novo na próxima
//! leitura. Aqui a capability vem de uma função do mundo (`lookup`), e a invalidação é o
//! [`AcceptorCache::invalidate`], que o jogo chama quando o `BlockCapabilityCache` avisaria.

use super::connection::{connection_map_contains_side, face_ordinal};
use crate::sides::Face;
use std::cell::RefCell;

/// `AcceptorCache.DEFAULT_ACCEPTOR_CONNECTIONS`.
pub const DEFAULT_ACCEPTOR_CONNECTIONS: u8 = 0;

/// O valor guardado de um lado: `Unknown` até a primeira leitura e depois de uma invalidação.
#[derive(Clone, Debug)]
enum Slot<A> {
    Unknown,
    Known(Option<A>),
}

#[derive(Clone, Debug)]
pub struct AcceptorCache<A> {
    /// `currentAcceptorConnections`: os lados em que há um aceitador.
    pub current_acceptor_connections: u8,
    /// Vazio até `initializeCache`; depois, um `BlockCapabilityCache` por lado.
    slots: Option<RefCell<[Slot<A>; 6]>>,
}

impl<A: Clone> Default for AcceptorCache<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Clone> AcceptorCache<A> {
    pub fn new() -> Self {
        AcceptorCache {
            current_acceptor_connections: DEFAULT_ACCEPTOR_CONNECTIONS,
            slots: None,
        }
    }

    /// `initializeCache(level)`: cria os seis caches, uma vez só (com os caches já criados não faz
    /// nada, como o `cachedAcceptors.isEmpty()` do original).
    pub fn initialize_cache(&mut self) {
        if self.slots.is_none() {
            self.slots = Some(RefCell::new(std::array::from_fn(|_| Slot::Unknown)));
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.slots.is_some()
    }

    /// `getCachedAcceptor(side)`: o aceitador, só se o mapa de conexões tem o lado.
    pub fn get_cached_acceptor(&self, side: Face, lookup: &dyn Fn(Face) -> Option<A>) -> Option<A> {
        if connection_map_contains_side(self.current_acceptor_connections, side) {
            self.get_connected_acceptor(side, lookup)
        } else {
            None
        }
    }

    /// `getConnectedAcceptors(sides)`: os aceitadores dos lados dados, na ordem dada, sem os nulos.
    pub fn get_connected_acceptors(
        &self,
        sides: &[Face],
        lookup: &dyn Fn(Face) -> Option<A>,
    ) -> Vec<A> {
        sides
            .iter()
            .filter_map(|side| self.get_connected_acceptor(*side, lookup))
            .collect()
    }

    /// `getConnectedAcceptor(side)`: o aceitador do cache, sem olhar o mapa de conexões. Sem
    /// `initializeCache` (o lado não está no mapa do original) é nulo. `lookup` é a capability do
    /// vizinho; só é chamada quando o cache não tem o valor.
    pub fn get_connected_acceptor(
        &self,
        side: Face,
        lookup: &dyn Fn(Face) -> Option<A>,
    ) -> Option<A> {
        let slots = self.slots.as_ref()?;
        let index = face_ordinal(side);
        if let Slot::Known(value) = &slots.borrow()[index] {
            return value.clone();
        }
        let value = lookup(side);
        slots.borrow_mut()[index] = Slot::Known(value.clone());
        value
    }

    /// A invalidação do `BlockCapabilityCache` de um lado: a próxima leitura procura de novo.
    pub fn invalidate(&self, side: Face) {
        if let Some(slots) = &self.slots {
            slots.borrow_mut()[face_ordinal(side)] = Slot::Unknown;
        }
    }

    /// `RefreshListener.run`: o aviso de invalidação de um lado. Se o transmissor é válido
    /// (`Transmitter.isValid`: não removido e carregado) devolve o lado que fica sujo
    /// (`markDirtyAcceptor`); senão, nada. O cache em si é invalidado nos dois casos, porque o
    /// `BlockCapabilityCache` se invalida antes de chamar o ouvinte.
    pub fn refresh_listener_run(&self, side: Face, transmitter_valid: bool) -> Option<Face> {
        self.invalidate(side);
        transmitter_valid.then_some(side)
    }
}
