// `LevelChunk.removeBlockEntity`: o block entity de `pos` sai (bloco quebrado ou trocado). O chunk que descarrega não passa por aqui: o `chunk_unload` o conta.
fn machinery_basis_transmitter_remove(entities: &BlockEntities, pos: &Pos) {
    if !machinery_transmitters::is_active() {
        return;
    }
    machinery_transmitters::trace_source("remove", [pos.0, pos.1, pos.2], entities as *const BlockEntities as usize);
    if let Some(entity) = entities.get(pos) {
        let id = match entity {
            BlockEntity::Mod(entity) => Some(entity.id.clone()),
            _ => None,
        };
        machinery_transmitters::push(machinery_transmitters::Raw::Removed { pos: [pos.0, pos.1, pos.2], id });
    }
}
