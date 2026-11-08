// Os tooltips dos widgets da tela: a aba de avisos (`GuiWarningTab`, o cabeçalho com a cor de `EnumColor.YELLOW`), a barra de energia
// e a barra de químico. O hook `native_screen_tooltip` só entrega texto com os códigos `§` do vanilla e o posiciona sobre o cursor,
// e o tooltip de um widget fica por cima ou por baixo dele. Roda no começo de `draw_native_screen_tooltip` e, sobre um desses
// widgets, desenha o tooltip e cancela o resto; fora deles segue o caminho normal. Com um item no cursor o original não desenha
// tooltip.
fn mekanism_widget_tooltip(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
) -> core::ops::ControlFlow<()> {
    if game.carried.is_some() {
        return core::ops::ControlFlow::Continue(());
    }
    let Some((block, _, at)) = game.native_screen_at() else {
        return core::ops::ControlFlow::Continue(());
    };
    if crate::game::mekanism_glue::widget_tooltip(game, batch, &block, at, game.cursor) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
