// O realce do slot sob o cursor nas telas do Mekanism. `VirtualSlotContainerScreen.extractSlotHighlightFront` chama
// `extractSlotHighlightBack`, e não `Front`: a frente do realce usa o sprite de trás, o que o vanilla não faz. Roda no começo de
// `draw_slot_highlight`, só para a camada da frente e só numa tela de bloco do Mekanism ou do Mekanism Generators; o resto segue.
fn mekanism_slot_highlight(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    front: bool,
    x: f32,
    y: f32,
) -> core::ops::ControlFlow<()> {
    if !front {
        return core::ops::ControlFlow::Continue(());
    }
    let Some(pos) = game.native_screen_pos() else {
        return core::ops::ControlFlow::Continue(());
    };
    if game.native_block(pos).starts_with("mekanism") {
        game.draw_slot_highlight(batch, false, x, y);
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
