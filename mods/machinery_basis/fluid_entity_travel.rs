fn machinery_basis_fluid_entity_travel(world: &mut MotionWorld<'_, '_, '_>) -> core::ops::ControlFlow<Result<()>> {
    // `LivingEntity.travel` de um fluido de mod que não é água nem lava: `FluidType.move` devolve falso e nada move a entidade (sem
    // gravidade, sem atrito, sem `move`). Antes, a direção de nado do `Player.travel`.
    if world.player.flying {
        return core::ops::ControlFlow::Continue(());
    }
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
    if !contact.feet || contact.real {
        return core::ops::ControlFlow::Continue(());
    }
    if world.player.swimming && !world.player.is_passenger {
        let look_y = horizon_world::view_direction::view_vector(world.player.pitch, world.player.yaw).y;
        let near_surface = BlockPos::containing(Vec3::new(
            world.player.position.x,
            world.player.position.y + 1.0 - 0.1,
            world.player.position.z,
        ));
        if look_y <= 0.0
            || world.player.jumping
            || world.fluid_at(near_surface).kind() != horizon_world::fluid::FluidKind::Empty
        {
            let blend = if look_y < -0.2 { 0.085 } else { 0.06 };
            world.player.velocity.y += (look_y - world.player.velocity.y) * blend;
        }
    }
    core::ops::ControlFlow::Break(Ok(()))
}
