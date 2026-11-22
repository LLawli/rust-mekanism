// `MultiPlayerGameMode.interact` com o item do Fluid Tank no modo balde: o `Interact` sai pela função de dentro (a marca de reentrada está em `mekanism_tank`), e o
// resultado do `interactLivingEntity` previsto no cliente decide se a mão balança e se o clique acaba ali.
fn mekanism_tank_use_entity(game: &mut crate::game::Game<'_>, hand: Hand) -> core::ops::ControlFlow<bool> {
    let Some(outcome) = crate::game::mekanism_tank::client_use_entity_begin(game, hand) else {
        return core::ops::ControlFlow::Continue(());
    };
    let done = game.remote_use_entity(hand);
    core::ops::ControlFlow::Break(crate::game::mekanism_tank::client_use_entity_end(game, hand, outcome, done))
}
