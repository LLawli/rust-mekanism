//! O dono dos transmissores de energia (`transmission::owner`) contra o rastro do original, movido pelos eventos do jogo (`ClearRemoved`,
//! `Update`, `Neighbor`, `Capability`, `ServerTickPost`, `Removed`) e não por chamadas ao registro: é o que o jogo faz. Os cenários são os
//! de `dev/referencia/rastros/energy_*.jsonl` (`EnergyNetworkTraceTest`); a cada tick o teste compara a energia de cada rede, a que ela
//! mandou (`prev`) e a de cada máquina com a linha `estate` do rastro.
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::sim::chunks::ChunkPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::owner::ENERGY_KEY;
use machinery_basis_core::transmission::owner::{
    Acceptor, CableBuffer, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::transmitter::Tag;
use machinery_basis_core::transmission::transmitter::{Compound, Overlay};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub const DIMENSION: DimensionId = DimensionId(0);

pub fn tier_capacity(tier: &str) -> i64 {
    match tier {
        "basic" => 8_000,
        "advanced" => 128_000,
        "elite" => 1_024_000,
        "ultimate" => 8_192_000,
        other => panic!("tier desconhecido: {other}"),
    }
}

/// Uma máquina com todos os lados em `INPUT`, ou um cubo que entrega pelo `extract` (o cabo em `PULL`).
pub struct Machine {
    pub container: BufferEnergyContainer,
    pub gives: bool,
    /// Um vizinho de simulação errada: `extract` devolve isto a mais do que o pedido, sem ter como entregar (o `EnergyHandler` que erra, de que `pullFromAcceptors` se defende).
    pub overdraw: i32,
    /// Os `extract` que o vizinho recebeu: o pedido e se era simulação.
    pub extracts: std::cell::RefCell<Vec<(i32, bool)>>,
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }

    fn extract(&self, amount: i32, simulate: bool) -> i32 {
        self.extracts.borrow_mut().push((amount, simulate));
        if !self.gives {
            return 0;
        }
        let transaction = Transaction::open_root();
        let got = self
            .container
            .extract(amount, &transaction, AutomationType::External);
        if !simulate {
            transaction.commit();
        }
        got + self.overdraw
    }
}

pub enum Block {
    Cable {
        capacity: i64,
        saved: Option<Compound>,
    },
    Machine(Rc<Machine>),
}

#[derive(Default)]
pub struct Level {
    pub blocks: HashMap<BlockPos, Block>,
    pub unloaded: Vec<BlockPos>,
    pub published: Vec<(BlockPos, CableView)>,
    pub overlays: Vec<Overlay>,
    pub powered: Vec<BlockPos>,
    /// `BlockEntity.setChanged`, `sideChanged` e `redstoneChanged`, na ordem em que o dono os chamou.
    pub changed: Vec<BlockPos>,
    pub side_changes: Vec<(BlockPos, Face, ConnectionType, ConnectionType)>,
    pub redstone_changes: Vec<(BlockPos, bool)>,
    /// O jogo dá capability de energia também ao bloco de um cabo (o dono nunca deve perguntar por ela).
    pub cables_have_capability: bool,
    /// Cada posição por que o dono perguntou `is_block_loaded`, na ordem.
    pub asked: std::cell::RefCell<Vec<BlockPos>>,
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        self.asked.borrow_mut().push(pos);
        !self.unloaded.contains(&pos)
    }

    fn is_getting_powered(&self, pos: BlockPos) -> bool {
        Face::ALL
            .iter()
            .any(|side| self.powered.contains(&pos.relative(*side)))
    }

    fn energy_handler(&self, pos: BlockPos, _side: Face) -> Option<Acceptor> {
        // `BlockCapabilityCache.getCapability` de um chunk sem carregar é nulo.
        if self.unloaded.contains(&pos) {
            return None;
        }
        match self.blocks.get(&pos)? {
            Block::Machine(machine) => {
                let machine: Acceptor = machine.clone();
                Some(machine)
            }
            Block::Cable { .. } if self.cables_have_capability => {
                let ghost: Acceptor = Rc::new(Machine {
                    container: BufferEnergyContainer::new(1_000, always, always, None),
                    gives: false,
                    overdraw: 0,
                    extracts: Default::default(),
                });
                Some(ghost)
            }
            Block::Cable { .. } => None,
        }
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        match self.blocks.get(&pos)? {
            Block::Cable { capacity, saved } => Some(CableInfo {
                capacity: *capacity,
                saved: saved.clone(),
            }),
            Block::Machine(_) => None,
        }
    }

    fn publish(&mut self, pos: BlockPos, view: &CableView) {
        self.published.push((pos, view.clone()));
    }

    fn overlay(&mut self, _pos: BlockPos, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    fn set_changed(&mut self, pos: BlockPos) {
        self.changed.push(pos);
    }

    fn side_changed(
        &mut self,
        pos: BlockPos,
        side: Face,
        old: ConnectionType,
        new: ConnectionType,
    ) {
        self.side_changes.push((pos, side, old, new));
    }

    fn redstone_changed(&mut self, pos: BlockPos, powered: bool) {
        self.redstone_changes.push((pos, powered));
    }
}

pub struct Sim {
    pub owner: EnergyTransmitters,
    pub level: Level,
    pub time: i64,
    pub frozen: bool,
}

impl Sim {
    /// `uuid_hashes` são os `UUID.hashCode()` das redes que vão nascer, na ordem, e `tile_hashes` os `identityHashCode` de cada cabo por índice
    /// (`z * width + x`), na ordem em que ele é posto (o que o rastro gravou, decisão 101).
    pub fn new(
        uuid_hashes: Vec<i32>,
        origin: BlockPos,
        width: i32,
        tile_hashes: HashMap<i32, VecDeque<i32>>,
    ) -> Self {
        let uuids = std::rc::Rc::new(std::cell::RefCell::new(VecDeque::from(uuid_hashes)));
        let tiles = std::cell::RefCell::new(tile_hashes);
        Self {
            owner: EnergyTransmitters::new(
                DIMENSION,
                move || {
                    Uuid::with_hash_code(
                        uuids
                            .borrow_mut()
                            .pop_front()
                            .expect("mais redes do que o rastro tem"),
                    )
                },
                move |pos| {
                    let index = (pos.z - origin.z) * width + (pos.x - origin.x);
                    tiles
                        .borrow_mut()
                        .get_mut(&index)
                        .and_then(VecDeque::pop_front)
                        .unwrap_or_else(|| panic!("o rastro não tem hash para o cabo {index}"))
                },
            ),
            level: Level::default(),
            time: 0,
            frozen: false,
        }
    }

    /// Um nível com hashes que o teste inventa (a conta que ele confere não depende da ordem das tabelas).
    pub fn free() -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        Self {
            owner: EnergyTransmitters::new(
                DIMENSION,
                move || {
                    uuid += 1;
                    Uuid::with_hash_code(uuid.wrapping_mul(0x1234_5679))
                },
                move |_| {
                    tile += 1;
                    tile.wrapping_mul(0x2545_F491)
                },
            ),
            level: Level::default(),
            time: 0,
            frozen: false,
        }
    }

    fn send(&mut self, event: Event) {
        self.owner.handle(&mut self.level, event);
    }

    /// O que o jogo entrega ao pôr ou tirar um bloco em `pos`: a capability e o aviso dos cabos vizinhos.
    fn neighbors(&mut self, pos: BlockPos) {
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.cable_at(neighbor).is_some() {
                self.send(Event::Capability(neighbor, side.opposite()));
            }
        }
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.cable_at(neighbor).is_some() {
                self.send(Event::Neighbor(neighbor));
            }
        }
    }

    pub fn place_cable(&mut self, pos: BlockPos, capacity: i64, saved: Option<Compound>) {
        self.level
            .blocks
            .insert(pos, Block::Cable { capacity, saved });
        self.send(Event::ClearRemoved(pos));
        self.neighbors(pos);
    }

    pub fn place_machine(
        &mut self,
        pos: BlockPos,
        max: i64,
        stored: i64,
        gives: bool,
    ) -> Rc<Machine> {
        let container = BufferEnergyContainer::new(max, always, always, None);
        container.set_energy(stored, None);
        let machine = Rc::new(Machine {
            container,
            gives,
            overdraw: 0,
            extracts: Default::default(),
        });
        self.level
            .blocks
            .insert(pos, Block::Machine(machine.clone()));
        self.neighbors(pos);
        machine
    }

    pub fn break_block(&mut self, pos: BlockPos) {
        let was_cable = matches!(self.level.blocks.remove(&pos), Some(Block::Cable { .. }));
        if was_cable {
            self.send(Event::Removed(pos));
        }
        self.neighbors(pos);
    }

    /// Um tick do servidor: os block entities andam (`onUpdateServer`) e o registro fecha o tick.
    pub fn tick(&mut self) {
        for pos in self.owner.positions() {
            self.send(Event::Update(pos));
        }
        self.owner.set_game_time(self.time);
        self.send(Event::ServerTickPost {
            runs_normally: !self.frozen,
        });
        self.time += 1;
    }

    pub fn buffer(&self, pos: BlockPos) -> Option<CableBuffer> {
        self.owner.buffer_at(&self.level, pos)
    }
}

