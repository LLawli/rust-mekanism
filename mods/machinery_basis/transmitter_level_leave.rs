// `Game::enter_level`: o nível no lugar vai mudar. O que os ganchos de block entity anotaram até aqui é dele (as operações só mexem no nível no lugar), e sai agora, com o nome dele. A varredura
// de colisão desta thread passa a ler as formas de transmissor do nível que entra (`machinery_transmitter_collision`).
fn machinery_basis_transmitter_level_leave(game: &mut crate::game::Game<'_>, target: crate::game::Dim) {
    horizon_client::collisions::machinery_transmitter_collision::enter(target.id());
    if target != game.dim {
        crate::game::machinery_basis::transmitters_flush(game);
    }
}
