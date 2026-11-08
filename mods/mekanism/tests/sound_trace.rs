//! Repete, tick a tick, o som das máquinas do cliente oficial (`rastros/som_medido.jsonl`, da `testes/CenaSomMedida.java`): para cada cenário, o que o tile
//! e o jogador valiam em cada tick do cliente (`lit`, `fator`, `mudo`, os pés, `baseSoundVolume`, `enableMachineSounds`) entra na regra do port
//! (`TileSound`, `TileTickableSound`), e o volume que ela devolve tem de ser o da instância de som do original naquele tick.
//!
//! O intervalo de verificação do `TileTickableSound` é sorteado (`ThreadLocalRandom`): o rastro o traz, e a regra recebe o mesmo.
use machinery_basis_core::sound::{
    DEFAULT_ATTENUATION_DISTANCE, MufflingSync, SOUND_SOURCE_BLOCKS_NAME, SoundConfig,
    TILE_SOUND_CAN_START_SILENT, TILE_SOUND_DELAY, TILE_SOUND_LOOPING, TILE_SOUND_PITCH, TileSound,
    TileSoundEngine, TileTickableSound, TileView, player_in_range, sound_position,
};
use serde_json::Value;
use std::path::Path;

const SOUND: &str = "mekanism:tile.machine.enrichment_chamber";
/// O bloco da `CenaSomMedida` (`X`, `Y`, `Z`).
const BLOCK: [i32; 3] = [300, -60, 3];

/// As linhas do rastro, com as repetições abertas: uma linha `tick` com `x` vale `x` linhas a mais, cada uma com `n` e `gt` um acima da anterior
/// (`dev/referencia/som_compactar.py`).
fn rows() -> Vec<Value> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/som_medido.jsonl");
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).unwrap();
        let repeats = row["x"].as_u64().unwrap_or(0);
        out.push(row.clone());
        for step in 1..=repeats {
            let mut copy = row.clone();
            copy["n"] = (row["n"].as_u64().unwrap() + step).into();
            copy["gt"] = (row["gt"].as_i64().unwrap() + step as i64).into();
            out.push(copy);
        }
    }
    out
}

struct Scenario {
    name: String,
    /// `muffling` do cenário: os upgrades de silêncio postos antes de tudo.
    muffling: i64,
    ticks: Vec<Value>,
    /// (tick do cenário, contagem de silêncio que o servidor passa a ter): os eventos `muffling` do arquivo de cenários.
    muffling_events: Vec<(u64, i64)>,
}

fn scenarios() -> Vec<Scenario> {
    let mut out: Vec<Scenario> = Vec::new();
    for row in rows() {
        match row["e"].as_str().unwrap() {
            "cenario" => out.push(Scenario {
                name: row["nome"].as_str().unwrap().to_owned(),
                muffling: row["muffling"].as_i64().unwrap(),
                ticks: Vec::new(),
                muffling_events: Vec::new(),
            }),
            "tick" => out.last_mut().unwrap().ticks.push(row),
            "evento" if row["que"] == "muffling" => {
                out.last_mut().unwrap().muffling_events.push((
                    row["n"].as_u64().unwrap(),
                    row["valor"].as_str().unwrap().parse().unwrap(),
                ));
            }
            _ => {}
        }
    }
    out
}

/// O jogo falso: só responde se o jogador está ao alcance e guarda a instância.
struct Game {
    feet: [f64; 3],
    factor: f32,
    base_volume: f32,
    /// O intervalo que o Java sorteou para a próxima instância.
    interval: i32,
    playing: Option<TileTickableSound>,
}

impl TileSoundEngine for Game {
    fn start(&mut self, _sound: &str, initial_volume: f32) -> bool {
        if self.playing.is_some() {
            return true;
        }
        let instance = TileTickableSound::new(
            initial_volume,
            self.base_volume,
            self.factor,
            self.interval - 20,
        );
        if !player_in_range(
            self.feet,
            sound_position(BLOCK),
            instance.volume,
            DEFAULT_ATTENUATION_DISTANCE,
        ) {
            return false;
        }
        self.playing = Some(instance);
        true
    }

    fn stop(&mut self) {
        self.playing = None;
    }
}

/// O que o original tinha a tocar no fim do tick: `Some(volume)` se a instância está ativa e não parou.
fn java_sound(row: &Value) -> Option<f64> {
    let sound = &row["som"];
    if sound.is_null() || !sound["ativo"].as_bool().unwrap() || sound["parado"].as_bool().unwrap() {
        return None;
    }
    sound["volume"].as_f64()
}

