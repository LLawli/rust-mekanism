// Roda no começo de `block_contact_damage`, que `base_tick` chama depois de contar o fogo do mob: devolve a água que `fire_mob_flip` tirou dele (`water_like.rs`, `mob_restore`).
fn machinery_basis_fire_mob_restore(m: &mut crate::mobs::Mob) {
    crate::motion::machinery_water_like::mob_restore(m.id, &mut m.fluids.in_water);
}
