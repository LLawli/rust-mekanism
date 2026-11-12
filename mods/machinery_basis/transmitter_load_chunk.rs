// `load_chunk`: os block entities de um chunk que carregou ou nasceu, na ordem em que o jogo os recebeu; o `chunk_load` os anuncia.
fn machinery_basis_transmitter_load_chunk(entities: &Vec<(Pos, BlockEntity)>) {
    // Sem dono ainda (o primeiro tipo se registra quando a tabela de ticks nasce) o módulo guarda só o que tem block entity de mod, para a primeira passada o entregar.
    if entities.is_empty() || (!machinery_transmitters::is_active() && !entities.iter().any(|(_, entity)| matches!(entity, BlockEntity::Mod(_)))) {
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
