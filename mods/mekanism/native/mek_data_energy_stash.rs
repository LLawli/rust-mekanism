// O primeiro HEAD de `Recipes::find_with` (ver `mek_data_energy_hook.rs`): guarda a grade se ela tem algum item com energia.
fn mekanism_mek_data_energy_stash(grid: &[Option<ItemStack>]) {
    mekanism_mek_data::stash(grid);
}
