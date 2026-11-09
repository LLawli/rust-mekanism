fn machinery_basis_block_changed(
    level: &GameLevel<'_>,
    dimension: LevelDimension,
    pos: BlockPos,
    new: BlockStateId,
) {
    crate::game::machinery_liquid::note(level, dimension_id(dimension), pos, new);
}
