//! Os caminhos da rede de energia que o rastro de `EnergyNetworkTraceTest` não alcança, lidos do Java: a escala com a tolerância de
//! `MekanismUtils.scaleChanged`, o chunk de `ChunkPos.pack`, o teto de `updateCapacity`, o limite do `SaveShareJournal`, o dado de upgrade, o
//! `pullFromAcceptors`, a energia que a fusão tira da outra rede, o corte do buffer, a escala forçada e as regras de `BufferedTransmitter`.
mod energy_world;
use energy_world::World;
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::buffered_network::{BufferedNetwork, chunk_of};
use machinery_basis_core::transmission::buffered_transmitter::{
    Ends, extra_valid_transmitter, primary_and_secondary, recheck_connections_sides,
};
use machinery_basis_core::transmission::energy_container::BufferEnergyContainer;
use machinery_basis_core::transmission::universal_cable::UniversalCable;
use machinery_basis_core::transmission::{NetworkEvent, NetworkId};

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
        .tx_at(world.cable(x))
        .and_then(|(_, tx)| tx.network)
        .expect("cabo em rede")
}

fn push(world: &mut World, x: i32, amount: i32) -> i32 {
    let (id, tx) = world.tx_at(world.cable(x)).unwrap();
    let state = &world.level.hooks.state;
    let container = match tx.network {
        Some(network) => state.family.network(network).unwrap().energy.clone(),
        None => state.family.cable(id).buffer.clone(),
    };
    let transaction = Transaction::open_root();
    let got = container.insert(amount, &transaction, AutomationType::External);
    transaction.commit();
    got
}

#[test]
fn the_scale_changes_only_past_the_tolerance_of_scale_changed() {
    let mut network = BufferedNetwork::default();
    network.current_scale = 0.5;
    network.update_scale(0.500_001);
    assert!(!network.needs_update);
    assert_eq!(network.current_scale, 0.5);
    network.update_scale(0.51);
    assert!(network.needs_update);
    assert_eq!(network.current_scale, 0.51);
    // Dentro da tolerância, mas indo a zero ou a um: muda mesmo assim.
    let mut network = BufferedNetwork::default();
    network.update_scale(0.000_001);
    assert!(network.needs_update);
}

#[test]
fn the_chunk_of_a_transmitter_is_chunk_pos_pack() {
    for (x, z) in [(0, 0), (15, 16), (-1, -17), (855_112, -3_777_695)] {
        assert_eq!(chunk_of(x, z), BlockPos::new(x, 64, z).chunk().pack());
    }
}

#[test]
fn the_capacity_sum_caps_at_the_max_with_the_overflow_test_against_the_old_capacity() {
    let mut network = BufferedNetwork::default();
    network.update_capacity([8_000, 128_000]);
    assert_eq!(network.capacity(), 136_000);
    // O teste de estouro compara com a capacidade de antes, não com a soma parcial.
    network.update_capacity([i64::MAX - 135_999, 1]);
    assert_eq!(network.capacity(), i64::MAX);
    // Com a capacidade antiga no teto, qualquer capacidade positiva "estoura" (o teste usa `Long.MAX_VALUE - capacity`, que vale 0).
    network.update_capacity([5]);
    assert_eq!(network.capacity(), i64::MAX);
    let mut network = BufferedNetwork::default();
    network.update_capacity([5]);
    assert_eq!(network.capacity(), 5);
    let mut network = BufferedNetwork::default();
    network.transmitter_committed(0, i64::MAX);
    network.transmitter_committed(0, 10);
    assert_eq!(network.capacity(), i64::MAX);
}

