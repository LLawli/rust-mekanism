// Roda no começo de `Streaming::update` (a cada tick do jogo: o carregamento e a geração dos chunks em volta de quem os pede). O Horizon dimensiona o trabalho em voo pelos `workers` do
// streamer (a geração, 3 por worker; a decoração, 2; a luz, 4; a malha, 32), de modo que o servidor integrado traz uns 9 chunks por tick (3 workers) a um mundo que o original enche em
// dezenas de ticks: o original não limita por tick, o que limita é a velocidade dos workers (decisão 197, medido: os 223 cabos de uma linha, em 14 chunks, entram na rede em 21 ticks no
// original e em mais de 68 no port). Aqui o streamer passa a dimensionar o trabalho em voo para, no mínimo, 16 workers (48 chunks por tick, a média do original: 53); a conta é idempotente
// (só sobe, nunca multiplica a cada tick).
fn machinery_basis_stream_budget(streaming: &mut Streaming) {
    if streaming.workers < 16 {
        streaming.workers = 16;
    }
}
