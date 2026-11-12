//! O mundo simulado dos testes das redes de recurso com buffer (fluido e químico): canos mecânicos ou tubos pressurizados (o `Transmitter`
//! da `rede-conexao`, o registro da `rede-registro`, a família de recurso da lib) sobre os chunks falsos de `sim::chunks`, ligados como o
//! `TileEntityTransmitter` os liga, e tanques como vizinhos. Quem chama o registro é este arquivo; o cano, a rede e a fusão são os da lib.
//!
//! É genérico sobre o [`Kind`]: o recurso, o tipo de transmissão, os valores dos tiers do cano e os tanques. [`FluidKind`] e
//! [`ChemicalKind`] são os dois que existem.
#![allow(dead_code, type_alias_bounds)]
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::resource_tank::{Resource, ResourceTank, StackContents};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, ChunkPos, LevelView, SimLevel};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::buffered_resource_network::{
    ResourceAcceptor, ResourceExtract, ResourceFamily,
};
use machinery_basis_core::transmission::buffered_transmitter::{
    extra_valid_transmitter, merge_networks_manually, recheck_connections_sides, requests_update,
};
use machinery_basis_core::transmission::chemical_network::ChemicalFamily;
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::fluid_network::FluidFamily;
use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
use machinery_basis_core::transmission::pressurized_tube::TubeTier;
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::transmitter::{
    BlockPos as TPos, Compound, Effect, NeighborTransmitter, Transmitter, TransmitterWorld,
    relative,
};
use machinery_basis_core::transmission::validator::CompatibleTransmitterValidator;
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// O bloco de um tanque.
pub const TANK: &str = "test:tank";
/// O prefixo do bloco do cano: `test:pipe_<tier>`.
pub const PIPE_PREFIX: &str = "test:pipe_";
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

/// O que muda de uma rede de recurso para outra: o recurso, o tipo de transmissão, os valores dos tiers do cano e dos tanques.
pub trait Kind: 'static {
    type R: Resource;
    /// `TransmissionType` do transmissor.
    const TRANSMISSION: TransmissionType;
    /// O nome dos arquivos do rastro: `<NAME>_<cenário>.jsonl`.
    const NAME: &'static str;
    /// A chave do recurso nas linhas do rastro (`fluid` ou `chem`).
    const KEY: &'static str;
    /// A chave do recurso do `saveShare` (`shareFluid` ou `shareChem`).
    const SHARE_KEY: &'static str;
    /// O recurso de um id.
    fn resource(id: &str) -> Self::R;
    /// O id de um recurso.
    fn id(resource: &Self::R) -> String;
    /// A capacidade e a vazão do tier do cano (`basic`, `advanced`, `elite`, `ultimate`).
    fn pipe(tier: &str) -> (i64, i32);
    /// A capacidade do tanque do tier (`creative` é a máxima).
    fn tank_capacity(tier: &str) -> u64;
    /// A família da lib para este recurso (a de químico registra os `disperse`).
    fn family() -> Family<Self>
    where
        Self: Sized;
    /// O Heat Generator (`generator` do rastro): só o fluido tem.
    fn place_generator(_world: &mut World<Self>, _pos: BlockPos) -> Rc<Tank<Self>>
    where
        Self: Sized,
    {
        panic!("só o fluido tem o Heat Generator")
    }
}

/// O fluido: `MechanicalPipe` e o Fluid Tank.
pub struct FluidKind;
/// O químico: `PressurizedTube` e o Chemical Tank.
pub struct ChemicalKind;

impl Kind for FluidKind {
    type R = FluidResource;
    const TRANSMISSION: TransmissionType = TransmissionType::Fluid;
    const NAME: &'static str = "fluid";
    const KEY: &'static str = "fluid";
    const SHARE_KEY: &'static str = "shareFluid";

    fn resource(id: &str) -> FluidResource {
        FluidResource::new(id)
    }

    fn id(resource: &FluidResource) -> String {
        resource.id.to_string()
    }

    fn pipe(tier: &str) -> (i64, i32) {
        let tier = match tier {
            "basic" => PipeTier::Basic,
            "advanced" => PipeTier::Advanced,
            "elite" => PipeTier::Elite,
            "ultimate" => PipeTier::Ultimate,
            other => panic!("tier desconhecido: {other}"),
        };
        (tier.capacity(), tier.transfer_rate())
    }

    fn tank_capacity(tier: &str) -> u64 {
        match tier {
            "basic" => 32_000,
            "advanced" => 64_000,
            "elite" => 128_000,
            "ultimate" => 256_000,
            "creative" => i64::MAX as u64,
            other => panic!("tier de tanque desconhecido: {other}"),
        }
    }

    fn family() -> Family<Self> {
        FluidFamily::new()
    }

    fn place_generator(world: &mut World<Self>, pos: BlockPos) -> Rc<Tank<Self>> {
        world.place_generator(pos)
    }
}