#[test]
fn the_cable_save_share_stops_at_the_capacity_of_the_tier() {
    let cable = UniversalCable::new(8_000);
    let transaction = Transaction::open_root();
    let share = cable.start_new_save_share(&transaction);
    assert_eq!(share.accept(0, &transaction), 0);
    assert_eq!(share.accept(5_000, &transaction), 5_000);
    assert_eq!(share.accept(5_000, &transaction), 3_000);
    assert_eq!(share.accept(1, &transaction), 0);
    transaction.commit();
    assert_eq!(cable.save_share_amount(), 8_000);
    assert_eq!(cable.saved_count(), 1);
    // Sem commit, a transação desfaz.
    let transaction = Transaction::open_root();
    cable.start_new_save_share(&transaction);
    drop(transaction);
    assert_eq!(cable.save_share_amount(), 8_000);
}

#[test]
fn the_cable_reads_its_energy_and_releases_the_buffer() {
    let cable = UniversalCable::new(8_000);
    cable.read_energy(1_234);
    assert_eq!((cable.save_share_amount(), cable.share()), (1_234, 1_234));
    assert_eq!(cable.release_share(), 1_234);
    assert_eq!(cable.share(), 0);
    assert_eq!(cable.save_share_amount(), 1_234);
}

#[test]
fn the_upgrade_data_carries_the_buffer_to_the_new_cable() {
    let old = UniversalCable::new(8_000);
    old.buffer.set_energy(777, None);
    let data = old.upgrade_data();
    let new = UniversalCable::new(128_000);
    let transaction = Transaction::open_root();
    new.parse_upgrade_data(&data, &transaction);
    transaction.commit();
    assert_eq!(new.share(), 777);
    // Desfeito junto com a transação.
    let other = UniversalCable::new(128_000);
    let transaction = Transaction::open_root();
    other.parse_upgrade_data(&data, &transaction);
    drop(transaction);
    assert_eq!(other.share(), 0);
}

#[test]
fn a_pulling_cable_takes_what_the_acceptor_gives_until_the_container_is_full() {
    let container = BufferEnergyContainer::new(
        100,
        machinery_basis_core::transmission::energy_container::not_external,
        machinery_basis_core::transmission::energy_container::always,
        None,
    );
    assert_eq!(UniversalCable::available_pull(&container), 100);
    let asked = std::cell::Cell::new(0);
    let full = UniversalCable::pull_from_side(&container, |amount, _| {
        asked.set(amount);
        60
    });
    assert_eq!((asked.get(), full, container.amount()), (100, false, 60));
    assert_eq!(UniversalCable::available_pull(&container), 40);
    let full = UniversalCable::pull_from_side(&container, |amount, _| amount);
    assert!(full);
    assert_eq!(container.amount(), 100);
    // Um aceitador que dá mais do que cabe: nada é guardado.
    let container = BufferEnergyContainer::new(
        100,
        machinery_basis_core::transmission::energy_container::not_external,
        machinery_basis_core::transmission::energy_container::always,
        None,
    );
    let full = UniversalCable::pull_from_side(&container, |_, _| 150);
    assert!(!full);
    assert_eq!(container.amount(), 0);
}

#[test]
fn the_merge_takes_all_the_energy_out_of_the_network_it_adopts() {
    let mut world = world();
    for x in [0, 1, 3, 4] {
        world.place_cable(x, "basic");
    }
    tick(&mut world, 3);
    let (left, right) = (network_of(&world, 0), network_of(&world, 3));
    assert_ne!(left, right);
    assert_eq!(push(&mut world, 0, 5_000), 5_000);
    assert_eq!(push(&mut world, 3, 3_000), 3_000);
    world.place_cable(2, "basic");
    tick(&mut world, 3);
    let merged = network_of(&world, 2);
    let family = &world.level.hooks.state.family;
    assert_eq!(family.network(merged).unwrap().energy.amount(), 8_000);
    assert_eq!(family.network(left).unwrap().energy.amount(), 0);
    assert_eq!(family.network(right).unwrap().energy.amount(), 0);
}

