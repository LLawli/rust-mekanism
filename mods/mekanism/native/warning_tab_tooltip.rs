// O tooltip da aba de avisos (`GuiWarningTab`): o cabeçalho tem a cor de `EnumColor.YELLOW`, que o hook `native_screen_tooltip` não
// consegue dar, porque só entrega texto com os códigos `§` do vanilla. Roda no começo de `draw_native_screen_tooltip` e, sobre a
// aba, desenha o tooltip e cancela o resto; fora dela segue o caminho normal. Com um item no cursor o original não desenha tooltip.
fn mekanism_warning_tab_tooltip(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
) -> core::ops::ControlFlow<()> {
    if game.carried.is_some() {
        return core::ops::ControlFlow::Continue(());
    }
    let Some((block, data, at)) = game.native_screen_at() else {
        return core::ops::ControlFlow::Continue(());
    };
    if crate::game::mekanism_glue::warning_tab_tooltip(game, batch, &block, &data, at, game.cursor) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
