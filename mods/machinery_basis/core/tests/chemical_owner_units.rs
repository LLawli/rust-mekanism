//! O dono dos tubos pressurizados (`transmission::resource_owner` com o químico como recurso) pelos eventos do jogo, num mundo simulado simples (o que a cola faz em jogo): o `PULL` de uma
//! fonte até um vizinho que só aceita o químico que uma receita usa (o tanque de infusão do Metallurgic Infuser: `containsRecipeBA`, entra de fora por um lado `INPUT`), o lado
//! que não deixa entrar ou sair (`ConfigHolder.canInsert` e `canExtract`), o NBT que o tubo grava e lê, e o `disperse` do último tubo. Os cenários tick a tick contra o original são do
//! `chemical_network_trace.rs`, que passa pelo mesmo dono.
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::distribution::ResourceInsert;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::chemical_network::{
    ChemicalHandler, ChemicalTransmitters, chemical_transmitters,
};
use machinery_basis_core::transmission::connection::{ConnectionType, TransmissionType};
use machinery_basis_core::transmission::host_resource::{HostHandler, ResourceSnapshot};
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::pressurized_tube::TubeTier;
use machinery_basis_core::transmission::resource_owner::{PipeInfo, ResourceOwnerWorld};
use machinery_basis_core::transmission::transmitter::{Compound, Overlay};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::collections::HashMap;
use std::rc::Rc;

const CARBON: &str = "mekanism:carbon";
const HYDROGEN: &str = "mekanism:hydrogen";
const WASTE: &str = "mekanism:nuclear_waste";

fn chemical(id: &str) -> ChemicalResource {
    ChemicalResource::new(id)
}

enum Block {
    Tube(Option<(Compound, StackContents<ChemicalResource>)>),
    Tank(Rc<HostHandler<ChemicalResource>>),
}

#[derive(Default)]
struct Level {
    blocks: HashMap<BlockPos, Block>,
    published: Vec<(BlockPos, CableView)>,
}

impl ResourceOwnerWorld<ChemicalResource> for Level {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    fn handler(&self, pos: BlockPos, _: Face) -> Option<ChemicalHandler> {
        match self.blocks.get(&pos)? {
            Block::Tank(handler) => Some(handler.clone()),
            Block::Tube(_) => None,
        }
    }

    fn pipe_info(&mut self, pos: BlockPos) -> Option<PipeInfo<ChemicalResource>> {
        match self.blocks.get_mut(&pos)? {
            Block::Tube(saved) => Some(PipeInfo {
                capacity: TubeTier::Basic.capacity(),
                transfer_rate: TubeTier::Basic.transfer_rate(),
                saved: saved.take(),
            }),
            Block::Tank(_) => None,
        }
    }

    fn publish(&mut self, pos: BlockPos, view: &CableView) {
        self.published.push((pos, view.clone()));
    }

    fn overlay(&mut self, _: BlockPos, _: Overlay) {}
}

struct Sim {
    owner: ChemicalTransmitters,
    level: Level,
    time: i64,
}

impl Sim {
    fn new() -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        Self {
            owner: chemical_transmitters(
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
        }
    }

    fn send(&mut self, event: Event) {
        self.owner.handle(&mut self.level, event);
    }

    fn pos(x: i32) -> BlockPos {
        BlockPos::new(x, 0, 0)
    }

