fn machinery_basis_fluid_entity_mob_travel(m: &Mob, w: &MobWorld) -> core::ops::ControlFlow<()> {
    // O mesmo que o `LivingEntity.travel` do jogador: num fluido de mod que não é água nem lava a entidade não se move.
    if m.kind.flags.flying || m.fluids.in_water || m.fluids.in_lava {
        return core::ops::ControlFlow::Continue(());
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
    if contact.feet && !contact.real {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