/// Joga um cenário e devolve, por tick, (n, o volume do port, o volume do original). `orphan` é o som que o cenário anterior deixou tocando: com as máquinas
/// desligadas o tile que sai não para o som (`updateSound` volta antes do `stopTileSound`), e o tile novo no mesmo bloco o encontra em `startTileSound`.
fn replay(
    scenario: &Scenario,
    orphan: &mut Option<TileTickableSound>,
) -> Vec<(u64, Option<f64>, Option<f64>)> {
    let mut tile = TileSound::default();
    let mut game = Game {
        feet: [0.0; 3],
        factor: 1.0,
        base_volume: 1.0,
        interval: 20,
        playing: orphan.take(),
    };
    let mut last_config = SoundConfig::default();
    let mut out = Vec::new();
    // Os primeiros ticks de um cenário mostram o bloco do cenário anterior ou o do mundo salvo.
    let first = scenario
        .ticks
        .iter()
        .position(|row| !row["tile"].is_null() && !row["tile"]["lit"].as_bool().unwrap())
        .unwrap_or(0);
    for (index, row) in scenario.ticks.iter().enumerate().skip(first) {
        let lit = row["tile"]["lit"].as_bool().unwrap();
        game.factor = row["tile"]["fator"].as_f64().unwrap() as f32;
        game.feet = [
            row["pes"][0].as_f64().unwrap(),
            row["pes"][1].as_f64().unwrap(),
            row["pes"][2].as_f64().unwrap(),
        ];
        game.base_volume = row["base"].as_f64().unwrap() as f32;
        // O intervalo da instância que está para nascer: o da primeira linha, daqui em diante, que tem som.
        game.interval = scenario.ticks[index..]
            .iter()
            .find_map(|later| later["som"]["intervalo"].as_i64())
            .unwrap_or(20) as i32;
        let config = SoundConfig {
            enabled: row["liga"].as_bool().unwrap(),
            base_volume: game.base_volume,
        };
        let view = TileView {
            can_play: lit,
            removed: false,
            fully_muffled: row["tile"]["mudo"].as_bool().unwrap(),
            sound: SOUND,
        };
        let engine_active = game.playing.is_some();
        tile.update(config, view, engine_active, &mut game);
        last_config = config;
        // O motor de som tica a instância antes de o nível somar o tick: o relógio que ela lê é o do fim do tick menos um.
        let gt = row["gt"].as_i64().unwrap() - 1;
        // `MekanismUtils.isTickingNormally(level)`: o `runsNormally` do relógio do nível do cliente, que o rastro grava.
        let normal = row["normal"].as_bool().unwrap();
        let (feet, factor) = (game.feet, game.factor);
        let mut stopped = false;
        if let Some(instance) = game.playing.as_mut() {
            instance.tick(
                Some(gt),
                normal,
                |volume| {
                    player_in_range(
                        feet,
                        sound_position(BLOCK),
                        volume,
                        DEFAULT_ATTENUATION_DISTANCE,
                    )
                },
                factor,
            );
            stopped = instance.is_stopped();
        }
        if stopped {
            game.playing = None;
        }
        out.push((
            row["n"].as_u64().unwrap(),
            game.playing
                .as_ref()
                .map(|instance| f64::from(instance.volume)),
            java_sound(row),
        ));
    }
    // O bloco sai (`setRemoved` chama `updateSound` uma vez).
    tile.update(
        last_config,
        TileView {
            can_play: true,
            removed: true,
            fully_muffled: false,
            sound: "",
        },
        true,
        &mut game,
    );
    *orphan = game.playing.take();
    out
}

fn same(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => (a - b).abs() < 1e-3,
        _ => false,
    }
}

#[test]
fn the_trace_has_the_scenarios_the_port_repeats() {
    let names: Vec<String> = scenarios()
        .into_iter()
        .map(|scenario| scenario.name)
        .collect();
    for needed in [
        "padrao",
        "silencio_1",
        "base_0_5",
        "maquinas_off",
        "pes_perto",
        "afastando",
    ] {
        assert!(
            names.iter().any(|name| name == needed),
            "falta o cenário {needed}"
        );
    }
}

