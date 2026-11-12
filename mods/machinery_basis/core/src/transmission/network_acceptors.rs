//! `NetworkAcceptorCache`: os aceitadores que a rede conhece, por posição (do aceitador) e lado (do aceitador voltado para o cabo), e
//! os lados de cada transmissor que mudaram desde o último `commit`.
use super::{Hashed, LongKey, TransmitterHost, TransmitterId};
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::sim::open_hash::OpenHashMap;

/// O `EnumMap<Direction, ACCEPTOR>` de uma posição, na ordem do `Direction`.
#[derive(Clone, Debug)]
pub struct AcceptorSides<A> {
    slots: [Option<A>; 6],
}

impl<A> Default for AcceptorSides<A> {
    fn default() -> Self {
        Self {
            slots: [const { None }; 6],
        }
    }
}

fn index(side: Face) -> usize {
    Face::ALL
        .iter()
        .position(|face| *face == side)
        .expect("Face::ALL tem os seis lados")
}

impl<A> AcceptorSides<A> {
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, side: Face) -> Option<&A> {
        self.slots[index(side)].as_ref()
    }

    fn put(&mut self, side: Face, acceptor: A) {
        self.slots[index(side)] = Some(acceptor);
    }

    fn remove(&mut self, side: Face) {
        self.slots[index(side)] = None;
    }

    /// Os lados com aceitador, na ordem do `Direction` (a do `EnumMap`).
    pub fn sides(&self) -> Vec<Face> {
        Face::ALL
            .into_iter()
            .filter(|side| self.slots[index(*side)].is_some())
            .collect()
    }
}

impl<A: Clone> AcceptorSides<A> {
    /// `Map.putAll`.
    fn put_all(&mut self, other: &AcceptorSides<A>) {
        for (slot, theirs) in self.slots.iter_mut().zip(&other.slots) {
            if let Some(acceptor) = theirs {
                *slot = Some(acceptor.clone());
            }
        }
    }
}

/// Os lados de um transmissor, o `EnumSet<Direction>`.
type SideSet = [bool; 6];

#[derive(Clone, Debug)]
pub struct NetworkAcceptorCache<A> {
    /// `cachedAcceptors`: `Long2ObjectOpenHashMap`, chave `BlockPos.asLong` do aceitador.
    cached: OpenHashMap<LongKey, AcceptorSides<A>>,
    /// `changedAcceptors`: `Object2ObjectOpenHashMap`, chave o transmissor (pelo `hashCode` de identidade).
    changed: OpenHashMap<Hashed<TransmitterId>, SideSet>,
}

impl<A> Default for NetworkAcceptorCache<A> {
    fn default() -> Self {
        Self {
            cached: OpenHashMap::new(),
            changed: OpenHashMap::new(),
        }
    }
}

impl<A: Clone> NetworkAcceptorCache<A> {
    /// `updateTransmitterOnSide(transmitter, side)`.
    pub fn update_transmitter_on_side(
        &mut self,
        host: &mut dyn TransmitterHost<A>,
        transmitter: TransmitterId,
        side: Face,
    ) {
        host.refresh_acceptor_connections(transmitter, side);
        let acceptor = host.acceptor(transmitter, side);
        let acceptor_pos = LongKey(host.block_pos(transmitter).relative(side).as_long());
        match acceptor {
            None => {
                if let Some(cached) = self.cached.get_mut(acceptor_pos) {
                    cached.remove(side.opposite());
                    if cached.is_empty() {
                        self.cached.remove(acceptor_pos);
                    }
                }
            }
            Some(acceptor) => {
                if let Some(cached) = self.cached.get_mut(acceptor_pos) {
                    cached.put(side.opposite(), acceptor);
                } else {
                    let mut sides = AcceptorSides::default();
                    sides.put(side.opposite(), acceptor);
                    self.cached.insert(acceptor_pos, sides);
                }
            }
        }
    }

    /// `adoptAcceptors(other)`: na ordem do iterador da tabela de `other`.
    pub fn adopt_acceptors(&mut self, other: &NetworkAcceptorCache<A>) {
        for (pos, sides) in other.cached.entries_in_order() {
            if let Some(mine) = self.cached.get_mut(pos) {
                mine.put_all(sides);
            } else {
                self.cached.insert(pos, sides.clone());
            }
        }
        for (transmitter, sides) in other.changed.entries_in_order() {
            if let Some(mine) = self.changed.get_mut(transmitter) {
                for (mine, theirs) in mine.iter_mut().zip(sides) {
                    *mine |= *theirs;
                }
            } else {
                self.changed.insert(transmitter, *sides);
            }
        }
    }

    /// `acceptorChanged(transmitter, side)`, sem o `registerChangedNetwork` (que é do registro: ele conhece o conjunto).
    pub(crate) fn mark_changed(&mut self, transmitter: Hashed<TransmitterId>, side: Face) {
        if let Some(sides) = self.changed.get_mut(transmitter) {
            sides[index(side)] = true;
        } else {
            let mut sides = [false; 6];
            sides[index(side)] = true;
            self.changed.insert(transmitter, sides);
        }
    }

    /// `commit()`: atualiza os lados que mudaram, dos transmissores ainda válidos.
    pub fn commit(&mut self, host: &mut dyn TransmitterHost<A>) {
        if self.changed.is_empty() {
            return;
        }
        let changed: Vec<(TransmitterId, SideSet)> = self
            .changed
            .entries_in_order()
            .into_iter()
            .map(|(key, sides)| (key.id, *sides))
            .collect();
        for (transmitter, sides) in changed {
            if host.is_valid(transmitter) {
                for side in Face::ALL {
                    if sides[index(side)] {
                        self.update_transmitter_on_side(host, transmitter, side);
                    }
                }
            }
        }
        self.changed.clear_keeping_table();
    }

    /// `deregister()`: `clear()` das duas tabelas.
    pub fn deregister(&mut self) {
        self.cached.clear_keeping_table();
        self.changed.clear_keeping_table();
    }

    /// `getAcceptorCount()`: cada lado de cada posição conta como um aceitador.
    pub fn acceptor_count(&self) -> usize {
        self.cached
            .entries_in_order()
            .into_iter()
            .map(|(_, sides)| sides.len())
            .sum()
    }

    /// `hasAcceptor(pos)`.
    pub fn has_acceptor(&self, pos: BlockPos) -> bool {
        self.cached.contains_key(LongKey(pos.as_long()))
    }

    /// `getCachedAcceptor(pos, side)`.
    pub fn cached_acceptor(&self, pos: BlockPos, side: Face) -> Option<&A> {
        self.cached
            .get(LongKey(pos.as_long()))
            .and_then(|sides| sides.get(side))
    }

    /// `getAcceptorDirections(pos)`, vazio se não há aceitador ali (o Java dá `NullPointerException`).
    pub fn acceptor_directions(&self, pos: BlockPos) -> Vec<Face> {
        self.cached
            .get(LongKey(pos.as_long()))
            .map(AcceptorSides::sides)
            .unwrap_or_default()
    }

    /// `getAcceptorFastIterator`: as posições (`BlockPos.asLong`) na ordem da tabela.
    pub fn positions_in_order(&self) -> Vec<i64> {
        self.cached
            .entries_in_order()
            .into_iter()
            .map(|(pos, _)| pos.0)
            .collect()
    }
}
