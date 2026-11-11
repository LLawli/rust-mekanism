fn mekanism_screen_render_post(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    crate::game::mekanism_configurator::screen_render_post(game, batch);
}
