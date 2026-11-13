//! Os eventos do jogo entregues no fim da operação (o port, decisão 107) contra os entregues na hora, de dentro da chamada que os causa (o original):
//! o mesmo dono (`EnergyTransmitters`), a mesma lista de eventos na mesma ordem, e o mundo mudando entre eles (original) ou já mudado por inteiro (port).
//!
//! Uma operação é uma lista de mudanças de bloco (um `fill`, um `clone`, uma explosão, um comando de vários `setblock`). No original cada mudança
//! chama os eventos dela antes de a próxima acontecer; no port a fila só esvazia no fim da operação (`transmitters_flush_operation`), com o mundo como
//! ficou no fim. Os eventos são gerados pelas mesmas regras do `Tracker` (`transmitter_events.rs`): o que sai de um bloco posto, quebrado ou trocado,
//! na ordem `Removed`, as capabilities do que saiu, `ClearRemoved`, as do que entrou e os avisos dos vizinhos.
//!
//! O teste compara, depois de cada tick, tudo o que o dono mostra: a vista de cada cabo, a rede dele, a energia de cada rede e de cada máquina, e a
//! última vista que cada cabo publicou. Um cenário em que as duas execuções diferem aparece com o nome da semente.
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::transmitter::{Compound, Overlay};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

const DIMENSION: DimensionId = DimensionId(0);
const CABLE_CAPACITY: i64 = 8_000;

struct Machine {
    container: BufferEnergyContainer,
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }
}

#[derive(Clone)]
enum Block {
    Cable,
    Machine(Rc<Machine>),
}

#[derive(Default)]
struct Level {
    blocks: HashMap<BlockPos, Block>,
    unloaded: HashSet<BlockPos>,
    /// Os blocos que dão sinal de redstone (um bloco de redstone: sem block entity).
    powered: HashSet<BlockPos>,
    published: Vec<(BlockPos, CableView)>,
    changed: Vec<BlockPos>,
    side_changes: Vec<(BlockPos, Face, ConnectionType, ConnectionType)>,
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
        if self.unloaded.contains(&pos) {
            return None;
        }
        match self.blocks.get(&pos)? {
            Block::Machine(machine) => {
                let machine: Acceptor = machine.clone();
                Some(machine)
            }
            Block::Cable => None,
        }
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        match self.blocks.get(&pos)? {
            Block::Cable => Some(CableInfo {
                capacity: CABLE_CAPACITY,
                saved: None::<Compound>,
            }),
            Block::Machine(_) => None,
        }
    }

    fn publish(&mut self, pos: BlockPos, view: &CableView) {
        self.published.push((pos, view.clone()));
    }

    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {}

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
}

/// Uma mudança de bloco de uma operação.
#[derive(Clone, Copy, Debug)]
enum Change {
    Cable(BlockPos),
    Machine(BlockPos, i64),
    Air(BlockPos),
    /// Um bloco de redstone entra (sem block entity: só avisa os vizinhos).
    Power(BlockPos),
    /// O bloco de redstone sai.
    Unpower(BlockPos),
}

impl Change {
    fn pos(self) -> BlockPos {
        match self {
            Change::Cable(pos)
            | Change::Machine(pos, _)
            | Change::Air(pos)
            | Change::Power(pos)
            | Change::Unpower(pos) => pos,
        }
    }
}

struct Sim {
    owner: EnergyTransmitters,
    level: Level,
    time: i64,
}

impl Sim {
    fn new() -> Self {
        let mut uuid = 0i32;
        Self {
            owner: EnergyTransmitters::new(
                DIMENSION,
                move || {
                    uuid += 1;
                    Uuid::with_hash_code(uuid.wrapping_mul(0x1234_5679))
                },
                // O `identityHashCode` do block entity é sorteado pela JVM: aqui sai da posição, para as duas execuções darem o mesmo a cada cabo
                // (um contador mudaria de lugar se uma delas criasse um cabo a menos).
                move |pos| {
                    (pos.x
                        .wrapping_mul(31)
                        .wrapping_add(pos.z)
                        .wrapping_mul(17)
                        .wrapping_add(pos.y))
                    .wrapping_mul(0x2545_F491)
                },
            ),
            level: Level::default(),
            time: 0,
        }
    }

