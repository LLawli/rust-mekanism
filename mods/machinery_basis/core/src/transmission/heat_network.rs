//! `HeatNetwork`: a rede de calor, sobre a `DynamicNetwork` sem buffer. Guarda a temperatura média e o que a última rodada moveu
//! (`meanTemp`, `heatLost`, `heatTransferred`), que o leitor de rede e a tela mostram.
//!
//! A topologia (os condutores, a fusão, a invalidação) é do [`super::registry::TransmitterNetworkRegistry`]; aqui está o `onUpdate`. O
//! dono dos condutores o chama de [`super::TransmitterHost::on_update`], com os condutores da rede na ordem de
//! [`super::dynamic_network::DynamicNetwork::transmitters`], e implementa [`ConductorSet`] por cima do mundo dele.
use super::{NetworkEvent, NetworkId, TransmitterId};
use crate::heat::{AMBIENT_TEMP, HeatTransfer};
use std::collections::BTreeMap;

/// O que o `onUpdate` pede aos condutores.
pub trait ConductorSet {
    /// `transmitter.simulate(transaction)`, dentro da transação raiz aberta por `onUpdate`.
    fn simulate(&mut self, conductor: TransmitterId) -> HeatTransfer;
    /// `transaction.commit()` da raiz: cada capacitor que mudou chama o ouvinte dele (`onContentsChanged`).
    fn commit(&mut self);
    /// `transmitter.getTemperature()`.
    fn temperature(&mut self, conductor: TransmitterId) -> f64;
}

/// `HeatNetwork`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatNetwork {
    mean_temp: f64,
    heat_lost: f64,
    heat_transferred: f64,
}

impl Default for HeatNetwork {
    fn default() -> Self {
        Self::new()
    }
}

/// A porcentagem de `getFlowInfo` para o calor transferido e o perdido que o leitor de rede mostra.
pub fn flow_efficiency_percent(heat_transferred: f64, heat_lost: f64) -> Option<f32> {
    if heat_transferred + heat_lost == 0.0 {
        return None;
    }
    let scaled = heat_transferred / (heat_transferred + heat_lost) * 10_000.0;
    // `Math.round(double)`: `floor(x + 0.5)`, em `long`; a divisão é de `long` por `float`.
    let rounded = (scaled + 0.5).floor() as i64;
    Some(rounded as f32 / 100.0_f32)
}

impl HeatNetwork {
    /// Uma rede nova: `meanTemp = HeatAPI.AMBIENT_TEMP` e nenhuma troca. A fusão não copia nada das redes adotadas.
    pub fn new() -> Self {
        Self {
            mean_temp: AMBIENT_TEMP,
            heat_lost: 0.0,
            heat_transferred: 0.0,
        }
    }

    /// `meanTemp`, o que `getStoredInfo` mostra.
    pub fn mean_temp(&self) -> f64 {
        self.mean_temp
    }

    /// `heatLost`: a temperatura que o ar levou na última rodada.
    pub fn heat_lost(&self) -> f64 {
        self.heat_lost
    }

    /// `heatTransferred`: a temperatura que passou entre vizinhos na última rodada.
    pub fn heat_transferred(&self) -> f64 {
        self.heat_transferred
    }

    /// A porcentagem de `getFlowInfo` (`Math.round(heatTransferred / (heatTransferred + heatLost) * 10_000) / 100F`), ou `None` quando a
    /// soma é zero e o texto não leva a eficiência.
    pub fn flow_efficiency_percent(&self) -> Option<f32> {
        flow_efficiency_percent(self.heat_transferred, self.heat_lost)
    }

    /// `onUpdate()`: cada condutor simula numa transação só, a transação confirma, e só então se soma a temperatura (um condutor que
    /// recebeu calor de outro já atualizado entra com a temperatura final). Uma rede sem condutores deixa `meanTemp` em NaN (0 / 0).
    pub fn on_update(&mut self, set: &mut dyn ConductorSet, conductors: &[TransmitterId]) {
        let mut new_sum_temp = 0.0;
        let mut new_heat_lost = 0.0;
        let mut new_heat_transferred = 0.0;
        for &conductor in conductors {
            let transfer = set.simulate(conductor);
            new_heat_transferred += transfer.adjacent;
            new_heat_lost += transfer.environment;
        }
        set.commit();
        for &conductor in conductors {
            new_sum_temp += set.temperature(conductor);
        }
        self.heat_lost = new_heat_lost;
        self.heat_transferred = new_heat_transferred;
        self.mean_temp = new_sum_temp / conductors.len() as f64;
    }
}

/// O `HeatNetwork` de cada rede do registro, criado quando a rede nasce (`new HeatNetwork(Collection)`). O dono chama
/// [`HeatNetworks::network_event`] de `TransmitterHost::network_event` e [`HeatNetworks::on_update`] de `TransmitterHost::on_update`.
#[derive(Default)]
pub struct HeatNetworks {
    networks: BTreeMap<NetworkId, HeatNetwork>,
}

