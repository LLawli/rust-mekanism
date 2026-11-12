//! O mundo simulado dos testes da rede de calor: condutores termodinâmicos (o `Transmitter` da `rede-conexao` e o registro da
//! `rede-registro`, ligados como o `TileEntityTransmitter` os liga), um Heat Generator mínimo por cima de um condutor e o ar de cada célula.
//! Quem chama o registro é este arquivo; o condutor, a rede de calor e a troca de calor são os da lib.
#![allow(dead_code)]
use machinery_basis_core::heat::{
    self, AMBIENT_TEMP, AmbientCache, AmbientSource, EPSILON, HeatCapacitors, HeatHandlers,
    HeatTransfer, TileHeatHandler,
};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos as Pos;
use machinery_basis_core::transmission::connection::{ConnectionType, face_ordinal};
use machinery_basis_core::transmission::heat_network::{ConductorSet, HeatNetworks};
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::thermodynamic_conductor::{
    ConductorTier, ConductorTile, ThermodynamicConductor,
};
use machinery_basis_core::transmission::transmitter::{
    BlockPos as TPos, Effect, NeighborTransmitter, Transmitter, TransmitterWorld, relative,
};
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub const DIMENSION: DimensionId = DimensionId(0);
/// `NeighborUpdater.UPDATE_ORDER`.
const UPDATE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

/// O `IHeatHandler` de um vizinho: o aceitador que o condutor guarda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sink {
    Conductor(u32),
    /// O capacitor do Heat Generator, visto de qualquer lado menos de baixo.
    Generator,
    /// O mesmo capacitor visto de baixo (`DefaultInsulationCapacitor`: sem isolação).
    GeneratorBottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Conductor(u32),
    Generator,
}

/// O que a rede fez, na ordem, para o teste comparar com o rastro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Update(i32),
    Commit(NetworkId),
    Invalidate(NetworkId),
    NetUpdate(NetworkId),
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
}

pub struct Cond {
    pub pos: TPos,
    tc: Option<ThermodynamicConductor<Sink>>,
    pub hash: i32,
    pub network: Option<NetworkId>,
    pub mark_joined: bool,
    /// Quantas vezes o `onContentsChanged` mandou o pacote de atualização.
    pub packets: Cell<u32>,
}

impl Cond {
    pub fn tc(&self) -> &ThermodynamicConductor<Sink> {
        self.tc.as_ref().expect("condutor em uso")
    }
}

/// O Heat Generator no que a rede lê dele: o capacitor (capacidade 10, condução 5, isolação 100 menos embaixo) e o `simulate`.
pub struct Gen {
    pub pos: TPos,
    heat: Cell<Option<f64>>,
    ambient: RefCell<AmbientCache>,
    pub last_transfer: Cell<f64>,
    pub last_env: Cell<f64>,
}

const GEN_CAPACITY: f64 = 10.0;
const GEN_INVERSE_CONDUCTION: f64 = 5.0;
const GEN_INSULATION: f64 = 100.0;
const THERMAL_EFFICIENCY: f64 = 0.5;

impl Gen {
    fn ambient_self(&self, src: &dyn AmbientSource) -> f64 {
        self.ambient
            .borrow_mut()
            .get(AmbientCache::SELF_INDEX, || src.read(None))
    }

    pub fn ambient_side(&self, side: Face, src: &dyn AmbientSource) -> f64 {
        self.ambient
            .borrow_mut()
            .get(face_ordinal(side), || src.read(Some(side)))
    }

    pub fn heat(&self, src: &dyn AmbientSource) -> f64 {
        if let Some(heat) = self.heat.get() {
            return heat;
        }
        let heat = (GEN_CAPACITY * self.ambient_self(src)).max(0.0);
        self.heat.set(Some(heat));
        heat
    }

    pub fn stored_heat(&self) -> Option<f64> {
        self.heat.get()
    }

    pub fn temperature(&self, src: &dyn AmbientSource) -> f64 {
        self.heat(src) / GEN_CAPACITY
    }

    pub fn handle_heat(&self, transfer: f64, src: &dyn AmbientSource) {
        if transfer.abs() > EPSILON {
            self.heat.set(Some((self.heat(src) + transfer).max(0.0)));
        }
    }
}

