fn machinery_basis_transmitter_collision(position: BlockPos) -> core::ops::ControlFlow<io::Result<VoxelShape>> {
    match machinery_transmitter_collision::shape_at([position.x, position.y, position.z]) {
        Some(shape) => core::ops::ControlFlow::Break(Ok(shape)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
