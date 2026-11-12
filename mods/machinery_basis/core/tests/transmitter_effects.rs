//! A ordem dos efeitos e os ramos de `Transmitter.java` e de `TileEntityTransmitter.onSneakRightClick` que o rastro não alcança
//! (cliente, rede, o `onConfigure` que consome o clique, `hasPullSide`), pelo texto do Java.

use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType, side_bit,
};
use machinery_basis_core::transmission::transmitter::{
    BlockPos, Compound, Effect, InteractionResult, NeighborTransmitter, Overlay, Tag, Transmitter,
    TransmitterWorld, UpdateTagNetwork, keys, relative,
};

/// Um mundo com um transmissor vizinho de energia em cada posição de `neighbors` e um bloco de redstone em `powered`.
struct World {
    neighbors: Vec<(BlockPos, bool)>,
    powered: bool,
    /// Os lados do transmissor em `POS` que têm uma capability.
    acceptor_sides: Vec<Face>,
    /// O tipo de transmissão dos transmissores vizinhos.
    neighbor_kind: TransmissionType,
}

impl TransmitterWorld for World {
    type Acceptor = ();

    fn is_block_loaded(&self, _pos: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _pos: BlockPos) -> bool {
        self.powered
    }

    fn transmitter_at(&self, pos: BlockPos, _side: Face) -> Option<NeighborTransmitter> {
        self.neighbors
            .iter()
            .find(|(at, _)| *at == pos)
            .map(|(_, accepts)| NeighborTransmitter {
                supported: TransmissionSet::of(&[self.neighbor_kind]),
                can_connect_opposite: *accepts,
            })
    }

    fn acceptor(&self, _pos: BlockPos, from_side: Face) -> Option<()> {
        self.acceptor_sides
            .contains(&from_side.opposite())
            .then_some(())
    }
}

const POS: BlockPos = [0, 0, 0];

fn cable(client: bool) -> Transmitter<()> {
    let mut t = Transmitter::new(POS, TransmissionSet::of(&[TransmissionType::Energy]), true);
    t.set_level(client);
    t
}

fn world_with(sides: &[Face]) -> World {
    World {
        neighbors: sides.iter().map(|s| (relative(POS, *s), true)).collect(),
        powered: false,
        acceptor_sides: Vec::new(),
        neighbor_kind: TransmissionType::Energy,
    }
}

#[test]
fn a_new_transmitter_connection_asks_for_a_recheck_before_the_update_packet() {
    let mut t = cable(false);
    let world = world_with(&[Face::East, Face::Up]);
    let mut out = Vec::new();
    t.refresh_connections(&world, &mut out);
    assert_eq!(
        t.current_transmitter_connections,
        side_bit(Face::East) | side_bit(Face::Up)
    );
    assert_eq!(
        out,
        vec![
            Effect::RecheckConnections(side_bit(Face::East) | side_bit(Face::Up)),
            Effect::SendUpdatePacket
        ]
    );
    // A segunda leitura não muda nada: nenhum efeito.
    let mut again = Vec::new();
    t.refresh_connections(&world, &mut again);
    assert_eq!(again, Vec::<Effect>::new());
}

#[test]
fn losing_a_transmitter_sends_the_packet_but_asks_for_no_recheck() {
    let mut t = cable(false);
    let mut out = Vec::new();
    t.refresh_connections(&world_with(&[Face::North]), &mut out);
    let mut out = Vec::new();
    t.refresh_connections(&world_with(&[]), &mut out);
    assert_eq!(t.current_transmitter_connections, 0);
    assert_eq!(out, vec![Effect::SendUpdatePacket]);
}

#[test]
fn the_client_never_refreshes_or_reacts_to_a_sneak_click() {
    let mut t = cable(true);
    let world = world_with(&[Face::East]);
    let mut out = Vec::new();
    t.refresh_connections(&world, &mut out);
    t.refresh_connections_side(Face::East, &world, &mut out);
    t.refresh_acceptor_connections(Face::East, &world, &mut out);
    assert_eq!(t.current_transmitter_connections, 0);
    let result = t.on_sneak_right_click(
        Face::East,
        None,
        |t, _| t.base_on_configure(),
        &world,
        &mut out,
    );
    assert_eq!(result, InteractionResult::Success);
    assert_eq!(
        t.get_connection_type_raw(Face::East),
        ConnectionType::Normal
    );
    assert!(out.is_empty());
}

