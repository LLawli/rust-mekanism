//! O pacote de escala e o cliente das redes (`transmission::client`), contra o que o Java faz: `PacketNetworkScale` (o `STREAM_CODEC` e o
//! `handle`), `DynamicBufferedNetwork.genPacketRange`, o teste de raio de `PacketUtils.sendToAllTracking`, `Transmitter.handleUpdateTag`
//! no cliente e `TransmitterNetworkRegistry.clientNetworks`. O quando e o valor de cada pacote no servidor são conferidos contra o
//! rastro do original em `owner_trace.rs` (os `EnergyTransferEvent`) e em `resource_owner_units.rs`.
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::DimensionId;
use machinery_basis_core::transmission::client::{
    ClientKind, ClientTransmitters, MAX_BATCH, Op, ScaleOutbox, ScalePacket, TagMessage,
    decode_batch, decode_contents, decode_gone, encode_batches, encode_contents, encode_gone,
    gen_packet_range, player_in_range,
};
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::transmitter::{Compound, Tag, keys};

const DIM: DimensionId = DimensionId(0);
const OTHER: DimensionId = DimensionId(1);
const A: u128 = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210;
const B: u128 = 0x1111_1111_2222_2222_3333_3333_4444_4444;

fn at(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, 64, z)
}

fn uuid_tag(id: u128) -> Tag {
    Tag::IntArray(vec![
        (id >> 96) as i32,
        (id >> 64) as i32,
        (id >> 32) as i32,
        id as i32,
    ])
}

/// O `writeReducedUpdatedTag` de um transmissor com a rede `network` e os tipos crus dados.
fn tag(
    network: Option<u128>,
    raw: [i32; 6],
    transmitter_connections: i8,
    acceptors: i8,
) -> Compound {
    let mut tag = Compound::new();
    tag.insert(
        keys::CURRENT_CONNECTIONS.into(),
        Tag::Byte(transmitter_connections),
    );
    tag.insert(keys::CURRENT_ACCEPTORS.into(), Tag::Byte(acceptors));
    tag.insert(keys::CONNECTION.into(), Tag::IntArray(raw.to_vec()));
    if let Some(id) = network {
        tag.insert(keys::NETWORK.into(), uuid_tag(id));
    }
    tag
}

#[test]
fn the_scale_packet_is_the_uuid_as_two_big_endian_longs_then_the_float() {
    let packet = ScalePacket {
        network: A,
        scale: 0.075,
    };
    let bytes = packet.encode();
    assert_eq!(bytes.len(), 20);
    assert_eq!(&bytes[..8], &0x0123_4567_89ab_cdef_u64.to_be_bytes());
    assert_eq!(&bytes[8..16], &0xfedc_ba98_7654_3210_u64.to_be_bytes());
    assert_eq!(&bytes[16..], &0.075_f32.to_bits().to_be_bytes());
    assert_eq!(ScalePacket::decode(&bytes), Some(packet));
    assert_eq!(ScalePacket::decode(&bytes[..19]), None);
}

#[test]
fn the_packet_range_is_the_box_of_the_network_whatever_the_order() {
    let list = [at(5, 3), at(-2, 9), at(7, -4), at(0, 0)];
    let range = gen_packet_range(list, DIM).unwrap();
    assert_eq!(
        (range.x_min, range.z_min, range.x_max, range.z_max),
        (-2, -4, 7, 9)
    );
    let mut reversed = list;
    reversed.reverse();
    assert_eq!(gen_packet_range(reversed, DIM), Some(range));
    let single = gen_packet_range([at(3, 4)], DIM).unwrap();
    assert_eq!(
        (single.x_min, single.z_min, single.x_max, single.z_max),
        (3, 4, 3, 4)
    );
    assert_eq!(gen_packet_range([], DIM), None);
}