    fn apply(&mut self, change: Change) {
        match change {
            Change::Cable(pos) => {
                self.level.blocks.insert(pos, Block::Cable);
            }
            Change::Machine(pos, stored) => {
                let container = BufferEnergyContainer::new(1_000_000, always, always, None);
                container.set_energy(stored, None);
                self.level
                    .blocks
                    .insert(pos, Block::Machine(Rc::new(Machine { container })));
            }
            Change::Air(pos) => {
                self.level.blocks.remove(&pos);
            }
            Change::Power(pos) => {
                self.level.powered.insert(pos);
            }
            Change::Unpower(pos) => {
                self.level.powered.remove(&pos);
            }
        }
    }

    fn send(&mut self, event: Event) {
        self.owner.handle(&mut self.level, event);
    }

    /// Um tick do servidor: os block entities andam (`onUpdateServer`), `before_post` roda no fim do tick (o `ServerTickEnd` do port esvazia a fila) e o registro fecha o tick.
    fn tick_with(&mut self, before_post: impl FnOnce(&mut Sim)) {
        for pos in self.owner.positions() {
            self.send(Event::Update(pos));
        }
        before_post(self);
        self.owner.set_game_time(self.time);
        self.send(Event::ServerTickPost {
            runs_normally: true,
        });
        self.time += 1;
    }

    /// Como `fingerprint`, com o id de cada rede trocado pela ordem em que aparece: o id é o contador do registro, e uma rede que o original desfez e refez tem outro.
    fn canonical(&self) -> String {
        let text = self.fingerprint();
        let mut seen: Vec<String> = Vec::new();
        let mut out = String::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("NetworkId(") {
            out += &rest[..at];
            let tail = &rest[at + "NetworkId(".len()..];
            let end = tail.find(')').unwrap();
            let id = tail[..end].to_owned();
            let index = seen
                .iter()
                .position(|known| *known == id)
                .unwrap_or_else(|| {
                    seen.push(id);
                    seen.len() - 1
                });
            out += &format!("Net#{index}");
            rest = &tail[end + 1..];
        }
        out + rest
    }

    /// Tudo o que o dono e o jogo mostram, em texto, para comparar duas execuções.
    fn fingerprint(&self) -> String {
        let mut out = String::new();
        let mut machines: BTreeMap<(i32, i32, i32), i64> = BTreeMap::new();
        for (pos, block) in &self.level.blocks {
            if let Block::Machine(machine) = block {
                machines.insert((pos.x, pos.y, pos.z), machine.container.amount());
            }
        }
        out += &format!("machines {machines:?}\n");
        let mut networks = Vec::new();
        for pos in sorted(self.owner.positions()) {
            out += &format!(
                "cable {pos:?} net {:?} buffer {:?}\n",
                self.owner.network_of(pos),
                self.owner.buffer_at(&self.level, pos)
            );
            if let Some(network) = self.owner.network_of(pos)
                && !networks.contains(&network)
            {
                networks.push(network);
                let net = self.owner.family().network(network).unwrap();
                out += &format!(
                    "network {network:?} {} {} {}\n",
                    net.buffer(),
                    net.capacity(),
                    net.prev_transfer_amount()
                );
            }
        }
        let mut last: BTreeMap<(i32, i32, i32), &CableView> = BTreeMap::new();
        for (pos, view) in &self.level.published {
            // Só o que um cabo que ainda existe mostra conta: o que um cabo já quebrado publicou foi o que o bloco levou consigo.
            if self.owner.cable_at(*pos).is_some() {
                last.insert((pos.x, pos.y, pos.z), view);
            }
        }
        out += &format!("published {last:?}\n");
        out
    }
}

