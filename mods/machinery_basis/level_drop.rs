fn machinery_basis_level_drop(level: &mut GameLevel<'_>, pos: BlockPos, state: BlockStateId) {
    crate::game::machinery_basis::level_drop_note(level, pos, state);
}
