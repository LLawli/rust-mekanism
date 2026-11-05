fn machinery_basis_update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
) {
    crate::game::machinery_basis::update_shape(level, ticks, pos, state);
}
