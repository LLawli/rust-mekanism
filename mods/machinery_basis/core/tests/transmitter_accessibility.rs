//! A abertura de um mundo com uma linha longa de cabos que passa por vários chunks, na ordem que a cola da `machinery_basis` entrega (decisão 198): os chunks carregam, os cabos dos que andam
//! entram na rede no `Update` do tick, o anel acessível (o ticket de carga de 32 ou menos do original) muda entre as passadas e o `Post` trata os chunks que mudaram de lado.
//! O módulo de eventos (`transmitter_events.rs`) e o dono dos cabos (`EnergyTransmitters`) rodam como estão; o jogo é simulado por trás. Um arquivo só, porque o módulo de eventos guarda o
//! estado em `static`.
#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}
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
        (pos.z == 0 && (1..CHUNKS * 16).contains(&pos.x)).then_some(CableInfo {
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
            },
            pass: 0,
        }
    }

    fn deliver(&mut self, events: Vec<GameEvent>) {
        for event in &events {
            if let Some(event) = owner_event(event) {
                self.owner.handle(&mut self.game, event);
            }
        }
    }

    /// O chunk carrega (o `chunk_load` do Horizon): os cabos dele entram na fila e o `ChunkEvent.Load` sai com o lado do anel em que o chunk está agora.
    fn load(&mut self, chunk: i32) {
        let blocks: Vec<_> = (chunk * 16..chunk * 16 + 16)
            .filter(|x| (1..CHUNKS * 16).contains(x))
            .map(|x| (cable_pos(x), Some(CABLE.to_owned())))
            .collect();
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
            .collect();
        for chunk in walking {
            for x in chunk * 16..chunk * 16 + 16 {
                if (1..CHUNKS * 16).contains(&x) {
                    let events = events::update(OW, cable_pos(x));
                    self.deliver(events);
                }
            }
        }
        let ring = self.game.ring.clone();
        let loaded = self.game.loaded.clone();
        let events = events::accessibility(OW, |chunk| {
            loaded.contains(&chunk[0]) && ring.contains(&chunk[0])
        });
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

/// O número de cabos dos chunks de `first` a `last`.
fn cables_in(first: i32, last: i32) -> usize {
    (first..=last)
        .map(|chunk| {
            (chunk * 16..chunk * 16 + 16)
                .filter(|x| (1..CHUNKS * 16).contains(x))
                .count()
        })
        .sum()
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
