fn machinery_basis_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    // `RadiationManager.onTickPre` (o sorteio do decaimento) e o `tickServerWorld` de cada nível, que o original roda no `LevelTickEvent.Post`: depois das
    // entidades de todos os níveis, como aqui no fim do tick.
    match phase {
        horizon_client::mod_api::ServerTickPhase::ServerTickStart { world_running } => crate::game::machinery_radiation::server_tick_start(world_running),
        horizon_client::mod_api::ServerTickPhase::ServerTickEnd { .. } => crate::game::machinery_radiation::server_tick_end(),
        _ => {}
    }
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
