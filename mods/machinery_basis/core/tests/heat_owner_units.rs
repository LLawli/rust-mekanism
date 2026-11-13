//! O dono dos condutores (`HeatTransmitters`) onde os rastros do original não chegam: o Configurator num condutor do meio de uma linha, o redstone, o NBT
//! (`heat_capacitor/state`) que sai e volta, e o condutor como vizinho do Heat Generator. Cada regra vem do Java citado no teste; os valores numéricos da troca
//! de calor são os que os rastros já conferem (`heat_owner_trace.rs`), então aqui se compara o que muda entre dois estados do mesmo mundo.
mod heat_owner_world;

use heat_owner_world::{World, ambient_default};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::thermodynamic_conductor::{CapacitorState, ConductorTier};

fn world() -> World {
    World::new(
        BlockPos::new(0, 64, 0),
        ambient_default(),
        (1..40).map(|n| n * 7919).collect(),
        (1..40).map(|n| n * 31).collect(),
    )
}

fn tick(world: &mut World, n: u32) {
    for _ in 0..n {
        world.tick();
    }
}

/// Uma linha de `n` condutores básicos, já juntos numa rede.
fn line(n: i32) -> World {
    let mut world = world();
    for x in 0..n {
        world.place_conductor(x, ConductorTier::Basic);
    }
    tick(&mut world, 4);
    world
}

fn heat(world: &World, x: i32) -> f64 {
    world
        .owner()
        .state_at(world.pos(x, 0))
        .and_then(|state| state.heat)
        .expect("calor lido")
}

/// `currentAcceptorConnections` do condutor em `x`.
fn acceptor_bits(world: &World, x: i32) -> u8 {
    world
        .owner()
        .transmitter(world.pos(x, 0))
        .expect("condutor")
        .acceptor_cache
        .current_acceptor_connections
}

/// Quantos aceitadores a rede do condutor em `x` tem (`NetworkAcceptorCache`).
fn acceptors_of_network(world: &World, x: i32) -> usize {
    let network = world
        .owner()
        .state_at(world.pos(x, 0))
        .and_then(|state| state.network)
        .expect("em rede");
    world.owner().registry().network(network).acceptor_count()
}

fn network(world: &World, x: i32) -> Option<u32> {
    world
        .owner()
        .state_at(world.pos(x, 0))
        .and_then(|state| state.network)
        .map(|network| network.0)
}

#[test]
fn a_line_of_conductors_is_one_network_and_the_heat_walks_to_the_far_end() {
    let mut world = line(3);
    assert_eq!(network(&world, 0), network(&world, 2));
    world.heat_conductor(0, 5_000.0);
    let before = heat(&world, 2);
    tick(&mut world, 30);
    assert!(heat(&world, 2) > before, "o calor chegou ao outro lado");
}

/// `Transmitter.onModeChange`, `refreshConnections` e `notifyTileChange` de um `NONE` no braço leste do condutor do meio: a conexão cai nos dois lados (`canConnectMutual`),
/// a linha vira duas redes, e o capacitor deixa de existir pelos dois lados (`TileEntityThermodynamicConductor`: sem capability onde a conexão crua é `NONE`), então o
/// calor não cruza. Com `NORMAL` de volta a linha se junta e o calor volta a andar.
#[test]
fn none_on_the_middle_conductor_cuts_the_heat_and_normal_brings_it_back() {
    let mut world = line(3);
    world.heat_conductor(0, 5_000.0);
    tick(&mut world, 10);
    world.set_mode(1, Face::East, ConnectionType::None);
    tick(&mut world, 2);
    assert_eq!(network(&world, 0), network(&world, 1));
    assert_ne!(network(&world, 1), network(&world, 2), "a linha se dividiu");
    // O condutor 2 olha o 1 pelo lado oeste: a capability dele no leste é nula (`NONE`), então 2 não o tem como aceitador, e a rede dele não conta aceitador nenhum.
    assert_eq!(
        acceptor_bits(&world, 2),
        0,
        "sem capacitor do outro lado não há aceitador"
    );
    assert_eq!(acceptors_of_network(&world, 2), 0);
    let cut = heat(&world, 2);
    tick(&mut world, 30);
    assert!(
        heat(&world, 2) <= cut,
        "sem a conexão o outro lado só perde calor para o ar"
    );
    // O braço some nos dois lados: o condutor 2 também não tem a conexão com o 1.
    assert!(!world.view(2).connected[Face::West as usize]);
    assert!(!world.view(1).connected[Face::East as usize]);
    assert!(world.view(1).connected[Face::West as usize]);

    world.set_mode(1, Face::East, ConnectionType::Normal);
    tick(&mut world, 2);
    assert_eq!(
        network(&world, 0),
        network(&world, 2),
        "a linha se juntou de novo"
    );
    let rejoined = heat(&world, 2);
    tick(&mut world, 30);
    assert!(heat(&world, 2) > rejoined, "o calor voltou a andar");
}

/// O `NONE` de um lado tira a capability só dele (`getConnectionTypeRaw(direction) == NONE`): `sink_at` do lado de cima responde, o do lado leste não.
#[test]
fn the_capability_exists_only_on_the_sides_that_are_not_none() {
    let mut world = line(2);
    let pos = world.pos(0, 0);
    world.set_mode(0, Face::Up, ConnectionType::None);
    let up = sink_exists(&mut world, pos, Face::Up);
    let down = sink_exists(&mut world, pos, Face::Down);
    assert!(!up, "o lado de cima é NONE");
    assert!(down, "o de baixo continua com o capacitor");
}

