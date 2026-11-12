//! Os caminhos da rede de químico que o rastro de `ChemicalNetworkTraceTest` não alcança, lidos do Java: os valores do `TubeTier`, o
//! `getRadiationScale` do tubo, o `disperse` e a diferença para o fluido.
mod resource_world;
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::buffered_resource_network::unsent;
use machinery_basis_core::transmission::pressurized_tube::TubeTier;
use machinery_basis_core::transmission::{NetworkEvent, NetworkId, TransmitterId};
use resource_world::{ChemicalKind, FluidKind, chemical};

type World = resource_world::World<ChemicalKind>;

const HYDROGEN: &str = "mekanism:hydrogen";
const WASTE: &str = "mekanism:nuclear_waste";

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

/// Empurra químico num tubo como um vizinho faz: o contêiner que a capability entrega (o da rede, ou o buffer sem rede).
fn push(world: &mut World, x: i32, id: &str, amount: i32) -> i32 {
    let (tube_id, tube) = world.pipe_at(world.pipe_pos(x)).unwrap();
    let container = world.family().container_of(tube_id, tube.net());
    let transaction = Transaction::open_root();
    let got = container.insert(
        &chemical(id),
        amount,
        &transaction,
        AutomationType::External,
    );
    transaction.commit();
    world.family_mut().flush_contents();
    got
}

fn network_of(world: &World, x: i32) -> NetworkId {
    world
        .pipe_at(world.pipe_pos(x))
        .and_then(|(_, tube)| tube.network)
        .expect("tubo em rede")
}

fn radioactive(resource: &machinery_basis_core::chemical::ChemicalResource) -> bool {
    resource.id.as_ref() == WASTE
}

#[test]
fn the_tube_tiers_have_the_values_of_the_original() {
    // `TubeTier`: capacidade e vazão em mB (`FluidType.BUCKET_VOLUME` = 1000).
    let values: Vec<(i64, i32)> = TubeTier::ALL
        .iter()
        .map(|tier| (tier.capacity(), tier.transfer_rate()))
        .collect();
    assert_eq!(
        values,
        [
            (4_000, 750),
            (16_000, 2_000),
            (256_000, 64_000),
            (1_024_000, 256_000)
        ]
    );
    assert_eq!(TubeTier::Elite.serialized_name(), "elite");
    assert_eq!(TubeTier::Ultimate.ordinal(), 3);
    let tube = TubeTier::Advanced.new_tube();
    assert_eq!((tube.capacity(), tube.transfer_rate()), (16_000, 2_000));
}

#[test]
fn the_last_tube_removed_disperses_what_the_network_holds_at_the_tube_that_triggered_it() {
    use machinery_basis_core::chemical::ChemicalResource;
    use machinery_basis_core::transmission::chemical_network::ChemicalFamily;
    use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
    use machinery_basis_core::transmission::{Uuid, buffered_resource_network::ResourceAcceptor};
    let registry: TransmitterNetworkRegistry<resource_world::Acceptor<ChemicalKind>> =
        TransmitterNetworkRegistry::new(|| Uuid::with_hash_code(1));
    let mut family: ChemicalFamily<dyn ResourceAcceptor<ChemicalResource>> =
        ChemicalFamily::chemical();
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::Created {
            uuid: Uuid::with_hash_code(1),
        },
    );
    let tube = TransmitterId(0);
    family.add_pressurized_tube(tube, BlockPos::new(3, 64, 5), TubeTier::Basic);
    // Rede vazia: nada a despejar.
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::LastTransmitterRemoved(tube),
    );
    assert!(family.take_dispersals().is_empty());
    family
        .network(NetworkId(0))
        .unwrap()
        .container()
        .set_contents(Some(chemical(WASTE)), 700, None);
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::LastTransmitterRemoved(tube),
    );
    let dispersals = family.take_dispersals();
    assert_eq!(dispersals.len(), 1);
    assert_eq!(dispersals[0].transmitter, tube);
    assert_eq!(dispersals[0].pos, BlockPos::new(3, 64, 5));
    assert_eq!(dispersals[0].resource, chemical(WASTE));
    assert_eq!(dispersals[0].amount, 700);
    // O `disperse` é despejar, não tirar: a rede segue com o que tinha, e a chamada seguinte começa do zero.
    assert_eq!(family.network(NetworkId(0)).unwrap().buffer().amount, 700);
    assert!(family.take_dispersals().is_empty());
}

