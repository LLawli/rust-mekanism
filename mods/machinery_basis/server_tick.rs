fn machinery_basis_server_tick(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    if let horizon_client::mod_api::ServerTickPhase::ServerTickStart { world_running } = phase {
        // O jogo refaz a conta de quem tem o menu aberto na fase das entidades, que um servidor parado não roda; o Java a refaz na hora em que o
        // menu fecha, parado ou não.
        if !world_running {
            game.mb_container_recount();
        }
        game.mb_container_broadcast();
    }
}