fn sorted(mut list: Vec<BlockPos>) -> Vec<BlockPos> {
    list.sort_by_key(|pos| (pos.x, pos.y, pos.z));
    list
}

/// O que o `Tracker` do jogo faz com uma mudança, na ordem do original (`Removed`, as capabilities do que saiu, `ClearRemoved`, as do que entrou), e o que ele sabe
/// depois: os cabos seguidos e as posições com block entity.
#[derive(Default, Clone)]
struct Shadow {
    blocks: HashMap<BlockPos, bool>,
    tracked: HashSet<BlockPos>,
    powered: HashSet<BlockPos>,
}

impl Shadow {
    fn capabilities_at(&self, pos: BlockPos, out: &mut Vec<Event>) {
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.tracked.contains(&neighbor) {
                out.push(Event::Capability(neighbor, side.opposite()));
            }
        }
    }

    /// Se a mudança troca o bloco por outro (o `setBlock` de um estado igual não faz nada, nem eventos).
    fn changes(&self, change: Change) -> bool {
        let new = match change {
            Change::Cable(_) => Some(true),
            Change::Machine(..) => Some(false),
            Change::Air(_) => None,
            Change::Power(pos) => return !self.powered.contains(&pos),
            Change::Unpower(pos) => return self.powered.contains(&pos),
        };
        self.blocks.get(&change.pos()).copied() != new
    }

    /// Os eventos do block entity que sai e do que entra (`LevelChunk.setBlockState`), sem os avisos de vizinho. `true` é um cabo, `false` uma máquina (as duas têm block entity).
    fn others(&mut self, change: Change) -> Vec<Event> {
        let pos = change.pos();
        let mut out = Vec::new();
        match change {
            Change::Power(pos) => {
                self.powered.insert(pos);
                return out;
            }
            Change::Unpower(pos) => {
                self.powered.remove(&pos);
                return out;
            }
            _ => {}
        }
        if let Some(old) = self.blocks.remove(&pos) {
            if old {
                self.tracked.remove(&pos);
                out.push(Event::Removed(pos));
            }
            self.capabilities_at(pos, &mut out);
        }
        match change {
            Change::Cable(_) => {
                self.blocks.insert(pos, true);
                self.tracked.insert(pos);
                out.push(Event::ClearRemoved(pos));
                self.capabilities_at(pos, &mut out);
            }
            Change::Machine(..) => {
                self.blocks.insert(pos, false);
                self.capabilities_at(pos, &mut out);
            }
            Change::Air(_) | Change::Power(_) | Change::Unpower(_) => {}
        }
        out
    }

    /// `BlockTransmitter.neighborChanged` dos cabos ao redor de `pos`, na ordem dos lados, com o que é seguido agora.
    fn neighbors(&self, pos: BlockPos) -> Vec<Event> {
        Face::ALL
            .iter()
            .map(|side| pos.relative(*side))
            .filter(|neighbor| self.tracked.contains(neighbor))
            .map(Event::Neighbor)
            .collect()
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self, below: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % below
    }
}

/// Uma operação e um tick depois dela, repetidos.
#[derive(Clone, Debug)]
enum Step {
    Op(Vec<Change>),
    Ticks(u32),
    Push(BlockPos, i32),
    /// O clique direito do Configurator num cabo: liga ou desliga o redstone reativo.
    Reactive(BlockPos),
}

const SIZE: u64 = 4;

fn random_pos(rng: &mut Rng) -> BlockPos {
    BlockPos::new(rng.next(SIZE) as i32, 0, rng.next(SIZE) as i32)
}

fn random_change(rng: &mut Rng, pos: BlockPos) -> Change {
    match rng.next(12) {
        0..=4 => Change::Cable(pos),
        5..=6 => Change::Machine(pos, rng.next(2_000) as i64),
        7..=8 => Change::Air(pos),
        9..=10 => Change::Power(pos),
        _ => Change::Unpower(pos),
    }
}

