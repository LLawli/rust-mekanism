fn machinery_basis_server_data_loaded(game: &mut crate::game::Game<'_>) {
    crate::game::machinery_basis::data_loaded();
    crate::game::machinery_liquid::index_states(game);
    crate::game::machinery_basis::index_lighting_states(game);
    crate::game::machinery_transmitter_client::index_model_states(game);
    game.mb_window_data_reloaded();
}
