fn machinery_basis_transmitter_arrow_clip(scene: &ArrowScene<'_, '_>, from: Vec3, to: Vec3, ignored: Option<BlockPos>) -> core::ops::ControlFlow<Option<(Vec3, BlockPos, [i32; 3])>> {
    if !horizon_client::collisions::machinery_transmitter_collision::any() {
        return core::ops::ControlFlow::Continue(());
    }
    // O mesmo corpo de `ArrowScene::clip`, com a forma guardada do transmissor no lugar da forma do estado.
    let mut world = SceneCollisions {
        world: scene.world,
        binding: scene.collision,
        subject: None,
    };
    let hit = horizon_core::geometry::traverse_blocks(from, to, 1024, |p: BlockPos| {
        if Some(p) == ignored {
            return None;
        }
        let state = scene.block(p);
        if scene.is_air(state) {
            return None;
        }
        let shape = match horizon_client::collisions::machinery_transmitter_collision::shape_at([p.x, p.y, p.z]) {
            Some(shape) => shape,
            None => scene.collision.collision_shape(&mut world, state, [p.x, p.y, p.z]).ok()?,
        };
        shape.clip(from, to, p)
    });
    let hit = match hit.ok().flatten() {
        Some(hit) => hit,
        None => return core::ops::ControlFlow::Break(None),
    };
    core::ops::ControlFlow::Break(Some((hit.location, hit.block, hit.direction.normal())))
}