#[test]
fn the_sneak_click_cycles_and_tells_the_player_the_new_type() {
    let mut t = cable(false);
    let world = world_with(&[]);
    let mut seen = Vec::new();
    for _ in 0..5 {
        let mut out = Vec::new();
        let result = t.on_sneak_right_click(
            Face::Up,
            None,
            |t, _| t.base_on_configure(),
            &world,
            &mut out,
        );
        assert_eq!(result, InteractionResult::Success);
        assert_eq!(out.last(), Some(&Effect::SendUpdatePacket));
        let overlay = out.iter().find_map(|e| match e {
            Effect::Overlay(Overlay::ConnectionType { connection }) => Some(*connection),
            _ => None,
        });
        seen.push(overlay.unwrap());
    }
    use ConnectionType::{None as Off, Normal, Pull, Push};
    assert_eq!(seen, vec![Push, Pull, Off, Normal, Push]);
}

#[test]
fn the_sneak_click_on_a_none_side_skips_on_configure() {
    let mut t = cable(false);
    let world = world_with(&[]);
    t.set_connection_type_raw(Face::Up, ConnectionType::None, &mut Vec::new());
    let mut asked = false;
    let mut out = Vec::new();
    t.on_sneak_right_click(
        Face::Up,
        None,
        |_, _| {
            asked = true;
            InteractionResult::Success
        },
        &world,
        &mut out,
    );
    assert!(!asked);
    assert_eq!(t.get_connection_type_raw(Face::Up), ConnectionType::Normal);
}

#[test]
fn an_on_configure_that_consumes_the_click_changes_no_type() {
    let mut t = cable(false);
    let world = world_with(&[]);
    let mut out = Vec::new();
    let result = t.on_sneak_right_click(
        Face::East,
        None,
        |_, side| {
            assert_eq!(side, Face::East);
            InteractionResult::Success
        },
        &world,
        &mut out,
    );
    assert_eq!(result, InteractionResult::Success);
    assert_eq!(
        t.get_connection_type_raw(Face::East),
        ConnectionType::Normal
    );
    assert_eq!(out, vec![Effect::NotifyTileChange]);
}

#[test]
fn an_arm_hit_by_the_ray_wins_over_the_clicked_side() {
    let mut t = cable(false);
    let world = world_with(&[Face::East]);
    t.refresh_connections(&world, &mut Vec::new());
    let mut out = Vec::new();
    t.on_sneak_right_click(
        Face::Up,
        Some(Face::East),
        |t, _| t.base_on_configure(),
        &world,
        &mut out,
    );
    assert_eq!(t.get_connection_type_raw(Face::East), ConnectionType::Push);
    assert_eq!(t.get_connection_type_raw(Face::Up), ConnectionType::Normal);
}

#[test]
fn side_looking_at_maps_the_box_index_to_the_arm() {
    let mut t = cable(false);
    t.refresh_connections(
        &world_with(&[Face::Up, Face::South, Face::East]),
        &mut Vec::new(),
    );
    assert_eq!(t.side_looking_at(Some(0)), Some(Face::Up));
    assert_eq!(t.side_looking_at(Some(1)), Some(Face::South));
    assert_eq!(t.side_looking_at(Some(2)), Some(Face::East));
    // O centro é a última caixa: nenhum lado.
    assert_eq!(t.side_looking_at(Some(3)), None);
    assert_eq!(t.side_looking_at(None), None);
}

