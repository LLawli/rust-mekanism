fn machinery_basis_window_draw_slots(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    ox: f32,
    oy: f32,
) -> core::ops::ControlFlow<()> {
    if game.mb_window_draw_slots(batch, ox, oy) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
