// Roda no começo de `Game::move_in_item` (`tryMoveInItem`): todo item que entra num contêiner de bloco, vindo de um funil, de um dropper ou de `native_insert_item` (o ejetor de outra
// máquina), passa por ela. Num transportador logístico a entrada não é um slot: é o `TransporterItemHandler` (`insertUnchecked`, que acha o caminho e põe a pilha em trânsito), e o
// que sobra volta a quem pôs. O bloco de mod tem um `ModContainer` de um slot sempre vazio só para o Horizon o achar como contêiner. Qualquer outro bloco segue o corpo original.
fn machinery_basis_transporter_insert(
    game: &mut crate::game::Game<'_>,
    view: &View,
    stack: &ItemStack,
    face: Option<horizon_core::geometry::Direction>,
) -> core::ops::ControlFlow<(Option<ItemStack>, bool)> {
    use core::ops::ControlFlow::{Break, Continue};
    if view.kind != Kind::Mod {
        return Continue(());
    }
    let Some(((x, y, z), _)) = view.parts.first().copied() else {
        return Continue(());
    };
    match crate::game::machinery_transporters::hopper_insert(game, [x, y, z], face, stack) {
        Some(result) => Break(result),
        None => Continue(()),
    }
}
