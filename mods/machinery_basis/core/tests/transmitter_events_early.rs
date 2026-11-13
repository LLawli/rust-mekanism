//! O chunk que carrega antes de o primeiro tipo de transmissor se registrar (o spawn carrega antes de a tabela de ticks nascer): o que `load_chunk` trouxe dele não se perde.
//! Um arquivo só, porque o módulo guarda o estado em `static`: este teste precisa do processo todo sem registro.
#[allow(dead_code)]
mod events {
    include!("../../transmitter_events.rs");
}
use events::{Raw, Tracker};

#[test]
fn a_chunk_loaded_before_the_first_registration_is_replayed_once() {
    // Antes do registro: um chunk com um cabo e um baú, e outro só com um baú (esse não importa).
    events::push(Raw::LoadChunk(vec![
        ([2, 150, 0], Some("test:cable".to_owned())),
        ([3, 150, 0], None),
    ]));
    events::push(Raw::LoadChunk(vec![([200, 64, 200], None)]));
    // Dois `load_chunk` do mesmo chunk (o 1, 0): ele volta uma vez só.
    events::push(Raw::LoadChunk(vec![(
        [20, 150, 5],
        Some("test:cable".to_owned()),
    )]));
    events::push(Raw::LoadChunk(vec![(
        [21, 150, 5],
        Some("test:cable".to_owned()),
    )]));
    assert!(events::take().is_empty(), "sem dono nada entra na fila");
    events::register("test:cable");
    assert_eq!(
        events::take_early_chunks(),
        vec![[0, 0], [1, 0]],
        "só os chunks com block entity de mod voltam, cada um uma vez"
    );
    assert!(events::take_early_chunks().is_empty(), "uma vez só");
    // A fila traz o que o chunk trouxe, e o `chunk_loaded` os anuncia na ordem do original: o ticket, o `clearRemoved` do cabo e a capability do vizinho.
    let queued = events::take();
    let translated = events::translate("overworld", queued);
    assert!(
        translated.is_empty(),
        "a tradução só guarda o chunk à espera do `chunk_loaded`"
    );
    let loaded = events::chunk_loaded("overworld", [0, 0]);
    let _ = Tracker::new();
    assert!(
        loaded.iter().any(|event| matches!(event, events::Event::Transmitter { pos, kind: events::Kind::ClearRemoved, .. } if *pos == [2, 150, 0])),
        "o cabo do chunk reentra: {loaded:?}"
    );
}
