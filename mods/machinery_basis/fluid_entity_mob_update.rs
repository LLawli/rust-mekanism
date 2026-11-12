fn machinery_basis_fluid_entity_mob_update(m: &mut Mob, w: &MobWorld) {
    // O mob não passa pelo `FluidResources::fluid`: o fluido de mod entra aqui, antes do corpo do `update_fluids`, que só empurra pela água e pela lava da tabela de
    // fatos. Fora da água, a queda anda pelo `fallDistanceModifier`, a correnteza empurra com o `motionScale` (0,014), como a do jogador, e os olhos no fluido
    // tiram o ar (`fluid_entity_mob_air`).
    crate::motion::machinery_water_like::set_mob_eye(m.id, false);
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
    crate::motion::machinery_water_like::set_mob_eye(m.id, contact.eye);
    m.fall_distance *= 0.5;
    if m.pushed_by_fluid() {
        // `fluid_currents` lê a altura pela tabela de fatos, que não conhece o fluido de mod: a mesma conta com a altura do `water_like`.
        let base_y = bounds.min_position().y;
        let inner = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        let (ilo, ihi) = (inner.min_position(), inner.max_position());
        let mut scene = FlowScene(w);
        let mut water = CurrentAccumulator::default();
        for x in ilo.x.floor() as i32..=(ihi.x.ceil() as i32 - 1) {
            for y in ilo.y.floor() as i32..=(ihi.y.ceil() as i32 - 1) {
                for z in ilo.z.floor() as i32..=(ihi.z.ceil() as i32 - 1) {
                    let state = w.world.block(x, y, z);
                    let Some(level) = crate::motion::machinery_water_like::get(state.0) else {
                        continue;
                    };
                    let above = crate::motion::machinery_water_like::get(w.world.block(x, y + 1, z).0).is_some();
                    let surface = f64::from(y) + crate::motion::machinery_water_like::height(level, above);
                    if surface < ilo.y {
                        continue;
                    }
                    water.height = water.height.max(surface - base_y);
                    let mut flow = flow_at(&mut scene, w.fluid_tables.fluid(state), BlockPos::new(x, y, z)).unwrap_or(Vec3::ZERO);
                    if water.height < 0.4 {
                        flow = flow.scale(water.height);
                    }
                    water.accumulate(flow);
                }
            }
        }
        if water.height > 0.0 {
            let mut pushed = Pushed(&mut m.velocity);
            let _ = water.apply_to(&mut pushed, 0.014);
        }
    }
}
