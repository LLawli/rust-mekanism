//! O conteúdo animado dos transmissores (`RenderTransmitterBase`, `RenderUniversalCable`, `RenderMechanicalPipe`, `RenderPressurizedTube`, `RenderThermodynamicConductor`,
//! `TransmitterContentsManager`, Fase 4): a conta pura da cor, do alfa, do brilho, da altura do líquido e dos cubos do cano, sem o jogo. A lib não conhece fluido nem químico do
//! Mekanism: a cor base, o fluido e o químico entram como dados de quem chama.
//!
//! Tudo aqui é do Java: `ARGB.as8BitChannel` (`floor(valor * 255)`), `MekanismRenderer.getColorARGB`, `ModelRenderer.getStage`, `HeatUtils.getColorFromTemp` com o `colorCache`
//! do processo, `LightCoordsUtil.lightCoordsWithEmission` e `RenderResizableCuboid.renderCube` (a parte que cabe num bloco).

use crate::sides::Face;
use crate::transmission::connection::ConnectionType;
use std::collections::HashMap;

/// `ARGB.as8BitChannel(float)`.
pub fn as_8bit_channel(value: f32) -> i32 {
    (value * 255.0).floor() as i32
}

/// `ARGB.color(float alpha, int rgb)`.
pub fn argb_color(alpha: f32, rgb: u32) -> u32 {
    (as_8bit_channel(alpha) as u32) << 24 | (rgb & 0xFF_FFFF)
}

/// `ARGB.white(float alpha)`.
pub fn white(alpha: f32) -> u32 {
    (as_8bit_channel(alpha) as u32) << 24 | 0xFF_FFFF
}

/// `RenderUniversalCable.extractRenderState`: `ARGB.white(network.currentScale)`.
pub fn energy_tint(scale: f32) -> u32 {
    white(scale)
}

/// `RenderUniversalCable.shouldRenderTransmitter`: sem rede ou com escala zero o cabo não desenha o conteúdo (a energia nem chega ao cliente).
pub fn cable_shows_contents(has_network: bool, scale: f32) -> bool {
    has_network && scale > 0.0
}

/// `RenderPressurizedTube.extractRenderState`: `ARGB.color(max(0.2F, scale), tint)`.
pub fn tube_tint(scale: f32, tint: u32) -> u32 {
    argb_color(scale.max(0.2), tint)
}

/// `RenderMechanicalPipe.shouldRenderTransmitter` e o do tubo: precisam de rede, de um último tipo, do contêiner cheio de alguma coisa e de escala.
pub fn resource_shows_contents(
    has_network: bool,
    has_last_type: bool,
    container_empty: bool,
    scale: f32,
) -> bool {
    has_network && has_last_type && !container_empty && scale > 0.0
}

/// `MekanismRenderer.getColorARGB(int argb, float alpha)`.
pub fn color_argb(argb: u32, alpha: f32) -> u32 {
    if alpha >= 1.0 {
        argb | 0xFF00_0000
    } else if alpha <= 0.0 {
        argb & 0xFF_FFFF
    } else {
        argb_color(alpha, argb)
    }
}

/// `MekanismRenderer.getColorARGB(FluidResource, float)`: só o fluido mais leve que o ar e gasoso leva a escala no alfa (com 0,2 a mais); os outros ficam com a cor do fluido.
pub fn fluid_tint(color: u32, lighter_than_air: bool, scale: f32) -> u32 {
    if lighter_than_air {
        color_argb(color, (scale + 0.2).min(1.0))
    } else {
        color
    }
}

/// `RenderMechanicalPipe.STAGES`.
/// `RenderTransmitterBase.submit`: `poseStack.translate(0.5, 0.5, 0.5)` antes do modelo de conteúdo, cujos vértices (o OBJ `transmitter_contents`) estão em torno do centro do bloco.
pub const CONTENTS_MODEL_ORIGIN: [f32; 3] = [0.5, 0.5, 0.5];

pub const STAGES: i32 = 100;
/// `RenderMechanicalPipe.HEIGHT`.
pub const HEIGHT: f32 = 0.45;
/// `RenderMechanicalPipe.OFFSET`.
pub const OFFSET: f32 = 0.02;

/// `ModelRenderer.getStage(boolean, int, double)`.
pub fn stage_of(gaseous: bool, stages: i32, scale: f64) -> i32 {
    if gaseous {
        stages - 1
    } else {
        (stages - 1).min((scale * f64::from(stages - 1)) as i32)
    }
}

/// O estágio que o cano usa: `Math.max(3, getStage(fluid, 100, scale))`.
pub fn pipe_stage(lighter_than_air: bool, scale: f32) -> i32 {
    stage_of(lighter_than_air, STAGES, f64::from(scale)).max(3)
}

