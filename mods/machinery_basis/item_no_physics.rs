fn machinery_basis_item_no_physics(no_physics: bool) {
    // `ItemEntity.tick` marca `noPhysics` antes de mover o item; o `environment_tick` que vem logo depois (os efeitos dos blocos: lava, água, cacto, fogo) precisa saber.
    horizon_client::entities::machinery_item_effects::set_no_physics(no_physics);
}
