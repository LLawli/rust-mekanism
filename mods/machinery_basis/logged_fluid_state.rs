fn machinery_basis_logged_fluid_view(value: Option<&BlockState>, id: BlockStateId) -> Option<&BlockState> {
    machinery_logged_fluid::view(value, id.0)
}
