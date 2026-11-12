//! `TransporterManager`: as pilhas em voo para cada destino (`flowingStacks`), para que a busca conte, ao prever o que um destino
//! aceita, o que já está a caminho dele (`getPredictedInsert`). Cada destino tem um diário que desfaz as mudanças de uma transação.
//!
//! O original é todo estático; aqui é um objeto que o dono dos transportadores guarda (um por servidor, zerado por [`TransporterManager::reset`]).
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

/// `FlowingJournal`: as pilhas em voo para um destino. O `Set<TransporterStack>` do original é por identidade e a ordem dele muda a cada
/// JVM; aqui as pilhas ficam na ordem de inserção (decisão 118).
pub struct FlowingJournal {
    key: GlobalKey,
    stacks: Vec<StackRef>,
    owner: Weak<RefCell<FlowingMap>>,
}

impl Journaled for FlowingJournal {
    type Snapshot = Vec<StackRef>;

    fn revert_to_snapshot(&mut self, snapshot: Vec<StackRef>) {
        self.stacks = snapshot;
    }

    /// Sem pilhas ao fim da transação, o destino sai do mapa.
    fn on_root_commit(&mut self, _original: Vec<StackRef>) -> Vec<Callback> {
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
            .map(|journal| journal.borrow().stacks.clone())
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
                        stacks: Vec::new(),
                        owner: Rc::downgrade(&self.flowing),
                    })
                })
                .clone()
        };
        if let Some(transaction) = transaction {
            journal.update_snapshots(transaction, || journal.borrow().stacks.clone());
        }
        let mut state = journal.borrow_mut();
        if !state.stacks.iter().any(|other| other.same(stack)) {
            state.stacks.push(stack.clone());
        }
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
            journal.update_snapshots(transaction, || journal.borrow().stacks.clone());
        }
        let now_empty = {
            let mut state = journal.borrow_mut();
            let before = state.stacks.len();
            state.stacks.retain(|other| !other.same(stack));
            state.stacks.len() != before && state.stacks.is_empty()
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
                (state.item_type().cloned(), state.size(), state.side_of_dest())
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
