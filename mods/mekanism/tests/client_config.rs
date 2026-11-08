//! O arquivo de cliente `Mekanism/client.toml` do original (`ClientConfig.java:74-81`, a seção `sounds`): o texto que o port escreve, o que
//! ele lê, e o que `baseSoundVolume` e `enableMachineSounds` mudam no som. O arquivo de referência (`mekanism_client.toml`) é o que o
//! cliente oficial escreveu sozinho na primeira vez que abriu.
use machinery_basis_core::sound::{
    SoundConfig, TileSound, TileSoundEngine, TileTickableSound, TileView,
};
use mekanism_rules::{CLIENT_CONFIG, sound_config};
use std::path::Path;

const SOUND: &str = "mekanism:tile.machine.enrichment_chamber";

fn original() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/rastros/mekanism_client.toml"),
    )
    .unwrap()
}

/// A seção `sounds` do arquivo do original, sem `enablePlayer` (o port não tem os sons de jogador).
fn original_sounds_section() -> String {
    let text = original();
    let start = text
        .find("#Settings for configuring Mekanism's Sounds")
        .unwrap();
    let end = text[start..].find("\n\n").unwrap() + start + 1;
    let section = &text[start..end];
    let mut kept = Vec::new();
    let mut lines = section.lines().peekable();
    while let Some(line) = lines.next() {
        if line
            .trim_start()
            .starts_with("#Play sounds for when any player")
        {
            // O comentário e a chave `enablePlayer` vão juntos.
            lines.next();
            continue;
        }
        kept.push(line);
    }
    kept.join("\n") + "\n"
}

#[test]
fn the_default_text_is_the_sounds_section_of_the_file_the_original_wrote() {
    assert_eq!(CLIENT_CONFIG.default_text(), original_sounds_section());
}

#[test]
fn the_original_file_reads_as_correct_with_the_defaults() {
    let loaded = CLIENT_CONFIG.load(&original());
    assert!(!loaded.corrected);
    assert_eq!(
        sound_config(&loaded),
        SoundConfig {
            enabled: true,
            base_volume: 1.0
        }
    );
    assert_eq!(
        sound_config(&CLIENT_CONFIG.load(&CLIENT_CONFIG.default_text())),
        SoundConfig::default()
    );
}

fn config_of(volume: &str, machine: &str) -> SoundConfig {
    sound_config(&CLIENT_CONFIG.load(&format!(
        "[sounds]\n\tbaseVolume = {volume}\n\tenableMachine = {machine}\n"
    )))
}

#[test]
fn the_volume_keeps_the_double_of_the_file_as_a_float() {
    assert_eq!(config_of("0.5", "true").base_volume, 0.5);
    assert_eq!(config_of("2", "true").base_volume, 2.0);
    assert_eq!(config_of("10.0", "true").base_volume, 10.0);
    assert_eq!(config_of("0.0", "true").base_volume, 0.0);
    // `CachedFloatValue`: o `double` do arquivo vira `float`.
    assert_eq!(config_of("0.1", "true").base_volume, 0.1_f32);
}

#[test]
fn a_volume_out_of_range_is_corrected_to_the_limit_and_the_file_is_written_again() {
    let high = CLIENT_CONFIG.load("[sounds]\nbaseVolume = 25.5\nenableMachine = true\n");
    assert_eq!(sound_config(&high).base_volume, 10.0);
    assert!(high.corrected);
    let low = CLIENT_CONFIG.load("[sounds]\nbaseVolume = -1\nenableMachine = true\n");
    assert_eq!(sound_config(&low).base_volume, 0.0);
    assert!(low.corrected);
    assert!(CLIENT_CONFIG.render(&high).contains("baseVolume = 10.0"));
}

#[test]
fn machine_sounds_can_be_turned_off() {
    assert!(!config_of("1.0", "false").enabled);
    assert!(config_of("1.0", "true").enabled);
}

/// O jogo falso: o jogador está ao alcance e nada mais.
#[derive(Default)]
struct Game {
    started: Vec<String>,
    stopped: u32,
}

