//! A mira do Configurator num transmissor: as caixas de `TileEntityTransmitter.getCollisionBoxes` (um braço por lado ligado e o centro por último) e o lado que o raio
//! toca (`getSideLookingAt`), contra o algoritmo do original. A forma de estado do Horizon (`pipe_shape`) junta o centro e os braços numa união: dois braços opostos viram uma
//! caixa só, de ponta a ponta, e o lado que se tirava dela era sempre o primeiro da conta (o oeste, mirando o braço leste); por isso a mira usa as peças e não a forma.
use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::connection::ConnectionType;
use mekanism_rules::{
    TransmitterPiece, clip_box, looked_piece, partial_shape, side_looking_at, transmitter_center,
    transmitter_pieces,
};

const CABLE: &str = "mekanism:basic_universal_cable";
const PIPE: &str = "mekanism:basic_mechanical_pipe";

fn kinds(connected: &[Face]) -> [ConnectionType; 6] {
    let mut kinds = [ConnectionType::None; 6];
    for side in connected {
        kinds[*side as usize] = ConnectionType::Normal;
    }
    kinds
}

fn pixels(bounds: [f64; 6]) -> [f64; 6] {
    bounds.map(|value| value * 16.0)
}

/// As caixas de `BlockSmallTransmitter` e `BlockLargeTransmitter`, escritas como no Java (`box(x1, y1, z1, x2, y2, z2)` do braço que desce, em pixels) e giradas à mão para o
/// lado leste, que é onde um `x` de 11 a 16 mostra o espelho.
#[test]
fn the_arm_boxes_are_the_ones_of_the_java_shapes() {
    let all = |block: &str, kind: ConnectionType| {
        let mut set = [ConnectionType::None; 6];
        set[Face::East as usize] = kind;
        transmitter_pieces(block, &set).unwrap()[0]
            .boxes
            .iter()
            .map(|bounds| pixels(*bounds))
            .collect::<Vec<_>>()
    };
    // `SIDES` do pequeno: `box(5, 0, 5, 11, 5, 11)`.
    assert_eq!(
        all(CABLE, ConnectionType::Normal),
        vec![[11.0, 5.0, 5.0, 16.0, 11.0, 11.0]]
    );
    // `SIDES_PUSH`: `box(5, 3, 5, 11, 5, 11)`, `box(6, 1, 6, 10, 3, 10)`, `box(7, 0, 7, 9, 1, 9)`.
    assert_eq!(
        all(CABLE, ConnectionType::Push),
        vec![
            [11.0, 5.0, 5.0, 13.0, 11.0, 11.0],
            [13.0, 6.0, 6.0, 15.0, 10.0, 10.0],
            [15.0, 7.0, 7.0, 16.0, 9.0, 9.0],
        ]
    );
    // `SIDES_PULL`: `box(5, 4, 5, 11, 5, 11)`, `box(6, 2, 6, 10, 4, 10)`, `box(4, 0, 4, 12, 2, 12)`.
    assert_eq!(
        all(CABLE, ConnectionType::Pull),
        vec![
            [11.0, 5.0, 5.0, 12.0, 11.0, 11.0],
            [12.0, 6.0, 6.0, 14.0, 10.0, 10.0],
            [14.0, 4.0, 4.0, 16.0, 12.0, 12.0],
        ]
    );
    // O grande: `box(4, 0, 4, 12, 4, 12)`, `SIDES_PUSH` (`box(4, 3, ..)`, `box(5, 1, ..)`, `box(6, 0, ..)`) e `SIDES_PULL` (`box(4, 3, ..)`, `box(5, 2, ..)`, `box(3, 0, ..)`).
    assert_eq!(
        all(PIPE, ConnectionType::Normal),
        vec![[12.0, 4.0, 4.0, 16.0, 12.0, 12.0]]
    );
    assert_eq!(
        all(PIPE, ConnectionType::Push),
        vec![
            [12.0, 4.0, 4.0, 13.0, 12.0, 12.0],
            [13.0, 5.0, 5.0, 15.0, 11.0, 11.0],
            [15.0, 6.0, 6.0, 16.0, 10.0, 10.0],
        ]
    );
    assert_eq!(
        all(PIPE, ConnectionType::Pull),
        vec![
            [12.0, 4.0, 4.0, 13.0, 12.0, 12.0],
            [13.0, 5.0, 5.0, 14.0, 11.0, 11.0],
            [14.0, 3.0, 3.0, 16.0, 13.0, 13.0],
        ]
    );
    // O lado negativo não espelha: oeste, norte e baixo ficam de 0 ao fim do braço.
    let mut west = [ConnectionType::None; 6];
    west[Face::West as usize] = ConnectionType::Normal;
    assert_eq!(
        pixels(transmitter_pieces(CABLE, &west).unwrap()[0].boxes[0]),
        [0.0, 5.0, 5.0, 5.0, 11.0, 11.0]
    );
    let mut up = [ConnectionType::None; 6];
    up[Face::Up as usize] = ConnectionType::Normal;
    assert_eq!(
        pixels(transmitter_pieces(CABLE, &up).unwrap()[0].boxes[0]),
        [5.0, 11.0, 5.0, 11.0, 16.0, 11.0]
    );
    // Um bloco que não é transmissor não tem peças.
    assert!(
        transmitter_pieces("mekanism:enrichment_chamber", &[ConnectionType::None; 6]).is_none()
    );
}

