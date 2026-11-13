fn mekanism_perform_command(game: &mut crate::game::Game<'_>, player: &horizon_client::mod_api::NativeServerPlayer, command: &str, ran: &mut bool) {
    crate::game::mekanism_commands::perform_command(game, player, command, ran);
}
