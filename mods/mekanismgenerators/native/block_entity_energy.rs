fn mekanismgenerators_energy(
    block: &str,
    energy: &mut Option<horizon_client::containers::ModEnergyBuffer>,
) {
    crate::game::mekanismgenerators_glue::energy(block, energy);
}