#[test]
fn has_pull_side_follows_the_raw_types() {
    let mut t = cable(false);
    let mut out = Vec::new();
    t.set_connection_type_raw(Face::Up, ConnectionType::Pull, &mut out);
    t.set_connection_type_raw(Face::Down, ConnectionType::Pull, &mut out);
    assert!(t.has_pull_side());
    t.set_connection_type_raw(Face::Up, ConnectionType::Push, &mut out);
    assert!(t.has_pull_side());
    t.set_connection_type_raw(Face::Down, ConnectionType::None, &mut out);
    assert!(!t.has_pull_side());
    // Repor o mesmo tipo não avisa ninguém.
    let mut quiet = Vec::new();
    t.set_connection_type_raw(Face::Down, ConnectionType::None, &mut quiet);
    assert!(quiet.is_empty());
    t.set_connection_types_raw([ConnectionType::Pull; 6]);
    assert!(t.has_pull_side());
}

#[test]
fn a_reactive_transmitter_reads_the_redstone_once_and_follows_its_changes() {
    let mut t = cable(false);
    let mut world = world_with(&[Face::East]);
    let mut out = Vec::new();
    assert_eq!(
        t.on_right_click(&world, &mut out),
        InteractionResult::Success
    );
    assert!(t.redstone_reactive);
    assert!(out.contains(&Effect::Overlay(Overlay::RedstoneSensitivity { on: true })));
    assert_eq!(t.current_transmitter_connections, side_bit(Face::East));
    world.powered = true;
    let mut out = Vec::new();
    t.on_neighbor_block_change(Some(Face::Down), &world, &mut out);
    assert_eq!(t.current_transmitter_connections, 0);
    assert!(t.redstone_powered());
    assert_eq!(
        out,
        vec![
            Effect::NotifyTileChange,
            Effect::RedstoneChanged(true),
            Effect::SendUpdatePacket
        ]
    );
    assert!(!t.can_connect(Face::East, &world));
    assert!(t.is_redstone_activated(&world));
}

#[test]
fn a_transmitter_that_ignores_redstone_only_answers_the_click() {
    let mut t = Transmitter::<()>::new(POS, TransmissionSet::of(&[TransmissionType::Item]), false);
    t.set_level(false);
    let world = world_with(&[]);
    let mut out = Vec::new();
    assert_eq!(
        t.on_right_click(&world, &mut out),
        InteractionResult::Success
    );
    assert!(!t.redstone_reactive);
    assert!(out.is_empty());
    assert!(!t.is_redstone_activated(&world));
}

#[test]
fn without_redstone_reactive_a_side_change_refreshes_only_that_side() {
    let mut t = cable(false);
    let world = world_with(&[Face::East, Face::West]);
    t.on_neighbor_block_change(Some(Face::East), &world, &mut Vec::new());
    assert_eq!(t.current_transmitter_connections, side_bit(Face::East));
    t.on_neighbor_block_change(None, &world, &mut Vec::new());
    assert_eq!(
        t.current_transmitter_connections,
        side_bit(Face::East) | side_bit(Face::West)
    );
}

#[test]
fn the_network_follows_set_transmitter_network_on_each_side() {
    let mut server = cable(false);
    let mut out = Vec::new();
    assert!(!server.set_transmitter_network(Some(5), true, &mut out));
    assert_eq!(out, vec![Effect::RequestsUpdate]);
    assert!(server.has_transmitter_network());
    let mut out = Vec::new();
    assert!(server.set_transmitter_network(None, false, &mut out));
    assert!(out.is_empty());
    assert!(server.is_orphan());
    // A mesma rede não faz nada.
    assert!(!server.set_transmitter_network(None, true, &mut out));

    let mut client = cable(true);
    let mut out = Vec::new();
    client.set_transmitter_network(Some(1), true, &mut out);
    client.set_transmitter_network(Some(2), true, &mut out);
    assert_eq!(
        out,
        vec![
            Effect::ClientNetworkAdd(1),
            Effect::ClientNetworkRemove(1),
            Effect::ClientNetworkAdd(2)
        ]
    );
}

