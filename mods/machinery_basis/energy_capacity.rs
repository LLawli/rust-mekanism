fn machinery_basis_energy_capacity(
    game: &crate::game::Game<'_>,
    pos: [i32; 3],
) -> core::ops::ControlFlow<Option<(Arc<ModEnergyBuffer>, i32)>> {
    crate::game::machinery_basis::energy_capacity(game, pos)
}
