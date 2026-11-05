fn machinery_basis_submit(
    game: &crate::game::Game<'_>,
    pos: [i32; 3],
    draw: &mut crate::game::native_block_entity_draw::BlockEntityDraw,
) {
    crate::game::machinery_basis::submit(game, pos, draw);
}
