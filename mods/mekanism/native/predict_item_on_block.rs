fn mekanism_predict_item_on_block(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, hit: &crate::game::Hit) -> core::ops::ControlFlow<bool> {
    crate::game::mekanism_configurator::predict_item_on_block(game, stack, hit)
}
