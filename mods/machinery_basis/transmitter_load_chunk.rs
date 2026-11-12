// `load_chunk`: os block entities de um chunk que carregou ou nasceu, na ordem em que o jogo os recebeu; o `chunk_load` os anuncia.
fn machinery_basis_transmitter_load_chunk(entities: &Vec<(Pos, BlockEntity)>) {
    if !machinery_transmitters::is_active() || entities.is_empty() {
        return;
    }
    let list = entities
        .iter()
        .map(|(pos, entity)| {
            let id = match entity {
                BlockEntity::Mod(entity) => Some(entity.id.clone()),
                _ => None,
            };
            ([pos.0, pos.1, pos.2], id)
        })
        .collect();
    machinery_transmitters::push(machinery_transmitters::Raw::LoadChunk(list));
}
