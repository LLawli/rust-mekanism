fn machinery_basis_window_net_server(game: &crate::game::Game<'_>, channel: &str, data: &[u8]) {
    game.mb_window_net_server(channel, data);
    crate::game::machinery_basis::security_net_from_server(channel, data);
}
