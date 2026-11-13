// Roda no começo de `Game::update_chunk_tickets` (os tickets que dizem em que chunks o nível anda). No primeiro tick do servidor depois de abrir o mundo o dono do mundo ainda não
// entrou (o Horizon o põe no fim da primeira passada, e o `settle_spawn` soltou o jogador de que precisou para carregar a área); no original ele já está no `PlayerList` e os tickets
// dele (`PLAYER_LOADING` e `PLAYER_SIMULATION`) fazem os block entities do chunk dele andarem no primeiro tick (decisão 195). Enquanto o dono não entra, `mb_owner_tickets` refaz o corpo
// da função com o chunk do dono como mais um jogador e responde por ela; sem dono à espera deixa o corpo original, que não é tocado.
fn machinery_basis_owner_tickets(game: &mut crate::game::Game<'_>) -> core::ops::ControlFlow<()> {
    if game.mb_owner_tickets() {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
