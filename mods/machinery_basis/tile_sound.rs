// O som contínuo de um tile (H19): `TileEntityMekanism.updateSound` e `SoundHandler.TileTickableSound`. As regras estão em
// `machinery_basis_core::sound`; aqui só o que toca no jogo: o motor de som, a posição do jogador e o relógio.
use super::Game;
use horizon_client::sound_engine::{Attenuation, InstanceId, SoundEngine, SoundInstance, Sounds, Ticking};
use machinery_basis_core::sound::{
    DEFAULT_ATTENUATION_DISTANCE, SoundConfig, TileSound, TileSoundEngine, TileTickableSound, TileView, player_in_range,
    sound_position,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// `SoundSource.BLOCKS`, a categoria de `ITileSound.getSoundCategory`.
const BLOCKS: usize = 4;
/// `MekanismConfig.client.baseSoundVolume`: o padrão. O port ainda não tem o arquivo de configuração de cliente.
const BASE_VOLUME: f32 = 1.0;
/// Escreve no stderr cada início, parada e mudança de volume. Só as cópias de teste ligam.
const TRACE: bool = false;

/// O que o dono do tile diz dele a cada tick.
pub(in crate::game) struct SoundTile {
    pub key: (&'static str, [i32; 3]),
    /// `canPlaySound()`: `getActive()`, que no cliente é o estado `lit` do bloco.
    pub lit: bool,
    pub fully_muffled: bool,
    /// `getVolumeFactor()`.
    pub volume_factor: f32,
    /// `soundEvent.get()`, já com o filtro de feriado.
    pub sound: String,
}

struct Entry {
    tile: TileSound,
    playing: Option<(InstanceId, TileTickableSound)>,
    last_factor: f32,
}

/// Quantos upgrades de silêncio cada tile tem, como o cliente sabe: no servidor o `TileComponentUpgrade.saveToUpdateTag` os manda
/// (`MUFFLING_COUNT`) e o cliente guarda o número, que é só o que ele precisa. No anfitrião os dois lados são o mesmo processo e
/// dividem esta tabela.
static MUFFLING: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), i32>>> = std::sync::Mutex::new(None);

const MUFFLING_CHANNEL: &str = "machinery_basis:muffling";

fn muffling_table<R>(f: impl FnOnce(&mut HashMap<(String, [i32; 3]), i32>) -> R) -> R {
    let mut guard = MUFFLING.lock().unwrap_or_else(|error| error.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// Quantos upgrades de silêncio o cliente sabe que o tile tem.
pub(in crate::game) fn muffling_count(dimension: &str, pos: [i32; 3]) -> i32 {
    muffling_table(|table| table.get(&(dimension.to_owned(), pos)).copied().unwrap_or(0))
}

fn encode_muffling(dimension: &str, pos: [i32; 3], count: i32) -> Vec<u8> {
    let mut bytes = vec![dimension.len() as u8];
    bytes.extend_from_slice(dimension.as_bytes());
    for value in pos.into_iter().chain([count]) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}

fn decode_muffling(bytes: &[u8]) -> Option<(String, [i32; 3], i32)> {
    let (&length, rest) = bytes.split_first()?;
    let (dimension, rest) = rest.split_at_checked(usize::from(length))?;
    let dimension = std::str::from_utf8(dimension).ok()?.to_owned();
    let mut values = rest.chunks_exact(4).map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    let (x, y, z, count) = (values.next()?, values.next()?, values.next()?, values.next()?);
    Some((dimension, [x, y, z], count))
}

/// `NativeNetFromServer`: o cliente guarda a contagem que o servidor mandou.
pub(in crate::game) fn muffling_from_server(channel: &str, data: &[u8]) -> bool {
    if channel != MUFFLING_CHANNEL {
        return false;
    }
    if let Some((dimension, pos, count)) = decode_muffling(data) {
        muffling_table(|table| {
            if count == 0 {
                table.remove(&(dimension, pos));
            } else {
                table.insert((dimension, pos), count);
            }
        });
    }
    true
}


/// Os nomes que o cliente conhece de cada tile (`customName` do `Nameable`): o servidor os manda ao abrir a tela, como o título do
/// menu. No anfitrião os dois lados dividem esta tabela.
static NAMES: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), String>>> = std::sync::Mutex::new(None);

const NAME_CHANNEL: &str = "machinery_basis:name";

fn name_table<R>(f: impl FnOnce(&mut HashMap<(String, [i32; 3]), String>) -> R) -> R {
    let mut guard = NAMES.lock().unwrap_or_else(|error| error.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// O nome que o cliente sabe que o tile tem.
pub(in crate::game) fn tile_name(dimension: &str, pos: [i32; 3]) -> Option<String> {
    name_table(|table| table.get(&(dimension.to_owned(), pos)).cloned())
}

fn set_name(dimension: &str, pos: [i32; 3], name: Option<&str>) {
    name_table(|table| match name {
        Some(name) => {
            table.insert((dimension.to_owned(), pos), name.to_owned());
        }
        None => {
            table.remove(&(dimension.to_owned(), pos));
        }
    });
}

fn encode_name(dimension: &str, pos: [i32; 3], name: Option<&str>) -> Vec<u8> {
    let mut bytes = vec![dimension.len() as u8];
    bytes.extend_from_slice(dimension.as_bytes());
    for value in pos {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(name.unwrap_or_default().as_bytes());
    bytes
}

fn decode_name(bytes: &[u8]) -> Option<(String, [i32; 3], Option<String>)> {
    let (&length, rest) = bytes.split_first()?;
    let (dimension, rest) = rest.split_at_checked(usize::from(length))?;
    let (coordinates, name) = rest.split_at_checked(12)?;
    let mut values = coordinates.chunks_exact(4).map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    let pos = [values.next()?, values.next()?, values.next()?];
    let name = std::str::from_utf8(name).ok()?;
    Some((std::str::from_utf8(dimension).ok()?.to_owned(), pos, (!name.is_empty()).then(|| name.to_owned())))
}

/// `NativeNetFromServer`: o cliente guarda o nome que o servidor mandou.
pub(in crate::game) fn name_from_server(channel: &str, data: &[u8]) -> bool {
    if channel != NAME_CHANNEL {
        return false;
    }
    if let Some((dimension, pos, name)) = decode_name(data) {
        set_name(&dimension, pos, name.as_deref());
    }
    true
}

thread_local! {
    static SOUNDS: RefCell<HashMap<(&'static str, [i32; 3]), Entry>> = RefCell::new(HashMap::new());
}

/// `ThreadLocalRandom.nextInt(TICKS_PER_SECOND)`: o instante em que o som confere o silenciador.
fn random_second() -> i32 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.subsec_nanos());
    (nanos % 20) as i32
}

/// `SoundHandler.startTileSound` e `stopTileSound` de um tile.
struct Adapter<'a, 'g> {
    game: &'a mut Game<'g>,
    pos: [i32; 3],
    playing: &'a mut Option<(InstanceId, TileTickableSound)>,
    factor: f32,
}

impl TileSoundEngine for Adapter<'_, '_> {
    fn start(&mut self, sound: &str, initial_volume: f32) -> bool {
        // Já há um som ativo neste ponto: o Java devolve a instância dele.
        if let Some((id, _)) = self.playing.as_ref()
            && self.game.audio.is_active(*id)
        {
            return true;
        }
        let instance = TileTickableSound::new(initial_volume, BASE_VOLUME, self.factor, random_second());
        let center = sound_position(self.pos);
        // O jogador local não está no `Game` durante o tick do nível; o quadro mais recente dá a posição dele.
        let Some(player) = self.game.player_position() else {
            if TRACE {
                eprintln!("MEKSOUND no-player pos={:?}", self.pos);
            }
            return false;
        };
        if !player_in_range(player, center, instance.volume, DEFAULT_ATTENUATION_DISTANCE) {
            if TRACE {
                eprintln!("MEKSOUND out-of-range pos={:?} player={player:?}", self.pos);
            }
            return false;
        }
        let mut sound = SoundInstance::new(
            sound,
            BLOCKS,
            instance.volume,
            1.0,
            SoundEngine::unseeded(),
            true,
            0,
            Attenuation::Linear,
            center,
            false,
        );
        sound.ticking = Ticking::Native;
        sound.can_start_silent = true;
        let (id, _) = self.game.audio.play_instance(sound, true);
        if TRACE {
            eprintln!("MEKSOUND start pos={:?} id={id} volume={}", self.pos, instance.volume);
        }
        *self.playing = Some((id, instance));
        true
    }

    fn stop(&mut self) {
        if let Some((id, _)) = self.playing.take() {
            if TRACE {
                eprintln!("MEKSOUND stop pos={:?} id={id}", self.pos);
            }
            self.game.audio.stop_and_release(id);
        }
    }
}

impl Game<'_> {
    /// `TileComponentUpgrade.setUpgrades` / `removeUpgrade` com silêncio: o servidor avisa os jogadores da nova contagem
    /// (`sendUpdatePacket`).
    pub(in crate::game) fn mb_muffling_changed(&mut self, dimension: &str, pos: [i32; 3], count: i32) {
        muffling_table(|table| {
            if count == 0 {
                table.remove(&(dimension.to_owned(), pos));
            } else {
                table.insert((dimension.to_owned(), pos), count);
            }
        });
        let bytes = encode_muffling(dimension, pos, count);
        let players: Vec<u128> = self.native_server_players().iter().map(|player| player.uuid).collect();
        for player in players {
            let _ = self.native_net_send_to_player(player, MUFFLING_CHANNEL, &bytes);
        }
    }

    /// O jogador abriu o contêiner do tile: o servidor manda o nome dele, como o `getDisplayName` vai no pacote de abertura. Sem nome,
    /// manda vazio, para o cliente largar um nome velho.
    pub(in crate::game) fn mb_name_open(&mut self, player: u128, dimension: &str, pos: [i32; 3], name: Option<&str>) {
        set_name(dimension, pos, name);
        let _ = self.native_net_send_to_player(player, NAME_CHANNEL, &encode_name(dimension, pos, name));
    }

    /// Quem entrou depois recebe as contagens que o servidor tem.
    pub(in crate::game) fn mb_muffling_player_in(&mut self, player: u128) {
        let entries: Vec<(String, [i32; 3], i32)> =
            muffling_table(|table| table.iter().map(|((dimension, pos), count)| (dimension.clone(), *pos, *count)).collect());
        for (dimension, pos, count) in entries {
            let _ = self.native_net_send_to_player(player, MUFFLING_CHANNEL, &encode_muffling(&dimension, pos, count));
        }
    }

    /// `Minecraft.player.position()`: onde o jogador local está.
    fn player_position(&self) -> Option<[f64; 3]> {
        self.native_camera().map(|camera| camera.position)
    }

    /// Um tick do cliente para os tiles com som de uma dimensão. Um tile que já estava aqui e não vem mais foi removido.
    pub(in crate::game) fn mb_tile_sounds(&mut self, dimension: &str, tiles: &[SoundTile]) {
        let config = SoundConfig::default();
        let gone: Vec<(&'static str, [i32; 3])> = SOUNDS.with_borrow(|sounds| {
            sounds
                .keys()
                .filter(|(owner, pos)| *owner == dimension && !tiles.iter().any(|tile| tile.key == (*owner, *pos)))
                .copied()
                .collect()
        });
        for key in gone {
            let Some(mut entry) = SOUNDS.with_borrow_mut(|sounds| sounds.remove(&key)) else {
                continue;
            };
            // `setRemoved` chama `updateSound` uma vez: `isRemoved()` para o som.
            let mut adapter = Adapter { game: self, pos: key.1, playing: &mut entry.playing, factor: entry.last_factor };
            entry.tile.update(
                config,
                TileView { can_play: true, removed: true, fully_muffled: false, sound: "" },
                true,
                &mut adapter,
            );
        }
        let now = self.world.game_time;
        let normal = self.clock.runs_normally();
        if TRACE && !tiles.is_empty() && now % 100 == 0 {
            eprintln!("MEKSOUND tick now={now} tiles={} first_lit={} muffled={} factor={}", tiles.len(), tiles[0].lit, tiles[0].fully_muffled, tiles[0].volume_factor);
        }
        for tile in tiles {
            if tile.key.0 != dimension {
                continue;
            }
            let mut entry = SOUNDS
                .with_borrow_mut(|sounds| sounds.remove(&tile.key))
                .unwrap_or(Entry { tile: TileSound::default(), playing: None, last_factor: tile.volume_factor });
            entry.last_factor = tile.volume_factor;
            let engine_active = entry.playing.as_ref().is_some_and(|(id, _)| self.audio.is_active(*id));
            let view = TileView { can_play: tile.lit, removed: false, fully_muffled: tile.fully_muffled, sound: &tile.sound };
            let mut adapter = Adapter { game: self, pos: tile.key.1, playing: &mut entry.playing, factor: tile.volume_factor };
            entry.tile.update(config, view, engine_active, &mut adapter);
            // A instância em laço, no tick do motor de som.
            if let Some((id, instance)) = entry.playing.as_mut() {
                let player = self.player_position();
                let center = sound_position(tile.key.1);
                instance.tick(
                    Some(now),
                    normal,
                    |volume| player.is_some_and(|player| player_in_range(player, center, volume, DEFAULT_ATTENUATION_DISTANCE)),
                    tile.volume_factor,
                );
                if instance.is_stopped() {
                    let id = *id;
                    entry.playing = None;
                    if TRACE {
                        eprintln!("MEKSOUND ended pos={:?} id={id}", tile.key.1);
                    }
                    self.audio.stop_and_release(id);
                } else if let Some(playing) = self.audio.engine().instance_mut(*id) {
                    if TRACE && playing.volume != instance.volume {
                        eprintln!("MEKSOUND volume pos={:?} {} -> {}", tile.key.1, playing.volume, instance.volume);
                    }
                    playing.volume = instance.volume;
                }
            }
            SOUNDS.with_borrow_mut(|sounds| sounds.insert(tile.key, entry));
        }
    }
}

/// O dia de hoje, no fuso do jogador: `YearlyDate.now()` do feriado.
pub(in crate::game) fn today() -> (u32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now();
    (now.month(), now.day())
}
