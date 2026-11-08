fn mekanism_player_logged_in(game: &mut crate::game::Game<'_>, player: u128) {
    crate::game::mekanism_glue::player_logged_in(game, player);
}
