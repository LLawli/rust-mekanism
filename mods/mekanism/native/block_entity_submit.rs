fn mekanism_submit(
    game: &crate::game::Game<'_>,
    pos: [i32; 3],
    draw: &mut crate::game::native_block_entity_draw::BlockEntityDraw,
) {
    crate::game::mekanism_cube::submit(game, pos, draw);
    crate::game::mekanism_configurator::diversion_submit(game, pos, draw);
    crate::game::machinery_transmitter_contents::submit(game, pos, draw);
}
