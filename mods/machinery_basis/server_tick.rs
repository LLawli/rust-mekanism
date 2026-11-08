fn machinery_basis_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    if matches!(phase, horizon_client::mod_api::ServerTickPhase::ServerTickStart { .. }) {
        // Quem fechou o menu sai das sessões (o Java o tira na hora em que o menu fecha), e depois o `broadcastChanges`.
        game.mb_container_recount();
        game.mb_container_broadcast();
    }
}
