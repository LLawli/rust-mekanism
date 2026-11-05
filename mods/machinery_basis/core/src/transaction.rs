//! Porte das transações do NeoForge (`Transaction`, `TransactionManager`, `SnapshotJournal`). Cada contêiner se registra sozinho
//! na transação que o muda, em qualquer bloco, e os avisos de mudança disparam no commit da raiz, na ordem da fila.
//!
//! Como no NeoForge, o gerenciador é um por thread: uma raiz por vez, e as filhas empilham a partir dela.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

/// O que um contêiner avisa ao mudar: no Java, o `IContentsListener`.
pub type Callback = Rc<dyn Fn()>;

/// Quem se registra numa transação: o lado do `SnapshotJournal` que o gerenciador chama.
pub trait Journal {
    /// `SnapshotJournal.onClose`: o gerenciador passa o próprio `Rc`, para o diário poder se registrar de novo.
    fn on_close(&self, me: &Rc<dyn Journal>, depth: usize, aborted: bool);
    /// `SnapshotJournal.callOnRootCommit`.
    fn call_on_root_commit(&self);
}

struct Frame {
    open: bool,
    journals_to_close: Vec<Rc<dyn Journal>>,
}

struct Manager {
    stack: Vec<Frame>,
    /// `currentDepth`, com -1 para nenhuma transação aberta.
    current_depth: isize,
    root_commit_queue: VecDeque<Rc<dyn Journal>>,
    processing_root_commit_queue: bool,
}

thread_local! {
    static MANAGER: RefCell<Manager> = const {
        RefCell::new(Manager {
            stack: Vec::new(),
            current_depth: -1,
            root_commit_queue: VecDeque::new(),
            processing_root_commit_queue: false,
        })
    };
}

impl Manager {
    fn validate_current(&self, depth: usize) {
        assert!(
            self.current_depth == depth as isize,
            "a transação de profundidade {depth} não é a atual ({})",
            self.current_depth
        );
    }

    /// `processRootCommitQueue`: um diário por vez, soltando o gerenciador antes de cada aviso, porque um aviso pode abrir
    /// outra transação.
    fn process_root_commit_queue() {
        let already = MANAGER.with_borrow_mut(|manager| {
            std::mem::replace(&mut manager.processing_root_commit_queue, true)
        });
        if already {
            return;
        }
        while let Some(journal) =
            MANAGER.with_borrow_mut(|manager| manager.root_commit_queue.pop_front())
        {
            journal.call_on_root_commit();
        }
        MANAGER.with_borrow_mut(|manager| manager.processing_root_commit_queue = false);
    }
}

/// Uma transação aberta. Sem `commit`, fechar (inclusive por `drop`) desfaz tudo o que ela e as filhas mudaram.
#[derive(Debug)]
pub struct Transaction {
    depth: usize,
    done: Cell<bool>,
}

impl Transaction {
    /// `Transaction.openRoot`.
    pub fn open_root() -> Self {
        Self::open(None)
    }

