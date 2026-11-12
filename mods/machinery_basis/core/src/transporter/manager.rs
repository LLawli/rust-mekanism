//! `TransporterManager`: as pilhas em voo para cada destino (`flowingStacks`), para que a busca conte, ao prever o que um destino
//! aceita, o que já está a caminho dele (`getPredictedInsert`). Cada destino tem um diário que desfaz as mudanças de uma transação.
//!
//! O original é todo estático; aqui é um objeto que o dono dos transportadores guarda (um por servidor, zerado por [`TransporterManager::reset`]).
//!
//! # A ordem das pilhas em voo (decisão 118)
//!
//! O `FlowingJournal` do original guarda um `HashSet<TransporterStack>`, e `TransporterStack` não sobrescreve `hashCode`: a ordem em que
//! `predictFlowing` as percorre é a da tabela do `HashMap` com o `System.identityHashCode` de cada pilha, que muda a cada execução.
//! [`StackSet`] reproduz essa tabela (o balde de `(h ^ h >>> 16) & (n - 1)`, a ordem de inserção dentro do balde, a tabela que dobra e não
//! encolhe) e cada pilha traz o hash dela (`TransporterStack::identity_hash`, sorteado em produção e dado pelo rastro no teste).
use super::stack::StackRef;
use crate::containers::ItemResource;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use crate::transit::{TransitRequest, TransitResponse, item_data_info};
use crate::transmission::DimensionId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

/// `GlobalPos`: a dimensão e o `BlockPos` empacotado.
pub type GlobalKey = (DimensionId, i64);

type FlowingMap = HashMap<GlobalKey, Rc<JournalCell<FlowingJournal>>>;

/// `HashMap.MIN_TREEIFY_CAPACITY` e `TREEIFY_THRESHOLD`.
const MIN_TREEIFY_CAPACITY: usize = 64;
const TREEIFY_THRESHOLD: usize = 8;
/// A tabela de um `HashSet` criado sem argumentos (`DEFAULT_INITIAL_CAPACITY`).
const DEFAULT_CAPACITY: usize = 16;

/// Um `java.util.HashSet<TransporterStack>` por identidade, no que muda a ordem: os membros em ordem de inserção e o tamanho da tabela.
#[derive(Clone, Default)]
pub struct StackSet {
    /// Os membros atuais, na ordem em que entraram (sair não muda a ordem dos outros).
    members: Vec<StackRef>,
    /// O tamanho da tabela; 0 até a primeira inserção (`table == null`).
    capacity: usize,
}

impl StackSet {
    fn bucket(stack: &StackRef, capacity: usize) -> usize {
        let hash = stack.borrow().identity_hash() as u32;
        ((hash ^ (hash >> 16)) as usize) & (capacity - 1)
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn contains(&self, stack: &StackRef) -> bool {
        self.members.iter().any(|other| other.same(stack))
    }

    /// `add`: falso se a pilha já está.
    pub fn add(&mut self, stack: &StackRef) -> bool {
        if self.contains(stack) {
            return false;
        }
        if self.capacity == 0 {
            self.capacity = DEFAULT_CAPACITY;
        }
        self.members.push(stack.clone());
        // `putVal`: o nono nó de um balde numa tabela menor que 64 dobra a tabela (`treeifyBin`); numa de 64 ou mais vira árvore, e a
        // ordem da árvore não é portada.
        let bucket = Self::bucket(stack, self.capacity);
        let in_bucket = self
            .members
            .iter()
            .filter(|other| Self::bucket(other, self.capacity) == bucket)
            .count();
        if in_bucket > TREEIFY_THRESHOLD {
            assert!(
                self.capacity < MIN_TREEIFY_CAPACITY,
                "HashSet de pilhas com um balde de {in_bucket} nós numa tabela de {}: a ordem da árvore não é portada",
                self.capacity
            );
            self.capacity *= 2;
        }
        if self.members.len() > self.capacity * 3 / 4 {
            self.capacity *= 2;
        }
        true
    }

    /// `remove`: verdadeiro se a pilha estava.
    pub fn remove(&mut self, stack: &StackRef) -> bool {
        let before = self.members.len();
        self.members.retain(|other| !other.same(stack));
        self.members.len() != before
    }

    /// A ordem do iterador: balde a balde, e em ordem de inserção dentro de cada um.
    pub fn in_order(&self) -> Vec<StackRef> {
        if self.capacity == 0 {
            return Vec::new();
        }
        let mut ordered = self.members.clone();
        ordered.sort_by_key(|stack| Self::bucket(stack, self.capacity));
        ordered
    }

    /// `new HashSet<>(set)`: `createSnapshot` do diário. Cabe a `max((int) (size / .75f) + 1, 16)` e entra na ordem do iterador do original.
    pub fn copy(&self) -> StackSet {
        let mut copy = StackSet {
            members: Vec::new(),
            capacity: crate::sim::java_hash::initial_capacity_for(self.len()).next_power_of_two(),
        };
        if self.is_empty() {
            copy.capacity = 0;
            return copy;
        }
        for stack in self.in_order() {
            copy.add(&stack);
        }
        copy
    }
}

/// `FlowingJournal`: as pilhas em voo para um destino, no conjunto que o original usa (decisão 118).
pub struct FlowingJournal {
    key: GlobalKey,
    stacks: StackSet,
    owner: Weak<RefCell<FlowingMap>>,
}

impl Journaled for FlowingJournal {
    type Snapshot = StackSet;

