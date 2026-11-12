fn mekanism_item_core(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    state: ItemRenderState,
) -> ItemRenderState {
    crate::game::mekanism_cube::item_core(game, stack, state)
}