#[test]
fn the_reduced_update_tag_carries_the_network_and_the_update_reads_it_back() {
    let mut a = cable(false);
    a.set_transmitter_network(
        Some(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
        true,
        &mut Vec::new(),
    );
    let tag = a.write_reduced_updated_tag();
    assert_eq!(
        tag.get(keys::NETWORK),
        Some(&Tag::IntArray(vec![
            0x0102_0304,
            0x0506_0708,
            0x090a_0b0c,
            0x0d0e_0f10
        ]))
    );
    assert!(!tag.contains_key(keys::REDSTONE));

    let mut b = cable(true);
    let result = b.handle_update_tag(&tag, &mut Vec::new());
    assert_eq!(
        result.network,
        UpdateTagNetwork::Attach(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10)
    );
    b.set_transmitter_network(
        Some(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
        true,
        &mut Vec::new(),
    );
    let again = b.handle_update_tag(&tag, &mut Vec::new());
    assert_eq!(again.network, UpdateTagNetwork::Unchanged);
    // Sem a chave, o transmissor perde a rede.
    let empty = Compound::new();
    let none = b.handle_update_tag(&empty, &mut Vec::new());
    assert_eq!(none.network, UpdateTagNetwork::Detached);
    assert!(!b.has_transmitter_network());
}

#[test]
fn can_extract_and_can_insert_follow_the_visible_type() {
    let mut t = cable(false);
    t.refresh_connections(&world_with(&[Face::East]), &mut Vec::new());
    t.set_connection_type_raw(Face::Up, ConnectionType::Pull, &mut Vec::new());
    // Sem conexão visível o lado é `NONE`: nem entra nem sai.
    assert!(!t.can_insert(Some(Face::Up)));
    assert!(!t.can_extract(Some(Face::Up)));
    // Com um transmissor do outro lado o lado é `NORMAL`.
    assert!(t.can_insert(Some(Face::East)));
    assert!(t.can_extract(Some(Face::East)));
    assert!(t.can_insert(None));
    assert!(t.can_extract(None));
}

#[test]
fn a_side_refresh_sets_and_clears_the_bits() {
    let mut t = cable(false);
    let mut out = Vec::new();
    t.refresh_connections_side(Face::East, &world_with(&[Face::East]), &mut out);
    assert_eq!(t.current_transmitter_connections, side_bit(Face::East));
    // O lado ganhou um transmissor que antes não era: pede a releitura do lado de volta, depois manda o pacote.
    assert_eq!(
        out,
        vec![
            Effect::RecheckConnection(Face::East),
            Effect::SendUpdatePacket
        ]
    );
    let mut out = Vec::new();
    t.refresh_connections_side(Face::East, &world_with(&[]), &mut out);
    assert_eq!(t.current_transmitter_connections, 0);
    assert_eq!(out, vec![Effect::SendUpdatePacket]);
    let mut out = Vec::new();
    t.refresh_connections_side(Face::East, &world_with(&[]), &mut out);
    assert!(out.is_empty());
}

#[test]
fn an_acceptor_refresh_sets_and_clears_only_its_bit() {
    let mut t = cable(false);
    let mut world = world_with(&[]);
    world.acceptor_sides = vec![Face::Up];
    let mut out = Vec::new();
    t.refresh_acceptor_connections(Face::Up, &world, &mut out);
    assert_eq!(
        t.acceptor_cache.current_acceptor_connections,
        side_bit(Face::Up)
    );
    assert_eq!(out, vec![Effect::SendUpdatePacket]);
    let mut out = Vec::new();
    t.refresh_acceptor_connections(Face::Up, &world, &mut out);
    assert!(out.is_empty());
    t.acceptor_cache.invalidate(Face::Up);
    world.acceptor_sides.clear();
    t.refresh_acceptor_connections(Face::Up, &world, &mut out);
    assert_eq!(t.acceptor_cache.current_acceptor_connections, 0);
    assert_eq!(out, vec![Effect::SendUpdatePacket]);
}

#[test]
fn the_base_recheck_refreshes_the_neighbors_only_without_a_network() {
    let mut t = cable(false);
    let world = world_with(&[Face::East, Face::West]);
    let mask = side_bit(Face::East) | side_bit(Face::Up);
    let mut out = Vec::new();
    t.base_recheck_connections(mask, &world, &mut out);
    // O de cima não tem transmissor, o leste tem: relê o lado de volta (o oeste não estava na máscara).
    assert_eq!(
        out,
        vec![Effect::RefreshNeighbor {
            pos: relative(POS, Face::East),
            side: Face::West
        }]
    );
    t.set_transmitter_network(Some(9), true, &mut Vec::new());
    let mut out = Vec::new();
    t.base_recheck_connections(mask, &world, &mut out);
    assert!(out.is_empty());
}

#[test]
fn the_sneak_click_effects_come_in_the_order_of_the_original() {
    let mut t = cable(false);
    let world = world_with(&[]);
    let mut out = Vec::new();
    t.on_sneak_right_click(
        Face::Up,
        None,
        |t, _| t.base_on_configure(),
        &world,
        &mut out,
    );
    assert_eq!(
        out,
        vec![
            Effect::SideChanged {
                side: Face::Up,
                old: ConnectionType::Normal,
                new: ConnectionType::Push
            },
            Effect::SetChanged,
            Effect::NotifyTileChange,
            Effect::Overlay(Overlay::ConnectionType {
                connection: ConnectionType::Push
            }),
            Effect::SendUpdatePacket,
        ]
    );
}

#[test]
fn the_sneak_click_that_cuts_a_transmitter_invalidates_the_network_and_the_acceptors() {
    let mut t = cable(false);
    let world = world_with(&[Face::East]);
    t.refresh_connections(&world, &mut Vec::new());
    t.set_transmitter_network(Some(1), true, &mut Vec::new());
    t.set_connection_types_raw({
        let mut types = [ConnectionType::Normal; 6];
        types[1] = ConnectionType::Normal;
        types[5] = ConnectionType::Pull;
        types
    });
    let mut out = Vec::new();
    t.on_sneak_right_click(
        Face::East,
        None,
        |t, _| t.base_on_configure(),
        &world,
        &mut out,
    );
    assert_eq!(t.get_connection_type_raw(Face::East), ConnectionType::None);
    assert_eq!(t.current_transmitter_connections, 0);
    let mut expected = vec![
        Effect::SideChanged {
            side: Face::East,
            old: ConnectionType::Pull,
            new: ConnectionType::None,
        },
        Effect::AcceptorChanged(Face::East),
        Effect::NotifyTileChange,
        Effect::InvalidateTransmitter,
        Effect::SetChanged,
    ];
    // `getPossibleAcceptorConnections` marca sujo todo lado sem aceitador, na ordem dos lados.
    expected.extend(Face::ALL.iter().map(|side| Effect::AcceptorChanged(*side)));
    expected.extend([
        Effect::SendUpdatePacket,
        Effect::NotifyTileChange,
        Effect::Overlay(Overlay::ConnectionType {
            connection: ConnectionType::None,
        }),
        Effect::SendUpdatePacket,
    ]);
    assert_eq!(out, expected);
}

#[test]
fn a_transmitter_of_another_type_can_be_an_acceptor_and_one_of_our_type_cannot() {
    let mut t = cable(false);
    let mut world = world_with(&[Face::East]);
    world.acceptor_sides = vec![Face::East];
    // O vizinho é do nosso tipo: é transmissor, não aceitador, ainda que a capability exista.
    t.refresh_connections(&world, &mut Vec::new());
    assert_eq!(t.current_transmitter_connections, side_bit(Face::East));
    assert_eq!(t.acceptor_cache.current_acceptor_connections, 0);
    // O vizinho é de outro tipo (um cano ao lado do cabo): não é transmissor para nós, e a capability o faz aceitador.
    let mut t = cable(false);
    world.neighbor_kind = TransmissionType::Fluid;
    t.refresh_connections(&world, &mut Vec::new());
    assert_eq!(t.current_transmitter_connections, 0);
    assert_eq!(
        t.acceptor_cache.current_acceptor_connections,
        side_bit(Face::East)
    );
}
