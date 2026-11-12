fn machinery_basis_transmitter_tick(game: &mut crate::game::Game<'_>, pos: [i32; 3], entity: &mut horizon_client::containers::ModBlockEntity) {
    crate::game::machinery_basis::transmitters_update(game, pos, &entity.id);
}
