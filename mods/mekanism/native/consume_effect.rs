fn mekanism_consume_effect(game: &mut crate::game::Game<'_>, kind: &str) {
    crate::game::mekanism_glue::paste_consumed(game, kind);
}