/// `redstoneChanged`: um condutor com redstone reativo ativado por sinal não expõe capacitor em lado nenhum (`conductor.isRedstoneActivated()`), e o vizinho
/// deixa de ser aceitador dele.
#[test]
fn a_conductor_activated_by_redstone_exposes_no_capacitor() {
    let mut world = line(2);
    let pos = world.pos(0, 0);
    assert!(sink_exists(&mut world, pos, Face::Up));
    assert!(world.right_click(0), "o clique liga o redstone reativo");
    world.level.hooks.powered = true;
    world.update_neighbors(-1);
    tick(&mut world, 2);
    assert!(
        !sink_exists(&mut world, pos, Face::Up),
        "ativado por redstone"
    );
    world.level.hooks.powered = false;
    world.update_neighbors(-1);
    tick(&mut world, 2);
    assert!(
        sink_exists(&mut world, pos, Face::Up),
        "sem sinal o capacitor volta"
    );
}

/// `ThermodynamicConductor.write`: o calor sai do capacitor (`getHeat` inicializa o calor que nunca foi lido), e um condutor novo que lê esse estado (`loadAdditional`)
/// recomeça com o mesmo calor e a mesma capacidade, mesmo que o ar de agora seja outro.
#[test]
fn the_capacitor_state_goes_out_and_comes_back() {
    let mut world = line(1);
    world.heat_conductor(0, 700.0);
    tick(&mut world, 3);
    let state = world.owner().state_at(world.pos(0, 0)).unwrap();
    let tag = world_write(&mut world, 0);
    assert_eq!(tag.capacitor.heat, state.heat.unwrap());
    assert_eq!(tag.capacitor.heat_capacity, 1.0);

    let mut other = world_with_air(280.0);
    other.place_saved_conductor(
        0,
        ConductorTier::Basic,
        (tag.transmitter.clone(), Some(tag.capacitor)),
    );
    tick(&mut other, 1);
    let loaded = other.owner().state_at(other.pos(0, 0)).unwrap();
    // O calor lido entra na hora (`setHeatAndCapacity`), e um tick sem vizinhos só deixa o ar levar um pouco dele.
    assert!(
        loaded.heat.unwrap() > 600.0,
        "o calor salvo voltou, e não os 280 do ar"
    );
    assert_eq!(loaded.heat_capacity, 1.0);
}

#[test]
fn a_conductor_never_read_initializes_its_heat_to_the_air_when_it_is_written() {
    let mut world = world_with_air(310.0);
    world.place_conductor(0, ConductorTier::Elite);
    assert_eq!(world.owner().state_at(world.pos(0, 0)).unwrap().heat, None);
    let tag = world_write(&mut world, 0);
    assert_eq!(
        tag.capacitor.heat, 310.0,
        "getHeat inicializa com capacidade vezes o ar"
    );
    assert_eq!(
        world.owner().state_at(world.pos(0, 0)).unwrap().heat,
        Some(310.0)
    );
}

/// Um estado de capacitor fora da faixa (`heat < 0`, `capacity < 1`) faz o codec inteiro falhar: o condutor nasce com os valores da config.
#[test]
fn an_invalid_saved_state_is_not_read() {
    assert_eq!(CapacitorState::decode(Some(-5.0), Some(2.0)), None);
    let mut world = world_with_air(300.0);
    world.place_saved_conductor(
        0,
        ConductorTier::Basic,
        (
            Default::default(),
            CapacitorState::decode(Some(10.0), Some(0.5)),
        ),
    );
    tick(&mut world, 1);
    let state = world.owner().state_at(world.pos(0, 0)).unwrap();
    assert_eq!(state.heat_capacity, 1.0);
}

/// O Heat Generator em cima do condutor: o calor passa pelo lado de baixo dele (`getAdjacent(DOWN)`) e some quando o condutor tem `NONE` no lado de cima.
#[test]
fn the_generator_gives_heat_to_the_conductor_below_it_unless_that_side_is_none() {
    let mut world = line(1);
    world.place_generator(0);
    world.heat_generator(5_000.0);
    tick(&mut world, 5);
    let transferred = *world
        .level
        .hooks
        .heater
        .as_ref()
        .unwrap()
        .last_transfer
        .borrow();
    assert!(transferred > 0.0, "o gerador quente passa calor para baixo");
    world.set_mode(0, Face::Up, ConnectionType::None);
    tick(&mut world, 2);
    let after = *world
        .level
        .hooks
        .heater
        .as_ref()
        .unwrap()
        .last_transfer
        .borrow();
    assert_eq!(after, 0.0, "sem capacitor no lado de cima nada passa");
}

// -----------------------------------------------------------------------------------------------------------------------------

fn world_with_air(kelvin: f64) -> World {
    World::new(
        BlockPos::new(0, 64, 0),
        vec![vec![[kelvin; 7]; 8]; 2],
        (1..40).map(|n| n * 7919).collect(),
        (1..40).map(|n| n * 31).collect(),
    )
}

fn world_write(
    world: &mut World,
    x: i32,
) -> machinery_basis_core::transmission::thermodynamic_conductor::ConductorTag {
    let pos = world.pos(x, 0);
    world.write(pos)
}

fn sink_exists(world: &mut World, pos: BlockPos, side: Face) -> bool {
    world.sink_exists(pos, side)
}
