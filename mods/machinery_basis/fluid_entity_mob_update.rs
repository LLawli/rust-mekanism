fn machinery_basis_fluid_entity_mob_update(m: &mut Mob, w: &MobWorld) {
    // O mob não passa pelo `FluidResources::fluid`: o fluido de mod entra aqui. Fora da água, a queda anda pelo `fallDistanceModifier` e a
    // correnteza empurra com o `motionScale` (0,014), como a do jogador.
    if m.fluids.in_water || m.fluids.in_lava {
        return;
    }
    let bounds = m.bounds();
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    let contact = crate::motion::machinery_water_like::contact(
        [lo.x, lo.y, lo.z],
        [hi.x, hi.y, hi.z],
        m.eye_y(),
        &|x, y, z| crate::motion::machinery_water_like::get(w.world.block(x, y, z).0),
        &|x, y, z| w.facts.get(w.world.block(x, y, z)).fluid != crate::blocks::Fluid::None,
    );
    if !contact.mod_only() {
        return;
    }
    m.fall_distance *= 0.5;
    if m.pushed_by_fluid() {
        let (water, _) = w.fluid_currents(m.bounds());
        if water.height > 0.0 {
            let mut pushed = Pushed(&mut m.velocity);
            let _ = water.apply_to(&mut pushed, 0.014);
        }
    }
}
