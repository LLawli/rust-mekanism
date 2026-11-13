fn machinery_basis_window_player_in(game: &mut crate::game::Game<'_>, player: u128) {
    game.mb_window_player_in(player);
    crate::game::machinery_transmitter_client::player_in(game, player);
}
