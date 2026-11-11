fn mekanism_context_outline(game: &crate::game::Game<'_>, pos: [i32; 3], state: horizon_world::registry::BlockStateId) -> core::ops::ControlFlow<Option<[f64; 6]>> {
    crate::game::mekanism_configurator::partial_outline(game, pos, state)
}
