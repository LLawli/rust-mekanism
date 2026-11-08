//! `LastEnergyTracker` (`mekanism.common.lib.transaction`): quanta energia o tile recebeu pela capability de energia no tick anterior.
//! É o `lastEnergyReceived` que o `TileEntityMekanism.addContainerTrackers` sincroniza (`trackLastEnergy`) e que `getInputRate` lê.
//!
//! O Java não conta por tick do relógio: `GameTimeBasedJournal` só repara que o tick mudou quando alguém pergunta (`received`, no
//! `TrackingEnergyHandler.insert`, ou o fim do `tickServer` do tile, que pergunta de propósito). Então o valor de um tick só vira
//! `lastEnergyReceived` na próxima pergunta de um tick novo, e um tile que não recebe nada volta a zero no fim do tick seguinte
//! ao da chegada. Uma transação desfeita (`revertToSnapshot`) não passa por aqui: só o que entra de verdade é contado.
use std::cell::Cell;

#[derive(Debug, Default)]
pub struct LastEnergyTracker {
    current_game_time: Cell<u64>,
    last_energy_received: Cell<i64>,
    current_energy_received: Cell<i64>,
}

impl LastEnergyTracker {
    /// `checkTickChanged`: se o tick é outro, o que entrou até aqui passa a ser o do "último tick" e a conta recomeça.
    pub fn check_tick_changed(&self, game_time: u64) {
        if self.current_game_time.get() != game_time {
            self.current_game_time.set(game_time);
            self.last_energy_received
                .set(self.current_energy_received.get());
            self.current_energy_received.set(0);
        }
    }

    /// `received(amount, transaction)`: o `TrackingEnergyHandler.insert` diz quanto entrou de fato (zero também conta como pergunta).
    pub fn received(&self, amount: i64, game_time: u64) {
        self.check_tick_changed(game_time);
        self.current_energy_received
            .set(self.current_energy_received.get() + amount);
    }

    /// `getLastEnergyReceived`.
    pub fn last_energy_received(&self) -> i64 {
        self.last_energy_received.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_arrives_in_a_tick_shows_up_after_the_next_check_and_is_gone_after_the_one_after() {
        let tracker = LastEnergyTracker::default();
        tracker.check_tick_changed(10);
        tracker.received(1_000, 10);
        tracker.received(250, 10);
        assert_eq!(tracker.last_energy_received(), 0, "ainda é o mesmo tick");
        // O fim do tick seguinte do tile.
        tracker.check_tick_changed(11);
        assert_eq!(tracker.last_energy_received(), 1_250);
        tracker.check_tick_changed(11);
        assert_eq!(
            tracker.last_energy_received(),
            1_250,
            "o mesmo tick não conta duas vezes"
        );
        tracker.check_tick_changed(12);
        assert_eq!(tracker.last_energy_received(), 0);
    }

    #[test]
    fn a_receive_in_a_new_tick_closes_the_previous_one_first() {
        let tracker = LastEnergyTracker::default();
        tracker.received(5, 20);
        tracker.received(7, 21);
        assert_eq!(tracker.last_energy_received(), 5);
        tracker.check_tick_changed(22);
        assert_eq!(tracker.last_energy_received(), 7);
    }
}
