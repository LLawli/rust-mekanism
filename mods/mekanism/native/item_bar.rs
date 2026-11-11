fn mekanism_item_bar(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, bar: &mut Option<(i32, u32)>) {
    crate::game::mekanism_glue::item_bar(game, stack, bar);
}
