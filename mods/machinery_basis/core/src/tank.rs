//! Tanque de um recurso tipado, em quantidades que passam de `i32`.

/// Versão do formato salvo dos tanques. É o contrato entre mods: mudar pede migração.
pub const FORMAT: i32 = 1;

/// Se a operação muda o tanque ou só diz o que aconteceria.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Simulate,
    Execute,
}

/// Um tanque: guarda um recurso de cada vez.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tank {
    resource: Option<String>,
    amount: u64,
    capacity: u64,
}

impl Tank {
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }

    /// Reconstrói um tanque salvo, ou o que um item traz. A quantidade fica como veio, mesmo acima da capacidade: no original
    /// `ResourceContainerType.copyToContainers` põe o conteúdo com `setContents`, que não limita (só `clampContents`, que o tile
    /// não chama, o faria), então um item com 5.000 mB num tanque de 1.000 enche o tile com 5.000 (decisão 243). Vazio não guarda recurso.
    pub fn restored(resource: Option<String>, amount: u64, capacity: u64) -> Self {
        Self {
            resource: resource.filter(|_| amount > 0),
            amount,
            capacity,
        }
    }

    pub fn resource(&self) -> Option<&str> {
        self.resource.as_deref()
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.amount == 0
    }

    /// O que ainda cabe: `capacity - amount` do original, sem passar de zero num tanque acima da capacidade (`needed <= 0`, nada entra).
    pub fn room(&self) -> u64 {
        self.capacity.saturating_sub(self.amount)
    }

    /// Coloca até `amount` de `resource` e diz quanto entrou.
    pub fn insert(&mut self, resource: &str, amount: u64, action: Action) -> u64 {
        if self
            .resource
            .as_deref()
            .is_some_and(|held| held != resource)
        {
            return 0;
        }
        let moved = amount.min(self.room());
        if action == Action::Execute && moved > 0 {
            self.resource = Some(resource.to_owned());
            self.amount += moved;
        }
        moved
    }

    /// Tira até `amount` e diz quanto saiu. Ao esvaziar, o tanque aceita outro recurso.
    pub fn extract(&mut self, amount: u64, action: Action) -> u64 {
        let moved = amount.min(self.amount);
        if action == Action::Execute {
            self.amount -= moved;
            if self.amount == 0 {
                self.resource = None;
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAS: &str = "a:gas";
    const OUTRO: &str = "a:outro";

    #[test]
    fn fills_up_to_capacity() {
        let mut tank = Tank::new(100);
        assert_eq!(tank.insert(GAS, 60, Action::Execute), 60);
        assert_eq!(tank.insert(GAS, 60, Action::Execute), 40);
        assert_eq!(tank.amount(), 100);
        assert_eq!(tank.insert(GAS, 1, Action::Execute), 0);
    }

    #[test]
    fn holds_one_resource_at_a_time() {
        let mut tank = Tank::new(100);
        tank.insert(GAS, 10, Action::Execute);
        assert_eq!(tank.insert(OUTRO, 10, Action::Execute), 0);
        assert_eq!(tank.resource(), Some(GAS));
    }

    #[test]
    fn empties_and_takes_another_resource() {
        let mut tank = Tank::new(100);
        tank.insert(GAS, 10, Action::Execute);
        assert_eq!(tank.extract(50, Action::Execute), 10);
        assert!(tank.is_empty());
        assert_eq!(tank.resource(), None);
        assert_eq!(tank.insert(OUTRO, 5, Action::Execute), 5);
    }

    #[test]
    fn simulating_changes_nothing() {
        let mut tank = Tank::new(100);
        assert_eq!(tank.insert(GAS, 10, Action::Simulate), 10);
        assert_eq!(tank, Tank::new(100));
        tank.insert(GAS, 10, Action::Execute);
        let before = tank.clone();
        assert_eq!(tank.extract(4, Action::Simulate), 4);
        assert_eq!(tank, before);
    }

    #[test]
    fn inserting_nothing_claims_nothing() {
        let mut tank = Tank::new(100);
        assert_eq!(tank.insert(GAS, 0, Action::Execute), 0);
        assert_eq!(tank.resource(), None);
    }

    #[test]
    fn holds_more_than_an_i32() {
        let capacity = 8_192_000_000_000;
        let mut tank = Tank::new(capacity);
        assert_eq!(tank.insert(GAS, u64::MAX, Action::Execute), capacity);
        assert_eq!(tank.room(), 0);
    }

    #[test]
    fn an_overfilled_tank_takes_nothing_and_gives_everything_back() {
        let mut over = Tank::restored(Some(GAS.into()), 5_000, 1_000);
        assert_eq!(over.insert(GAS, 10, Action::Execute), 0);
        assert_eq!(over.extract(4_000, Action::Execute), 4_000);
        assert_eq!(over.amount(), 1_000);
        assert_eq!(over.insert(GAS, 10, Action::Execute), 0);
    }

    #[test]
    fn a_restored_tank_stays_valid() {
        let over = Tank::restored(Some(GAS.into()), 500, 100);
        assert_eq!(over.amount(), 500);
        assert_eq!(over.room(), 0);
        let empty = Tank::restored(Some(GAS.into()), 0, 100);
        assert_eq!(empty.resource(), None);
    }
}
