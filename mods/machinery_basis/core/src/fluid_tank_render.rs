//! O que se vê dentro de um tanque de fluido (`RenderFluidTank` e `RenderFluidTankItem`): a caixa do conteúdo, com a altura do estágio, a da válvula, a tinta, e o que o
//! servidor manda ao cliente para o desenho (`getReducedUpdateTag`: a escala, o fluido e o fluido da válvula). Sem ids: o fluido entra como texto e a cor como número.
use crate::sides::Face;
use crate::transmitter_contents::{ALL_FACES, Cuboid, fluid_tint, side_bit, stage_of};

/// `RenderFluidTank.stages`.
pub const STAGES: i32 = 1_400;
/// `RenderFluidTank.CONTENTS_MIN_XZ`, `CONTENTS_MAX_XZ` e `CONTENTS_MIN_Y`.
pub const CONTENTS_MIN_XZ: f32 = 0.135;
pub const CONTENTS_MAX_XZ: f32 = 0.865;
pub const CONTENTS_MIN_Y: f32 = 0.12375;
/// `RenderFluidTank.VALVE_MIN_XZ`, `VALVE_MAX_XZ` e `VALVE_MAX_Y`.
pub const VALVE_MIN_XZ: f32 = 0.3225;
pub const VALVE_MAX_XZ: f32 = 0.6775;
pub const VALVE_MAX_Y: f32 = 0.87625;
/// O quanto do bloco o conteúdo cheio sobe (`contentsMaxY`) e o quanto a válvula desce (`valveMinY`): os dois números do original, que não são iguais.
const CONTENTS_HEIGHT: f32 = 0.75225;
const VALVE_HEIGHT: f32 = 0.7525;
/// `CommonColors.WHITE`, a tinta de um tanque vazio.
pub const WHITE: u32 = 0xFFFF_FFFF;

/// `SideRender.NOT_DOWN` e `SideRender.HORIZONTAL`.
pub const NOT_DOWN: u8 = ALL_FACES ^ side_bit(Face::Down);
pub const HORIZONTAL: u8 = ALL_FACES ^ side_bit(Face::Down) ^ side_bit(Face::Up);

/// `RenderFluidTank.contentsMaxY`.
pub fn contents_max_y(fluid_scale: f32, gaseous: bool) -> f32 {
    let stage = stage_of(gaseous, STAGES, f64::from(fluid_scale));
    CONTENTS_MIN_Y + CONTENTS_HEIGHT * (stage as f32 / STAGES as f32)
}

/// `RenderFluidTank.valveMinY`: aqui a conta do estágio é em `float`, não em `double` como a de `getStage`.
pub fn valve_min_y(fluid_scale: f32) -> f32 {
    let stage = (STAGES - 1).min((fluid_scale * (STAGES - 1) as f32) as i32);
    CONTENTS_MIN_Y + VALVE_HEIGHT * (stage as f32 / STAGES as f32)
}

/// O que um quadro desenha dentro do tanque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TankFrame {
    /// A tinta das duas caixas (`state.fluidTint`).
    pub tint: u32,
    /// A caixa do conteúdo (`SideRender.NOT_DOWN`).
    pub contents: Option<Cuboid>,
    /// A caixa da válvula (`SideRender.HORIZONTAL`), do conteúdo até o teto.
    pub valve: Option<Cuboid>,
}

fn contents_cuboid(max_y: f32) -> Cuboid {
    Cuboid {
        min: [CONTENTS_MIN_XZ, CONTENTS_MIN_Y, CONTENTS_MIN_XZ],
        max: [CONTENTS_MAX_XZ, max_y, CONTENTS_MAX_XZ],
        sides: NOT_DOWN,
    }
}

