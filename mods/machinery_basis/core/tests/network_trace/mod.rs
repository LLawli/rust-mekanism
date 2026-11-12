//! O motor dos testes de rede de recurso contra o rastro do original (`FluidNetworkTraceTest` e `ChemicalNetworkTraceTest`, um `.jsonl` por
//! cenário em `dev/referencia/rastros/<fluid|chemical>_*.jsonl`). Cada linha é a topologia (como em `transmission_trace.rs`) e o recurso do tick
//! (`estate`). Cada linha do rastro é um evento ou o estado depois do `TransmitterNetworkRegistry.onTick` de um tick; o teste monta o mesmo cenário
//! no mundo simulado ([`crate::resource_world`]), dá os `identityHashCode` dos canos e os `UUID.hashCode()` das redes que o rastro gravou, repete as ações
//! (as linhas `mark`) no tick em que aconteceram e compara, a cada tick, os eventos na ordem, o estado de cada cano, cada rede (tamanho, aceitadores,
//! transmissores na ordem da tabela) e a ordem do conjunto `networks`, o recurso de tudo, os `contents`, os `transfer`, os `disperse`, os `nbt`, os
//! `push` e os tickets. Duas execuções do original têm hashes diferentes e ordens diferentes: as duas saem iguais ao port quando ele recebe os hashes
//! de cada uma.
#![allow(dead_code)]
use crate::resource_world::{Event, Kind, Tank, World};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL, SimEvent};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::NetworkId;
use machinery_basis_core::transmission::connection::ConnectionType;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

