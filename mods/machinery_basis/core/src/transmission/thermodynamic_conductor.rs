//! `ThermodynamicConductor`: o condutor de calor, um `Transmitter` com um capacitor (`VariableHeatCapacitor`) que troca calor com os
//! vizinhos e com o ar (`ITileHeatHandler`), e o `ConductorTier` que dá a capacidade, a condução e a isolação dele.
//!
//! O que fica de fora: o cliente (a rede do cliente, o modelo, a cor) e o `TileEntityThermodynamicConductor` (a capability `HEAT` que o tile
//! expõe, `sideChanged`, `redstoneChanged`), que são do dono dos transmissores. O `takeShare` do condutor é vazio (o calor fica no capacitor
//! quando a rede se desfaz): o gancho `TransmitterHost::take_share` já é vazio por padrão.
//!
//! # O capacitor
//!
//! `BasicHeatCapacitor` guarda `storedHeat = -1` até alguém ler o calor; a primeira leitura (`getHeat`, `getTemperature`, ou o primeiro
//! `handleHeat`) o põe em `capacidade * ar`, com o ar lido do mundo e guardado (`CachedAmbientTemperature`). Aqui `heat` é um `Cell<Option>`
//! pelo mesmo motivo: a temperatura de um condutor ainda não inicializado, vista por um vizinho que simula antes dele, o inicializa.
//! A capacidade vem da config quando o condutor nasce e depois só muda ao ler o NBT; a condução (`max(1, ...)`) e a isolação são lidas a
//! cada uso (`VariableHeatCapacitor`), então [`ThermodynamicConductor::set_tier_values`] é o que o dono chama quando a config muda.
//!
//! # A transação
//!
//! `BasicHeatCapacitor` é um `SnapshotJournal`: o primeiro `handleHeat` de uma transação guarda o estado de antes, e no commit da raiz,
//! se o calor ou a capacidade mudaram, chama o ouvinte (`onContentsChanged`). Aqui [`ThermodynamicConductor::handle_heat`] guarda o estado
//! de antes e [`ThermodynamicConductor::commit`] é o commit da raiz. A rede de calor abre uma raiz só para todos os condutores (e os
//! vizinhos que recebem calor), então o dono chama `commit` de cada capacitor que mudou depois do último `simulate`.

use super::connection::{
    ConnectionType, TransmissionSet, TransmissionType, connection_map_contains_side, face_ordinal,
};
use super::transmitter::{
    BlockPos, Compound, Effect, Tag, Transmitter, TransmitterWorld, relative,
};
use crate::heat::{AmbientCache, AmbientSource, EPSILON, TileHeatHandler};
use crate::sides::Face;
use std::cell::{Cell, RefCell};

/// As chaves de NBT do capacitor e do pacote de atualização (`SerializationConstants`).
pub mod keys {
    /// `HEAT_CAPACITOR`: o filho do NBT do tile.
    pub const HEAT_CAPACITOR: &str = "heat_capacitor";
    /// `STATE`: o filho do capacitor.
    pub const STATE: &str = "state";
    /// `HEAT_STORED`: o calor, no estado e no pacote de atualização.
    pub const HEAT_STORED: &str = "heat";
    /// `HEAT_CAPACITY`.
    pub const HEAT_CAPACITY: &str = "heat_capacity";
}

/// `ConductorTier`, os quatro níveis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConductorTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

/// Os três números de um nível, como o `tiers.toml` os dá (`ConductorTier.getInverseConduction`, `getHeatCapacity`,
/// `getInverseConductionInsulation`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierValues {
    pub inverse_conduction: f64,
    pub heat_capacity: f64,
    pub insulation: f64,
}

impl ConductorTier {
    pub const ALL: [ConductorTier; 4] = [
        ConductorTier::Basic,
        ConductorTier::Advanced,
        ConductorTier::Elite,
        ConductorTier::Ultimate,
    ];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// Os valores de fábrica (`baseConduction`, `baseHeatCapacity`, `baseConductionInsulation`). O mínimo da config é 1 para a condução e
    /// a capacidade e 0 para a isolação (`TierConfig.addThermodynamicConductorsCategory`).
    pub fn base_values(self) -> TierValues {
        TierValues {
            inverse_conduction: 5.0,
            heat_capacity: 1.0,
            insulation: match self {
                ConductorTier::Basic => 10.0,
                ConductorTier::Advanced => 400.0,
                ConductorTier::Elite => 8_000.0,
                ConductorTier::Ultimate => 100_000.0,
            },
        }
    }
}

