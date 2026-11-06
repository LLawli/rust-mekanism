fn machinery_basis_window_slot_at(game: &crate::game::Game<'_>, found: Option<usize>, x: f32, y: f32) -> Option<usize> {
    game.mb_window_slot_at(found, x, y)
}
