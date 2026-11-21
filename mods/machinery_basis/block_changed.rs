fn machinery_basis_block_changed(
    level: &GameLevel<'_>,
    dimension: LevelDimension,
    pos: BlockPos,
    old: BlockStateId,
    new: BlockStateId,
) {
    crate::game::machinery_liquid::note(level, dimension_id(dimension), pos, new);
    crate::game::machinery_basis::transmitters_block_changed(level, pos, old, new);
    crate::game::machinery_basis::bounding_block_changed(level, dimension_id(dimension), pos, old, new);
}