impl Kind for ChemicalKind {
    type R = ChemicalResource;
    const TRANSMISSION: TransmissionType = TransmissionType::Chemical;
    const NAME: &'static str = "chemical";
    const KEY: &'static str = "chem";
    const SHARE_KEY: &'static str = "shareChem";

    fn resource(id: &str) -> ChemicalResource {
        ChemicalResource::new(id)
    }

    fn id(resource: &ChemicalResource) -> String {
        resource.id.to_string()
    }

    fn pipe(tier: &str) -> (i64, i32) {
        let tier = match tier {
            "basic" => TubeTier::Basic,
            "advanced" => TubeTier::Advanced,
            "elite" => TubeTier::Elite,
            "ultimate" => TubeTier::Ultimate,
            other => panic!("tier desconhecido: {other}"),
        };
        (tier.capacity(), tier.transfer_rate())
    }

    /// `ChemicalTankTier`: 64, 256, 1.024 e 8.192 baldes; o criativo, `Long.MAX_VALUE`.
    fn tank_capacity(tier: &str) -> u64 {
        match tier {
            "basic" => 64_000,
            "advanced" => 256_000,
            "elite" => 1_024_000,
            "ultimate" => 8_192_000,
            "creative" => i64::MAX as u64,
            other => panic!("tier de tanque desconhecido: {other}"),
        }
    }

    fn family() -> Family<Self> {
        ChemicalFamily::chemical()
    }
}

/// O aceitador que o vizinho dá ao cano (`ResourceHandler<RESOURCE>`).
pub type Acceptor<K: Kind> = Rc<dyn ResourceAcceptor<K::R>>;
pub type Family<K: Kind> = ResourceFamily<K::R, dyn ResourceAcceptor<K::R>>;
type Registry<K: Kind> = TransmitterNetworkRegistry<Acceptor<K>>;

pub fn fluid(id: &str) -> FluidResource {
    FluidKind::resource(id)
}

pub fn chemical(id: &str) -> ChemicalResource {
    ChemicalKind::resource(id)
}

/// Um tanque vizinho (o Fluid Tank de um tier, ou o `lavaTank` do Heat Generator): o contêiner, e os lados em que ele expõe a capability
/// de fluido.
pub struct Tank<K: Kind> {
    pub tank: ResourceTank<K::R>,
    /// Os lados (por `Face::ALL`) que expõem a capability.
    pub sides: [bool; 6],
    /// O tanque criativo (`FluidTankFluidTank.isCreative`): `insert` e `extract` pela capability dão o resultado sem mudar nada.
    pub creative: bool,
    /// O que o gerador queima por tick (`heatGenerationFluidRate`), ou 0.
    pub burn: i32,
    /// `currentActive` do gerador, o `active` do estado do bloco (`getClientActive`) e a espera de desligar (`updateDelay`).
    pub active: std::cell::Cell<bool>,
    pub client_active: std::cell::Cell<bool>,
    pub update_delay: std::cell::Cell<i32>,
    /// O contêiner de energia do gerador está cheio (`EnergyHandlerUtil.isFull`): vem de fora, porque a conta de calor e de energia é do
    /// gerador e não da rede.
    pub energy_full: std::cell::Cell<bool>,
}

impl<K: Kind> Tank<K> {
    pub fn exposes(&self, side: Face) -> bool {
        self.sides[Face::ALL.iter().position(|f| *f == side).unwrap()]
    }

    /// Um tick do Heat Generator (`TileEntityMekanism.tickServer` e `TileEntityHeatGenerator.onUpdateServer`, a parte do `lavaTank` e do
    /// `setActive`): a espera de desligar anda, o gerador tira a taxa do `lavaTank` (`INTERNAL`) se ela inteira couber e o contêiner de
    /// energia não está cheio, e `setActive` liga na hora e desliga na hora só se a espera está zerada (e a reinicia sempre).
    ///
    /// Devolve se o estado de ativo do bloco mudou, que o mundo avisa aos vizinhos com um `setBlockAndUpdate`.
    pub fn burn_tick(&self) -> bool {
        if self.burn == 0 {
            return false;
        }
        let mut notify = false;
        if self.update_delay.get() > 0 {
            self.update_delay.set(self.update_delay.get() - 1);
            if self.update_delay.get() == 0 && self.client_active.get() != self.active.get() {
                self.client_active.set(self.active.get());
                notify = true;
            }
        }
        let mut is_active = false;
        if !self.energy_full.get()
            && let Some(resource) = self.tank.resource()
        {
            let transaction = Transaction::open_root();
            if self
                .tank
                .extract(&resource, self.burn, &transaction, AutomationType::Internal)
                == self.burn
            {
                is_active = true;
                transaction.commit();
            }
        }
        if is_active != self.active.get() {
            self.active.set(is_active);
            if self.client_active.get() != is_active {
                if is_active {
                    self.client_active.set(true);
                    notify = true;
                } else {
                    if self.update_delay.get() == 0 {
                        self.client_active.set(false);
                        notify = true;
                    }
                    self.update_delay.set(BLOCK_DEACTIVATION_DELAY);
                }
            }
        }
        notify
    }
}

