fn machinery_basis_fluid_entity_fall(world: &mut MotionWorld<'_, '_, '_>) -> core::ops::ControlFlow<Result<()>> {
    // Fora da água, o `Entity.baseTick` anda a queda pelo `fallDistanceModifier` do fluido (0,5) em vez de zerá-la.
    if machinery_water_like::player_update_mod_only() {
        world.player.fall_distance *= 0.5;
        core::ops::ControlFlow::Break(Ok(()))
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
