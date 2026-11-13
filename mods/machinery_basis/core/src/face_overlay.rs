//! A sobreposição colorida que o Configurator põe na face que o olhar toca (`ConfiguratorOverlayHandler`): os quatro vértices da face, deslocados 0,01 para
//! fora, e a rotação que gira o desenho das faces de cima e de baixo para o jogador ler o ícone de pé. O sprite e a cor são de quem usa.

use crate::sides::Face;

/// `ConfiguratorOverlayHandler`: a sobreposição fica a 0,01 da face, para fora.
pub const OFFSET: f32 = 0.01;

/// `poseStack.scale(0.5F, 0.5F, 0.5F)` do desviador.
pub const DIVERSION_SCALE: f32 = 0.5;

/// `poseStack.translate(0.5, 0.5, 0.5)` do desviador (na escala de meio: 0,25 de bloco).
pub const DIVERSION_TRANSLATE: f32 = 0.5;

/// Um vértice da sobreposição: a posição relativa ao canto do bloco e o uv normalizado do sprite (0 a 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// `FaceInfo.fromFacing(face)`: a extensão de cada um dos quatro vértices (0 é o mínimo do eixo, 1 o máximo).
fn face_info(face: Face) -> [[f32; 3]; 4] {
    match face {
        Face::Down => [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.], [1., 0., 1.]],
        Face::Up => [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
        Face::North => [[1., 1., 0.], [1., 0., 0.], [0., 0., 0.], [0., 1., 0.]],
        Face::South => [[0., 1., 1.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.]],
        Face::West => [[0., 1., 0.], [0., 0., 0.], [0., 0., 1.], [0., 1., 1.]],
        Face::East => [[1., 1., 1.], [1., 0., 1.], [1., 0., 0.], [1., 1., 0.]],
    }
}

/// `Direction.fromYRot`: a direção horizontal para onde a câmera olha (`from2DDataValue(floor(yRot / 90 + 0.5) & 3)`: 0 sul, 1 oeste, 2 norte, 3 leste).
pub fn camera_facing(y_rot: f64) -> Face {
    match ((y_rot / 90.0 + 0.5).floor() as i64) & 3 {
        0 => Face::South,
        1 => Face::West,
        2 => Face::North,
        _ => Face::East,
    }
}

/// A rotação em torno de `(0,5; 1; 0,5)` que o `ConfiguratorOverlayHandler` aplica nas faces de cima e de baixo (`V_ROT`): sul 180 graus, oeste 90, leste 270
/// no eixo Y; norte não gira.
fn top_bottom_rotation(facing: Face) -> Option<f32> {
    match facing {
        Face::South => Some(180.0),
        Face::West => Some(90.0),
        Face::East => Some(270.0),
        _ => None,
    }
}

/// Os quatro vértices da face `face` do bloco, com a rotação para a câmera de `y_rot` graus nas faces de cima e de baixo. A rotação é a de um quaternion
/// (`setAngleAxis` de `Mth.DEG_TO_RAD`) em float, como o JOML a aplica.
pub fn vertices(face: Face, y_rot: f64) -> [Vertex; 4] {
    build(face, y_rot, false)
}

/// `ConfiguratorOverlayHandler` com `isDiversion`: depois da rotação das faces de cima e de baixo o `PoseStack` faz `scale(0.5F)` e `translate(0.5, 0.5, 0.5)`, então o
/// quadrilátero vira o de meio bloco que cobre a face do núcleo do desviador (de 0,25 a 0,75, mais os 0,01 da face na escala de meio: 0,005 para fora).
pub fn diversion_vertices(face: Face, y_rot: f64) -> [Vertex; 4] {
    build(face, y_rot, true)
}

