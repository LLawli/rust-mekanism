//! O mundo simulado dos testes do dono dos condutores termodinâmicos: os chunks falsos de `sim::chunks` entregam ao
//! [`HeatTransmitters`] os eventos do jogo na ordem do original (`clearRemoved`, o ticker, `neighborChanged`, a capability invalidada, o
//! `ServerTickEvent.Post`), e um Heat Generator mínimo (o capacitor, o `simulate` por baixo e o ar) é o vizinho com a capability `HEAT`.
//! Quem chama o registro é o dono; o condutor, a rede de calor e a troca de calor são os da lib.
#![allow(dead_code)]
use machinery_basis_core::heat::{AMBIENT_TEMP, Capacitor};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, ChunkPos, LevelView, SimLevel};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::heat_owner::{
    ConductorInfo, HeatOwnerWorld, HeatTransmitters, HostCapacitor, TraceEvent,
};
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::thermodynamic_conductor::{CapacitorState, ConductorTier};
use machinery_basis_core::transmission::transmitter::Compound;
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

pub const DIMENSION: DimensionId = DimensionId(0);
/// O prefixo do bloco do condutor: `test:conductor_<tier>`.
pub const CONDUCTOR_PREFIX: &str = "test:conductor_";
pub const GENERATOR: &str = "test:generator";

const GEN_CAPACITY: f64 = 10.0;
const GEN_INVERSE_CONDUCTION: f64 = 5.0;
const GEN_INSULATION: f64 = 100.0;
const THERMAL_EFFICIENCY: f64 = 0.5;
/// `INSULATION_BY_FACE` do Heat Generator: a de baixo é a padrão.
const GEN_INSULATION_BY_FACE: [f64; 6] = [0.0, 100.0, 100.0, 100.0, 100.0, 100.0];

pub fn tier_of(name: &str) -> ConductorTier {
    match name {
        "basic" => ConductorTier::Basic,
        "advanced" => ConductorTier::Advanced,
        "elite" => ConductorTier::Elite,
        "ultimate" => ConductorTier::Ultimate,
        other => panic!("nível {other}"),
    }
}

/// O ar de cada célula da estrutura: `[y][x]` com os seis lados e o próprio bloco.
#[derive(Clone)]
pub struct Ambient {
    origin: BlockPos,
    cells: Vec<Vec<[f64; 7]>>,
}

impl Ambient {
    pub fn new(origin: BlockPos, cells: Vec<Vec<[f64; 7]>>) -> Self {
        Self { origin, cells }
    }

    fn read(&self, pos: BlockPos, side: Option<Face>) -> f64 {
        let x = (pos.x - self.origin.x) as usize;
        let y = (pos.y - self.origin.y) as usize;
        self.cells[y][x][side.map_or(6, |side| side as usize)]
    }
}

/// O Heat Generator no que a rede lê dele: o capacitor (capacidade 10, condução 5, isolação 100, a de baixo padrão) e o `simulate`.
pub struct Gen {
    pub pos: BlockPos,
    /// `None` enquanto o calor vale -1 (a primeira leitura o inicializa no ar do bloco).
    heat: RefCell<Option<Capacitor>>,
    ambient: Ambient,
    pub last_transfer: RefCell<f64>,
    pub last_env: RefCell<f64>,
}

impl Gen {
    fn capacitor(&self) -> Capacitor {
        let mut heat = self.heat.borrow_mut();
        *heat.get_or_insert_with(|| {
            Capacitor::new(
                GEN_CAPACITY,
                GEN_INVERSE_CONDUCTION,
                GEN_INSULATION,
                self.ambient.read(self.pos, None),
            )
        })
    }

    pub fn stored_heat(&self) -> Option<f64> {
        self.heat.borrow().map(|capacitor| capacitor.heat)
    }

    pub fn handle_heat(&self, transfer: f64) {
        let mut capacitor = self.capacitor();
        capacitor.handle(transfer);
        *self.heat.borrow_mut() = Some(capacitor);
    }

