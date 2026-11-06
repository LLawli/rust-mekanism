fn machinery_basis_window_net(game: &mut crate::game::Game<'_>, player: u128, channel: &str, data: &[u8]) {
    game.mb_window_net(player, channel, data);
}
