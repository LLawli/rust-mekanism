fn mekanism_energy_store(game: &crate::game::Game<'_>, pos: [i32; 3], energy: i32) {
    crate::game::mekanism_glue::energy_stored(game, pos, energy);
}
