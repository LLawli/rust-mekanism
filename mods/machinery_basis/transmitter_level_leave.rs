// `Game::enter_level`: o nível no lugar vai mudar. O que os ganchos de block entity anotaram até aqui é dele (as operações só mexem no nível no lugar), e sai agora, com o nome dele.
fn machinery_basis_transmitter_level_leave(game: &mut crate::game::Game<'_>, target: crate::game::Dim) {
    if target != game.dim {
        crate::game::machinery_basis::transmitters_flush(game);
    }
}
