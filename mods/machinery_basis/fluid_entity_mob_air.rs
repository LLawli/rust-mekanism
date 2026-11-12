fn machinery_basis_fluid_entity_mob_air(m: &Mob, w: &MobWorld, value: bool) -> bool {
    // `CommonHooks.onLivingBreathe`: com os olhos num fluido que não é ar e que a entidade se afoga (`canDrown`), o ar acaba. O fluido de mod
    // não passa pela tabela de fatos, de onde vem o `eye_in_water`.
    if value {
        return true;
    }
    let bounds = m.bounds();
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    crate::motion::machinery_water_like::contact(
        [lo.x, lo.y, lo.z],
        [hi.x, hi.y, hi.z],
        m.eye_y(),
        &|x, y, z| crate::motion::machinery_water_like::get(w.world.block(x, y, z).0),
        &|_, _, _| false,
    )
    .eye
}
