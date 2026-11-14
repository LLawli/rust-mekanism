fn machinery_basis_transmitter_landing(p: BlockPos) -> core::ops::ControlFlow<Result<VoxelShape>> {
    match horizon_client::collisions::machinery_transmitter_collision::shape_at([p.x, p.y, p.z]) {
        Some(shape) => core::ops::ControlFlow::Break(Ok(shape)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
