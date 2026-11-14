fn machinery_basis_transmitter_arrow(p: BlockPos) -> core::ops::ControlFlow<Vec<Aabb>> {
    match horizon_client::collisions::machinery_transmitter_collision::boxes_at([p.x, p.y, p.z]) {
        Some(boxes) => core::ops::ControlFlow::Break(boxes),
        None => core::ops::ControlFlow::Continue(()),
    }
}
