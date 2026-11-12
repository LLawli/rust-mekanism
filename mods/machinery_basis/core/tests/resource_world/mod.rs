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
use machinery_basis_core::transmission::chemical_network::ChemicalFamily;
use machinery_basis_core::transmission::connection::{ConnectionType, TransmissionType};
use machinery_basis_core::transmission::fluid_network::FluidFamily;
use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::pressurized_tube::TubeTier;
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::resource_owner::TraceEvent;
use machinery_basis_core::transmission::resource_owner::{
    PipeInfo, ResourceOwnerWorld, ResourceTransmitters,
};
use machinery_basis_core::transmission::transmitter::{Compound, Transmitter};
use machinery_basis_core::transmission::{DimensionId, NetworkId, TransmitterId, Uuid};
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

/// O cano de um bloco, como o dono o guarda e o rastro o lê (veja [`World::pipe_at`]).
pub struct Pipe<'a, K: Kind> {
    pub pos: BlockPos,
    tr: &'a Transmitter<Acceptor<K>>,
    /// A rede que o cano guarda, órfão ou não.
    pub network: Option<NetworkId>,
    pub orphan: bool,
}

impl<K: Kind> Pipe<'_, K> {
    pub fn tr(&self) -> &Transmitter<Acceptor<K>> {
        self.tr
    }

    /// `hasTransmitterNetwork()`.
    pub fn net(&self) -> Option<NetworkId> {
        self.network.filter(|_| !self.orphan)
    }
}

/// O que um cano novo leva do bloco: a capacidade, a vazão e o que o chunk guardou.
type Pending<K> = (i64, i32, Option<Saved<K>>);

/// O que o cano grava no chunk: o NBT do `Transmitter` e o `stored`.
pub type Saved<K: Kind> = (Compound, StackContents<K::R>);

/// O jogo, como o dono dos transmissores o vê: o nível, os tanques vizinhos e o que o chunk guardou dos canos novos.
pub struct Game<'a, K: Kind> {
    level: &'a LevelView<()>,
    tanks: &'a BTreeMap<BlockPos, Rc<Tank<K>>>,
    pending: &'a mut BTreeMap<BlockPos, Pending<K>>,
}

impl<K: Kind> ResourceOwnerWorld<K::R> for Game<'_, K> {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        self.level.is_block_loaded(pos)
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    /// A capability de recurso do vizinho: o tanque, nos lados em que ele a expõe.
    fn handler(&self, pos: BlockPos, side: Face) -> Option<Acceptor<K>> {
        if self.level.block_if_loaded(pos) != Some(TANK) {
            return None;
        }
        let tank = self.tanks.get(&pos)?;
        tank.exposes(side).then(|| tank.clone() as Acceptor<K>)
    }

    fn pipe_info(&mut self, pos: BlockPos) -> Option<PipeInfo<K::R>> {
        let (capacity, transfer_rate, saved) = self.pending.remove(&pos)?;
        Some(PipeInfo {
            capacity,
            transfer_rate,
            saved,
        })
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}
}

pub struct Hooks<K: Kind> {
    pub owner: ResourceTransmitters<K::R>,
    /// Os tanques postos, por posição.
    pub tanks: BTreeMap<BlockPos, Rc<Tank<K>>>,
    pending: BTreeMap<BlockPos, Pending<K>>,
    pub runs_normally: bool,
}

impl<K: Kind> Hooks<K> {
    fn event(&mut self, level: &LevelView<()>, event: Event) {
        let mut game = Game {
            level,
            tanks: &self.tanks,
            pending: &mut self.pending,
        };
        self.owner.handle(&mut game, event);
    }
}

impl<K: Kind> ChunkHooks for Hooks<K> {
    type BlockEntity = ();
    type Saved = Saved<K>;

    fn new_block_entity(
        &mut self,
        pos: BlockPos,
        block: &str,
        saved: Option<&Saved<K>>,
    ) -> Option<()> {
        let (capacity, rate) = K::pipe(block.strip_prefix(PIPE_PREFIX)?);
        self.pending.insert(pos, (capacity, rate, saved.cloned()));
        Some(())
    }

    /// `MechanicalPipe.write`: valida as shares e grava o `saveShare`, e o NBT do `Transmitter`.
    fn save_block_entity(&mut self, pos: BlockPos, _: &mut ()) -> Saved<K> {
        self.owner.write(pos).expect("o cano está no dono")
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block.starts_with(PIPE_PREFIX)
    }

    fn ticket_level_updated(&mut self, level: &LevelView<()>, chunk: ChunkPos, old: i32, new: i32) {
        use machinery_basis_core::sim::chunks::MAX_VIEW_DISTANCE;
        let loaded = if old > MAX_VIEW_DISTANCE && new <= MAX_VIEW_DISTANCE {
            true
        } else if new > MAX_VIEW_DISTANCE && old <= MAX_VIEW_DISTANCE {
            false
        } else {
            return;
        };
        self.event(level, Event::Ticket { chunk, loaded });
    }

