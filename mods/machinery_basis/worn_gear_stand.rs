// Roda no começo de `armor_stand_vertices`: o equipamento vestido com modelo próprio num armor stand, na pose dele (`worn_gear.rs`).
fn machinery_basis_worn_stand(game: &crate::game::Game<'_>, a: &horizon_client::entities::ArmorStand, base: glam::Mat4, packed: u32, out: &mut Vec<horizon_client::mesher::Vertex>) {
    crate::game::machinery_basis_gear::stand_gear(game, a, base, packed, out);
}