/// Um `HeatCapacitorData` guardado: o calor, se já estava inicializado, e a capacidade.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Snapshot {
    heat: Option<f64>,
    capacity: f64,
}

/// `IHeatCapacitor.CapacitorState`: o que o NBT do capacitor guarda.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapacitorState {
    pub heat: f64,
    pub heat_capacity: f64,
}

impl CapacitorState {
    /// O codec do estado: o calor (padrão 0, mínimo 0) e a capacidade (padrão 1, mínimo 1). Um valor fora do mínimo faz o estado todo
    /// falhar (`deserialize` não lê nada).
    pub fn decode(heat: Option<f64>, heat_capacity: Option<f64>) -> Option<Self> {
        let heat = heat.unwrap_or(0.0);
        let heat_capacity = heat_capacity.unwrap_or(1.0);
        (heat >= 0.0 && heat_capacity >= 1.0).then_some(Self {
            heat,
            heat_capacity,
        })
    }
}

/// O que `onContentsChanged` do condutor pede ao tile. O `setChanged` acontece sempre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentsChanged {
    /// `getTransmitterTile().sendUpdatePacket()`.
    pub send_update_packet: bool,
}

/// O NBT do tile: o do transmissor e o filho `heat_capacitor`/`state` (`ContainerType.HEAT.saveTo`).
#[derive(Clone, Debug, PartialEq)]
pub struct ConductorTag {
    pub transmitter: Compound,
    pub capacitor: CapacitorState,
}

/// `ThermodynamicConductorUpgradeData`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductorUpgradeData {
    pub redstone_reactive: bool,
    pub connection_types: [ConnectionType; 6],
    pub heat: f64,
}

/// `Mth.equal(a, b)`: `abs(b - a) < 1.0E-5F`.
fn mth_equal(a: f64, b: f64) -> bool {
    (b - a).abs() < f64::from(1.0e-5_f32)
}

/// O `ThermodynamicConductor`. `A` é o aceitador, que no calor é o identificador do `IHeatHandler` do vizinho.
pub struct ThermodynamicConductor<A: Clone> {
    pub transmitter: Transmitter<A>,
    pub tier: ConductorTier,
    /// A condução e a isolação vivas (a config); a capacidade daqui só vale até o `buffer` nascer.
    values: TierValues,
    /// `BasicHeatCapacitor.heatCapacity`.
    capacity: Cell<f64>,
    /// `storedHeat`, `None` enquanto vale -1.
    heat: Cell<Option<f64>>,
    ambient: RefCell<AmbientCache>,
    /// `clientTemperature`, -1 até o primeiro `onContentsChanged` no servidor.
    client_temperature: Cell<f64>,
    /// O estado de antes do primeiro `handleHeat` da transação em curso.
    journal: Cell<Option<Snapshot>>,
}

impl<A: Clone> ThermodynamicConductor<A> {
    /// Um condutor novo, com os valores de fábrica do nível. `handles_redstone` é verdadeiro.
    pub fn new(pos: BlockPos, tier: ConductorTier) -> Self {
        Self::with_values(pos, tier, tier.base_values())
    }

    /// Um condutor com os valores que a config dá agora.
    pub fn with_values(pos: BlockPos, tier: ConductorTier, values: TierValues) -> Self {
        let mut transmitter =
            Transmitter::new(pos, TransmissionSet::of(&[TransmissionType::Heat]), true);
        transmitter.any_connected_acceptor_valid = true;
        Self {
            transmitter,
            tier,
            values,
            capacity: Cell::new(values.heat_capacity),
            heat: Cell::new(None),
            ambient: RefCell::new(AmbientCache::default()),
            client_temperature: Cell::new(-1.0),
            journal: Cell::new(None),
        }
    }

    /// A config mudou: a condução e a isolação são lidas a cada uso, a capacidade não muda.
    pub fn set_tier_values(&mut self, values: TierValues) {
        self.values = values;
    }