/// A ordem da lista (um braço por lado cujo tipo não é `NONE`, os lados na ordem de `Direction`, e o centro por último).
#[test]
fn the_pieces_are_the_arms_in_side_order_then_the_center() {
    let mut set = [ConnectionType::None; 6];
    set[Face::East as usize] = ConnectionType::Pull;
    set[Face::Down as usize] = ConnectionType::Normal;
    set[Face::North as usize] = ConnectionType::Push;
    let pieces = transmitter_pieces(CABLE, &set).unwrap();
    let sides: Vec<Option<Face>> = pieces.iter().map(|piece| piece.side).collect();
    assert_eq!(
        sides,
        vec![Some(Face::Down), Some(Face::North), Some(Face::East), None]
    );
    assert_eq!(
        pieces.last().unwrap().boxes,
        vec![transmitter_center(CABLE).unwrap()]
    );
    assert_eq!(pieces[1].boxes.len(), 3, "o braço PUSH é de três caixas");
    // Sem conexão nenhuma só resta o centro.
    let alone = transmitter_pieces(CABLE, &[ConnectionType::None; 6]).unwrap();
    assert_eq!(alone.len(), 1);
    assert_eq!(alone[0].side, None);
}

fn ray(from: [f64; 3], to: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    (from, to)
}

/// O caso do jogo: dois cabos em linha, cada um com um braço para o oeste e outro para o leste. O olhar no braço leste tem de dar leste (a forma de estado do Horizon, uma
/// caixa só de ponta a ponta, dava o oeste), e o olhar no braço oeste, oeste.
#[test]
fn looking_at_the_east_arm_of_a_cable_in_a_line_gives_east() {
    let pieces = transmitter_pieces(CABLE, &kinds(&[Face::West, Face::East])).unwrap();
    // Do alto, no meio do braço leste (x de 11 a 16 pixels) e do oeste (de 0 a 5).
    let (origin, end) = ray([13.5 / 16.0, 2.0, 0.5], [13.5 / 16.0, -1.0, 0.5]);
    assert_eq!(side_looking_at(&pieces, origin, end), Some(Face::East));
    let (origin, end) = ray([2.5 / 16.0, 2.0, 0.5], [2.5 / 16.0, -1.0, 0.5]);
    assert_eq!(side_looking_at(&pieces, origin, end), Some(Face::West));
    // Do alto, no centro: nenhum braço (o centro vem antes dos dois, e o raio vertical só toca o centro).
    let (origin, end) = ray([0.5, 2.0, 0.5], [0.5, -1.0, 0.5]);
    assert_eq!(side_looking_at(&pieces, origin, end), None);
    // Pela ponta do braço leste olhando para o oeste: o braço é a primeira peça que o raio toca.
    let (origin, end) = ray([3.0, 0.5, 0.5], [-2.0, 0.5, 0.5]);
    assert_eq!(side_looking_at(&pieces, origin, end), Some(Face::East));
    // Um raio que erra tudo não dá lado.
    let (origin, end) = ray([0.5, 2.0, 3.0], [0.5, -1.0, 3.0]);
    assert_eq!(side_looking_at(&pieces, origin, end), None);
}

