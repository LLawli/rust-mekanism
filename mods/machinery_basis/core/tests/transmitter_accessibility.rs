//! A abertura de um mundo com uma linha longa de cabos que passa por vários chunks, na ordem que a cola da `machinery_basis` entrega (decisão 198): os chunks carregam, os cabos dos que andam
//! entram na rede no `Update` do tick, o anel acessível (o ticket de carga de 32 ou menos do original) muda entre as passadas e o `Post` trata os chunks que mudaram de lado.
//! O módulo de eventos (`transmitter_events.rs`) e o dono dos cabos (`EnergyTransmitters`) rodam como estão; o jogo é simulado por trás. Um arquivo só, porque o módulo de eventos guarda o
//! estado em `static`.
#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}
use events::LoadingTickets;
use events::{Event as GameEvent, Kind, Raw};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard};

const CABLE: &str = "test:cable";
const OW: &str = "minecraft:overworld";
/// Os chunks da linha: do 0 ao 13, como a pista de energia dos setups reais (o chunk 0 tem 15 cabos, de x = 1 a 15).
const CHUNKS: i32 = 14;
/// A linha longa da decisão 199: até o chunk 23, onde o Network Reader em x = 100 lê 383 transmissores no original.
const LONG: i32 = 24;

static SERIAL: Mutex<()> = Mutex::new(());

fn begin() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    events::reset();
    events::register(CABLE);
    guard
}

fn chunk_of(x: i32) -> i32 {
    x >> 4
}

/// O jogo simulado: quais chunks da linha estão carregados e quais estão no anel acessível (e andam).
struct Game {
    loaded: BTreeSet<i32>,
    ring: BTreeSet<i32>,
    /// Os chunks da linha.
    chunks: i32,
    /// Onde os block entities andam (a distância de simulação), além de carregados e no anel; `None` anda em todo o anel.
    ticking: Option<BTreeSet<i32>>,
}

impl OwnerWorld for Game {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        self.loaded.contains(&chunk_of(pos.x))
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    fn energy_handler(&self, _: BlockPos, _: Face) -> Option<Acceptor> {
        None
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        (pos.z == 0 && (1..self.chunks * 16).contains(&pos.x)).then_some(CableInfo {
            capacity: 8_000,
            saved: None,
        })
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}
}

struct World {
    owner: EnergyTransmitters,
    game: Game,
    pass: i64,
    /// Os `clearRemoved` e os `setRemoved` que o dono recebeu.
    clears: usize,
    removals: usize,
}

fn cable_pos(x: i32) -> [i32; 3] {
    [x, 64, 0]
}

fn owner_event(event: &GameEvent) -> Option<Event> {
    Some(match event {
        GameEvent::Transmitter { pos, kind, .. } => {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            match kind {
                Kind::ClearRemoved => Event::ClearRemoved(at),
                Kind::PlacedByItem => Event::PlacedByItem(at),
                Kind::ChunkUnloaded => Event::ChunkUnloaded(at),
                Kind::Removed => Event::Removed(at),
                Kind::Update => Event::Update(at),
                Kind::Neighbor => Event::Neighbor(at),
                Kind::Capability(side) => Event::Capability(at, Face::ALL[*side]),
            }
        }
        GameEvent::Ticket { chunk, loaded, .. } => Event::Ticket {
            chunk: ChunkPos::new(chunk[0], chunk[1]),
            loaded: *loaded,
        },
        GameEvent::ServerTickPost { runs_normally } => Event::ServerTickPost {
            runs_normally: *runs_normally,
        },
        GameEvent::Reset => return None,
    })
}

impl World {
    fn new() -> Self {
        Self::with_chunks(CHUNKS)
    }