fn read_rows(run: &str, scenario: &str) -> Vec<Value> {
    let file = if run.is_empty() {
        format!("energy_{scenario}.jsonl")
    } else {
        format!("energy_{run}_{scenario}.jsonl")
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn int(row: &Value, key: &str) -> i64 {
    row[key]
        .as_i64()
        .unwrap_or_else(|| panic!("{key} em {row}"))
}

fn kind(row: &Value) -> &str {
    row["e"].as_str().unwrap()
}

/// As redes do cenário: as que o `state` nomeia (as de outros testes do mesmo jogo aparecem no `estate` e não contam).
fn scenario_nets(rows: &[Value]) -> Vec<i64> {
    let mut mine: Vec<i64> = rows
        .iter()
        .filter(|row| kind(row) == "state")
        .flat_map(|row| {
            row["nets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|net| int(net, "net"))
                .chain(
                    row["cables"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|cable| cable["net"].as_i64()),
                )
        })
        .collect();
    mine.sort_unstable();
    mine.dedup();
    mine
}

fn trace_estate(row: &Value, mine: &[i64]) -> Value {
    let mut nets: Vec<(i64, i64, i64)> = row["nets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|net| mine.contains(&int(net, "net")))
        .map(|net| (int(net, "energy"), int(net, "capacity"), int(net, "prev")))
        .collect();
    nets.sort_unstable();
    let actors: Vec<(String, i64)> = row["actors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["name"].as_str().unwrap().to_owned(), int(a, "energy")))
        .collect();
    let cables: Vec<(i64, i64, i64, i64)> = row["cables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (int(c, "x"), int(c, "buf"), int(c, "share"), int(c, "cap")))
        .collect();
    serde_json::json!({ "nets": nets, "actors": actors, "cables": cables })
}

struct Run {
    origin: BlockPos,
    sim: Sim,
    actors: Vec<(String, Rc<Machine>)>,
    pushed: Vec<i64>,
}

impl Run {
    fn at(&self, x: i64, z: i64) -> BlockPos {
        BlockPos::new(
            self.origin.x + x as i32,
            self.origin.y,
            self.origin.z + z as i32,
        )
    }

    fn apply(&mut self, mark: &str) {
        let mark = mark.strip_prefix("pre ").unwrap_or(mark);
        let words: Vec<&str> = mark.split(' ').collect();
        let number = |i: usize| words[i].parse::<i64>().unwrap();
        match words[0] {
            "place" => {
                self.sim
                    .place_cable(self.at(number(1), number(2)), tier_capacity(words[3]), None)
            }
            "break" => self.sim.break_block(self.at(number(1), number(2))),
            "chamber" => {
                let pos = BlockPos::new(
                    self.origin.x + number(2) as i32,
                    self.origin.y + number(3) as i32,
                    self.origin.z + number(4) as i32,
                );
                let machine = self.sim.place_machine(pos, number(5), number(6), false);
                self.actors.push((words[1].to_owned(), machine));
            }
            "setenergy" => {
                let machine = &self
                    .actors
                    .iter()
                    .find(|(name, _)| name == words[1])
                    .unwrap()
                    .1;
                machine.container.set_energy(number(2), None);
            }
            "push" => {
                let pos = self.at(number(1), number(2));
                let got = self.sim.owner.insert_external(pos, number(3) as i32);
                self.pushed.push(i64::from(got));
            }
            "finish" => {}
            other => panic!("ação desconhecida no rastro: {other}"),
        }
    }

    fn estate(&self, rows_x: &[i64]) -> Value {
        let mut nets: Vec<(i64, i64, i64)> = Vec::new();
        let mut seen = Vec::new();
        for pos in self.sim.owner.positions() {
            if let Some(network) = self.sim.owner.network_of(pos)
                && !seen.contains(&network)
            {
                seen.push(network);
                let net = self.sim.owner.family().network(network).unwrap();
                nets.push((net.buffer(), net.capacity(), net.prev_transfer_amount()));
            }
        }
        nets.sort_unstable();
        let actors: Vec<(String, i64)> = self
            .actors
            .iter()
            .map(|(name, machine)| (name.clone(), machine.container.amount()))
            .collect();
        let cables: Vec<(i64, i64, i64, i64)> = rows_x
            .iter()
            .filter_map(|x| {
                let id = self.sim.owner.cable_at(self.at(*x, 0))?;
                let cable = self.sim.owner.family().cable(id);
                Some((
                    *x,
                    cable.buffer.amount(),
                    cable.save_share_amount(),
                    cable.capacity(),
                ))
            })
            .collect();
        serde_json::json!({ "nets": nets, "actors": actors, "cables": cables })
    }
}

fn run(run_name: &str, scenario: &str) {
    let rows = read_rows(run_name, scenario);
    let mine = scenario_nets(&rows);
    let last_tick = rows.iter().map(|row| int(row, "t")).max().unwrap();
    let setup = &rows[0];
    let width = int(setup, "width") as i32;
    let mut tile_hashes: HashMap<i32, VecDeque<i32>> = HashMap::new();
    for tile in setup["tiles"].as_array().unwrap() {
        tile_hashes
            .entry(int(tile, "x") as i32)
            .or_default()
            .push_back(int(tile, "hash") as i32);
    }
    for row in rows.iter().filter(|row| kind(row) == "tile") {
        tile_hashes
            .entry(int(row, "x") as i32)
            .or_default()
            .push_back(int(row, "hash") as i32);
    }
    let begin = rows.iter().find(|row| kind(row) == "state").unwrap();
    let mut uuid_hashes: Vec<(i64, i32)> = begin["nets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|net| (int(net, "net"), int(net, "hash") as i32))
        .collect();
    uuid_hashes.extend(
        rows.iter()
            .filter(|row| kind(row) == "created" && mine.contains(&int(row, "net")))
            .map(|row| (int(row, "net"), int(row, "hash") as i32)),
    );
    uuid_hashes.sort_unstable();
    uuid_hashes.dedup();
    let uuid_hashes = uuid_hashes.into_iter().map(|(_, hash)| hash).collect();
    let origin = BlockPos::new(
        int(setup, "x") as i32,
        int(setup, "y") as i32,
        int(setup, "z") as i32,
    );
    let mut state = Run {
        origin,
        sim: Sim::new(uuid_hashes, origin, width, tile_hashes),
        actors: Vec::new(),
        pushed: Vec::new(),
    };
    let trace_pushes: Vec<i64> = rows
        .iter()
        .filter(|row| kind(row) == "push")
        .map(|row| int(row, "got"))
        .collect();
    for t in 0..=last_tick {
        let marks: Vec<(bool, String)> = rows
            .iter()
            .filter(|row| kind(row) == "mark" && int(row, "t") == t)
            .map(|row| {
                let mark = row["m"].as_str().unwrap();
                (mark.starts_with("pre "), mark.to_owned())
            })
            .collect();
        if marks.iter().any(|(_, mark)| mark == "pre finish") {
            break;
        }
        for (pre, mark) in &marks {
            if *pre {
                state.apply(mark);
            }
        }
        state.sim.tick();
        if let Some(row) = rows
            .iter()
            .find(|row| kind(row) == "estate" && int(row, "t") == t)
        {
            let want = trace_estate(row, &mine);
            let xs: Vec<i64> = want["cables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c[0].as_i64().unwrap())
                .collect();
            let got = state.estate(&xs);
            assert_eq!(
                got["nets"], want["nets"],
                "{run_name} {scenario}, tick {t}: redes (energia, capacidade, prev)"
            );
            assert_eq!(
                got["actors"], want["actors"],
                "{run_name} {scenario}, tick {t}: máquinas"
            );
            assert_eq!(
                got["cables"], want["cables"],
                "{run_name} {scenario}, tick {t}: cabos (buffer, share, capacidade)"
            );
        }
        for (pre, mark) in &marks {
            if !*pre {
                state.apply(mark);
            }
        }
    }
    assert_eq!(
        state.pushed, trace_pushes,
        "{run_name} {scenario}: o que cada push aceitou"
    );
}

const SCENARIOS: [&str; 6] = [
    "line_consumer",
    "two_consumers",
    "tiers",
    "merge_full",
    "split_middle",
    "break_end_odd",
];

#[test]
fn scenarios_of_the_first_run_match_the_original_by_game_events() {
    for scenario in SCENARIOS {
        run("", scenario);
    }
}

#[test]
fn scenarios_of_the_second_run_match_the_original_by_game_events() {
    for scenario in SCENARIOS {
        run("run2", scenario);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O mundo salvo e reaberto: `energy_reopen.jsonl`. O chunk sai (o ticket, `onChunkUnloaded` de cada cabo com o `saveAdditional` logo depois, `setRemoved`), volta com o NBT
// que os cabos gravaram, e a rede nova tem a energia de antes.

impl Sim {
    fn unload(&mut self, chunk: ChunkPos, saved: &mut HashMap<BlockPos, (i64, Compound)>) {
        let mut cables: Vec<BlockPos> = self
            .owner
            .positions()
            .into_iter()
            .filter(|pos| pos.chunk() == chunk)
            .collect();
        cables.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        self.send(Event::Ticket {
            chunk,
            loaded: false,
        });
        for pos in &cables {
            self.send(Event::ChunkUnloaded(*pos));
            let capacity = match self.level.blocks.get(pos) {
                Some(Block::Cable { capacity, .. }) => *capacity,
                _ => unreachable!("o cabo está no nível"),
            };
            saved.insert(
                *pos,
                (capacity, self.owner.write(*pos).expect("o cabo grava")),
            );
        }
        for pos in &cables {
            self.send(Event::Removed(*pos));
            self.level.blocks.remove(pos);
        }
        self.level.unloaded.extend(cables);
    }

    fn reload(&mut self, chunk: ChunkPos, saved: &mut HashMap<BlockPos, (i64, Compound)>) {
        self.send(Event::Ticket {
            chunk,
            loaded: true,
        });
        let mut back: Vec<BlockPos> = saved
            .keys()
            .copied()
            .filter(|pos| pos.chunk() == chunk)
            .collect();
        back.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        self.level.unloaded.retain(|pos| pos.chunk() != chunk);
        for pos in back {
            let (capacity, nbt) = saved.remove(&pos).unwrap();
            self.place_cable(pos, capacity, Some(nbt));
        }
    }
}

#[test]
fn a_reopened_chunk_gives_the_networks_energy_back_like_the_original() {
    let rows = read_rows("", "reopen");
    let setup = &rows[0];
    let origin = BlockPos::new(
        int(setup, "x") as i32,
        int(setup, "y") as i32,
        int(setup, "z") as i32,
    );
    let mut sim = Sim::free();
    let tiers = ["basic", "basic", "advanced", "basic", "basic"];
    let at = |x: i64| BlockPos::new(origin.x + x as i32, origin.y, origin.z);
    let cables: Vec<BlockPos> = (28..33).map(at).collect();
    for (pos, tier) in cables.iter().zip(tiers) {
        sim.place_cable(*pos, tier_capacity(tier), None);
    }
    for _ in 0..6 {
        sim.tick();
    }
    assert_eq!(sim.owner.insert_external(cables[0], 31_001), 31_001);
    for _ in 0..4 {
        sim.tick();
    }
    let nbt_row = rows.iter().find(|row| kind(row) == "nbt").unwrap();
    let written = sim.owner.write(cables[0]).unwrap();
    assert_eq!(
        written.get(ENERGY_KEY),
        Some(&Tag::Long(int(nbt_row, "energy"))),
        "o `energy` que o cabo grava"
    );
    // O chunk sai com os cabos de um lado só (o rastro põe os cinco no mesmo): saem todos os cabos que o chunk tem, e os outros ficam.
    let mut saved = HashMap::new();
    let chunks: Vec<ChunkPos> = {
        let mut list: Vec<ChunkPos> = cables.iter().map(|pos| pos.chunk()).collect();
        list.dedup();
        list
    };
    for chunk in &chunks {
        sim.unload(*chunk, &mut saved);
    }
    sim.tick();
    assert!(sim.owner.positions().is_empty(), "todos os cabos saíram");
    for chunk in &chunks {
        sim.reload(*chunk, &mut saved);
    }
    for _ in 0..6 {
        sim.tick();
    }
    let last = rows
        .iter()
        .rev()
        .find(|row| kind(row) == "estate" && !row["nets"].as_array().unwrap().is_empty())
        .unwrap();
    let want = &last["nets"][0];
    let network = sim.owner.network_of(cables[0]).expect("a rede voltou");
    let net = sim.owner.family().network(network).unwrap();
    assert_eq!(
        (net.buffer(), net.capacity()),
        (int(want, "energy"), int(want, "capacity")),
        "a energia da rede depois de reabrir"
    );
    for (x, cable) in last["cables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (int(c, "x"), c))
    {
        let id = sim.owner.cable_at(at(x)).unwrap();
        assert_eq!(
            sim.owner.family().cable(id).save_share_amount(),
            int(cable, "share"),
            "o share do cabo {x}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que o jogo faz com o cabo posto por um item: `ClearRemoved`, o aviso do vizinho, `PlacedByItem` (`onAdded` e `refreshConnections`) e só depois o tick.

#[test]
fn cables_placed_by_item_one_after_the_other_connect_both_ways() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(100, 64, 100);
    for x in 0..5 {
        let pos = BlockPos::new(origin.x + x, origin.y, origin.z);
        sim.place_cable(pos, 8_000, None);
        sim.send(Event::PlacedByItem(pos));
        // O jogo tica entre uma colocação e outra (a mão do jogador é mais lenta): a rede dos de antes já existe quando o seguinte chega.
        for _ in 0..3 {
            sim.tick();
        }
    }
    for _ in 0..3 {
        sim.tick();
    }
    for x in 0..5 {
        let pos = BlockPos::new(origin.x + x, origin.y, origin.z);
        let view = sim.owner.view_at(&sim.level, pos).unwrap();
        let west = view.connected[Face::West as usize];
        let east = view.connected[Face::East as usize];
        assert_eq!(
            (west, east),
            (x > 0, x < 4),
            "o cabo {x}: braços para oeste e leste"
        );
    }
    assert_eq!(
        sim.owner.registry().networks_in_order().len(),
        1,
        "uma rede só"
    );
}

// ---------------------------------------------------------------------------------------------------------------------------
// O Configurator (`TileEntityTransmitter.onSneakRightClick`): o tipo de conexão do lado cicla `NORMAL`, `PUSH`, `PULL`, `NONE`, a mensagem diz o tipo, e o braço some (e o do
// vizinho junto) quando o lado vai a `NONE`.

fn line(sim: &mut Sim, origin: BlockPos, count: i32) -> Vec<BlockPos> {
    let cables: Vec<BlockPos> = (0..count)
        .map(|x| BlockPos::new(origin.x + x, origin.y, origin.z))
        .collect();
    for pos in &cables {
        sim.place_cable(*pos, 8_000, None);
        sim.send(Event::PlacedByItem(*pos));
    }
    for _ in 0..3 {
        sim.tick();
    }
    cables
}

fn sneak(sim: &mut Sim, pos: BlockPos, side: Face, hit: Option<Face>) -> bool {
    let Sim { owner, level, .. } = sim;
    owner.sneak_right_click(level, pos, side, hit)
}

#[test]
fn the_configurator_cycles_a_side_and_the_arm_goes_and_comes_back() {
    use machinery_basis_core::transmission::transmitter::Overlay;
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line(&mut sim, origin, 3);
    let kinds = [
        ConnectionType::Push,
        ConnectionType::Pull,
        ConnectionType::None,
        ConnectionType::Normal,
    ];
    for kind in kinds {
        // Agachado, olhando o braço para leste do cabo do meio.
        assert!(
            sneak(&mut sim, cables[1], Face::North, Some(Face::East)),
            "o clique é do cabo"
        );
        assert_eq!(
            sim.level.overlays.last(),
            Some(&Overlay::ConnectionType { connection: kind }),
            "a mensagem diz o tipo"
        );
        let view = sim.owner.view_at(&sim.level, cables[1]).unwrap();
        assert_eq!(view.raw[Face::East as usize], kind);
        let arm = kind != ConnectionType::None;
        assert_eq!(
            view.connected[Face::East as usize],
            arm,
            "{kind:?}: o braço do cabo do meio"
        );
        let other = sim.owner.view_at(&sim.level, cables[2]).unwrap();
        assert_eq!(
            other.connected[Face::West as usize],
            arm,
            "{kind:?}: o braço do vizinho, que o aviso refaz"
        );
    }
    // Fora de um braço (o olhar no centro) o lado clicado é o que cicla.
    assert!(sneak(&mut sim, cables[1], Face::Up, None));
    assert_eq!(
        sim.owner.view_at(&sim.level, cables[1]).unwrap().raw[Face::Up as usize],
        ConnectionType::Push
    );
}

/// O que o jogo mostra de um cabo é a última vista publicada dele (`publish` vira as propriedades de lado do bloco): as conexões que o dono calcula
/// e a vista que chegou ao bloco têm de ser as mesmas depois de todos os efeitos de um clique.
fn last_published(sim: &Sim, pos: BlockPos) -> CableView {
    sim.level
        .published
        .iter()
        .rev()
        .find(|(at, _)| *at == pos)
        .map(|(_, view)| view.clone())
        .unwrap_or_else(|| panic!("o cabo {pos:?} nunca publicou"))
}

#[test]
fn what_the_game_shows_follows_the_configurator_in_both_cables() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line(&mut sim, origin, 3);
    for kind in [
        ConnectionType::Push,
        ConnectionType::Pull,
        ConnectionType::None,
        ConnectionType::Normal,
    ] {
        assert!(sneak(&mut sim, cables[1], Face::North, Some(Face::East)));
        // O jogo tica entre um clique e outro, e o que o tick refaz também chega ao bloco.
        for _ in 0..2 {
            sim.tick();
        }
        let arm = kind != ConnectionType::None;
        for (pos, side) in [(cables[1], Face::East), (cables[2], Face::West)] {
            let live = sim.owner.view_at(&sim.level, pos).unwrap();
            let shown = last_published(&sim, pos);
            assert_eq!(live.connected[side as usize], arm, "{kind:?}: o dono");
            assert_eq!(
                shown.connected[side as usize], arm,
                "{kind:?}: o braço que o jogo mostra em {pos:?}"
            );
            assert_eq!(
                shown, live,
                "{kind:?}: a vista publicada é a viva em {pos:?}"
            );
        }
    }
}

#[test]
fn notify_tile_change_walks_the_neighbors_in_direction_values_order() {
    // `Transmitter.notifyTileChange` -> `WorldUtils.notifyLoadedNeighborsOfTileChange` percorre `EnumUtils.DIRECTIONS` (`Direction.values()`: baixo, cima, norte, sul, oeste, leste),
    // e não `NeighborUpdater.UPDATE_ORDER` (oeste, leste, baixo, cima, norte, sul) de `Level.updateNeighborsAt`. Cada vizinho que recebe o aviso relê as conexões dele e pergunta se os
    // lados dele estão carregados; o ponto a dois blocos do centro, na direção do vizinho, só esse vizinho pergunta.
    let mut sim = Sim::free();
    let center = BlockPos::new(10, 64, 10);
    for pos in std::iter::once(center).chain(Face::ALL.map(|side| center.relative(side))) {
        sim.place_cable(pos, 8_000, None);
        sim.send(Event::PlacedByItem(pos));
    }
    for _ in 0..3 {
        sim.tick();
    }
    sim.level.asked.borrow_mut().clear();
    // Agachado no braço de cima: o vizinho de cima também é atualizado por `refreshConnections(side)`, então fica fora da comparação.
    assert!(sneak(&mut sim, center, Face::Up, Some(Face::Up)));
    let asked = sim.level.asked.borrow().clone();
    let mut order = Vec::new();
    for pos in asked {
        for side in Face::ALL {
            let outer = center.relative(side).relative(side);
            if pos == outer && !order.contains(&side) {
                order.push(side);
            }
        }
    }
    order.retain(|side| *side != Face::Up);
    assert_eq!(
        order,
        [Face::Down, Face::North, Face::South, Face::West, Face::East]
    );
}

#[test]
fn a_cable_in_pull_takes_energy_from_the_neighbor_that_gives_it() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line(&mut sim, origin, 3);
    let source = sim.place_machine(
        BlockPos::new(origin.x, origin.y + 1, origin.z),
        1_000_000,
        20_000,
        true,
    );
    sim.tick();
    // Sem `PULL` a máquina que dá não é puxada: o cabo só a vê como aceitador.
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(source.container.amount(), 20_000);
    // `NORMAL`, `PUSH`, `PULL` no lado de cima do primeiro cabo.
    for _ in 0..2 {
        sneak(&mut sim, cables[0], Face::Up, Some(Face::Up));
    }
    assert_eq!(
        sim.owner.view_at(&sim.level, cables[0]).unwrap().raw[Face::Up as usize],
        ConnectionType::Pull
    );
    for _ in 0..3 {
        sim.tick();
    }
    // O contêiner da rede (3 cabos de 8.000) enche com o que a máquina deu, de uma vez, e o que ela deu sai dela.
    let network = sim.owner.network_of(cables[0]).unwrap();
    let net = sim.owner.family().network(network).unwrap();
    assert_eq!(
        net.buffer() + source.container.amount(),
        20_000,
        "a energia só mudou de lugar"
    );
    assert!(net.buffer() > 0 && net.buffer() <= 24_000, "o cabo puxou");
}

#[test]
fn a_redstone_reactive_cable_lets_go_while_it_is_powered() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line(&mut sim, origin, 3);
    let Sim { owner, level, .. } = &mut sim;
    assert!(
        owner.right_click(level, cables[1]),
        "o clique liga o redstone reativo"
    );
    assert!(owner.view_at(level, cables[1]).unwrap().redstone_reactive);
    // Um bloco de redstone ao lado do cabo do meio: o vizinho muda e o cabo solta as ligações.
    level
        .powered
        .push(BlockPos::new(origin.x + 1, origin.y + 1, origin.z));
    sim.send(Event::Neighbor(cables[1]));
    sim.tick();
    let view = sim.owner.view_at(&sim.level, cables[1]).unwrap();
    assert!(view.redstone_activated, "o cabo está ativado");
    assert!(
        !view.connected[Face::West as usize] && !view.connected[Face::East as usize],
        "sem braços enquanto está ligado"
    );
    assert!(!view.insert.iter().any(|&b| b), "sem capability de entrada");
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que o jogo mostra e o que o dono faz com cada evento (as mutações da frente `cabo-ajustes`): cada teste abaixo é o que mata uma mutação que sobreviveu aos rastros.

fn line_at(sim: &mut Sim, origin: BlockPos, count: i32) -> Vec<BlockPos> {
    line(sim, origin, count)
}

/// O que o jogo mostra de cada cabo é a última vista publicada dele, e tem de ser a que o dono tem agora.
fn assert_shown(sim: &Sim, step: &str) {
    for pos in sim.owner.positions() {
        let live = sim.owner.view_at(&sim.level, pos).unwrap();
        let Some((_, shown)) = sim.level.published.iter().rev().find(|(at, _)| *at == pos) else {
            continue;
        };
        // `onRightClick` liga o redstone reativo sem `sendUpdatePacket`: o modelo não o usa.
        let (mut shown, mut live) = (shown.clone(), live);
        shown.redstone_reactive = false;
        live.redstone_reactive = false;
        assert_eq!(
            shown, live,
            "{step}: o que o jogo mostra do cabo {pos:?} é o que o dono tem"
        );
    }
}

#[test]
fn what_the_game_shows_is_always_what_the_owner_has_after_every_operation() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line_at(&mut sim, origin, 4);
    assert_shown(&sim, "a linha posta");
    sim.place_machine(
        BlockPos::new(origin.x + 3, origin.y + 1, origin.z),
        100_000,
        0,
        false,
    );
    assert_shown(&sim, "a máquina posta");
    sim.tick();
    assert_shown(&sim, "o tick com a máquina");
    for round in 0..4 {
        sneak(&mut sim, cables[1], Face::North, Some(Face::East));
        assert_shown(&sim, &format!("o clique no leste, volta {round}"));
        sneak(&mut sim, cables[3], Face::Up, Some(Face::Up));
        assert_shown(&sim, &format!("o clique na máquina, volta {round}"));
        sim.tick();
        assert_shown(&sim, &format!("o tick depois dos cliques, volta {round}"));
    }
    // O redstone reativo liga e desliga com o sinal ao lado.
    assert!(sim.owner.right_click(&mut sim.level, cables[2]));
    assert_shown(&sim, "o redstone reativo ligado");
    let powered = BlockPos::new(origin.x + 2, origin.y + 1, origin.z);
    sim.level.powered.push(powered);
    sim.send(Event::Neighbor(cables[2]));
    assert_shown(&sim, "o sinal chegou");
    sim.tick();
    sim.level.powered.clear();
    sim.send(Event::Neighbor(cables[2]));
    assert_shown(&sim, "o sinal saiu");
    sim.tick();
    assert_shown(&sim, "o tick depois do sinal");
    // O cabo quebrado sai: o que o bloco mostra não muda mais por ele.
    sim.break_block(cables[1]);
    let published = sim.level.published.len();
    for _ in 0..3 {
        sim.tick();
    }
    assert_shown(&sim, "o cabo do meio quebrado");
    assert!(
        sim.level.published[published..]
            .iter()
            .all(|(at, _)| *at != cables[1]),
        "o cabo que saiu não publica mais"
    );
    sim.place_cable(cables[1], 8_000, None);
    sim.send(Event::PlacedByItem(cables[1]));
    for _ in 0..3 {
        sim.tick();
    }
    assert_shown(&sim, "o cabo reposto");
    for pos in sim.owner.positions() {
        assert!(
            sim.owner.view_at(&sim.level, pos).unwrap().has_network,
            "o cabo {pos:?} tem rede no fim"
        );
    }
}

/// `TileEntityTransmitter.canInsert(side)` é o tipo visível do lado (`getConnectionType`: `NONE` onde não há braço, e `NORMAL` onde o braço é para outro transmissor, seja qual for o
/// tipo cru) e a capability existe onde o tipo cru não é `NONE` e o cabo não está ativado por redstone; o contêiner que aceita de fora é o da rede.
#[test]
fn the_capability_of_a_cable_follows_the_visible_type_and_the_network() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line_at(&mut sim, origin, 3);
    sim.place_machine(cables[1].relative(Face::Up), 100_000, 0, false);
    sim.tick();
    let view = sim.owner.view_at(&sim.level, cables[1]).unwrap();
    assert!(view.has_network);
    // Em cima fica a máquina (um braço para um aceitador) e em baixo não há nada: o tipo cru é `NORMAL` nos dois (a capability existe), o visível só onde há braço.
    let (up, down, east) = (Face::Up as usize, Face::Down as usize, Face::East as usize);
    assert_eq!(view.raw[down], ConnectionType::Normal);
    assert_eq!(view.kind[down], ConnectionType::None);
    assert!(view.exists[down] && !view.insert[down]);
    // O braço da máquina mostra o tipo cru: `PUSH` só entrega (não deixa entrar), `PULL` deixa entrar, `NONE` tira a capability.
    for (raw, kind, exists, insert) in [
        (ConnectionType::Push, ConnectionType::Push, true, false),
        (ConnectionType::Pull, ConnectionType::Pull, true, true),
        (ConnectionType::None, ConnectionType::None, false, false),
        (ConnectionType::Normal, ConnectionType::Normal, true, true),
    ] {
        sneak(&mut sim, cables[1], Face::North, Some(Face::Up));
        let view = sim.owner.view_at(&sim.level, cables[1]).unwrap();
        assert_eq!(view.raw[up], raw);
        assert_eq!(view.kind[up], kind, "{raw:?}: o tipo visível");
        assert_eq!(view.exists[up], exists, "{raw:?}: a capability existe");
        assert_eq!(
            view.insert[up], insert,
            "{raw:?}: a capability deixa entrar"
        );
    }
    // O braço para outro cabo mostra `NORMAL` seja qual for o tipo cru, e `NONE` corta.
    for (raw, kind, exists, insert) in [
        (ConnectionType::Push, ConnectionType::Normal, true, true),
        (ConnectionType::Pull, ConnectionType::Normal, true, true),
        (ConnectionType::None, ConnectionType::None, false, false),
        (ConnectionType::Normal, ConnectionType::Normal, true, true),
    ] {
        sneak(&mut sim, cables[1], Face::North, Some(Face::East));
        let view = sim.owner.view_at(&sim.level, cables[1]).unwrap();
        assert_eq!(view.raw[east], raw);
        assert_eq!(view.kind[east], kind, "{raw:?}: o tipo visível");
        assert_eq!(view.exists[east], exists, "{raw:?}: a capability existe");
        assert_eq!(
            view.insert[east], insert,
            "{raw:?}: a capability deixa entrar"
        );
    }
    // O contêiner é o da rede: três cabos de 8.000 e a energia que um vizinho empurrou.
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(sim.owner.container_state(cables[0]).unwrap(), (24_000, 0));
    assert_eq!(sim.owner.insert_external(cables[0], 1_000), 1_000);
    for pos in &cables {
        assert_eq!(sim.owner.container_state(*pos).unwrap(), (24_000, 1_000));
        let buffer = sim.buffer(*pos).unwrap();
        assert_eq!((buffer.capacity, buffer.amount), (24_000, 1_000));
    }
    // Um cabo sozinho, sem rede ainda: o contêiner é o do próprio cabo, e nenhuma face deixa entrar (a capability é a da rede).
    let lone = BlockPos::new(30, 64, 30);
    sim.place_cable(lone, 8_000, None);
    let view = sim.owner.view_at(&sim.level, lone).unwrap();
    assert!(!view.has_network);
    assert!(view.insert.iter().all(|inserts| !inserts));
    assert!(view.exists.iter().all(|exists| *exists));
    assert_eq!(sim.owner.container_state(lone).unwrap(), (8_000, 0));
}

#[test]
fn the_positions_of_the_cables_are_sorted_by_x_then_y_then_z() {
    let mut sim = Sim::free();
    for pos in [
        BlockPos::new(2, 64, 0),
        BlockPos::new(1, 64, 5),
        BlockPos::new(1, 63, 9),
    ] {
        sim.place_cable(pos, 8_000, None);
    }
    assert_eq!(
        sim.owner.positions(),
        vec![
            BlockPos::new(1, 63, 9),
            BlockPos::new(1, 64, 5),
            BlockPos::new(2, 64, 0)
        ]
    );
}

/// `runsNormally`: com o servidor parado o registro ainda trata os chunks e as órfãs, mas nenhuma rede se atualiza (não entrega energia).
#[test]
fn a_frozen_server_tick_moves_no_energy() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    let cables = line_at(&mut sim, origin, 3);
    let machine = sim.place_machine(
        BlockPos::new(origin.x + 3, origin.y, origin.z),
        100_000,
        0,
        false,
    );
    sim.tick();
    assert_eq!(sim.owner.insert_external(cables[0], 5_000), 5_000);
    sim.frozen = true;
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(machine.container.amount(), 0, "parado, nada sai da rede");
    assert_eq!(sim.owner.container_state(cables[0]).unwrap().1, 5_000);
    sim.frozen = false;
    sim.tick();
    assert!(machine.container.amount() > 0, "andando, a rede entrega");
}

/// `ChunkTicketLevelUpdatedEvent`: o chunk que deixa de ser acessível tira os cabos dele da rede (`chunkAccessibilityChange(false)`: toma a parte da energia e separa) e o que volta os
/// junta de novo.
#[test]
fn a_chunk_that_stops_being_accessible_leaves_the_network_and_comes_back() {
    let mut sim = Sim::free();
    // Os cabos de x 14 a 17 passam da borda do chunk: dois no chunk 0 e dois no 1.
    let cables = line_at(&mut sim, BlockPos::new(14, 64, 10), 4);
    assert_eq!(cables[2].chunk(), ChunkPos::new(1, 0));
    assert_eq!(sim.owner.insert_external(cables[0], 4_000), 4_000);
    sim.tick();
    let network = sim.owner.network_of(cables[0]).unwrap();
    assert_eq!(sim.owner.network_of(cables[3]), Some(network));
    let net_energy = |sim: &Sim, pos: BlockPos| {
        let id = sim.owner.network_of(pos)?;
        sim.owner.family().network(id).map(|net| net.buffer())
    };
    assert_eq!(net_energy(&sim, cables[0]), Some(4_000));
    sim.send(Event::Ticket {
        chunk: ChunkPos::new(1, 0),
        loaded: false,
    });
    sim.tick();
    assert_eq!(
        sim.owner.network_of(cables[2]),
        None,
        "o chunk que saiu não tem rede"
    );
    assert_eq!(sim.owner.network_of(cables[3]), None);
    let rest = net_energy(&sim, cables[0]).unwrap();
    assert!(
        rest < 4_000,
        "as partes dos cabos do chunk que saiu não ficam na rede: {rest}"
    );
    assert_eq!(
        sim.owner.network_of(cables[1]),
        sim.owner.network_of(cables[0])
    );
    sim.send(Event::Ticket {
        chunk: ChunkPos::new(1, 0),
        loaded: true,
    });
    for _ in 0..3 {
        sim.tick();
    }
    let joined = sim.owner.network_of(cables[0]).unwrap();
    for pos in &cables {
        assert_eq!(
            sim.owner.network_of(*pos),
            Some(joined),
            "{pos:?} volta à rede"
        );
    }
    assert!(
        net_energy(&sim, cables[0]).unwrap() >= rest,
        "o que a rede tinha não se perde na volta"
    );
}

/// `onChunkUnloaded`: o cabo toma a parte dele da energia da rede antes de o chunk ir para o disco.
#[test]
fn a_cable_of_a_chunk_that_unloads_takes_its_share_from_the_network() {
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    assert_eq!(sim.owner.insert_external(cables[0], 3_000), 3_000);
    sim.tick();
    assert_eq!(
        sim.owner.container_state(cables[2]).unwrap(),
        (24_000, 3_000)
    );
    sim.send(Event::ChunkUnloaded(cables[0]));
    let (_, left) = sim.owner.container_state(cables[1]).unwrap();
    assert_eq!(left, 2_000, "a parte de um dos três cabos saiu da rede");
    let saved = sim.owner.write(cables[0]).unwrap();
    assert_eq!(
        saved.get(ENERGY_KEY),
        Some(&Tag::Long(1_000)),
        "e está no NBT dele"
    );
    assert_eq!(
        ENERGY_KEY, "energy",
        "a chave de `SerializationConstants.ENERGY`"
    );
}

#[test]
fn a_world_that_closes_leaves_no_cable_and_no_network() {
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    assert_eq!(sim.owner.positions().len(), 3);
    assert_eq!(sim.owner.registry().networks_in_order().len(), 1);
    sim.send(Event::Reset);
    assert!(sim.owner.positions().is_empty());
    assert!(sim.owner.registry().networks_in_order().is_empty());
    assert_eq!(sim.owner.cable_at(cables[0]), None);
    // Um mundo novo começa do zero com o mesmo dono.
    sim.level.blocks.clear();
    let again = line_at(&mut sim, BlockPos::new(10, 64, 10), 2);
    assert_eq!(sim.owner.positions(), again);
    assert_eq!(sim.owner.registry().networks_in_order().len(), 1);
}

/// `onAdded`: o bloco posto por um item entra na rede e relê as conexões na hora, sem esperar o tick do block entity.
#[test]
fn a_cable_placed_by_an_item_joins_and_connects_before_its_first_tick() {
    let mut sim = Sim::free();
    let a = BlockPos::new(10, 64, 10);
    let b = BlockPos::new(11, 64, 10);
    for pos in [a, b] {
        sim.level.blocks.insert(
            pos,
            Block::Cable {
                capacity: 8_000,
                saved: None,
            },
        );
        sim.send(Event::ClearRemoved(pos));
    }
    assert!(
        !sim.owner.view_at(&sim.level, b).unwrap().connected[Face::West as usize],
        "só entrou: ainda não leu os lados"
    );
    sim.send(Event::PlacedByItem(b));
    assert!(
        sim.owner.view_at(&sim.level, b).unwrap().connected[Face::West as usize],
        "o `onAdded` relê as conexões"
    );
    // O `onWorldJoin(level, false)` do `onAdded` põe o cabo no registro: o tick o acha como órfão, sem passar pelo ticker do block entity.
    sim.send(Event::ServerTickPost {
        runs_normally: true,
    });
    assert!(
        sim.owner.network_of(b).is_some(),
        "o cabo tem rede sem ter passado pelo ticker"
    );
}

/// Um vizinho num chunk que não está carregado não é um transmissor a que se liga (`WorldUtils.isBlockLoaded`), e o aviso de um clique não o alcança.
#[test]
fn a_neighbor_in_an_unloaded_chunk_is_not_connected_and_not_told() {
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    assert!(sim.owner.view_at(&sim.level, cables[1]).unwrap().connected[Face::East as usize]);
    sim.level.unloaded.push(cables[2]);
    sim.send(Event::Neighbor(cables[1]));
    assert!(
        !sim.owner.view_at(&sim.level, cables[1]).unwrap().connected[Face::East as usize],
        "o vizinho descarregado não é um braço"
    );
    // O cabo do fim, que não foi avisado, segue como estava.
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    sim.level.unloaded.push(cables[2]);
    // `NORMAL`, `PUSH`, `PULL` e `NONE`: no último o braço some, e o vizinho descarregado não é avisado.
    for _ in 0..3 {
        sneak(&mut sim, cables[1], Face::North, Some(Face::East));
    }
    assert_eq!(
        sim.owner.view_at(&sim.level, cables[1]).unwrap().raw[Face::East as usize],
        ConnectionType::None
    );
    assert!(
        sim.owner.view_at(&sim.level, cables[2]).unwrap().connected[Face::West as usize],
        "o aviso do clique não chega ao vizinho descarregado"
    );
}

/// O dono nunca pergunta pela capability de um bloco que é um cabo (o cabo não é aceitador de cabo).
#[test]
fn a_cable_is_never_the_acceptor_of_another_cable() {
    let mut sim = Sim::free();
    sim.level.cables_have_capability = true;
    line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    for _ in 0..2 {
        sim.tick();
    }
    let network = sim.owner.registry().networks_in_order()[0];
    assert_eq!(sim.owner.registry().network(network).acceptor_count(), 0);
}

/// O UUID que o cabo guarda da rede (`setTransmitterNetwork`, que vai no pacote de atualização) é o da rede do registro, também depois de duas redes se fundirem.
#[test]
fn the_network_each_cable_remembers_is_the_one_of_the_registry() {
    let mut sim = Sim::free();
    let origin = BlockPos::new(10, 64, 10);
    for x in [0, 1, 3, 4] {
        let pos = BlockPos::new(origin.x + x, origin.y, origin.z);
        sim.place_cable(pos, 8_000, None);
        sim.send(Event::PlacedByItem(pos));
    }
    for _ in 0..3 {
        sim.tick();
    }
    let check = |sim: &Sim| {
        let mut seen = std::collections::HashSet::new();
        for pos in sim.owner.positions() {
            let id = sim.owner.network_of(pos).expect("tem rede");
            let uuid = sim.owner.registry().network(id).uuid();
            let expected = (u128::from(uuid.most_significant as u64) << 64)
                | u128::from(uuid.least_significant as u64);
            assert_ne!(expected, 0);
            assert_eq!(
                sim.owner.transmitter(pos).unwrap().transmitter_network(),
                Some(expected),
                "o cabo {pos:?} guarda o UUID da rede dele"
            );
            seen.insert(expected);
        }
        seen.len()
    };
    assert_eq!(check(&sim), 2, "duas redes");
    let bridge = BlockPos::new(origin.x + 2, origin.y, origin.z);
    sim.place_cable(bridge, 8_000, None);
    sim.send(Event::PlacedByItem(bridge));
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(check(&sim), 1, "a rede fundida tem um UUID só");
}

#[test]
fn the_hash_of_each_new_cable_is_asked_once_in_the_order_they_enter() {
    let asked = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = asked.clone();
    let mut sim = Sim {
        owner: EnergyTransmitters::new(
            DIMENSION,
            || Uuid::with_hash_code(1),
            move |pos| {
                log.borrow_mut().push(pos);
                7
            },
        ),
        level: Level::default(),
        time: 0,
        frozen: false,
    };
    let positions = [
        BlockPos::new(5, 64, 5),
        BlockPos::new(3, 64, 9),
        BlockPos::new(4, 64, 5),
    ];
    for pos in positions {
        sim.place_cable(pos, 8_000, None);
    }
    assert_eq!(*asked.borrow(), positions.to_vec());
}

// As ordens do `UniversalCable.pullFromAcceptors`.

fn pull_setup(sim: &mut Sim, cable: BlockPos, sides: &[Face]) {
    for side in sides {
        for _ in 0..2 {
            sneak(sim, cable, Face::North, Some(*side));
        }
        assert_eq!(
            sim.owner.view_at(&sim.level, cable).unwrap().raw[*side as usize],
            ConnectionType::Pull
        );
    }
}

fn giver(sim: &mut Sim, cable: BlockPos, side: Face, stored: i64) -> Rc<Machine> {
    sim.place_machine(cable.relative(side), 1_000_000, stored, true)
}

#[test]
fn a_cable_pulls_only_from_the_sides_in_pull_and_only_what_it_can_keep() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    sim.place_cable(cable, 8_000, None);
    let up = giver(&mut sim, cable, Face::Up, 100_000);
    let down = giver(&mut sim, cable, Face::Down, 5_000);
    pull_setup(&mut sim, cable, &[Face::Up]);
    sim.send(Event::Update(cable));
    assert_eq!(
        down.container.amount(),
        5_000,
        "o lado `NORMAL` não é puxado"
    );
    assert!(down.extracts.borrow().is_empty());
    // A simulação pede, e só o que o cabo guarda sai de verdade, uma vez.
    assert_eq!(up.container.amount(), 100_000 - 8_000);
    assert_eq!(sim.buffer(cable).unwrap().amount, 8_000);
    assert_eq!(
        up.extracts
            .borrow()
            .iter()
            .filter(|(_, simulate)| !simulate)
            .count(),
        1,
        "uma retirada de verdade só"
    );
}

#[test]
fn a_cable_stops_pulling_when_it_is_full() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    sim.place_cable(cable, 8_000, None);
    // Os lados na ordem de `Direction`: baixo, cima, norte.
    let down = giver(&mut sim, cable, Face::Down, 5_000);
    let up = giver(&mut sim, cable, Face::Up, 5_000);
    let north = giver(&mut sim, cable, Face::North, 5_000);
    pull_setup(&mut sim, cable, &[Face::Down, Face::Up, Face::North]);
    sim.send(Event::Update(cable));
    assert_eq!(down.container.amount(), 0);
    assert_eq!(up.container.amount(), 2_000, "o segundo dá o que falta");
    assert_eq!(
        north.container.amount(),
        5_000,
        "cheio, o terceiro nem é perguntado"
    );
    assert_eq!(sim.buffer(cable).unwrap().amount, 8_000);
}

#[test]
fn a_cable_without_a_network_pulls_into_its_own_buffer() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    sim.place_cable(cable, 8_000, None);
    let source = giver(&mut sim, cable, Face::Up, 3_000);
    pull_setup(&mut sim, cable, &[Face::Up]);
    assert!(!sim.owner.view_at(&sim.level, cable).unwrap().has_network);
    sim.send(Event::Update(cable));
    assert_eq!(
        sim.buffer(cable).unwrap().amount,
        3_000,
        "o próprio buffer recebe do que puxa"
    );
    assert_eq!(source.container.amount(), 0);
}

/// O vizinho que erra a simulação (devolve mais do que o pedido): o cabo só confirma se guardar tudo o que o vizinho disse, e então não tira nada dele.
#[test]
fn a_neighbor_that_overstates_what_it_gives_loses_nothing() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    sim.place_cable(cable, 8_000, None);
    assert_eq!(sim.owner.insert_external(cable, 7_800), 7_800);
    let machine = Rc::new(Machine {
        container: {
            let container = BufferEnergyContainer::new(1_000_000, always, always, None);
            container.set_energy(1_000, None);
            container
        },
        gives: true,
        overdraw: 500,
        extracts: Default::default(),
    });
    sim.level
        .blocks
        .insert(cable.relative(Face::Up), Block::Machine(machine.clone()));
    pull_setup(&mut sim, cable, &[Face::Up]);
    sim.send(Event::Update(cable));
    assert_eq!(machine.container.amount(), 1_000, "nada saiu do vizinho");
    assert_eq!(sim.buffer(cable).unwrap().amount, 7_800);
    assert!(
        machine
            .extracts
            .borrow()
            .iter()
            .all(|(_, simulate)| *simulate),
        "só a simulação foi pedida"
    );
}

// Os avisos que o dono dá ao mundo.

#[test]
fn the_configurator_tells_the_world_the_side_and_the_block_that_changed() {
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    sim.level.changed.clear();
    sneak(&mut sim, cables[1], Face::North, Some(Face::East));
    assert_eq!(
        sim.level.side_changes,
        vec![(
            cables[1],
            Face::East,
            ConnectionType::Normal,
            ConnectionType::Push
        )]
    );
    assert!(
        sim.level.changed.contains(&cables[1]),
        "`setChanged` do cabo"
    );
}

#[test]
fn a_reactive_cable_tells_the_world_when_the_signal_comes_and_goes_and_shows_it() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    sim.place_cable(cable, 8_000, None);
    sim.tick();
    assert!(sim.owner.right_click(&mut sim.level, cable));
    sim.level.powered.push(BlockPos::new(10, 65, 10));
    sim.send(Event::Neighbor(cable));
    assert_eq!(sim.level.redstone_changes, vec![(cable, true)]);
    assert!(
        last_published(&sim, cable).redstone_activated,
        "o bloco mostra o cabo ativado"
    );
    sim.level.powered.clear();
    sim.send(Event::Neighbor(cable));
    assert_eq!(
        sim.level.redstone_changes,
        vec![(cable, true), (cable, false)]
    );
    assert!(!last_published(&sim, cable).redstone_activated);
}

/// `refreshConnections` num vizinho que está num chunk sem carregar marca o `forceUpdate`: quando o chunk carrega, o ticker do block entity relê tudo e o cabo liga.
#[test]
fn a_cable_next_to_a_machine_in_an_unloaded_chunk_connects_when_the_chunk_loads() {
    let mut sim = Sim::free();
    let cable = BlockPos::new(10, 64, 10);
    let machine_at = cable.relative(Face::Up);
    sim.level.unloaded.push(machine_at);
    sim.place_cable(cable, 8_000, None);
    sim.place_machine(machine_at, 100_000, 0, false);
    sim.tick();
    assert!(!sim.owner.view_at(&sim.level, cable).unwrap().connected[Face::Up as usize]);
    sim.level.unloaded.clear();
    // O chunk que carrega invalida as capabilities dele (`ServerLevel.invalidateCapabilities(ChunkPos)`).
    sim.send(Event::Capability(cable, Face::Up));
    sim.tick();
    assert!(
        sim.owner.view_at(&sim.level, cable).unwrap().connected[Face::Up as usize],
        "o ticker relê as conexões quando o chunk carrega"
    );
}

/// `recheckConnections`: um cabo sem rede que liga a um transmissor vizinho faz o vizinho reler o lado de volta, sem esperar o aviso nem o ticker dele.
#[test]
fn a_cable_that_connects_makes_the_new_neighbor_read_its_side_back() {
    let mut sim = Sim::free();
    let a = BlockPos::new(10, 64, 10);
    let b = BlockPos::new(11, 64, 10);
    for pos in [a, b] {
        sim.level.blocks.insert(
            pos,
            Block::Cable {
                capacity: 8_000,
                saved: None,
            },
        );
        sim.send(Event::ClearRemoved(pos));
    }
    // Só o ticker de `a` anda: ele relê as conexões (o `forceUpdate` do cabo novo), liga a `b` e pede a `b` que releia o lado dele.
    sim.send(Event::Update(a));
    assert!(sim.owner.view_at(&sim.level, a).unwrap().connected[Face::East as usize]);
    assert!(
        sim.owner.view_at(&sim.level, b).unwrap().connected[Face::West as usize],
        "`b` releu o lado de volta sem ter andado"
    );
}

/// O clique que corta o braço de um cabo invalida a rede (`InvalidateTransmitter`): as duas pontas viram duas redes, e o braço de volta as junta.
#[test]
fn cutting_the_middle_arm_with_the_configurator_splits_the_network_and_restoring_it_joins_it() {
    let mut sim = Sim::free();
    let cables = line_at(&mut sim, BlockPos::new(10, 64, 10), 3);
    assert_eq!(sim.owner.registry().networks_in_order().len(), 1);
    for _ in 0..3 {
        sneak(&mut sim, cables[1], Face::North, Some(Face::East));
    }
    assert_eq!(
        sim.owner.view_at(&sim.level, cables[1]).unwrap().raw[Face::East as usize],
        ConnectionType::None
    );
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(
        sim.owner.registry().networks_in_order().len(),
        2,
        "duas redes"
    );
    assert_ne!(
        sim.owner.network_of(cables[0]),
        sim.owner.network_of(cables[2])
    );
    sneak(&mut sim, cables[1], Face::North, Some(Face::East));
    for _ in 0..3 {
        sim.tick();
    }
    assert_eq!(
        sim.owner.registry().networks_in_order().len(),
        1,
        "uma rede de novo"
    );
}

// `TileEntityTransmitter.clearRemoved` e o que o jogo dá do bloco.

#[test]
fn a_cable_remembers_the_energy_the_save_gave_as_an_int_or_a_long() {
    for (tag, expected) in [(Tag::Int(1_500), 1_500), (Tag::Long(2_500), 2_500)] {
        let mut sim = Sim::free();
        let pos = BlockPos::new(10, 64, 10);
        let mut saved = Compound::new();
        saved.insert(ENERGY_KEY.to_owned(), tag);
        sim.place_cable(pos, 8_000, Some(saved));
        assert_eq!(sim.buffer(pos).unwrap().amount, expected);
        assert_eq!(
            sim.buffer(pos).unwrap().capacity,
            8_000,
            "a capacidade do tier"
        );
        for _ in 0..3 {
            sim.tick();
        }
        assert_eq!(
            sim.buffer(pos).unwrap().amount,
            expected,
            "a rede guarda o que o cabo trazia"
        );
    }
}

#[test]
fn a_clear_removed_where_the_block_is_not_a_cable_makes_no_cable() {
    let mut sim = Sim::free();
    let pos = BlockPos::new(10, 64, 10);
    sim.place_machine(pos, 1_000, 0, false);
    sim.send(Event::ClearRemoved(pos));
    assert_eq!(sim.owner.cable_at(pos), None);
    assert!(sim.owner.positions().is_empty());
    sim.send(Event::Update(pos));
    sim.send(Event::ServerTickPost {
        runs_normally: true,
    });
    assert!(sim.owner.registry().networks_in_order().is_empty());
}

/// Antes de o registro dar uma rede ao cabo, a capability dele não deixa entrar nada de fora (o contêiner que aceita é o da rede), mesmo com braços ligados; e o cabo ativado por
/// redstone não tem capability em lado nenhum.
#[test]
fn a_cable_with_arms_but_no_network_yet_and_a_powered_cable_take_nothing_from_outside() {
    let mut sim = Sim::free();
    let a = BlockPos::new(10, 64, 10);
    let b = BlockPos::new(11, 64, 10);
    sim.place_cable(a, 8_000, None);
    sim.place_cable(b, 8_000, None);
    sim.send(Event::PlacedByItem(b));
    let view = sim.owner.view_at(&sim.level, b).unwrap();
    assert!(view.connected[Face::West as usize], "o braço está ligado");
    assert!(!view.has_network, "mas ainda não há rede");
    assert!(view.insert.iter().all(|inserts| !inserts));
    // Com rede, o braço deixa entrar.
    for _ in 0..3 {
        sim.tick();
    }
    assert!(sim.owner.view_at(&sim.level, b).unwrap().insert[Face::West as usize]);
    // O redstone reativo com sinal: o cabo solta tudo e a capability some de todo lado.
    assert!(sim.owner.right_click(&mut sim.level, b));
    sim.level.powered.push(BlockPos::new(11, 65, 10));
    sim.send(Event::Neighbor(b));
    let view = sim.owner.view_at(&sim.level, b).unwrap();
    assert!(view.redstone_activated);
    assert!(
        view.exists.iter().all(|exists| !exists),
        "ativado, nenhum lado tem capability"
    );
    assert!(view.insert.iter().all(|inserts| !inserts));
}

/// `loadAdditional`: o que o bloco guardou das conexões (o tipo cru de cada lado e o redstone reativo) volta com o cabo.
#[test]
fn a_cable_remembers_its_connection_types_from_the_save() {
    let mut sim = Sim::free();
    let cables = line(&mut sim, BlockPos::new(10, 64, 10), 3);
    // `NORMAL`, `PUSH`, `PULL` na face de cima do cabo do meio, e o redstone reativo ligado.
    for _ in 0..2 {
        sneak(&mut sim, cables[1], Face::North, Some(Face::Up));
    }
    assert!(sim.owner.right_click(&mut sim.level, cables[1]));
    let saved = sim.owner.write(cables[1]).unwrap();
    let mut other = Sim::free();
    let again = BlockPos::new(40, 64, 40);
    other.place_cable(again, 8_000, Some(saved));
    let view = other.owner.view_at(&other.level, again).unwrap();
    assert_eq!(view.raw[Face::Up as usize], ConnectionType::Pull);
    assert!(view.redstone_reactive);
    assert_eq!(view.raw[Face::East as usize], ConnectionType::Normal);
}
