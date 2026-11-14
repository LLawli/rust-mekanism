// O conteúdo animado dos transmissores no jogo (frente `cabo-conteudo`, Fase 4): `RenderTransmitterBase`, `RenderUniversalCable`, `RenderMechanicalPipe`, `RenderPressurizedTube` e
// `RenderThermodynamicConductor`, desenhados como geometria de block entity (`block_entity_submit`). A conta (a cor, o alfa, o brilho, a altura do líquido, os cubos do cano) é de
// `machinery_basis_core::transmitter_contents`; esta parte lê o cliente dos transmissores (`machinery_transmitter_client`), junta os grupos do modelo `transmitter_contents` que
// valem para a instância (`TransmitterContentsManager.getBaked`: um modelo avulso por grupo `<lado><TIPO>`, assado no começo) e troca o sprite por instância, como o original troca
// o slot `contents` da textura. O mod só diz os blocos, os modelos, os sprites do cabo e do condutor e o que é cada fluido e cada químico (`register`).
//
// Entra em `crates/horizon-client/src/game.rs` como `machinery_transmitter_contents`.
use crate::game::native_block_entity_draw::BlockEntityDraw;
use crate::game::Game;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmitter_contents as contents;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// `LightCoordsUtil.FULL_BRIGHT`, em 16 vezes o céu (bits 0 a 8) e o bloco (bits 8 a 16).
const FULL_BRIGHT: u32 = 0xF0F0;
/// O `Kd` do `.mtl` do conteúdo (`Kd 0.8 0.8 0.8`): o NeoForge multiplica a cor do quad por ele (`ObjGeometry.makeQuad`).
const CONTENTS_KD: f32 = 0.8;

/// O que cada família de transmissor mostra.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `RenderUniversalCable`: `mek_liquid/energy` com o alfa da escala.
    Energy,
    /// `RenderMechanicalPipe`: o fluido da rede, em cubos e nos braços `PUSH` e `PULL`.
    Fluid,
    /// `RenderPressurizedTube`: o ícone do químico com a cor dele.
    Chemical,
    /// `RenderThermodynamicConductor`: `mek_liquid/heat` com a cor da temperatura.
    Heat,
}

/// Um bloco transmissor e o que ele mostra.
#[derive(Clone, Debug)]
pub struct Family {
    pub block: String,
    pub kind: Kind,
    /// `ConductorTier.getBaseColor`, só no condutor.
    pub base_color: contents::Color,
}

/// O fluido de um cano como `MekanismRenderer` o lê: o sprite parado, a cor RGB que o multiplica, a luz (`FluidType.getLightLevel`) e se é gasoso e mais leve que o ar
/// (`MekanismUtils.lighterThanAirGas`).
#[derive(Clone, Debug)]
pub struct FluidLook {
    pub sprite: String,
    pub rgb: u32,
    pub light: u32,
    pub lighter_than_air: bool,
}

/// O químico de um tubo: o ícone (`Chemical.icon`), a cor RGB (`tint`) e a luz (`lightLevel`).
#[derive(Clone, Debug)]
pub struct ChemicalLook {
    pub sprite: String,
    pub rgb: u32,
    pub light: u32,
}

/// O que o mod entrega uma vez, no começo do jogo.
pub struct Provider {
    pub families: Vec<Family>,
    /// O sprite do cabo (`MekanismRenderer.ENERGY_ICON_LOCATION`) e o do condutor (`mek_liquid/heat`).
    pub energy_sprite: String,
    pub heat_sprite: String,
    /// O prefixo dos modelos avulsos (`<prefixo>/<lado><TIPO>`); o sprite do cabo é o que eles já têm.
    pub model_prefix: String,
    /// `tier.getHeatCapacity()` de um condutor: o cliente guarda o calor, e a temperatura é o calor sobre ela.
    pub heat_capacity: fn(&str) -> f64,
    pub fluid: fn(&str) -> Option<FluidLook>,
    pub chemical: fn(&str) -> Option<ChemicalLook>,
}

