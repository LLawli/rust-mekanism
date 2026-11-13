fn machinery_basis_transmitter_save(game: &mut crate::game::Game<'_>) {
    crate::game::machinery_basis::cables_save(game);
    crate::game::machinery_resources::pipes_save(game);
    crate::game::machinery_heat::conductors_save(game);
    crate::game::machinery_resources::tubes_save(game);
    crate::game::machinery_transporters::transporters_save(game);
}
