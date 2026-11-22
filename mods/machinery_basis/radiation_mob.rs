fn machinery_basis_radiation_mob(game: &mut crate::game::Game<'_>, dimension: &str, entity: u32) {
    crate::game::machinery_radiation::mob_ticked(game, dimension, entity);
}