    /// `Transaction.open(parent)`: sem pai, abre uma raiz, e só pode haver uma na thread.
    pub fn open(parent: Option<&Transaction>) -> Self {
        MANAGER.with_borrow_mut(|manager| {
            if let Some(parent) = parent {
                manager.validate_current(parent.depth);
                assert!(
                    manager.stack[parent.depth].open,
                    "operação de transação em uma transação fechada"
                );
            } else {
                assert!(
                    manager.current_depth < 0,
                    "já há uma transação raiz ativa nesta thread"
                );
            }
            manager.current_depth += 1;
            let depth = manager.current_depth as usize;
            if manager.stack.len() == depth {
                manager.stack.push(Frame {
                    open: false,
                    journals_to_close: Vec::new(),
                });
            }
            manager.stack[depth].open = true;
            Self {
                depth,
                done: Cell::new(false),
            }
        })
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    /// `commit`: as mudanças ficam na transação de fora, ou valem de vez se esta é a raiz.
    pub fn commit(self) {
        self.close(false);
    }

    /// `aborted` é o `wasAborted` do Java.
    fn close(&self, aborted: bool) {
        if self.done.replace(true) {
            return;
        }
        let journals = MANAGER.with_borrow_mut(|manager| {
            manager.validate_current(self.depth);
            let frame = &mut manager.stack[self.depth];
            assert!(frame.open, "operação de transação em uma transação fechada");
            frame.open = false;
            std::mem::take(&mut frame.journals_to_close)
        });
        for journal in &journals {
            journal.on_close(journal, self.depth, aborted);
        }
        let root_closed = MANAGER.with_borrow_mut(|manager| {
            manager.current_depth -= 1;
            manager.current_depth == -1
        });
        if root_closed {
            Manager::process_root_commit_queue();
        }
    }

    /// Registra o diário para fechar junto desta transação (`journalsToClose.add`).
    fn register(&self, journal: Rc<dyn Journal>) {
        MANAGER
            .with_borrow_mut(|manager| manager.stack[self.depth].journals_to_close.push(journal));
    }
}

impl Drop for Transaction {
    /// O `close()` de um `try` sem `commit`: desfaz.
    fn drop(&mut self) {
        if !self.done.get() && !std::thread::panicking() {
            self.close(true);
        }
    }
}

/// O estado de um `SnapshotJournal<T>`, sem o dono.
struct JournalState<T> {
    /// Um por profundidade; `None` é o `NO_SNAPSHOT`.
    snapshots: Vec<Option<T>>,
    snapshot_count: usize,
    original_state: Option<T>,
}

/// O que um contêiner implementa para ter diário: voltar ao retrato, e reagir ao commit da raiz.
pub trait Journaled: 'static {
    type Snapshot;
    /// `revertToSnapshot`: volta sem disparar aviso.
    fn revert_to_snapshot(&mut self, snapshot: Self::Snapshot);
    /// `onRootCommit(originalState)`: devolve os avisos a disparar, na ordem. Quem chama os dispara depois de soltar o
    /// empréstimo do contêiner, então um aviso pode mexer em outros contêineres.
    fn on_root_commit(&mut self, original: Self::Snapshot) -> Vec<Callback>;
}

/// Um contêiner com o seu `SnapshotJournal`. Vive num `Rc`, compartilhado por quem o usa e pela transação.
pub struct JournalCell<S: Journaled> {
    me: Weak<JournalCell<S>>,
    state: RefCell<S>,
    journal: RefCell<JournalState<S::Snapshot>>,
}

