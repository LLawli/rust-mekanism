fn mekanism_outline_boxes(game: &crate::game::Game<'_>, pos: [i32; 3]) -> core::ops::ControlFlow<Vec<[f64; 6]>> {
    if let Some(boxes) = crate::game::mekanism_configurator::partial_boxes(game, pos) {
        return core::ops::ControlFlow::Break(boxes);
    }
    match crate::game::mekanism_cube::outline_boxes(game, pos) {
        Some(boxes) => core::ops::ControlFlow::Break(boxes),
        None => core::ops::ControlFlow::Continue(()),
    }
}
