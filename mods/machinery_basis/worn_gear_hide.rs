// Roda no começo de `anim::setup`, que monta a pose das partes de todo mob e jogador: esconde as partes que o que ele veste esconde (`worn_hides.rs`).
fn machinery_basis_worn_hide(m: &crate::mobs::Mob, p: &mut crate::mobs::anim::Poses) {
    crate::mobs::anim::machinery_basis_worn::begin(m, p);
}
