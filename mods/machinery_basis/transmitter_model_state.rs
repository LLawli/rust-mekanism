// Roda no começo de `StateModels::state`, que diz ao catálogo qual escolha de blockstate e quais partes valem para um estado. Se o estado é de transmissor e a block entity do bloco que o
// mesher está desenhando já deu o dado do modelo (`machinery_transmitter_model::select`), devolve as partes do `TransmitterBlockStateModel.collectParts`: a escolha do blockstate do estado
// com um `multipart` de uma parte por lado. Sem o dado o corpo original segue, com as partes `NONE` que o blockstate tem por padrão (o que o original mostra sem dado).
fn machinery_transmitter_model_state(
    catalog: &StateModels,
    state: BlockStateId,
) -> core::ops::ControlFlow<Option<&(usize, SelectedModels)>> {
    let choice = catalog
        .states
        .get(state.0 as usize)
        .and_then(|entry| entry.as_ref())
        .map(|entry| entry.0);
    match crate::mesher::machinery_transmitter_model::select(state.0, choice, |choice, parts| (choice, SelectedModels::Multipart(parts))) {
        Some(parts) => core::ops::ControlFlow::Break(Some(parts)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
