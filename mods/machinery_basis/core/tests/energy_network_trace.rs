//! A rede de energia contra o rastro do original (`dev/referencia/testes/EnergyNetworkTraceTest.java`, um `.jsonl` por cenário em
//! `dev/referencia/rastros/energy_*.jsonl`). Cada linha é a topologia (como em `transmission_trace.rs`) e a energia do tick (`estate`). Cada linha do rastro é um evento ou o estado depois do
//! `TransmitterNetworkRegistry.onTick` de um tick; o teste monta o mesmo cenário no mundo simulado, dá os `identityHashCode` dos cabos
//! e os `UUID.hashCode()` das redes que o rastro gravou, repete as ações (as linhas `mark`) no tick em que aconteceram e compara, a cada
//! tick, os eventos na ordem, o estado de cada cabo, cada rede (tamanho, aceitadores, transmissores na ordem da tabela) e a ordem do
//! conjunto `networks`. Duas execuções do original (`network_*` e `network_run2_*`) têm hashes diferentes e ordens diferentes: as duas
//! saem iguais ao port quando ele recebe os hashes de cada uma.
mod energy_world;
use energy_world::{Event, Machine, World};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL, SimEvent};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::NetworkId;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

const SCENARIOS: [&str; 9] = [
    "line_consumer",
    "two_consumers",
    "tiers",
    "merge_full",
    "split_middle",
    "break_end_odd",
    "cube",
    "chunk_reload",
    "reopen",
];

