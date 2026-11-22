fn machinery_basis_transmitter_reset(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::WorldPhase) {
    if phase == horizon_client::mod_api::WorldPhase::WorldCloseFinal {
        // Os níveis soltam os chunks (`onChunkUnloaded` e `setRemoved` de cada transmissor) antes de o registro zerar: o original os entrega antes do `ServerStoppedEvent`.
        crate::game::machinery_basis::world_closing(game);
        crate::game::machinery_basis::transmitters_reset(game);
        // `FrequencyControllerManager.reset` e `MekanismClient.reset`.
        crate::game::machinery_basis::security_world_closed();
        // O `RadiationManager` e o `PlayerExposure.clear` do `ServerStoppedEvent`.
        crate::game::machinery_radiation::reset();
    } else if phase == horizon_client::mod_api::WorldPhase::WorldOpenReady {
        crate::game::machinery_basis::transmitter_probe_note("open_ready");
        game.mb_open_visited_levels();
        // A tabela de segurança do mundo que abriu é lida no primeiro pedido; o anfitrião recebe o lote que um convidado recebe ao entrar (`SyncAllSecurityData`).
        crate::game::machinery_basis::security_world_opened(game);
        // Os níveis do mundo leem as attachments deles (as fontes de radiação).
        crate::game::machinery_radiation::world_opened(game);
    }
}
