fn mekanismgenerators_block_entity(
    block: &str,
    entity: &mut Option<(String, horizon_world::nbt::Compound)>,
) {
    crate::game::mekanismgenerators_glue::block_entity(block, entity);
}
