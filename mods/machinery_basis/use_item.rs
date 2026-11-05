fn machinery_basis_use_item(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::machinery_basis::use_item(game, stack, claimed);
}
