//! O custo por tick da lib dos transmissores sem o jogo (frente `rede-desempenho`, decisão 202): uma pista de 223 cabos de energia (a de `dev/referencia/cenas/setups_reais.txt` que o
//! servidor mantém carregada) com a fonte numa ponta e três máquinas ao lado, movida pelos eventos do jogo (`Update` de cada cabo e `ServerTickPost`, o que a cola entrega a cada tick).
//! Diz se o custo por transmissor que o jogo mostrou (cerca de 0,1 ms por tick) é da lib ou da cola com o Horizon: `cargo test -p machinery-basis-core --test transmitters_tick_cost --release -- --nocapture`
//! imprime os microssegundos por tick e por cabo. O teste só confere o que a conta tem de dar (a energia chega às máquinas) e um teto folgado (o tick da lib, em modo de teste, não
//! passa de 20 ms por pista): o número exato depende da máquina.
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::transmitter::Overlay;
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

/// O comprimento da pista: os cabos do chunk 0 ao 13, os que o servidor mantém andando (decisão 196).
const LANE: i32 = 223;

struct Machine {
    container: BufferEnergyContainer,
    gives: bool,
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

#[derive(Default)]
struct Level {
    cables: HashMap<BlockPos, i64>,
    machines: HashMap<BlockPos, Rc<Machine>>,
    /// Quantas vezes o dono falou com o mundo (qualquer pergunta ou publicação).
    calls: std::cell::Cell<usize>,
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, _pos: BlockPos) -> bool {
        self.calls.set(self.calls.get() + 1);
        true
    }

    fn is_getting_powered(&self, _pos: BlockPos) -> bool {
        self.calls.set(self.calls.get() + 1);
        false
    }

    fn energy_handler(&self, pos: BlockPos, _side: Face) -> Option<Acceptor> {
        self.calls.set(self.calls.get() + 1);
        let machine: Acceptor = self.machines.get(&pos)?.clone();
        Some(machine)
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        self.calls.set(self.calls.get() + 1);
        Some(CableInfo {
            capacity: *self.cables.get(&pos)?,
            saved: None,
        })
    }

    fn publish(&mut self, _pos: BlockPos, _view: &CableView) {
        self.calls.set(self.calls.get() + 1);
    }

    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {
        self.calls.set(self.calls.get() + 1);
    }
}

struct Lane {
    owner: EnergyTransmitters,
    level: Level,
    time: i64,
}

impl Lane {
    fn new() -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        let mut lane = Self {
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
            level: Level::default(),
            time: 0,
        };
        for x in 0..LANE {
            let pos = BlockPos::new(x, -60, 0);
            lane.level.cables.insert(pos, 8_000);
            lane.owner.handle(&mut lane.level, Event::ClearRemoved(pos));
        }
        // Os aceitadores (Enrichment Chambers) ao lado da pista; a fonte (o Heat Generator) empurra energia no primeiro cabo a cada tick (`tick`).
        for x in [20, 100, 200] {
            let chamber = Rc::new(Machine {
                container: BufferEnergyContainer::new(10_000_000, always, always, None),
                gives: false,
            });
            lane.level
                .machines
                .insert(BlockPos::new(x, -60, 1), chamber);
        }
        // O que o jogo entrega quando os blocos vizinhos entram: a capability e o aviso de cada cabo ao lado de uma máquina.
        let touched: Vec<BlockPos> = lane.level.machines.keys().copied().collect();
        for pos in touched {
            for side in Face::ALL {
                let neighbor = pos.relative(side);
                if lane.owner.cable_at(neighbor).is_some() {
                    lane.owner.handle(
                        &mut lane.level,
                        Event::Capability(neighbor, side.opposite()),
                    );
                    lane.owner
                        .handle(&mut lane.level, Event::Neighbor(neighbor));
                }
            }
        }
        lane
    }

    /// Um tick do servidor, como a cola o entrega: o `Update` de cada cabo, e o `ServerTickPost`.
    fn tick(&mut self) {
        self.owner.insert_external(BlockPos::new(0, -60, 0), 4_000);
        for x in 0..LANE {
            self.owner
                .handle(&mut self.level, Event::Update(BlockPos::new(x, -60, 0)));
        }
        self.owner.set_game_time(self.time);
        self.owner.handle(
            &mut self.level,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
        self.time += 1;
    }
}

#[test]
fn the_energy_lane_ticks_in_the_library_far_under_the_cost_the_game_showed() {
    let mut lane = Lane::new();
    // Aquece: as ligações e a primeira distribuição.
    for _ in 0..40 {
        lane.tick();
    }
    let ticks = 200;
    let start = Instant::now();
    for _ in 0..ticks {
        lane.tick();
    }
    let per_tick = start.elapsed().as_secs_f64() / f64::from(ticks);
    println!(
        "pista de energia de {LANE} cabos: {:.1} us por tick, {:.3} us por cabo",
        per_tick * 1e6,
        per_tick * 1e6 / f64::from(LANE)
    );
    let received = lane.level.machines[&BlockPos::new(100, -60, 1)]
        .container
        .amount();
    assert!(
        received > 0,
        "a energia tem de chegar à máquina do meio da pista"
    );
    assert!(
        per_tick < 0.020,
        "o tick da lib passou de 20 ms: {per_tick}"
    );
}

/// O `Update` de um block entity que o dono não tem (a cola entrega o de cada transmissor a todos os donos, e a cola sai antes de entregar o que não é do dono, decisão 202) não faz nada
/// nele: nem fala com o mundo, nem muda a energia de uma rede, nem a posição que ele guarda. É o que torna esse atalho exato.
#[test]
fn an_update_of_a_block_entity_the_owner_does_not_have_changes_nothing() {
    let mut lane = Lane::new();
    for _ in 0..10 {
        lane.tick();
    }
    let positions = lane.owner.positions();
    let energy = lane.level.machines[&BlockPos::new(20, -60, 1)]
        .container
        .amount();
    let before = lane.level.calls.get();
    for foreign in [
        BlockPos::new(500, -60, 0),
        BlockPos::new(20, -60, 1),
        BlockPos::new(0, -60, -3),
    ] {
        assert!(lane.owner.cable_at(foreign).is_none());
        lane.owner.handle(&mut lane.level, Event::Update(foreign));
    }
    assert_eq!(
        lane.level.calls.get(),
        before,
        "o dono falou com o mundo por um bloco que não é dele"
    );
    assert_eq!(lane.owner.positions(), positions);
    assert_eq!(
        lane.level.machines[&BlockPos::new(20, -60, 1)]
            .container
            .amount(),
        energy
    );
}
