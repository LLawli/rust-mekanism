// `LevelChunk.setBlockEntity`: um block entity entra no lugar de `pos` (posto, ou trocado por outro). Anota a ocorrência para a cola dos transmissores.
fn machinery_basis_transmitter_insert(entities: &BlockEntities, pos: Pos, entity: &BlockEntity) {
    let id = |entity: &BlockEntity| match entity {
        BlockEntity::Mod(entity) => Some(entity.id.clone()),
        _ => None,
    };
    let new = id(entity);
    let old = entities.get(&pos).map(id);
    machinery_transmitters::note_insert([pos.0, pos.1, pos.2], new.as_deref(), old.as_ref().map(Option::as_deref));
}