pub struct State {
    conds: Vec<Cond>,
    cells: HashMap<TPos, Block>,
    /// A ordem em que os block entities foram postos, que é a do tick.
    be_order: Vec<Block>,
    pub heater: Option<Gen>,
    origin: TPos,
    /// `ambient[y][x]`: os seis lados e o próprio bloco.
    ambient: Vec<Vec<[f64; 7]>>,
    heat_networks: HeatNetworks,
    pub events: Vec<Event>,
    hashes: VecDeque<i32>,
    pending_uuid: Option<Uuid>,
}

type Registry = TransmitterNetworkRegistry<Sink>;

/// O ar de uma célula da estrutura.
struct AmbientAt<'a> {
    st: &'a State,
    pos: TPos,
}

impl AmbientSource for AmbientAt<'_> {
    fn read(&self, side: Option<Face>) -> Option<f64> {
        let x = (self.pos[0] - self.st.origin[0]) as usize;
        let y = (self.pos[1] - self.st.origin[1]) as usize;
        Some(self.st.ambient[y][x][side.map_or(6, face_ordinal)])
    }
}

impl State {
    pub fn conductor(&self, x: i32) -> Option<&Cond> {
        match self.cells.get(&self.at(x, 0))? {
            Block::Conductor(id) => Some(&self.conds[*id as usize]),
            Block::Generator => None,
        }
    }

    pub fn at(&self, x: i32, y: i32) -> TPos {
        [self.origin[0] + x, self.origin[1] + y, self.origin[2]]
    }

    pub fn ambient_at(&self, pos: TPos) -> impl AmbientSource + '_ {
        AmbientAt { st: self, pos }
    }

    pub fn cond_pos(&self, id: u32) -> TPos {
        self.conds[id as usize].pos
    }

    pub fn x_of(&self, pos: TPos) -> i32 {
        pos[0] - self.origin[0]
    }

    fn id_at(&self, pos: TPos) -> Option<u32> {
        match self.cells.get(&pos)? {
            Block::Conductor(id) => Some(*id),
            Block::Generator => None,
        }
    }

    fn pos(p: TPos) -> Pos {
        Pos::new(p[0], p[1], p[2])
    }
}

impl TransmitterWorld for State {
    type Acceptor = Sink;

    fn is_block_loaded(&self, _: TPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: TPos) -> bool {
        false
    }

    fn transmitter_at(&self, pos: TPos, side: Face) -> Option<NeighborTransmitter> {
        let tc = self.conds[self.id_at(pos)? as usize].tc.as_ref()?;
        Some(NeighborTransmitter {
            supported: tc.transmitter.supported_transmission_types(),
            can_connect_opposite: tc.transmitter.can_connect(side.opposite(), self),
        })
    }

    /// `HeatHandlerManager.resolve`: nenhum capacitor onde a conexão do condutor é `NONE` (e o redstone não age aqui); o gerador dá o
    /// capacitor em todo lado.
    fn acceptor(&self, pos: TPos, from_side: Face) -> Option<Sink> {
        match self.cells.get(&pos)? {
            Block::Conductor(id) => {
                let tc = self.conds[*id as usize].tc.as_ref()?;
                (tc.transmitter.get_connection_type_raw(from_side) != ConnectionType::None)
                    .then_some(Sink::Conductor(*id))
            }
            Block::Generator => Some(if from_side == Face::Down {
                Sink::GeneratorBottom
            } else {
                Sink::Generator
            }),
        }
    }
}

impl TransmitterHost<Sink> for State {
    fn block_pos(&self, t: TransmitterId) -> Pos {
        Self::pos(self.conds[t.0 as usize].pos)
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        DIMENSION
    }

    fn is_valid(&self, t: TransmitterId) -> bool {
        self.conds[t.0 as usize].tc().transmitter.is_valid()
    }

    fn is_orphan(&self, t: TransmitterId) -> bool {
        self.conds[t.0 as usize].tc().transmitter.is_orphan()
    }

    fn set_orphan(&mut self, t: TransmitterId, orphan: bool) {
        self.conds[t.0 as usize]
            .tc
            .as_mut()
            .unwrap()
            .transmitter
            .set_orphan(orphan);
    }

    fn transmitter_network(&self, t: TransmitterId) -> Option<NetworkId> {
        self.conds[t.0 as usize].network
    }

    fn store_transmitter_network(&mut self, t: TransmitterId, network: Option<NetworkId>) {
        let cond = &mut self.conds[t.0 as usize];
        cond.network = network;
        // A cópia do `Transmitter` da `rede-conexao`: o UUID dela é só uma marca de "tem rede".
        cond.tc
            .as_mut()
            .unwrap()
            .transmitter
            .set_transmitter_network(network.map(|n| u128::from(n.0) + 1), false, &mut Vec::new());
    }

