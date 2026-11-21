fn machinery_basis_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    crate::game::machinery_basis::transmitters_tick(game, phase);
    if matches!(phase, horizon_client::mod_api::ServerTickPhase::ServerTickStart { .. }) {
        // Quem fechou o menu sai das sessões (o Java o tira na hora em que o menu fecha), e depois o `broadcastChanges`.
        // Os apoios de um bloco principal que saiu sem ninguém o quebrar (`removeBoundingBlocks`).
        crate::game::machinery_basis::bounding_flush(game);
        // Quem perdeu o acesso ao tile que tem aberto sai (`validateAccess` da Security Desk e `securityChanged` da aba), antes da recontagem.
        game.mb_security_revalidate();
        game.mb_container_recount();
        game.mb_container_broadcast();
    }
}