static PROVIDER: Mutex<Option<Provider>> = Mutex::new(None);
/// `MekanismConfig.client.opaqueTransmitters`.
static OPAQUE: AtomicBool = AtomicBool::new(false);
/// `HeatUtils.colorCache`: do processo.
static HEAT_COLORS: Mutex<Option<contents::HeatColorCache>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// O mod diz os transmissores dele (uma vez, no começo do jogo) e os modelos avulsos que o jogo tem de assar para eles.
pub(in crate::game) fn register(provider: Provider, models: &mut Vec<String>) {
    for group in contents::all_groups() {
        let name = format!("{}/{group}", provider.model_prefix);
        if !models.contains(&name) {
            models.push(name);
        }
    }
    *lock(&PROVIDER) = Some(provider);
}

/// `ModConfigEvent`: `opaqueTransmitters`.
pub(in crate::game) fn set_opaque(opaque: bool) {
    OPAQUE.store(opaque, Ordering::Relaxed);
}

/// O `Vertex.color` (RGBA8 com o alfa em cima) de uma cor ARGB do Java, com o RGB multiplicado por `factor`.
fn vertex_color(argb: u32, factor: f32) -> u32 {
    let channel = |shift: u32| ((((argb >> shift) & 0xFF) as f32) * factor + 0.5) as u32;
    (argb >> 24) << 24 | channel(0) << 16 | channel(8) << 8 | channel(16)
}

/// `LightCoordsUtil.lightCoordsWithEmission` sobre a luz do vértice (16 vezes o céu e o bloco).
fn light_with_emission(light: u32, emission: u32) -> u32 {
    let (sky, block) = contents::light_with_emission((light & 0xFF) / 16, (light >> 8 & 0xFF) / 16, emission);
    sky * 16 | block * 16 << 8
}

/// O retângulo do sprite no atlas (`u0`, `v0`, `u1`, `v1`): o nome como o modelo o cita, ou sem o `minecraft:` (o vanilla é `block/water_still`).
fn rect_of(game: &Game<'_>, name: &str) -> Option<[f32; 4]> {
    let sprites = &game.gui.atlas.sprites;
    sprites.get(name).or_else(|| sprites.get(name.strip_prefix("minecraft:")?)).map(|rect| [rect.u0, rect.v0, rect.u1, rect.v1])
}

/// Desenha os grupos do modelo de conteúdo (`submitBlockModel` do `RenderTransmitterBase.submit`) com `sprite` no lugar do sprite do modelo, a cor `argb` (o alfa vai nos vértices) e
/// a luz dada.
fn draw_groups(game: &Game<'_>, draw: &mut BlockEntityDraw, provider: &Provider, groups: &[String], sprite: &str, argb: u32, light: u32) {
    let (Some(from), Some(to)) = (rect_of(game, &provider.energy_sprite), rect_of(game, sprite)) else {
        return;
    };
    let kept = draw.light;
    draw.light = light;
    for group in groups {
        let start = draw.vertices_mut().len();
        if !game.native_draw_model(draw, &format!("{}/{group}", provider.model_prefix), 0xFF_FFFF) {
            continue;
        }
        for vertex in &mut draw.vertices_mut()[start..] {
            vertex.uv[0] = to[0] + (vertex.uv[0] - from[0]) / (from[2] - from[0]) * (to[2] - to[0]);
            vertex.uv[1] = to[1] + (vertex.uv[1] - from[1]) / (from[3] - from[1]) * (to[3] - to[1]);
            vertex.color = vertex_color(argb, CONTENTS_KD);
        }
    }
    draw.light = kept;
}

/// Os cubos do cano (`RenderMechanicalPipe.submit`, `RenderResizableCuboid.renderCube`): o fluido parado, com a altura do estágio.
fn draw_pipe_cubes(game: &Game<'_>, draw: &mut BlockEntityDraw, frame: &contents::PipeFrame, sprite: &str, argb: u32, light: u32) {
    let kept = draw.light;
    draw.light = light;
    for cuboid in contents::pipe_cuboids(frame) {
        for quad in contents::cuboid_quads(&cuboid) {
            let start = draw.vertices_mut().len();
            let [u0, v0] = [quad.uv[0][0], quad.uv[0][1]];
            let [u1, v1] = [quad.uv[2][0], quad.uv[2][1]];
            if game.native_draw_quad(draw, sprite, quad.corners, [u0 * 16.0, v0 * 16.0, u1 * 16.0, v1 * 16.0], 0xFF_FFFF) {
                for vertex in &mut draw.vertices_mut()[start..] {
                    vertex.color = vertex_color(argb, 1.0);
                    vertex.normal = horizon_client::mesher::CULL_BACK;
                }
            }
        }
    }
    draw.light = kept;
}

