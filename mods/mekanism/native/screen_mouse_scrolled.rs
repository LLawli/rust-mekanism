fn mekanism_screen_mouse_scrolled(game: &crate::game::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    crate::game::mekanism_configurator::screen_mouse_scrolled(game, scroll, canceled);
}
