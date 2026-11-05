fn mekanismgenerators_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::mekanismgenerators_glue::tick(game, pos, entity);
}