fn read_rows<K: Kind>(run: &str, scenario: &str) -> Vec<Value> {
    let file = if run.is_empty() {
        format!("{}_{scenario}.jsonl", K::NAME)
    } else {
        format!("{}_{run}_{scenario}.jsonl", K::NAME)
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
        "adopt" => net(row)
            .and_then(|n| rank(mine, int(row, "from")).map(|from| format!("adopt {n} {from}"))),
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
        Event::Adopt { network, from } => format!("adopt {} {}", network.0, from.0),
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
fn port_state<K: Kind>(world: &World<K>, depth: i64, mine: &Mine) -> Value {
    let registry = world.registry();
    let width = i64::from(world.width);
    let in_line = |network: NetworkId| {
        registry.network(network).transmitters().iter().any(|t| {
            let pos = world.level.hooks.state.pipes[t.0 as usize].pos;
            world.index_of(pos, depth as i32).is_some()
        })
    };
    let mut nets: BTreeMap<u32, ()> = BTreeMap::new();
    let mut cables = Vec::new();
    for x in 0..width * depth {
        match world.pipe_at(world.pipe_pos(x as i32)) {
            None => cables.push(Value::Null),
            Some((_, pipe)) => {
                if let Some(network) = pipe.network {
                    nets.insert(network.0, ());
                }
                cables.push(serde_json::json!({
                    "net": pipe.network.map(|n| mine[n.0 as usize]),
                    "orphan": pipe.orphan,
                    "valid": pipe.tr().is_valid(),
                    "loaded": pipe.tr().loaded,
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
                            .index_of(world.level.hooks.state.pipes[t.0 as usize].pos, depth as i32)
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

/// `Direction.getName()`.
fn face(name: &str) -> Face {
    match name {
        "down" => Face::Down,
        "up" => Face::Up,
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        "east" => Face::East,
        other => panic!("lado desconhecido: {other}"),
    }
}

struct Replay<K: Kind> {
    world: World<K>,
    depth: i64,
    /// Os índices do rastro das redes do cenário, por ordem de criação: o `i`-ésimo é a rede `i` do port.
    mine: Vec<i64>,
    /// O nível do ticket que o chunk tinha ao descarregar, para a carga devolvê-lo.
    unloaded_level: i32,
    /// Os tanques postos, na ordem (a de `actors` do rastro).
    actors: Vec<(String, Rc<Tank<K>>)>,
    /// O que `nbt x` gravou, na ordem: o cano, o fluido e a quantidade.
    nbt: VecDeque<(i64, String, i64)>,
    /// O que `push` aceitou, na ordem.
    pushed: VecDeque<i64>,
}

impl<K: Kind> Replay<K> {
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
        let mut world = World::<K>::new(origin, width as i32, hashes, move |pos| {
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
        // E uma margem de um chunk em volta: o cano da ponta pergunta se o vizinho está carregado (`getPossibleAcceptorConnections`), e o
        // `gameTest` roda num mundo com os chunks em volta da estrutura carregados.
        for cx in origin.chunk().x - 1..=last_pos.chunk().x + 1 {
            for cz in origin.chunk().z - 1..=last_pos.chunk().z + 1 {
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
                world.place_pipe(int(tile, "x") as i32, "basic");
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
                    world.place_pipe(x as i32, "basic");
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
            nbt: VecDeque::new(),
            pushed: VecDeque::new(),
        }
    }

    /// Empurra o recurso como um vizinho faz: o contêiner que a capability do cano entrega (`getContainer()`: o da rede, ou o buffer dele
    /// sem rede), com a automação `EXTERNAL`.
    fn insert_into_pipe(&mut self, index: i32, id: &str, amount: i32) -> i32 {
        let pos = self.world.pipe_pos(index);
        let (pipe_id, pipe) = self.world.pipe_at(pos).expect("cano");
        let container = self.world.family().container_of(pipe_id, pipe.net());
        let transaction = Transaction::open_root();
        let got = container.insert(
            &K::resource(id),
            amount,
            &transaction,
            AutomationType::External,
        );
        transaction.commit();
        self.world.level.hooks.state.family.flush_contents();
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
                    .place_pipe(number(2) * width + number(1), words[3]);
            }
            // `break x z`
            "break" => {
                let width = self.world.width;
                let pos = self.world.pipe_pos(number(2) * width + number(1));
                self.world.break_block(pos);
            }
            // `tank nome x y z tier fluido quantidade` (`empty` sem fluido, `-1` a capacidade) e `source nome x y z fluido` (um tanque
            // criativo cheio): o `y` é a altura acima da linha.
            "tank" | "source" => {
                let origin = self.world.origin;
                let pos = BlockPos::new(
                    origin.x + number(2),
                    origin.y + number(3),
                    origin.z + number(4),
                );
                let (tier, id, amount) = if words[0] == "tank" {
                    (words[5], words[6], words[7].parse::<i64>().unwrap())
                } else {
                    ("creative", words[5], -1)
                };
                let content = (id != "empty" && amount != 0)
                    .then(|| (id, (amount >= 0).then_some(amount as u64)));
                let tank = self.world.place_tank(pos, tier, content);
                self.actors.push((words[1].to_owned(), tank));
            }
            // `generator nome x y z`: o Heat Generator.
            "generator" => {
                let origin = self.world.origin;
                let pos = BlockPos::new(
                    origin.x + number(2),
                    origin.y + number(3),
                    origin.z + number(4),
                );
                let tank = K::place_generator(&mut self.world, pos);
                self.actors.push((words[1].to_owned(), tank));
            }
            // `setfluid nome fluido quantidade` (`max` é a capacidade do fluido; `empty` ou 0 esvazia)
            "setchem" | "setfluid" => {
                let tank = &self
                    .actors
                    .iter()
                    .find(|(name, _)| name == words[1])
                    .unwrap()
                    .1
                    .tank;
                let amount = words[3];
                if words[2] == "empty" || amount == "0" {
                    tank.set_contents(None, 0, None);
                } else {
                    let resource = K::resource(words[2]);
                    let amount = if amount == "max" {
                        tank.capacity(&resource)
                    } else {
                        amount.parse().unwrap()
                    };
                    tank.set_contents(Some(resource), amount, None);
                }
                self.world.level.hooks.state.family.flush_contents();
            }
            // `push x z fluido quantidade`
            "push" => {
                let index = number(2) * self.world.width + number(1);
                let got = self.insert_into_pipe(index, words[3], number(4));
                self.pushed.push_back(i64::from(got));
            }
            // `pull x z lado` e `mode x z lado tipo`: o que a chave inglesa faz.
            "pull" | "mode" => {
                let index = number(2) * self.world.width + number(1);
                let side = face(words[3]);
                let kind = if words[0] == "pull" {
                    ConnectionType::Pull
                } else {
                    match words[4] {
                        "normal" => ConnectionType::Normal,
                        "push" => ConnectionType::Push,
                        "pull" => ConnectionType::Pull,
                        "none" => ConnectionType::None,
                        other => panic!("tipo de conexão desconhecido: {other}"),
                    }
                };
                self.world.set_mode(index, side, kind);
            }
            // `nbt x`: `MechanicalPipe.write`.
            "nbt" => {
                let stored = self.world.write_stored(number(1));
                self.world.level.hooks.state.family.flush_contents();
                self.nbt.push_back((
                    i64::from(number(1)),
                    stored.resource.as_ref().map_or(String::new(), K::id),
                    stored.amount as i64,
                ));
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
fn normalized_estate<K: Kind>(row: &Value, mine: &Mine) -> Value {
    let nets: Vec<Value> = row["nets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|net| {
            rank(mine, int(net, "net")).map(|_| {
                serde_json::json!({
                    "net": net["net"],
                    K::KEY: net[K::KEY],
                    "amount": net["amount"],
                    "capacity": net["capacity"],
                    "scaleBits": net["scaleBits"],
                    "prev": net["prev"],
                    "last": net["last"],
                })
            })
        })
        .collect();
    serde_json::json!({ "pipes": row["pipes"], "nets": nets, "actors": row["actors"] })
}

/// O recurso do port, no formato do `estate` normalizado.
fn resource_id<K: Kind>(contents: &StackContents<K::R>) -> String {
    contents.resource.as_ref().map_or(String::new(), K::id)
}

fn port_estate<K: Kind>(replay: &Replay<K>) -> Value {
    let world = &replay.world;
    let state = &world.level.hooks.state;
    let width = i64::from(world.width);
    let mut cables = Vec::new();
    let mut nets: BTreeMap<u32, ()> = BTreeMap::new();
    for x in 0..width * replay.depth {
        if let Some((id, pipe)) = world.pipe_at(world.pipe_pos(x as i32)) {
            if let Some(network) = pipe.network {
                nets.insert(network.0, ());
            }
            let pipe = state.family.pipe(id);
            let (buf, share) = (pipe.share(), pipe.save_share_contents());
            cables.push(serde_json::json!({
                "x": x,
                "buf": buf.amount,
                K::KEY: resource_id::<K>(&buf),
                "share": share.amount,
                K::SHARE_KEY: resource_id::<K>(&share),
                "cap": pipe.capacity(),
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
                let pos = state.pipes[t.0 as usize].pos;
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
            let buffer = network.buffer();
            serde_json::json!({
                "net": replay.mine[id as usize],
                K::KEY: resource_id::<K>(&buffer),
                "amount": buffer.amount,
                "capacity": network.capacity(),
                "scaleBits": i64::from(network.current_scale().to_bits() as i32),
                "prev": network.prev_transfer_amount(),
                "last": network.last_type().map_or(String::new(), |r| K::id(&r)),
            })
        })
        .collect();
    let actors: Vec<Value> = replay
        .actors
        .iter()
        .map(|(name, tank)| {
            let contents = tank.tank.contents();
            serde_json::json!({
                "name": name,
                K::KEY: resource_id::<K>(&contents),
                "amount": contents.amount,
            })
        })
        .collect();
    serde_json::json!({ "pipes": cables, "nets": nets, "actors": actors })
}

/// Repete um cenário do rastro no mundo simulado e compara com ele tick a tick.
pub fn run<K: Kind>(run: &str, scenario: &str) {
    let rows = read_rows::<K>(run, scenario);
    let mut replay = Replay::<K>::new(&rows);
    let origin = replay.world.origin;
    // O último tick é o do último `state` (gravado depois do `onTick` de cada tick): um evento solto depois dele, como o `contents` de uma rede de
    // outro teste que empurra no `ServerTickEvent.Pre` do tick em que este termina, não é um tick do cenário.
    let last_tick = rows
        .iter()
        .filter(|row| kind(row) == "state")
        .map(|row| int(row, "t"))
        .max()
        .unwrap();
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
        if scenario == "heat_generator" {
            // O contêiner de energia do Heat Generator enche no tick 31 do rastro (o gerador para de queimar): a conta de calor e de energia
            // é do gerador, não da rede, então entra de fora.
            for (_, tank) in &replay.actors {
                tank.energy_full.set(t >= 31);
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
                &port_state::<K>(&replay.world, replay.depth, &replay.mine),
                begin,
                &format!("{run} {scenario}, estado inicial"),
            );
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
        // O que o cano aceitou de cada `push` e o fluido de cada `nbt`.
        for row in tick_rows.iter().filter(|row| kind(row) == "push") {
            let got = replay.pushed.pop_front().expect("push do port");
            assert_eq!(got, int(row, "got"), "{context}: push");
        }
        for row in tick_rows.iter().filter(|row| kind(row) == "nbt") {
            let (x, id, amount) = replay.nbt.pop_front().expect("nbt do port");
            assert_eq!(
                (x, id.as_str(), amount),
                (
                    int(row, "x"),
                    row[K::KEY].as_str().unwrap(),
                    int(row, "amount")
                ),
                "{context}: nbt"
            );
        }
        // Os `FluidTransferEvent` (o `PacketNetworkScale`), na ordem.
        let expected_transfers: Vec<(i64, f32, String)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "transfer")
            .filter_map(|row| {
                rank(&replay.mine, int(row, "net")).map(|net| {
                    (
                        net,
                        row["scale"].to_string().parse::<f32>().unwrap(),
                        row[K::KEY].as_str().unwrap().to_owned(),
                    )
                })
            })
            .collect();
        let transfers: Vec<(i64, f32, String)> = replay
            .world
            .level
            .hooks
            .state
            .family
            .take_transfer_events()
            .into_iter()
            .map(|event| {
                (
                    i64::from(event.network.0),
                    event.scale,
                    event.last_type.map_or(String::new(), |r| K::id(&r)),
                )
            })
            .collect();
        assert_eq!(transfers, expected_transfers, "{context}: TransferEvent");
        // O `onContentsChanged` do contêiner de cada rede, na ordem.
        let expected_contents: Vec<(i64, String, i64)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "contents")
            .filter_map(|row| {
                rank(&replay.mine, int(row, "net")).map(|net| {
                    (
                        net,
                        row[K::KEY].as_str().unwrap().to_owned(),
                        int(row, "amount"),
                    )
                })
            })
            .collect();
        let contents: Vec<(i64, String, i64)> = replay
            .world
            .level
            .hooks
            .state
            .family
            .take_contents_events()
            .into_iter()
            .map(|event| {
                (
                    i64::from(event.network.0),
                    resource_id::<K>(&event.contents),
                    event.contents.amount as i64,
                )
            })
            .collect();
        assert_eq!(contents, expected_contents, "{context}: onContentsChanged");
        // Os `disperse` (o vazamento de radiação da rede de químico), na ordem: o tubo que os disparou, o recurso e a quantidade.
        let expected_dispersals: Vec<(i64, String, i64)> = tick_rows
            .iter()
            .filter(|row| kind(row) == "disperse")
            .filter_map(|row| {
                rank(&replay.mine, int(row, "net")).map(|_| {
                    (
                        int(row, "x"),
                        row[K::KEY].as_str().unwrap().to_owned(),
                        int(row, "amount"),
                    )
                })
            })
            .collect();
        let dispersals: Vec<(i64, String, i64)> = replay
            .world
            .level
            .hooks
            .state
            .family
            .take_dispersals()
            .into_iter()
            .map(|dispersal| {
                (
                    i64::from(
                        replay
                            .world
                            .index_of(dispersal.pos, replay.depth as i32)
                            .unwrap(),
                    ),
                    K::id(&dispersal.resource),
                    dispersal.amount,
                )
            })
            .collect();
        assert_eq!(dispersals, expected_dispersals, "{context}: disperse");
        let estate = tick_rows
            .iter()
            .rfind(|row| kind(row) == "estate")
            .unwrap_or_else(|| panic!("{context}: falta o estado do recurso"));
        assert_eq!(
            port_estate::<K>(&replay),
            normalized_estate::<K>(estate, &replay.mine),
            "{context}: recurso"
        );
        let state = states
            .next()
            .unwrap_or_else(|| panic!("{context}: falta o estado"));
        assert_eq!(int(state, "t"), t);
        assert_state(
            &port_state::<K>(&replay.world, replay.depth, &replay.mine),
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
