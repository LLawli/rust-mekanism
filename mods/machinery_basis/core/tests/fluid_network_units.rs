//! Os caminhos da rede de fluido que o rastro de `FluidNetworkTraceTest` não alcança, lidos do Java, e a base genérica de recurso.
mod resource_world;
use resource_world::{FluidKind, fluid};

type World = resource_world::World<FluidKind>;
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
    let tank = world.place_tank(tank_pos, "basic", None);
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
    assert_ne!(
        network_of(&world, 0),
        network_of(&world, 6),
        "continuam duas redes"
    );
    dbg!(world.family().network(water).map(|n| n.buffer()));
    dbg!(world.family().network(lava).map(|n| n.buffer()));
}

#[test]
fn the_pipe_tiers_carry_the_capacity_and_the_rate_of_tiers_toml() {
    use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
    let values: Vec<(i64, i32)> = PipeTier::ALL
        .iter()
        .map(|tier| (tier.capacity(), tier.transfer_rate()))
        .collect();
    assert_eq!(
        values,
        [
            (2_000, 250),
            (8_000, 1_000),
            (32_000, 8_000),
            (128_000, 32_000)
        ]
    );
    assert_eq!(PipeTier::Elite.serialized_name(), "elite");
}

#[test]
fn the_pipe_save_share_stops_at_the_capacity_of_the_tier_and_refuses_another_fluid() {
    use machinery_basis_core::transmission::buffered_resource_transmitter::ResourcePipe;
    let pipe = ResourcePipe::new(2_000, 250);
    let transaction = Transaction::open_root();
    let share = pipe.start_new_save_share(&transaction);
    assert_eq!(share.accept(&fluid("water"), 1_500, &transaction), 1_500);
    // outro fluido: nada; o mesmo: só o que cabe
    assert_eq!(share.accept(&fluid("lava"), 100, &transaction), 0);
    assert_eq!(share.accept(&fluid("water"), 900, &transaction), 500);
    assert_eq!(share.accept(&fluid("water"), 0, &transaction), 0);
    assert_eq!(pipe.save_share_contents().amount, 2_000);
    // sem confirmar, a transação desfaz o save
    drop(transaction);
    assert!(pipe.save_share_contents().is_empty());
}

#[test]
fn a_new_save_share_clears_the_old_one_and_a_commit_asks_the_tile_to_save() {
    use machinery_basis_core::transmission::buffered_resource_transmitter::ResourcePipe;
    let pipe = ResourcePipe::new(2_000, 250);
    let transaction = Transaction::open_root();
    pipe.start_new_save_share(&transaction)
        .accept(&fluid("water"), 700, &transaction);
    transaction.commit();
    assert_eq!(pipe.save_share_contents().amount, 700);
    assert_eq!(pipe.saved_count(), 1);
    let transaction = Transaction::open_root();
    pipe.start_new_save_share(&transaction);
    transaction.commit();
    assert!(pipe.save_share_contents().is_empty());
    assert_eq!(pipe.saved_count(), 2);
    // zerar o que já era zero não pede nada
    let transaction = Transaction::open_root();
    pipe.start_new_save_share(&transaction);
    transaction.commit();
    assert_eq!(pipe.saved_count(), 2);
}

#[test]
fn the_pipe_reads_its_stored_fluid_and_releases_the_buffer() {
    use machinery_basis_core::resource_tank::StackContents;
    use machinery_basis_core::transmission::buffered_resource_transmitter::ResourcePipe;
    let pipe = ResourcePipe::new(2_000, 250);
    pipe.read_stored(StackContents::new(Some(fluid("lava")), 640));
    assert_eq!(pipe.share().amount, 640);
    assert_eq!(pipe.save_share_contents().amount, 640);
    let released = pipe.release_share();
    assert_eq!(released.amount, 640);
    assert!(pipe.share().is_empty());
    // o save share não some ao soltar o buffer
    assert_eq!(pipe.save_share_contents().amount, 640);
    assert_eq!(pipe.upgrade_data().buffer.amount, 0);
}

#[test]
fn the_external_automation_cannot_take_fluid_out_of_the_pipe_buffer_but_the_network_container_allows_it()
 {
    use machinery_basis_core::transmission::buffered_resource_transmitter::ResourcePipe;
    let pipe = ResourcePipe::new(2_000, 250);
    let transaction = Transaction::open_root();
    assert_eq!(
        pipe.buffer
            .insert(&fluid("water"), 500, &transaction, AutomationType::External),
        500
    );
    assert_eq!(
        pipe.buffer
            .extract(&fluid("water"), 100, &transaction, AutomationType::External),
        0
    );
    assert_eq!(
        pipe.buffer
            .extract(&fluid("water"), 100, &transaction, AutomationType::Internal),
        100
    );
    transaction.commit();
}

