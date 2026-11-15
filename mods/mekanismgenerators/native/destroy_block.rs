fn mekanismgenerators_destroy_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    crate::game::mekanismgenerators_glue::destroy_block(game, pos, canceled);
}
