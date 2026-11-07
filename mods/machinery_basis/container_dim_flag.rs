fn machinery_basis_container_dim_flag(game: &crate::game::Game<'_>) {
    horizon_client::hud::machinery_container_dim::set(game.mb_screen_owns_dim());
}