/// `general.blockDeactivationDelay`: 3 segundos.
const BLOCK_DEACTIVATION_DELAY: i32 = 60;

impl<K: Kind> ResourceInsert<K::R> for Tank<K> {
    fn insert(&self, resource: &K::R, amount: i32, transaction: &Transaction) -> i32 {
        if self.creative {
            // Aceita sem mudar nada: o resultado de uma simulação.
            let simulation = Transaction::open(Some(transaction));
            return self
                .tank
                .insert(resource, amount, &simulation, AutomationType::External);
        }
        self.tank
            .insert(resource, amount, transaction, AutomationType::External)
    }
}

impl<K: Kind> ResourceExtract<K::R> for Tank<K> {
    fn size(&self) -> usize {
        1
    }

    fn resource_at(&self, _: usize) -> Option<K::R> {
        self.tank.resource()
    }

    fn amount_as_int_at(&self, _: usize) -> i32 {
        self.tank.amount_as_int()
    }

    fn extract_at(&self, _: usize, resource: &K::R, amount: i32, transaction: &Transaction) -> i32 {
        self.extract(resource, amount, transaction)
    }

    fn extract(&self, resource: &K::R, amount: i32, transaction: &Transaction) -> i32 {
        if self.creative {
            let simulation = Transaction::open(Some(transaction));
            return self
                .tank
                .extract(resource, amount, &simulation, AutomationType::External);
        }
        self.tank
            .extract(resource, amount, transaction, AutomationType::External)
    }
}

impl<K: Kind> ResourceAcceptor<K::R> for Tank<K> {}

/// O `MechanicalPipe` e o `TileEntityMechanicalPipe` dele, no que a rede lê.
pub struct Pipe<K: Kind> {
    pub pos: BlockPos,
    /// Fora do lugar enquanto uma operação o usa (veja `Ctx::with_tr`).
    pub tr: Option<Transmitter<Acceptor<K>>>,
    /// `markJoined`.
    pub mark_joined: bool,
    /// A cópia da rede e do órfão que o `Transmitter` guarda, para ler com ele fora do lugar.
    pub network: Option<NetworkId>,
    pub orphan: bool,
    /// O `System.identityHashCode` do transmissor.
    pub hash: i32,
}

impl<K: Kind> Pipe<K> {
    pub fn tr(&self) -> &Transmitter<Acceptor<K>> {
        self.tr.as_ref().expect("cano em uso")
    }

    /// `hasTransmitterNetwork()`.
    pub fn net(&self) -> Option<NetworkId> {
        self.network.filter(|_| !self.orphan)
    }
}

/// O que a rede fez, na ordem, para o teste comparar com o rastro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Update(BlockPos),
    Commit(NetworkId),
    Invalidate(NetworkId),
    NetUpdate(NetworkId),
    Adopt {
        network: NetworkId,
        from: NetworkId,
    },
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
    Access(BlockPos, bool),
}

pub struct State<K: Kind> {
    pub pipes: Vec<Pipe<K>>,
    /// O `identityHashCode` do próximo cano posto em cada posição.
    pub hash_for: Box<dyn FnMut(BlockPos) -> i32>,
    pub events: Vec<Event>,
    pub runs_normally: bool,
    pub family: Family<K>,
    /// O cano de cada posição (veja `Ctx::id_at`).
    by_pos: BTreeMap<BlockPos, TransmitterId>,
    /// Os tanques postos, por posição.
    pub tanks: BTreeMap<BlockPos, Rc<Tank<K>>>,
    pending_uuid: Option<Uuid>,
    /// A transação raiz que `DynamicNetwork.invalidate` abre em volta dos `takeShare` (`begin_take_shares`).
    share_tx: Option<Transaction>,
    /// A fusão manual está rodando: só o `adoptTransmittersAndAcceptorsFrom` dela vai para o rastro (o do construtor aparece em `created`).
    manual_merge: bool,
}

pub struct Hooks<K: Kind> {
    pub registry: Registry<K>,
    pub state: State<K>,
}

/// O que o registro enxerga num instante: os canos e os chunks do nível.
pub struct Ctx<'a, K: Kind> {
    pub st: &'a mut State<K>,
    pub level: &'a LevelView<TransmitterId>,
}

fn tpos(pos: BlockPos) -> TPos {
    [pos.x, pos.y, pos.z]
}

fn spos(pos: TPos) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

