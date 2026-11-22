// HEAD cancelável de `light::light_chunk` (um trabalho de luz de um chunk inteiro, nas threads do `streaming.rs`): quando alguma posição do 3x3 emite luz por posição
// (`machinery_light`), a cópia de `machinery_light_engine` faz a conta com `getLightEmission(state, level, pos)`; sem isso o corpo do jogo roda.
fn machinery_basis_position_light_chunk(region: &[[Option<&Chunk>; 3]; 3], tables: &LightTables, sky_light: bool) -> core::ops::ControlFlow<Vec<Light>> {
    match machinery_light_engine::light_chunk(region, tables, sky_light) {
        Some(lights) => core::ops::ControlFlow::Break(lights),
        None => core::ops::ControlFlow::Continue(()),
    }
}