/// A seleção parcial desenha a peça tocada: o braço normal inteiro, o centro, e o centro se o raio erra.
#[test]
fn the_partial_shape_is_the_piece_the_ray_touches() {
    let pieces = transmitter_pieces(CABLE, &kinds(&[Face::West, Face::East])).unwrap();
    let (origin, end) = ray([13.5 / 16.0, 2.0, 0.5], [13.5 / 16.0, -1.0, 0.5]);
    assert_eq!(
        pixels(partial_shape(&pieces, origin, end)),
        [11.0, 5.0, 5.0, 16.0, 11.0, 11.0]
    );
    let (origin, end) = ray([0.5, 2.0, 0.5], [0.5, -1.0, 0.5]);
    assert_eq!(
        partial_shape(&pieces, origin, end),
        transmitter_center(CABLE).unwrap()
    );
    let (origin, end) = ray([0.5, 2.0, 3.0], [0.5, -1.0, 3.0]);
    assert_eq!(
        partial_shape(&pieces, origin, end),
        transmitter_center(CABLE).unwrap()
    );
    // Um braço `PUSH` (três caixas) vale pela menor caixa que o contém.
    let mut push = [ConnectionType::None; 6];
    push[Face::East as usize] = ConnectionType::Push;
    let pieces = transmitter_pieces(CABLE, &push).unwrap();
    let (origin, end) = ray([15.5 / 16.0, 2.0, 0.5], [15.5 / 16.0, -1.0, 0.5]);
    assert_eq!(
        pixels(partial_shape(&pieces, origin, end)),
        [11.0, 5.0, 5.0, 16.0, 11.0, 11.0]
    );
}

/// `TileEntityTransmitter.getSideLookingAt` escrito como no Java, sobre a mesma lista de caixas: o `subHit` do `collisionRayTrace` começa em -1 (`int i = -1`), a lista é a
/// dos lados de `getAllCurrentConnections` e o índice é `subHit + 1`; fora da lista, nulo.
fn java_side_looking_at(
    connected: &[Face],
    pieces: &[TransmitterPiece],
    origin: [f64; 3],
    end: [f64; 3],
) -> Option<Face> {
    let mut min_distance = f64::INFINITY;
    let mut hit: Option<i32> = None;
    let mut i = -1;
    for piece in pieces {
        let nearest = piece
            .boxes
            .iter()
            .filter_map(|bounds| clip_box(origin, end, bounds))
            .min_by(f64::total_cmp);
        if let Some(distance) = nearest
            && distance < min_distance
        {
            min_distance = distance;
            hit = Some(i);
        }
        i += 1;
    }
    let list: Vec<Face> = Face::ALL
        .into_iter()
        .filter(|side| connected.contains(side))
        .collect();
    let index = usize::try_from(hit? + 1).ok()?;
    list.get(index).copied()
}

