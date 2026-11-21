fn mekanism_interact_entity(game: &mut crate::game::Game<'_>, entity: i32, stack: Option<&horizon_client::items::ItemStack>, claimed: &mut bool) {
    crate::game::mekanism_tank::interact_entity(game, entity, stack, claimed);
}
