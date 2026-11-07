fn machinery_basis_bar_clicked_outside(game: &crate::game::Game<'_>, batch: &HudBatch) -> core::ops::ControlFlow<bool> {
    if game.mb_bar_over_element(batch) {
        core::ops::ControlFlow::Break(false)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
