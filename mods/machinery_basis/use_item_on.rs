fn machinery_basis_use_item_on(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    crate::game::machinery_basis::use_item_on(game, stack, pos, claimed);
}
