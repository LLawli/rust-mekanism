//! Os caminhos da rede de fluido que o rastro de `FluidNetworkTraceTest` não alcança, lidos do Java, e a base genérica de recurso.
mod fluid_world;
use fluid_world::{World, fluid};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::NetworkId;

fn world() -> World {
    let mut next = 100;
    let mut world = World::new(
        BlockPos::new(0, 64, 0),
        8,
        (1..40).map(|n| n * 7919).collect(),
        move |_| {
            next += 1;
            next * 31
        },
    );
    world
        .level
        .set_ticket_level(ChunkPos::new(0, 0), ENTITY_TICKING_LEVEL);
    world.level.tick();
    world
}

fn tick(world: &mut World, n: u32) {
    for _ in 0..n {
        world.tick_level();
        world.level.finish_tick();
    }
}

fn network_of(world: &World, x: i32) -> NetworkId {
    world
        .pipe_at(world.pipe_pos(x))
        .and_then(|(_, pipe)| pipe.network)
        .expect("cano em rede")
}

/// Empurra fluido num cano como um vizinho faz: o contêiner que a capability entrega (o da rede, ou o buffer sem rede).
fn push(world: &mut World, x: i32, id: &str, amount: i32) -> i32 {
    let (pipe_id, pipe) = world.pipe_at(world.pipe_pos(x)).unwrap();
    let container = world.family().container_of(pipe_id, pipe.net());
    let transaction = Transaction::open_root();
    let got = container.insert(&fluid(id), amount, &transaction, AutomationType::External);
    transaction.commit();
    got
}

#[test]
fn a_line_of_pipes_shares_the_fluid_in_one_network_and_the_tank_drinks_it() {
    let mut world = world();
    for x in 0..5 {
        world.place_pipe(x, "basic");
    }
    let tank_pos = BlockPos::new(4, 65, 0);
    let tank = world.place_tank(tank_pos, 1_000_000, None, [true; 6], false);
    tick(&mut world, 4);
    let network = network_of(&world, 0);
    assert_eq!(world.family().network(network).unwrap().capacity(), 10_000);
    assert_eq!(push(&mut world, 0, "water", 300), 300);
    tick(&mut world, 3);
    assert_eq!(tank.tank.amount(), 300);
    assert_eq!(world.family().network(network).unwrap().buffer().amount, 0);
}

#[test]
fn two_networks_with_different_fluids_stay_apart_until_one_is_empty_and_a_side_is_reread() {
    let mut world = world();
    for x in [0, 1, 2, 4, 5, 6] {
        world.place_pipe(x, "basic");
    }
    tick(&mut world, 4);
    assert_eq!(push(&mut world, 0, "water", 500), 500);
    assert_eq!(push(&mut world, 6, "lava", 700), 700);
    tick(&mut world, 2);
    let (water, lava) = (network_of(&world, 0), network_of(&world, 6));
    assert_ne!(water, lava);
    // A ponte nasce órfã, vazia, e entra na primeira rede compatível que a busca vê.
    world.place_pipe(3, "basic");
    tick(&mut world, 4);
    let bridge = network_of(&world, 3);
    assert!(bridge == water || bridge == lava);
    assert_ne!(network_of(&world, 0), network_of(&world, 6), "continuam duas redes");
    dbg!(world.family().network(water).map(|n| n.buffer()));
    dbg!(world.family().network(lava).map(|n| n.buffer()));
}
