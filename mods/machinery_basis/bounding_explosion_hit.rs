fn machinery_basis_bounding_explosion_hit(game: &mut crate::game::Game<'_>, pos: [i32; 3], radius: f32, interaction: BlockInteraction, stacks: &mut Vec<([i32; 3], horizon_client::items::ItemStack)>) -> core::ops::ControlFlow<Result<()>> {
    // `BlockBounding.onExplosionHit`: a explosão que pega o apoio é passada ao principal (que sai com o loot dele e leva os apoios); o apoio sem principal segue o
    // bloco comum.
    match crate::game::machinery_basis::bounding_proxy(game, pos) {
        Some(Some(main)) => core::ops::ControlFlow::Break(game.explosion_hit_block(main, radius, interaction, stacks)),
        _ => core::ops::ControlFlow::Continue(()),
    }
}
