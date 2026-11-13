//! O `ServerTickEvent.Post` com duas dimensões (decisão 134): `TransmitterNetworkRegistry.onTick` é um só e trata as redes de todas as dimensões depois do tick de todas; aqui cada
//! dimensão tem o registro dela (`EnergyTransmitters`), e a cola entra em cada nível que o servidor ticou para lhe entregar o `ServerTickPost`, uma vez por tick do servidor. O teste
//! monta duas dimensões simuladas (o overworld e o Nether), cada uma com um cabo em linha, uma fonte e uma máquina, e roda os ticks pelo que a cola roda (`deliver_server_tick_post`).
//! Um arquivo só, porque o módulo de eventos guarda o estado em `static`.
#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}
use events::{Event as GameEvent, Kind, Raw, ServerLevels};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

const CABLE: &str = "test:cable";
const OVERWORLD: &str = "minecraft:overworld";
const NETHER: &str = "minecraft:the_nether";
const END: &str = "minecraft:the_end";

/// O módulo de eventos é um `static` do processo: um teste por vez, cada um com o mundo recém-aberto e o tipo de cabo registrado.
static SERIAL: Mutex<()> = Mutex::new(());

fn begin() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    events::reset();
    events::register(CABLE);
    guard
}

struct Machine {
    container: BufferEnergyContainer,
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }
}

/// O mundo de uma dimensão: três cabos em linha (x de 0 a 2) e uma máquina ao lado do último (x = 3).
struct Level {
    machine: Rc<Machine>,
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    fn energy_handler(&self, pos: BlockPos, _: Face) -> Option<Acceptor> {
        (pos == BlockPos::new(3, 64, 0)).then(|| {
            let machine: Acceptor = self.machine.clone();
            machine
        })
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        (pos.x < 3).then_some(CableInfo {
            capacity: 8_000,
            saved: None,
        })
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}
}

struct Dimension {
    owner: EnergyTransmitters,
    level: Level,
}

/// Os níveis do servidor: o que a cola faz com `ticked`, `current`, `enter` e `deliver`, mais o registro do que aconteceu.
struct Server {
    dimensions: HashMap<String, Dimension>,
    ticked: Vec<String>,
    current: String,
    /// Os níveis em que a cola entrou, na ordem.
    entered: Vec<String>,
    /// Os `ServerTickPost` que cada dimensão recebeu.
    posts: HashMap<String, u32>,
    /// A ordem em que os `ServerTickPost` saíram.
    post_order: Vec<String>,
    /// Um nível em que `enter` falha.
    broken: Option<String>,
    time: i64,
    /// `runsNormally` do tick.
    runs_normally: bool,
}

impl ServerLevels for Server {
    fn ticked(&self) -> Vec<String> {
        self.ticked.clone()
    }

    fn current(&self) -> String {
        self.current.clone()
    }

    fn enter(&mut self, dim: &str) -> bool {
        if self.broken.as_deref() == Some(dim) {
            return false;
        }
        self.entered.push(dim.to_owned());
        self.current = dim.to_owned();
        true
    }

    fn deliver(&mut self, events: Vec<GameEvent>) {
        for event in events {
            let GameEvent::ServerTickPost { runs_normally } = event else {
                panic!("só o fim do tick passa por aqui: {event:?}");
            };
            let current = self.current.clone();
            *self.posts.entry(current.clone()).or_default() += 1;
            self.post_order.push(current.clone());
            let dimension = self.dimensions.get_mut(&current);
            let Some(Dimension { owner, level }) = dimension else {
                panic!("a dimensão {current} não tem dono");
            };
            owner.set_game_time(self.time);
            owner.handle(level, Event::ServerTickPost { runs_normally });
        }
    }
}

fn pos(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 0)
}

fn raw(x: i32) -> Raw {
    Raw::Added {
        pos: [x, 64, 0],
        id: Some(CABLE.to_owned()),
    }
}

