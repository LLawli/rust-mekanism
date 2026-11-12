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
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }

    fn extract(&self, amount: i32, simulate: bool) -> i32 {
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
        got
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
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        !self.unloaded.contains(&pos)
    }

    fn is_getting_powered(&self, pos: BlockPos) -> bool {
        Face::ALL
            .iter()
            .any(|side| self.powered.contains(&pos.relative(*side)))
    }

    fn energy_handler(&self, pos: BlockPos, _side: Face) -> Option<Acceptor> {
        match self.blocks.get(&pos)? {
            Block::Machine(machine) => {
                let machine: Acceptor = machine.clone();
                Some(machine)
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
        let machine = Rc::new(Machine { container, gives });
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