fn read_rows(run: &str, scenario: &str) -> Vec<Value> {
    let file = if run.is_empty() {
        format!("energy_{scenario}.jsonl")
    } else {
        format!("energy_{run}_{scenario}.jsonl")
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
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

/// O índice que o port dá a uma rede do original: a posição dela entre as redes do cenário, por ordem de criação (o índice do
/// rastro conta as redes de todos os testes que rodam ao mesmo tempo no jogo).
type Mine = [i64];

fn rank(mine: &Mine, java_index: i64) -> Option<i64> {
    mine.iter().position(|&n| n == java_index).map(|i| i as i64)
}

/// O que o rastro grava como evento da rede, no formato do teste: `update 4`, `commit 3`...
fn trace_event(row: &Value, mine: &Mine) -> Option<String> {
    // Redes de outros testes seguem registradas e recebem `onUpdate`: só contam as do cenário.
    let net = |row: &Value| rank(mine, int(row, "net"));
    match kind(row) {
        "update" => Some(format!("update {}", int(row, "x"))),
        "access" => Some(format!(
            "access {} {}",
            int(row, "x"),
            row["loaded"].as_bool().unwrap()
        )),
        "commit" => net(row).map(|n| format!("commit {n}")),
        "invalidate" => net(row).map(|n| format!("invalidate {n}")),
        "net_update" => net(row).map(|n| format!("net_update {n}")),
        "created" => net(row).map(|n| {
            format!(
                "created {n} {} [{}]",
                int(row, "hash"),
                row["from"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|from| rank(mine, from.as_i64().unwrap()).unwrap().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }),
        _ => None,
    }
}

fn port_event(event: &Event, index: &dyn Fn(BlockPos) -> i32) -> String {
    match event {
        Event::Update(pos) => format!("update {}", index(*pos)),
        Event::Commit(net) => format!("commit {}", net.0),
        Event::Invalidate(net) => format!("invalidate {}", net.0),
        Event::NetUpdate(net) => format!("net_update {}", net.0),
        Event::Created {
            network,
            hash,
            from,
        } => format!(
            "created {} {hash} [{}]",
            network.0,
            from.iter()
                .map(|n| n.0.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        Event::Access(pos, loaded) => format!("access {} {loaded}", index(*pos)),
    }
}

/// O estado do port, no formato do `state` do rastro (as redes com o índice do original).
fn port_state(world: &World, depth: i64, mine: &Mine) -> Value {
    let registry = world.registry();
    let width = i64::from(world.width);
    let in_line = |network: NetworkId| {
        registry.network(network).transmitters().iter().any(|t| {
            let pos = world.level.hooks.state.txs[t.0 as usize].pos;
            world.index_of(pos, depth as i32).is_some()
        })
    };
    let mut nets: BTreeMap<u32, ()> = BTreeMap::new();
    let mut cables = Vec::new();
    for x in 0..width * depth {
        match world.tx_at(world.cable(x as i32)) {
            None => cables.push(Value::Null),
            Some((_, tx)) => {
                if let Some(network) = tx.network {
                    nets.insert(network.0, ());
                }
                cables.push(serde_json::json!({
                    "net": tx.network.map(|n| mine[n.0 as usize]),
                    "orphan": tx.orphan,
                    "valid": !tx.removed && tx.loaded,
                    "loaded": tx.loaded,
                }));
            }
        }
    }
    let registry_order: Vec<i64> = registry
        .networks_in_order()
        .into_iter()
        .filter(|network| in_line(*network))
        .map(|network| {
            nets.insert(network.0, ());
            mine[network.0 as usize]
        })
        .collect();
    let nets: Vec<Value> = nets
        .keys()
        .map(|&id| {
            let network = registry.network(NetworkId(id));
            serde_json::json!({
                "net": mine[id as usize],
                "hash": network.uuid().hash_code(),
                "size": network.transmitters_size(),
                "acceptors": network.acceptor_count(),
                "transmitters": network.transmitters().iter()
                    .map(|t| {
                        world
                            .index_of(world.level.hooks.state.txs[t.0 as usize].pos, depth as i32)
                            .unwrap()
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({ "cables": cables, "nets": nets, "registry": registry_order })
}

/// Compara o estado do port com a linha `state` do rastro, campo a campo.
fn assert_state(port: &Value, trace: &Value, context: &str) {
    for key in ["cables", "nets", "registry"] {
        assert_eq!(port[key], trace[key], "{context}: {key}");
    }
}

fn parse_mark(mark: &str) -> (bool, Vec<&str>) {
    let (pre, rest) = match mark.strip_prefix("pre ") {
        Some(rest) => (true, rest),
        None => (false, mark),
    };
    (pre, rest.split(' ').collect())
}

struct Replay {
    world: World,
    depth: i64,
    /// Os índices do rastro das redes do cenário, por ordem de criação: o `i`-ésimo é a rede `i` do port.
    mine: Vec<i64>,
    /// O nível do ticket que o chunk tinha ao descarregar, para a carga devolvê-lo.
    unloaded_level: i32,
    /// As máquinas postas, na ordem (a de `actors` do rastro).
    actors: Vec<(String, Rc<Machine>)>,
    /// O cabo em que o cubo criativo empurra, depois de posto.
    source: Option<i32>,
    /// O que `nbt x` gravou, na ordem.
    nbt: VecDeque<(i64, i64)>,
    /// O que `push` aceitou, na ordem.
    pushed: VecDeque<i64>,
}

impl Replay {
    /// A linha `index` do rastro tem de existir; devolve o mundo no estado do `setup`, ainda antes do primeiro tick do rastro.
    fn new(rows: &[Value]) -> Self {
        let setup = &rows[0];
        assert_eq!(kind(setup), "setup");
        let origin = BlockPos::new(
            int(setup, "x") as i32,
            int(setup, "y") as i32,
            int(setup, "z") as i32,
        );
        let width = int(setup, "width");
        let depth = setup["depth"].as_i64().unwrap_or(1);
        let begin = rows.iter().find(|row| kind(row) == "state").unwrap();
        // Os hashes dos cabos: os do `setup` e depois os de cada `tile`, na ordem, por posição.
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
        // Os hashes das redes: as do estado inicial, por índice, e depois as de cada `created`.
        let mut uuid_hashes: Vec<(i64, i32)> = begin["nets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|net| (int(net, "net"), int(net, "hash") as i32))
            .collect();
        uuid_hashes.sort_unstable();
        let mut created: Vec<(i64, i32)> = rows
            .iter()
            .filter(|row| kind(row) == "created")
            .map(|row| (int(row, "net"), int(row, "hash") as i32))
            .collect();
        created.sort_unstable();
        // As redes do cenário são as que aparecem no estado (cabo ou lista de redes): as de outros testes que rodam junto não.
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
        created.retain(|(net, _)| mine.contains(net));
        uuid_hashes.extend(created);
        uuid_hashes.sort_unstable();
        uuid_hashes.dedup();
        let hashes = uuid_hashes.into_iter().map(|(_, hash)| hash).collect();
        let index_of = move |pos: BlockPos| (pos.z - origin.z) * width as i32 + (pos.x - origin.x);
        let mut world = World::new(origin, width as i32, hashes, move |pos| {
            tile_hashes
                .get_mut(&index_of(pos))
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| panic!("o rastro não tem hash para o cabo {}", index_of(pos)))
        });
        // Os chunks da linha ficam com o ticket da estrutura do gameTest.
        let last_pos = BlockPos::new(
            origin.x + width as i32 - 1,
            origin.y,
            origin.z + depth as i32 - 1,
        );
        for cx in origin.chunk().x..=last_pos.chunk().x {
            for cz in origin.chunk().z..=last_pos.chunk().z {
                world
                    .level
                    .set_ticket_level(ChunkPos::new(cx, cz), ENTITY_TICKING_LEVEL);
            }
        }
        world.level.tick();
        // A estrutura do gameTest nasce cabo a cabo e cada cabo entra na rede no primeiro tick do block entity dele, que depende do
        // chunk: o rastro grava os `update` de antes do `setup` (`before_update`), e o cabo é posto um tick antes do primeiro deles.
        let mut joined: HashMap<i64, i64> = HashMap::new();
        for row in rows.iter().filter(|row| kind(row) == "before_update") {
            joined.entry(int(row, "x")).or_insert(int(row, "t"));
        }
        let first = joined.values().min().map_or(0, |t| t - 1);
        if joined.is_empty() {
            // Sem o histórico (a estrutura nasceu enquanto outro cenário rodava e o rastro o perdeu): todos juntos, no mesmo tick.
            for tile in setup["tiles"].as_array().unwrap() {
                world.place_cable(int(tile, "x") as i32, "basic");
            }
            for _ in 0..3 {
                world.tick_level();
                world.level.finish_tick();
            }
        }
        for t in first..0 {
            world.tick_level();
            for tile in setup["tiles"].as_array().unwrap() {
                let x = int(tile, "x");
                if joined.get(&x).map_or(first, |joined| joined - 1) == t {
                    world.place_cable(x as i32, "basic");
                }
            }
            world.level.finish_tick();
        }
        world.take_events();
        world.level.take_events();
        Self {
            world,
            depth,
            mine,
            unloaded_level: 0,
            actors: Vec::new(),
            source: None,
            nbt: VecDeque::new(),
            pushed: VecDeque::new(),
        }
    }

    /// Onde estão o contêiner que o cabo entrega (`getContainer()`): o da rede, ou o buffer dele sem rede.
    fn insert_into_cable(&mut self, index: i32, amount: i32) -> i32 {
        let pos = self.world.cable(index);
        let (id, tx) = self.world.tx_at(pos).expect("cabo");
        let network = tx.network;
        let state = &self.world.level.hooks.state;
        let container = match network {
            Some(network) => state.family.network(network).unwrap().energy.clone(),
            None => state.family.cable(id).buffer.clone(),
        };
        let transaction = Transaction::open_root();
        let got = container.insert(amount, &transaction, AutomationType::External);
        transaction.commit();
        got
    }

    fn line_chunk(&self, index: i32) -> ChunkPos {
        BlockPos::new(
            self.world.origin.x + 16 * index + 8,
            self.world.origin.y,
            self.world.origin.z,
        )
        .chunk()
    }

    fn apply(&mut self, mark: &str, load_delay: Option<u32>) {
        let (_, words) = parse_mark(mark);
        let number = |i: usize| words[i].parse::<i32>().unwrap();
        match words[0] {
            // `place x z tier`
            "place" => {
                let width = self.world.width;
                self.world
                    .place_cable(number(2) * width + number(1), words[3]);
            }
            // `break x z`
            "break" => {
                let width = self.world.width;
                let pos = self.world.cable(number(2) * width + number(1));
                self.world.break_block(pos);
            }
            // `chamber nome x y z max guardada`: o `y` é a altura acima da linha.
            "chamber" => {
                let origin = self.world.origin;
                let pos = BlockPos::new(
                    origin.x + number(2),
                    origin.y + number(3),
                    origin.z + number(4),
                );
                let machine = self.world.place_machine(
                    pos,
                    words[5].parse().unwrap(),
                    words[6].parse().unwrap(),
                );
                self.actors.push((words[1].to_owned(), machine));
            }
            // `source nome x y z`: um cubo criativo ao lado do cabo (`z + 1`) que empurra a cada tick; o que ele empurra vem do rastro.
            // O cubo também é um aceitador (o `commit` e a contagem do rastro), que não aceita nada: os lados dele são `OUTPUT`.
            "source" => {
                let origin = self.world.origin;
                let pos = BlockPos::new(
                    origin.x + number(2),
                    origin.y + number(3),
                    origin.z + number(4) + 1,
                );
                let machine = self.world.place_machine(pos, 0, i64::MAX);
                self.actors.push((words[1].to_owned(), machine));
                self.source = Some(number(4) * self.world.width + number(2));
            }
            // `setenergy nome energia`
            "setenergy" => {
                let machine = &self
                    .actors
                    .iter()
                    .find(|(name, _)| name == words[1])
                    .unwrap()
                    .1;
                machine
                    .container
                    .set_energy(words[2].parse().unwrap(), None);
            }
            // `push x z quantidade`
            "push" => {
                let index = number(2) * self.world.width + number(1);
                let got = self.insert_into_cable(index, number(3));
                self.pushed.push_back(i64::from(got));
            }
            // `nbt x`: `UniversalCable.write`.
            "nbt" => {
                let pos = self.world.cable(number(1));
                let (id, network) = {
                    let (id, tx) = self.world.tx_at(pos).expect("cabo");
                    (id, tx.network)
                };
                let state = &mut self.world.level.hooks.state;
                let energy =
                    state
                        .family
                        .write_energy(&self.world.level.hooks.registry, id, network);
                self.nbt.push_back((i64::from(number(1)), energy));
            }
            // `ChunkData.updateChunkLoading`: não faz nada se o chunk já está no estado pedido.
            "unload" => {
                let chunk = self.line_chunk(number(1));
                if self.world.level.view().is_chunk_loaded(chunk) {
                    self.unloaded_level = self.world.level.unload_chunk(chunk);
                }
            }
            "load" => {
                let chunk = self.line_chunk(number(1));
                if !self.world.level.view().is_chunk_loaded(chunk) {
                    self.world.level.set_load_delay(load_delay.unwrap_or(0));
                    self.world.level.load_chunk(chunk, self.unloaded_level);
                }
            }
            "level" => {
                let chunk = self.line_chunk(number(1));
                self.world.level.set_chunk_load_level(chunk, number(2));
            }
            "freeze" => self.world.level.hooks.state.runs_normally = false,
            "unfreeze" => self.world.level.hooks.state.runs_normally = true,
            "finish" => {}
            other => panic!("ação desconhecida no rastro: {other}"),
        }
    }
}

/// O `estate` do rastro com as redes pelo índice do port (as de outros testes saem) e sem o texto da escala.
fn normalized_estate(row: &Value, mine: &Mine) -> Value {
    let nets: Vec<Value> = row["nets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|net| {
            rank(mine, int(net, "net")).map(|_| {
                serde_json::json!({
                    "net": net["net"],
                    "energy": net["energy"],
                    "capacity": net["capacity"],
                    "scaleBits": net["scaleBits"],
                    "prev": net["prev"],
                })
            })
        })
        .collect();
    serde_json::json!({ "cables": row["cables"], "nets": nets, "actors": row["actors"] })
}

/// A energia do port, no formato do `estate` normalizado.
fn port_estate(replay: &Replay) -> Value {
    let world = &replay.world;
    let state = &world.level.hooks.state;
    let width = i64::from(world.width);
    let mut cables = Vec::new();
    let mut nets: BTreeMap<u32, ()> = BTreeMap::new();
    for x in 0..width * replay.depth {
        if let Some((id, tx)) = world.tx_at(world.cable(x as i32)) {
            if let Some(network) = tx.network {
                nets.insert(network.0, ());
            }
            let cable = state.family.cable(id);
            cables.push(serde_json::json!({
                "x": x,
                "buf": cable.buffer.amount(),
                "share": cable.save_share_amount(),
                "cap": cable.capacity(),
            }));
        }
    }
    // As redes do registro que têm um cabo da linha entram também (como `registry` do `state`).
    for network in world.registry().networks_in_order() {
        let in_line = world
            .registry()
            .network(network)
            .transmitters()
            .iter()
            .any(|t| {
                let pos = state.txs[t.0 as usize].pos;
                world.index_of(pos, replay.depth as i32).is_some()
            });
        if in_line {
            nets.insert(network.0, ());
        }
    }
    let nets: Vec<Value> = nets
        .keys()
        .map(|&id| {
            let network = state.family.network(NetworkId(id)).unwrap();
            serde_json::json!({
                "net": replay.mine[id as usize],
                "energy": network.energy.amount(),
                "capacity": network.capacity(),
                "scaleBits": i64::from(network.current_scale().to_bits() as i32),
                "prev": network.prev_transfer_amount(),
            })
        })
        .collect();
    let actors: Vec<Value> = replay
        .actors
        .iter()
        .map(|(name, machine)| {
            serde_json::json!({ "name": name, "energy": machine.container.amount() })
        })
        .collect();
    serde_json::json!({ "cables": cables, "nets": nets, "actors": actors })
}

fn run(run: &str, scenario: &str) {
    let rows = read_rows(run, scenario);
    let mut replay = Replay::new(&rows);
    let origin = replay.world.origin;
    let last_tick = rows.iter().map(|row| int(row, "t")).max().unwrap();
    // O estado antes do primeiro `onTick`, que o rastro grava em `begin`.
    let mut states: Vec<&Value> = rows.iter().filter(|row| kind(row) == "state").collect();
    let begin = states.remove(0);
    let mut states = states.into_iter();
    let mut first = true;
    for t in 0..=last_tick {
        let tick_rows: Vec<&Value> = rows.iter().filter(|row| int(row, "t") == t).collect();
        let marks: Vec<(bool, String)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "mark")
            .map(|row| {
                let mark = row["m"].as_str().unwrap();
                (parse_mark(mark).0, mark.to_owned())
            })
            .collect();
        // O tempo que a carga do chunk leva, para o `load_chunk` do port: do pedido até o `ChunkEvent.Load`.
        let load_delay = |mark_tick: i64| {
            rows.iter()
                .find(|row| kind(row) == "load" && int(row, "t") > mark_tick)
                .map(|row| (int(row, "t") - mark_tick - 1) as u32)
        };
        if marks.iter().any(|(pre, mark)| *pre && mark == "pre finish") {
            // `Recorder.finish` roda antes do tick, e o rastro não grava mais nada.
            break;
        }
        for (pre, mark) in &marks {
            if *pre {
                replay.apply(mark, load_delay(t));
            }
        }
        replay.world.tick_level();
        // Um `ChunkTicketLevelUpdatedEvent` que nenhuma ação do cenário explica é o jogo devolvendo o chunk ao nível dos tickets dele
        // (`LoadingChunkTracker`): o simulador não sabe quando, o rastro diz. O chunk carrega depois do tick dos block entities.
        let acts_on_chunks = marks
            .iter()
            .any(|(_, mark)| matches!(parse_mark(mark).1[0], "unload" | "load" | "level"));
        if !acts_on_chunks {
            for row in tick_rows.iter().filter(|row| kind(row) == "ticket") {
                let chunk =
                    ChunkPos::new(origin.chunk().x + int(row, "cx") as i32, origin.chunk().z);
                replay.world.level.set_load_delay(0);
                replay.world.level.update_ticket_level(chunk);
                replay.world.level.complete_pending_loads();
            }
        }
        if first {
            // O `setup` do rastro roda dentro do passo do primeiro tick, depois do tick do nível.
            first = false;
            assert_state(
                &port_state(&replay.world, replay.depth, &replay.mine),
                begin,
                &format!("{run} {scenario}, estado inicial"),
            );
        }
        // O cubo criativo empurra pelo ejetor dele, no tick do block entity: o que entrou na rede do cabo antes do `net_update` dela.
        if let Some(index) = replay.source {
            let network = replay
                .world
                .tx_at(replay.world.cable(index))
                .and_then(|(_, tx)| tx.network);
            if let Some(network) = network {
                let mine = replay.mine[network.0 as usize];
                let mut before_update = true;
                let mut after_push = None;
                for row in tick_rows.iter() {
                    if kind(row) == "net_update" && int(row, "net") == mine {
                        before_update = false;
                    }
                    if before_update && kind(row) == "contents" && int(row, "net") == mine {
                        after_push = Some(int(row, "energy"));
                        break;
                    }
                }
                if let Some(energy) = after_push {
                    let now = replay
                        .world
                        .level
                        .hooks
                        .state
                        .family
                        .network(network)
                        .unwrap()
                        .energy
                        .amount();
                    let pushed = (energy - now) as i32;
                    let got = replay.insert_into_cable(index, pushed);
                    assert_eq!(got, pushed, "{run} {scenario}, tick {t}: o cubo empurrou");
                }
            }
        }
        for (pre, mark) in &marks {
            if !*pre {
                replay.apply(mark, load_delay(t));
            }
        }
        replay.world.level.finish_tick();
        let context = format!("{run} {scenario}, tick {t}");
        let expected: Vec<String> = tick_rows
            .iter()
            .filter_map(|row| trace_event(row, &replay.mine))
            .collect();
        let actual: Vec<String> = replay
            .world
            .take_events()
            .iter()
            .map(|event| {
                port_event(event, &|pos| {
                    replay.world.index_of(pos, replay.depth as i32).unwrap()
                })
            })
            .collect();
        assert_eq!(actual, expected, "{context}: eventos");
        // O que o cabo aceitou de cada `push` e a energia de cada `nbt`.
        for row in tick_rows.iter().filter(|row| kind(row) == "push") {
            let got = replay.pushed.pop_front().expect("push do port");
            assert_eq!(got, int(row, "got"), "{context}: push");
        }
        for row in tick_rows.iter().filter(|row| kind(row) == "nbt") {
            let (x, energy) = replay.nbt.pop_front().expect("nbt do port");
            assert_eq!(
                (x, energy),
                (int(row, "x"), int(row, "energy")),
                "{context}: nbt"
            );
        }
        // Os `EnergyTransferEvent` (o `PacketNetworkScale`), na ordem.
        let expected_transfers: Vec<(i64, String)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "transfer")
            .filter_map(|row| {
                rank(&replay.mine, int(row, "net")).map(|net| (net, row["scale"].to_string()))
            })
            .collect();
        let transfers: Vec<(i64, String)> = replay
            .world
            .level
            .hooks
            .state
            .family
            .take_transfer_events()
            .into_iter()
            .map(|(network, scale)| (i64::from(network.0), format!("{scale:?}")))
            .collect();
        let expected_scales: Vec<(i64, f32)> = expected_transfers
            .iter()
            .map(|(net, scale)| (*net, scale.parse::<f32>().unwrap()))
            .collect();
        let port_scales: Vec<(i64, f32)> = transfers
            .iter()
            .map(|(net, scale)| (*net, scale.parse::<f32>().unwrap()))
            .collect();
        assert_eq!(
            port_scales, expected_scales,
            "{context}: EnergyTransferEvent"
        );
        let estate = tick_rows
            .iter()
            .rfind(|row| kind(row) == "estate")
            .unwrap_or_else(|| panic!("{context}: falta o estado de energia"));
        assert_eq!(
            port_estate(&replay),
            normalized_estate(estate, &replay.mine),
            "{context}: energia"
        );
        let state = states
            .next()
            .unwrap_or_else(|| panic!("{context}: falta o estado"));
        assert_eq!(int(state, "t"), t);
        assert_state(
            &port_state(&replay.world, replay.depth, &replay.mine),
            state,
            &context,
        );
        // Os eventos de ticket do nível: os que o rastro viu, e só esses.
        let tickets: Vec<(i64, i64, i64)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "ticket")
            .map(|row| (int(row, "cx"), int(row, "old"), int(row, "new")))
            .collect();
        let origin_chunk = origin.chunk();
        let port_tickets: Vec<(i64, i64, i64)> = replay
            .world
            .level
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                SimEvent::TicketLevelUpdated { chunk, old, new } => Some((
                    i64::from(chunk.x - origin_chunk.x),
                    i64::from(old),
                    i64::from(new),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(port_tickets, tickets, "{context}: níveis de ticket");
    }
}

#[test]
fn every_scenario_matches_the_first_run_of_the_original() {
    for scenario in SCENARIOS {
        run("", scenario);
    }
}

#[test]
fn every_scenario_matches_the_second_run_of_the_original() {
    for scenario in SCENARIOS {
        run("run2", scenario);
    }
}

/// A quantidade da rede de cada cabo que sobrou no fim do cenário, no rastro do original: (índice do cabo, energia). No `break_end_odd` são as duas metades que
/// a unidade ímpar deixa desiguais.
fn recorded_halves(run: &str, scenario: &str) -> Vec<(i32, u64)> {
    let rows = read_rows(run, scenario);
    let state = rows.iter().rfind(|row| kind(row) == "state").unwrap();
    let estate = rows.iter().rfind(|row| kind(row) == "estate").unwrap();
    let amount_of = |net: i64| {
        estate["nets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|candidate| int(candidate, "net") == net)
            .map(|candidate| int(candidate, "energy") as u64)
    };
    let mut out = Vec::new();
    for (x, cable) in state["cables"].as_array().unwrap().iter().enumerate() {
        if let Some(amount) = cable["net"].as_i64().and_then(amount_of) {
            out.push((x as i32, amount));
        }
    }
    out
}

/// Repete as ações do cenário do rastro `run` no mundo simulado, sem comparar com o rastro, com a linha deslocada `dx` blocos em x e, se `seed` vem, com os hashes dos
/// cabos e das redes sorteados por ela no lugar dos que o rastro gravou. Devolve o mesmo que [`recorded_halves`].
fn replay_halves(run: &str, scenario: &str, dx: i32, seed: Option<u64>) -> Vec<(i32, u64)> {
    let mut rows = read_rows(run, scenario);
    rows[0]["x"] = Value::from(int(&rows[0], "x") + i64::from(dx));
    if let Some(mut state) = seed {
        // Um congruencial qualquer: só precisa de hashes de 32 bits diferentes a cada semente.
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            i64::from((state >> 32) as i32)
        };
        for row in rows.iter_mut().filter(|row| kind(row) == "tile" || kind(row) == "created") {
            row["hash"] = Value::from(next());
        }
        for tile in rows[0]["tiles"].as_array_mut().unwrap() {
            tile["hash"] = Value::from(next());
        }
        for row in rows.iter_mut().filter(|row| kind(row) == "state") {
            for net in row["nets"].as_array_mut().into_iter().flatten() {
                net["hash"] = Value::from(next());
            }
        }
    }
    let mut replay = Replay::new(&rows);
    let last_tick = rows
        .iter()
        .filter(|row| kind(row) == "state")
        .map(|row| int(row, "t"))
        .max()
        .unwrap();
    for t in 0..=last_tick {
        let marks: Vec<(bool, String)> = rows
            .iter()
            .filter(|row| int(row, "t") == t && kind(row) == "mark")
            .map(|row| {
                let mark = row["m"].as_str().unwrap();
                (parse_mark(mark).0, mark.to_owned())
            })
            .collect();
        if marks.iter().any(|(pre, mark)| *pre && mark == "pre finish") {
            break;
        }
        for (pre, mark) in &marks {
            if *pre {
                replay.apply(mark, None);
            }
        }
        replay.world.tick_level();
        for (pre, mark) in &marks {
            if !*pre {
                replay.apply(mark, None);
            }
        }
        replay.world.level.finish_tick();
        replay.world.take_events();
    }
    let world = &replay.world;
    let state = &world.level.hooks.state;
    let mut out = Vec::new();
    for x in 0..i64::from(world.width) * replay.depth {
        if let Some((_, tx)) = world.tx_at(world.cable(x as i32))
            && let Some(network) = tx.network
        {
            out.push((x as i32, state.family.network(network).unwrap().energy.amount() as u64));
        }
    }
    out
}

/// Quantas rodadas do original o `break_end_odd` tem além das duas primeiras (`odd_0` a `odd_9`, de `EnergyOddTraceTest.java`: cada uma é um `GameTest` em outro lote,
/// logo em outra posição do mundo).
const ODD_RUNS: usize = 10;

#[test]
fn the_ten_more_positions_of_break_end_odd_match_the_original() {
    for k in 0..ODD_RUNS {
        run("", &format!("odd_{k}"));
    }
}

/// A unidade ímpar do `break_end_odd` (1.001 em cinco cabos, quebrados um a um) vai para uma das duas metades que sobram, e qual delas depende da posição da linha
/// no mundo, não dos hashes: `DynamicNetwork.invalidate` percorre `positionedTransmitters`, um `Long2ObjectOpenHashMap` por `BlockPos.asLong` (a decisão 155).
/// As doze rodadas do original: o port com hashes sorteados dá a mesma metade, as duas aparecem entre as posições, e deslocar a linha a troca.
#[test]
fn the_odd_unit_of_break_end_odd_goes_where_the_position_puts_it() {
    let runs: Vec<(&str, String)> = [("", "break_end_odd".to_owned()), ("run2", "break_end_odd".to_owned())]
        .into_iter()
        .chain((0..ODD_RUNS).map(|k| ("", format!("odd_{k}"))))
        .collect();
    let mut at_cable_one = std::collections::BTreeSet::new();
    for (run, scenario) in &runs {
        let recorded = recorded_halves(run, scenario);
        assert_eq!(recorded.len(), 2, "{run} {scenario}: as duas metades");
        at_cable_one.insert(recorded.iter().find(|(x, _)| *x == 1).unwrap().1);
        assert_eq!(replay_halves(run, scenario, 0, None), recorded, "{run} {scenario}");
        for seed in 1..=20 {
            assert_eq!(
                replay_halves(run, scenario, 0, Some(seed)),
                recorded,
                "{run} {scenario}, hashes da semente {seed}"
            );
        }
    }
    assert_eq!(at_cable_one.len(), 2, "as duas metades aparecem entre as posições: {at_cable_one:?}");
    let moved: std::collections::BTreeSet<u64> = (0..16)
        .map(|dx| replay_halves("", "break_end_odd", dx, None).iter().find(|(x, _)| *x == 1).unwrap().1)
        .collect();
    assert_eq!(moved.len(), 2, "deslocar a linha troca a metade: {moved:?}");
}
