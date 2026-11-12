//! A topologia das redes de transmissores contra o rastro do original (`dev/referencia/testes/NetworkTopologyTraceTest.java`, um
//! `.jsonl` por cenário em `dev/referencia/rastros/network_*.jsonl`). Cada linha do rastro é um evento ou o estado depois do
//! `TransmitterNetworkRegistry.onTick` de um tick; o teste monta o mesmo cenário no mundo simulado, dá os `identityHashCode` dos cabos
//! e os `UUID.hashCode()` das redes que o rastro gravou, repete as ações (as linhas `mark`) no tick em que aconteceram e compara, a cada
//! tick, os eventos na ordem, o estado de cada cabo, cada rede (tamanho, aceitadores, transmissores na ordem da tabela) e a ordem do
//! conjunto `networks`. Duas execuções do original (`network_*` e `network_run2_*`) têm hashes diferentes e ordens diferentes: as duas
//! saem iguais ao port quando ele recebe os hashes de cada uma.
mod transmission_world;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL, SimEvent};
use machinery_basis_core::transmission::NetworkId;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use transmission_world::{CUBE, Event, World};

const SCENARIOS: [&str; 11] = [
    "line",
    "merge",
    "split_middle",
    "break_end",
    "same_tick",
    "acceptors",
    "chunk_reload",
    "chunk_inaccessible",
    "frozen",
    "grid",
    "chunk_flip",
];

fn read_rows(run: &str, scenario: &str) -> Vec<Value> {
    let file = if run.is_empty() {
        format!("network_{scenario}.jsonl")
    } else {
        format!("network_{run}_{scenario}.jsonl")
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
                world.place_cable(int(tile, "x") as i32);
            }
            world.level.tick_n(3);
        }
        for t in first..0 {
            world.level.tick_level();
            for tile in setup["tiles"].as_array().unwrap() {
                let x = int(tile, "x");
                if joined.get(&x).map_or(first, |joined| joined - 1) == t {
                    world.place_cable(x as i32);
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
        }
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
            "place" => self.world.place_cable(number(1)),
            "break" => {
                let pos = self.world.cable(number(1));
                self.world.break_block(pos);
            }
            "cube" => {
                let pos = self.world.cable(number(1));
                self.world
                    .level
                    .set_block(BlockPos::new(pos.x, pos.y, pos.z + 1), Some(CUBE));
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
        replay.world.level.tick_level();
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