    /// `TileEntityHeatGenerator.onUpdateServer`, a parte do calor (sem lava, sem boost): `simulate` numa transação, que confirma no fim.
    fn tick<W: HeatOwnerWorld>(&self, owner: &mut HeatTransmitters, world: &mut W) {
        let mut capacitor = self.capacitor();
        let ambient = self.ambient.read(self.pos, None);
        let heat_lost = THERMAL_EFFICIENCY * (capacitor.temperature() - ambient);
        capacitor.handle(-heat_lost);
        // `getAdjacent(DOWN)`: o único lado por onde o gerador entrega calor.
        let below = self.pos.relative(Face::Down);
        let transfer = match owner.sink_at(&*world, below, Face::Up) {
            Some(mut sink) => capacitor.simulate_adjacent(&mut sink),
            None => 0.0,
        };
        let face_ambient = Face::ALL.map(|side| self.ambient.read(self.pos, Some(side)));
        let environment = capacitor.lose_to_faces(face_ambient, GEN_INSULATION_BY_FACE);
        *self.heat.borrow_mut() = Some(capacitor);
        *self.last_transfer.borrow_mut() = transfer;
        *self.last_env.borrow_mut() = environment;
        // O `transaction.commit()`: o condutor de baixo, que recebeu calor nela, confirma o capacitor dele.
        owner.commit_pending(world);
    }
}

/// O que um condutor novo leva do bloco: o nível e o que o chunk guardou dele.
pub type Pending = (ConductorTier, Option<(Compound, Option<CapacitorState>)>);

/// O jogo, como o dono o vê.
pub struct Game<'a> {
    level: &'a LevelView<()>,
    heater: &'a Option<Gen>,
    tiers: &'a mut BTreeMap<BlockPos, Pending>,
    ambient: &'a Ambient,
    powered: bool,
}

impl HeatOwnerWorld for Game<'_> {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        self.level.is_block_loaded(pos)
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        self.powered
    }

    fn has_host(&self, pos: BlockPos, _: Face) -> bool {
        self.level.block_if_loaded(pos) == Some(GENERATOR)
    }

    /// O capacitor do gerador, visto de qualquer lado.
    fn host(&self, pos: BlockPos, side: Face) -> HostCapacitor {
        let heater = self
            .heater
            .as_ref()
            .filter(|heater| heater.pos == pos)
            .expect("o gerador");
        let capacitor = heater.capacitor();
        HostCapacitor {
            temperature: capacitor.temperature(),
            heat_capacity: GEN_CAPACITY,
            inverse_conduction: GEN_INVERSE_CONDUCTION,
            inverse_insulation: if side == Face::Down {
                0.0
            } else {
                GEN_INSULATION
            },
        }
    }

    fn host_handle_heat(&self, pos: BlockPos, _: Face, transfer: f64) {
        if let Some(heater) = self.heater.as_ref().filter(|heater| heater.pos == pos) {
            heater.handle_heat(transfer);
        }
    }

    fn ambient(&self, pos: BlockPos, side: Option<Face>) -> Option<f64> {
        Some(self.ambient.read(pos, side))
    }

    fn conductor_info(&mut self, pos: BlockPos) -> Option<ConductorInfo> {
        let (tier, saved) = self.tiers.remove(&pos)?;
        Some(ConductorInfo {
            tier,
            values: tier.base_values(),
            saved,
        })
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}
}

pub struct Hooks {
    pub owner: HeatTransmitters,
    pub heater: Option<Gen>,
    tiers: BTreeMap<BlockPos, Pending>,
    saved: BTreeMap<BlockPos, (Compound, Option<CapacitorState>)>,
    ambient: Ambient,
    pub runs_normally: bool,
    /// `WorldUtils.isGettingPowered` de todo bloco.
    pub powered: bool,
}

impl Hooks {
    fn event(&mut self, level: &LevelView<()>, event: Event) {
        let mut game = Game {
            level,
            heater: &self.heater,
            tiers: &mut self.tiers,
            ambient: &self.ambient,
            powered: self.powered,
        };
        self.owner.handle(&mut game, event);
    }
}

impl ChunkHooks for Hooks {
    type BlockEntity = ();
    type Saved = Compound;

    fn new_block_entity(&mut self, pos: BlockPos, block: &str, _: Option<&Compound>) -> Option<()> {
        if block == GENERATOR {
            return Some(());
        }
        let tier = tier_of(block.strip_prefix(CONDUCTOR_PREFIX)?);
        let saved = self.saved.remove(&pos);
        self.tiers.insert(pos, (tier, saved));
        Some(())
    }

    fn save_block_entity(&mut self, _: BlockPos, _: &mut ()) -> Compound {
        Compound::default()
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block == GENERATOR || block.starts_with(CONDUCTOR_PREFIX)
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

    /// `TileEntityTransmitter.clearRemoved`, no servidor.
    fn block_entity_added(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::ClearRemoved(pos));
    }