#[test]
fn the_players_in_range_are_the_ones_within_the_view_distance_of_the_box() {
    let range = gen_packet_range([at(0, 0), at(10, 20)], DIM).unwrap();
    // `playerX + radius + 1.99999 > xMin` e `xMax + 0.99999 > playerX - radius`, com `radius = viewDistance * 16`.
    let near = |x, z| player_in_range(&range, DIM, x, z, 10);
    assert!(near(-161, 0), "o limite oeste: -161 + 160 + 1.99999 > 0");
    assert!(!near(-162, 0));
    assert!(near(170, 0), "o limite leste: 10 + 0.99999 > 170 - 160");
    assert!(!near(171, 0));
    assert!(near(0, -161) && !near(0, -162));
    assert!(
        near(0, 180) && !near(0, 181),
        "o sul: 20 + 0.99999 > z - 160"
    );
    assert!(!player_in_range(&range, OTHER, 0, 0, 10), "outra dimensão");
    assert!(player_in_range(&range, DIM, 0, 0, 2));
    assert!(
        !player_in_range(&range, DIM, 100, 0, 2),
        "a distância de visão vale: 2 * 16 = 32"
    );
}

#[test]
fn the_outbox_keeps_the_range_until_the_network_gets_new_transmitters_or_dies() {
    use machinery_basis_core::transmission::NetworkId;
    let mut outbox: ScaleOutbox<()> = ScaleOutbox::new(DIM);
    let net = NetworkId(7);
    outbox.send(net, A, 0.5, [at(0, 0), at(4, 0)], None);
    // A cache vale: os transmissores novos não mudam o retângulo até o `validTransmittersAdded`.
    outbox.send(net, A, 0.6, [at(0, 0), at(40, 0)], None);
    outbox.flush_range(net);
    outbox.send(net, A, 0.7, [at(0, 0), at(40, 0)], None);
    let sent = outbox.take();
    let boxes: Vec<(i32, f32)> = sent
        .iter()
        .map(|out| (out.range.x_max, out.packet.scale))
        .collect();
    assert_eq!(boxes, vec![(4, 0.5), (4, 0.6), (40, 0.7)]);
    assert!(outbox.take().is_empty());
    // Uma rede sem transmissores não manda nada (`getPacketRange` nulo).
    outbox.send(NetworkId(8), B, 1.0, [], None);
    assert!(outbox.take().is_empty());
}

#[test]
fn an_update_tag_with_a_network_creates_it_on_the_client_and_sums_the_capacity() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.add_tile(at(1, 0), ClientKind::Energy, 128_000);
    assert!(client.get_client_network(A).is_none());
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    let network = client.get_client_network(A).unwrap();
    assert_eq!(network.transmitters_size(), 1);
    assert_eq!(network.capacity, 8_000);
    assert_eq!(network.current_scale, 0.0);
    client.handle_update_tag(at(1, 0), &tag(Some(A), [0; 6], 0, 0));
    let network = client.get_client_network(A).unwrap();
    assert_eq!(network.transmitters_size(), 2);
    assert_eq!(
        network.capacity, 136_000,
        "updateClientNetwork soma o getCapacity de cada um"
    );
    assert_eq!(client.network_at(at(1, 0)).map(|net| net.uuid), Some(A));
}

#[test]
fn the_scale_packet_sets_the_scale_of_a_known_network_only() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Resource, 1_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    client.on_scale_packet(&ScalePacket {
        network: A,
        scale: 0.75,
    });
    client.on_scale_packet(&ScalePacket {
        network: B,
        scale: 0.25,
    });
    assert_eq!(client.get_client_network(A).unwrap().current_scale, 0.75);
    assert!(
        client.get_client_network(B).is_none(),
        "o pacote de uma rede desconhecida não a cria"
    );
    // `PacketFluidNetworkContents.handle`.
    client.on_contents_packet(A, Some("minecraft:water".into()));
    assert_eq!(
        client.get_client_network(A).unwrap().last_type.as_deref(),
        Some("minecraft:water")
    );
}

#[test]
fn a_heat_network_has_no_scale() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Heat, 0);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    client.on_scale_packet(&ScalePacket {
        network: A,
        scale: 0.5,
    });
    assert_eq!(client.get_client_network(A).unwrap().current_scale, 0.0);
    assert_eq!(client.get_client_network(A).unwrap().capacity, 0);
}

