// Entra em `crates/horizon-client-render/src/mesher.rs` como `pub mod machinery_enhanced` (`visibility = "pub"`). O sombreamento de vértice (o AO e a luz suave) que o NeoForge dá aos quads de
// um modelo de bloco: o `EnhancedBlockModelLighter` (`enhancedLighting`, ligado por padrão), que troca o `BlockModelLighter` do vanilla. O mesher do Horizon segue o do vanilla (`lighting.rs`);
// para os blocos de mod, o RETURN de `lighting::ambient_occlusion` troca o resultado pelo daqui. O código é puro e só de `std`: o mesmo arquivo entra no teste de `machinery_basis`
// (`tests/enhanced_lighting_module.rs`), que o confere contra o NeoForge (`rastros/lighting_enhanced.jsonl`, `testes/CenaLuz.java`).
//
// O que muda em relação ao vanilla: (1) um quad de face inteira (`isAxisAligned`) usa as mesmas quatro quinas do vanilla com os consertos do NeoForge (o canto sem os dois lados claros usa o
// lado vizinho, a luz de dentro vem sempre do ponto de amostra, a mistura de luz ignora a de um bloco que não deixa a luz passar) e interpola pelas posições dos vértices, sem supor a ordem
// deles; (2) um quad que não é de face inteira (um elemento girado, como as abas da caixa de papelão) combina o AO das faces que a normal dele toca, pelo quadrado de cada componente
// da normal, e mistura 75 % disso com 25 % do máximo. O sombreamento direcional (`CardinalLighting`) já vem multiplicado no brilho.
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// As seis faces, na ordem de `Direction` do Horizon (`Down` 0 a `East` 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face {
    fn offset(self) -> [i32; 3] {
        match self {
            Face::Down => [0, -1, 0],
            Face::Up => [0, 1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::West => [-1, 0, 0],
            Face::East => [1, 0, 0],
        }
    }

    /// `BlockModelLighter.AdjacencyInfo.corners`: os quatro lados da face, na ordem do vanilla.
    fn sides(self) -> [Face; 4] {
        use Face::*;
        match self {
            Down => [West, East, North, South],
            Up => [East, West, North, South],
            North => [Up, Down, East, West],
            South => [West, East, Down, Up],
            West => [Up, Down, North, South],
            East => [Down, Up, North, South],
        }
    }
}

/// O que o mundo diz de uma célula em volta do bloco (`BlockModelLighter.Cache`): a luz (`LightCoordsUtil.getLightCoords`, bloco em 4..8 e céu em 20..24), o brilho de sombra do
/// estado (`getShadeBrightness`) e se ele deixa a luz passar (`isLightPermeable`).
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub light: u32,
    pub shade: f32,
    pub permeable: bool,
}

/// O quad: as posições dos quatro vértices (em blocos), a `direction` que o bake achou (a normal mais próxima) e o `shadeDirectionOverride`.
pub struct Quad<'a> {
    pub positions: &'a [[f32; 3]; 4],
    pub direction: Face,
    pub shade_override: Option<Face>,
}

/// A saída: o brilho de cada vértice (`ARGB.gray` do original, o canal de 0 a 255) e a luz suave (bloco em 0..8 e céu em 16..24).
pub type Shaded = ([i32; 4], [u32; 4]);

const AO_EPS: f32 = 1e-4;
const AVERAGE_WEIGHT: f32 = 0.75;
const MAX_WEIGHT: f32 = 1.0 - AVERAGE_WEIGHT;

/// `Math.clamp(float, 0, 1)`.
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// `Math.round(float)` do Java: o maior inteiro que não passa de `v + 1/2`, sem o arredondamento da soma em `f32`.
fn java_round(v: f32) -> i32 {
    let floor = v.floor();
    (if v - floor >= 0.5 { floor + 1.0 } else { floor }) as i32
}

/// `ARGB.gray` (o canal): `Mth.floor(brightness * 255)`.
pub fn gray(brightness: f32) -> i32 {
    let v = brightness * 255.0;
    let whole = v as i32;
    if whole as f32 > v {
        whole.saturating_sub(1)
    } else {
        whole
    }
}

fn smooth_block(packed: u32) -> i32 {
    (packed & 0xFF) as i32
}

fn smooth_sky(packed: u32) -> i32 {
    (packed >> 16 & 0xFF) as i32
}

fn smooth_pack(block: i32, sky: i32) -> u32 {
    (block & 0xFF) as u32 | ((sky & 0xFF) as u32) << 16
}