impl Server {
    /// Um servidor com as dimensões dadas, cada uma com os três cabos postos (o `clearRemoved` que a fila traduz), e `current` no lugar.
    fn new(dimensions: &[&str], current: &str) -> Self {
        let mut server = Self {
            dimensions: HashMap::new(),
            ticked: dimensions.iter().map(|dim| (*dim).to_owned()).collect(),
            current: current.to_owned(),
            entered: Vec::new(),
            posts: HashMap::new(),
            post_order: Vec::new(),
            broken: None,
            time: 0,
            runs_normally: true,
        };
        for dim in dimensions {
            let mut uuid = 0i32;
            let mut tile = 0i32;
            server.dimensions.insert(
                (*dim).to_owned(),
                Dimension {
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
                    level: Level {
                        machine: Rc::new(Machine {
                            container: BufferEnergyContainer::new(1_000_000, always, always, None),
                        }),
                    },
                },
            );
        }
        server
    }

    /// Põe os cabos de uma dimensão como o jogo faz (`clearRemoved` de cada um, que a fila de ocorrências traduz).
    fn place_cables(&mut self, dim: &str) {
        let events = events::translate(dim, (0..3).map(raw).collect());
        for event in events {
            if let GameEvent::Transmitter {
                pos: at,
                kind: Kind::ClearRemoved,
                ..
            } = event
            {
                let Dimension { owner, level } = self.dimensions.get_mut(dim).unwrap();
                owner.handle(
                    level,
                    Event::ClearRemoved(BlockPos::new(at[0], at[1], at[2])),
                );
            }
        }
    }

    /// Um tick do servidor: cada nível ticado anda os block entities dos cabos (`onUpdateServer`), e no fim o `ServerTickPost` de todos.
    fn tick(&mut self) {
        for dim in self.ticked.clone() {
            let Dimension { owner, level } = self.dimensions.get_mut(&dim).unwrap();
            for x in 0..3 {
                if owner.cable_at(pos(x)).is_some() {
                    owner.handle(level, Event::Update(pos(x)));
                }
            }
        }
        let runs_normally = self.runs_normally;
        events::deliver_server_tick_post(self, runs_normally);
        self.time += 1;
    }

    fn machine_energy(&self, dim: &str) -> i64 {
        self.dimensions[dim].level.machine.container.amount()
    }

    /// A fonte empurra energia para o primeiro cabo da dimensão (um vizinho que dá, pela capability do cabo).
    fn push(&mut self, dim: &str, amount: i32) {
        let dimension = self.dimensions.get_mut(dim).unwrap();
        dimension.owner.insert_external(pos(0), amount);
    }
}

#[test]
fn every_dimension_with_cables_gets_its_tick_once_per_server_tick() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD, NETHER], NETHER);
    server.place_cables(OVERWORLD);
    server.place_cables(NETHER);
    // Os cabos entram na rede no tick dos registros; a fonte empurra em cada dimensão, e os dois lados são iguais.
    for _ in 0..3 {
        server.tick();
    }
    for dim in [OVERWORLD, NETHER] {
        server.push(dim, 5_000);
    }
    for _ in 0..4 {
        server.tick();
    }
    let overworld = server.machine_energy(OVERWORLD);
    let nether = server.machine_energy(NETHER);
    assert!(overworld > 0, "a máquina do overworld recebeu {overworld}");
    assert_eq!(
        overworld, nether,
        "o Nether, que não é o nível em que o jogo está, recebe o mesmo que o overworld"
    );
    assert_eq!(overworld, 5_000, "a energia toda chegou à máquina");
    assert_eq!(server.posts[OVERWORLD], 7, "um `ServerTickPost` por tick");
    assert_eq!(server.posts[NETHER], 7, "um `ServerTickPost` por tick");
}

