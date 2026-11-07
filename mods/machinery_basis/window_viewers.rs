fn machinery_basis_window_viewers(
    game: &crate::game::Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    viewers: u32,
) {
    crate::game::machinery_basis::viewers_changed(game, dimension, pos, viewers);
}