    /// `TileEntityTransmitter.clearRemoved`, `:146`, no servidor.
    fn block_entity_added(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::ClearRemoved(pos));
    }

    /// `TileEntityResourceTransmitter.onUpdateServer`.
    fn block_entity_tick(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::Update(pos));
    }

    /// `TileEntityTransmitter.onChunkUnloaded`, `:156`.
    fn block_entity_chunk_unloaded(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::ChunkUnloaded(pos));
    }

    /// `TileEntityTransmitter.setRemoved`, `:165`.
    fn block_entity_removed(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::Removed(pos));
    }

    /// `Level.setBlock` confirmou um estado novo: o `BlockCapabilityCache` de cada cano vizinho, no lado que olha `pos`, se invalida.
    fn block_changed(
        &mut self,
        level: &LevelView<()>,
        pos: BlockPos,
        _old: Option<&str>,
        _new: Option<&str>,
    ) {
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if level.block_entity(neighbor).is_some() {
                self.event(level, Event::Capability(neighbor, side.opposite()));
            }
        }
    }

    /// `BlockTransmitter.neighborChanged`: `onNeighborBlockChange(null)`.
    fn neighbor_changed(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut (), _: bool) {
        self.event(level, Event::Neighbor(pos));
    }

    /// `TransmitterNetworkRegistry.onTick`.
    fn server_tick_post(&mut self, level: &LevelView<()>) {
        let runs_normally = self.runs_normally;
        self.event(level, Event::ServerTickPost { runs_normally });
    }
}

/// O mundo de um cenário: o nível, e a origem da estrutura (a posição do cano 0).
pub struct World<K: Kind> {
    pub level: SimLevel<Hooks<K>>,
    pub origin: BlockPos,
    /// A largura da estrutura: o cano de índice `i` está em `x = i % width`, `z = i / width`.
    pub width: i32,
}

/// O que a rede fez, na ordem, para o teste comparar com o rastro.
pub type NetEvent = TraceEvent;

impl<K: Kind> World<K> {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `hash_for` o `identityHashCode` de cada cano novo.
    pub fn new(
        origin: BlockPos,
        width: i32,
        uuid_hashes: Vec<i32>,
        hash_for: impl FnMut(BlockPos) -> i32 + 'static,
    ) -> Self {
        let queue = Rc::new(RefCell::new(VecDeque::from(uuid_hashes)));
        let mut owner = ResourceTransmitters::new(
            DIMENSION,
            K::TRANSMISSION,
            K::family,
            move || {
                Uuid::with_hash_code(
                    queue
                        .borrow_mut()
                        .pop_front()
                        .expect("mais redes do que o rastro tem"),
                )
            },
            hash_for,
        );
        owner.record_events(true);
        let hooks = Hooks {
            owner,
            tanks: BTreeMap::new(),
            pending: BTreeMap::new(),
            runs_normally: true,
        };
        Self {
            level: SimLevel::new(hooks),
            origin,
            width,
        }
    }

    pub fn owner(&self) -> &ResourceTransmitters<K::R> {
        &self.level.hooks.owner
    }

    pub fn family(&self) -> &Family<K> {
        self.level.hooks.owner.family()
    }

    pub fn family_mut(&mut self) -> &mut Family<K> {
        self.level.hooks.owner.family_mut()
    }

    pub fn registry(&self) -> &Registry<K> {
        self.level.hooks.owner.registry()
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
        self.level.hooks.tanks.insert(pos, tank.clone());
        assert!(self.level.set_block(pos, Some(TANK)), "tanque {pos:?}");
        tank
    }

    /// O que a chave inglesa faz num lado: `setConnectionTypeRaw`, `onModeChange`, `refreshConnections` e `notifyTileChange`.
    pub fn set_mode(&mut self, index: i32, side: Face, kind: ConnectionType) {
        let pos = self.pipe_pos(index);
        self.level.with_hooks(|hooks, level| {
            let mut game = Game {
                level,
                tanks: &hooks.tanks,
                pending: &mut hooks.pending,
            };
            hooks.owner.set_mode(&mut game, pos, side, kind);
        });
    }

    /// `MechanicalPipe.write`, a parte de fluido: valida as shares e devolve o `saveShare`.
    pub fn write_stored(&mut self, index: i32) -> StackContents<K::R> {
        let pos = self.pipe_pos(index);
        self.level.hooks.owner.write(pos).expect("cano").1
    }

    /// Um tick do nível: o relógio do jogo (`getGameTime`) anda antes de tudo o que roda nele.
    pub fn tick_level(&mut self) {
        self.family_mut().game_time += 1;
        // Os tickers dos tanques (o Heat Generator queima) rodam com os block entities.
        let changed: Vec<BlockPos> = self
            .level
            .hooks
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
    pub fn pipe_at(&self, pos: BlockPos) -> Option<(TransmitterId, Pipe<'_, K>)> {
        self.level.view().block_entity(pos)?;
        let owner = &self.level.hooks.owner;
        let id = owner.pipe_at(pos)?;
        Some((
            id,
            Pipe {
                pos,
                tr: owner.transmitter(pos)?,
                network: owner.raw_network_of(pos),
                orphan: owner.is_orphan(pos)?,
            },
        ))
    }

    /// Os eventos da rede desde a última chamada.
    pub fn take_events(&mut self) -> Vec<NetEvent> {
        self.level.hooks.owner.take_events()
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
