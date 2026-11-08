//! O som contínuo de uma máquina: `TileEntityMekanism.updateSound` (o tile, no cliente) e o `SoundHandler.TileTickableSound` (a
//! instância que toca). Sem tipos do jogo: o que o jogo sabe entra por parâmetro, e o que a regra decide sai como comando.

/// `SharedConstants.TICKS_PER_SECOND`.
pub const TICKS_PER_SECOND: i32 = 20;
/// `Sound.getAttenuationDistance` quando o `sounds.json` não diz outra coisa: o `attenuation_distance` padrão.
pub const DEFAULT_ATTENUATION_DISTANCE: i32 = 16;
/// `ITileSound.getInitialVolume`.
pub const INITIAL_VOLUME: f32 = 1.0;
/// `SoundSource.BLOCKS`, a categoria de `ITileSound.getSoundCategory`, pelo índice do enum (`MASTER`, `MUSIC`, `RECORDS`, `WEATHER`, `BLOCKS`...).
pub const SOUND_SOURCE_BLOCKS: usize = 4;
/// O nome do `SoundSource.BLOCKS` (`getName`).
pub const SOUND_SOURCE_BLOCKS_NAME: &str = "block";
/// O que o `TileTickableSound` fixa na instância: o tom (o `pitch` de `AbstractSoundInstance`), o atraso (`delay = 0`), o laço (`looping = true`, o padrão de
/// `startTileSound`) e `canStartSilent`.
pub const TILE_SOUND_PITCH: f32 = 1.0;
pub const TILE_SOUND_DELAY: i32 = 0;
pub const TILE_SOUND_LOOPING: bool = true;
pub const TILE_SOUND_CAN_START_SILENT: bool = true;

/// `1 - min(mufflerCount, max) / max`, o `getVolumeFactor` do tile. Sem o upgrade (`max` `None`), 1.
pub fn volume_factor(muffling: Option<(i32, i32)>) -> f32 {
    match muffling {
        Some((count, max)) => 1.0 - (count.min(max) as f32 / max as f32),
        None => 1.0,
    }
}

/// `isFullyMuffled`: o upgrade existe e está no máximo.
pub fn fully_muffled(muffling: Option<(i32, i32)>) -> bool {
    muffling.is_some_and(|(count, max)| count >= max)
}

/// `SoundHandler.isClientPlayerInRange`: a distância de atenuação cresce com o volume acima de 1, e vale a distância ao quadrado
/// entre o jogador e o centro do bloco.
pub fn player_in_range(
    player: [f64; 3],
    sound: [f64; 3],
    volume: f32,
    attenuation_distance: i32,
) -> bool {
    let scaled = volume.max(1.0) * attenuation_distance as f32;
    let distance_squared = (player[0] - sound[0]).powi(2)
        + (player[1] - sound[1]).powi(2)
        + (player[2] - sound[2]).powi(2);
    distance_squared < f64::from(scaled * scaled)
}

/// O centro do bloco, onde o som toca (`pos.getX() + 0.5F`).
pub fn sound_position(pos: [i32; 3]) -> [f64; 3] {
    [
        f64::from(pos[0] as f32 + 0.5),
        f64::from(pos[1] as f32 + 0.5),
        f64::from(pos[2] as f32 + 0.5),
    ]
}

/// `HolidayManager.getSoundEventSupplier`: no dia do feriado e com `holidays` ligado, o som do tile vira o do feriado, se a tabela
/// tem um para ele. `today` e `holiday` são (mês, dia).
pub fn holiday_sound<'a>(
    sound: &'a str,
    enabled: bool,
    today: (u32, u32),
    holiday: (u32, u32),
    table: &[(&str, &'a str)],
) -> &'a str {
    if enabled && today == holiday {
        table
            .iter()
            .find(|(original, _)| *original == sound)
            .map_or(sound, |(_, replacement)| replacement)
    } else {
        sound
    }
}

/// O que o servidor e o cliente sabem de quantos upgrades de silêncio um tile tem. O original só avisa o cliente quando um upgrade ENTRA
/// (`TileComponentUpgrade.setUpgrades` chama `sendUpdatePacket`; `removeUpgrade`, `TileComponentUpgrade.java:167-189`, não): o cliente segue com a
/// contagem velha, e o som que o upgrade calou não volta, até um pacote do bloco levar a contagem verdadeira (o `lit` que muda, via
/// `ChunkHolder.broadcastBlockEntityIfNeeded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MufflingSync {
    server: i32,
    sent: i32,
}

