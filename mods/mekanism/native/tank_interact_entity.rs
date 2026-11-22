// `Player.interactOn` com o item do Fluid Tank no modo balde: a função roda por dentro (a marca de reentrada está em `mekanism_tank`), para o `interact` da entidade
// ter a vez antes do `interactLivingEntity` do item, que só então ordenha.
fn mekanism_tank_interact_entity(game: &mut crate::game::Game<'_>, target: Option<InteractTarget>) -> core::ops::ControlFlow<Result<bool>> {
    match crate::game::mekanism_tank::interact_entity_wrap(game, target) {
        Some(result) => core::ops::ControlFlow::Break(result),
        None => core::ops::ControlFlow::Continue(()),
    }
}