fn scenario(seed: u64, repeat_positions: bool) -> Vec<Step> {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut steps = Vec::new();
    for _ in 0..8 {
        let count = 1 + rng.next(8);
        let mut taken = HashSet::new();
        let mut op = Vec::new();
        for _ in 0..count {
            let pos = random_pos(&mut rng);
            if !repeat_positions && !taken.insert((pos.x, pos.z)) {
                continue;
            }
            op.push(random_change(&mut rng, pos));
        }
        steps.push(Step::Op(op));
        if rng.next(2) == 0 {
            steps.push(Step::Push(
                random_pos(&mut rng),
                1_000 + rng.next(6_000) as i32,
            ));
        }
        if rng.next(3) == 0 {
            steps.push(Step::Reactive(random_pos(&mut rng)));
        }
        steps.push(Step::Ticks(rng.next(4) as u32));
    }
    steps.push(Step::Ticks(3));
    steps
}

/// Como os eventos chegam ao dono.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// O original de um `setBlock` por vez (uma explosão, uma fila de comandos `/setblock`): cada mudança chama os eventos dela, os avisos de vizinho no fim, antes da seguinte.
    Immediate,
    /// `Immediate` com cada invalidação de capability entregue três vezes: o `BlockCapabilityCache` do original só avisa de novo depois de um `getCapability`, e o port avisa a cada invalidação.
    ImmediateRepeated,
    /// O original de `/fill' e `/clone`: a primeira passada muda os blocos (só os block entities avisam) e a segunda avisa os vizinhos de cada posição, com tudo no lugar.
    TwoPass,
    /// A operação inteira muda o mundo e depois os eventos de `Immediate` saem todos.
    Batched,
    /// Como `Batched`, na ordem contrária (a prova de que o teste vê uma ordem errada).
    BatchedReversed,
    /// O port: cada mudança é uma operação do Horizon (o block entity que sai antes do `setBlock`, o aviso dos vizinhos dentro dele, o block entity que entra depois), a fila esvazia
    /// no fim de cada `with_level` (`transmitters_flush_operation`) com os avisos da operação anterior, e o resto nos pontos do tick.
    Port,
    /// O port com a injeção que esta frente pesou e não pôs: um HEAD cancelável em `Game::place_state` que esvazia a fila inteira (os avisos segurados também) no fim de cada bloco posto
    /// por comando, em vez de esperar a operação seguinte ou o fim do tick.
    PortEager,
}

/// O que as injeções anotam (`Raw` do `transmitter_events.rs`): `true` é um cabo.
#[derive(Clone, Debug)]
enum Raw {
    Removed(BlockPos, bool),
    Added(BlockPos, bool),
    Neighbor(BlockPos),
}

/// O `Tracker::apply` do jogo sobre as ocorrências.
#[derive(Default)]
struct PortTracker {
    present: HashSet<BlockPos>,
    tracked: HashSet<BlockPos>,
}

impl PortTracker {
    fn capabilities_at(&self, pos: BlockPos, out: &mut Vec<Event>) {
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.tracked.contains(&neighbor) {
                out.push(Event::Capability(neighbor, side.opposite()));
            }
        }
    }

    fn translate(&mut self, raws: Vec<Raw>) -> Vec<Event> {
        let mut out = Vec::new();
        for raw in raws {
            match raw {
                Raw::Added(pos, cable) => {
                    if !self.present.insert(pos) {
                        continue;
                    }
                    if cable {
                        self.tracked.insert(pos);
                        out.push(Event::ClearRemoved(pos));
                    }
                    self.capabilities_at(pos, &mut out);
                }
                Raw::Removed(pos, cable) => {
                    if !self.present.remove(&pos) {
                        continue;
                    }
                    if cable {
                        self.tracked.remove(&pos);
                        out.push(Event::Removed(pos));
                    }
                    self.capabilities_at(pos, &mut out);
                }
                Raw::Neighbor(pos) => {
                    if self.tracked.contains(&pos) {
                        out.push(Event::Neighbor(pos));
                    }
                }
            }
        }
        out
    }
}

