// A sobreposição colorida que o Configurator põe na face que o olhar toca (`ConfiguratorOverlayHandler`), no formato dos primitivos do mundo do Horizon: o quadrilátero do
// sprite do tipo de transmissão, com a cor do tipo de dado a 60 % de opacidade, camera-relativo e com teste de profundidade (`RenderTypes.eyes`: mistura translúcida, sem
// escrever a profundidade). Este arquivo é o módulo `mekanism_overlay`, enxertado em `game/world_primitives.rs` para alcançar os campos privados de `PrimitiveGeometry`.
use super::PrimitiveGeometry;
use crate::game::Game;
use horizon_client::mesher::Vertex;
use horizon_client::world_primitives::{WorldPrimitives, vertex_color};

/// Os primitivos do quadro quando a sobreposição está na tela. Outros mods que também desenham primitivos deixam o caminho do jogo valer (sem sobreposição naquele quadro).
pub(super) fn geometry(game: &mut Game<'_>, camera: glam::DVec3, alpha: f64) -> Option<PrimitiveGeometry> {
    let request = crate::game::mekanism_configurator::overlay_request(game)?;
    let corners = crate::game::mekanism_configurator::overlay_vertices(game, &request, camera.to_array())?;
    let mut primitives = WorldPrimitives::default();
    game.native_world_primitives(alpha as f32, camera.to_array(), &mut primitives);
    if !primitives.is_empty() {
        return None;
    }
    let rect = game.gui.atlas.sprites.get(request.sprite.as_str()).copied()?;
    let color = vertex_color(request.argb, 255);
    let mut out = PrimitiveGeometry::default();
    for (index, corner) in corners.iter().enumerate() {
        // `ConfiguratorOverlayHandler.render`: `u = vertex < 2 ? 0 : 1`, `v = vertex == 0 || vertex == 3 ? 0 : 1`.
        let u = if index < 2 { 0.0 } else { 1.0 };
        let v = if index == 0 || index == 3 { 0.0 } else { 1.0 };
        out.quads[0].push(Vertex {
            pos: *corner,
            uv: [rect.u0 + (rect.u1 - rect.u0) * u, rect.v0 + (rect.v1 - rect.v0) * v],
            layer: 0,
            color,
            light: 0,
            normal: [0; 4],
            glint_uv: [0.0, 0.0],
        });
    }
    Some(out)
}