    /// `getAmbientTemperature(side)` (`CachedAmbientTemperature.getTemperature(side)`).
    pub fn ambient_temperature(&self, side: Face, source: &dyn AmbientSource) -> f64 {
        self.ambient
            .borrow_mut()
            .get(face_ordinal(side), || source.read(Some(side)))
    }

    /// `ambientTemperature.getAsDouble()`: o ar do próprio bloco.
    fn ambient_self(&self, source: &dyn AmbientSource) -> f64 {
        self.ambient
            .borrow_mut()
            .get(AmbientCache::SELF_INDEX, || source.read(None))
    }

    /// `getHeat`: inicializa o calor na primeira leitura (`max(0, capacidade * ar)`).
    pub fn heat(&self, source: &dyn AmbientSource) -> f64 {
        if let Some(heat) = self.heat.get() {
            return heat;
        }
        let heat = (self.capacity.get() * self.ambient_self(source)).max(0.0);
        self.heat.set(Some(heat));
        heat
    }

    /// O calor guardado, sem inicializar (`None` é o -1).
    pub fn stored_heat(&self) -> Option<f64> {
        self.heat.get()
    }

    /// `getHeatCapacity`.
    pub fn heat_capacity(&self) -> f64 {
        self.capacity.get()
    }

    /// `getTemperature`.
    pub fn temperature(&self, source: &dyn AmbientSource) -> f64 {
        self.heat(source) / self.capacity.get()
    }

    /// `VariableHeatCapacitor.getInverseConduction`: `max(1, condução)`.
    pub fn inverse_conduction(&self) -> f64 {
        self.values.inverse_conduction.max(1.0)
    }

    /// `VariableHeatCapacitor.getInverseInsulation`.
    pub fn inverse_insulation(&self) -> f64 {
        self.values.insulation
    }

    /// `clientTemperature`.
    pub fn client_temperature(&self) -> f64 {
        self.client_temperature.get()
    }

    /// `updateSnapshots(transaction)`: guarda o estado de antes na primeira vez da transação.
    fn update_snapshots(&self) {
        if self.journal.get().is_none() {
            self.journal.set(Some(Snapshot {
                heat: self.heat.get(),
                capacity: self.capacity.get(),
            }));
        }
    }

    /// `handleHeat(transfer, transaction)`: uma troca menor que o épsilon não faz nada.
    pub fn handle_heat(&self, transfer: f64, source: &dyn AmbientSource) {
        if transfer.abs() > EPSILON {
            self.update_snapshots();
            let heat = (self.heat(source) + transfer).max(0.0);
            self.heat.set(Some(heat));
        }
    }

    /// `setHeat(heat, transaction)` com uma transação (o `parseUpgradeData`): o ouvinte só roda no commit da raiz.
    pub fn set_heat_in_transaction(&self, heat: f64) {
        assert!(heat >= 0.0, "o calor não pode ser negativo");
        self.update_snapshots();
        self.heat.set(Some(heat));
    }

    /// `onRootCommit(originalState)`: o commit da raiz da transação. Se o calor ou a capacidade mudaram em relação a antes do primeiro
    /// `handleHeat`, é um `onContentsChanged`.
    pub fn commit(&self, source: &dyn AmbientSource) -> Option<ContentsChanged> {
        let original = self.journal.take()?;
        let changed = self.heat.get().unwrap_or(-1.0) != original.heat.unwrap_or(-1.0)
            || !mth_equal(self.capacity.get(), original.capacity);
        changed.then(|| self.on_contents_changed(source))
    }

    /// `onContentsChanged`: no servidor guarda a temperatura enviada e manda o pacote de atualização quando ela se afastou mais que um
    /// vigésimo da atual; o `setChanged` do tile acontece sempre.
    pub fn on_contents_changed(&self, source: &dyn AmbientSource) -> ContentsChanged {
        let mut send_update_packet = false;
        if !self.transmitter.is_client_side() {
            if self.client_temperature.get() == -1.0 {
                self.client_temperature.set(self.ambient_self(source));
            }
            let temperature = self.temperature(source);
            if (temperature - self.client_temperature.get()).abs() > (temperature / 20.0) {
                self.client_temperature.set(temperature);
                send_update_packet = true;
            }
        }
        ContentsChanged { send_update_packet }
    }

