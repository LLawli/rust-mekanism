// Roda no começo de `head_item_geometry`, o `CustomHeadLayer`: o ponto por onde passam o jogador da janela em terceira pessoa, os outros jogadores, os mobs e o jogador da tela
// do inventário, depois do modelo e da armadura do vanilla. Desenha o equipamento vestido com modelo próprio (`worn_gear.rs`).
fn machinery_basis_worn_mob(game: &crate::game::Game<'_>, mob: &horizon_client::mobs::Mob, camera: glam::DVec3, alpha: f64, light: u32, out: &mut Vec<horizon_client::mesher::Vertex>) {
    crate::game::machinery_basis_gear::mob_gear(game, mob, camera, alpha, light, out);
}