/// `BlockEntityRenderer.submit` do conteúdo de um transmissor em `pos` (`block_entity_submit`).
pub(in crate::game) fn submit(game: &Game<'_>, pos: [i32; 3], draw: &mut BlockEntityDraw) {
    if OPAQUE.load(Ordering::Relaxed) {
        return;
    }
    let provider = lock(&PROVIDER);
    let Some(provider) = provider.as_ref() else {
        return;
    };
    let block = game.native_block(pos);
    let Some(family) = provider.families.iter().find(|family| family.block == *block) else {
        return;
    };
    let dimension = game.native_dimension();
    let Some(view) = crate::game::machinery_transmitter_client::view_at(dimension, pos) else {
        return;
    };
    let network = crate::game::machinery_transmitter_client::network_at(dimension, pos);
    match family.kind {
        Kind::Energy => {
            let Some(network) = network.filter(|network| contents::cable_shows_contents(true, network.current_scale)) else {
                return;
            };
            let groups = contents::contents_groups(contents::all_sides(view.kind));
            draw_groups(game, draw, provider, &groups, &provider.energy_sprite, contents::energy_tint(network.current_scale), FULL_BRIGHT);
        }
        Kind::Heat => {
            let heat = crate::game::machinery_transmitter_client::heat_at(dimension, pos);
            let capacity = (provider.heat_capacity)(&family.block);
            // `BasicHeatCapacitor.getHeat`: sem o primeiro pacote o calor se inicializa com `capacidade * ar` (o ar do bioma na posição, `max(0, ...)`).
            let heat = heat.unwrap_or_else(|| {
                let ambient = crate::game::machinery_basis::ambient_temperature(game, pos).unwrap_or(machinery_basis_core::heat::AMBIENT_TEMP);
                (capacity * ambient).max(0.0)
            });
            let temperature = heat / capacity;
            let color = lock(&HEAT_COLORS).get_or_insert_with(Default::default).color_from_temp(temperature, family.base_color);
            let groups = contents::contents_groups(contents::all_sides(view.kind));
            draw_groups(game, draw, provider, &groups, &provider.heat_sprite, color.argb(), FULL_BRIGHT);
        }
        Kind::Chemical => {
            let Some(network) = network else {
                return;
            };
            let last = network.last_type.as_deref();
            if !contents::resource_shows_contents(true, last.is_some(), last.is_none(), network.current_scale) {
                return;
            }
            let Some(look) = last.and_then(provider.chemical) else {
                return;
            };
            let groups = contents::contents_groups(contents::all_sides(view.kind));
            let light = light_with_emission(draw.light, look.light);
            draw_groups(game, draw, provider, &groups, &look.sprite, contents::tube_tint(network.current_scale, look.rgb), light);
        }
        Kind::Fluid => {
            let Some(network) = network else {
                return;
            };
            let last = network.last_type.as_deref();
            if !contents::resource_shows_contents(true, last.is_some(), last.is_none(), network.current_scale) {
                return;
            }
            let Some(look) = last.and_then(provider.fluid) else {
                return;
            };
            let argb = contents::fluid_tint(look.rgb | 0xFF00_0000, look.lighter_than_air, network.current_scale);
            let light = light_with_emission(draw.light, look.light);
            let types: [ConnectionType; 6] = view.kind;
            let groups = contents::contents_groups(contents::pipe_contents_types(types));
            draw_groups(game, draw, provider, &groups, &look.sprite, argb, light);
            let frame = contents::pipe_frame(types, contents::pipe_stage(look.lighter_than_air, network.current_scale));
            draw_pipe_cubes(game, draw, &frame, &look.sprite, argb, light);
        }
    }
}