/// `LightCoordsUtil.lightCoordsWithEmission`, com o céu e o bloco em níveis de 0 a 15 (`(céu, bloco)`).
pub fn light_with_emission(sky: u32, block: u32, emission: u32) -> (u32, u32) {
    if emission == 0 {
        (sky, block)
    } else {
        (sky.max(emission), block.max(emission))
    }
}

/// `Color`: quatro canais `double` de 0 a 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// `Math.round(double)`: o meio sobe.
fn java_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

impl Color {
    pub const fn rgbad(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    /// `Color.argb()`: cada canal em `Math.round(valor * 255)`.
    pub fn argb(self) -> u32 {
        let channel = |value: f64| (java_round(value * 255.0) & 0xFF) as u32;
        (java_round(self.a * 255.0) as u32) << 24
            | channel(self.r) << 16
            | channel(self.g) << 8
            | channel(self.b)
    }

    /// `Color.blendOnto`.
    pub fn blend_onto(self, base: Color) -> Color {
        let (s_a, d_a) = (self.a, base.a);
        Color {
            r: self.r * s_a + base.r * (1.0 - s_a),
            g: self.g * s_a + base.g * (1.0 - s_a),
            b: self.b * s_a + base.b * (1.0 - s_a),
            a: d_a + s_a * (1.0 - d_a),
        }
    }
}

/// `HeatUtils.colorCache`: do processo, por balde de 100 K. A primeira temperatura que cai num balde fixa a cor dele, com o alfa dela (`temperature / 1000`); os outros condutores
/// do mesmo balde usam essa mesma cor. É o comportamento do original.
#[derive(Default, Debug)]
pub struct HeatColorCache {
    colors: HashMap<i32, Color>,
}

impl HeatColorCache {
    /// `HeatUtils.getColorFromTemp(temperature, baseColor)`.
    pub fn color_from_temp(&mut self, temperature: f64, base: Color) -> Color {
        let abs_temp = (temperature + crate::heat::AMBIENT_TEMP) / 100.0;
        let key = abs_temp as i32;
        if let Some(color) = self.colors.get(&key) {
            return color.blend_onto(base);
        }
        let effective = abs_temp.clamp(10.0, 400.0);
        let red = if effective <= 66.0 {
            1.0
        } else {
            329.698727446 * (effective - 60.0).powf(-0.1332047592) / 255.0
        };
        let green_calc = if effective <= 66.0 {
            99.4708025861 * effective.ln() - 161.1195681661
        } else {
            288.1221695283 * (effective - 60.0).powf(-0.0755148492)
        };
        let green = green_calc / 255.0;
        let blue = if effective >= 66.0 {
            1.0
        } else if effective <= 19.0 {
            0.0
        } else {
            (138.5177312231 * (effective - 10.0).ln() - 305.0447927307) / 255.0
        };
        let alpha = temperature / 1_000.0;
        let color = Color::rgbad(
            red.clamp(0.0, 1.0),
            green.clamp(0.0, 1.0),
            blue.clamp(0.0, 1.0),
            alpha.clamp(0.0, 1.0),
        );
        self.colors.insert(key, color);
        color.blend_onto(base)
    }
}

/// `ConductorTier.getBaseColor`: `Color.rgbad(0.2, 0.2, 0.2, 1)` nos quatro níveis.
pub const CONDUCTOR_BASE_COLOR: Color = Color::rgbad(0.2, 0.2, 0.2, 1.0);

/// O nome do lado no OBJ do conteúdo (`Direction.getSerializedName`).
pub fn side_name(side: Face) -> &'static str {
    match side {
        Face::Down => "down",
        Face::Up => "up",
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
    }
}

/// O nome do tipo de conexão no OBJ do conteúdo (`ConnectionType.name()`).
pub fn type_name(kind: ConnectionType) -> &'static str {
    match kind {
        ConnectionType::Normal => "NORMAL",
        ConnectionType::Push => "PUSH",
        ConnectionType::Pull => "PULL",
        ConnectionType::None => "NONE",
    }
}

/// Os grupos do `transmitter_contents.obj` que valem (`TransmitterContentsManager.getBaked`): `<lado><TIPO>` para cada lado com tipo; sem nenhum tipo (todos `null`, a chave
/// zero) o modelo é vazio.
pub fn contents_groups(types: [Option<ConnectionType>; 6]) -> Vec<String> {
    Face::ALL
        .iter()
        .zip(types)
        .filter_map(|(side, kind)| {
            kind.map(|kind| format!("{}{}", side_name(*side), type_name(kind)))
        })
        .collect()
}

/// Os 24 grupos do OBJ, na ordem do lado e do tipo (`ConnectionType.VALUES`).
pub fn all_groups() -> Vec<String> {
    Face::ALL
        .iter()
        .flat_map(|side| {
            ConnectionType::VALUES
                .iter()
                .map(move |kind| format!("{}{}", side_name(*side), type_name(*kind)))
        })
        .collect()
}

