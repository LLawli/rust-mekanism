//! O modo de depuração (`MekanismAPI.debug`) no registro de redes: as seis mensagens `Mekanism.logger.info` de `TransmitterNetworkRegistry.java`
//! (linhas 170, 255, 260, 352, 370 e 398) e o espelho do `networks` único do original, de onde sai a ordem do `toComponents`.
mod transmission_world;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkPos, ENTITY_TICKING_LEVEL};
use machinery_basis_core::sim::open_hash::{OpenHashSet, OpenKey, mix_int};
use machinery_basis_core::transmission::registry::NetworkMirror;
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

fn debug_log(world: &mut World) -> Vec<String> {
    world.registry_mut().take_debug_log()
}

#[test]
fn with_debug_off_the_registry_writes_nothing() {
    let mut world = world(vec![11, 12, 13, 14, 15, 16]);
    for x in [0, 1, 2, 4] {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    world.place_cable(3);
    world.level.tick_n(3);
    world.break_block(world.cable(1));
    world.level.tick_n(3);
    assert!(debug_log(&mut world).is_empty());
}

#[test]
fn the_orphan_path_finder_and_the_invalid_transmitters_log_what_the_original_logs() {
    let mut world = world(vec![11, 12, 13, 14, 15]);
    world.registry_mut().set_debug(true);
    // Cinco cabos soltos: cinco órfãos, nenhuma rede por perto (`MekanismAPI.debug` liga os `info` das linhas 255, 260 e 398).
    for x in [0, 1, 3, 4, 5] {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    let first = debug_log(&mut world);
    assert_eq!(
        first,
        vec![
            "Dealing with 5 orphan Transmitters".to_owned(),
            "No networks found. Creating new network for 2 transmitters".to_owned(),
            "No networks found. Creating new network for 3 transmitters".to_owned(),
        ]
    );
    // Um cabo que une as duas redes: "Merging 2 networks with 1 new transmitters".
    world.place_cable(2);
    world.level.tick_n(3);
    let merge = debug_log(&mut world);
    assert_eq!(
        merge,
        vec![
            "Dealing with 1 orphan Transmitters".to_owned(),
            "Merging 2 networks with 1 new transmitters".to_owned(),
        ]
    );
    // Um cabo que entra numa rede só: "Adding 1 transmitters to single found network".
    world.place_cable(6);
    world.level.tick_n(3);
    assert_eq!(
        debug_log(&mut world),
        vec![
            "Dealing with 1 orphan Transmitters".to_owned(),
            "Adding 1 transmitters to single found network".to_owned(),
        ]
    );
    // Um cabo que sai: um inválido (linha 170), e os dois pedaços da rede divida viram órfãos de novo.
    world.break_block(world.cable(3));
    world.level.tick_n(3);
    let broken = debug_log(&mut world);
    assert_eq!(
        broken[0], "Dealing with 1 invalid Transmitters",
        "{broken:?}"
    );
}

#[test]
fn a_chunk_whose_ticket_level_changes_logs_the_count_and_the_chunk() {
    use machinery_basis_core::sim::chunks::{ENTITY_TICKING_LEVEL as LOADED, MAX_VIEW_DISTANCE};
    let mut world = world(vec![11]);
    world.registry_mut().set_debug(true);
    for x in 0..3 {
        world.place_cable(x);
    }
    world.level.tick_n(3);
    debug_log(&mut world);
    let origin = world.origin;
    let chunk = origin.chunk();
    let key = chunk.pack();
    let dimension = machinery_basis_core::transmission::DimensionId(0);
    world
        .registry_mut()
        .on_ticket_level_change(dimension, key, LOADED, MAX_VIEW_DISTANCE + 1);
    // O `onTick` do registro faz o `handleChangedChunks`; o do mundo roda no `tick` do nível.
    world.level.tick();
    let log = debug_log(&mut world);
    assert_eq!(log[0], "Dealing with 1 changed chunks", "{log:?}");
    assert_eq!(
        log[1],
        format!("Unloaded 3 transmitters in chunk: {}, {}", chunk.x, chunk.z),
        "{log:?}"
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Hash(i32);

impl OpenKey for Hash {
    fn mixed(self) -> i32 {
        mix_int(self.0)
    }
}

#[test]
fn the_mirror_orders_the_networks_of_every_registry_as_one_object_open_hash_set() {
    // Dois mundos, cada um com seu registro: as redes de um e do outro entram no mesmo conjunto, como no `networks` estático do original.
    let hashes_a = vec![0x1234_5678, -0x2222_1111, 77, 0x7fff_0001];
    let hashes_b = vec![-5, 0x0abc_def0, 1_000_003];
    let mut a = world(hashes_a.clone());
    let mut b = world(hashes_b.clone());
    // Duas linhas de cabos separadas em `a`, uma em `b`, e depois mais uma em `a`: a ordem de entrada é a das redes.
    for x in [0, 1, 3, 4] {
        a.place_cable(x);
    }
    b.place_cable(0);
    a.level.tick_n(3);
    b.level.tick_n(3);
    b.place_cable(2);
    b.place_cable(4);
    a.place_cable(6);
    a.level.tick_n(3);
    b.level.tick_n(3);
    let mirror = NetworkMirror::current();
    let order = mirror.in_order();
    let all = a.registry().networks_in_order().len() + b.registry().networks_in_order().len();
    assert_eq!(order.len(), all);
    assert!(all >= 5, "{all}");
    // O mesmo conjunto montado à mão: as redes na ordem em que foram registradas (a de cada mundo está em `networks_in_order`; a ordem relativa entre os mundos é a dos ticks acima).
    let hash_of = |key: &machinery_basis_core::transmission::registry::MirrorKey| {
        let registry = if key.registry == a.registry().id() {
            a.registry()
        } else {
            b.registry()
        };
        registry.network(key.network).uuid().hash_code()
    };
    let mut expected = OpenHashSet::new();
    for key in &order {
        expected.insert(Hash(hash_of(key)));
    }
    let wanted: Vec<i32> = expected.in_order().into_iter().map(|h| h.0).collect();
    let got: Vec<i32> = order.iter().map(hash_of).collect();
    // Os mesmos elementos, na ordem da tabela: sem colisão entre os hashes escolhidos a ordem não depende da de entrada.
    assert_eq!(got, wanted);
    // Um registro que cai leva as redes dele do espelho.
    let b_id = b.registry().id();
    drop(b);
    assert!(mirror.in_order().iter().all(|key| key.registry != b_id));
    assert!(!mirror.in_order().is_empty());
}

#[test]
fn a_reset_takes_the_networks_of_that_registry_out_of_the_mirror() {
    let mut a = world(vec![21, 22]);
    let mut b = world(vec![23]);
    for x in [0, 1, 3] {
        a.place_cable(x);
    }
    b.place_cable(0);
    a.level.tick_n(3);
    b.level.tick_n(3);
    let mirror = NetworkMirror::current();
    assert_eq!(mirror.in_order().len(), 3);
    let a_id = a.registry().id();
    a.registry_mut().reset();
    let left = mirror.in_order();
    assert_eq!(left.len(), 1);
    assert!(left.iter().all(|key| key.registry != a_id));
}
