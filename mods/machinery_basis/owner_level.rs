// Roda no fim de `Game::levels_to_tick` (os níveis que o servidor anda a cada tick: os que têm convidado, chunk forçado ou ticket). O dono do mundo que ainda não entrou já está no nível dele
// no primeiro tick do original (decisão 195): `mb_owner_level` acrescenta esse nível à lista, para o chunk dele andar na primeira passada mesmo quando outro nível (um chunk forçado do Nether)
// já a enche. Sem dono à espera devolve a lista como veio.
fn machinery_basis_owner_level(game: &crate::game::Game<'_>, value: Vec<Dim>) -> Vec<Dim> {
    game.mb_owner_level(value)
}
