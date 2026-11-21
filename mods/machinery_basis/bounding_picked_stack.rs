fn machinery_basis_bounding_picked_stack(game: &crate::game::Game<'_>, pos: [i32; 3], include_data: bool) -> core::ops::ControlFlow<Option<ItemStack>> {
    // `BlockBounding.getCloneItemStack`: o item do principal, ou nada sem principal; com o Ctrl no criativo, os dados do tile do apoio (`addBlockDataToItem` da
    // posição mirada).
    match crate::game::machinery_basis::bounding_proxy(game, pos) {
        Some(Some(main)) => core::ops::ControlFlow::Break(crate::game::machinery_basis::bounding_picked_stack(game, pos, main, include_data)),
        Some(None) => core::ops::ControlFlow::Break(None),
        None => core::ops::ControlFlow::Continue(()),
    }
}
