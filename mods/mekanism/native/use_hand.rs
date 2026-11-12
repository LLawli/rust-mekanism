fn mekanism_use_hand(game: &mut crate::game::Game<'_>, hit: Option<&crate::game::Hit>) -> core::ops::ControlFlow<Result<bool>> {
    crate::game::mekanism_configurator::use_hand(game, hit)
}
