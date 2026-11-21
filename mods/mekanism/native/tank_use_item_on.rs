// `MultiPlayerGameMode.useItemOn` com o item do Fluid Tank no modo balde: o bloco tem a vez e o `useOn` do item passa. A função roda de novo por dentro (a marca de
// reentrada está em `mekanism_tank`), com um item que não é de bloco na mão do cliente, e a pilha volta em seguida.
fn mekanism_tank_use_item_on(game: &mut crate::game::Game<'_>, hand: Hand) -> core::ops::ControlFlow<bool> {
    let Some(saved) = crate::game::mekanism_tank::client_use_on_begin(game, hand) else {
        return core::ops::ControlFlow::Continue(());
    };
    let done = game.remote_use_item_on(hand);
    core::ops::ControlFlow::Break(crate::game::mekanism_tank::client_use_on_end(game, hand, saved, done))
}