    /// `setHeatAndCapacity(heat, capacity, null)`: a leitura do NBT. Os dois mudam numa transação raiz só.
    fn set_state(
        &self,
        state: CapacitorState,
        source: &dyn AmbientSource,
    ) -> Option<ContentsChanged> {
        self.update_snapshots();
        self.capacity.set(state.heat_capacity);
        self.heat.set(Some(state.heat));
        self.commit(source)
    }

    // -----------------------------------------------------------------------------------------
    // ITileHeatHandler
    // -----------------------------------------------------------------------------------------

    /// `getAdjacent(side)`: só onde o mapa de conexões tem o lado, e pelo cache de aceitadores (a rede conta os outros condutores como
    /// aceitadores, então não há o que distinguir aqui).
    pub fn get_adjacent<W: TransmitterWorld<Acceptor = A>>(
        &self,
        side: Face,
        world: &W,
    ) -> Option<A> {
        if !connection_map_contains_side(self.transmitter.get_all_current_connections(), side) {
            return None;
        }
        let pos = self.transmitter.pos();
        self.transmitter
            .acceptor_cache
            .get_connected_acceptor(side, &|s| world.acceptor(relative(pos, s), s.opposite()))
    }

    /// `countsAsAdjacent(side)`: sem rede conta, e com rede só conta se o vizinho é um condutor dela. O comentário do original diz o
    /// contrário ("condutor para condutor não conta como transferência adjacente"); o código é o que vale e é portado como está
    /// (comportamento do original que parece bug e fica). `network_has` pergunta à rede se há um condutor em `pos`.
    pub fn counts_as_adjacent(&self, side: Face, network_has: &dyn Fn(BlockPos) -> bool) -> bool {
        !self.transmitter.has_transmitter_network()
            || network_has(relative(self.transmitter.pos(), side))
    }

    // -----------------------------------------------------------------------------------------
    // NBT, pacote de atualização e upgrade
    // -----------------------------------------------------------------------------------------

    /// `write(output)`: o do transmissor e o capacitor. `getHeat` inicializa o calor.
    pub fn write(&self, source: &dyn AmbientSource) -> ConductorTag {
        ConductorTag {
            transmitter: self.transmitter.write(),
            capacitor: CapacitorState {
                heat: self.heat(source),
                heat_capacity: self.capacity.get(),
            },
        }
    }

    /// `read(input)`: o do transmissor e, se o filho `heat_capacitor/state` existe e é válido, o capacitor
    /// (`setHeatAndCapacity(heat, capacity, null)`: a capacidade do NBT vale no lugar da da config).
    pub fn read(
        &mut self,
        transmitter: &Compound,
        capacitor: Option<CapacitorState>,
        source: &dyn AmbientSource,
        out: &mut Vec<Effect>,
    ) -> Option<ContentsChanged> {
        self.transmitter.read(transmitter, out);
        capacitor.and_then(|state| self.set_state(state, source))
    }

    /// `writeReducedUpdatedTag(output)`: o do transmissor e o calor (`putDouble("heat", getHeat())`).
    pub fn write_reduced_updated_tag(&self, source: &dyn AmbientSource) -> Compound {
        let mut tag = self.transmitter.write_reduced_updated_tag();
        tag.insert(keys::HEAT_STORED.into(), Tag::Double(self.heat(source)));
        tag
    }

    /// `handleUpdateTag(input)`, no cliente: o do transmissor e `setHeat(getDoubleOr("heat", getHeat()), null)`.
    pub fn handle_update_tag(
        &mut self,
        tag: &Compound,
        source: &dyn AmbientSource,
        out: &mut Vec<Effect>,
    ) -> super::transmitter::UpdateTag {
        let result = self.transmitter.handle_update_tag(tag, out);
        let heat = match tag.get(keys::HEAT_STORED) {
            Some(Tag::Double(value)) => *value,
            _ => self.heat(source),
        };
        assert!(heat >= 0.0, "o calor não pode ser negativo");
        self.update_snapshots();
        self.heat.set(Some(heat));
        self.commit(source);
        result
    }

    /// `getUpgradeData()`.
    pub fn upgrade_data(&self, source: &dyn AmbientSource) -> ConductorUpgradeData {
        ConductorUpgradeData {
            redstone_reactive: self.transmitter.redstone_reactive,
            connection_types: *self.transmitter.connection_types_raw(),
            heat: self.heat(source),
        }
    }