/// `RenderFluidTank.extractRenderState` e os dois `if` do `submit`. `color` é a cor do fluido do tanque (`getColorARGB(FluidResource)`), `None` com o tanque vazio.
///
/// A tinta é lida antes de `state.fluidScale` ser escrito, e o estado nasce a cada quadro (`BlockEntityRenderDispatcher`: `createRenderState`): o fluido mais leve que o ar
/// leva sempre o alfa da escala 0 (0,2), qualquer que seja o nível. Fica como no original.
pub fn tile_frame(
    color: Option<u32>,
    gaseous: bool,
    prev_scale: f32,
    has_valve: bool,
) -> TankFrame {
    let tint = color.map_or(WHITE, |color| fluid_tint(color, gaseous, 0.0));
    let fluid_scale = if color.is_some() { prev_scale } else { 0.0 };
    let contents =
        (fluid_scale > 0.0).then(|| contents_cuboid(contents_max_y(fluid_scale, gaseous)));
    // `lighterThanAirGas` de um tanque vazio é falso: a válvula aparece mesmo sem fluido no tanque.
    let valve = (has_valve && !(gaseous && color.is_some())).then(|| Cuboid {
        min: [VALVE_MIN_XZ, valve_min_y(fluid_scale), VALVE_MIN_XZ],
        max: [VALVE_MAX_XZ, VALVE_MAX_Y, VALVE_MAX_XZ],
        sides: HORIZONTAL,
    });
    TankFrame {
        tint,
        contents,
        valve,
    }
}

/// `RenderFluidTankItem.extractArgument` e o `if` do `submit`: a caixa do conteúdo do item e a tinta, que aqui leva a escala de verdade. `None` sem nada a desenhar.
pub fn item_frame(color: u32, gaseous: bool, amount: i64, capacity: i64) -> Option<(u32, Cuboid)> {
    let fluid_scale = amount as f32 / capacity as f32;
    let max_y = if fluid_scale > 0.0 {
        contents_max_y(fluid_scale, gaseous)
    } else {
        0.0
    };
    (max_y > 0.0).then(|| {
        (
            fluid_tint(color, gaseous, fluid_scale),
            contents_cuboid(max_y),
        )
    })
}

/// O que o cliente sabe do tanque em uma posição: o `getReducedUpdateTag` do tile (`scale`, `fluid` e `valve`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TankView {
    pub scale: f32,
    pub fluid: Option<String>,
    pub valve: Option<String>,
}

fn put_text(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
}

fn take_text<'a>(bytes: &mut &'a [u8]) -> Option<&'a str> {
    let (length, rest) = bytes.split_at_checked(2)?;
    let (text, rest) =
        rest.split_at_checked(usize::from(u16::from_be_bytes([length[0], length[1]])))?;
    *bytes = rest;
    std::str::from_utf8(text).ok()
}

/// O pacote do desenho: a dimensão, a posição, a escala e os dois fluidos (texto vazio é nenhum).
pub fn encode_view(dimension: &str, pos: [i32; 3], view: &TankView) -> Vec<u8> {
    let mut bytes = Vec::new();
    put_text(&mut bytes, dimension);
    for value in pos {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&view.scale.to_be_bytes());
    put_text(&mut bytes, view.fluid.as_deref().unwrap_or(""));
    put_text(&mut bytes, view.valve.as_deref().unwrap_or(""));
    bytes
}

pub fn decode_view(mut bytes: &[u8]) -> Option<(String, [i32; 3], TankView)> {
    let dimension = take_text(&mut bytes)?.to_owned();
    let (numbers, mut rest) = bytes.split_at_checked(16)?;
    let word = |at: usize| {
        [
            numbers[at],
            numbers[at + 1],
            numbers[at + 2],
            numbers[at + 3],
        ]
    };
    let pos = [
        i32::from_be_bytes(word(0)),
        i32::from_be_bytes(word(4)),
        i32::from_be_bytes(word(8)),
    ];
    let scale = f32::from_be_bytes(word(12));
    let some = |text: &str| (!text.is_empty()).then(|| text.to_owned());
    let fluid = some(take_text(&mut rest)?);
    let valve = some(take_text(&mut rest)?);
    rest.is_empty().then_some((
        dimension,
        pos,
        TankView {
            scale,
            fluid,
            valve,
        },
    ))
}