/// `RenderThermodynamicConductor`/`RenderUniversalCable`: o tipo de cada lado inteiro, o do cabo, do tubo e do condutor (`setContentsModel`).
pub fn all_sides(types: [ConnectionType; 6]) -> [Option<ConnectionType>; 6] {
    types.map(Some)
}

/// `RenderMechanicalPipe.extractRenderState`: só `PUSH` e `PULL` entram no modelo de conteúdo; os outros lados saem nos cubos.
pub fn pipe_contents_types(types: [ConnectionType; 6]) -> [Option<ConnectionType>; 6] {
    types.map(|kind| matches!(kind, ConnectionType::Push | ConnectionType::Pull).then_some(kind))
}

/// `SideRender.of(Direction)`: um bit por lado, na ordem do `Direction`.
pub const fn side_bit(side: Face) -> u8 {
    1 << (side as u8)
}

/// `SideRender.ALL_FACES`.
pub const ALL_FACES: u8 = 0b11_1111;

fn opposite(side: Face) -> Face {
    match side {
        Face::Down => Face::Up,
        Face::Up => Face::Down,
        Face::North => Face::South,
        Face::South => Face::North,
        Face::West => Face::East,
        Face::East => Face::West,
    }
}

fn is_vertical(side: Face) -> bool {
    matches!(side, Face::Down | Face::Up)
}

/// O que `RenderMechanicalPipe.extractRenderState` decide dos tipos de conexão e do estágio.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeFrame {
    pub stage: i32,
    pub render_base: bool,
    pub core_sides: u8,
    /// `renderSideModel`: o lado `NORMAL` leva um cubo próprio, do tamanho certo.
    pub side_cubes: [bool; 6],
}

pub fn pipe_frame(types: [ConnectionType; 6], stage: i32) -> PipeFrame {
    let mut has_horizontal = false;
    let mut vertical_sides = 0;
    let mut render_sides = [false; 6];
    for (index, side) in Face::ALL.iter().enumerate() {
        let kind = types[index];
        render_sides[index] = kind != ConnectionType::Normal;
        if kind != ConnectionType::None {
            if is_vertical(*side) {
                vertical_sides += 1;
            } else {
                has_horizontal = true;
            }
        }
    }
    let render_base = has_horizontal || vertical_sides < 2;
    let mut core_sides = 0;
    for (index, side) in Face::ALL.iter().enumerate() {
        if render_sides[index] || (render_base && stage != STAGES - 1 && is_vertical(*side)) {
            core_sides |= side_bit(*side);
        }
    }
    PipeFrame {
        stage,
        render_base,
        core_sides,
        side_cubes: types.map(|kind| kind == ConnectionType::Normal),
    }
}

/// Um cubo do `RenderResizableCuboid.renderCube`: o canto de baixo, o de cima e os lados que saem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuboid {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub sides: u8,
}

/// Os cubos do cano na ordem de `RenderMechanicalPipe.submit`: o do braço de cada lado `NORMAL` (na ordem do `Direction`) e depois o do núcleo.
pub fn pipe_cuboids(frame: &PipeFrame) -> Vec<Cuboid> {
    let stage_ratio = (frame.stage as f32 / STAGES as f32) * HEIGHT;
    let mut cuboids = Vec::new();
    for (index, side) in Face::ALL.iter().enumerate() {
        if !frame.side_cubes[index] {
            continue;
        }
        let sides = ALL_FACES ^ side_bit(*side) ^ side_bit(opposite(*side));
        let (min, max);
        if !is_vertical(*side) {
            let (min_y, max_y) = (0.25 + OFFSET, 0.25 + OFFSET + stage_ratio);
            let (min_x, max_x, min_z, max_z);
            if matches!(side, Face::North | Face::South) {
                min_x = 0.25 + OFFSET;
                max_x = 0.75 - OFFSET;
                if matches!(side, Face::South) {
                    min_z = 0.75 - OFFSET;
                    max_z = 1.0;
                } else {
                    min_z = 0.0;
                    max_z = 0.25 + OFFSET;
                }
            } else {
                min_z = 0.25 + OFFSET;
                max_z = 0.75 - OFFSET;
                if matches!(side, Face::East) {
                    min_x = 0.75 - OFFSET;
                    max_x = 1.0;
                } else {
                    min_x = 0.0;
                    max_x = 0.25 + OFFSET;
                }
            }
            min = [min_x, min_y, min_z];
            max = [max_x, max_y, max_z];
        } else {
            let low = 0.5 - stage_ratio / 2.0;
            let high = 0.5 + stage_ratio / 2.0;
            let (min_y, max_y) = if matches!(side, Face::Down) {
                (0.0, 0.25 + OFFSET)
            } else {
                (0.25 + OFFSET + stage_ratio, 1.0)
            };
            min = [low, min_y, low];
            max = [high, max_y, high];
        }
        cuboids.push(Cuboid { min, max, sides });
    }
    let (low, high) = if frame.render_base {
        (0.25 + OFFSET, 0.75 - OFFSET)
    } else {
        (0.5 - stage_ratio / 2.0, 0.5 + stage_ratio / 2.0)
    };
    cuboids.push(Cuboid {
        min: [low, 0.25 + OFFSET, low],
        max: [high, 0.25 + OFFSET + stage_ratio, high],
        sides: frame.core_sides,
    });
    cuboids
}