    fn with_chunks(chunks: i32) -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        Self {
            owner: EnergyTransmitters::new(
                DimensionId(0),
                move || {
                    uuid += 1;
                    Uuid::with_hash_code(uuid.wrapping_mul(0x1234_5679))
                },
                move |_| {
                    tile += 1;
                    tile.wrapping_mul(0x2545_F491)
                },
            ),
            game: Game {
                loaded: BTreeSet::new(),
                ring: BTreeSet::new(),
                chunks,
                ticking: None,
            },
            pass: 0,
            clears: 0,
            removals: 0,
        }
    }

    fn deliver(&mut self, events: Vec<GameEvent>) {
        for event in &events {
            if let Some(event) = owner_event(event) {
                match event {
                    Event::ClearRemoved(_) => self.clears += 1,
                    Event::Removed(_) => self.removals += 1,
                    _ => {}
                }
                self.owner.handle(&mut self.game, event);
            }
        }
    }

    /// Os cabos de um chunk da linha, como o jogo os lista.
    fn blocks_of(&self, chunk: i32) -> Vec<([i32; 3], Option<String>)> {
        (chunk * 16..chunk * 16 + 16)
            .filter(|x| (1..self.game.chunks * 16).contains(x))
            .map(|x| (cable_pos(x), Some(CABLE.to_owned())))
            .collect()
    }

    /// O Horizon solta o chunk (o `chunk_unload` com os block entities ainda no lugar); `hold` diz que o original o seguraria.
    fn unload(&mut self, chunk: i32, hold: bool) {
        let blocks = self.blocks_of(chunk);
        let events = events::chunk_unloading_with(OW, [chunk, 0], &blocks, hold);
        self.deliver(events);
        self.game.loaded.remove(&chunk);
    }

    /// O fim do tick do nível: o original solta o que já não segura (`tickets.retains`), antes dos tickets do anel e do `Post`.
    fn release(&mut self, tickets: &LoadingTickets) {
        let (_, events) = events::release_held(OW, |chunk| tickets.retains(chunk));
        self.deliver(events);
    }

    /// O chunk carrega (o `chunk_load` do Horizon): os cabos dele entram na fila e o `ChunkEvent.Load` sai com o lado do anel em que o chunk está agora.
    fn load(&mut self, chunk: i32) {
        let blocks = self.blocks_of(chunk);
        let first = events::translate(OW, vec![Raw::LoadChunk(blocks)]);
        assert!(first.is_empty());
        self.game.loaded.insert(chunk);
        let accessible = self.game.ring.contains(&chunk);
        let events = events::chunk_loaded_with(OW, [chunk, 0], accessible);
        self.deliver(events);
    }

    /// Uma passada do servidor: o `Update` de cada cabo de um chunk que anda (carregado e no anel), depois os tickets do anel e o `Post`.
    fn tick(&mut self) {
        self.pass += 1;
        self.owner.set_game_time(self.pass);
        let walking: Vec<i32> = self
            .game
            .loaded
            .iter()
            .copied()
            .filter(|chunk| self.game.ring.contains(chunk))
            .filter(|chunk| {
                self.game
                    .ticking
                    .as_ref()
                    .is_none_or(|set| set.contains(chunk))
            })
            .collect();
        for chunk in walking {
            for x in chunk * 16..chunk * 16 + 16 {
                if (1..self.game.chunks * 16).contains(&x) {
                    let events = events::update(OW, cable_pos(x));
                    self.deliver(events);
                }
            }
        }
        let ring = self.game.ring.clone();
        // Como a cola: o anel diz se o chunk está nele (os tickets do Horizon), sem olhar se o chunk está carregado; o chunk segurado o rastreador não pergunta.
        let events = events::accessibility(OW, |chunk| ring.contains(&chunk[0]));
        self.deliver(events);
        self.deliver(vec![GameEvent::ServerTickPost {
            runs_normally: true,
        }]);
    }

    /// Os tamanhos das redes, da maior para a menor.
    fn networks(&self) -> Vec<usize> {
        let mut sizes: Vec<usize> = self
            .owner
            .registry()
            .network_summaries()
            .into_iter()
            .map(|(_, transmitters, _)| transmitters)
            .collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes
    }
}

/// O número de cabos dos chunks de `first` a `last` numa linha de `chunks` chunks.
fn cables_of(chunks: i32, first: i32, last: i32) -> usize {
    (first..=last)
        .map(|chunk| {
            (chunk * 16..chunk * 16 + 16)
                .filter(|x| (1..chunks * 16).contains(x))
                .count()
        })
        .sum()
}

/// O número de cabos dos chunks de `first` a `last` da linha de `CHUNKS` chunks.
fn cables_in(first: i32, last: i32) -> usize {
    cables_of(CHUNKS, first, last)
}

#[test]
fn a_line_whose_chunks_load_in_the_ring_forms_one_network() {
    let _world = begin();
    let mut world = World::new();
    world.game.ring = (0..CHUNKS).collect();
    for chunk in 0..CHUNKS {
        world.load(chunk);
    }
    for _ in 0..5 {
        world.tick();
    }
    assert_eq!(world.networks(), vec![cables_in(0, CHUNKS - 1)]);
}