/// O que um tick do servidor decide: a contagem verdadeira nova (para quem entrar depois) e a que vai ao cliente agora.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MufflingStep {
    pub server: Option<i32>,
    pub send: Option<i32>,
}

impl Default for MufflingSync {
    /// Nada ainda: a primeira contagem vista vai ao cliente, como o pacote de dados que ele recebe ao ver o tile.
    fn default() -> Self {
        Self {
            server: -1,
            sent: -1,
        }
    }
}

impl MufflingSync {
    /// A contagem do servidor neste tick. Só sobe avisa (o primeiro valor conta como subida).
    pub fn tick(&mut self, count: i32) -> MufflingStep {
        let mut step = MufflingStep::default();
        if count != self.server {
            self.server = count;
            step.server = Some(count);
        }
        if count > self.sent {
            self.sent = count;
            step.send = Some(count);
        }
        step
    }

    /// O bloco mandou um pacote de dados (o estado dele mudou): o cliente fica com a contagem verdadeira.
    pub fn block_update(&mut self) -> Option<i32> {
        if self.server >= 0 && self.server != self.sent {
            self.sent = self.server;
            return Some(self.server);
        }
        None
    }

    /// A contagem que o cliente sabe (`-1` antes da primeira).
    pub fn sent(&self) -> i32 {
        self.sent
    }
}

/// A configuração de cliente que o som lê: `enableMachineSounds` e `baseSoundVolume`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundConfig {
    pub enabled: bool,
    pub base_volume: f32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_volume: 1.0,
        }
    }
}

/// O que o jogo diz do tile no cliente, a cada tick.
#[derive(Clone, Copy, Debug)]
pub struct TileView<'a> {
    /// `canPlaySound()`, que é `getActive()`.
    pub can_play: bool,
    /// `isRemoved()`.
    pub removed: bool,
    pub fully_muffled: bool,
    /// `soundEvent.get()`, já com o filtro de feriado.
    pub sound: &'a str,
}

/// O jogo, do ponto de vista do tile: `SoundHandler.startTileSound` e `stopTileSound`.
pub trait TileSoundEngine {
    /// Toca `sound` em laço no bloco, se o jogador estiver ao alcance (`isClientPlayerInRange`). Diz se tocou ou se já havia um
    /// som ativo ali, que é quando o Java devolve a instância e não `null`.
    fn start(&mut self, sound: &str, initial_volume: f32) -> bool;
    fn stop(&mut self);
}

/// `activeSound`, `lastSoundEvent` e `playSoundCooldown` do tile.
#[derive(Clone, Debug, Default)]
pub struct TileSound {
    /// Tem um `activeSound` (que pode ter parado: ver `still_active`).
    active: bool,
    last_sound: Option<String>,
    play_sound_cooldown: i32,
}

impl TileSound {
    pub fn has_active_sound(&self) -> bool {
        self.active
    }

    pub fn cooldown(&self) -> i32 {
        self.play_sound_cooldown
    }

    /// `updateSound`, uma vez por tick no cliente (e uma vez ao remover o tile). `engine_active` é
    /// `SoundManager.isActive(activeSound)`.
    pub fn update(
        &mut self,
        config: SoundConfig,
        view: TileView<'_>,
        engine_active: bool,
        engine: &mut dyn TileSoundEngine,
    ) {
        if !config.enabled {
            return;
        }
        if view.can_play && !view.removed {
            self.play_sound_cooldown -= 1;
            if self.play_sound_cooldown > 0 {
                return;
            }
            if self.last_sound.as_deref() != Some(view.sound) {
                if self.active {
                    // O som mudou: para, para tocar o novo.
                    engine.stop();
                    self.active = false;
                }
                self.last_sound = Some(view.sound.to_owned());
            }
            if !view.fully_muffled && (!self.active || !engine_active) {
                self.active = engine.start(view.sound, INITIAL_VOLUME);
            }
            self.play_sound_cooldown = TICKS_PER_SECOND;
        } else if self.active {
            engine.stop();
            self.active = false;
            self.play_sound_cooldown = 0;
        }
    }
}