/// Um quadrilátero do cubo: os quatro cantos e a fração de textura (u, v) de cada um, na ordem dos vértices de `drawFace` (`getU` e `getV` do sprite entram depois).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubeQuad {
    pub side: Face,
    pub corners: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
}

/// `RenderResizableCuboid.minBound`.
fn min_bound(min: f32, max: f32) -> f32 {
    let bigger = min > max;
    let wrapped = min % 1.0;
    if bigger && wrapped == 0.0 {
        1.0
    } else {
        wrapped
    }
}

/// `RenderResizableCuboid.maxBound`.
fn max_bound(min: f32, max: f32) -> f32 {
    let bigger = min > max;
    let wrapped = max % 1.0;
    if bigger {
        wrapped
    } else if wrapped == 0.0 {
        1.0
    } else {
        wrapped
    }
}

/// Os quadriláteros de um cubo dentro de um bloco (`renderCube` com os limites em 0 a 1, onde `calculateDelta` dá zero): o eixo X (oeste e leste), o Y (cima e baixo) e o Z
/// (norte e sul), cada um se algum lado dele sai. A face de trás (`FaceDisplay.FRONT` só tem a da frente) fica por conta do descarte de faces de costas do desenho.
pub fn cuboid_quads(cuboid: &Cuboid) -> Vec<CubeQuad> {
    let [x1, y1, z1] = cuboid.min;
    let [x2, y2, z2] = cuboid.max;
    let has = |side: Face| cuboid.sides & side_bit(side) != 0;
    let face = |verts: [[f32; 3]; 4], min_u: f32, max_u: f32, min_v: f32, max_v: f32| {
        (
            verts,
            [
                [min_u, max_v],
                [min_u, min_v],
                [max_u, min_v],
                [max_u, max_v],
            ],
        )
    };
    let mut quads = Vec::new();
    let mut push = |side: Face, made: ([[f32; 3]; 4], [[f32; 2]; 4])| {
        quads.push(CubeQuad {
            side,
            corners: made.0,
            uv: made.1,
        })
    };
    // X: o V vem do Y, o U do Z (`minBound(z2, z1)`).
    let (v_min, v_max) = (min_bound(y1, y2), max_bound(y1, y2));
    let (u_min, u_max) = (min_bound(z2, z1), max_bound(z2, z1));
    if has(Face::West) {
        push(
            Face::West,
            face(
                [[x1, y1, z2], [x1, y2, z2], [x1, y2, z1], [x1, y1, z1]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    if has(Face::East) {
        push(
            Face::East,
            face(
                [[x2, y1, z1], [x2, y2, z1], [x2, y2, z2], [x2, y1, z2]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    // Y: o V vem do Z (`minBound(z2, z1)`), o U do X (`minBound(x1, x2)`).
    let (v_min, v_max) = (min_bound(z2, z1), max_bound(z2, z1));
    let (u_min, u_max) = (min_bound(x1, x2), max_bound(x1, x2));
    if has(Face::Up) {
        push(
            Face::Up,
            face(
                [[x1, y2, z1], [x1, y2, z2], [x2, y2, z2], [x2, y2, z1]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    if has(Face::Down) {
        push(
            Face::Down,
            face(
                [[x1, y1, z2], [x1, y1, z1], [x2, y1, z1], [x2, y1, z2]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    // Z: o V vem do Y, o U do X (`minBound(x2, x1)`).
    let (v_min, v_max) = (min_bound(y1, y2), max_bound(y1, y2));
    let (u_min, u_max) = (min_bound(x2, x1), max_bound(x2, x1));
    if has(Face::North) {
        push(
            Face::North,
            face(
                [[x1, y1, z1], [x1, y2, z1], [x2, y2, z1], [x2, y1, z1]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    if has(Face::South) {
        push(
            Face::South,
            face(
                [[x2, y1, z2], [x2, y2, z2], [x1, y2, z2], [x1, y1, z2]],
                u_min,
                u_max,
                1.0 - v_max,
                1.0 - v_min,
            ),
        );
    }
    quads
}
