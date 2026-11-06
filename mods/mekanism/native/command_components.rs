// Os comandos só aceitam os componentes da lista que o jogo exporta, e o do mod não está nela. Qualquer outro id volta o cursor
// e segue o caminho normal.
fn mekanism_command_component(state: &mut ItemState<'_, '_>) -> core::ops::ControlFlow<CmdResult<String>> {
    const COMPONENT: &str = "mekanism:upgrade_type";
    let cursor = state.reader.cursor;
    if let Ok(id) = super::args::read_identifier(state.reader)
        && id == COMPONENT
    {
        return core::ops::ControlFlow::Break(Ok(id));
    }
    state.reader.cursor = cursor;
    core::ops::ControlFlow::Continue(())
}
