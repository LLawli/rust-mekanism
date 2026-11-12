fn machinery_basis_logged_fluid_state(
    state: &BlockState,
    value: std::io::Result<FluidState>,
) -> std::io::Result<FluidState> {
    match state.properties.get("fluid_logged").map(String::as_str) {
        Some("water") => Ok(FluidState::new(FluidKind::Water, 8, false)),
        Some("lava") => Ok(FluidState::new(FluidKind::Lava, 8, false)),
        _ => value,
    }
}