#[test]
fn the_network_goes_when_its_last_transmitter_leaves_or_the_tag_drops_it() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.add_tile(at(1, 0), ClientKind::Energy, 8_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    client.handle_update_tag(at(1, 0), &tag(Some(A), [0; 6], 0, 0));
    // Um pacote sem rede tira o transmissor dela (`setTransmitterNetwork(null)`).
    client.handle_update_tag(at(1, 0), &tag(None, [0; 6], 0, 0));
    assert_eq!(client.get_client_network(A).unwrap().transmitters_size(), 1);
    assert!(client.network_at(at(1, 0)).is_none());
    // O último a sair desregistra a rede (`removeTransmitter`: `isEmpty` e `deregister`).
    client.remove_tile(at(0, 0));
    assert!(client.get_client_network(A).is_none());
    assert_eq!(client.network_count(), 0);
}

#[test]
fn a_transmitter_moving_between_networks_leaves_the_old_one() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    client.handle_update_tag(at(0, 0), &tag(Some(B), [0; 6], 0, 0));
    assert!(
        client.get_client_network(A).is_none(),
        "a rede velha ficou vazia"
    );
    assert_eq!(client.get_client_network(B).unwrap().transmitters_size(), 1);
    // O mesmo pacote de novo não muda nada (o `return refreshModelData` cedo).
    client.handle_update_tag(at(0, 0), &tag(Some(B), [0; 6], 0, 0));
    assert_eq!(client.get_client_network(B).unwrap().transmitters_size(), 1);
}

#[test]
fn the_tile_reloaded_in_the_same_place_takes_the_place_of_the_old_one_in_the_network() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    // "When the client dies at spawn in single player": o tile novo entra no lugar do velho e a rede segue com um transmissor só.
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    assert_eq!(client.get_client_network(A).unwrap().transmitters_size(), 1);
}

#[test]
fn the_client_learns_the_raw_connection_types_and_shows_the_arms() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    // Lados: DOWN, UP, NORTH, SOUTH, WEST, EAST. Cru: NORMAL(0), PUSH(1), PULL(2), NONE(3) (`ConnectionType.ordinal`).
    let raw = [0, 0, 1, 2, 0, 3];
    // Braços em NORTH, SOUTH e EAST (bits 2, 3 e 5), todos de aceitador.
    let acceptors = (1 << 2) | (1 << 3) | (1 << 5);
    let changed = client.handle_update_tag(at(0, 0), &tag(None, raw, 0, acceptors));
    assert!(changed, "os dados do modelo mudaram");
    let view = client.view_at(at(0, 0)).unwrap();
    assert_eq!(view.kind[2], ConnectionType::Push);
    assert_eq!(view.kind[3], ConnectionType::Pull);
    assert_eq!(
        view.kind[5],
        ConnectionType::None,
        "o NONE cru não tem braço"
    );
    assert_eq!(
        view.kind[0],
        ConnectionType::None,
        "sem braço o lado mostra NONE"
    );
    // Um lado entre dois transmissores mostra NORMAL, seja qual for o cru.
    let again = client.handle_update_tag(at(0, 0), &tag(None, raw, 1 << 2, 1 << 3));
    assert!(again);
    let view = client.view_at(at(0, 0)).unwrap();
    assert_eq!(view.kind[2], ConnectionType::Normal);
    assert_eq!(view.kind[3], ConnectionType::Pull);
}

#[test]
fn clearing_the_client_networks_forgets_them_and_a_new_tag_makes_them_again() {
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0, 0), ClientKind::Energy, 8_000);
    client.handle_update_tag(at(0, 0), &tag(Some(A), [0; 6], 0, 0));
    client.clear_client_networks();
    assert!(client.get_client_network(A).is_none());
    assert_eq!(client.network_count(), 0);
}