#[test]
fn the_fluid_family_does_not_record_dispersals_because_its_disperse_is_empty() {
    use machinery_basis_core::fluid::FluidResource;
    use machinery_basis_core::transmission::fluid_network::FluidFamily;
    use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
    use machinery_basis_core::transmission::{Uuid, buffered_resource_network::ResourceAcceptor};
    let registry: TransmitterNetworkRegistry<resource_world::Acceptor<FluidKind>> =
        TransmitterNetworkRegistry::new(|| Uuid::with_hash_code(1));
    let mut family: FluidFamily<dyn ResourceAcceptor<FluidResource>> = FluidFamily::new();
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::Created {
            uuid: Uuid::with_hash_code(1),
        },
    );
    let pipe = TransmitterId(0);
    family.add_pipe(pipe, BlockPos::new(0, 64, 0), 2_000, 250);
    family
        .network(NetworkId(0))
        .unwrap()
        .container()
        .set_contents(Some(FluidResource::new("minecraft:lava")), 700, None);
    family.network_event(
        &registry,
        NetworkId(0),
        NetworkEvent::LastTransmitterRemoved(pipe),
    );
    assert!(family.take_dispersals().is_empty());
}

#[test]
fn what_the_save_shares_cannot_take_is_dispersed_only_when_a_tube_triggered_the_update() {
    let tube = Some(TransmitterId(4));
    // `triggerTransmitter != null && sent < toSend`: despeja `toSend - sent`.
    assert_eq!(unsent(tube, 900, 600), Some(300));
    assert_eq!(unsent(tube, 900, 0), Some(900));
    // Tudo coube: nada.
    assert_eq!(unsent(tube, 900, 900), None);
    // Sem quem disparou (`validateSaveShares` sempre tem um, mas `removeInvalid` pode vir sem): nada, mesmo que sobre.
    assert_eq!(unsent(None, 900, 600), None);
}

#[test]
fn a_tube_with_a_radioactive_chemical_reports_how_full_the_network_is_and_one_without_reports_zero()
{
    let mut world = world();
    for x in 0..2 {
        world.place_pipe(x, "basic");
    }
    tick(&mut world, 4);
    let (id, tube) = world.pipe_at(world.pipe_pos(0)).unwrap();
    let net = tube.net();
    // Sem nada: zero.
    assert_eq!(world.family().radiation_scale(id, net, &radioactive), 0.0);
    assert_eq!(push(&mut world, 0, WASTE, 2_000), 2_000);
    let (id, tube) = world.pipe_at(world.pipe_pos(0)).unwrap();
    // `amountAsLong() / (float) capacityAsLong(resource)`: o contêiner da rede, 2.000 de 8.000.
    assert_eq!(
        world.family().radiation_scale(id, tube.net(), &radioactive),
        0.25
    );
    // O mesmo conteúdo, sem radioatividade: zero.
    assert_eq!(
        world.family().radiation_scale(id, tube.net(), &|_| false),
        0.0
    );
    // Um tubo sem rede olha o próprio buffer e a capacidade do tier.
    let single = machinery_basis_core::transmission::chemical_network::ChemicalFamily::<
        dyn machinery_basis_core::transmission::buffered_resource_network::ResourceAcceptor<
                machinery_basis_core::chemical::ChemicalResource,
            >,
    >::chemical();
    let mut single = single;
    single.add_pressurized_tube(TransmitterId(0), BlockPos::new(0, 64, 0), TubeTier::Basic);
    single
        .pipe(TransmitterId(0))
        .buffer
        .set_contents(Some(chemical(WASTE)), 1_000, None);
    assert_eq!(
        single.radiation_scale(TransmitterId(0), None, &radioactive),
        0.25
    );
}

#[test]
fn hydrogen_and_the_waste_do_not_mix_in_a_tube_line_but_the_same_chemical_does() {
    let mut world = world();
    for x in [0, 1, 3, 4] {
        world.place_pipe(x, "basic");
    }
    tick(&mut world, 4);
    assert_eq!(push(&mut world, 0, HYDROGEN, 500), 500);
    assert_eq!(push(&mut world, 4, WASTE, 700), 700);
    tick(&mut world, 2);
    assert_ne!(network_of(&world, 0), network_of(&world, 4));
    // Outro químico no contêiner da rede cheia de hidrogênio não entra: o buffer só guarda um tipo.
    assert_eq!(push(&mut world, 0, WASTE, 100), 0);
}