    fn neighbors(&mut self, pos: BlockPos) {
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.pipe_at(neighbor).is_some() {
                self.send(Event::Capability(neighbor, side.opposite()));
            }
        }
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.pipe_at(neighbor).is_some() {
                self.send(Event::Neighbor(neighbor));
            }
        }
    }

    fn place_tube(&mut self, x: i32, saved: Option<(Compound, StackContents<ChemicalResource>)>) {
        let pos = Self::pos(x);
        self.level.blocks.insert(pos, Block::Tube(saved));
        self.send(Event::ClearRemoved(pos));
        self.neighbors(pos);
    }

    fn place_tank(
        &mut self,
        x: i32,
        snapshot: ResourceSnapshot<ChemicalResource>,
    ) -> Rc<HostHandler<ChemicalResource>> {
        let pos = Self::pos(x);
        let handler = Rc::new(HostHandler::new(snapshot));
        self.level.blocks.insert(pos, Block::Tank(handler.clone()));
        self.neighbors(pos);
        handler
    }

    fn tick(&mut self) {
        for pos in self.owner.positions() {
            self.send(Event::Update(pos));
        }
        self.owner.set_game_time(self.time);
        self.send(Event::ServerTickPost {
            runs_normally: true,
        });
        self.time += 1;
    }

    fn network_amount(&self, x: i32) -> u64 {
        let network = self.owner.network_of(Self::pos(x)).unwrap();
        self.owner
            .family()
            .network(network)
            .unwrap()
            .buffer()
            .amount
    }
}

fn line(sim: &mut Sim, count: i32) {
    for x in 0..count {
        sim.place_tube(x, None);
    }
    for _ in 0..4 {
        sim.tick();
    }
}

/// Um tanque criativo cheio (o Chemical Tank criativo): entrega e aceita sem mudar.
fn source(id: &str) -> ResourceSnapshot<ChemicalResource> {
    ResourceSnapshot {
        contents: StackContents::new(Some(chemical(id)), i64::MAX as u64),
        capacity: i64::MAX as u64,
        accepts: Rc::new(|_| true),
        can_insert: true,
        can_extract: true,
        creative: true,
    }
}

/// O tanque de infusão do Metallurgic Infuser por um lado `INPUT`: o que não é `carbon` não tem receita (`containsRecipeBA`), entra de fora e ninguém de fora tira.
fn infusion_tank(amount: u64) -> ResourceSnapshot<ChemicalResource> {
    ResourceSnapshot {
        contents: StackContents::new(Some(chemical(CARBON)), amount),
        capacity: 1_000,
        accepts: Rc::new(|chemical: &ChemicalResource| &*chemical.id == CARBON),
        can_insert: true,
        can_extract: false,
        creative: false,
    }
}

fn pull_from_the_west(sim: &mut Sim) {
    sim.owner.set_mode(
        &mut sim.level,
        Sim::pos(0),
        Face::West,
        ConnectionType::Pull,
    );
}

fn run_until_full(sim: &mut Sim, tank: &HostHandler<ChemicalResource>) -> u64 {
    let mut delivered = 0;
    for _ in 0..30 {
        sim.tick();
        // O fim do evento: o que a rede mudou no espelho vai para o contêiner do jogo.
        if let Some(contents) = tank.changed() {
            delivered = contents.amount;
            tank.settle();
        }
    }
    delivered
}

#[test]
fn a_pulling_tube_carries_carbon_to_the_infusion_tank_and_stops_when_it_is_full() {
    let mut sim = Sim::new();
    let source = sim.place_tank(-1, source(CARBON));
    line(&mut sim, 4);
    let tank = sim.place_tank(4, infusion_tank(0));
    for _ in 0..3 {
        sim.tick();
    }
    pull_from_the_west(&mut sim);
    let delivered = run_until_full(&mut sim, &tank);
    assert_eq!(delivered, 1_000, "o tanque enche até a capacidade");
    assert_eq!(tank.contents().amount, 1_000);
    // A fonte criativa não perde nada, e o tanque cheio deixa o resto na rede.
    assert_eq!(source.contents().amount, i64::MAX as u64);
    assert!(sim.network_amount(0) > 0);
    // A máquina gasta 100 e o jogo devolve o tanque com 900: a rede reenche.
    tank.refresh(infusion_tank(900));
    sim.tick();
    assert_eq!(tank.contents().amount, 1_000);
}