    fn identity_hash(&self, t: TransmitterId) -> i32 {
        self.conds[t.0 as usize].hash
    }

    fn request_update(&mut self, t: TransmitterId) {
        let x = self.x_of(self.conds[t.0 as usize].pos);
        self.events.push(Event::Update(x));
    }

    fn supports_transmission_type(&self, _: TransmitterId, _: TransmitterId) -> bool {
        true
    }

    fn is_valid_transmitter_basic(
        &self,
        t: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        let neighbor =
            TransmitterWorld::transmitter_at(self, self.conds[other.0 as usize].pos, side)
                .expect("o vizinho existe");
        self.conds[t.0 as usize]
            .tc()
            .transmitter
            .is_valid_transmitter_basic(&neighbor, side, self)
    }

    fn transmitter_at(&self, _: DimensionId, pos: Pos) -> Option<TransmitterId> {
        self.id_at([pos.x, pos.y, pos.z]).map(TransmitterId)
    }

    fn chunk_accessibility_change(&mut self, _: &mut Registry, _: TransmitterId, _: bool) {}

    fn refresh_acceptor_connections(&mut self, t: TransmitterId, side: Face) {
        let mut tc = self.conds[t.0 as usize].tc.take().unwrap();
        let mut out = Vec::new();
        tc.transmitter
            .refresh_acceptor_connections(side, &*self, &mut out);
        self.conds[t.0 as usize].tc = Some(tc);
        // Os efeitos de `refreshAcceptorConnections` são só o pacote de atualização.
        debug_assert!(out.iter().all(|e| matches!(e, Effect::SendUpdatePacket)));
    }

    fn acceptor(&self, t: TransmitterId, side: Face) -> Option<Sink> {
        let tc = self.conds[t.0 as usize].tc();
        tc.transmitter
            .can_connect_to_acceptor(side)
            .then(|| tc.transmitter.get_acceptor(side, self))
            .flatten()
    }

    fn on_update(&mut self, registry: &mut Registry, network: NetworkId) {
        self.events.push(Event::NetUpdate(network));
        let conductors = registry.network(network).transmitters();
        let mut heat_networks = std::mem::take(&mut self.heat_networks);
        {
            let mut set = Set {
                st: &*self,
                registry: &*registry,
            };
            heat_networks.on_update(network, &mut set, &conductors);
        }
        self.heat_networks = heat_networks;
    }

    fn network_event(&mut self, _: &Registry, network: NetworkId, event: NetworkEvent) {
        self.heat_networks.network_event(network, &event);
        match event {
            NetworkEvent::Created { uuid } => self.pending_uuid = Some(uuid),
            NetworkEvent::Merged { from } => {
                let uuid = self.pending_uuid.take().expect("Created vem antes");
                self.events.push(Event::Created {
                    network,
                    hash: uuid.hash_code(),
                    from,
                });
            }
            NetworkEvent::Committing => self.events.push(Event::Commit(network)),
            NetworkEvent::Invalidating(_) => self.events.push(Event::Invalidate(network)),
            _ => {}
        }
    }
}

/// Os `IHeatHandler` do mundo, por `Sink`. Só lê o mundo: o calor e o diário de cada capacitor são `Cell`.
struct Arena<'a> {
    st: &'a State,
}

impl Arena<'_> {
    fn heater(&self) -> (&Gen, AmbientAt<'_>) {
        let heater = self.st.heater.as_ref().expect("há gerador");
        (
            heater,
            AmbientAt {
                st: self.st,
                pos: heater.pos,
            },
        )
    }
}

impl HeatHandlers for Arena<'_> {
    type Handle = Sink;

    fn temperature(&self, h: Sink) -> f64 {
        match h {
            Sink::Conductor(i) => {
                let c = &self.st.conds[i as usize];
                c.tc().temperature(&AmbientAt {
                    st: self.st,
                    pos: c.pos,
                })
            }
            _ => {
                let (heater, amb) = self.heater();
                heater.temperature(&amb)
            }
        }
    }

    fn inverse_conduction(&self, h: Sink) -> f64 {
        match h {
            Sink::Conductor(i) => self.st.conds[i as usize].tc().inverse_conduction(),
            _ => GEN_INVERSE_CONDUCTION,
        }
    }

    fn heat_capacity(&self, h: Sink) -> f64 {
        match h {
            Sink::Conductor(i) => self.st.conds[i as usize].tc().heat_capacity(),
            _ => GEN_CAPACITY,
        }
    }

    fn handle_heat(&mut self, h: Sink, transfer: f64) {
        match h {
            Sink::Conductor(i) => {
                let c = &self.st.conds[i as usize];
                c.tc().handle_heat(
                    transfer,
                    &AmbientAt {
                        st: self.st,
                        pos: c.pos,
                    },
                );
            }
            _ => {
                let (heater, amb) = self.heater();
                heater.handle_heat(transfer, &amb);
            }
        }
    }
}