    /// `parseUpgradeData(data, transaction)`: o calor entra na transação do dono, que chama [`Self::commit`] no fim dela.
    pub fn parse_upgrade_data(&mut self, data: &ConductorUpgradeData) {
        self.transmitter.redstone_reactive = data.redstone_reactive;
        self.transmitter
            .set_connection_types_raw(data.connection_types);
        self.set_heat_in_transaction(data.heat);
    }
}

/// O tile do condutor como `ITileHeatHandler`, para uma chamada de [`crate::heat::simulate`]. O dono monta um por condutor, com o que o
/// mundo responde: o identificador do capacitor do próprio condutor (`own`), o mundo dos transmissores (os aceitadores), o ar e a rede.
pub struct ConductorTile<'a, A: Copy, W: TransmitterWorld<Acceptor = A>> {
    pub conductor: &'a ThermodynamicConductor<A>,
    pub own: A,
    pub world: &'a W,
    pub ambient: &'a dyn AmbientSource,
    /// `getTransmitterNetworkNN().getTransmitter(pos) != null`.
    pub network_has: &'a dyn Fn(BlockPos) -> bool,
}

impl<A: Copy, W: TransmitterWorld<Acceptor = A>> TileHeatHandler<A> for ConductorTile<'_, A, W> {
    fn heat_capacitor(&self, _side: Face) -> Option<A> {
        Some(self.own)
    }

    fn adjacent(&self, side: Face) -> Option<A> {
        self.conductor.get_adjacent(side, self.world)
    }

    fn ambient_temperature(&self, side: Face) -> f64 {
        self.conductor.ambient_temperature(side, self.ambient)
    }

    fn counts_as_adjacent(&self, side: Face) -> bool {
        self.conductor.counts_as_adjacent(side, self.network_has)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heat::AMBIENT_TEMP;
    use std::cell::Cell;

    /// Um ar que conta as leituras do mundo.
    struct Air {
        value: Cell<f64>,
        reads: Cell<u32>,
    }

    impl Air {
        fn new(value: f64) -> Self {
            Self {
                value: Cell::new(value),
                reads: Cell::new(0),
            }
        }
    }

    impl AmbientSource for Air {
        fn read(&self, _side: Option<Face>) -> Option<f64> {
            self.reads.set(self.reads.get() + 1);
            Some(self.value.get())
        }
    }

    struct Void;

    impl AmbientSource for Void {
        fn read(&self, _side: Option<Face>) -> Option<f64> {
            None
        }
    }

    fn server_conductor() -> ThermodynamicConductor<()> {
        let mut conductor = ThermodynamicConductor::new([0, 0, 0], ConductorTier::Basic);
        conductor.transmitter.set_level(false);
        conductor
    }

    #[test]
    fn the_tiers_differ_only_in_insulation() {
        let values: Vec<TierValues> = ConductorTier::ALL.iter().map(|t| t.base_values()).collect();
        assert!(
            values
                .iter()
                .all(|v| v.inverse_conduction == 5.0 && v.heat_capacity == 1.0)
        );
        let insulation: Vec<f64> = values.iter().map(|v| v.insulation).collect();
        assert_eq!(insulation, [10.0, 400.0, 8_000.0, 100_000.0]);
    }

    #[test]
    fn the_inverse_conduction_is_at_least_one() {
        let mut conductor = server_conductor();
        assert_eq!(conductor.inverse_conduction(), 5.0);
        conductor.set_tier_values(TierValues {
            inverse_conduction: 0.25,
            heat_capacity: 1.0,
            insulation: 0.0,
        });
        assert_eq!(conductor.inverse_conduction(), 1.0);
        assert_eq!(conductor.inverse_insulation(), 0.0);
    }

    #[test]
    fn the_heat_starts_at_the_air_of_the_first_read_and_the_air_is_read_once() {
        let conductor = server_conductor();
        let air = Air::new(310.0);
        assert_eq!(conductor.stored_heat(), None);
        assert_eq!(conductor.heat(&air), 310.0);
        air.value.set(250.0);
        assert_eq!(conductor.heat(&air), 310.0);
        assert_eq!(conductor.ambient_temperature(Face::Up, &air), 250.0);
        assert_eq!(conductor.ambient_temperature(Face::Up, &air), 250.0);
        assert_eq!(air.reads.get(), 2);
    }

    #[test]
    fn without_a_world_the_air_is_300_and_is_not_kept() {
        let conductor = server_conductor();
        assert_eq!(conductor.heat(&Void), 300.0);
        let other = server_conductor();
        assert_eq!(other.ambient_temperature(Face::North, &Void), AMBIENT_TEMP);
        let air = Air::new(280.0);
        assert_eq!(other.ambient_temperature(Face::North, &air), 280.0);
    }

    #[test]
    fn a_first_handle_heat_initializes_the_heat_before_adding() {
        let conductor = server_conductor();
        let air = Air::new(300.0);
        conductor.handle_heat(50.0, &air);
        assert_eq!(conductor.stored_heat(), Some(350.0));
    }

    #[test]
    fn tiny_transfers_do_nothing_and_the_heat_never_goes_negative() {
        let conductor = server_conductor();
        let air = Air::new(300.0);
        conductor.handle_heat(EPSILON, &air);
        assert_eq!(conductor.stored_heat(), None);
        assert!(conductor.commit(&air).is_none());
        conductor.handle_heat(-1.0e9, &air);
        assert_eq!(conductor.stored_heat(), Some(0.0));
    }

    #[test]
    fn the_listener_sends_a_packet_only_when_far_from_the_last_one_sent() {
        let conductor = server_conductor();
        let air = Air::new(300.0);
        conductor.heat(&air);
        conductor.handle_heat(100.0, &air);
        // client = ar (300); |400 - 300| = 100 > 400 / 20
        assert_eq!(
            conductor.commit(&air),
            Some(ContentsChanged {
                send_update_packet: true
            })
        );
        assert_eq!(conductor.client_temperature(), 400.0);
        conductor.handle_heat(10.0, &air);
        // |410 - 400| = 10, não passa de 410 / 20 = 20,5
        assert_eq!(
            conductor.commit(&air),
            Some(ContentsChanged {
                send_update_packet: false
            })
        );
        assert_eq!(conductor.client_temperature(), 400.0);
        conductor.handle_heat(15.0, &air);
        // |425 - 400| = 25 > 21,25
        assert_eq!(
            conductor.commit(&air),
            Some(ContentsChanged {
                send_update_packet: true
            })
        );
        assert_eq!(conductor.client_temperature(), 425.0);
    }

    #[test]
    fn a_transaction_that_ends_where_it_started_is_silent() {
        let conductor = server_conductor();
        let air = Air::new(300.0);
        conductor.heat(&air);
        conductor.handle_heat(100.0, &air);
        conductor.handle_heat(-100.0, &air);
        assert!(conductor.commit(&air).is_none());
        assert_eq!(conductor.client_temperature(), -1.0);
    }

    #[test]
    fn the_first_change_of_an_uninitialized_conductor_always_counts() {
        // O snapshot de antes tem o calor "vazio" (-1): qualquer valor guardado é diferente dele.
        let conductor = server_conductor();
        let air = Air::new(300.0);
        conductor.update_snapshots();
        conductor.heat(&air);
        assert!(conductor.commit(&air).is_some());
    }

    #[test]
    fn a_client_conductor_never_sends_the_packet() {
        let conductor = ThermodynamicConductor::<()>::new([0, 0, 0], ConductorTier::Basic);
        let air = Air::new(300.0);
        conductor.heat(&air);
        conductor.handle_heat(500.0, &air);
        assert_eq!(
            conductor.commit(&air),
            Some(ContentsChanged {
                send_update_packet: false
            })
        );
        assert_eq!(conductor.client_temperature(), -1.0);
    }

    #[test]
    fn the_capacitor_state_codec_has_defaults_and_minimums() {
        assert_eq!(
            CapacitorState::decode(None, None),
            Some(CapacitorState {
                heat: 0.0,
                heat_capacity: 1.0
            })
        );
        assert_eq!(CapacitorState::decode(Some(-0.5), None), None);
        assert_eq!(CapacitorState::decode(None, Some(0.99)), None);
        assert_eq!(
            CapacitorState::decode(Some(12.5), Some(3.0)),
            Some(CapacitorState {
                heat: 12.5,
                heat_capacity: 3.0
            })
        );
    }

    #[test]
    fn the_nbt_round_trip_keeps_the_heat_and_the_saved_capacity_wins() {
        let air = Air::new(300.0);
        let source = server_conductor();
        source.handle_heat(40.0, &air);
        let tag = source.write(&air);
        assert_eq!(
            tag.capacitor,
            CapacitorState {
                heat: 340.0,
                heat_capacity: 1.0
            }
        );
        let mut loaded = server_conductor();
        loaded.set_tier_values(TierValues {
            heat_capacity: 9.0,
            ..ConductorTier::Basic.base_values()
        });
        let state = CapacitorState {
            heat: 340.0,
            heat_capacity: 4.0,
        };
        let changed = loaded.read(&tag.transmitter, Some(state), &air, &mut Vec::new());
        assert!(changed.is_some());
        assert_eq!(loaded.stored_heat(), Some(340.0));
        assert_eq!(loaded.heat_capacity(), 4.0);
        assert_eq!(loaded.temperature(&air), 85.0);
    }

    #[test]
    fn reading_without_a_capacitor_tag_changes_nothing() {
        let air = Air::new(300.0);
        let mut loaded = server_conductor();
        assert!(
            loaded
                .read(&Compound::new(), None, &air, &mut Vec::new())
                .is_none()
        );
        assert_eq!(loaded.stored_heat(), None);
    }

    #[test]
    fn a_new_capacity_alone_counts_as_a_change_by_mth_equal() {
        let air = Air::new(300.0);
        let mut loaded = server_conductor();
        loaded.heat(&air);
        let same_heat = CapacitorState {
            heat: 300.0,
            heat_capacity: 1.0,
        };
        assert!(
            loaded
                .read(&Compound::new(), Some(same_heat), &air, &mut Vec::new())
                .is_none()
        );
        let bigger = CapacitorState {
            heat: 300.0,
            heat_capacity: 1.000_001,
        };
        assert!(
            loaded
                .read(&Compound::new(), Some(bigger), &air, &mut Vec::new())
                .is_none()
        );
        let bigger = CapacitorState {
            heat: 300.0,
            heat_capacity: 1.1,
        };
        assert!(
            loaded
                .read(&Compound::new(), Some(bigger), &air, &mut Vec::new())
                .is_some()
        );
    }

    #[test]
    fn the_update_tag_carries_the_heat_as_a_double() {
        let air = Air::new(300.0);
        let conductor = server_conductor();
        let tag = conductor.write_reduced_updated_tag(&air);
        assert_eq!(tag.get(keys::HEAT_STORED), Some(&Tag::Double(300.0)));
        let mut client = ThermodynamicConductor::<()>::new([0, 0, 0], ConductorTier::Elite);
        client.handle_update_tag(&tag, &air, &mut Vec::new());
        assert_eq!(client.stored_heat(), Some(300.0));
        let mut tag = tag;
        tag.insert(keys::HEAT_STORED.into(), Tag::Double(123.0));
        client.handle_update_tag(&tag, &air, &mut Vec::new());
        assert_eq!(client.stored_heat(), Some(123.0));
        tag.remove(keys::HEAT_STORED);
        client.handle_update_tag(&tag, &air, &mut Vec::new());
        assert_eq!(client.stored_heat(), Some(123.0));
    }

    #[test]
    fn the_upgrade_data_moves_the_connections_the_redstone_and_the_heat() {
        let air = Air::new(300.0);
        let mut old = server_conductor();
        old.transmitter.redstone_reactive = true;
        let mut types = [ConnectionType::Normal; 6];
        types[2] = ConnectionType::None;
        old.transmitter.set_connection_types_raw(types);
        old.handle_heat(7.0, &air);
        let data = old.upgrade_data(&air);
        assert_eq!(data.heat, 307.0);
        let mut next = ThermodynamicConductor::<()>::new([0, 0, 0], ConductorTier::Advanced);
        next.transmitter.set_level(false);
        next.parse_upgrade_data(&data);
        assert!(next.transmitter.redstone_reactive);
        assert_eq!(next.transmitter.connection_types_raw(), &types);
        assert_eq!(next.stored_heat(), Some(307.0));
        // O ouvinte só roda no commit da raiz.
        assert!(next.commit(&air).is_some());
    }

    #[test]
    fn a_conductor_counts_any_connected_acceptor_as_valid() {
        assert!(server_conductor().transmitter.any_connected_acceptor_valid);
    }
}
