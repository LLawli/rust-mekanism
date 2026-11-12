fn mekanism_right_click_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], face: [i32; 3], canceled: &mut bool) {
    crate::game::mekanism_configurator::right_click_block(game, pos, face, canceled);
    crate::game::mekanism_configurator::open_cube_menu(game, pos, canceled);
}
