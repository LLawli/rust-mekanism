//! Porte de `RateLimitIntegerJournal` (`GameTimeBasedJournal`): o limite de taxa por tick de um contêiner, com diário. Só o
//! `createInternalOnlyRateLimit` existe aqui: quem é `INTERNAL` é limitado, e a conta volta ao teto quando o tick muda.
use crate::containers::AutomationType;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::Cell;
use std::rc::Rc;

struct RateState {
    /// `currentGameTime`, que nasce em zero como o `long` do Java.
    time: u64,
    remaining: i32,
}

impl Journaled for RateState {
    type Snapshot = i32;

    fn revert_to_snapshot(&mut self, snapshot: i32) {
        self.remaining = snapshot;
    }

    fn on_root_commit(&mut self, _original: i32) -> Vec<Callback> {
        Vec::new()
    }
}

/// `ITransactionHelper.createInternalOnlyRateLimit(gameTime, rate)`: só o tipo `INTERNAL` é limitado, e a conta volta ao teto quando o
/// tick muda.
#[derive(Clone)]
pub struct RateLimit {
    cell: Rc<JournalCell<RateState>>,
    game_time: Rc<Cell<u64>>,
    rate: i32,
}

impl RateLimit {
    pub fn new(game_time: Rc<Cell<u64>>, rate: i32) -> Self {
        Self {
            cell: JournalCell::new(RateState {
                time: 0,
                remaining: 0,
            }),
            game_time,
            rate,
        }
    }

    fn check_tick_changed(&self) {
        let now = self.game_time.get();
        let mut state = self.cell.borrow_mut();
        if state.time != now {
            state.time = now;
            state.remaining = self.rate;
        }
    }

    /// `getRemainingLimit`.
    pub fn remaining(&self, automation: AutomationType) -> i32 {
        if automation == AutomationType::Internal {
            self.check_tick_changed();
            return self.cell.borrow().remaining;
        }
        i32::MAX
    }

    /// `consumeLimit`.
    pub fn consume(&self, limit: i32, automation: AutomationType, transaction: &Transaction) {
        if automation != AutomationType::Internal {
            return;
        }
        self.check_tick_changed();
        let remaining = self.cell.borrow().remaining;
        assert!(limit <= remaining, "consumiu mais do limite que havia");
        self.cell.update_snapshots(transaction, || remaining);
        self.cell.borrow_mut().remaining -= limit;
    }
}