impl HeatCapacitors for Arena<'_> {
    fn inverse_insulation(&self, h: Sink) -> f64 {
        match h {
            Sink::Conductor(i) => self.st.conds[i as usize].tc().inverse_insulation(),
            Sink::Generator => GEN_INSULATION,
            Sink::GeneratorBottom => 0.0,
        }
    }
}

/// `HeatNetwork.onUpdate` sobre o mundo.
struct Set<'a> {
    st: &'a State,
    registry: &'a Registry,
}

impl ConductorSet for Set<'_> {
    fn simulate(&mut self, conductor: TransmitterId) -> HeatTransfer {
        let cond = &self.st.conds[conductor.0 as usize];
        let ambient = AmbientAt {
            st: self.st,
            pos: cond.pos,
        };
        let network = cond.network;
        let registry = self.registry;
        let network_has = move |p: TPos| {
            network.is_some_and(|n| {
                registry
                    .network(n)
                    .transmitter_at(Pos::new(p[0], p[1], p[2]))
                    .is_some()
            })
        };
        let tile = ConductorTile {
            conductor: cond.tc(),
            own: Sink::Conductor(conductor.0),
            world: self.st,
            ambient: &ambient,
            network_has: &network_has,
        };
        heat::simulate(&tile, &mut Arena { st: self.st })
    }

    fn commit(&mut self) {
        for cond in &self.st.conds {
            if let Some(tc) = &cond.tc {
                let ambient = AmbientAt {
                    st: self.st,
                    pos: cond.pos,
                };
                if let Some(changed) = tc.commit(&ambient)
                    && changed.send_update_packet
                {
                    cond.packets.set(cond.packets.get() + 1);
                }
            }
        }
    }

    fn temperature(&mut self, conductor: TransmitterId) -> f64 {
        Arena { st: self.st }.temperature(Sink::Conductor(conductor.0))
    }
}

/// O Heat Generator como `ITileHeatHandler`.
struct GenTile<'a> {
    st: &'a State,
    heater: &'a Gen,
}

impl TileHeatHandler<Sink> for GenTile<'_> {
    fn heat_capacitor(&self, side: Face) -> Option<Sink> {
        Some(if side == Face::Down {
            Sink::GeneratorBottom
        } else {
            Sink::Generator
        })
    }

    /// "Only allow adjacent heat transfer through the bottom face".
    fn adjacent(&self, side: Face) -> Option<Sink> {
        if side != Face::Down {
            return None;
        }
        TransmitterWorld::acceptor(self.st, relative(self.heater.pos, side), side.opposite())
    }

    fn ambient_temperature(&self, side: Face) -> f64 {
        self.heater.ambient_side(
            side,
            &AmbientAt {
                st: self.st,
                pos: self.heater.pos,
            },
        )
    }
}

pub struct World {
    pub registry: Registry,
    pub st: State,
}

impl World {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `tile_hashes` o `identityHashCode` de cada condutor novo.
    pub fn new(
        origin: TPos,
        ambient: Vec<Vec<[f64; 7]>>,
        uuid_hashes: Vec<i32>,
        tile_hashes: Vec<i32>,
    ) -> Self {
        let queue = Rc::new(RefCell::new(VecDeque::from(uuid_hashes)));
        let registry = TransmitterNetworkRegistry::new(move || {
            Uuid::with_hash_code(
                queue
                    .borrow_mut()
                    .pop_front()
                    .expect("mais redes do que o rastro tem"),
            )
        });
        Self {
            registry,
            st: State {
                conds: Vec::new(),
                cells: HashMap::new(),
                be_order: Vec::new(),
                heater: None,
                origin,
                ambient,
                heat_networks: HeatNetworks::default(),
                events: Vec::new(),
                hashes: VecDeque::from(tile_hashes),
                pending_uuid: None,
            },
        }
    }

