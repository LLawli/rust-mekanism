//! A topologia da rede de transmissores, em cenários pequenos montados à mão (os de rastro estão em `transmission_trace.rs`).
mod transmission_world;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use transmission_world::*;

fn world(uuid_hashes: Vec<i32>) -> World {
    let origin = BlockPos::new(100, 64, 8);
    let mut counter = 1000;
    let mut world = World::new(origin, 8, uuid_hashes, move |_| {
        counter += 7919;
        counter
    });
    for x in 0..3 {
        world.level.set_ticket_level(
            ChunkPos::new(origin.chunk().x + x - 1, origin.chunk().z),
            ENTITY_TICKING_LEVEL,
        );
    }
    world.level.tick();
    world
}

#[test]
fn a_line_of_cables_becomes_one_network_after_the_first_tick_of_its_block_entities() {
    let mut world = world(vec![11]);
    for x in 0..5 {
        world.place_cable(x);
    }
    world.level.tick();
    let registry = world.registry();
    assert_eq!(registry.networks_in_order().len(), 1);
    let network = registry.networks_in_order()[0];
    assert_eq!(registry.network(network).transmitters_size(), 5);
    for x in 0..5 {
        let (_, tx) = world.tx_at(world.cable(x)).unwrap();
        assert_eq!(tx.network, Some(network));
        assert!(!tx.orphan);
    }
}

fn network_of(world: &World, x: i32) -> Option<machinery_basis_core::transmission::NetworkId> {
    world.tx_at(world.cable(x)).and_then(|(_, tx)| tx.network)
}

#[test]
fn a_cable_between_two_networks_merges_them_into_a_new_one_and_the_old_ones_are_gone() {
    let mut world = world(vec![1, 2, 3]);
    for x in [0, 1, 3, 4] {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    let old: Vec<_> = world.registry().networks_in_order();
    assert_eq!(old.len(), 2);
    world.place_cable(2);
    world.level.tick_n(3);
    let registry = world.registry();
    let merged = registry.networks_in_order();
    assert_eq!(merged.len(), 1, "as duas redes viraram uma");
    assert!(!old.contains(&merged[0]), "a rede é nova, não uma das duas");
    assert_eq!(registry.network(merged[0]).transmitters_size(), 5);
    for network in old {
        assert!(!registry.network(network).is_registered());
        assert!(registry.network(network).is_empty());
    }
    for x in 0..5 {
        assert_eq!(network_of(&world, x), Some(merged[0]));
    }
}

#[test]
fn breaking_the_middle_cable_splits_the_network_in_two() {
    let mut world = world(vec![1, 2, 3]);
    for x in 0..5 {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    let before = world.registry().networks_in_order();
    assert_eq!(before.len(), 1);
    let pos = world.cable(2);
    world.break_block(pos);
    world.level.tick_n(3);
    let registry = world.registry();
    assert_eq!(registry.networks_in_order().len(), 2);
    let (left, right) = (network_of(&world, 0), network_of(&world, 4));
    assert!(left.is_some() && right.is_some() && left != right);
    assert_eq!(network_of(&world, 1), left);
    assert_eq!(network_of(&world, 3), right);
    assert_eq!(registry.network(left.unwrap()).transmitters_size(), 2);
    assert_eq!(registry.network(right.unwrap()).transmitters_size(), 2);
    assert!(!registry.network(before[0]).is_registered());
}

#[test]
fn breaking_the_only_cable_leaves_no_network() {
    let mut world = world(vec![1]);
    world.place_cable(0);
    world.level.tick_n(3);
    let pos = world.cable(0);
    world.break_block(pos);
    world.level.tick_n(3);
    assert!(world.registry().networks_in_order().is_empty());
}

#[test]
fn uuid_hash_code_round_trips_and_random_ones_differ() {
    use machinery_basis_core::transmission::Uuid;
    for hash in [0, 1, -1, i32::MIN, i32::MAX, 123_456_789, -987_654_321] {
        assert_eq!(Uuid::with_hash_code(hash).hash_code(), hash);
    }
    let (a, b) = (Uuid::random(), Uuid::random());
    assert_ne!(a, b);
    // versão 4 e variante 2
    assert_eq!((a.most_significant >> 12) & 0xF, 4);
    assert_eq!((a.least_significant >> 62) & 0x3, 2);
}

/// Um cubo ao lado de um cabo é um aceitador depois que a invalidação da capability marca o lado: a rede o conta no `commit`, a fusão o
/// adota (`adoptAcceptors`) e a quebra do cabo o tira da rede nova.
#[test]
fn acceptors_are_counted_adopted_by_a_merge_and_dropped_with_their_cable() {
    use machinery_basis_core::sides::Face;
    let mut world = world(vec![1, 2, 3, 4, 5]);
    world.level.hooks.state.cube_is_acceptor = true;
    for x in [0, 1, 3, 4] {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    for x in [1, 3] {
        let cable = world.cable(x);
        world
            .level
            .set_block(BlockPos::new(cable.x, cable.y, cable.z + 1), Some(CUBE));
        world
            .level
            .hooks
            .state
            .acceptor_invalidations
            .push((cable, Face::South));
    }
    world.level.tick_n(2);
    let count = |world: &World, x| {
        let network = network_of(world, x).unwrap();
        world.registry().network(network).acceptor_count()
    };
    assert_eq!((count(&world, 0), count(&world, 4)), (1, 1));
    world.place_cable(2);
    world.level.tick_n(3);
    assert_eq!(
        count(&world, 0),
        2,
        "a rede nova adotou os dois aceitadores"
    );
    let network = network_of(&world, 0).unwrap();
    let cube = world.cable(3);
    let cube = BlockPos::new(cube.x, cube.y, cube.z + 1);
    assert!(world.registry().network(network).has_acceptor(cube));
    assert_eq!(
        world.registry().network(network).acceptor_directions(cube),
        vec![Face::North]
    );
    let cable = world.cable(3);
    world.break_block(cable);
    world.level.tick_n(3);
    assert_eq!(count(&world, 0), 1, "o cubo do cabo quebrado saiu");
}

/// `registerOrphanTransmitter` de um transmissor que está em `invalidTransmitters` só o tira de lá (`TransmitterNetworkRegistry.java:93-99`): se
/// era órfão ainda é, se não era continua na rede em que estava. A rede não se desfaz.
#[test]
fn registering_an_invalid_transmitter_as_orphan_only_takes_it_off_the_invalid_set() {
    let mut world = world(vec![1, 2, 3]);
    for x in 0..3 {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    let network = network_of(&world, 1).unwrap();
    let pos = world.cable(1);
    world.level.hooks.state.registry_calls = vec![
        RegistryCall::InvalidateTransmitter(pos),
        RegistryCall::RegisterOrphanTransmitter(pos),
    ];
    world.level.tick_n(3);
    assert_eq!(
        network_of(&world, 1),
        Some(network),
        "continua na mesma rede"
    );
    assert!(world.registry().network(network).is_registered());
    assert_eq!(world.registry().networks_in_order(), vec![network]);

    // Sem o registro por cima, a invalidação desfaz a rede e os três cabos formam outra.
    world.level.hooks.state.registry_calls = vec![RegistryCall::InvalidateTransmitter(pos)];
    world.level.tick_n(3);
    let rebuilt = network_of(&world, 1).unwrap();
    assert_ne!(rebuilt, network);
    assert_eq!(world.registry().network(rebuilt).transmitters_size(), 3);
}