/// A fila de ocorrências do port: o que as injeções anotaram e o que o fim da operação anterior segurou.
#[derive(Default)]
struct PortQueue {
    queue: Vec<Raw>,
    held: Vec<Raw>,
    tracker: PortTracker,
}

impl PortQueue {
    /// `transmitters_flush`: os pontos do tick (começo e fim), tudo.
    fn flush(&mut self, sim: &mut Sim) {
        let raws = std::mem::take(&mut self.queue);
        let (mut now, neighbors): (Vec<Raw>, Vec<Raw>) = raws
            .into_iter()
            .partition(|raw| !matches!(raw, Raw::Neighbor(_)));
        now.extend(std::mem::take(&mut self.held));
        now.extend(neighbors);
        for event in self.tracker.translate(now) {
            sim.send(event);
        }
    }

    /// `transmitters_flush_operation`: o fim de um `with_level`.
    fn flush_operation(&mut self, sim: &mut Sim) {
        let raws = std::mem::take(&mut self.queue);
        let (mut others, neighbors): (Vec<Raw>, Vec<Raw>) = raws
            .into_iter()
            .partition(|raw| !matches!(raw, Raw::Neighbor(_)));
        if others.is_empty() {
            self.held.extend(neighbors);
            return;
        }
        others.extend(std::mem::take(&mut self.held));
        self.held.extend(neighbors);
        for event in self.tracker.translate(others) {
            sim.send(event);
        }
    }

    /// `Commands.place_state` do Horizon: o block entity antigo sai, `level_set` muda o bloco e avisa os vizinhos, o block entity novo entra.
    fn place(&mut self, sim: &mut Sim, change: Change, changes: bool) {
        if !changes {
            return;
        }
        let pos = change.pos();
        if matches!(change, Change::Power(_) | Change::Unpower(_)) {
            // Um bloco sem block entity: só o `level_set` e o aviso dos vizinhos.
            sim.apply(change);
            for side in Face::ALL {
                self.queue.push(Raw::Neighbor(pos.relative(side)));
            }
            self.flush_operation(sim);
            return;
        }
        match sim.level.blocks.get(&pos) {
            Some(Block::Cable) => self.queue.push(Raw::Removed(pos, true)),
            Some(Block::Machine(_)) => self.queue.push(Raw::Removed(pos, false)),
            None => {}
        }
        // `level_set`: o bloco muda, os vizinhos são avisados (todos os seis, o filtro é na tradução) e a operação termina.
        match change {
            Change::Cable(_) => {
                sim.level.blocks.insert(pos, Block::Cable);
            }
            Change::Machine(..) | Change::Air(_) => {
                sim.level.blocks.remove(&pos);
            }
            Change::Power(_) | Change::Unpower(_) => unreachable!(),
        }
        for side in Face::ALL {
            self.queue.push(Raw::Neighbor(pos.relative(side)));
        }
        self.flush_operation(sim);
        // O block entity novo entra depois da operação.
        match change {
            Change::Cable(_) => self.queue.push(Raw::Added(pos, true)),
            Change::Machine(..) => {
                sim.apply(change);
                self.queue.push(Raw::Added(pos, false));
            }
            Change::Air(_) | Change::Power(_) | Change::Unpower(_) => {}
        }
    }
}

/// Roda o cenário e devolve a impressão do estado depois de cada tick.
fn run_mode(steps: &[Step], mode: Mode) -> Vec<String> {
    run_mode_in(steps, mode, canonical_ids())
}

