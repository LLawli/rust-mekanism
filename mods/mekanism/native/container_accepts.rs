fn mekanism_container_accepts(
    game: &crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &horizon_client::containers::ModBlockEntity,
    slot: usize,
    stack: &horizon_client::items::ItemStack,
    face: Option<horizon_core::geometry::Direction>,
    cancel: &mut bool,
) {
    crate::game::mekanism_glue::container_accepts(game, pos, entity, slot, stack, face, cancel);
}
