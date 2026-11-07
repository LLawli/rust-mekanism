fn mekanism_client_tick(game: &mut crate::game::Game<'_>) {
    crate::game::mekanism_glue::client_tick(game);
}
