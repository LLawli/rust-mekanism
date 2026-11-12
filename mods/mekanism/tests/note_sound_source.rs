//! A categoria de som do bloco de nota (decisão 92): o original toca a nota em `SoundSource.RECORDS` (`NoteBlock.java:162`), e o Horizon dá aos eventos `block.*` a categoria `block`. O HEAD
//! cancelável de `Audio::play_seeded` (`native/note_sound_source.rs`) toca os eventos `block.note_block.*` com o `source_override` em `record` e deixa os outros como o Horizon os dá. O teste
//! roda o helper de verdade sobre um `Audio` de mentira, que chama o helper no começo de `play_seeded` como a injeção faz.
use std::ops::ControlFlow;

mod settings {
    /// As categorias do Horizon, na ordem de `horizon_client_core::sound_source::SOUND_SOURCES` (a ordem do `SoundSource` do vanilla).
    pub const SOUND_SOURCES: [&str; 11] = [
        "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
        "voice", "ui",
    ];
}

/// Um som que chegou ao motor: o evento, a semente e a categoria que o `source_override` dava nessa hora (`None`: a que o nome dá).
type Played = (String, i64, Option<usize>);

#[derive(Default)]
struct Audio {
    source_override: Option<usize>,
    played: Vec<Played>,
}

impl Audio {
    /// `Audio::play_seeded` com a injeção no começo (o `ControlFlow::Break` cancela o resto) e, no corpo, o registro do som.
    fn play_seeded(
        &mut self,
        event: &str,
        at: Option<[f64; 3]>,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) {
        if let ControlFlow::Break(()) =
            mekanism_note_sound_source(self, event, at, volume, pitch, seed)
        {
            return;
        }
        self.played
            .push((event.to_owned(), seed, self.source_override));
    }

    /// `Audio::play_on`: o `/playsound` com a categoria pedida.
    fn play_on(&mut self, source: usize, event: &str) {
        self.source_override = Some(source);
        self.play_seeded(event, None, 1.0, 1.0, 7);
        self.source_override = None;
    }
}

include!("../native/note_sound_source.rs");

fn index(name: &str) -> usize {
    settings::SOUND_SOURCES
        .iter()
        .position(|n| *n == name)
        .unwrap()
}

#[test]
fn every_note_block_sound_plays_on_the_record_category() {
    let record = index("record");
    // Os 16 instrumentos do vanilla (`NoteBlockInstrument`) e os que imitam um mob, com e sem o `minecraft:`.
    for instrument in [
        "harp",
        "bass",
        "basedrum",
        "snare",
        "hat",
        "bell",
        "flute",
        "chime",
        "guitar",
        "xylophone",
        "iron_xylophone",
        "cow_bell",
        "didgeridoo",
        "bit",
        "banjo",
        "pling",
        "imitate.zombie",
        "imitate.ender_dragon",
    ] {
        for prefix in ["", "minecraft:"] {
            let event = format!("{prefix}block.note_block.{instrument}");
            let mut audio = Audio::default();
            audio.play_seeded(&event, Some([1.0, 2.0, 3.0]), 3.0, 1.0, 42);
            // Um só som, com a mesma semente (o sorteio da variante não muda), na categoria `record`, e o override devolvido.
            assert_eq!(
                audio.played,
                vec![(event.clone(), 42, Some(record))],
                "{event}"
            );
            assert_eq!(audio.source_override, None, "{event}");
        }
    }
}

#[test]
fn any_other_event_keeps_the_category_horizon_gave_it() {
    for event in [
        "minecraft:block.stone.break",
        "minecraft:block.note_blocks.harp",
        "minecraft:block.noteblock.harp",
        "mekanism:block.note_block.harp",
        "minecraft:music_disc.cat",
        "minecraft:entity.player.hurt",
        "block.note_block",
    ] {
        let mut audio = Audio::default();
        audio.play_seeded(event, None, 1.0, 1.0, 9);
        assert_eq!(audio.played, vec![(event.to_owned(), 9, None)], "{event}");
    }
}

#[test]
fn a_requested_category_is_not_touched() {
    // `/playsound minecraft:block.note_block.harp master`: a categoria pedida vale.
    let mut audio = Audio::default();
    audio.play_on(index("master"), "minecraft:block.note_block.harp");
    assert_eq!(
        audio.played,
        vec![(
            "minecraft:block.note_block.harp".to_owned(),
            7,
            Some(index("master"))
        )]
    );
    assert_eq!(audio.source_override, None);
}
