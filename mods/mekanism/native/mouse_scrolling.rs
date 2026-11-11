fn mekanism_mouse_scrolling(game: &mut crate::game::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    crate::game::mekanism_configurator::scroll(game, scroll, canceled);
}
