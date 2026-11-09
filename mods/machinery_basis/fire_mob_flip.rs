// Roda no fim de `Mob::update_fluids`, no começo do `base_tick` do mob: o mob em chamas dentro de um bloco de líquido de mod não está na água para o fogo
// (`base_tick` zera `fire_ticks` quando `fluids.in_water`), como o `canExtinguish` falso do `FluidType` (`water_like.rs`, `mob_flip`).
fn machinery_basis_fire_mob_flip(value: (), m: &mut crate::mobs::Mob, w: &crate::mobs::MobWorld<'_>) -> () {
    let block = |x: f64, y: f64, z: f64| w.world.block(x.floor() as i32, y.floor() as i32, z.floor() as i32).0;
    let feet = block(m.position.x, m.position.y, m.position.z);
    let eye = block(m.position.x, m.eye_y(), m.position.z);
    crate::motion::machinery_water_like::mob_flip(m.id, m.fire_ticks > 0, feet, eye, &mut m.fluids.in_water);
    value
}
