fn mekanism_fill_bucket(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, pos: [i32; 3], canceled: &mut bool) {
    crate::game::mekanism_glue::fill_bucket(game, stack, pos, canceled);
}
