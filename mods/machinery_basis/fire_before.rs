fn machinery_basis_fire_before(world: &mut MotionWorld<'_, '_, '_>) {
    let feet = world.block(BlockPos::containing(world.player.position));
    let eye = world.block(BlockPos::containing(horizon_core::geometry::Vec3::new(
        world.player.position.x,
        world.player.position.y + f64::from(world.player.dimensions.eye_height),
        world.player.position.z,
    )));
    if machinery_water_like::get(feet.0).is_some() || machinery_water_like::get(eye.0).is_some() {
        world.player.fluids.was_touching_water = false;
    }
}
