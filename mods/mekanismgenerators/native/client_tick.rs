fn mekanismgenerators_client_tick(game: &mut crate::game::Game<'_>) {
    let _ = game;
    crate::game::mekanismgenerators_glue::client_tick();
}
