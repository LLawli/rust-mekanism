fn mekanism_block_entity(
    block: &str,
    entity: &mut Option<(String, horizon_world::nbt::Compound)>,
) {
    crate::game::mekanism_glue::block_entity(block, entity);
}
