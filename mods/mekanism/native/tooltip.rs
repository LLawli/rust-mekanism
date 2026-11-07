fn mekanism_tooltip(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, lines: &mut Vec<(horizon_client::mod_api::Text, u32)>) {
    crate::game::mekanism_glue::tooltip(game, stack, lines);
}
