fn mekanism_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    let _span = crate::game::machinery_basis::perf_span(crate::game::machinery_basis::PERF_HOOK_MEK);
    crate::game::mekanism_glue::tick(game, pos, entity);
}
