fn machinery_basis_bar_quick_move(
    game: &mut crate::game::Game<'_>,
    host: &mut crate::game::lan::LanHost,
    conn: u32,
    p: &horizon_protocol::packets::play::ContainerClick,
) -> core::ops::ControlFlow<bool> {
    if game.machinery_bar_quick_move(host, conn, p) {
        core::ops::ControlFlow::Break(true)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