fn build(face: Face, y_rot: f64, diversion: bool) -> [Vertex; 4] {
    let step = face.offset();
    let rotation = matches!(face, Face::Up | Face::Down)
        .then(|| top_bottom_rotation(camera_facing(y_rot)))
        .flatten();
    let mut out = [Vertex {
        position: [0.0; 3],
        uv: [0.0; 2],
    }; 4];
    for (vertex, extent) in face_info(face).iter().enumerate() {
        let mut position = [
            extent[0] + step[0] as f32 * OFFSET,
            extent[1] + step[1] as f32 * OFFSET,
            extent[2] + step[2] as f32 * OFFSET,
        ];
        if diversion {
            // `scale(0.5F)` e depois `translate(0.5, 0.5, 0.5)` na matriz: `0.5 * x + 0.25` por eixo (o `m30` do JOML é `0.5 * 0.5`).
            position =
                position.map(|axis| DIVERSION_SCALE * axis + DIVERSION_SCALE * DIVERSION_TRANSLATE);
        }
        if let Some(degrees) = rotation {
            position = rotate_about_y(position, degrees, [0.5, 1.0, 0.5]);
        }
        out[vertex] = Vertex {
            position,
            uv: [
                if vertex < 2 { 0.0 } else { 1.0 },
                if vertex == 0 || vertex == 3 { 0.0 } else { 1.0 },
            ],
        };
    }
    out
}

/// `PoseStack.rotateAround(quaternion, x, y, z)`: gira em torno do ponto `pivot` o quaternion do eixo Y.
fn rotate_about_y(position: [f32; 3], degrees: f32, pivot: [f32; 3]) -> [f32; 3] {
    // `Quaternionf.setAngleAxis(angle, 0, 1, 0)`: (0, sin(angle/2), 0, cos(angle/2)).
    let angle = degrees * crate::mth::DEG_TO_RAD;
    let half = angle * 0.5;
    let (sin, cos) = (half.sin(), half.cos());
    let (x, y, z) = (
        position[0] - pivot[0],
        position[1] - pivot[1],
        position[2] - pivot[2],
    );
    // Rotação do vetor pelo quaternion (0, s, 0, c) em torno de Y.
    let rx = x * (cos * cos - sin * sin) + z * (2.0 * sin * cos);
    let rz = -x * (2.0 * sin * cos) + z * (cos * cos - sin * sin);
    [rx + pivot[0], y + pivot[1], rz + pivot[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1.0e-5)
    }

    #[test]
    fn a_side_face_is_the_plain_face_pushed_out_by_a_hundredth() {
        let south = vertices(Face::South, 0.0);
        assert!(close(south[0].position, [0.0, 1.0, 1.01]));
        assert!(close(south[2].position, [1.0, 0.0, 1.01]));
        let west = vertices(Face::West, 0.0);
        assert!(close(west[1].position, [-0.01, 0.0, 0.0]));
    }

    #[test]
    fn the_uv_runs_over_the_four_corners() {
        let uv: Vec<[f32; 2]> = vertices(Face::North, 0.0).iter().map(|v| v.uv).collect();
        assert_eq!(uv, vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    }

    #[test]
    fn the_camera_facing_is_from_y_rot() {
        assert_eq!(camera_facing(0.0), Face::South);
        assert_eq!(camera_facing(90.0), Face::West);
        assert_eq!(camera_facing(180.0), Face::North);
        assert_eq!(camera_facing(270.0), Face::East);
        assert_eq!(camera_facing(-90.0), Face::East);
        assert_eq!(camera_facing(44.9), Face::South);
        assert_eq!(camera_facing(45.0), Face::West);
        assert_eq!(camera_facing(360.0), Face::South);
    }

    #[test]
    fn the_top_face_only_turns_when_the_camera_is_not_facing_north() {
        let plain = vertices(Face::Up, 180.0);
        assert!(close(plain[0].position, [0.0, 1.01, 0.0]));
        // Sul: meia volta em torno do centro da face.
        let south = vertices(Face::Up, 0.0);
        assert!(close(south[0].position, [1.0, 1.01, 1.0]));
        assert!(close(south[2].position, [0.0, 1.01, 0.0]));
        // Oeste: 90 graus.
        let west = vertices(Face::Up, 90.0);
        assert!(close(west[0].position, [0.0, 1.01, 1.0]));
        // Leste: 270 graus.
        let east = vertices(Face::Up, 270.0);
        assert!(close(east[0].position, [1.0, 1.01, 0.0]));
    }

    #[test]
    fn side_faces_do_not_turn_with_the_camera() {
        assert_eq!(vertices(Face::East, 0.0), vertices(Face::East, 90.0));
    }
}