#[test]
fn the_messages_survive_the_wire() {
    let mut tag = tag(Some(A), [0, 1, 2, 3, 0, 1], 5, -3);
    tag.insert("scale".into(), Tag::Float(0.25));
    tag.insert("energy".into(), Tag::Long(-9_000_000_000));
    tag.insert("stored".into(), Tag::String("minecraft:water".into()));
    tag.insert("short".into(), Tag::Short(-7));
    tag.insert("int".into(), Tag::Int(i32::MIN));
    tag.insert("double".into(), Tag::Double(1.5e300));
    let message = TagMessage {
        dimension: "minecraft:the_nether".into(),
        pos: BlockPos::new(-12, 64, 30_000_000),
        kind: ClientKind::Resource,
        capacity: 4_000,
        tag,
    };
    assert_eq!(TagMessage::decode(&message.encode()), Some(message.clone()));
    let mut cut = message.encode();
    cut.pop();
    assert_eq!(TagMessage::decode(&cut), None, "um pacote cortado não vale");
    cut = message.encode();
    cut.push(0);
    assert_eq!(TagMessage::decode(&cut), None, "nem um com sobra");
    let gone = encode_gone("minecraft:overworld", BlockPos::new(1, 2, 3));
    assert_eq!(
        decode_gone(&gone),
        Some(("minecraft:overworld".to_owned(), BlockPos::new(1, 2, 3)))
    );
    assert_eq!(
        decode_contents(&encode_contents(A, Some("mekanism:hydrogen"))),
        Some((A, Some("mekanism:hydrogen".into())))
    );
    assert_eq!(decode_contents(&encode_contents(B, None)), Some((B, None)));
}

#[test]
fn the_packets_travel_in_batches_in_order_and_each_batch_fits_a_mod_message() {
    let mut ops = Vec::new();
    for x in 0..2_000 {
        ops.push(Op::Tag(TagMessage {
            dimension: "minecraft:overworld".into(),
            pos: at(x, 0),
            kind: ClientKind::Energy,
            capacity: 8_000,
            tag: tag(Some(A), [0; 6], 0, 0),
        }));
        if x % 100 == 0 {
            ops.push(Op::Scale(ScalePacket {
                network: A,
                scale: x as f32 / 2_000.0,
            }));
            ops.push(Op::Gone {
                dimension: "minecraft:overworld".into(),
                pos: at(x, 1),
            });
            ops.push(Op::Contents {
                network: A,
                resource: (x % 200 == 0).then(|| "minecraft:lava".to_owned()),
            });
        }
    }
    let batches = encode_batches(&ops);
    assert!(
        batches.len() > 1 && batches.len() < 32,
        "uma rede grande cabe nas 32 mensagens por tick: {}",
        batches.len()
    );
    assert!(
        batches
            .iter()
            .all(|batch| batch.len() <= 30_000 && batch.len() <= MAX_BATCH + 4)
    );
    let decoded: Vec<Op> = batches
        .iter()
        .flat_map(|batch| decode_batch(batch).unwrap())
        .collect();
    assert_eq!(decoded, ops, "a ordem se mantém");
    assert!(encode_batches(&[]).is_empty());
    assert_eq!(decode_batch(&[0, 1]), None);
}

#[test]
fn applying_the_packets_in_order_leaves_the_client_as_the_server_told() {
    let mut client = ClientTransmitters::new();
    let tag_for = |x| {
        Op::Tag(TagMessage {
            dimension: "d".into(),
            pos: at(x, 0),
            kind: ClientKind::Energy,
            capacity: 8_000,
            tag: tag(Some(A), [0; 6], 0, 0),
        })
    };
    client.apply(&tag_for(0));
    client.apply(&tag_for(1));
    client.apply(&Op::Scale(ScalePacket {
        network: A,
        scale: 0.5,
    }));
    assert_eq!(client.get_client_network(A).unwrap().current_scale, 0.5);
    client.apply(&Op::Gone {
        dimension: "d".into(),
        pos: at(0, 0),
    });
    client.apply(&Op::Gone {
        dimension: "d".into(),
        pos: at(1, 0),
    });
    assert!(client.get_client_network(A).is_none());
}