/// `LightCoordsUtil.smoothWeightedBlend`.
fn weighted_blend(c: [u32; 4], w: [f32; 4]) -> u32 {
    let sky = (smooth_sky(c[0]) as f32 * w[0]
        + smooth_sky(c[1]) as f32 * w[1]
        + smooth_sky(c[2]) as f32 * w[2]
        + smooth_sky(c[3]) as f32 * w[3]) as i32;
    let block = (smooth_block(c[0]) as f32 * w[0]
        + smooth_block(c[1]) as f32 * w[1]
        + smooth_block(c[2]) as f32 * w[2]
        + smooth_block(c[3]) as f32 * w[3]) as i32;
    smooth_pack(block, sky)
}

/// `EnhancedBlockModelLighter.lerpLightmap`.
fn lerp_lightmap(l1: u32, w1: f32, l2: u32, w2: f32) -> u32 {
    let block = 0xFF & java_round(smooth_block(l1) as f32 * w1 + smooth_block(l2) as f32 * w2);
    let sky = 0xFF & java_round(smooth_sky(l1) as f32 * w1 + smooth_sky(l2) as f32 * w2);
    smooth_pack(block, sky)
}

/// `EnhancedBlockModelLighter.maxLightmap`.
fn max_lightmap(l1: u32, l2: u32) -> u32 {
    smooth_pack(
        smooth_block(l1).max(smooth_block(l2)),
        smooth_sky(l1).max(smooth_sky(l2)),
    )
}

/// `AoFace.computeCornerWeights`: o peso de cada quina da face inteira para um ponto (as coordenadas ficam entre 0 e 1).
fn corner_weights(face: Face, x: f32, y: f32, z: f32) -> [f32; 4] {
    let (x, y, z) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0), z.clamp(0.0, 1.0));
    match face {
        Face::Down => [(1.0 - x) * z, (1.0 - x) * (1.0 - z), x * (1.0 - z), x * z],
        Face::Up => [x * z, x * (1.0 - z), (1.0 - x) * (1.0 - z), (1.0 - x) * z],
        Face::North => [y * (1.0 - x), y * x, (1.0 - y) * x, (1.0 - y) * (1.0 - x)],
        Face::South => [y * (1.0 - x), (1.0 - y) * (1.0 - x), (1.0 - y) * x, y * x],
        Face::West => [y * z, y * (1.0 - z), (1.0 - y) * (1.0 - z), (1.0 - y) * z],
        Face::East => [(1.0 - y) * z, (1.0 - y) * (1.0 - z), y * (1.0 - z), y * z],
    }
}

/// `AoFace.computeDepth`: 0 sobre a face, até 1 na face de trás.
fn depth(face: Face, x: f32, y: f32, z: f32) -> f32 {
    match face {
        Face::Down => y,
        Face::Up => 1.0 - y,
        Face::North => z,
        Face::South => 1.0 - z,
        Face::West => x,
        Face::East => 1.0 - x,
    }
}

/// O AO de uma face inteira (`AoCalculatedFace`): o brilho e a luz de cada quina.
struct FullFace {
    brightness: [f32; 4],
    light: [u32; 4],
}

/// `FullFaceCalculator.blend`, a mistura nova da luz de uma quina: a luz de cada posição que não deixa a luz passar sobe ao mínimo das que deixam.
fn blend(side_a: u32, side_b: u32, corner: u32, inside: u32, clear: [bool; 4]) -> u32 {
    let (a, b, c, i) = (side_a, side_b, corner, inside);
    let mut min_block = 0x10000;
    let mut min_sky = 0x10000;
    for (light, is_clear) in [a, b, c, i].into_iter().zip(clear) {
        if is_clear {
            min_block = min_block.min(smooth_block(light));
            min_sky = min_sky.min(smooth_sky(light));
        }
    }
    min_block &= 0xFFFF;
    min_sky &= 0xFFFF;
    let raise = |light: u32| {
        smooth_pack(
            min_block.max(smooth_block(light)),
            min_sky.max(smooth_sky(light)),
        )
    };
    let (a, b, c, i) = (raise(a), raise(b), raise(c), raise(i));
    (a.wrapping_add(b).wrapping_add(c).wrapping_add(i) >> 2) & 0x00FF_00FF
}

