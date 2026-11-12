// Anota o estado padrão dos três blocos de vidro (`glass_skip_table.rs`) quando a tabela de visibilidade das faces é montada: o registro de blocos só existe aqui, e `FaceVisibility::skips`,
// chamada por face no mesher, só vê o estado padrão de cada bloco (`State::block`). É um HEAD: o RETURN não aceita o `?` do corpo de `new`.
fn mekanismgenerators_glass_ids(occlusion: &CachedCollisionRegistry<'_>) {
    let registry = occlusion.registry();
    let id = |name: &str| registry.default_state(name).map_or(crate::block_render::mekanismgenerators_glass::MISSING, |state| state.0);
    crate::block_render::mekanismgenerators_glass::record(id("mekanism:structural_glass"), id("mekanismgenerators:reactor_glass"), id("mekanismgenerators:laser_focus_matrix"));
}
