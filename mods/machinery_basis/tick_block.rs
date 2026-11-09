fn machinery_basis_tick_block(
    level: &mut GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    handled: &mut bool,
) {
    crate::game::machinery_liquid::tick_block(level, ticks, pos, state, handled);
}