/// `FullFaceCalculator.calculateFaceUncached`. `cell` lê uma célula a partir da posição do bloco; `sample_outside` pede a luz de fora da face.
fn full_face(
    cell: &dyn Fn([i32; 3]) -> Cell,
    cardinal: &dyn Fn(Face) -> f32,
    direction: Face,
    shade_override: Option<Face>,
    sample_outside: bool,
) -> FullFace {
    let base = if sample_outside {
        direction.offset()
    } else {
        [0, 0, 0]
    };
    let at = |extra: &[Face]| {
        let mut p = base;
        for face in extra {
            let o = face.offset();
            p = [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
        }
        cell(p)
    };
    let sides = direction.sides();
    let side: [Cell; 4] = std::array::from_fn(|i| at(&[sides[i]]));
    // Os cantos: sem os dois lados claros, o canto é o lado vizinho (o do mesmo eixo que a quina pede, `1` para os dois últimos).
    let corner = |a: usize, b: usize, fallback: usize| -> Cell {
        if !side[b].permeable && !side[a].permeable {
            Cell {
                permeable: false,
                ..side[fallback]
            }
        } else {
            at(&[sides[a], sides[b]])
        }
    };
    let corner0 = corner(0, 2, 0);
    let corner1 = corner(0, 3, 0);
    let corner2 = corner(1, 2, 1);
    let corner3 = corner(1, 3, 1);
    let inside = at(&[]);
    let level = cardinal(shade_override.unwrap_or(direction));
    let shade =
        |a: Cell, b: Cell, c: Cell| ((a.shade + b.shade + c.shade + inside.shade) * 0.25) * level;
    let brightness = [
        shade(side[3], side[0], corner1),
        shade(side[2], side[0], corner0),
        shade(side[2], side[1], corner2),
        shade(side[3], side[1], corner3),
    ];
    let light = [
        blend(
            side[3].light,
            side[0].light,
            corner1.light,
            inside.light,
            [
                side[3].permeable,
                side[0].permeable,
                corner1.permeable,
                inside.permeable,
            ],
        ),
        blend(
            side[2].light,
            side[0].light,
            corner0.light,
            inside.light,
            [
                side[2].permeable,
                side[0].permeable,
                corner0.permeable,
                inside.permeable,
            ],
        ),
        blend(
            side[2].light,
            side[1].light,
            corner2.light,
            inside.light,
            [
                side[2].permeable,
                side[1].permeable,
                corner2.permeable,
                inside.permeable,
            ],
        ),
        blend(
            side[3].light,
            side[1].light,
            corner3.light,
            inside.light,
            [
                side[3].permeable,
                side[1].permeable,
                corner3.permeable,
                inside.permeable,
            ],
        ),
    ];
    FullFace { brightness, light }
}

/// `interpolateBrightness` e `interpolateLightmap`: a face inteira levada a um ponto pelos pesos das quinas.
fn interpolate(face: &FullFace, w: [f32; 4]) -> (f32, u32) {
    let b = &face.brightness;
    (
        clamp01(b[0] * w[0] + b[1] * w[1] + b[2] * w[2] + b[3] * w[3]),
        weighted_blend(face.light, w),
    )
}

/// `BakedNormals.computeQuadNormal`, empacotada em bytes (`pack`) e lida de volta (`unpackComponent`): a normal do quad com a precisão de 1/127 do original.
fn quad_normal(p: &[[f32; 3]; 4]) -> [f32; 3] {
    let d0 = [p[3][0] - p[1][0], p[3][1] - p[1][1], p[3][2] - p[1][2]];
    let d1 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let mut n = [
        d1[1] * d0[2] - d1[2] * d0[1],
        d1[2] * d0[0] - d1[0] * d0[2],
        d1[0] * d0[1] - d1[1] * d0[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 0.0 {
        n = n.map(|v| v / length);
    }
    // `(byte) Math.round(x * 127)` e `((byte) byte) / 127.0f`.
    n.map(|v| f32::from(java_round(v * 127.0) as i8) / 127.0)
}

/// `EnhancedBlockModelLighter.prepareQuadAmbientOcclusion`. `own_full` é `isCollisionShapeFullBlock` do estado do bloco; `cardinal` é `CardinalLighting.byFace`.
pub fn ambient_occlusion(
    cell: &dyn Fn([i32; 3]) -> Cell,
    own_full: bool,
    cardinal: &dyn Fn(Face) -> f32,
    quad: &Quad<'_>,
) -> Shaded {
    let mut min = [32.0f32; 3];
    let mut max = [-32.0f32; 3];
    for p in quad.positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let (low, high) = (1.0e-4f32, 0.9999f32);
    let (aligned, cubic) = match quad.direction {
        Face::Down => (
            min[1] == max[1],
            min[1] == max[1] && (min[1] < low || own_full),
        ),
        Face::Up => (
            min[1] == max[1],
            min[1] == max[1] && (max[1] > high || own_full),
        ),
        Face::North => (
            min[2] == max[2],
            min[2] == max[2] && (min[2] < low || own_full),
        ),
        Face::South => (
            min[2] == max[2],
            min[2] == max[2] && (max[2] > high || own_full),
        ),
        Face::West => (
            min[0] == max[0],
            min[0] == max[0] && (min[0] < low || own_full),
        ),
        Face::East => (
            min[0] == max[0],
            min[0] == max[0] && (max[0] > high || own_full),
        ),
    };
    let mut gray_out = [0i32; 4];
    let mut light_out = [0u32; 4];
    if aligned {
        let face = full_face(cell, cardinal, quad.direction, quad.shade_override, cubic);
        for (v, p) in quad.positions.iter().enumerate() {
            let (brightness, light) =
                interpolate(&face, corner_weights(quad.direction, p[0], p[1], p[2]));
            gray_out[v] = gray(brightness);
            light_out[v] = light;
        }
        return (gray_out, light_out);
    }
    let normal = quad_normal(quad.positions);
    for (v, p) in quad.positions.iter().enumerate() {
        let mut weighted_brightness = 0.0f32;
        let mut weighted_light = 0u32;
        let mut max_brightness = 0.0f32;
        let mut max_light = 0u32;
        for (axis, component) in normal.iter().enumerate() {
            if *component == 0.0 {
                continue;
            }
            let direction = match (axis, *component > 0.0) {
                (0, true) => Face::East,
                (0, false) => Face::West,
                (1, true) => Face::Up,
                (1, false) => Face::Down,
                (_, true) => Face::South,
                (_, false) => Face::North,
            };
            let sample_outside = depth(direction, p[0], p[1], p[2]) < AO_EPS || own_full;
            let face = full_face(
                cell,
                cardinal,
                direction,
                quad.shade_override,
                sample_outside,
            );
            let (brightness, light) =
                interpolate(&face, corner_weights(direction, p[0], p[1], p[2]));
            let axis_weight = component * component;
            weighted_brightness += brightness * axis_weight;
            weighted_light = lerp_lightmap(weighted_light, 1.0, light, axis_weight);
            max_brightness = max_brightness.max(brightness);
            max_light = max_lightmap(max_light, light);
        }
        gray_out[v] = gray(clamp01(
            weighted_brightness * AVERAGE_WEIGHT + max_brightness * MAX_WEIGHT,
        ));
        light_out[v] = lerp_lightmap(weighted_light, AVERAGE_WEIGHT, max_light, MAX_WEIGHT);
    }
    (gray_out, light_out)
}

/// O `brightness` que o mesher do Horizon (`brightness.map(|b| scale(gray(b), directional))`) transforma no canal `target`: o mesher multiplica o brilho que `ambient_occlusion` devolve pelo
/// sombreamento direcional da face (`directional`), e o NeoForge já o tem dentro de `target`. Devolve o canal `g` de menor valor tal que `floor(g * directional) == target` (o mais
/// próximo, se nenhum cabe), como brilho de ponto flutuante que `gray` lê de volta como `g`.
pub fn before_directional(target: i32, directional: f32) -> f32 {
    if directional <= 0.0 {
        return target as f32 / 255.0;
    }
    let scale = |g: i32| ((g as f32 * directional) as i32).clamp(0, 255);
    let mut best = target.clamp(0, 255);
    let mut best_error = (scale(best) - target).abs();
    let from = ((target as f32 / directional).floor() as i32 - 1).clamp(0, 255);
    for g in from..=(from + 4).min(255) {
        let error = (scale(g) - target).abs();
        if error < best_error || (error == best_error && error == 0 && g < best) {
            best = g;
            best_error = error;
        }
    }
    (best as f32 + 0.5) / 255.0
}

/// Os estados de bloco (por número) que levam o sombreamento do NeoForge: os de bloco de mod. O `Game` escreve a tabela a cada carga de dados e o RETURN de `ambient_occlusion` a lê.
static TABLE: RwLock<Option<Arc<[bool]>>> = RwLock::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static LOCAL: RefCell<(u64, Option<Arc<[bool]>>)> = const { RefCell::new((0, None)) };
}

/// Troca a tabela: a cada carga de dados.
pub fn set(table: Vec<bool>) {
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = Some(table.into());
    GENERATION.fetch_add(1, Ordering::Release);
}

/// Se o estado `state` leva o sombreamento do NeoForge. Cada thread de malha guarda a tabela que leu, e só a lê de novo quando ela troca.
pub fn uses(state: u32) -> bool {
    let generation = GENERATION.load(Ordering::Acquire);
    LOCAL.with(|local| {
        let mut local = local.borrow_mut();
        if local.0 != generation {
            *local = (
                generation,
                TABLE.read().unwrap_or_else(|e| e.into_inner()).clone(),
            );
        }
        local
            .1
            .as_ref()
            .is_some_and(|table| table.get(state as usize).copied().unwrap_or(false))
    })
}
