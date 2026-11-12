// A categoria de som do bloco de nota (decisão 92 do plano). `NoteBlock.triggerEvent` toca a nota em `SoundSource.RECORDS` (`NoteBlock.java:162`) e a tabela do Horizon (`settings::sound_source`)
// põe todo evento `block.*` em `block`: com o volume de `block` em 0 e o de `record` em 1 (as categorias são dois controles do jogador), o original toca e o Horizon não. `sound_source` tem itens
// aninhados (`E_UNSUPPORTED_CONTROL_FLOW`), então a injeção é o HEAD cancelável de `Audio::play_seeded`, por onde passa todo som tocado num ponto: para um evento `block.note_block.*` (todo bloco de
// nota, do vanilla ou sobre um bloco do mod, que é o que o `NoteBlock` do vanilla faz) liga o `source_override` que o `/playsound` já usa (`Audio::play_on`), chama a si mesmo com a mesma semente (o
// sorteio do som não muda) e cancela a chamada de fora. Com uma categoria já pedida (`/playsound ... master`) não toca em nada.
fn mekanism_note_sound_source(audio: &mut Audio, event: &str, at: Option<[f64; 3]>, volume: f32, pitch: f32, seed: i64) -> core::ops::ControlFlow<()> {
    const NOTE_BLOCK_SOUNDS: &str = "block.note_block.";
    if audio.source_override.is_some() || !event.trim_start_matches("minecraft:").starts_with(NOTE_BLOCK_SOUNDS) {
        return core::ops::ControlFlow::Continue(());
    }
    let Some(record) = crate::settings::SOUND_SOURCES.iter().position(|name| *name == "record") else {
        return core::ops::ControlFlow::Continue(());
    };
    audio.source_override = Some(record);
    audio.play_seeded(event, at, volume, pitch, seed);
    audio.source_override = None;
    core::ops::ControlFlow::Break(())
}