#[test]
fn removing_the_invalid_transmitters_clamps_the_buffer_to_the_capacity() {
    let mut world = world();
    for x in 0..3 {
        world.place_cable(x, "basic");
    }
    tick(&mut world, 3);
    let network = network_of(&world, 0);
    let hooks = &mut world.level.hooks;
    let capacity = hooks.state.family.network(network).unwrap().capacity();
    assert_eq!(capacity, 24_000);
    hooks
        .state
        .family
        .network(network)
        .unwrap()
        .energy
        .set_energy(capacity + 500, None);
    hooks
        .state
        .family
        .network_event(&hooks.registry, network, NetworkEvent::InvalidRemoved(None));
    let family = &hooks.state.family;
    assert_eq!(family.network(network).unwrap().energy.amount(), capacity);
    // E a energia foi repartida entre os três cabos até a capacidade de cada um.
    let shares: Vec<i64> = (0..3)
        .map(|x| {
            let (id, _) = world.tx_at(world.cable(x)).unwrap();
            world.level.hooks.state.family.cable(id).save_share_amount()
        })
        .collect();
    assert_eq!(shares, vec![8_000, 8_000, 8_000]);
}

#[test]
fn a_new_network_with_buffered_cables_forces_its_scale_to_the_target_at_once() {
    let mut world = world();
    world.place_cable(0, "basic");
    world.place_cable(1, "basic");
    for x in [0, 1] {
        let (id, _) = world.tx_at(world.cable(x)).unwrap();
        world
            .level
            .hooks
            .state
            .family
            .cable(id)
            .buffer
            .set_energy(4_000, None);
    }
    // Congelado: o `onUpdate` não roda, e a escala é a que o `commit` forçou.
    world.level.hooks.state.runs_normally = false;
    tick(&mut world, 4);
    let network = network_of(&world, 0);
    let net = world.level.hooks.state.family.network(network).unwrap();
    assert_eq!(net.energy.amount(), 8_000);
    assert_eq!(net.capacity(), 16_000);
    assert_eq!(net.current_scale(), 0.5);
}

#[test]
fn the_buffer_with_fallback_reads_the_network_when_the_cable_has_none() {
    let mut world = world();
    world.place_cable(0, "basic");
    world.place_cable(1, "basic");
    tick(&mut world, 3);
    push(&mut world, 0, 1_000);
    let (id, tx) = world.tx_at(world.cable(0)).unwrap();
    let family = &world.level.hooks.state.family;
    assert_eq!(family.buffer_with_fallback(id, tx.network), 1_000);
    assert_eq!(family.buffer_with_fallback(id, None), 0);
}

#[test]
fn a_network_does_not_join_an_orphan_when_both_may_have_incompatible_networks() {
    let end = |has_network, is_orphan| Ends {
        can_have_incompatible_networks: true,
        has_network,
        is_orphan,
    };
    assert!(!extra_valid_transmitter(end(true, false), end(false, true)));
    assert!(!extra_valid_transmitter(end(false, true), end(true, false)));
    assert!(extra_valid_transmitter(end(true, false), end(true, false)));
    assert!(extra_valid_transmitter(end(false, true), end(false, true)));
    // Se um dos dois não pode ter redes incompatíveis, a regra básica basta.
    let plain = Ends {
        can_have_incompatible_networks: false,
        has_network: false,
        is_orphan: true,
    };
    assert!(extra_valid_transmitter(end(true, false), plain));
}

#[test]
fn the_recheck_looks_at_the_enabled_sides_only_with_a_network_and_incompatible_networks() {
    use machinery_basis_core::sides::Face;
    assert_eq!(recheck_connections_sides(false, true, 0b111), None);
    assert_eq!(recheck_connections_sides(true, false, 0b111), Some(vec![]));
    let sides = recheck_connections_sides(true, true, 0b000_101).unwrap();
    assert_eq!(sides.len(), 2);
    assert!(sides.iter().all(|side| Face::ALL.contains(side)));
}

#[test]
fn the_manual_merge_makes_the_cable_without_buffer_take_the_other_network_as_primary() {
    let (a, b) = (NetworkId(1), NetworkId(2));
    assert_eq!(primary_and_secondary(a, b, false), (a, b));
    assert_eq!(primary_and_secondary(a, b, true), (b, a));
}