impl TileSoundEngine for Game {
    fn start(&mut self, sound: &str, _initial_volume: f32) -> bool {
        self.started.push(sound.to_owned());
        true
    }
    fn stop(&mut self) {
        self.stopped += 1;
    }
}

fn view(can_play: bool) -> TileView<'static> {
    TileView {
        can_play,
        removed: false,
        fully_muffled: false,
        sound: SOUND,
    }
}

#[test]
fn machine_sounds_off_start_nothing_and_leave_a_running_one_alone() {
    let off = config_of("1.0", "false");
    let mut tile = TileSound::default();
    let mut game = Game::default();
    tile.update(off, view(true), false, &mut game);
    assert!(game.started.is_empty());
    // `updateSound` volta antes de olhar o estado: a máquina que para também não para o som, nem o tick de um som já tocando.
    let on = config_of("1.0", "true");
    tile.update(on, view(true), false, &mut game);
    assert_eq!(game.started.len(), 1);
    tile.update(off, view(false), true, &mut game);
    assert_eq!(
        game.stopped, 0,
        "o Java sai de `updateSound` antes do `else if (activeSound != null)`"
    );
    assert!(tile.has_active_sound());
    tile.update(on, view(false), true, &mut game);
    assert_eq!(game.stopped, 1);
}

#[test]
fn the_base_volume_is_read_when_the_sound_is_created_and_the_muffler_scales_it() {
    for (base, expected) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (2.5, 2.5), (10.0, 10.0)] {
        let config = config_of(&base.to_string(), "true");
        let sound = TileTickableSound::new(1.0, config.base_volume, 1.0, 0);
        assert_eq!(sound.volume, expected);
        assert_eq!(sound.original_volume(), expected);
        // Sem upgrade de silêncio: o fator é 1. Com o máximo (1 de 1): 0.
        let muffled = TileTickableSound::new(1.0, config.base_volume, 0.0, 0);
        assert_eq!(muffled.volume, 0.0);
    }
}

#[test]
fn a_base_volume_change_does_not_touch_a_sound_already_playing() {
    // `originalVolume` é final e `tick()` o relê dele mesmo: só um som novo ouve o valor novo.
    let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 0);
    sound.tick(Some(20), true, |_| true, 1.0);
    assert_eq!(sound.volume, 1.0);
    assert_eq!(sound.original_volume(), 1.0);
}

/// O arquivo é o mesmo onde a janela de upgrades guarda a posição (`WindowPositions`): cada escritor deixa a seção do outro como está.
#[test]
fn the_sound_section_and_the_window_positions_live_in_one_file() {
    use machinery_basis_core::client_config::replace_sections;
    use machinery_basis_core::window::WindowPositions;
    let positions_only = "[window.upgrade]\nx = -11\ny = 15\npinned = true\n";
    let positions = WindowPositions::from_toml(positions_only);
    // Um arquivo que só tem a posição: a primeira leitura acrescenta `sounds` e deixa a janela.
    let loaded = CLIENT_CONFIG.load(positions_only);
    assert!(loaded.corrected, "falta a seção sounds");
    let with_sounds = CLIENT_CONFIG.rewrite(positions_only, &loaded);
    assert_eq!(WindowPositions::from_toml(&with_sounds), positions);
    assert!(!CLIENT_CONFIG.load(&with_sounds).corrected);
    assert_eq!(
        sound_config(&CLIENT_CONFIG.load(&with_sounds)),
        SoundConfig::default()
    );
    // A janela grava de novo: `sounds` fica, com o que o jogador pôs nele.
    let edited = with_sounds.replace("baseVolume = 1.0", "baseVolume = 0.25");
    let saved = replace_sections(&edited, "window", &positions.to_toml());
    assert_eq!(WindowPositions::from_toml(&saved), positions);
    assert_eq!(sound_config(&CLIENT_CONFIG.load(&saved)).base_volume, 0.25);
}
