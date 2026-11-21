// `MultiPlayerGameMode.useItem` com o item do Fluid Tank no modo balde: o pacote sai pela função de dentro (a marca de reentrada está em `mekanism_tank`), e o
// resultado do `use` previsto no cliente decide se a mão balança.
fn mekanism_tank_use_item(game: &mut crate::game::Game<'_>, hand: Hand) -> core::ops::ControlFlow<bool> {
    let Some(outcome) = crate::game::mekanism_tank::client_use_begin(game, hand) else {
        return core::ops::ControlFlow::Continue(());
    };
    let done = game.remote_use_item(hand);
    core::ops::ControlFlow::Break(crate::game::mekanism_tank::client_use_end(game, hand, outcome, done))
}
