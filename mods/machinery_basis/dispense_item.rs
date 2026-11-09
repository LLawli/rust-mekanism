fn machinery_basis_dispense_item(
    game: &mut crate::game::Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    facing: Direction,
    stack: &horizon_client::items::ItemStack,
    answer: &mut Option<horizon_client::mod_api::NativeDispensed>,
) {
    crate::game::machinery_liquid::dispense(game, dimension, pos, facing, stack, answer);
}