/// `SoundHandler.TileTickableSound`: a instância em laço presa ao bloco.
#[derive(Clone, Debug)]
pub struct TileTickableSound {
    original_volume: f32,
    /// Entre 20 e 39 ticks, sorteado por instância.
    check_interval: i32,
    pub volume: f32,
    stopped: bool,
}

impl TileTickableSound {
    /// O construtor: o volume original é o inicial vezes o `baseVolume`, e o atual já vem com o fator do tile.
    pub fn new(
        initial_volume: f32,
        base_volume: f32,
        tile_factor: f32,
        random_second: i32,
    ) -> Self {
        let original_volume = initial_volume * base_volume;
        Self {
            original_volume,
            check_interval: TICKS_PER_SECOND + random_second,
            volume: original_volume * tile_factor,
            stopped: false,
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn check_interval(&self) -> i32 {
        self.check_interval
    }

    pub fn original_volume(&self) -> f32 {
        self.original_volume
    }

    /// `tick()`. `level` é o relógio do mundo (`None` sem mundo), `ticking_normally` é `isTickingNormally` (falso com o tempo
    /// congelado), `in_range` responde `isClientPlayerInRange(this)` com o volume atual, e `tile_factor` é
    /// `getTileVolumeFactor()`: o `getVolumeFactor()` vezes o `getVolume()` do tile.
    pub fn tick(
        &mut self,
        game_time: Option<i64>,
        ticking_normally: bool,
        in_range: impl Fn(f32) -> bool,
        tile_factor: f32,
    ) {
        let Some(game_time) = game_time else {
            self.stopped = true;
            return;
        };
        if !ticking_normally {
            // Como o som do carrinho de mina: mudo.
            self.volume = 0.0;
        } else if game_time % i64::from(self.check_interval) == 0 {
            if !in_range(self.volume) {
                self.stopped = true;
                return;
            }
            // Nenhum outro mod filtra o som: `ClientHooks.playSound` devolve a própria instância.
            self.volume = self.original_volume * tile_factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: SoundConfig = SoundConfig {
        enabled: true,
        base_volume: 1.0,
    };

    /// O jogo falso: guarda o que o tile pediu.
    #[derive(Default)]
    struct Log {
        started: Vec<(String, f32)>,
        stopped: i32,
        /// O que `start` responde: o jogador estava ao alcance?
        in_range: bool,
    }

    impl Log {
        fn near() -> Self {
            Self {
                in_range: true,
                ..Self::default()
            }
        }
    }

    impl TileSoundEngine for Log {
        fn start(&mut self, sound: &str, initial_volume: f32) -> bool {
            self.started.push((sound.to_owned(), initial_volume));
            self.in_range
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
            sound: "mekanism:tile.machine.crusher",
        }
    }

    #[test]
    fn the_volume_factor_falls_with_each_muffler_up_to_the_maximum() {
        assert_eq!(volume_factor(None), 1.0);
        assert_eq!(volume_factor(Some((0, 1))), 1.0);
        assert_eq!(volume_factor(Some((1, 1))), 0.0);
        assert_eq!(volume_factor(Some((5, 1))), 0.0);
        assert_eq!(volume_factor(Some((2, 4))), 0.5);
        assert_eq!(volume_factor(Some((1, 4))), 0.75);
    }

    #[test]
    fn only_the_maximum_fully_muffles() {
        assert!(!fully_muffled(None));
        assert!(!fully_muffled(Some((0, 1))));
        assert!(fully_muffled(Some((1, 1))));
        assert!(!fully_muffled(Some((3, 4))));
        assert!(fully_muffled(Some((4, 4))));
    }

    #[test]
    fn the_range_scales_with_the_volume_above_one() {
        let sound = sound_position([0, 0, 0]);
        assert!(player_in_range([15.0, 0.5, 0.5], sound, 1.0, 16));
        assert!(!player_in_range([16.5, 0.5, 0.5], sound, 1.0, 16));
        // Abaixo de 1 o volume não encolhe o alcance.
        assert!(!player_in_range([16.5, 0.5, 0.5], sound, 0.1, 16));
        assert!(player_in_range([20.0, 0.5, 0.5], sound, 2.0, 16));
        // O alcance nunca encolhe abaixo de 16 blocos: com volume 0,5 a 10 blocos ainda está dentro.
        assert!(player_in_range([10.5, 0.5, 0.5], sound, 0.5, 16));
        // O limite é estrito.
        assert!(!player_in_range([16.5, 0.5, 0.5], sound, 1.0, 16));
    }

    #[test]
    fn a_machine_that_turns_on_starts_its_sound_at_once_and_one_that_stops_ends_it() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        tile.update(ON, view(true), false, &mut game);
        assert_eq!(
            game.started,
            vec![("mekanism:tile.machine.crusher".to_owned(), 1.0)]
        );
        assert_eq!(tile.cooldown(), 20);
        // Parou: o laço acaba e o intervalo zera.
        tile.update(ON, view(false), true, &mut game);
        assert_eq!(game.stopped, 1);
        assert_eq!(tile.cooldown(), 0);
        assert!(!tile.has_active_sound());
        // Ligar de novo toca de novo, sem esperar o intervalo.
        tile.update(ON, view(true), false, &mut game);
        assert_eq!(game.started.len(), 2);
    }

    #[test]
    fn a_running_sound_is_checked_once_a_second() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        tile.update(ON, view(true), false, &mut game);
        for _ in 0..19 {
            tile.update(ON, view(true), true, &mut game);
        }
        assert_eq!(game.started.len(), 1);
        // No vigésimo tick o som continua ativo: nada a fazer, e o intervalo recomeça.
        tile.update(ON, view(true), true, &mut game);
        assert_eq!(game.started.len(), 1);
        assert_eq!(tile.cooldown(), 20);
    }

    #[test]
    fn a_sound_that_ended_by_itself_is_started_again_at_the_next_check() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        for _ in 0..21 {
            tile.update(ON, view(true), false, &mut game);
        }
        assert_eq!(game.started.len(), 2);
    }

    #[test]
    fn a_player_out_of_range_starts_nothing_and_the_check_runs_again_a_second_later() {
        let mut tile = TileSound::default();
        let mut game = Log::default();
        tile.update(ON, view(true), false, &mut game);
        assert_eq!(game.started.len(), 1, "o jogo foi chamado e não tocou");
        assert!(!tile.has_active_sound());
        assert_eq!(tile.cooldown(), 20);
        for _ in 0..20 {
            tile.update(ON, view(true), false, &mut game);
        }
        assert_eq!(game.started.len(), 2);
    }

    #[test]
    fn a_fully_muffled_machine_never_starts_but_still_waits_a_second() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        let mut muffled = view(true);
        muffled.fully_muffled = true;
        tile.update(ON, muffled, false, &mut game);
        assert!(game.started.is_empty());
        assert_eq!(tile.cooldown(), 20);
    }

