// HEAD de `Streaming::submit` (todo trabalho que o `Streaming` manda às threads dele): o trabalho de luz de um chunk inteiro (`Job::Light`) do `Streaming` do cliente fica
// anotado pelo endereço do chunk do centro, para o HEAD de `light_chunk`, que roda numa thread sem o jogo, saber que o chunk é do mundo do cliente. O `Streaming` do cliente de um
// servidor, o do jogo de um jogador também, tem `remote` ligado (`game/remote.rs`); o do servidor não.
fn machinery_basis_position_light_submit(streaming: &Streaming, job: &Job) {
    if let Job::Light(_, _, region, _, _) = job
        && let Some(centre) = &region[1][1]
    {
        crate::light::machinery_light::note_job(Arc::as_ptr(centre) as usize, streaming.remote);
    }
}
