fn mekanism_container_opened(game: &mut crate::game::Game<'_>, player: u128, pos: Option<[i32; 3]>, cancel: &mut bool) {
    crate::game::mekanism_glue::container_opened(game, player, pos, cancel);
}
