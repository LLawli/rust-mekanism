fn machinery_basis_neighbor_changed(dimension: LevelDimension, pos: BlockPos, state: BlockStateId, level: &GameLevel<'_>) {
    crate::game::machinery_basis::neighbor_changed(level, dimension_id(dimension), pos, state);
}
