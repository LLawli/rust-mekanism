// Roda no começo de `first_person_arm`: com o equipamento vestido que esconde o braço, o braço do vanilla não se desenha e o equipamento desenha o dele (`worn_gear.rs`).
fn machinery_basis_worn_arm(game: &crate::game::Game<'_>, left: bool, m: glam::Mat4, light: u32, out: &mut Vec<horizon_client::mesher::Vertex>) -> core::ops::ControlFlow<()> {
    crate::game::machinery_basis_gear::first_person_arm(game, left, m, light, out)
}
