//! O arquivo de cliente `Mekanism/client.toml` do original (`ClientConfig.java:74-81`, a seção `sounds`): o texto que o port escreve, o que
//! ele lê, e o que `baseSoundVolume` e `enableMachineSounds` mudam no som. O arquivo de referência (`mekanism_client.toml`) é o que o
//! cliente oficial escreveu sozinho na primeira vez que abriu.
use machinery_basis_core::sound::{
    SoundConfig, TileSound, TileSoundEngine, TileTickableSound, TileView,
};
use mekanism_rules::{
    CLIENT_CONFIG, WINDOW_TYPES, client_default_text, machine_effects, sound_config,
};
use std::path::Path;

const SOUND: &str = "mekanism:tile.machine.enrichment_chamber";

fn original() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/rastros/mekanism_client.toml"),
    )
    .unwrap()
}

/// A seção `accessibility` do arquivo do original, sem `opaqueTransmitters` (os transmissores ainda não têm o que mostrar no lugar do conteúdo).
fn original_accessibility_section() -> String {
    let text = original();
    let start = text
        .find("#Settings for configuring Accessibility settings")
        .unwrap();
    let end = text[start..].find("\n\n").unwrap() + start + 1;
    let mut kept = Vec::new();
    let mut lines = text[start..end].lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("#If true, renders sides for") {
            // O comentário e a chave `opaqueTransmitters` vão juntos.
            lines.next();
            continue;
        }
        kept.push(line);
    }
    kept.join("\n") + "\n"
}

