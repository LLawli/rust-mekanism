// `get_or_insert_with` põe um block entity que não estava (`LevelChunk.setBlockEntity`): a closure o faz depois, então só a posição é anotada; a cola lê o tipo no mundo.
fn machinery_basis_transmitter_get_or_insert(entities: &BlockEntities, pos: Pos) {
    if machinery_transmitters::is_active() && entities.get(&pos).is_none() {
        machinery_transmitters::push_from(entities as *const BlockEntities as usize, machinery_transmitters::Raw::AddedLater { pos: [pos.0, pos.1, pos.2] });
    }
}
