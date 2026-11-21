fn machinery_basis_bounding_picked_stack(game: &crate::game::Game<'_>, pos: [i32; 3]) -> core::ops::ControlFlow<Option<ItemStack>> {
    // `BlockBounding.getCloneItemStack`: o item do principal, ou nada sem principal. Os dados do tile (`addBlockDataToItem`, o Ctrl no criativo) no original são
    // os do tile do apoio, de outro tipo que o do item e por isso sem efeito ao pôr o bloco: aqui o item sai sem eles.
    match crate::game::machinery_basis::bounding_proxy(game, pos) {
        Some(Some(main)) => core::ops::ControlFlow::Break(game.picked_stack(main, false)),
        Some(None) => core::ops::ControlFlow::Break(None),
        None => core::ops::ControlFlow::Continue(()),
    }
}
