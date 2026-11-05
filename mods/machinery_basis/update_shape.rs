fn machinery_basis_update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    direction: Direction,
    neighbour: BlockStateId,
    new: &mut Option<BlockStateId>,
) {
    crate::game::machinery_basis::update_shape(level, ticks, pos, state, direction, neighbour, new);
}
