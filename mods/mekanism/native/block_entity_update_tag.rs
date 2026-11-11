fn mekanism_update_tag(
    entity: &horizon_client::containers::ModBlockEntity,
    tag: &mut Option<horizon_world::nbt::Compound>,
) {
    crate::game::mekanism_cube::update_tag(entity, tag);
}
