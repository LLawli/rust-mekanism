fn mekanism_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    if matches!(phase, horizon_client::mod_api::ServerTickPhase::ServerTickEnd { .. }) {
        crate::game::mekanism_glue::server_tick_end(game);
    }
}
