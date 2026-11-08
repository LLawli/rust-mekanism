fn mekanism_server_data_loaded(game: &mut crate::game::Game<'_>, reload: bool) {
    crate::game::mekanism_glue::server_data_loaded(game, reload);
}
