// Roda no começo de `player_cape_geometry`: a capa some com o equipamento vestido que a esconde (`showCape = false`, em `worn_gear.rs`).
fn machinery_basis_worn_cape(m: &horizon_client::mobs::Mob) -> core::ops::ControlFlow<()> {
    crate::game::machinery_basis_gear::cape(m)
}
