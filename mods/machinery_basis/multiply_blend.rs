// Entra em `crates/horizon-client-render/src/hud.rs` como `pub mod machinery_multiply` (`visibility = "pub"`). O blend `DST_COLOR` do Mekanism
// (`MekanismRenderPipelines.GUI_DST_COLOR` e `GUI_TEXTURED_DST_COLOR`, `BlendFunction(DST_COLOR, ZERO)`): o `HudBatch` só tem os blends do enum `Blend`, que não tem
// esse. Quem desenha marca os vértices com `mark_since` (uma corrida de `Blend::CitGlint(255)`, um índice que nenhum pacote de CIT usa), e o HEAD de `draw_range`
// desenha essas corridas aqui, com um pipeline próprio do mesmo shader (`fs_main`) e do mesmo grupo, na ordem em que o lote as tem.
use super::{Blend, HudBatch, HudRenderer, HudVertex};
use std::ops::Range;
use std::sync::Mutex;

/// A corrida que o HEAD de `draw_range` desenha com o blend `(DST_COLOR, ZERO)`.
pub const MARK: Blend = Blend::CitGlint(u8::MAX);

/// Marca como multiplicados os vértices que o lote ganhou depois de `start` (`HudBatch::vertices.len()` antes do desenho).
pub fn mark_since(batch: &mut HudBatch, start: usize) {
    batch.blend_since(start, MARK);
}

/// `vs_main` e o caminho de sprite de `fs_main` de `hud.wgsl`, sem o resto (o módulo não pode incluir o arquivo: `E_MODULE`). Os bindings são os 0 a 2 do grupo do
/// `HudRenderer` (o uniforme de tamanho, o atlas e o amostrador), que o `layout` dele tem entre os seis.
const SHADER: &str = r#"
struct Hud { size: vec4<f32> }
@group(0) @binding(0) var<uniform> hud: Hud;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@vertex fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) depth: f32, @location(4) glint_uv: vec2<f32>) -> Out {
    let pixels = pos * hud.size.z;
    var out: Out;
    out.clip = vec4(pixels.x / hud.size.x * 2.0 - 1.0, 1.0 - pixels.y / hud.size.y * 2.0, depth, 1.0);
    out.uv = uv;
    out.color = color;
    return out;
}
@fragment fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let c = textureSample(atlas, atlas_sampler, in.uv) * in.color;
    if c.a < 0.004 { discard; }
    return c;
}
"#;

/// O pipeline e o `HudRenderer` (o endereço dele) para o qual foi feito.
static PIPELINE: Mutex<Option<(usize, wgpu::RenderPipeline)>> = Mutex::new(None);

fn owner(hud: &HudRenderer) -> usize {
    hud as *const HudRenderer as usize
}

/// Há uma corrida marcada em `runs`.
fn has_mark(runs: &[(Range<u32>, Blend)]) -> bool {
    runs.iter().any(|(_, blend)| *blend == MARK)
}

/// Um passo do desenho: o que o `draw_range` do Horizon desenha como está, ou uma corrida multiplicada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Plain(Range<u32>),
    Multiply(Range<u32>),
}

/// Parte as corridas em passos na ordem do lote: as corridas sem marca, vizinhas, viram um passo só, e cada marca é um passo.
pub fn steps(runs: &[(Range<u32>, Blend)]) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    for (range, blend) in runs {
        if *blend == MARK {
            out.push(Step::Multiply(range.clone()));
        } else if let Some(Step::Plain(last)) = out.last_mut() {
            last.end = range.end;
        } else {
            out.push(Step::Plain(range.clone()));
        }
    }
    out
}

/// Roda no começo de `prepare`: com uma marca no lote, cria o pipeline (uma vez por `HudRenderer`).
pub fn prepare(hud: &HudRenderer, device: &wgpu::Device, batch: &HudBatch) {
    if !has_mark(&batch.runs()) {
        return;
    }
    let Ok(mut slot) = PIPELINE.lock() else { return };
    if slot.as_ref().is_some_and(|(at, _)| *at == owner(hud)) {
        return;
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Horizon HUD multiply"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("HUD multiply layout"),
        bind_group_layouts: &[Some(&hud.layout)],
        ..Default::default()
    });
    let attributes = wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Unorm8x4,3=>Float32,4=>Float32x2];
    // `BlendFunction(DST_COLOR, ZERO)`: a cor e o alfa de origem multiplicam os do destino.
    let multiply = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Dst,
        dst_factor: wgpu::BlendFactor::Zero,
        operation: wgpu::BlendOperation::Add,
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("HUD multiply"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<HudVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: hud.format,
                blend: Some(wgpu::BlendState { color: multiply, alpha: multiply }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    *slot = Some((owner(hud), pipeline));
}

/// O HEAD de `draw_range`: com uma corrida marcada em `vertices`, desenha os passos em ordem (os sem marca pelo próprio `draw_range`, que não vê marca neles, e cada
/// marca num passo com o blend novo) e devolve `true` (cancela o desenho original). Sem marca devolve `false`.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    hud: &HudRenderer,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    batch: &HudBatch,
    size: [u32; 2],
    vertices: Range<u32>,
    mut timing: Option<&mut crate::gfx::pack_timing::PassTiming>,
) -> bool {
    let runs = batch.runs_in(vertices);
    if !has_mark(&runs) {
        return false;
    }
    let Ok(slot) = PIPELINE.lock() else { return false };
    let Some((at, pipeline)) = slot.as_ref() else { return false };
    if *at != owner(hud) {
        return false;
    }
    for step in steps(&runs) {
        match step {
            Step::Plain(part) => hud.draw_range(encoder, target, depth, batch, size, part, timing.as_deref_mut()),
            Step::Multiply(part) => {
                let attachments = [Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("HUD multiply"),
                    color_attachments: &attachments,
                    ..Default::default()
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &hud.group, &[]);
                pass.set_vertex_buffer(0, hud.buffer.slice(..));
                pass.draw(part, 0..1);
            }
        }
    }
    true
}