    fn revert_to_snapshot(&mut self, snapshot: StackSet) {
        self.stacks = snapshot;
    }

    /// Sem pilhas ao fim da transação, o destino sai do mapa.
    fn on_root_commit(&mut self, _original: StackSet) -> Vec<Callback> {
        if self.stacks.is_empty()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.borrow_mut().remove(&self.key);
        }
        Vec::new()
    }
}

#[derive(Default)]
pub struct TransporterManager {
    flowing: Rc<RefCell<FlowingMap>>,
}

impl TransporterManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// `reset()`.
    pub fn reset(&self) {
        self.flowing.borrow_mut().clear();
    }

    /// Quantos destinos têm pilhas em voo.
    pub fn flowing_destinations(&self) -> usize {
        self.flowing.borrow().len()
    }

    /// As pilhas em voo para um destino, na ordem.
    pub fn flowing_to(&self, dimension: DimensionId, pos: i64) -> Vec<StackRef> {
        self.flowing
            .borrow()
            .get(&(dimension, pos))
            .map(|journal| journal.borrow().stacks.in_order())
            .unwrap_or_default()
    }

    /// `add(world, stack, transaction)`: a pilha, já com o caminho, passa a contar no destino dela.
    pub fn add(&self, dimension: DimensionId, stack: &StackRef, transaction: Option<&Transaction>) {
        let key = (dimension, stack.borrow().get_dest());
        let journal = {
            let mut map = self.flowing.borrow_mut();
            map.entry(key)
                .or_insert_with(|| {
                    JournalCell::new(FlowingJournal {
                        key,
                        stacks: StackSet::default(),
                        owner: Rc::downgrade(&self.flowing),
                    })
                })
                .clone()
        };
        if let Some(transaction) = transaction {
            journal.update_snapshots(transaction, || journal.borrow().stacks.copy());
        }
        journal.borrow_mut().stacks.add(stack);
    }

    /// `remove(world, stack, transaction)`: tira a pilha do destino dela, se ela tem caminho com alvo.
    pub fn remove(
        &self,
        dimension: DimensionId,
        stack: &StackRef,
        transaction: Option<&Transaction>,
    ) {
        let key = {
            let stack = stack.borrow();
            if !(stack.has_path() && stack.path_type().has_target()) {
                return;
            }
            (dimension, stack.get_dest())
        };
        let journal = self.flowing.borrow().get(&key).cloned();
        let Some(journal) = journal else {
            return;
        };
        if let Some(transaction) = transaction {
            journal.update_snapshots(transaction, || journal.borrow().stacks.copy());
        }
        let now_empty = {
            let mut state = journal.borrow_mut();
            state.stacks.remove(stack) && state.stacks.is_empty()
        };
        if now_empty {
            self.flowing.borrow_mut().remove(&key);
        }
    }

    /// `getPredictedInsert(position, side, handler, request, transaction)`: o que o destino deve aceitar do pedido, contando o que já está
    /// a caminho dele.
    pub fn get_predicted_insert(
        &self,
        position: GlobalKey,
        side: Face,
        handler: &Rc<dyn ItemHandler>,
        request: &TransitRequest,
        transaction: Option<&Transaction>,
    ) -> TransitResponse {
        let simulation = Transaction::open(transaction);
        let flowing = self.flowing_to(position.0, position.1);
        let flowing_ok = self.flowing.borrow().get(&position).is_none()
            || Self::predict_flowing(&flowing, side, handler, &simulation);
        if flowing_ok {
            for data in request.item_data() {
                let (item, count) = item_data_info(&data);
                if count > 0 {
                    let inserted = handler.insert(&item, count, &simulation);
                    if inserted > 0 {
                        return request.create_response(&item, inserted, &data);
                    }
                }
            }
        }
        TransitResponse::EMPTY
    }

    fn predict_flowing(
        flowing: &[StackRef],
        side: Face,
        handler: &Rc<dyn ItemHandler>,
        simulation: &Transaction,
    ) -> bool {
        for stack in flowing {
            let (item, to_insert, side_of_dest): (Option<ItemResource>, u32, Option<Face>) = {
                let state = stack.borrow();
                if state.is_empty() || !state.path_type().has_target() {
                    continue;
                }
                (
                    state.item_type().cloned(),
                    state.size(),
                    state.side_of_dest(),
                )
            };
            let Some(item) = item else {
                continue;
            };
            // Tenta inserir a pilha mesmo que seja por outro lado do destino: o inventário (um baú) ou parte dos slots (as máquinas) pode ser
            // o mesmo, e então a pilha em voo ocupa o lugar.
            let inserted = handler.insert(&item, to_insert, simulation);
            if inserted < to_insert {
                if inserted == 0 && Some(side) != side_of_dest {
                    // Outro lado: o destino talvez nem tenha o slot a que a pilha vai, então segue com as outras e com o pedido.
                    continue;
                }
                // Esta pilha em voo não coube; não sobra lugar para ninguém.
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transporter::TransporterStack;

    fn stack(hash: i32) -> StackRef {
        let mut stack = TransporterStack::new();
        stack.set_identity_hash(hash);
        StackRef::new(stack)
    }

    fn hashes(set: &StackSet) -> Vec<i32> {
        set.in_order()
            .iter()
            .map(|stack| stack.borrow().identity_hash())
            .collect()
    }

    /// Dois membros que a tabela de 32 separa e a de 16 junta: o iterador do original segue a tabela, que não encolhe ao remover.
    #[test]
    fn the_table_does_not_shrink_and_a_copy_is_sized_for_its_contents() {
        let mut set = StackSet::default();
        let crowd: Vec<StackRef> = (0..13).map(|i| stack(100 + i * 16 + 3)).collect();
        for member in &crowd {
            set.add(member);
        }
        // 13 membros passam do limite de 12 da tabela de 16: ela dobra para 32.
        let (first, second) = (stack(17), stack(2));
        set.add(&first);
        set.add(&second);
        for member in &crowd {
            assert!(set.remove(member));
        }
        // Tabela de 32: o balde 2 vem antes do 17.
        assert_eq!(hashes(&set), [2, 17]);
        // `new HashSet<>(set)`: 16 de capacidade, balde 1 e balde 2.
        let copy = set.copy();
        assert_eq!(hashes(&copy), [17, 2]);
    }

    #[test]
    fn a_member_is_added_once_and_removed_by_identity() {
        let mut set = StackSet::default();
        let (a, b) = (stack(5), stack(5));
        assert!(set.add(&a));
        assert!(!set.add(&a));
        assert!(set.add(&b));
        assert!(set.remove(&a));
        assert!(!set.remove(&a));
        assert_eq!(set.len(), 1);
    }

    /// Mesmo balde: a ordem de inserção, e a que sai e volta passa para o fim.
    #[test]
    fn inside_a_bucket_the_order_is_the_insertion_order() {
        let mut set = StackSet::default();
        let (a, b, c) = (stack(1), stack(17), stack(33));
        set.add(&a);
        set.add(&b);
        set.add(&c);
        assert_eq!(hashes(&set), [1, 17, 33]);
        set.remove(&a);
        set.add(&a);
        assert_eq!(hashes(&set), [17, 33, 1]);
    }
}
