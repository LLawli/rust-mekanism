// HEAD cancelável de `light::relight_layer` (o `checkBlock` e o `runLightUpdates` de uma camada, para as posições que mudaram): na camada da luz de bloco, quando alguma posição
// emite luz por posição (`machinery_light`), a cópia de `machinery_light_engine` faz a conta com `getLightEmission(state, level, pos)`; sem isso, e na camada do céu, o corpo do
// jogo roda.
fn machinery_basis_position_relight_layer(world: &mut Region<'_>, tables: &LightTables, sky: bool, seeds: &[Pos], force: bool, changed: &mut LightChanges) -> core::ops::ControlFlow<()> {
    if machinery_light_engine::relight_layer(world, tables, sky, seeds, force, changed) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
