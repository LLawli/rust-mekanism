fn machinery_basis_transmitter_reset(game: &mut crate::game::Game<'_>, phase: horizon_client::mod_api::WorldPhase) {
    if phase == horizon_client::mod_api::WorldPhase::WorldCloseFinal {
        crate::game::machinery_basis::transmitters_reset(game);
    } else if phase == horizon_client::mod_api::WorldPhase::WorldOpenReady {
        game.mb_open_visited_levels();
    }
}
