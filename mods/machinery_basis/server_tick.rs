fn machinery_basis_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    if matches!(phase, horizon_client::mod_api::ServerTickPhase::ServerTickStart { .. }) {
        game.mb_container_broadcast();
    }
}