/// `canonical`: sem o id das redes na impressão.
fn run_mode_in(steps: &[Step], mode: Mode, canonical: bool) -> Vec<String> {
    let mut sim = Sim::new();
    let mut shadow = Shadow::default();
    let mut port = PortQueue::default();
    let mut prints = Vec::new();
    for step in steps {
        match step {
            Step::Op(op) if matches!(mode, Mode::Port | Mode::PortEager) => {
                for change in op {
                    let changes = shadow.changes(*change);
                    shadow.others(*change);
                    port.place(&mut sim, *change, changes);
                    if mode == Mode::PortEager {
                        port.flush(&mut sim);
                    }
                }
            }
            Step::Op(op) => {
                let op: Vec<Change> = op
                    .iter()
                    .copied()
                    .filter(|change| shadow.changes(*change))
                    .collect();
                match mode {
                    Mode::Immediate | Mode::ImmediateRepeated => {
                        for change in &op {
                            let others = shadow.others(*change);
                            let neighbors = shadow.neighbors(change.pos());
                            sim.apply(*change);
                            for event in others.into_iter().chain(neighbors) {
                                let times = match event {
                                    Event::Capability(..) if mode == Mode::ImmediateRepeated => 3,
                                    _ => 1,
                                };
                                for _ in 0..times {
                                    sim.send(event.clone());
                                }
                            }
                        }
                    }
                    Mode::TwoPass => {
                        for change in &op {
                            let others = shadow.others(*change);
                            sim.apply(*change);
                            for event in others {
                                sim.send(event);
                            }
                        }
                        for change in &op {
                            for event in shadow.neighbors(change.pos()) {
                                sim.send(event);
                            }
                        }
                    }
                    Mode::Batched | Mode::BatchedReversed => {
                        let planned: Vec<Vec<Event>> = op
                            .iter()
                            .map(|change| {
                                let others = shadow.others(*change);
                                let neighbors = shadow.neighbors(change.pos());
                                others.into_iter().chain(neighbors).collect()
                            })
                            .collect();
                        for change in &op {
                            sim.apply(*change);
                        }
                        let mut all: Vec<Event> = planned.into_iter().flatten().collect();
                        if mode == Mode::BatchedReversed {
                            all.reverse();
                        }
                        for event in all {
                            sim.send(event);
                        }
                    }
                    Mode::Port | Mode::PortEager => unreachable!(),
                }
            }
            Step::Ticks(count) => {
                for _ in 0..*count {
                    let port_like = matches!(mode, Mode::Port | Mode::PortEager);
                    if port_like {
                        port.flush(&mut sim);
                    }
                    sim.tick_with(|sim| {
                        if port_like {
                            port.flush(sim);
                        }
                    });
                    prints.push(if canonical {
                        sim.canonical()
                    } else {
                        sim.fingerprint()
                    });
                }
            }
            Step::Reactive(pos) => {
                // Um clique do jogador: o que a fila tem já saiu.
                if matches!(mode, Mode::Port | Mode::PortEager) {
                    port.flush(&mut sim);
                }
                let Sim { owner, level, .. } = &mut sim;
                owner.right_click(level, *pos);
            }
            Step::Push(pos, amount) => {
                // Quem empurra energia para um cabo é um ticker (uma máquina, um cubo), que anda depois da fila do começo do tick.
                if matches!(mode, Mode::Port | Mode::PortEager) {
                    port.flush(&mut sim);
                }
                sim.owner.insert_external(*pos, *amount);
            }
        }
    }
    prints
}

fn first_difference(a: &[String], b: &[String]) -> Option<usize> {
    if a.len() != b.len() {
        return Some(a.len().min(b.len()));
    }
    (0..a.len()).find(|&i| a[i] != b[i])
}

fn differing(from: Mode, to: Mode, repeat: bool, seeds: u64) -> Vec<u64> {
    (0..seeds)
        .filter(|seed| {
            let steps = scenario(*seed, repeat);
            first_difference(&run_mode(&steps, from), &run_mode(&steps, to)).is_some()
        })
        .collect()
}

/// Os cenários aleatórios comparam sem o id das redes (o do original muda quando um cabo reativo pisca, o caso de `a_redstone_block_that_changes_sides_in_one_operation_only_changes_the_id_of_the_network`);
/// `RAW_IDS=1` compara com ele.
fn canonical_ids() -> bool {
    std::env::var("RAW_IDS").is_err()
}

