fn machinery_basis_fluid_state(state: BlockStateId, found: FluidState) -> FluidState {
    // Um bloco de líquido de mod move e afoga a entidade como a água (H47); o resto da água (fonte, fogo, hidratar) não vem junto.
    match machinery_water_like::get(state.0) {
        Some(level) if found == FluidState::EMPTY => match level {
            0 => FluidState::new(FluidKind::Water, 8, false),
            1..=7 => FluidState::new(FluidKind::FlowingWater, 8 - level, false),
            _ => FluidState::new(FluidKind::FlowingWater, 8, true),
        },
        _ => found,
    }
}
