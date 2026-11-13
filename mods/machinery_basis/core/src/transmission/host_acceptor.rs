//! Um vizinho de um cabo (ou de qualquer transmissor de energia) que o jogo não deixa desfazer: o Horizon insere e tira na hora, e a lib não tem o jogo na mão quando distribui (a decisão
//! 122). O aceitador faz a conta de `SimpleEnergyHandler.insert` e `extract` sobre o que o buffer do vizinho dizia no começo do evento ([`BufferSnapshot`]) e guarda o que entregou e o
//! que tirou; quem o criou aplica isso no jogo no fim do evento ([`SnapshotAcceptor::take_pending`]).
use crate::distribution::EnergyInsert;
use crate::transaction::Transaction;
use std::cell::Cell;

/// O que o buffer do vizinho dizia no começo da conta (`Game::native_energy`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshot {
    pub amount: i32,
    pub capacity: i32,
    pub max_insert: i32,
    pub max_extract: i32,
    pub can_insert: bool,
    pub can_extract: bool,
}

/// O `EnergyHandler` de um vizinho, sobre a foto do buffer dele.
#[derive(Default)]
pub struct SnapshotAcceptor {
    pub snapshot: Cell<BufferSnapshot>,
    delivered: Cell<i32>,
    taken: Cell<i32>,
}

impl SnapshotAcceptor {
    pub fn new(snapshot: BufferSnapshot) -> Self {
        Self {
            snapshot: Cell::new(snapshot),
            delivered: Cell::new(0),
            taken: Cell::new(0),
        }
    }

    /// `SimpleEnergyHandler.insert`: o que cabe (`capacity - amount`, contando o que este evento já entregou), no máximo o oferecido e o `max_insert`, e nada se o buffer não deixa entrar.
    fn insertable(&self, offered: i32) -> i32 {
        let snapshot = self.snapshot.get();
        if !snapshot.can_insert {
            return 0;
        }
        snapshot
            .capacity
            .saturating_sub(snapshot.amount.saturating_add(self.delivered.get()))
            .min(offered)
            .min(snapshot.max_insert)
            .max(0)
    }

    /// Há algo a aplicar no jogo.
    pub fn has_pending(&self) -> bool {
        self.delivered.get() != 0 || self.taken.get() != 0
    }

    /// O que o evento entregou e o que tirou, e zera os dois.
    pub fn take_pending(&self) -> (i32, i32) {
        (self.delivered.replace(0), self.taken.replace(0))
    }
}

impl EnergyInsert for SnapshotAcceptor {
    fn insert(&self, amount: i32, _: &Transaction) -> i32 {
        let moved = self.insertable(amount);
        self.delivered
            .set(self.delivered.get().saturating_add(moved));
        moved
    }

    fn probe_insert(&self, amount: i32, _: &Transaction) -> i32 {
        self.insertable(amount)
    }

    fn commit_probe(&self, amount: i32, _: &Transaction) {
        let moved = self.insertable(amount);
        self.delivered
            .set(self.delivered.get().saturating_add(moved));
    }

    /// `SimpleEnergyHandler.extract`: o que o buffer tem (menos o que este evento já tirou), no máximo o pedido e o `max_extract`; só a retirada de verdade (`!simulate`) conta.
    fn extract(&self, amount: i32, simulate: bool) -> i32 {
        let snapshot = self.snapshot.get();
        if !snapshot.can_extract || amount <= 0 {
            return 0;
        }
        let moved = snapshot
            .amount
            .saturating_sub(self.taken.get())
            .min(amount)
            .min(snapshot.max_extract)
            .max(0);
        if !simulate {
            self.taken.set(self.taken.get().saturating_add(moved));
        }
        moved
    }
}
