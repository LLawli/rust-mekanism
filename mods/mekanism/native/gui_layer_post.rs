fn mekanism_gui_layer_post(game: &mut crate::game::Game<'_>, layer: &str, batch: &mut horizon_client::hud::HudBatch) {
    crate::game::mekanism_configurator::gui_layer_post(game, layer, batch);
}
