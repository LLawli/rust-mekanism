fn machinery_basis_logged_fluid_begin(facts: &BlockFacts) {
    horizon_world::registry::machinery_logged_fluid::begin(facts.states.len());
}