#[test]
fn every_scenario_sounds_as_the_original_does_tick_by_tick() {
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut orphan = None;
    for scenario in scenarios() {
        let ticks = replay(&scenario, &mut orphan);
        assert!(ticks.len() > 100, "{}: poucos ticks", scenario.name);
        checked += ticks.len();
        for (n, port, java) in ticks {
            if !same(port, java) {
                failures.push(format!(
                    "{} n={n}: port {port:?}, original {java:?}",
                    scenario.name
                ));
            }
        }
    }
    assert!(checked > 2000, "só {checked} ticks conferidos");
    assert!(
        failures.is_empty(),
        "{} ticks diferem:\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Quanto de silêncio o cliente enxerga: o `fator` do tile dele (`1 - contagem / max`, com `max` 1).
fn factor_of(count: i32) -> f64 {
    1.0 - f64::from(count.clamp(0, 1))
}

#[test]
fn the_client_learns_a_removed_muffler_only_with_the_next_block_update() {
    let mut checked = 0;
    for scenario in scenarios() {
        let mut sync = MufflingSync::default();
        let mut client = 0;
        let mut server = scenario.muffling as i32;
        let mut last_lit = None;
        let mut recent: Vec<f64> = Vec::new();
        for row in scenario
            .ticks
            .iter()
            .filter(|row| row["n"].as_u64().unwrap() > 2 && !row["tile"].is_null())
        {
            let n = row["n"].as_u64().unwrap();
            // O evento do tick `e` vale a partir do tick seguinte (o comando roda no servidor, depois do tick do cliente que o pediu).
            if let Some((_, count)) = scenario.muffling_events.iter().find(|(at, _)| at + 1 == n) {
                server = *count as i32;
            }
            if let Some(sent) = sync.tick(server).send {
                client = sent;
            }
            let lit = row["tile"]["lit"].as_bool().unwrap();
            // O `lit` que muda leva o pacote de dados do bloco, com a contagem verdadeira.
            if last_lit.is_some_and(|last| last != lit)
                && let Some(sent) = sync.block_update()
            {
                client = sent;
            }
            last_lit = Some(lit);
            // O pacote leva de 1 a 3 ticks do servidor ao cliente (servidor e cliente do jogo único andam em threads): vale o fator de algum dos últimos três ticks.
            recent.push(factor_of(client));
            let seen = row["tile"]["fator"].as_f64().unwrap();
            assert!(
                recent
                    .iter()
                    .rev()
                    .take(3)
                    .any(|expected| (expected - seen).abs() < 1e-3),
                "{} n={n}: o cliente viu o fator {seen}, a regra dá {:?}",
                scenario.name,
                recent.iter().rev().take(3).collect::<Vec<_>>()
            );
            checked += 1;
        }
    }
    assert!(checked > 5000, "só {checked} ticks conferidos");
}

/// Cada som que o motor do original começou (`PlaySoundEvent`) vem de uma instância com os campos que o port põe na dele.
#[test]
fn the_sound_instance_has_the_fields_the_original_gives_it() {
    let mut seen = 0;
    let center = sound_position(BLOCK);
    for row in rows().iter().filter(|row| row["e"] == "play") {
        assert_eq!(row["categoria"], SOUND_SOURCE_BLOCKS_NAME);
        assert_eq!(row["pitch"].as_f64().unwrap() as f32, TILE_SOUND_PITCH);
        assert_eq!(row["atenuacao"], "LINEAR");
        assert_eq!(row["laco"].as_bool().unwrap(), TILE_SOUND_LOOPING);
        assert_eq!(row["atraso"].as_i64().unwrap() as i32, TILE_SOUND_DELAY);
        assert_eq!(row["relativo"], false);
        assert_eq!(
            row["comeca_mudo"].as_bool().unwrap(),
            TILE_SOUND_CAN_START_SILENT
        );
        assert_eq!(
            row["distancia_atenuacao"].as_i64().unwrap(),
            i64::from(DEFAULT_ATTENUATION_DISTANCE)
        );
        assert!(
            row["id"]
                .as_str()
                .unwrap()
                .starts_with("mekanism:tile.machine.")
        );
        for (axis, key) in ["x", "y", "z"].into_iter().enumerate() {
            assert_eq!(row[key].as_f64().unwrap(), center[axis], "{key}");
        }
        seen += 1;
    }
    assert!(seen > 20, "só {seen} sons começaram");
}