impl HeatNetworks {
    pub fn network_event(&mut self, network: NetworkId, event: &NetworkEvent) {
        if matches!(event, NetworkEvent::Created { .. }) {
            self.networks.insert(network, HeatNetwork::new());
        }
    }

    pub fn get(&self, network: NetworkId) -> Option<&HeatNetwork> {
        self.networks.get(&network)
    }

    /// `network.onUpdate()`, com os condutores que o registro tem nela.
    pub fn on_update(
        &mut self,
        network: NetworkId,
        set: &mut dyn ConductorSet,
        conductors: &[TransmitterId],
    ) {
        if let Some(heat) = self.networks.get_mut(&network) {
            heat.on_update(set, conductors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Condutores falsos: cada `simulate` devolve uma troca fixa e muda a temperatura dele, para ver a ordem.
    struct Fake {
        log: Vec<String>,
        temperatures: Vec<f64>,
    }

    impl ConductorSet for Fake {
        fn simulate(&mut self, conductor: TransmitterId) -> HeatTransfer {
            self.log.push(format!("simulate {}", conductor.0));
            self.temperatures[conductor.0 as usize] += 10.0;
            HeatTransfer {
                adjacent: f64::from(conductor.0) + 1.0,
                environment: 0.5,
            }
        }

        fn commit(&mut self) {
            self.log.push("commit".into());
        }

        fn temperature(&mut self, conductor: TransmitterId) -> f64 {
            self.log.push(format!("temperature {}", conductor.0));
            self.temperatures[conductor.0 as usize]
        }
    }

    #[test]
    fn a_new_network_is_at_the_ambient_and_idle() {
        let network = HeatNetwork::new();
        assert_eq!(network.mean_temp(), 300.0);
        assert_eq!(network.heat_lost(), 0.0);
        assert_eq!(network.heat_transferred(), 0.0);
    }

    #[test]
    fn the_update_simulates_all_then_commits_then_reads_the_temperatures() {
        let mut set = Fake {
            log: Vec::new(),
            temperatures: vec![100.0, 200.0, 300.0],
        };
        let mut network = HeatNetwork::new();
        network.on_update(
            &mut set,
            &[TransmitterId(2), TransmitterId(0), TransmitterId(1)],
        );
        assert_eq!(
            set.log,
            [
                "simulate 2",
                "simulate 0",
                "simulate 1",
                "commit",
                "temperature 2",
                "temperature 0",
                "temperature 1"
            ]
        );
        assert_eq!(network.heat_transferred(), 3.0 + 1.0 + 2.0);
        assert_eq!(network.heat_lost(), 1.5);
        assert_eq!(network.mean_temp(), (310.0 + 110.0 + 210.0) / 3.0);
    }

    #[test]
    fn a_network_without_conductors_has_no_mean() {
        let mut set = Fake {
            log: Vec::new(),
            temperatures: Vec::new(),
        };
        let mut network = HeatNetwork::new();
        network.on_update(&mut set, &[]);
        assert!(network.mean_temp().is_nan());
        assert_eq!(set.log, ["commit"]);
    }

    #[test]
    fn the_flow_efficiency_rounds_to_two_decimals_like_math_round() {
        let mut network = HeatNetwork::new();
        assert_eq!(network.flow_efficiency_percent(), None);
        network.heat_transferred = 3.0;
        network.heat_lost = 1.0;
        assert_eq!(network.flow_efficiency_percent(), Some(75.0));
        network.heat_transferred = 1.0;
        network.heat_lost = 2.0;
        // round(3333,33) = 3333
        assert_eq!(network.flow_efficiency_percent(), Some(33.33));
        network.heat_transferred = 0.0;
        network.heat_lost = 1.0;
        assert_eq!(network.flow_efficiency_percent(), Some(0.0));
        // `Math.round` arredonda a metade para cima: 0,5 vira 1
        network.heat_transferred = 5.0e-5;
        network.heat_lost = 1.0 - 5.0e-5;
        assert_eq!(network.flow_efficiency_percent(), Some(0.01));
    }

    #[test]
    fn a_network_is_created_by_the_event_and_a_merge_does_not_copy_the_old_values() {
        let mut networks = HeatNetworks::default();
        assert!(networks.get(NetworkId(0)).is_none());
        networks.network_event(NetworkId(0), &NetworkEvent::Committing);
        assert!(networks.get(NetworkId(0)).is_none());
        let uuid = super::super::Uuid::with_hash_code(1);
        networks.network_event(NetworkId(0), &NetworkEvent::Created { uuid });
        let mut set = Fake {
            log: Vec::new(),
            temperatures: vec![400.0],
        };
        networks.on_update(NetworkId(0), &mut set, &[TransmitterId(0)]);
        assert_eq!(networks.get(NetworkId(0)).unwrap().mean_temp(), 410.0);
        networks.network_event(NetworkId(1), &NetworkEvent::Created { uuid });
        assert_eq!(networks.get(NetworkId(1)), Some(&HeatNetwork::new()));
    }
}
