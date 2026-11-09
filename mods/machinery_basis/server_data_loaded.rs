fn machinery_basis_server_data_loaded(game: &mut crate::game::Game<'_>) {
    crate::game::machinery_basis::data_loaded();
    crate::game::machinery_liquid::index_states(game);
    game.mb_window_data_reloaded();
}
