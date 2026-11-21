fn mekanism_item_core(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    state: ItemRenderState,
) -> ItemRenderState {
    let state = crate::game::mekanism_cube::item_core(game, stack, state);
    crate::game::mekanism_tank::item_contents(game, stack, state)
}
