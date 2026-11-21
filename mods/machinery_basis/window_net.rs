fn machinery_basis_window_net(game: &mut crate::game::Game<'_>, player: u128, channel: &str, data: &[u8]) {
    if !crate::game::machinery_basis::security_net_from_client(game, player, channel, data) {
        game.mb_window_net(player, channel, data);
    }
}