fn seeds() -> u64 {
    std::env::var("SEEDS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(600)
}

#[test]
fn delivering_the_events_at_the_end_of_the_operation_changes_nothing_the_owner_shows() {
    let different = differing(Mode::Immediate, Mode::Batched, false, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
}

#[test]
fn the_same_scenarios_with_a_position_changed_twice_in_one_operation_also_match() {
    let different = differing(Mode::Immediate, Mode::Batched, true, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
}

#[test]
fn the_comparison_sees_a_wrong_order_of_events() {
    let caught = differing(Mode::Immediate, Mode::BatchedReversed, false, 600).len();
    assert!(
        caught >= 50,
        "a ordem errada só apareceu em {caught} de 600 cenários"
    );
}

/// O `BlockCapabilityCache` do original só avisa de novo depois de um `getCapability`, e o port repete a entrega a cada invalidação: o aviso é idempotente (`markDirtyAcceptor`), então
/// entregar três vezes o que o original entrega uma não muda o que o dono mostra.
#[test]
fn a_capability_told_again_changes_nothing() {
    let different = differing(Mode::Immediate, Mode::ImmediateRepeated, true, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
}

/// O original de `/fill` e de `/clone` avisa os vizinhos numa segunda passada; o Horizon muda e avisa bloco a bloco, e o port entrega cada aviso na operação seguinte.
#[test]
fn the_port_order_of_a_fill_gives_the_state_the_two_pass_original_gives() {
    let different = differing(Mode::TwoPass, Mode::Port, false, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
}

#[test]
fn the_port_order_of_one_set_block_after_another_gives_the_state_the_original_gives() {
    let different = differing(Mode::Immediate, Mode::Port, false, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
}

/// A injeção que esvaziaria a fila no fim de cada `place_state` não muda o que o dono mostra (nem o que o port já entrega): fica sem ela.
#[test]
fn flushing_the_queue_at_the_end_of_each_placed_block_would_change_nothing() {
    let different = differing(Mode::Port, Mode::PortEager, false, seeds());
    assert!(
        different.is_empty(),
        "{} cenários diferem: {:?}",
        different.len(),
        &different[..different.len().min(12)]
    );
    let against_the_original = differing(Mode::TwoPass, Mode::PortEager, false, seeds());
    assert!(against_the_original.is_empty());
}

/// Reduz um cenário que difere ao menor que ainda difere (tira um passo ou uma mudança de cada vez).
fn shrink(steps: Vec<Step>, from: Mode, to: Mode) -> Vec<Step> {
    let differs =
        |steps: &[Step]| first_difference(&run_mode(steps, from), &run_mode(steps, to)).is_some();
    let mut steps = steps;
    loop {
        let mut shrunk = false;
        for i in 0..steps.len() {
            let mut candidate = steps.clone();
            candidate.remove(i);
            if differs(&candidate) {
                steps = candidate;
                shrunk = true;
                break;
            }
            if let Step::Op(op) = &steps[i] {
                for j in 0..op.len() {
                    let mut candidate = steps.clone();
                    if let Step::Op(inner) = &mut candidate[i] {
                        inner.remove(j);
                    }
                    if differs(&candidate) {
                        steps = candidate;
                        shrunk = true;
                        break;
                    }
                }
                if shrunk {
                    break;
                }
            }
        }
        if !shrunk {
            return steps;
        }
    }
}

/// `FROM=TwoPass TO=Port REPEAT=1 cargo test show_the_smallest_difference -- --ignored --nocapture`.
#[test]
#[ignore]
fn show_the_smallest_difference() {
    let mode = |name: &str| match name {
        "Immediate" => Mode::Immediate,
        "TwoPass" => Mode::TwoPass,
        "Batched" => Mode::Batched,
        "PortEager" => Mode::PortEager,
        _ => Mode::Port,
    };
    let from = mode(&std::env::var("FROM").unwrap_or_else(|_| "Immediate".into()));
    let to = mode(&std::env::var("TO").unwrap_or_else(|_| "Port".into()));
    let repeat = std::env::var("REPEAT").is_ok();
    for seed in 0..seeds() {
        let steps = scenario(seed, repeat);
        if first_difference(&run_mode(&steps, from), &run_mode(&steps, to)).is_some() {
            let small = shrink(steps, from, to);
            let a = run_mode(&small, from);
            let b = run_mode(&small, to);
            let tick = first_difference(&a, &b).unwrap();
            eprintln!("seed {seed}: {small:?}");
            eprintln!(
                "tick {tick}\n--- {from:?}\n{}\n--- {to:?}\n{}",
                a.get(tick).cloned().unwrap_or_default(),
                b.get(tick).cloned().unwrap_or_default()
            );
            return;
        }
    }
    eprintln!("nenhum cenário difere");
}

/// Um bloco de redstone que muda de lado numa operação só (um pistão que o empurra, um `/clone` que o move): o cabo reativo do meio da linha, ligado, vê no original o sinal cair e voltar
/// (as duas mudanças chamam o aviso do vizinho uma depois da outra); o port entrega os avisos com o mundo já mudado, e o cabo nunca vê o sinal cair.
fn glitch_scenario(energy: i32) -> Vec<Step> {
    let cable = |x| Change::Cable(BlockPos::new(x, 0, 1));
    let spot = |x, z| BlockPos::new(x, 0, z);
    vec![
        Step::Op(vec![cable(0), cable(1), cable(2), cable(3), cable(4)]),
        Step::Ticks(2),
        Step::Reactive(spot(2, 1)),
        Step::Push(spot(0, 1), energy),
        Step::Ticks(3),
        Step::Op(vec![Change::Power(spot(2, 0))]),
        Step::Ticks(3),
        Step::Op(vec![Change::Unpower(spot(2, 0)), Change::Power(spot(2, 2))]),
        Step::Ticks(4),
    ]
}

/// A diferença que sobra (decisão 187): o cabo reativo do original pisca (o sinal cai e volta no meio da operação) e a rede dele é desfeita e refeita, com outro id no registro; a
/// energia, as ligações e o que cada cabo mostra, tick a tick, são os mesmos. O port lê o sinal no fim da operação (H34) e não vê o sinal cair.
#[test]
fn a_redstone_block_that_changes_sides_in_one_operation_only_changes_the_id_of_the_network() {
    for energy in [1, 2, 7, 1_201, 3_001, 3_999, 24_000] {
        let steps = glitch_scenario(energy);
        let original = run_mode_in(&steps, Mode::Immediate, false);
        let port = run_mode_in(&steps, Mode::Port, false);
        assert!(
            first_difference(&original, &port).is_some(),
            "o original refaz a rede, com outro id (energia {energy})"
        );
        let original = run_mode_in(&steps, Mode::Immediate, true);
        let port = run_mode_in(&steps, Mode::Port, true);
        assert_eq!(
            original, port,
            "sem o id das redes é igual (energia {energy})"
        );
    }
}

#[test]
#[ignore]
fn hunt_an_energy_difference_in_the_blink() {
    let mut found = Vec::new();
    for energy in 1..4000 {
        let steps = glitch_scenario(energy);
        let a: Vec<String> = run_mode(&steps, Mode::Immediate);
        let b: Vec<String> = run_mode(&steps, Mode::Port);
        // Sem o id das redes.
        let strip = |text: &String| {
            let mut out = String::new();
            let mut rest = text.as_str();
            while let Some(at) = rest.find("NetworkId(") {
                out += &rest[..at];
                let tail = &rest[at + 10..];
                rest = &tail[tail.find(')').unwrap() + 1..];
            }
            out + rest
        };
        if a.iter().map(strip).collect::<Vec<_>>() != b.iter().map(strip).collect::<Vec<_>>() {
            found.push(energy);
        }
    }
    eprintln!(
        "energias que mudam algo além do id: {} {:?}",
        found.len(),
        &found[..found.len().min(10)]
    );
}