/// As 64 combinações de braços (do tipo normal), com raios de cada lado do cubo sobre uma grade de 5 por 5: a mira das peças dá o que o algoritmo do original dá, e quem
/// olha para o braço que existe nunca recebe outro lado.
#[test]
fn the_side_looked_at_is_what_the_java_algorithm_gives_for_every_set_of_arms() {
    let mut checked = 0;
    let mut arm_hits = 0;
    for mask in 0..64u8 {
        let connected: Vec<Face> = Face::ALL
            .into_iter()
            .filter(|side| mask & (1 << (*side as u8)) != 0)
            .collect();
        let pieces = transmitter_pieces(CABLE, &kinds(&connected)).unwrap();
        for axis in 0..3 {
            for sign in [-1.0f64, 1.0] {
                for u in 0..5 {
                    for v in 0..5 {
                        let mut from = [0.0f64; 3];
                        let mut to = [0.0f64; 3];
                        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
                        let (ua, vb) = (0.3 + 0.1 * f64::from(u), 0.3 + 0.1 * f64::from(v));
                        from[a] = ua;
                        from[b] = vb;
                        to[a] = ua;
                        to[b] = vb;
                        from[axis] = 0.5 + 2.0 * sign;
                        to[axis] = 0.5 - 2.0 * sign;
                        let got = side_looking_at(&pieces, from, to);
                        assert_eq!(
                            got,
                            java_side_looking_at(&connected, &pieces, from, to),
                            "arms {connected:?}, ray {from:?} -> {to:?}"
                        );
                        if let Some(side) = got {
                            assert!(connected.contains(&side), "o lado {side:?} não tem braço");
                            arm_hits += 1;
                        }
                        // O raio que dá um braço é o que toca o braço antes do centro; o `looked_piece` e o lado concordam.
                        assert_eq!(
                            looked_piece(&pieces, from, to).and_then(|piece| piece.side),
                            got
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 64 * 6 * 25);
    assert!(arm_hits > 2_000, "a grade toca braços: {arm_hits}");
}

/// A conta que o jogo fazia antes: a caixa mais perto da forma de estado (`facts.outline`, a união do centro e dos braços) e o lado achado pelas coordenadas dela. Fica como
/// registro da causa: num cabo em linha a união é uma caixa só, de x 0 a 1, e o primeiro lado que bate é o oeste, mirando o braço leste ou o oeste.
#[test]
fn the_old_side_from_the_merged_state_shape_was_the_west_for_a_line() {
    let merged = [0.0, 5.0 / 16.0, 5.0 / 16.0, 1.0, 11.0 / 16.0, 11.0 / 16.0];
    let side_of = |nearest: [f64; 6]| {
        if nearest[1] == 0.0 {
            Face::Down
        } else if nearest[4] == 1.0 {
            Face::Up
        } else if nearest[2] == 0.0 {
            Face::North
        } else if nearest[5] == 1.0 {
            Face::South
        } else if nearest[0] == 0.0 {
            Face::West
        } else {
            Face::East
        }
    };
    let (origin, end) = ray([13.5 / 16.0, 2.0, 0.5], [13.5 / 16.0, -1.0, 0.5]);
    assert!(
        clip_box(origin, end, &merged).is_some(),
        "o raio toca a caixa única"
    );
    assert_eq!(
        side_of(merged),
        Face::West,
        "o lado que saía era o oeste, olhando o braço leste"
    );
}

/// Um raio que toca duas peças à mesma distância fica com a primeira da lista (`distance < minDistance`, sem `<=`): o raio que desce pela borda entre o braço leste e o centro
/// (x de 11 pixels) entra nos dois no mesmo y, e o braço vem antes do centro.
#[test]
fn a_tie_between_pieces_goes_to_the_first_in_the_list() {
    let pieces = transmitter_pieces(CABLE, &kinds(&[Face::East])).unwrap();
    let (origin, end) = ray([11.0 / 16.0, 2.0, 0.5], [11.0 / 16.0, -1.0, 0.5]);
    let arm = clip_box(origin, end, &pieces[0].boxes[0]).unwrap();
    let center = clip_box(origin, end, &pieces[1].boxes[0]).unwrap();
    assert_eq!(arm, center, "os dois à mesma distância");
    assert_eq!(side_looking_at(&pieces, origin, end), Some(Face::East));
    // A peça de três caixas vale pela primeira que o raio entra (a mais perto), e não pela última: `A` entra em x = 1 e só depois em x = 0,1; `B` fica no meio.
    let thin = |from: f64, to: f64| [from, 0.4, 0.4, to, 0.6, 0.6];
    let a = TransmitterPiece {
        side: Some(Face::East),
        boxes: vec![thin(0.9, 1.0), thin(0.0, 0.1)],
    };
    let b = TransmitterPiece {
        side: Some(Face::West),
        boxes: vec![thin(0.4, 0.6)],
    };
    let (origin, end) = ray([2.0, 0.5, 0.5], [-1.0, 0.5, 0.5]);
    assert_eq!(
        looked_piece(&[a.clone(), b.clone()], origin, end)
            .unwrap()
            .side,
        Some(Face::East)
    );
    assert_eq!(
        looked_piece(&[b, a], origin, end).unwrap().side,
        Some(Face::East)
    );
}