#[test]
fn the_infusion_tank_takes_nothing_a_recipe_does_not_use() {
    let mut sim = Sim::new();
    sim.place_tank(-1, source(HYDROGEN));
    line(&mut sim, 4);
    let tank = sim.place_tank(4, infusion_tank(0));
    for _ in 0..3 {
        sim.tick();
    }
    pull_from_the_west(&mut sim);
    assert_eq!(run_until_full(&mut sim, &tank), 0);
    assert_eq!(tank.contents().amount, 0);
    // O hidrogênio fica na rede (a rede aceita o que o `PULL` traz).
    assert!(sim.network_amount(0) > 0);
    let transaction = Transaction::open_root();
    assert_eq!(tank.insert(&chemical(HYDROGEN), 100, &transaction), 0);
}

#[test]
fn a_side_that_does_not_let_chemicals_in_or_out_gives_the_tube_nothing() {
    let mut sim = Sim::new();
    sim.place_tank(-1, source(CARBON));
    line(&mut sim, 4);
    // Um lado `OUTPUT` do tanque: `canInsert` falso. A rede não entrega.
    let closed = ResourceSnapshot {
        can_insert: false,
        can_extract: true,
        contents: StackContents::new(Some(chemical(CARBON)), 500),
        ..infusion_tank(0)
    };
    let tank = sim.place_tank(4, closed);
    for _ in 0..3 {
        sim.tick();
    }
    pull_from_the_west(&mut sim);
    run_until_full(&mut sim, &tank);
    assert_eq!(tank.contents().amount, 500);
}

#[test]
fn the_tube_writes_its_share_and_reads_it_back_when_the_chunk_comes_back() {
    let mut sim = Sim::new();
    line(&mut sim, 3);
    assert_eq!(
        sim.owner
            .insert_external(Sim::pos(0), &chemical(HYDROGEN), 9_001),
        9_001
    );
    sim.tick();
    let saved: Vec<_> = (0..3)
        .map(|x| sim.owner.write(Sim::pos(x)).expect("o tubo está no dono"))
        .collect();
    let total: u64 = saved.iter().map(|(_, stored)| stored.amount).sum();
    assert_eq!(total, 9_001);
    assert!(
        saved
            .iter()
            .all(|(_, stored)| stored.resource == Some(chemical(HYDROGEN)))
    );
    let mut reopened = Sim::new();
    for (x, saved) in saved.into_iter().enumerate() {
        reopened.place_tube(x as i32, Some(saved));
    }
    for _ in 0..6 {
        reopened.tick();
    }
    assert_eq!(reopened.network_amount(0), 9_001);
}

#[test]
fn the_last_tube_removed_registers_the_dispersal_and_nobody_delivers_it() {
    let mut sim = Sim::new();
    line(&mut sim, 1);
    assert_eq!(
        sim.owner
            .insert_external(Sim::pos(0), &chemical(WASTE), 700),
        700
    );
    sim.tick();
    assert!(sim.owner.family_mut().take_dispersals().is_empty());
    sim.level.blocks.remove(&Sim::pos(0));
    sim.send(Event::Removed(Sim::pos(0)));
    // A rede só se desfaz no `onTick` do registro (`removeInvalidTransmitters`).
    for _ in 0..3 {
        sim.tick();
    }
    let dispersals = sim.owner.family_mut().take_dispersals();
    assert_eq!(dispersals.len(), 1);
    assert_eq!(dispersals[0].pos, Sim::pos(0));
    assert_eq!(dispersals[0].resource, chemical(WASTE));
    assert_eq!(dispersals[0].amount, 700);
}

#[test]
fn a_tube_carries_chemicals_and_nothing_else() {
    // `PressurizedTube`: `TransmissionType.CHEMICAL`, o que decide com quem um tubo se liga (`supportsTransmissionType`).
    let mut sim = Sim::new();
    sim.place_tube(0, None);
    let supported = sim
        .owner
        .transmitter(Sim::pos(0))
        .unwrap()
        .supported_transmission_types();
    assert_eq!(
        supported.iter().collect::<Vec<_>>(),
        [TransmissionType::Chemical]
    );
}
