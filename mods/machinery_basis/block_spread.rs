fn machinery_basis_block_spread(level: &GameLevel<'_>, kind: &str, from: BlockPos, to: BlockPos, canceled: &mut bool) {
    crate::game::machinery_liquid::block_spread(level, kind, from, to, canceled);
}