    pub fn heat_networks(&self) -> &HeatNetworks {
        &self.st.heat_networks
    }

    // -----------------------------------------------------------------------------------------
    // Efeitos do transmissor
    // -----------------------------------------------------------------------------------------

    fn apply(&mut self, id: u32, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => self.notify_neighbors(self.st.conds[id as usize].pos),
                Effect::RefreshNeighbor { pos, side } => {
                    if let Some(other) = self.st.id_at(pos) {
                        self.with_tc(other, |t, w, out| t.refresh_connections_side(side, w, out));
                    }
                }
                Effect::RecheckConnections(mask) => {
                    self.with_tc(id, |t, w, out| t.base_recheck_connections(mask, w, out));
                }
                Effect::InvalidateTransmitter => {
                    self.registry
                        .invalidate_transmitter(&self.st, TransmitterId(id));
                }
                Effect::AcceptorChanged(side) => {
                    let network = self.st.conds[id as usize]
                        .network
                        .expect("só há AcceptorChanged com rede");
                    self.registry.network_acceptor_changed(
                        &self.st,
                        network,
                        TransmitterId(id),
                        side,
                    );
                }
                Effect::SetForceUpdate => {
                    self.st.conds[id as usize]
                        .tc
                        .as_mut()
                        .unwrap()
                        .transmitter
                        .set_force_update();
                }
                _ => {}
            }
        }
    }

    fn with_tc<R>(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut Transmitter<Sink>, &State, &mut Vec<Effect>) -> R,
    ) -> R {
        let mut tc = self.st.conds[id as usize].tc.take().unwrap();
        let mut out = Vec::new();
        let result = f(&mut tc.transmitter, &self.st, &mut out);
        self.st.conds[id as usize].tc = Some(tc);
        self.apply(id, out);
        result
    }

    /// Os vizinhos recebem `neighborChanged` sem orientação.
    fn notify_neighbors(&mut self, pos: TPos) {
        for side in UPDATE_ORDER {
            if let Some(id) = self.st.id_at(relative(pos, side)) {
                self.with_tc(id, |t, w, out| t.on_neighbor_block_change(None, w, out));
            }
        }
    }

    /// O `BlockCapabilityCache` de cada condutor vizinho, no lado que olha `pos`, se invalida.
    fn invalidate_capabilities(&mut self, pos: TPos) {
        for side in Face::ALL {
            let Some(id) = self.st.id_at(relative(pos, side)) else {
                continue;
            };
            let mut out = Vec::new();
            {
                let tc = self.st.conds[id as usize].tc();
                if let Some(dirty) = tc
                    .transmitter
                    .acceptor_cache
                    .refresh_listener_run(side.opposite(), tc.transmitter.is_valid())
                {
                    tc.transmitter.mark_dirty_acceptor(dirty, &mut out);
                }
            }
            self.apply(id, out);
        }
    }

    // -----------------------------------------------------------------------------------------
    // O que o jogador e o cenário fazem
    // -----------------------------------------------------------------------------------------

    pub fn place_conductor(&mut self, x: i32, tier: ConductorTier) {
        let pos = self.st.at(x, 0);
        let id = self.st.conds.len() as u32;
        let hash = self
            .st
            .hashes
            .pop_front()
            .expect("mais condutores que o rastro");
        let mut tc = ThermodynamicConductor::new(pos, tier);
        tc.transmitter.set_level(false);
        self.st.conds.push(Cond {
            pos,
            tc: Some(tc),
            hash,
            network: None,
            mark_joined: true,
            packets: Cell::new(0),
        });
        self.st.cells.insert(pos, Block::Conductor(id));
        self.st.be_order.push(Block::Conductor(id));
        self.invalidate_capabilities(pos);
        self.notify_neighbors(pos);
    }

    pub fn place_generator(&mut self, x: i32) {
        let pos = self.st.at(x, 1);
        self.st.heater = Some(Gen {
            pos,
            heat: Cell::new(None),
            ambient: RefCell::new(AmbientCache::default()),
            last_transfer: Cell::new(0.0),
            last_env: Cell::new(0.0),
        });
        self.st.cells.insert(pos, Block::Generator);
        self.st.be_order.push(Block::Generator);
        self.invalidate_capabilities(pos);
        self.notify_neighbors(pos);
    }

    pub fn break_conductor(&mut self, x: i32) {
        let pos = self.st.at(x, 0);
        let id = self.st.id_at(pos).expect("há um condutor");
        self.st.conds[id as usize]
            .tc
            .as_mut()
            .unwrap()
            .transmitter
            .removed = true;
        self.world_separate(id);
        self.st.cells.remove(&pos);
        self.st.be_order.retain(|b| *b != Block::Conductor(id));
        self.invalidate_capabilities(pos);
        self.notify_neighbors(pos);
    }

    /// Calor posto num condutor como um aquecedor faz: `handleHeat` numa transação raiz.
    pub fn heat_conductor(&mut self, x: i32, amount: f64) {
        let id = self.st.id_at(self.st.at(x, 0)).expect("há um condutor");
        let cond = &self.st.conds[id as usize];
        let ambient = AmbientAt {
            st: &self.st,
            pos: cond.pos,
        };
        cond.tc().handle_heat(amount, &ambient);
        if let Some(changed) = cond.tc().commit(&ambient)
            && changed.send_update_packet
        {
            cond.packets.set(cond.packets.get() + 1);
        }
    }

    pub fn heat_generator(&mut self, amount: f64) {
        let heater = self.st.heater.as_ref().expect("há gerador");
        heater.handle_heat(
            amount,
            &AmbientAt {
                st: &self.st,
                pos: heater.pos,
            },
        );
    }

    // -----------------------------------------------------------------------------------------
    // O tick
    // -----------------------------------------------------------------------------------------

    fn world_join(&mut self, id: u32, was_present: bool) {
        if !was_present {
            self.registry.track_transmitter(&self.st, TransmitterId(id));
        }
        if !self.st.conds[id as usize].tc().transmitter.loaded {
            self.st.conds[id as usize]
                .tc
                .as_mut()
                .unwrap()
                .transmitter
                .loaded = true;
            self.registry
                .register_orphan_transmitter(&self.st, TransmitterId(id));
        }
    }

    fn world_separate(&mut self, id: u32) {
        self.registry
            .untrack_transmitter(&self.st, TransmitterId(id));
        if self.st.conds[id as usize].tc().transmitter.loaded {
            self.st.conds[id as usize]
                .tc
                .as_mut()
                .unwrap()
                .transmitter
                .loaded = false;
            self.registry
                .invalidate_transmitter(&self.st, TransmitterId(id));
        }
    }

    /// `TileEntityTransmitter.onUpdateServer`.
    fn conductor_tick(&mut self, id: u32) {
        if self.st.conds[id as usize].mark_joined {
            self.world_join(id, false);
            self.st.conds[id as usize].mark_joined = false;
        }
        self.with_tc(id, |t, w, out| t.on_update_server(w, out));
    }

    /// `TileEntityHeatGenerator.onUpdateServer`, a parte do calor (sem lava, sem boost): `simulate` numa transação.
    fn generator_tick(&mut self) {
        let heater = self.st.heater.as_ref().expect("há gerador");
        let ambient = AmbientAt {
            st: &self.st,
            pos: heater.pos,
        };
        let ambient_temperature = heater.ambient_self(&ambient);
        let temperature = heater.temperature(&ambient);
        let heat_lost = THERMAL_EFFICIENCY * (temperature - ambient_temperature);
        heater.handle_heat(-heat_lost, &ambient);
        let tile = GenTile {
            st: &self.st,
            heater,
        };
        let loss = heat::simulate(&tile, &mut Arena { st: &self.st });
        heater.last_transfer.set(loss.adjacent);
        heater.last_env.set(loss.environment);
        // O `transaction.commit()` do gerador: o condutor de baixo, que recebeu calor nela, confirma o capacitor dele.
        for cond in &self.st.conds {
            if let Some(tc) = &cond.tc {
                let ambient = AmbientAt {
                    st: &self.st,
                    pos: cond.pos,
                };
                if let Some(changed) = tc.commit(&ambient)
                    && changed.send_update_packet
                {
                    cond.packets.set(cond.packets.get() + 1);
                }
            }
        }
    }

    /// Um tick do servidor: os block entities na ordem em que foram postos e o `ServerTickEvent.Post` do registro.
    pub fn tick(&mut self) {
        for block in self.st.be_order.clone() {
            match block {
                Block::Conductor(id) => self.conductor_tick(id),
                Block::Generator => self.generator_tick(),
            }
        }
        self.registry.on_tick(&mut self.st, true);
    }
}

pub fn ambient_default() -> Vec<Vec<[f64; 7]>> {
    vec![vec![[AMBIENT_TEMP; 7]; 8]; 2]
}