impl<S: Journaled> JournalCell<S> {
    pub fn new(state: S) -> Rc<Self> {
        Rc::new_cyclic(|me| Self {
            me: me.clone(),
            state: RefCell::new(state),
            journal: RefCell::new(JournalState {
                snapshots: Vec::new(),
                snapshot_count: 0,
                original_state: None,
            }),
        })
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, S> {
        self.state.borrow()
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, S> {
        self.state.borrow_mut()
    }

    /// `SnapshotJournal.updateSnapshots`: guarda o retrato na primeira mudança de cada profundidade e se registra na
    /// transação. `snapshot` só roda quando precisa.
    pub fn update_snapshots(
        &self,
        transaction: &Transaction,
        snapshot: impl FnOnce() -> S::Snapshot,
    ) {
        let depth = transaction.depth();
        let mut journal = self.journal.borrow_mut();
        while journal.snapshots.len() <= depth {
            journal.snapshots.push(None);
        }
        if journal.snapshots[depth].is_none() {
            journal.snapshot_count += 1;
            journal.snapshots[depth] = Some(snapshot());
            drop(journal);
            if let Some(me) = self.me.upgrade() {
                transaction.register(me);
            }
        }
    }

    /// `isInTransaction`.
    pub fn is_in_transaction(&self) -> bool {
        self.journal.borrow().snapshot_count > 0
    }
}

/// Dispara os avisos de uma mudança fora de transação (`setContents(.., null)`), já sem empréstimo aberto.
pub fn fire(callbacks: Vec<Callback>) {
    for callback in callbacks {
        callback();
    }
}

impl<S: Journaled> Journal for JournalCell<S> {
    fn on_close(&self, me: &Rc<dyn Journal>, depth: usize, aborted: bool) {
        let mut journal = self.journal.borrow_mut();
        let snapshot = journal
            .snapshots
            .remove(depth)
            .expect("diário registrado sem retrato");
        if aborted {
            drop(journal);
            self.state.borrow_mut().revert_to_snapshot(snapshot);
            self.journal.borrow_mut().snapshot_count -= 1;
        } else if depth == 0 {
            if journal.original_state.is_none() {
                journal.original_state = Some(snapshot);
                MANAGER.with_borrow_mut(|manager| manager.root_commit_queue.push_back(me.clone()));
            }
            journal.snapshot_count = 0;
        } else if journal.snapshots[depth - 1].is_none() {
            journal.snapshots[depth - 1] = Some(snapshot);
            drop(journal);
            MANAGER.with_borrow_mut(|manager| {
                manager.stack[depth - 1].journals_to_close.push(me.clone());
            });
        } else {
            journal.snapshot_count -= 1;
        }
    }

    fn call_on_root_commit(&self) {
        let original = self
            .journal
            .borrow_mut()
            .original_state
            .take()
            .expect("commit da raiz sem estado original");
        let callbacks = self.state.borrow_mut().on_root_commit(original);
        fire(callbacks);
    }
}

/// `RootCommitJournal`: não guarda nada, só avisa uma vez quando a raiz commita.
pub struct RootCommitJournal {
    callback: Callback,
}

impl RootCommitJournal {
    pub fn new(callback: Callback) -> Rc<JournalCell<Self>> {
        JournalCell::new(Self { callback })
    }
}

impl Journaled for RootCommitJournal {
    type Snapshot = ();

    fn revert_to_snapshot(&mut self, _snapshot: ()) {}

    fn on_root_commit(&mut self, _original: ()) -> Vec<Callback> {
        vec![self.callback.clone()]
    }
}

impl JournalCell<RootCommitJournal> {
    /// `updateSnapshots` de quem não guarda nada.
    pub fn touch(&self, transaction: &Transaction) {
        self.update_snapshots(transaction, || ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um valor com aviso, o mínimo para exercitar o diário.
    struct Counter {
        value: i32,
        fired: Rc<Cell<u32>>,
    }

    impl Journaled for Counter {
        type Snapshot = i32;

        fn revert_to_snapshot(&mut self, snapshot: i32) {
            self.value = snapshot;
        }

        fn on_root_commit(&mut self, original: i32) -> Vec<Callback> {
            if original == self.value {
                return Vec::new();
            }
            let fired = self.fired.clone();
            vec![Rc::new(move || fired.set(fired.get() + 1))]
        }
    }

    fn counter(value: i32) -> (Rc<JournalCell<Counter>>, Rc<Cell<u32>>) {
        let fired = Rc::new(Cell::new(0));
        let cell = JournalCell::new(Counter {
            value,
            fired: fired.clone(),
        });
        (cell, fired)
    }

    fn set(cell: &JournalCell<Counter>, value: i32, tx: &Transaction) {
        let mut state = cell.borrow_mut();
        cell.update_snapshots(tx, || state.value);
        state.value = value;
    }

    #[test]
    fn a_simulation_always_goes_back() {
        let (cell, fired) = counter(5);
        let root = Transaction::open_root();
        let simulation = Transaction::open(Some(&root));
        set(&cell, 9, &simulation);
        drop(simulation);
        drop(root);
        assert_eq!(cell.borrow().value, 5);
        assert_eq!(fired.get(), 0);
    }

    #[test]
    fn only_the_root_commit_fires() {
        let (cell, fired) = counter(5);
        let root = Transaction::open_root();
        let nested = Transaction::open(Some(&root));
        set(&cell, 9, &nested);
        nested.commit();
        assert_eq!(fired.get(), 0);
        root.commit();
        assert_eq!(cell.borrow().value, 9);
        assert_eq!(fired.get(), 1);
    }

    #[test]
    fn rolling_back_the_root_undoes_a_committed_child() {
        let (cell, fired) = counter(5);
        let root = Transaction::open_root();
        set(&cell, 6, &root);
        let nested = Transaction::open(Some(&root));
        set(&cell, 7, &nested);
        nested.commit();
        drop(root);
        assert_eq!(cell.borrow().value, 5);
        assert_eq!(fired.get(), 0);
    }

    #[test]
    fn a_commit_back_to_the_original_says_nothing() {
        let (cell, fired) = counter(5);
        let root = Transaction::open_root();
        set(&cell, 9, &root);
        set(&cell, 5, &root);
        root.commit();
        assert_eq!(fired.get(), 0);
    }

    #[test]
    fn the_root_queue_fires_in_the_order_of_registration() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let journal = |name: &'static str| {
            let order = order.clone();
            RootCommitJournal::new(Rc::new(move || order.borrow_mut().push(name)))
        };
        let (first, second) = (journal("first"), journal("second"));
        let root = Transaction::open_root();
        second.touch(&root);
        first.touch(&root);
        root.commit();
        assert_eq!(*order.borrow(), ["second", "first"]);
    }

    #[test]
    #[should_panic(expected = "já há uma transação raiz")]
    fn two_roots_at_once_are_refused() {
        let _first = Transaction::open_root();
        let _second = Transaction::open_root();
    }
}