/// A seção `hud` do arquivo do original, com as três chaves que os efeitos leem (`enabled`, `scale` e `reverse`): o resto é do HUD do MekaSuit.
fn original_hud_section() -> String {
    let text = original();
    let start = text.find("#Settings for configuring Mekanism's HUD").unwrap();
    let end = text[start..].find("\n\n").unwrap() + start + 1;
    let mut kept = Vec::new();
    let mut keys = 0;
    for line in text[start..end].lines() {
        let is_key = line.starts_with('\t') && !line.trim_start().starts_with('#');
        if is_key {
            keys += 1;
        }
        // Cada chave tem os comentários na frente: ficam os das três primeiras.
        if keys >= 3 && !is_key && line.starts_with('\t') {
            continue;
        }
        if keys > 3 {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n") + "\n"
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

/// A seção `rendering` do arquivo do original, sem as chaves que o port não lê: o comentário e o cabeçalho de `[rendering]` (o `berRange` e o resto ficam de fora) e a
/// sub-seção `rendering.particle` só com `machineEffects` (multibloco, radiação, atração magnética e área de ferramenta ficam de fora).
fn original_rendering_section() -> String {
    let text = original();
    let start = text
        .find("#Settings for adjusting how Mekanism renders certain blocks")
        .unwrap();
    // `energyColor` é a primeira chave de `[rendering]` (o comentário e a chave, sem faixa), e o port a lê (a barra de energia dos itens).
    let header: Vec<&str> = text[start..].lines().take(4).collect();
    assert!(
        header[3].trim_start().starts_with("energyColor"),
        "{header:?}"
    );
    let nested_start = text
        .find("\t#Settings for configuring Mekanism's Particles")
        .unwrap();
    let nested = &text[nested_start..];
    let nested = &nested[..nested.find("\n\n").unwrap_or(nested.len())];
    let mut kept = Vec::new();
    let mut lines = nested.lines().peekable();
    while let Some(line) = lines.next() {
        // Cada chave leva o comentário na linha de cima: fica só o par de `machineEffects`.
        let own = line.contains("machineEffects")
            || lines
                .peek()
                .is_some_and(|next| next.contains("machineEffects"));
        if line.starts_with("\t\t") && !own {
            continue;
        }
        kept.push(line);
    }
    format!("{}\n\n{}\n", header.join("\n"), kept.join("\n"))
}

#[test]
fn the_default_text_is_the_sounds_and_rendering_sections_of_the_file_the_original_wrote() {
    assert_eq!(
        CLIENT_CONFIG.default_text(),
        format!(
            "{}\n{}\n{}\n{}",
            original_accessibility_section(),
            original_sounds_section(),
            original_rendering_section(),
            original_hud_section()
        )
    );
}

#[test]
fn machine_effects_is_read_from_the_particle_section_and_defaults_to_on() {
    assert!(machine_effects(&CLIENT_CONFIG.load(&original())));
    assert!(machine_effects(&CLIENT_CONFIG.load("")));
    let off = original().replace("machineEffects = true", "machineEffects = false");
    assert!(!machine_effects(&CLIENT_CONFIG.load(&off)));
    // Uma chave com o nome certo na seção errada não vale.
    assert!(machine_effects(
        &CLIENT_CONFIG.load("[rendering]\n\tmachineEffects = false\n")
    ));
    // O valor errado volta ao padrão e o arquivo é escrito de novo, com o resto de `[rendering]` como estava.
    let wrong = original().replace("machineEffects = true", "machineEffects = 5");
    let loaded = CLIENT_CONFIG.load(&wrong);
    assert!(loaded.corrected);
    assert!(machine_effects(&loaded));
    let rewritten = CLIENT_CONFIG.rewrite(&wrong, &loaded);
    assert!(rewritten.contains("berRange = 256"));
    assert!(rewritten.contains("machineEffects = true"));
    assert!(!CLIENT_CONFIG.load(&rewritten).corrected);
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

/// O arquivo é o mesmo onde as janelas guardam a posição (`WindowPositions`): cada escritor deixa a seção do outro como está.
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
    let saved = replace_sections(&edited, "window", &positions.to_toml(&WINDOW_TYPES));
    assert_eq!(WindowPositions::from_toml(&saved), positions);
    assert_eq!(sound_config(&CLIENT_CONFIG.load(&saved)).base_volume, 0.25);
}

/// O bloco `[window]` do arquivo que o cliente oficial escreveu sozinho (onze janelas, a de upgrades em (-11, 15)): ler e escrever de novo dá o mesmo texto,
/// byte a byte, com os comentários, a ordem do enum `WindowType` e o recuo do NightConfig.
#[test]
fn the_window_block_is_the_one_the_original_writes() {
    use machinery_basis_core::window::WindowPositions;
    let text = original();
    let block = &text[text.find("#Stores the last position").unwrap()..];
    let positions = WindowPositions::from_toml(&text);
    assert_eq!(positions.to_toml(&WINDOW_TYPES), block);
}

/// O que o Horizon escreve quando o arquivo não existe é o do original sem o que o port não lê: a seção `sounds` (sem `enablePlayer`) e a janela de upgrades, cada
/// trecho igual ao do arquivo do original.
#[test]
fn the_default_file_is_the_original_without_what_the_port_does_not_read() {
    use machinery_basis_core::window::WindowPositions;
    let text = client_default_text();
    assert!(
        text.starts_with(&format!(
            "{}\n{}",
            original_accessibility_section(),
            original_sounds_section()
        )),
        "{text}"
    );
    let window = &text[text.find("#Stores the last position").unwrap()..];
    let original = original();
    let original_window = &original[original.find("#Stores the last position").unwrap()..];
    let upgrade = &original_window[original_window
        .find("\t#The last position the Side")
        .unwrap()..];
    // O padrão tem só as janelas de lados, de transporte e de upgrades, com x e y por gravar; o original tem a de upgrades que o jogo gravou em (-11, 15).
    let header = &original_window[..original_window
        .find("\t#The last position the Color")
        .unwrap()];
    assert_eq!(
        window,
        format!(
            "{header}{}",
            upgrade
                .replace("x = -11", "x = 2147483647")
                .replace("y = 15", "y = 2147483647")
        )
    );
    // Lê limpo: nada a corrigir.
    assert!(!CLIENT_CONFIG.load(&text).corrected);
    let positions = WindowPositions::from_toml(&text);
    assert_eq!(positions.to_toml(&WINDOW_TYPES), window);
}

/// Um arquivo que veio do mod de verdade (as seis seções e as onze janelas) não perde nada quando o port corrige o volume: o port só escreve o que lê.
#[test]
fn a_file_from_the_original_keeps_everything_it_does_not_read() {
    let edited = original().replace("baseVolume = 1.0", "baseVolume = 25.5");
    let loaded = CLIENT_CONFIG.load(&edited);
    assert!(loaded.corrected);
    assert_eq!(sound_config(&loaded).base_volume, 10.0);
    let written = CLIENT_CONFIG.rewrite(&edited, &loaded);
    assert_eq!(
        written,
        original().replace("baseVolume = 1.0", "baseVolume = 10.0")
    );
}
