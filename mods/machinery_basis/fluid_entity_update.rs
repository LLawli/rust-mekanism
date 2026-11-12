fn machinery_basis_fluid_entity_update(world: &MotionWorld<'_, '_, '_>) {
    let bounds = world.player.bounds();
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    let eye = world.player.position.y + f64::from(world.player.dimensions.eye_height);
    let contact = machinery_water_like::contact(
        [lo.x, lo.y, lo.z],
        [hi.x, hi.y, hi.z],
        eye,
        &|x, y, z| machinery_water_like::get(world.block(BlockPos::new(x, y, z)).0),
        &|x, y, z| {
            world
                .resources
                .fluids
                .fluid(world.block(BlockPos::new(x, y, z)))
                .kind()
                != horizon_world::fluid::FluidKind::Empty
        },
    );
    machinery_water_like::set_player_update(contact.mod_only());
}