fn standalone_family() -> (
    machinery_basis_core::transmission::fluid_network::FluidFamily<
        dyn machinery_basis_core::transmission::buffered_resource_network::ResourceAcceptor<
                machinery_basis_core::fluid::FluidResource,
            >,
    >,
    machinery_basis_core::transmission::registry::TransmitterNetworkRegistry<
        resource_world::Acceptor<FluidKind>,
    >,
) {
    use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
    use machinery_basis_core::transmission::{NetworkEvent, Uuid};
    let registry = TransmitterNetworkRegistry::new(|| Uuid::with_hash_code(1));
    let mut family = resource_world::Family::<FluidKind>::new();
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::Created {
            uuid: Uuid::with_hash_code(1),
        },
    );
    (family, registry)
}

#[test]
fn a_network_that_has_no_capacity_yet_does_not_clamp_its_buffer_to_zero() {
    let (mut family, _) = standalone_family();
    family
        .network(NetworkId(0))
        .unwrap()
        .container()
        .set_contents(Some(fluid("minecraft:water")), 500, None);
    assert_eq!(family.network(NetworkId(0)).unwrap().capacity(), 0);
    family.clamp_buffer(NetworkId(0));
    // A rede finge capacidade zero até terminar de se montar: o corte a ignora.
    assert_eq!(family.network(NetworkId(0)).unwrap().buffer().amount, 500);
}

#[test]
fn a_transmitter_buffer_of_another_fluid_is_released_and_lost_when_it_joins_a_network() {
    use machinery_basis_core::sim::chunks::BlockPos;
    use machinery_basis_core::transmission::{NetworkEvent, TransmitterId};
    let (mut family, registry) = standalone_family();
    let pipe = TransmitterId(0);
    family.add_pipe(pipe, BlockPos::new(0, 64, 0), 2_000, 250);
    family
        .pipe(pipe)
        .buffer
        .set_contents(Some(fluid("minecraft:water")), 300, None);
    family
        .network(NetworkId(0))
        .unwrap()
        .container()
        .set_contents(Some(fluid("minecraft:lava")), 100, None);
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::TransmitterCommitted(pipe),
    );
    let net = family.network(NetworkId(0)).unwrap();
    assert_eq!(net.buffer().resource, Some(fluid("minecraft:lava")));
    assert_eq!(net.buffer().amount, 100);
    assert!(family.pipe(pipe).share().is_empty());
}

#[test]
fn a_pulling_pipe_stops_at_the_first_side_that_fills_it() {
    use machinery_basis_core::sides::Face;
    use machinery_basis_core::transmission::connection::ConnectionType;
    let mut world = world();
    world.place_pipe(0, "basic");
    let below = world.place_tank(
        BlockPos::new(0, 63, 0),
        "basic",
        Some(("minecraft:water", Some(5_000))),
    );
    let above = world.place_tank(
        BlockPos::new(0, 65, 0),
        "basic",
        Some(("minecraft:water", Some(5_000))),
    );
    tick(&mut world, 4);
    assert_eq!(push(&mut world, 0, "minecraft:water", 1_800), 1_800);
    world.set_mode(0, Face::Down, ConnectionType::Pull);
    world.set_mode(0, Face::Up, ConnectionType::Pull);
    tick(&mut world, 1);
    // falta 200 (menos que a taxa de 250): o primeiro lado (embaixo) enche o cano e o de cima não é tocado
    assert_eq!(below.tank.amount(), 4_800);
    assert_eq!(above.tank.amount(), 5_000);
}

#[test]
fn a_manual_merge_tells_the_clients_the_new_scale_even_when_no_fluid_moved() {
    use machinery_basis_core::sides::Face;
    use machinery_basis_core::transmission::connection::ConnectionType;
    let mut world = world();
    for x in 0..6 {
        world.place_pipe(x, "basic");
    }
    tick(&mut world, 4);
    // As duas metades separadas pelos lados que se encaram.
    world.set_mode(2, Face::East, ConnectionType::None);
    world.set_mode(3, Face::West, ConnectionType::None);
    tick(&mut world, 3);
    assert_ne!(network_of(&world, 0), network_of(&world, 5));
    assert_eq!(push(&mut world, 0, "minecraft:water", 1_000), 1_000);
    tick(&mut world, 60);
    world.family();
    let before = world.level.hooks.state.family.take_transfer_events();
    drop(before);
    world.set_mode(2, Face::East, ConnectionType::Normal);
    world.set_mode(3, Face::West, ConnectionType::Normal);
    tick(&mut world, 1);
    assert_eq!(
        network_of(&world, 0),
        network_of(&world, 5),
        "as redes se juntaram"
    );
    let events = world.level.hooks.state.family.take_transfer_events();
    assert!(
        !events.is_empty(),
        "a escala da rede fundida mudou e o cliente precisa saber"
    );
}
