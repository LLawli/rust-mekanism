//! O dono dos canos mecânicos (`transmission::resource_owner`) pelos eventos do jogo, num mundo simulado simples (o que a cola faz em jogo, sem os chunks falsos dos rastros): a
//! capability do cano (o contêiner da rede aceita de fora, o buffer sem rede não), o Configurator, o `PULL` de um vizinho com tanque por um `HostHandler` até o `lavaTank` do Heat
//! Generator, e o NBT que o cano grava e lê. Os cenários tick a tick contra o original são do `fluid_network_trace.rs`, que passa pelo mesmo dono.
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::resource_tank::StackContents;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::Uuid;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::fluid_network::{
    FluidHandler, FluidTransmitters, fluid_transmitters,
};
use machinery_basis_core::transmission::host_resource::{HostHandler, ResourceSnapshot};
use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::resource_owner::{PipeInfo, ResourceOwnerWorld};
use machinery_basis_core::transmission::transmitter::{Compound, Overlay};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn lava() -> FluidResource {
    FluidResource::new("minecraft:lava")
}

fn water() -> FluidResource {
    FluidResource::new("minecraft:water")
}

enum Block {
    Pipe(Option<(Compound, StackContents<FluidResource>)>),
    Tank(Rc<HostHandler<FluidResource>>),
}

#[derive(Default)]
struct Level {
    blocks: HashMap<BlockPos, Block>,
    published: Vec<(BlockPos, CableView)>,
    overlays: Vec<Overlay>,
    /// Cada posição por que o dono perguntou `is_block_loaded`, na ordem.
    asked: RefCell<Vec<BlockPos>>,
}

