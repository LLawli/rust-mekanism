fn mekanismgenerators_loot_drops(game: &crate::game::Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut Vec<horizon_client::items::ItemStack>) {
    crate::game::mekanismgenerators_glue::loot_drops(game, context, drops);
}