    /// `TileEntityTransmitter.onUpdateServer`, ou o `onUpdateServer` do gerador.
    fn block_entity_tick(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        if level.block_if_loaded(pos) == Some(GENERATOR) {
            let mut game = Game {
                level,
                heater: &self.heater,
                tiers: &mut self.tiers,
                ambient: &self.ambient,
                powered: self.powered,
            };
            if let Some(heater) = self.heater.as_ref().filter(|heater| heater.pos == pos) {
                heater.tick(&mut self.owner, &mut game);
            }
            return;
        }
        self.event(level, Event::Update(pos));
    }

    fn block_entity_chunk_unloaded(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::ChunkUnloaded(pos));
    }

    fn block_entity_removed(&mut self, level: &LevelView<()>, pos: BlockPos, _: &mut ()) {
        self.event(level, Event::Removed(pos));
    }

    /// `Level.setBlock` confirmou um estado novo: o `BlockCapabilityCache` de cada condutor vizinho, no lado que olha `pos`, se invalida.
    fn block_changed(
        &mut self,
        level: &LevelView<()>,
        pos: BlockPos,
        _: Option<&str>,
        _: Option<&str>,
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
        if level
            .block_if_loaded(pos)
            .is_some_and(|block| block.starts_with(CONDUCTOR_PREFIX))
        {
            self.event(level, Event::Neighbor(pos));
        }
    }

    fn server_tick_post(&mut self, level: &LevelView<()>) {
        let runs_normally = self.runs_normally;
        self.event(level, Event::ServerTickPost { runs_normally });
    }
}

/// O mundo de um cenário: o nível, e a origem da estrutura (a posição do condutor 0).
pub struct World {
    pub level: SimLevel<Hooks>,
    pub origin: BlockPos,
    ambient: Ambient,
}

impl World {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem; `tile_hashes` o `identityHashCode` de cada condutor novo.
    pub fn new(
        origin: BlockPos,
        ambient: Vec<Vec<[f64; 7]>>,
        uuid_hashes: Vec<i32>,
        tile_hashes: Vec<i32>,
    ) -> Self {
        let queue = Rc::new(RefCell::new(VecDeque::from(uuid_hashes)));
        let tiles = Rc::new(RefCell::new(VecDeque::from(tile_hashes)));
        let mut owner = HeatTransmitters::new(
            DIMENSION,
            move || {
                Uuid::with_hash_code(
                    queue
                        .borrow_mut()
                        .pop_front()
                        .expect("mais redes do que o rastro tem"),
                )
            },
            move |_| {
                tiles
                    .borrow_mut()
                    .pop_front()
                    .expect("mais condutores que o rastro")
            },
        );
        owner.record_events(true);
        let ambient = Ambient::new(origin, ambient);
        let hooks = Hooks {
            owner,
            heater: None,
            tiers: BTreeMap::new(),
            saved: BTreeMap::new(),
            ambient: ambient.clone(),
            runs_normally: true,
            powered: false,
        };
        let mut level = SimLevel::new(hooks);
        // Os chunks da estrutura e a margem de um em volta ficam com o ticket do `gameTest`.
        for cx in origin.chunk().x - 1..=origin.chunk().x + 1 {
            for cz in origin.chunk().z - 1..=origin.chunk().z + 1 {
                level.set_ticket_level(
                    ChunkPos::new(cx, cz),
                    machinery_basis_core::sim::chunks::ENTITY_TICKING_LEVEL,
                );
            }
        }
        level.tick();
        level.take_events();
        level.hooks.owner.take_events();
        Self {
            level,
            origin,
            ambient,
        }
    }

    pub fn owner(&self) -> &HeatTransmitters {
        &self.level.hooks.owner
    }

    pub fn pos(&self, x: i32, y: i32) -> BlockPos {
        BlockPos::new(self.origin.x + x, self.origin.y + y, self.origin.z)
    }

    pub fn x_of(&self, pos: BlockPos) -> i32 {
        pos.x - self.origin.x
    }

    pub fn place_conductor(&mut self, x: i32, tier: ConductorTier) {
        let name = match tier {
            ConductorTier::Basic => "basic",
            ConductorTier::Advanced => "advanced",
            ConductorTier::Elite => "elite",
            ConductorTier::Ultimate => "ultimate",
        };
        let pos = self.pos(x, 0);
        assert!(
            self.level
                .set_block(pos, Some(&format!("{CONDUCTOR_PREFIX}{name}"))),
            "condutor {x}"
        );
    }

