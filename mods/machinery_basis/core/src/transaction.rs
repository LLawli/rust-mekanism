//! Porte das transações do NeoForge que o Mekanism usa: uma raiz por operação e simulações filhas, que sempre revertem.

/// Profundidade de uma transação aberta. Quem a abre fecha com `close` em cada participante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transaction {
    depth: u32,
}

impl Transaction {
    pub const fn open_root() -> Self {
        Self { depth: 0 }
    }

    pub const fn open_nested(self) -> Self {
        Self {
            depth: self.depth + 1,
        }
    }
}

/// Porte de `SnapshotJournal`: guarda o estado original só na primeira mudança de cada profundidade.
#[derive(Clone, Debug)]
pub struct SnapshotJournal<T> {
    snapshots: Vec<(u32, T)>,
}

impl<T> Default for SnapshotJournal<T> {
    fn default() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }
}

impl<T: Clone> SnapshotJournal<T> {
    pub fn update_snapshots(&mut self, transaction: Transaction, current: &T) {
        if self.snapshots.last().map(|(depth, _)| *depth) != Some(transaction.depth) {
            self.snapshots.push((transaction.depth, current.clone()));
        }
    }

    /// Fecha uma profundidade. Devolve o estado original só no commit da raiz, para o contêiner avisar quem escuta.
    pub fn close(&mut self, transaction: Transaction, commit: bool, current: &mut T) -> Option<T> {
        if self.snapshots.last().map(|(depth, _)| *depth) != Some(transaction.depth) {
            return None;
        }
        let (depth, snapshot) = self.snapshots.pop()?;
        if !commit {
            *current = snapshot;
            return None;
        }
        if depth == 0 {
            return Some(snapshot);
        }
        // Sem estado guardado na profundidade de fora, o original daqui passa a ser o dela.
        if self.snapshots.last().map(|(outer, _)| *outer) != Some(depth - 1) {
            self.snapshots.push((depth - 1, snapshot));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simulation_always_goes_back() {
        let root = Transaction::open_root();
        let simulation = root.open_nested();
        let (mut journal, mut value) = (SnapshotJournal::default(), 5);
        journal.update_snapshots(simulation, &value);
        value = 9;
        assert_eq!(journal.close(simulation, false, &mut value), None);
        assert_eq!(value, 5);
    }

    #[test]
    fn only_the_root_commit_reports_the_original() {
        let root = Transaction::open_root();
        let nested = root.open_nested();
        let (mut journal, mut value) = (SnapshotJournal::default(), 5);
        journal.update_snapshots(nested, &value);
        value = 9;
        assert_eq!(journal.close(nested, true, &mut value), None);
        assert_eq!(journal.close(root, true, &mut value), Some(5));
        assert_eq!(value, 9);
    }

    #[test]
    fn rolling_back_the_root_undoes_a_committed_child() {
        let root = Transaction::open_root();
        let nested = root.open_nested();
        let (mut journal, mut value) = (SnapshotJournal::default(), 5);
        journal.update_snapshots(root, &value);
        value = 6;
        journal.update_snapshots(nested, &value);
        value = 7;
        journal.close(nested, true, &mut value);
        journal.close(root, false, &mut value);
        assert_eq!(value, 5);
    }
}
