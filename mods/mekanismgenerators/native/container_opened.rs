fn mekanismgenerators_container_opened(game: &mut crate::game::Game<'_>, player: u128, pos: Option<[i32; 3]>) {
    crate::game::mekanismgenerators_glue::container_opened(game, player, pos);
}