    #[test]
    fn muffling_a_running_sound_leaves_it_to_the_tickable_which_goes_silent() {
        // O tile só deixa de iniciar; quem baixa o volume é a instância no seu próximo ciclo.
        let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 7);
        assert_eq!(sound.check_interval(), 27);
        sound.tick(Some(27), true, |_| true, 0.0);
        assert_eq!(sound.volume, 0.0);
        assert!(!sound.is_stopped());
    }

    #[test]
    fn the_first_count_always_reaches_the_client_and_only_an_increase_after_it() {
        let mut sync = MufflingSync::default();
        assert_eq!(
            sync.tick(0),
            MufflingStep {
                server: Some(0),
                send: Some(0)
            }
        );
        assert_eq!(sync.tick(0), MufflingStep::default());
        assert_eq!(
            sync.tick(1),
            MufflingStep {
                server: Some(1),
                send: Some(1)
            }
        );
        // Tirar o upgrade: o servidor sabe, o cliente não é avisado.
        assert_eq!(
            sync.tick(0),
            MufflingStep {
                server: Some(0),
                send: None
            }
        );
        assert_eq!(sync.sent(), 1);
        // Pôr de novo: sobe para 1, e o cliente já sabia 1.
        assert_eq!(
            sync.tick(1),
            MufflingStep {
                server: Some(1),
                send: None
            }
        );
    }

    #[test]
    fn a_block_update_brings_the_true_count_to_the_client() {
        let mut sync = MufflingSync::default();
        assert_eq!(sync.block_update(), None, "nada visto ainda");
        sync.tick(1);
        sync.tick(0);
        assert_eq!(sync.block_update(), Some(0));
        assert_eq!(sync.block_update(), None, "o cliente já sabe");
        assert_eq!(sync.sent(), 0);
    }

    #[test]
    fn the_holiday_replaces_only_the_sounds_in_its_table() {
        let table = [("a:crusher", "a:christmas.5")];
        assert_eq!(
            holiday_sound("a:crusher", true, (12, 25), (12, 25), &table),
            "a:christmas.5"
        );
        assert_eq!(
            holiday_sound("a:other", true, (12, 25), (12, 25), &table),
            "a:other"
        );
        assert_eq!(
            holiday_sound("a:crusher", true, (12, 24), (12, 25), &table),
            "a:crusher"
        );
        assert_eq!(
            holiday_sound("a:crusher", false, (12, 25), (12, 25), &table),
            "a:crusher"
        );
    }

    #[test]
    fn disabled_machine_sounds_do_nothing_at_all() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        let off = SoundConfig {
            enabled: false,
            base_volume: 1.0,
        };
        tile.update(off, view(true), false, &mut game);
        assert!(game.started.is_empty());
        assert_eq!(tile.cooldown(), 0);
    }

    #[test]
    fn a_removed_tile_stops_its_sound() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        tile.update(ON, view(true), false, &mut game);
        let mut gone = view(true);
        gone.removed = true;
        tile.update(ON, gone, true, &mut game);
        assert_eq!(game.stopped, 1);
    }

    #[test]
    fn a_changed_sound_event_restarts_the_loop() {
        let mut tile = TileSound::default();
        let mut game = Log::near();
        tile.update(ON, view(true), false, &mut game);
        for _ in 0..19 {
            tile.update(ON, view(true), true, &mut game);
        }
        let mut other = view(true);
        other.sound = "mekanism:tile.christmas.5";
        tile.update(ON, other, true, &mut game);
        assert_eq!(game.stopped, 1);
        assert_eq!(game.started.len(), 2);
        assert_eq!(game.started[1].0, "mekanism:tile.christmas.5");
    }

    #[test]
    fn the_instance_starts_at_the_base_volume_times_the_tile_factor() {
        let sound = TileTickableSound::new(1.0, 0.5, 0.0, 0);
        assert_eq!(sound.original_volume(), 0.5);
        assert_eq!(sound.volume, 0.0);
        let sound = TileTickableSound::new(1.0, 2.0, 1.0, 0);
        assert_eq!(sound.volume, 2.0);
    }

    #[test]
    fn the_instance_follows_the_muffling_only_at_its_check() {
        let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 4);
        assert_eq!(sound.check_interval(), 24);
        sound.tick(Some(10), true, |_| true, 0.0);
        assert_eq!(sound.volume, 1.0, "fora do ciclo o volume não muda");
        sound.tick(Some(48), true, |_| true, 0.0);
        assert_eq!(sound.volume, 0.0);
        sound.tick(Some(72), true, |_| true, 1.0);
        assert_eq!(sound.volume, 1.0, "tirar o upgrade volta o volume");
    }

    #[test]
    fn a_frozen_world_mutes_and_the_volume_comes_back_only_at_a_check() {
        let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 0);
        sound.tick(Some(5), false, |_| true, 1.0);
        assert_eq!(sound.volume, 0.0);
        sound.tick(Some(6), true, |_| true, 1.0);
        assert_eq!(sound.volume, 0.0);
        sound.tick(Some(20), true, |_| true, 1.0);
        assert_eq!(sound.volume, 1.0);
    }

    #[test]
    fn a_player_who_walked_away_ends_the_loop_at_a_check() {
        let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 0);
        sound.tick(Some(7), true, |_| false, 1.0);
        assert!(!sound.is_stopped(), "fora do ciclo não olha a distância");
        sound.tick(Some(20), true, |_| false, 1.0);
        assert!(sound.is_stopped());
    }

    #[test]
    fn no_world_ends_the_loop() {
        let mut sound = TileTickableSound::new(1.0, 1.0, 1.0, 0);
        sound.tick(None, true, |_| true, 1.0);
        assert!(sound.is_stopped());
    }
}
