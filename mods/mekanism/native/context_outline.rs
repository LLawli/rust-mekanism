fn mekanism_context_outline(game: &crate::game::Game<'_>, pos: [i32; 3], state: horizon_world::registry::BlockStateId) -> core::ops::ControlFlow<Option<[f64; 6]>> {
    // O transmissor desarma as arestas do contorno a cada chamada; o apoio (`BlockBounding.getShape`) as arma depois, se for ele o bloco mirado.
    match crate::game::mekanism_configurator::partial_outline(game, pos, state) {
        core::ops::ControlFlow::Continue(()) => match crate::game::machinery_basis::bounding_context_outline(game, pos) {
            Some(bounds) => core::ops::ControlFlow::Break(Some(bounds)),
            None => core::ops::ControlFlow::Continue(()),
        },
        answered => answered,
    }
}