#[test]
fn chunks_that_load_outside_the_ring_join_when_the_ring_reaches_them() {
    let _world = begin();
    let mut world = World::new();
    world.game.ring = (0..6).collect();
    for chunk in 0..CHUNKS {
        world.load(chunk);
    }
    for _ in 0..5 {
        world.tick();
    }
    // Os chunks de 6 em diante carregaram parados (nível 33): os cabos deles nem entraram.
    assert_eq!(world.networks(), vec![cables_in(0, 5)]);
    world.game.ring = (0..CHUNKS).collect();
    for _ in 0..5 {
        world.tick();
    }
    assert_eq!(world.networks(), vec![cables_in(0, CHUNKS - 1)]);
}

#[test]
fn the_chunks_that_leave_the_ring_leave_the_network_while_loaded_and_come_back_with_it() {
    // O afastar e o voltar da decisão 198 (352 e 287 cabos no original): o chunk segue carregado, mas o ticket passou de 32 e os cabos dele saem da rede.
    let _world = begin();
    let mut world = World::new();
    world.game.ring = (0..CHUNKS).collect();
    for chunk in 0..CHUNKS {
        world.load(chunk);
    }
    for _ in 0..5 {
        world.tick();
    }
    assert_eq!(world.networks(), vec![cables_in(0, CHUNKS - 1)]);
    world.game.ring = (3..CHUNKS).collect();
    for _ in 0..5 {
        world.tick();
    }
    assert_eq!(
        world.game.loaded.len(),
        CHUNKS as usize,
        "os chunks seguem carregados"
    );
    assert_eq!(world.networks(), vec![cables_in(3, CHUNKS - 1)]);
    world.game.ring = (0..CHUNKS).collect();
    for _ in 0..5 {
        world.tick();
    }
    assert_eq!(world.networks(), vec![cables_in(0, CHUNKS - 1)]);
}