impl<K: Kind> Ctx<'_, K> {
    pub fn pipe(&self, id: TransmitterId) -> &Pipe<K> {
        &self.st.pipes[id.0 as usize]
    }

    fn pipe_mut(&mut self, id: TransmitterId) -> &mut Pipe<K> {
        &mut self.st.pipes[id.0 as usize]
    }

    /// `WorldUtils.getTileEntity(TileEntityTransmitter.class, level, pos)`: o cano do bloco em `pos` se o chunk está carregado. Lê o
    /// mapa do mundo de teste e não o do nível, porque o nível entrega o block entity ao gancho e o deixa fora do mapa enquanto ele roda.
    fn id_at(&self, pos: TPos) -> Option<TransmitterId> {
        let pos = spos(pos);
        if !self.level.is_block_loaded(pos) {
            return None;
        }
        let id = *self.st.by_pos.get(&pos)?;
        self.level
            .block_if_loaded(pos)
            .is_some_and(|block| block.starts_with(PIPE_PREFIX))
            .then_some(id)
    }

    /// Roda uma operação do `Transmitter` com ele fora do lugar (ela lê o mundo, e o mundo guarda o transmissor), e aplica os
    /// efeitos que ela pediu.
    pub fn with_tr<T>(
        &mut self,
        registry: &mut Registry<K>,
        id: TransmitterId,
        f: impl FnOnce(&mut Transmitter<Acceptor<K>>, &Self, &mut Vec<Effect>) -> T,
    ) -> T {
        let mut tr = self.pipe_mut(id).tr.take().expect("cano em uso");
        let mut out = Vec::new();
        let result = f(&mut tr, &*self, &mut out);
        self.pipe_mut(id).tr = Some(tr);
        self.apply(registry, id, out);
        result
    }

    fn apply(&mut self, registry: &mut Registry<K>, id: TransmitterId, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => {
                    self.notify_neighbors(registry, tpos(self.pipe(id).pos))
                }
                Effect::RefreshNeighbor { pos, side } => {
                    if let Some(other) = self.id_at(pos) {
                        self.with_tr(registry, other, |t, w, out| {
                            t.refresh_connections_side(side, w, out);
                        });
                    }
                }
                Effect::RecheckConnections(mask) => {
                    // `BufferedTransmitter.recheckConnections`.
                    let net = self.pipe(id).net();
                    match recheck_connections_sides(net.is_some(), true, mask) {
                        None => {
                            self.with_tr(registry, id, |t, w, out| {
                                t.base_recheck_connections(mask, w, out);
                            });
                        }
                        Some(sides) => {
                            for side in sides {
                                self.recheck_connection_prechecked(registry, id, side);
                            }
                        }
                    }
                }
                Effect::RecheckConnection(side) => {
                    // `BufferedTransmitter.recheckConnection`.
                    if self.pipe(id).net().is_some() {
                        self.recheck_connection_prechecked(registry, id, side);
                    }
                }
                Effect::RequestsUpdate => self.requests_update(registry, id),
                Effect::InvalidateTransmitter => {
                    registry.invalidate_transmitter(&*self, id);
                }
                Effect::AcceptorChanged(side) => {
                    let network = self
                        .pipe(id)
                        .network
                        .expect("só há AcceptorChanged com rede");
                    registry.network_acceptor_changed(&*self, network, id, side);
                }
                Effect::SetForceUpdate => {
                    self.pipe_mut(id).tr.as_mut().unwrap().set_force_update();
                }
                _ => {}
            }
        }
    }

    /// `BufferedTransmitter.requestsUpdate()`: o `update` do rastro e as conexões relidas.
    fn requests_update(&mut self, registry: &mut Registry<K>, id: TransmitterId) {
        let pos = self.pipe(id).pos;
        self.st.events.push(Event::Update(pos));
        self.with_tr(registry, id, |t, w, out| requests_update(t, true, w, out));
    }

    /// `BufferedTransmitter.recheckConnectionPrechecked(side)`.
    fn recheck_connection_prechecked(
        &mut self,
        registry: &mut Registry<K>,
        id: TransmitterId,
        side: Face,
    ) {
        let pos = self.pipe(id).pos;
        let Some(other) = self.level.block_entity(pos.relative(side)).copied() else {
            return;
        };
        let (Some(network), Some(other_network)) = (self.pipe(id).net(), self.pipe(other).net())
        else {
            return;
        };
        let Some((primary, secondary)) = self.st.family.plan_merge(id, network, other_network)
        else {
            return;
        };
        self.st.manual_merge = true;
        let adopted = merge_networks_manually(registry, self, primary, secondary);
        self.st.manual_merge = false;
        self.st.family.clamp_buffer(primary);
        self.with_tr(registry, other, |t, w, out| {
            t.refresh_connections_side(side.opposite(), w, out);
        });
        for transmitter in adopted {
            self.requests_update(registry, transmitter);
        }
    }

    /// Os vizinhos recebem `neighborChanged` sem orientação.
    fn notify_neighbors(&mut self, registry: &mut Registry<K>, pos: TPos) {
        for side in UPDATE_ORDER {
            if let Some(id) = self.id_at(relative(pos, side)) {
                self.with_tr(registry, id, |t, w, out| {
                    t.on_neighbor_block_change(None, w, out);
                });
            }
        }
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(&mut self, registry: &mut Registry<K>, id: TransmitterId, was_present: bool) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.pipe(id).tr().loaded {
            self.pipe_mut(id).tr.as_mut().unwrap().loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(
        &mut self,
        registry: &mut Registry<K>,
        id: TransmitterId,
        still_present: bool,
    ) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.pipe(id).tr().loaded {
            self.pipe_mut(id).tr.as_mut().unwrap().loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

impl<K: Kind> TransmitterWorld for Ctx<'_, K> {
    type Acceptor = Acceptor<K>;

    fn is_block_loaded(&self, pos: TPos) -> bool {
        self.level.is_block_loaded(spos(pos))
    }

    fn is_getting_powered(&self, _: TPos) -> bool {
        false
    }

    fn transmitter_at(&self, pos: TPos, side: Face) -> Option<NeighborTransmitter> {
        let tr = self.pipe(self.id_at(pos)?).tr.as_ref()?;
        Some(NeighborTransmitter {
            supported: tr.supported_transmission_types(),
            can_connect_opposite: tr.can_connect(side.opposite(), self),
        })
    }

    /// A capability de fluido do vizinho: o tanque, nos lados em que ele a expõe.
    fn acceptor(&self, pos: TPos, from_side: Face) -> Option<Acceptor<K>> {
        let pos = spos(pos);
        if self.level.block_if_loaded(pos) != Some(TANK) {
            return None;
        }
        let tank = self.st.tanks.get(&pos)?;
        tank.exposes(from_side).then(|| tank.clone() as Acceptor<K>)
    }

    /// `MechanicalPipe.isValidTransmitter(transmitter, side)`, a parte que a regra básica não tem: `BufferedTransmitter` (rede contra
    /// órfão) e o conteúdo dos buffers.
    fn extra_valid_transmitter(&self, me: TPos, other: TPos, _side: Face) -> bool {
        let (Some(a), Some(b)) = (self.id_at(me), self.id_at(other)) else {
            return true;
        };
        let (pa, pb) = (self.pipe(a), self.pipe(b));
        let ends = |p: &Pipe<K>| self.st.family.ends(p.net().is_some(), p.orphan);
        extra_valid_transmitter(ends(pa), ends(pb))
            && self
                .st
                .family
                .is_valid_transmitter(a, pa.net(), b, pb.net())
    }
}

impl<K: Kind> TransmitterHost<Acceptor<K>> for Ctx<'_, K> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.pipe(transmitter).pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        DIMENSION
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        self.pipe(transmitter).tr().is_valid()
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.pipe(transmitter).orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        let pipe = self.pipe_mut(transmitter);
        pipe.orphan = orphan;
        if let Some(tr) = pipe.tr.as_mut() {
            tr.set_orphan(orphan);
        }
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.pipe(transmitter).network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        let pipe = self.pipe_mut(transmitter);
        pipe.network = network;
        // A cópia do `Transmitter` da `rede-conexao`: o UUID dela é só uma marca de "tem rede".
        if let Some(tr) = pipe.tr.as_mut() {
            tr.set_transmitter_network(
                network.map(|n| u128::from(n.0) + 1),
                false,
                &mut Vec::new(),
            );
        }
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.pipe(transmitter).hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        panic!("o registro chama request_update_in; {transmitter:?}");
    }

    fn request_update_in(&mut self, registry: &mut Registry<K>, transmitter: TransmitterId) {
        self.requests_update(registry, transmitter);
    }

    fn supports_transmission_type(&self, _: TransmitterId, _: TransmitterId) -> bool {
        true
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        let neighbor = TransmitterWorld::transmitter_at(self, tpos(self.pipe(other).pos), side)
            .expect("o vizinho existe");
        self.pipe(transmitter)
            .tr()
            .is_valid_transmitter_basic(&neighbor, side, self)
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.id_at(tpos(pos))
    }

    fn new_orphan_validator(
        &self,
        transmitter: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<Acceptor<K>>> {
        let pipe = self.pipe(transmitter);
        Box::new(self.st.family.new_orphan_validator(transmitter, pipe.net()))
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut Registry<K>,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        let pos = self.pipe(transmitter).pos;
        self.st.events.push(Event::Access(pos, loaded));
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            let network = self.pipe(transmitter).net();
            self.st
                .family
                .validate_and_take_share(registry, transmitter, network, None);
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, t: TransmitterId, side: Face) {
        let mut tr = self.pipe_mut(t).tr.take().unwrap();
        let mut out = Vec::new();
        tr.refresh_acceptor_connections(side, &*self, &mut out);
        self.pipe_mut(t).tr = Some(tr);
        // Os efeitos de `refreshAcceptorConnections` são só o pacote de atualização.
        debug_assert!(out.iter().all(|e| matches!(e, Effect::SendUpdatePacket)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Acceptor<K>> {
        let tr = self.pipe(transmitter).tr();
        tr.can_connect_to_acceptor(side)
            .then(|| tr.get_acceptor(side, self))
            .flatten()
    }

    fn begin_take_shares(&mut self) {
        self.st.share_tx = Some(Transaction::open_root());
    }

    fn end_take_shares(&mut self) {
        self.st
            .share_tx
            .take()
            .expect("begin_take_shares abriu a raiz")
            .commit();
        self.st.family.flush_contents();
    }

    fn take_share(&mut self, transmitter: TransmitterId) {
        let network = self.pipe(transmitter).net();
        match self.st.share_tx.take() {
            Some(transaction) => {
                self.st
                    .family
                    .take_share(transmitter, network, Some(&transaction));
                self.st.share_tx = Some(transaction);
            }
            None => self.st.family.take_share_in_root(transmitter, network),
        }
    }

    fn on_update(&mut self, registry: &mut Registry<K>, network: NetworkId) {
        self.st.events.push(Event::NetUpdate(network));
        self.st.family.on_update(registry, network);
    }

    fn network_event(&mut self, registry: &Registry<K>, network: NetworkId, event: NetworkEvent) {
        self.st
            .family
            .network_event(registry, network, event.clone());
        match event {
            NetworkEvent::Created { uuid } => self.st.pending_uuid = Some(uuid),
            NetworkEvent::Merged { from } => {
                let uuid = self.st.pending_uuid.take().expect("Created vem antes");
                self.st.events.push(Event::Created {
                    network,
                    hash: uuid.hash_code(),
                    from,
                });
            }
            NetworkEvent::Adopted { from } if self.st.manual_merge => {
                self.st.events.push(Event::Adopt { network, from });
            }
            NetworkEvent::Committing => self.st.events.push(Event::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.events.push(Event::Invalidate(network)),
            _ => {}
        }
    }
}

/// O que um cano grava no chunk: o NBT do `Transmitter` e o `stored`.
pub type Saved<K: Kind> = (Compound, StackContents<K::R>);

impl<K: Kind> ChunkHooks for Hooks<K> {
    type BlockEntity = TransmitterId;
    type Saved = Saved<K>;

    fn new_block_entity(
        &mut self,
        pos: BlockPos,
        block: &str,
        saved: Option<&Saved<K>>,
    ) -> Option<TransmitterId> {
        let (capacity, rate) = K::pipe(block.strip_prefix(PIPE_PREFIX)?);
        let hash = (self.state.hash_for)(pos);
        let mut tr: Transmitter<Acceptor<K>> =
            Transmitter::new(tpos(pos), TransmissionSet::of(&[K::TRANSMISSION]), true);
        let id = TransmitterId(self.state.pipes.len() as u32);
        if let Some((tag, stored)) = saved {
            tr.read(tag, &mut Vec::new());
            self.state.family.add_pipe(id, pos, capacity, rate);
            self.state.family.read_stored(id, stored.clone());
        } else {
            self.state.family.add_pipe(id, pos, capacity, rate);
        }
        self.state.by_pos.insert(pos, id);
        self.state.pipes.push(Pipe {
            pos,
            tr: Some(tr),
            mark_joined: false,
            network: None,
            orphan: true,
            hash,
        });
        Some(id)
    }

    /// `MechanicalPipe.write`: valida as shares e grava o `saveShare`, e o NBT do `Transmitter`.
    fn save_block_entity(&mut self, _: BlockPos, entity: &mut TransmitterId) -> Saved<K> {
        let network = self.state.pipes[entity.0 as usize].net();
        let stored = self
            .state
            .family
            .write_stored(&self.registry, *entity, network);
        (self.state.pipes[entity.0 as usize].tr().write(), stored)
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block.starts_with(PIPE_PREFIX)
    }

    fn ticket_level_updated(
        &mut self,
        _: &LevelView<TransmitterId>,
        chunk: ChunkPos,
        old: i32,
        new: i32,
    ) {
        self.registry
            .on_ticket_level_change(DIMENSION, chunk.pack(), old, new);
    }

    /// `TileEntityTransmitter.clearRemoved`, `:146`, no servidor; `setLevel` antes.
    fn block_entity_added(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let pipe = &mut self.state.pipes[entity.0 as usize];
        let tr = pipe.tr.as_mut().unwrap();
        tr.set_level(false);
        tr.removed = false;
        pipe.mark_joined = true;
    }

    /// `TileEntityResourceTransmitter.onUpdateServer`: `pullFromAcceptors` e depois `TileEntityTransmitter.onUpdateServer`.
    fn block_entity_tick(
        &mut self,
        level: &LevelView<TransmitterId>,
        pos: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let id = *entity;
        self.pull(level, pos, id);
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        if ctx.pipe(id).mark_joined {
            ctx.world_join(&mut self.registry, id, false);
            ctx.pipe_mut(id).mark_joined = false;
        }
        ctx.with_tr(&mut self.registry, id, |t, w, out| {
            t.on_update_server(w, out)
        });
    }

    /// `TileEntityTransmitter.onChunkUnloaded`, `:156`: `validateAndTakeShare`.
    fn block_entity_chunk_unloaded(
        &mut self,
        _: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        let network = self.state.pipes[entity.0 as usize].net();
        self.state
            .family
            .validate_and_take_share(&self.registry, *entity, network, None);
    }

    /// `TileEntityTransmitter.setRemoved`, `:165`.
    fn block_entity_removed(
        &mut self,
        level: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
    ) {
        self.state.pipes[entity.0 as usize]
            .tr
            .as_mut()
            .unwrap()
            .removed = true;
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        ctx.world_separate(&mut self.registry, *entity, false);
    }

    /// `Level.setBlock` confirmou um estado novo: o `BlockCapabilityCache` de cada cano vizinho, no lado que olha `pos`, se invalida.
    fn block_changed(
        &mut self,
        level: &LevelView<TransmitterId>,
        pos: BlockPos,
        _old: Option<&str>,
        _new: Option<&str>,
    ) {
        for side in Face::ALL {
            let Some(&id) = level.block_entity(pos.relative(side)) else {
                continue;
            };
            let mut out = Vec::new();
            {
                let tr = self.state.pipes[id.0 as usize].tr();
                if let Some(dirty) = tr
                    .acceptor_cache
                    .refresh_listener_run(side.opposite(), tr.is_valid())
                {
                    tr.mark_dirty_acceptor(dirty, &mut out);
                }
            }
            let mut ctx = Ctx {
                st: &mut self.state,
                level,
            };
            ctx.apply(&mut self.registry, id, out);
        }
    }

    /// `BlockTransmitter.neighborChanged`: `onNeighborBlockChange(null)`.
    fn neighbor_changed(
        &mut self,
        level: &LevelView<TransmitterId>,
        _: BlockPos,
        entity: &mut TransmitterId,
        _: bool,
    ) {
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        ctx.with_tr(&mut self.registry, *entity, |t, w, out| {
            t.on_neighbor_block_change(None, w, out);
        });
    }

    /// `TransmitterNetworkRegistry.onTick`.
    fn server_tick_post(&mut self, level: &LevelView<TransmitterId>) {
        let runs_normally = self.state.runs_normally;
        let mut ctx = Ctx {
            st: &mut self.state,
            level,
        };
        self.registry.on_tick(&mut ctx, runs_normally);
    }
}

impl<K: Kind> Hooks<K> {
    /// `BufferedResourceTransmitter.pullFromAcceptors(level)`.
    fn pull(&mut self, level: &LevelView<TransmitterId>, pos: BlockPos, id: TransmitterId) {
        let pipe = &self.state.pipes[id.0 as usize];
        let (net, has_pull) = (pipe.net(), pipe.tr().has_pull_side());
        // A família sai do estado enquanto lê o mundo (os aceitadores), que a contém.
        let mut family = std::mem::take(&mut self.state.family);
        {
            let ctx = Ctx {
                st: &mut self.state,
                level,
            };
            let tr = ctx.pipe(id).tr();
            family.pull_from_acceptors(
                id,
                net,
                has_pull,
                &|side| tr.is_connection_type(side, ConnectionType::Pull),
                &|side| {
                    tr.acceptor_cache.get_connected_acceptor(side, &|s| {
                        TransmitterWorld::acceptor(&ctx, tpos(pos.relative(s)), s.opposite())
                    })
                },
            );
        }
        self.state.family = family;
    }
}

/// O mundo de um cenário: o nível, e a origem da estrutura (a posição do cano 0).
pub struct World<K: Kind> {
    pub level: SimLevel<Hooks<K>>,
    pub origin: BlockPos,
    /// A largura da estrutura: o cano de índice `i` está em `x = i % width`, `z = i / width`.
    pub width: i32,
}

impl<K: Kind> World<K> {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `hash_for` o `identityHashCode` de cada cano novo.
    pub fn new(
        origin: BlockPos,
        width: i32,
        uuid_hashes: Vec<i32>,
        hash_for: impl FnMut(BlockPos) -> i32 + 'static,
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
        let hooks = Hooks {
            registry,
            state: State {
                pipes: Vec::new(),
                hash_for: Box::new(hash_for),
                events: Vec::new(),
                runs_normally: true,
                family: K::family(),
                by_pos: BTreeMap::new(),
                tanks: BTreeMap::new(),
                pending_uuid: None,
                share_tx: None,
                manual_merge: false,
            },
        };
        Self {
            level: SimLevel::new(hooks),
            origin,
            width,
        }
    }

    /// A posição do cano de índice `index` da estrutura.
    pub fn pipe_pos(&self, index: i32) -> BlockPos {
        BlockPos::new(
            self.origin.x + index % self.width,
            self.origin.y,
            self.origin.z + index / self.width,
        )
    }

    /// O índice da posição na estrutura (`z * width + x`), ou `None` fora dela.
    pub fn index_of(&self, pos: BlockPos, depth: i32) -> Option<i32> {
        let (x, z) = (pos.x - self.origin.x, pos.z - self.origin.z);
        ((0..self.width).contains(&x) && (0..depth).contains(&z) && pos.y == self.origin.y)
            .then_some(z * self.width + x)
    }

    pub fn registry(&self) -> &Registry<K> {
        &self.level.hooks.registry
    }

    pub fn family(&self) -> &Family<K> {
        &self.level.hooks.state.family
    }

    pub fn place_pipe(&mut self, x: i32, tier: &str) {
        let pos = self.pipe_pos(x);
        let block = format!("{PIPE_PREFIX}{tier}");
        assert!(self.level.set_block(pos, Some(&block)), "cano {x}");
    }

    /// Um Fluid Tank ou Chemical Tank do tier (`basic`, `advanced`, `elite`, `ultimate` ou `creative`) em `pos`, com o fluido (`None` é vazio) e a
    /// quantidade (`None` é a capacidade): expõe a capability nos seis lados.
    pub fn place_tank(
        &mut self,
        pos: BlockPos,
        tier: &str,
        content: Option<(&str, Option<u64>)>,
    ) -> Rc<Tank<K>> {
        let capacity = K::tank_capacity(tier);
        let tank = ResourceTank::open(capacity, None);
        if let Some((id, amount)) = content {
            tank.set_contents(Some(K::resource(id)), amount.unwrap_or(capacity), None);
        }
        self.put_tank(
            pos,
            Tank {
                tank,
                sides: [true; 6],
                creative: tier == "creative",
                burn: 0,
                active: Default::default(),
                client_active: Default::default(),
                update_delay: Default::default(),
                energy_full: Default::default(),
            },
        )
    }

    fn put_tank(&mut self, pos: BlockPos, tank: Tank<K>) -> Rc<Tank<K>> {
        let tank = Rc::new(tank);
        self.level.hooks.state.tanks.insert(pos, tank.clone());
        assert!(self.level.set_block(pos, Some(TANK)), "tanque {pos:?}");
        tank
    }

    /// O que a chave inglesa faz num lado: `setConnectionTypeRaw`, `onModeChange`, `refreshConnections` e `notifyTileChange`.
    pub fn set_mode(&mut self, index: i32, side: Face, kind: ConnectionType) {
        let id = self.pipe_at(self.pipe_pos(index)).expect("cano").0;
        self.level.with_hooks(|hooks, level| {
            let mut ctx = Ctx {
                st: &mut hooks.state,
                level,
            };
            ctx.with_tr(&mut hooks.registry, id, |t, _, out| {
                t.set_connection_type_raw(side, kind, out);
            });
            ctx.with_tr(&mut hooks.registry, id, |t, w, out| {
                t.on_mode_change(side, w, out);
            });
            ctx.with_tr(&mut hooks.registry, id, |t, w, out| {
                t.refresh_connections(w, out);
            });
            ctx.with_tr(&mut hooks.registry, id, |t, _, out| {
                t.notify_tile_change(out);
            });
        });
    }

    /// `MechanicalPipe.write`, a parte de fluido: valida as shares e devolve o `saveShare`.
    pub fn write_stored(&mut self, index: i32) -> StackContents<K::R> {
        let (id, pipe) = self.pipe_at(self.pipe_pos(index)).expect("cano");
        let network = pipe.net();
        let hooks = &mut self.level.hooks;
        hooks
            .state
            .family
            .write_stored(&hooks.registry, id, network)
    }

    /// Um tick do nível: o relógio do jogo (`getGameTime`) anda antes de tudo o que roda nele.
    pub fn tick_level(&mut self) {
        self.level.hooks.state.family.game_time += 1;
        // Os tickers dos tanques (o Heat Generator queima) rodam com os block entities.
        let changed: Vec<BlockPos> = self
            .level
            .hooks
            .state
            .tanks
            .iter()
            .filter(|(_, tank)| tank.burn_tick())
            .map(|(pos, _)| *pos)
            .collect();
        for pos in changed {
            // `level.setBlockAndUpdate(pos, state com `active`)`: os vizinhos recebem `neighborChanged`.
            self.level.update_neighbors_at(pos);
        }
        self.level.tick_level();
    }

    pub fn break_block(&mut self, pos: BlockPos) {
        self.level.set_block(pos, None);
    }

    /// O cano do bloco em `pos`, se o chunk está carregado e há um cano.
    pub fn pipe_at(&self, pos: BlockPos) -> Option<(TransmitterId, &Pipe<K>)> {
        let id = *self.level.view().block_entity(pos)?;
        Some((id, &self.level.hooks.state.pipes[id.0 as usize]))
    }

    /// Os eventos da rede desde a última chamada.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.level.hooks.state.events)
    }
}

impl World<FluidKind> {
    /// O Heat Generator em `pos`: o `lavaTank` (capacidade 1.000, só lava, `input`: ninguém de fora tira dele) em todo lado menos a frente
    /// (norte), que queima 100 por tick.
    pub fn place_generator(&mut self, pos: BlockPos) -> Rc<Tank<FluidKind>> {
        let lava = Rc::new(|resource: &FluidResource| resource.id.as_ref() == "minecraft:lava");
        let tank = ResourceTank::create(
            1_000,
            Rc::new(|_, automation| automation != AutomationType::External),
            Rc::new(|_, _| true),
            lava,
            None,
        );
        self.put_tank(
            pos,
            Tank {
                tank,
                sides: [true, true, false, true, true, true],
                creative: false,
                burn: 100,
                active: Default::default(),
                client_active: Default::default(),
                update_delay: Default::default(),
                energy_full: Default::default(),
            },
        )
    }
}
