fn mekanism_energy(
    block: &str,
    energy: &mut Option<horizon_client::containers::ModEnergyBuffer>,
) {
    crate::game::mekanism_glue::energy(block, energy);
}
