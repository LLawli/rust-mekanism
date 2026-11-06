fn mekanism_mod_container_viewers(
    game: &crate::game::Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    viewers: u32,
    previous: u32,
) {
    crate::game::mekanism_glue::viewers_changed(game, dimension, pos, viewers, previous);
}