#[test]
fn a_ring_that_flickers_while_the_chunks_load_ends_with_one_network() {
    // Cada ordem de carga e cada oscilação do anel de uma lista de sorteios, e no fim o anel parado: uma rede só com tudo o que anda (a abertura partida da decisão 198).
    let _world = begin();
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..300 {
        events::reset();
        events::register(CABLE);
        let mut world = World::new();
        let mut pending: Vec<i32> = (0..CHUNKS).collect();
        let mut ring_hi = 3;
        while !pending.is_empty() {
            // Até três chunks carregam por passada, o anel cresce e às vezes encolhe um passo.
            for _ in 0..(next() % 4) {
                if pending.is_empty() {
                    break;
                }
                let index = (next() as usize) % pending.len();
                let chunk = pending.remove(index);
                world.load(chunk);
            }
            match next() % 5 {
                0 | 1 => ring_hi = (ring_hi + 1).min(CHUNKS - 1),
                2 => ring_hi = (ring_hi - 1).max(2),
                _ => {}
            }
            world.game.ring = (0..=ring_hi).collect();
            world.tick();
        }
        world.game.ring = (0..CHUNKS - 1).collect();
        for _ in 0..8 {
            world.tick();
        }
        assert_eq!(
            world.networks(),
            vec![cables_in(0, CHUNKS - 2)],
            "rodada {round}: anel final de 0 a {}",
            CHUNKS - 2
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// O chunk que o Horizon solta e o original segura (decisão 199): o roteiro dos setups reais (afastar, voltar, `tp` a x = 100)
// ---------------------------------------------------------------------------------------------------------------------

/// A distância de visão do servidor, a de simulação e o raio de chunks que o Horizon mantém (visão mais 4).
const VIEW: i32 = 16;
const SIMULATION: i32 = 13;
const RESIDENT: i32 = VIEW + 4;

fn tickets_of(player: i32) -> LoadingTickets {
    LoadingTickets {
        players: vec![[player, 0]],
        view_distance: VIEW,
        ..LoadingTickets::default()
    }
}

impl World {
    /// O jogador vai ao chunk `player`: o Horizon carrega os chunks do raio residente (primeiro fora do anel de antes, como o original, que os carrega no nível 33 e só depois os promove)
    /// e solta os que saíram dele (segurando os que o original ainda teria, se `hold`); o anel acessível (nível 32 ou menos) e a simulação seguem o jogador, e o original solta o que já
    /// não segura. `ticks` passadas de servidor depois.
    fn follow(&mut self, player: i32, hold: bool, ticks: usize) {
        let tickets = tickets_of(player);
        // Os tickets de carga mudam primeiro (o jogador se moveu): o chunk que carrega já nasce no lado do anel em que está, como na cola (`chunk_accessible`).
        self.game.ring = (0..self.game.chunks)
            .filter(|chunk| tickets.level([*chunk, 0]) <= 32)
            .collect();
        self.game.ticking = Some(
            (0..self.game.chunks)
                .filter(|chunk| (chunk - player).abs() <= SIMULATION)
                .collect(),
        );
        // Uma passada com o anel novo e os chunks ainda como estavam (o Horizon carrega e solta nas passadas seguintes): o ticket de um chunk segurado não pode chegar antes de ele voltar.
        self.release(&tickets);
        self.tick();
        for chunk in 0..self.game.chunks {
            let resident = (chunk - player).abs() <= RESIDENT;
            if resident && !self.game.loaded.contains(&chunk) {
                self.load(chunk);
            } else if !resident && self.game.loaded.contains(&chunk) {
                self.unload(chunk, hold && tickets.retains([chunk, 0]));
            }
        }
        for _ in 0..ticks {
            self.release(&tickets);
            self.tick();
        }
    }
}

#[test]
fn the_ticket_levels_are_the_distance_managers_up_to_the_level_the_original_drops_the_chunk() {
    let tickets = tickets_of(0);
    assert_eq!(tickets.level([0, 0]), 31);
    assert_eq!(tickets.level([VIEW, 0]), 31);
    assert_eq!(tickets.level([VIEW + 1, 0]), 32, "o anel acessível");
    assert_eq!(
        tickets.level([VIEW + 2, 0]),
        33,
        "o chunk inteiro, inacessível"
    );
    assert_eq!(
        tickets.level([VIEW + 2, 5]),
        33,
        "a distância é a de Chebyshev"
    );
    assert!(tickets.retains([29, 0]), "o nível 44 ainda é segurado");
    assert!(!tickets.retains([30, 0]), "o 45 o original solta");
    let mut tickets = tickets_of(0);
    tickets.forced = vec![[100, 0]];
    tickets.portal = vec![[-50, 0]];
    tickets.dragon = Some([0, 200]);
    assert_eq!(tickets.level([100, 0]), 31, "um chunk forçado segura o 31");
    assert_eq!(tickets.level([110, 0]), 41);
    assert_eq!(tickets.level([-50, 0]), 30, "o ticket de portal é o 30");
    assert_eq!(tickets.level([-44, 0]), 36);
    assert_eq!(tickets.level([0, 200]), 24, "a arena do dragão é o 24");
    assert_eq!(tickets.level([0, 210]), 34);
    assert_eq!(
        LoadingTickets::default().level([0, 0]),
        i32::MAX,
        "sem tickets, o chunk não é segurado"
    );
}

#[test]
fn the_roteiro_of_the_setups_reais_ends_with_the_whole_line_in_the_network() {
    // O afastar, o voltar e o `tp` a x = 100 do original (decisão 199): os chunks 21 a 23 saem do raio residente do Horizon quando o jogador volta ao chunk 0 e o original os segura
    // (nível 36 a 38). No chunk 6 eles voltam ao anel acessível sem `clearRemoved`: o ticket de "carregado" os leva à rede, como no original (383 cabos), e não esperam um tick que
    // a distância de simulação não dá.
    let _world = begin();
    let mut world = World::with_chunks(LONG);
    world.follow(0, true, 6);
    // Os chunks 14 a 17 estão no anel mas fora da simulação: os cabos deles nunca andaram, e o ticket não os acha (decisão 189). Só entram na rede depois de andar, com o jogador longe.
    assert_eq!(
        world.networks(),
        vec![cables_of(LONG, 0, 13)],
        "o começo, no chunk 0 (223 no original)"
    );
    world.follow(20, true, 6);
    assert_eq!(
        world.networks(),
        vec![cables_of(LONG, 3, 23)],
        "afastado, no chunk 20"
    );
    world.follow(0, true, 6);
    assert_eq!(
        world.networks(),
        vec![cables_of(LONG, 0, 17)],
        "de volta ao chunk 0 (287 no original)"
    );
    assert_eq!(
        events::held_chunks(OW),
        vec![[21, 0], [22, 0], [23, 0]],
        "os chunks 21 a 23 o Horizon soltou e o original segura"
    );
    let clears = world.clears;
    let removals = world.removals;
    world.follow(6, true, 6);
    assert_eq!(
        world.networks(),
        vec![cables_of(LONG, 0, 23)],
        "no chunk 6, o Network Reader lê a linha inteira"
    );
    assert_eq!(
        world.clears, clears,
        "os cabos que voltam não recebem outro `clearRemoved`"
    );
    assert_eq!(
        world.removals, removals,
        "e nenhum `setRemoved` os tirou antes"
    );
    assert!(events::held_chunks(OW).is_empty());
}

#[test]
fn the_chunks_the_original_still_holds_are_released_when_it_drops_them() {
    let _world = begin();
    let mut world = World::with_chunks(LONG);
    world.follow(0, true, 6);
    world.follow(20, true, 6);
    world.follow(0, true, 6);
    assert_eq!(events::held_chunks(OW).len(), 3);
    let removals = world.removals;
    // O jogador vai longe: o nível de ticket dos chunks 21 a 23 passa de 44 e o original os solta (`ChunkEvent.Unload`, `onChunkUnloaded` e `setRemoved` de cada cabo).
    let far = tickets_of(200);
    world.release(&far);
    assert!(events::held_chunks(OW).is_empty());
    assert_eq!(world.removals - removals, cables_of(LONG, 21, 23));
    assert!(
        world.owner.positions().iter().all(|pos| pos.x >> 4 < 21),
        "os cabos dos chunks soltos saíram do dono"
    );
}

#[test]
fn a_chunk_the_original_no_longer_holds_is_unloaded_at_once() {
    // Sem segurar (`hold` falso, o chunk já passou do nível 44 quando o Horizon o solta), a saída é a de sempre, no mesmo tick.
    let _world = begin();
    let mut world = World::with_chunks(LONG);
    world.follow(0, false, 6);
    world.follow(20, false, 6);
    let removals = world.removals;
    world.follow(0, false, 6);
    assert!(events::held_chunks(OW).is_empty());
    assert_eq!(world.removals - removals, cables_of(LONG, 21, 23));
}

#[test]
fn a_chunk_the_horizon_drops_inside_the_ring_leaves_the_network_and_waits_held() {
    // O jogador se afasta de uma vez (`tp`): chunks que estavam no anel acessível saem do raio residente do Horizon no mesmo tick. O original tira os cabos da rede (o ticket passa de 32)
    // e segura os chunks (nível até 44): sem o ticket de saída do anel, os cabos soltos seguiriam na rede.
    let _world = begin();
    let mut world = World::with_chunks(LONG);
    world.follow(0, true, 6);
    world.follow(20, true, 6);
    assert_eq!(world.networks(), vec![cables_of(LONG, 3, 23)]);
    world.follow(-3, true, 6);
    assert_eq!(
        events::held_chunks(OW),
        (18..LONG).map(|chunk| [chunk, 0]).collect::<Vec<_>>()
    );
    assert_eq!(world.networks(), vec![cables_of(LONG, 0, 14)]);
}

#[test]
fn the_ticket_levels_and_the_drop_level_match_the_originals_trace() {
    // `dev/referencia/rastros/setups_niveis.jsonl` (`CenaSetups.java`, o cliente do original, decisão 199): de cada chunk da linha o nível de ticket do `ChunkHolder` e o que ele guarda
    // (`L` o `LevelChunk` com os block entities) com o jogador no chunk 0 e no chunk 6, e os chunks (cz = 0) que o `ChunkMap.processUnloads` soltou.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/setups_niveis.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut levels_checked = 0;
    let mut retained_checked = 0;
    let mut unloaded_checked = 0;
    for line in text.lines() {
        let row: serde_json::Value = serde_json::from_str(line).expect("json");
        let tickets = tickets_of(row["jogador_chunk"].as_i64().expect("jogador") as i32);
        assert_eq!(row["vista"].as_i64(), Some(i64::from(VIEW)));
        for entry in row["niveis"].as_array().into_iter().flatten() {
            let chunk = entry[0].as_i64().expect("cx") as i32;
            assert_eq!(
                i64::from(tickets.level([chunk, 0])),
                entry[1].as_i64().expect("nível"),
                "chunk {chunk}"
            );
            levels_checked += 1;
            if entry[2] == "L" {
                // O chunk inteiro que o original tinha na memória é o que a lib segura.
                assert!(
                    tickets.retains([chunk, 0]),
                    "chunk {chunk} com `LevelChunk` e nível {}",
                    entry[1]
                );
                retained_checked += 1;
            }
        }
        for chunk in row["descarregados_cz0"].as_array().into_iter().flatten() {
            let chunk = chunk[1].as_i64().expect("cx") as i32;
            assert!(
                !tickets.retains([chunk, 0]),
                "o original soltou o chunk {chunk}"
            );
            unloaded_checked += 1;
        }
    }
    assert!(levels_checked >= 40, "níveis conferidos: {levels_checked}");
    assert!(
        retained_checked >= 30,
        "chunks segurados conferidos: {retained_checked}"
    );
    assert_eq!(unloaded_checked, 9);
}
