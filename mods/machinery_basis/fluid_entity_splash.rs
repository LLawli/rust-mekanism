fn machinery_basis_fluid_entity_splash() -> core::ops::ControlFlow<Result<()>> {
    // Um fluido de mod não é água (`isWaterLike` falso): sem respingo na entrada.
    if machinery_water_like::player_update_mod_only() {
        core::ops::ControlFlow::Break(Ok(()))
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
