fn mekanism_explosion_detonate(game: &mut crate::game::Game<'_>, blocks: &Vec<[i32; 3]>) {
    crate::game::mekanism_glue::explosion_detonate(game, blocks);
}
