fn mekanism_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::mekanism_glue::notify_comparators(game, dimension);
}