impl ResourceOwnerWorld<FluidResource> for Level {
    fn is_block_loaded(&self, pos: BlockPos) -> bool {
        self.asked.borrow_mut().push(pos);
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    fn handler(&self, pos: BlockPos, _: Face) -> Option<FluidHandler> {
        match self.blocks.get(&pos)? {
            Block::Tank(handler) => Some(handler.clone()),
            Block::Pipe(_) => None,
        }
    }

    fn pipe_info(&mut self, pos: BlockPos) -> Option<PipeInfo<FluidResource>> {
        match self.blocks.get_mut(&pos)? {
            Block::Pipe(saved) => Some(PipeInfo {
                capacity: PipeTier::Basic.capacity(),
                transfer_rate: PipeTier::Basic.transfer_rate(),
                saved: saved.take(),
            }),
            Block::Tank(_) => None,
        }
    }

    fn publish(&mut self, pos: BlockPos, view: &CableView) {
        self.published.push((pos, view.clone()));
    }

    fn overlay(&mut self, _: BlockPos, overlay: Overlay) {
        self.overlays.push(overlay);
    }
}

struct Sim {
    owner: FluidTransmitters,
    level: Level,
    time: i64,
}

impl Sim {
    fn new() -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        Self {
            owner: fluid_transmitters(
                machinery_basis_core::transmission::DimensionId(0),
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

    /// O que o jogo entrega ao pôr ou tirar um bloco: a capability e o aviso dos canos vizinhos.
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

    fn place_pipe(&mut self, x: i32, saved: Option<(Compound, StackContents<FluidResource>)>) {
        let pos = Self::pos(x);
        self.level.blocks.insert(pos, Block::Pipe(saved));
        self.send(Event::ClearRemoved(pos));
        self.neighbors(pos);
    }

    fn place_tank(
        &mut self,
        x: i32,
        snapshot: ResourceSnapshot<FluidResource>,
    ) -> Rc<HostHandler<FluidResource>> {
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

    fn view(&self, x: i32) -> CableView {
        self.owner.view_at(&self.level, Self::pos(x)).unwrap()
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
        sim.place_pipe(x, None);
    }
    for _ in 0..4 {
        sim.tick();
    }
}

fn source(handler_fluid: FluidResource) -> ResourceSnapshot<FluidResource> {
    ResourceSnapshot {
        contents: StackContents::new(Some(handler_fluid), i64::MAX as u64),
        capacity: i64::MAX as u64,
        accepts: Rc::new(|_| true),
        can_insert: true,
        can_extract: true,
        creative: true,
        increment: 1,
    }
}

/// O `lavaTank` do Heat Generator: só lava, entra de fora, ninguém de fora tira.
fn lava_tank(amount: u64) -> ResourceSnapshot<FluidResource> {
    ResourceSnapshot {
        contents: StackContents::new(Some(lava()), amount),
        capacity: 1_000,
        accepts: Rc::new(|fluid| &*fluid.id == "minecraft:lava"),
        can_insert: true,
        can_extract: false,
        creative: false,
        increment: 1,
    }
}

#[test]
fn what_a_neighbor_pushes_goes_to_the_network_container_and_the_buffer_of_a_pipe_without_one_only_gives_in()
 {
    let mut sim = Sim::new();
    sim.place_pipe(0, None);
    // Sem rede o contêiner é o buffer do cano (`BasicFluidTank::create(capacity, notExternal, alwaysTrueBi, ..)`): de fora entra, e de fora ninguém tira.
    assert_eq!(sim.owner.insert_external(Sim::pos(0), &water(), 500), 500);
    let buffer = sim.owner.container_at(Sim::pos(0)).unwrap();
    let transaction = machinery_basis_core::transaction::Transaction::open_root();
    assert_eq!(
        buffer.extract(
            &water(),
            100,
            &transaction,
            machinery_basis_core::containers::AutomationType::External
        ),
        0
    );
    drop(transaction);
    for _ in 0..3 {
        sim.tick();
    }
    // O cano entrou na rede com o buffer dele (`absorbBuffer`).
    assert_eq!(sim.network_amount(0), 500);
    assert_eq!(sim.owner.insert_external(Sim::pos(0), &water(), 500), 500);
    assert_eq!(sim.network_amount(0), 1_000);
    // Outro fluido não entra numa rede que guarda água.
    assert_eq!(sim.owner.insert_external(Sim::pos(0), &lava(), 500), 0);
}

#[test]
fn a_neighbor_swapped_for_another_container_keeps_the_arm() {
    let mut sim = Sim::new();
    sim.place_pipe(0, None);
    sim.place_tank(1, lava_tank(0));
    for _ in 0..3 {
        sim.tick();
    }
    assert!(sim.view(0).connected[Face::East as usize]);
    // `Level.setBlock` troca o bloco do vizinho por outro com contêiner: a capability se invalida e o cano relê (o caldeirão de água vira o vazio).
    let handler = Rc::new(HostHandler::new(lava_tank(0)));
    sim.level.blocks.insert(Sim::pos(1), Block::Tank(handler));
    sim.neighbors(Sim::pos(1));
    for _ in 0..3 {
        sim.tick();
    }
    assert!(sim.view(0).connected[Face::East as usize]);
}

#[test]
fn the_configurator_cycles_the_arm_and_none_takes_it_away_from_both_pipes() {
    let mut sim = Sim::new();
    // Antes de a rede nascer o cano não aceita de fora (o contêiner que `canInsert` pede é o da rede).
    sim.place_pipe(0, None);
    sim.place_pipe(1, None);
    assert!(!sim.view(0).insert[Face::East as usize]);
    for _ in 0..4 {
        sim.tick();
    }
    assert!(sim.view(0).connected[Face::East as usize]);
    assert!(sim.view(1).connected[Face::West as usize]);
    // O cano deixa entrar pelos lados que `canInsert` e o contêiner da rede dão: o braço leste sim, os lados sem vizinho não.
    assert!(sim.view(0).insert[Face::East as usize]);
    assert!(!sim.view(0).insert[Face::North as usize]);
    let mut seen = Vec::new();
    for _ in 0..3 {
        let consumed =
            sim.owner
                .sneak_right_click(&mut sim.level, Sim::pos(0), Face::East, Some(Face::East));
        assert!(consumed);
        for _ in 0..2 {
            sim.tick();
        }
        seen.push(sim.view(0).raw[Face::East as usize]);
    }
    assert_eq!(
        seen,
        [
            ConnectionType::Push,
            ConnectionType::Pull,
            ConnectionType::None
        ]
    );
    // `NONE`: o braço do cano e o do vizinho somem, e a capability do lado também.
    assert!(!sim.view(0).connected[Face::East as usize]);
    assert!(!sim.view(1).connected[Face::West as usize]);
    assert!(!sim.view(0).exists[Face::East as usize]);
    assert!(!sim.view(0).insert[Face::East as usize]);
    assert_eq!(sim.level.overlays.len(), 3);
    // Sem braço, o clique cai na face (`getSideLookingAt` nulo): o lado clicado volta a `NORMAL`.
    sim.owner
        .sneak_right_click(&mut sim.level, Sim::pos(0), Face::East, None);
    for _ in 0..2 {
        sim.tick();
    }
    assert_eq!(sim.view(0).raw[Face::East as usize], ConnectionType::Normal);
    assert!(sim.view(0).connected[Face::East as usize]);
}

#[test]
fn a_pulling_pipe_carries_lava_from_a_source_to_the_generator_tank_and_stops_when_it_is_full() {
    let mut sim = Sim::new();
    // A fonte à esquerda, quatro canos, o gerador à direita (só lava, 1.000).
    let source = sim.place_tank(-1, source(lava()));
    line(&mut sim, 4);
    let generator = sim.place_tank(4, lava_tank(0));
    for _ in 0..3 {
        sim.tick();
    }
    sim.owner.set_mode(
        &mut sim.level,
        Sim::pos(0),
        Face::West,
        ConnectionType::Pull,
    );
    let mut delivered = 0;
    for _ in 0..30 {
        sim.tick();
        // O fim do evento: o que a rede mudou no espelho vai para o contêiner do jogo.
        if let Some(contents) = generator.changed() {
            delivered = contents.amount;
            generator.settle();
        }
    }
    assert_eq!(
        generator.contents().amount,
        1_000,
        "o tanque do gerador enche até a capacidade"
    );
    assert_eq!(delivered, 1_000);
    // A fonte criativa não perde nada (`isCreative`), e o gerador, cheio, para a rede com o resto.
    assert_eq!(source.contents().amount, i64::MAX as u64);
    assert!(sim.network_amount(0) > 0);
    // O gerador queima 100 e o jogo devolve o tanque com 900: a rede reenche.
    generator.refresh(lava_tank(900));
    sim.tick();
    assert_eq!(generator.contents().amount, 1_000);
    // Água não entra no `lavaTank`.
    let transaction = machinery_basis_core::transaction::Transaction::open_root();
    use machinery_basis_core::distribution::ResourceInsert;
    assert_eq!(generator.insert(&water(), 100, &transaction), 0);
}

#[test]
fn the_pipe_writes_its_share_and_reads_it_back_when_the_chunk_comes_back() {
    let mut sim = Sim::new();
    line(&mut sim, 3);
    assert_eq!(
        sim.owner.insert_external(Sim::pos(0), &water(), 3_001),
        3_001
    );
    sim.tick();
    let written = sim.owner.write(Sim::pos(1)).expect("o cano está no dono");
    // As shares: 3.001 em três canos de 2.000, na ordem da rede (as duas primeiras de 1.000, a última de 1.001).
    let total: u64 = (0..3)
        .map(|x| sim.owner.write(Sim::pos(x)).unwrap().1.amount)
        .sum();
    assert_eq!(total, 3_001);
    assert_eq!(written.1.resource, Some(water()));
    // O chunk volta: um dono novo lê o `Transmitter` e o `stored` do que o cano gravou.
    let saved: Vec<_> = (0..3)
        .map(|x| sim.owner.write(Sim::pos(x)).unwrap())
        .collect();
    let mut reopened = Sim::new();
    for (x, saved) in saved.into_iter().enumerate() {
        reopened.place_pipe(x as i32, Some(saved));
    }
    for _ in 0..6 {
        reopened.tick();
    }
    assert_eq!(reopened.network_amount(0), 3_001);
    assert_eq!(
        reopened
            .owner
            .transmitter(Sim::pos(1))
            .unwrap()
            .connection_types_raw(),
        sim.owner
            .transmitter(Sim::pos(1))
            .unwrap()
            .connection_types_raw()
    );
}

#[test]
fn a_removed_pipe_gives_its_share_back_and_the_two_halves_keep_the_fluid() {
    let mut sim = Sim::new();
    line(&mut sim, 5);
    assert_eq!(
        sim.owner.insert_external(Sim::pos(0), &water(), 6_000),
        6_000
    );
    sim.tick();
    sim.level.blocks.remove(&Sim::pos(2));
    sim.send(Event::Removed(Sim::pos(2)));
    sim.neighbors(Sim::pos(2));
    for _ in 0..4 {
        sim.tick();
    }
    assert_eq!(sim.network_amount(0) + sim.network_amount(4), 6_000);
    assert_ne!(
        sim.owner.network_of(Sim::pos(0)),
        sim.owner.network_of(Sim::pos(4))
    );
}

#[test]
fn notify_tile_change_walks_the_neighbors_in_direction_values_order() {
    // `Transmitter.notifyTileChange` -> `WorldUtils.notifyLoadedNeighborsOfTileChange` percorre `EnumUtils.DIRECTIONS` (`Direction.values()`: baixo, cima, norte, sul, oeste, leste),
    // e não `NeighborUpdater.UPDATE_ORDER` (oeste, leste, baixo, cima, norte, sul) de `Level.updateNeighborsAt`. Cada vizinho que recebe o aviso relê as conexões dele e pergunta se os
    // lados dele estão carregados; o ponto a dois blocos do centro, na direção do vizinho, só esse vizinho pergunta.
    let mut sim = Sim::new();
    let center = BlockPos::new(0, 0, 0);
    for pos in std::iter::once(center).chain(Face::ALL.map(|side| center.relative(side))) {
        sim.level.blocks.insert(pos, Block::Pipe(None));
        sim.send(Event::ClearRemoved(pos));
        sim.neighbors(pos);
    }
    for _ in 0..4 {
        sim.tick();
    }
    sim.level.asked.borrow_mut().clear();
    // Agachado no braço de cima: o vizinho de cima também é atualizado por `refreshConnections(side)`, então fica fora da comparação.
    sim.owner
        .sneak_right_click(&mut sim.level, center, Face::Up, Some(Face::Up));
    let asked = sim.level.asked.borrow().clone();
    let mut order = Vec::new();
    for pos in asked {
        for side in Face::ALL {
            let outer = center.relative(side).relative(side);
            if pos == outer && !order.contains(&side) {
                order.push(side);
            }
        }
    }
    order.retain(|side| *side != Face::Up);
    assert_eq!(
        order,
        [Face::Down, Face::North, Face::South, Face::West, Face::East]
    );
}

#[test]
fn the_alloy_raises_the_pipes_one_tier_keeps_the_fluid_and_remakes_the_network() {
    let mut sim = Sim::new();
    line(&mut sim, 5);
    assert_eq!(
        sim.owner.insert_external(Sim::pos(0), &water(), 4_000),
        4_000
    );
    sim.tick();
    let pos = Sim::pos(2);
    let old_network = sim.owner.network_of(pos).unwrap();
    // O nível 1 (avançado) sobe os de nível 0; um de nível 1 já não sobe com a mesma liga.
    let mut run = sim.owner.alloy_start(pos, 1, &|_| Some(0)).unwrap();
    assert_eq!(run.targets.len(), 5);
    assert_eq!(run.targets[0], pos, "o clicado primeiro");
    assert!(
        sim.owner
            .alloy_start(pos, 1, &|_| Some(1))
            .unwrap()
            .targets
            .is_empty()
    );
    for target in run.targets.clone() {
        let data = sim
            .owner
            .alloy_take(&mut sim.level, &mut run, target)
            .unwrap();
        // O bloco muda: o cano velho sai e o novo entra no mesmo lugar.
        sim.send(Event::Removed(target));
        sim.send(Event::ClearRemoved(target));
        assert!(sim.owner.alloy_give(&mut run, target, &data));
    }
    sim.owner.alloy_finish(&mut sim.level, &run);
    assert_eq!(run.upgraded, 5);
    assert!(!sim.owner.registry().network(old_network).is_registered());
    for _ in 0..4 {
        sim.tick();
    }
    let network = sim.owner.network_of(Sim::pos(0)).unwrap();
    for x in 1..5 {
        assert_eq!(
            sim.owner.network_of(Sim::pos(x)),
            Some(network),
            "uma rede só"
        );
    }
    assert_eq!(
        sim.network_amount(0),
        4_000,
        "o fluido da rede velha não se perde"
    );
}