#[test]
fn the_level_in_place_goes_last_and_is_back_in_place_after() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD, NETHER, END], NETHER);
    server.place_cables(OVERWORLD);
    server.place_cables(NETHER);
    server.place_cables(END);
    server.tick();
    assert_eq!(
        server.post_order,
        vec![OVERWORLD, END, NETHER],
        "o que estava no lugar fica por último, os outros na ordem de `levels`"
    );
    assert_eq!(server.current, NETHER, "o nível volta ao que estava");
    // Um nível só: nada troca.
    let mut one = Server::new(&[OVERWORLD], OVERWORLD);
    one.place_cables(OVERWORLD);
    one.tick();
    assert!(one.entered.is_empty(), "nenhum `enter` com um nível só");
    assert_eq!(one.posts[OVERWORLD], 1);
}

#[test]
fn a_dimension_without_cables_or_not_ticked_is_not_entered() {
    let _world = begin();
    // O End não tem cabo; o Nether tem, mas o servidor não o ticou neste tick (ninguém lá).
    let mut server = Server::new(&[OVERWORLD, END], OVERWORLD);
    server.dimensions.insert(
        NETHER.to_owned(),
        Server::new(&[NETHER], NETHER)
            .dimensions
            .remove(NETHER)
            .unwrap(),
    );
    server.place_cables(OVERWORLD);
    server.place_cables(NETHER);
    server.tick();
    assert_eq!(server.posts.get(OVERWORLD), Some(&1));
    assert_eq!(server.posts.get(END), None, "o End não tem transmissor");
    assert_eq!(server.posts.get(NETHER), None, "o Nether não foi ticado");
    assert!(
        server.entered.is_empty(),
        "nenhum nível trocado: {:?}",
        server.entered
    );
}

#[test]
fn a_level_that_cannot_be_entered_is_skipped_and_the_others_still_tick() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD, NETHER, END], END);
    for dim in [OVERWORLD, NETHER, END] {
        server.place_cables(dim);
    }
    server.broken = Some(NETHER.to_owned());
    server.tick();
    assert_eq!(
        server.posts.get(NETHER),
        None,
        "o nível que não entra fica sem"
    );
    assert_eq!(server.post_order, vec![OVERWORLD, END]);
    assert_eq!(server.current, END);
}

#[test]
fn a_world_that_closes_forgets_the_dimensions_it_saw() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD], OVERWORLD);
    server.place_cables(OVERWORLD);
    assert!(events::sees_dimension(OVERWORLD));
    let reset = events::reset();
    assert_eq!(reset, vec![GameEvent::Reset]);
    assert!(!events::sees_dimension(OVERWORLD));
    server.tick();
    assert_eq!(
        server.posts.get(OVERWORLD),
        None,
        "sem transmissor visto, nenhum tick"
    );
}

/// O servidor parado ainda chega a todas as dimensões (o registro trata chunks e órfãs), mas nenhuma rede entrega energia.
#[test]
fn a_frozen_server_tick_reaches_every_dimension_but_moves_no_energy() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD, NETHER], NETHER);
    server.place_cables(OVERWORLD);
    server.place_cables(NETHER);
    for _ in 0..3 {
        server.tick();
    }
    for dim in [OVERWORLD, NETHER] {
        server.push(dim, 5_000);
    }
    server.runs_normally = false;
    for _ in 0..3 {
        server.tick();
    }
    assert_eq!(server.machine_energy(OVERWORLD), 0);
    assert_eq!(server.machine_energy(NETHER), 0);
    assert_eq!(server.posts[OVERWORLD], 6);
    assert_eq!(server.posts[NETHER], 6);
    server.runs_normally = true;
    server.tick();
    assert!(server.machine_energy(NETHER) > 0);
}

/// O nível que estava no lugar volta mesmo quando ele não tem transmissor e não é entrado.
#[test]
fn the_level_in_place_comes_back_even_when_it_has_no_cables() {
    let _world = begin();
    let mut server = Server::new(&[OVERWORLD, NETHER], OVERWORLD);
    server.place_cables(NETHER);
    server.tick();
    assert_eq!(server.post_order, vec![NETHER]);
    assert_eq!(
        server.entered,
        vec![NETHER, OVERWORLD],
        "entra no Nether e volta ao overworld"
    );
    assert_eq!(server.current, OVERWORLD);
}
