fn mekanismgenerators_update_tag(
    entity: &horizon_client::containers::ModBlockEntity,
    tag: &mut Option<horizon_world::nbt::Compound>,
) {
    crate::game::mekanismgenerators_glue::update_tag(entity, tag);
}