    /// Um condutor que vem de um chunk salvo: `loadAdditional` lê o NBT do transmissor e o estado do capacitor antes de ele entrar no mundo.
    pub fn place_saved_conductor(
        &mut self,
        x: i32,
        tier: ConductorTier,
        saved: (Compound, Option<CapacitorState>),
    ) {
        let pos = self.pos(x, 0);
        self.level.hooks.saved.insert(pos, saved);
        self.place_conductor(x, tier);
    }

    /// `ItemConfigurator.useOn` sem agachar: o redstone reativo liga e desliga.
    pub fn right_click(&mut self, x: i32) -> bool {
        let pos = self.pos(x, 0);
        self.level.with_hooks(|hooks, level| {
            let mut game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.right_click(&mut game, pos)
        })
    }

    /// Os vizinhos de um bloco recebem `neighborChanged` (uma alavanca que muda).
    pub fn update_neighbors(&mut self, x: i32) {
        let pos = self.pos(x, 0);
        self.level.update_neighbors_at(pos);
    }

    /// `ThermodynamicConductor.write` do condutor em `x`.
    pub fn write(
        &mut self,
        pos: BlockPos,
    ) -> machinery_basis_core::transmission::thermodynamic_conductor::ConductorTag {
        self.level.with_hooks(|hooks, level| {
            let game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.write(&game, pos).expect("condutor")
        })
    }

    /// A capability `HEAT` do condutor em `pos` existe pelo lado `side` dele (o que o Heat Generator acha embaixo dele).
    pub fn sink_exists(&mut self, pos: BlockPos, side: Face) -> bool {
        self.level.with_hooks(|hooks, level| {
            let game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.sink_at(&game, pos, side).is_some()
        })
    }

    /// A vista do condutor em `x` (`getConnectionType` e `getAllCurrentConnections` de cada lado).
    pub fn view(&mut self, x: i32) -> CableView {
        let pos = self.pos(x, 0);
        self.level.with_hooks(|hooks, level| {
            let game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.view_at(&game, pos).expect("condutor")
        })
    }

    pub fn place_generator(&mut self, x: i32) {
        let pos = self.pos(x, 1);
        self.level.hooks.heater = Some(Gen {
            pos,
            heat: RefCell::new(None),
            ambient: self.ambient.clone(),
            last_transfer: RefCell::new(0.0),
            last_env: RefCell::new(0.0),
        });
        assert!(self.level.set_block(pos, Some(GENERATOR)), "gerador");
    }

    pub fn break_conductor(&mut self, x: i32) {
        let pos = self.pos(x, 0);
        self.level.set_block(pos, None);
    }

    /// Calor posto num condutor como um aquecedor faz: `handleHeat` numa transação raiz.
    pub fn heat_conductor(&mut self, x: i32, amount: f64) {
        let pos = self.pos(x, 0);
        self.level.with_hooks(|hooks, level| {
            let mut game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.handle_heat_at(&mut game, pos, amount);
        });
    }

    /// O calor do gerador posto de uma vez (o `data merge` do teste em jogo).
    pub fn set_generator_heat(&mut self, heat: f64) {
        let heater = self.level.hooks.heater.as_ref().expect("há gerador");
        let mut capacitor = heater.capacitor();
        capacitor.heat = heat;
        *heater.heat.borrow_mut() = Some(capacitor);
    }

    pub fn heat_generator(&mut self, amount: f64) {
        self.level
            .hooks
            .heater
            .as_ref()
            .expect("há gerador")
            .handle_heat(amount);
    }

    pub fn set_mode(&mut self, x: i32, side: Face, kind: ConnectionType) {
        let pos = self.pos(x, 0);
        self.level.with_hooks(|hooks, level| {
            let mut game = Game {
                level,
                heater: &hooks.heater,
                tiers: &mut hooks.tiers,
                ambient: &hooks.ambient,
                powered: hooks.powered,
            };
            hooks.owner.set_mode(&mut game, pos, side, kind);
        });
    }

    /// Um tick do servidor: os block entities e o `ServerTickEvent.Post` do registro.
    pub fn tick(&mut self) {
        self.level.tick();
    }

    pub fn take_events(&mut self) -> Vec<TraceEvent> {
        self.level.hooks.owner.take_events()
    }
}

pub fn ambient_default() -> Vec<Vec<[f64; 7]>> {
    vec![vec![[AMBIENT_TEMP; 7]; 8]; 2]
}
