fn mekanism_place_block(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, pos: [i32; 3]) {
    crate::game::mekanism_glue::place_block(game, stack, pos);
}
