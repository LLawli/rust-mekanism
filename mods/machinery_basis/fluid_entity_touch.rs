fn machinery_basis_fluid_entity_touch(world: &mut MotionWorld<'_, '_, '_>) -> core::ops::ControlFlow<()> {
    // `wasTouchingWater` é `isWaterLike`: a água emprestada ao bloco de mod não conta (fogo, respingo, nado).
    if machinery_water_like::player_update_mod_only() {
        world.player.fluids.was_touching_water = false;
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
