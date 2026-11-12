fn machinery_basis_fluid_state(state: BlockStateId, found: FluidState) -> FluidState {
    // Um bloco de líquido de mod move e afoga a entidade como a água (H47); o resto da água (fonte, fogo, hidratar) não vem junto.
    match machinery_water_like::get(state.0) {
        Some(level) if found == FluidState::EMPTY => match level {
            0 => FluidState::new(FluidKind::Water, 8, false),
            1..=7 => FluidState::new(FluidKind::FlowingWater, 8 - level, false),
            _ => FluidState::new(FluidKind::FlowingWater, 8, true),
        },
        // `IStateFluidLoggable.getFluid`: a água ou a lava dentro do bloco é a fonte do fluido do vanilla, com o `FluidType` dele (`machinery_water_like::logged`).
        _ => match machinery_water_like::logged(state.0) {
            Some(kind) if found == FluidState::EMPTY => FluidState::new(if kind == 1 { FluidKind::Water } else { FluidKind::Lava }, 8, false),
            _ => found,
        },
    }
}
