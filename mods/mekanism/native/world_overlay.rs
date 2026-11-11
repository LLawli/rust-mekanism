fn mekanism_world_overlay(game: &mut crate::game::Game<'_>, camera: glam::DVec3, alpha: f64) -> core::ops::ControlFlow<PrimitiveGeometry> {
    match mekanism_overlay::geometry(game, camera, alpha) {
        Some(geometry) => core::ops::ControlFlow::Break(geometry),
        None => core::ops::ControlFlow::Continue(()),
    }
}
