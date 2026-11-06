// Os tooltips da barra de energia e do medidor de lava: ficam por cima ou por baixo do widget, e não sobre o cursor como o de
// `native_screen_tooltip`. Roda no começo de `draw_native_screen_tooltip` e, sobre um desses widgets, desenha o tooltip e cancela o
// resto; fora deles segue o caminho normal. Com um item no cursor o original não desenha tooltip.
fn mekanismgenerators_widget_tooltip(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
) -> core::ops::ControlFlow<()> {
    if game.carried.is_some() {
        return core::ops::ControlFlow::Continue(());
    }
    let Some((block, data, at)) = game.native_screen_at() else {
        return core::ops::ControlFlow::Continue(());
    };
    if crate::game::mekanismgenerators_glue::widget_tooltip(game, batch, &block, &data, at, game.cursor) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
