fn machinery_basis_state_for_placement(
    level: &GameLevel<'_>,
    state: BlockStateId,
    context: &PlacementContext,
    answer: &mut Option<Option<BlockStateId>>,
) {
    crate::game::machinery_basis::state_for_placement(level, state, context.pos, answer);
}
