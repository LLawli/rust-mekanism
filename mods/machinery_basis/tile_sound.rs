// O som contínuo de um tile (H19): `TileEntityMekanism.updateSound` e `SoundHandler.TileTickableSound`. As regras estão em
// `machinery_basis_core::sound`; aqui só o que toca no jogo: o motor de som, a posição do jogador e o relógio.
use super::Game;
use horizon_world::nbt::{Limits, Tag, read_network, write_network};
use horizon_client::sound_engine::{Attenuation, InstanceId, SoundEngine, SoundInstance, Sounds, Ticking};
use machinery_basis_core::sound::{
    DEFAULT_ATTENUATION_DISTANCE, SOUND_SOURCE_BLOCKS, SoundConfig, TILE_SOUND_CAN_START_SILENT, TILE_SOUND_DELAY, TILE_SOUND_LOOPING, TILE_SOUND_PITCH, TileSound, TileSoundEngine, TileTickableSound, TileView, player_in_range,
    sound_position,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
    playing: Option<Playing>,
    last_factor: f32,
    /// Só para o rastro: o último `lit` e `fully_muffled` impressos.
    seen: Option<(bool, bool)>,
}

/// Quantos upgrades de silêncio cada tile tem, como o cliente sabe: no servidor o `TileComponentUpgrade.saveToUpdateTag` os manda
/// (`MUFFLING_COUNT`) e o cliente guarda o número, que é só o que ele precisa. No anfitrião os dois lados são o mesmo processo e
/// dividem esta tabela.
static MUFFLING: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), i32>>> = std::sync::Mutex::new(None);

/// Quantos upgrades de silêncio o servidor sabe que cada tile tem. Difere da tabela acima quando um upgrade sai: o original não avisa o cliente
/// (`TileComponentUpgrade.removeUpgrade` não chama `sendUpdatePacket`, só o `setUpgrades` de quem instala chama), então o cliente guarda a contagem
/// velha até o próximo pacote do bloco (o `lit` que muda, a troca de chunk). Quem entra no servidor recebe esta.
static MUFFLING_SERVER: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), i32>>> = std::sync::Mutex::new(None);

const MUFFLING_CHANNEL: &str = "machinery_basis:muffling";

/// O que o desenho de um tile precisa saber e só o servidor sabe (`getUpdateTag`/`ModelData` do Java): um número de 0 a 1 (a escala do núcleo do Energy Cube) e um valor por lado
/// (o `CubeSideState`). O servidor manda a cada mudança e a quem entra; no anfitrião os dois lados dividem a tabela.
static VIEWS: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), (f32, [u8; 6])>>> = std::sync::Mutex::new(None);
static VIEWS_SERVER: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), (f32, [u8; 6])>>> = std::sync::Mutex::new(None);

const VIEW_CHANNEL: &str = "machinery_basis:view";

fn view_table<R>(f: impl FnOnce(&mut HashMap<(String, [i32; 3]), (f32, [u8; 6])>) -> R) -> R {
    let mut guard = VIEWS.lock().unwrap_or_else(|error| error.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// O que o cliente sabe do desenho do tile em `pos`.
pub(in crate::game) fn tile_view(dimension: &str, pos: [i32; 3]) -> Option<(f32, [u8; 6])> {
    view_table(|table| table.get(&(dimension.to_owned(), pos)).copied())
}

fn encode_view(dimension: &str, pos: [i32; 3], view: (f32, [u8; 6])) -> Vec<u8> {
    let mut bytes = vec![dimension.len() as u8];
    bytes.extend_from_slice(dimension.as_bytes());
    for value in pos {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&view.0.to_be_bytes());
    bytes.extend_from_slice(&view.1);
    bytes
}

fn decode_view(bytes: &[u8]) -> Option<(String, [i32; 3], (f32, [u8; 6]))> {
    let (&length, rest) = bytes.split_first()?;
    let (dimension, rest) = rest.split_at_checked(usize::from(length))?;
    let dimension = std::str::from_utf8(dimension).ok()?.to_owned();
    let (position, rest) = rest.split_at_checked(12)?;
    let mut values = position.chunks_exact(4).map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    let pos = [values.next()?, values.next()?, values.next()?];
    let (scale, sides) = rest.split_at_checked(4)?;
    let scale = f32::from_be_bytes([scale[0], scale[1], scale[2], scale[3]]);
    let sides: [u8; 6] = sides.try_into().ok()?;
    Some((dimension, pos, (scale, sides)))
}

/// `NativeNetFromServer`: o cliente guarda o desenho que o servidor mandou.
pub(in crate::game) fn view_from_server(channel: &str, data: &[u8]) -> bool {
    if channel != VIEW_CHANNEL {
        return false;
    }
    if let Some((dimension, pos, view)) = decode_view(data) {
        view_table(|table| table.insert((dimension, pos), view));
    }
    true
}

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
/// menu. O nome é o componente inteiro em NBT (texto, cor, negrito, filhos), não o texto: o título da tela o desenha com o estilo.
/// No anfitrião os dois lados dividem esta tabela.
static NAMES: std::sync::Mutex<Option<HashMap<(String, [i32; 3]), Tag>>> = std::sync::Mutex::new(None);

const NAME_CHANNEL: &str = "machinery_basis:name";

fn name_table<R>(f: impl FnOnce(&mut HashMap<(String, [i32; 3]), Tag>) -> R) -> R {
    let mut guard = NAMES.lock().unwrap_or_else(|error| error.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// O nome que o cliente sabe que o tile tem.
pub(in crate::game) fn tile_name(dimension: &str, pos: [i32; 3]) -> Option<Tag> {
    name_table(|table| table.get(&(dimension.to_owned(), pos)).cloned())
}

fn set_name(dimension: &str, pos: [i32; 3], name: Option<&Tag>) {
    name_table(|table| match name {
        Some(name) => {
            table.insert((dimension.to_owned(), pos), name.clone());
        }
        None => {
            table.remove(&(dimension.to_owned(), pos));
        }
    });
}

fn encode_name(dimension: &str, pos: [i32; 3], name: Option<&Tag>) -> Vec<u8> {
    let mut bytes = vec![dimension.len() as u8];
    bytes.extend_from_slice(dimension.as_bytes());
    for value in pos {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    if let Some(name) = name
        && let Ok(encoded) = write_network(name, Limits::default())
    {
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

fn decode_name(bytes: &[u8]) -> Option<(String, [i32; 3], Option<Tag>)> {
    let (&length, rest) = bytes.split_first()?;
    let (dimension, rest) = rest.split_at_checked(usize::from(length))?;
    let (coordinates, name) = rest.split_at_checked(12)?;
    let mut values = coordinates.chunks_exact(4).map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    let pos = [values.next()?, values.next()?, values.next()?];
    let name = if name.is_empty() { None } else { read_network(name, Limits::default()).ok().map(|(tag, _)| tag) };
    Some((std::str::from_utf8(dimension).ok()?.to_owned(), pos, name))
}

/// `NativeNetFromServer`: o cliente guarda o nome que o servidor mandou.
pub(in crate::game) fn name_from_server(channel: &str, data: &[u8]) -> bool {
    if channel != NAME_CHANNEL {
        return false;
    }
    if let Some((dimension, pos, name)) = decode_name(data) {
        set_name(&dimension, pos, name.as_ref());
    }
    true
}

type Playing = (InstanceId, TileTickableSound);

thread_local! {
    static SOUNDS: RefCell<HashMap<(&'static str, [i32; 3]), Entry>> = RefCell::new(HashMap::new());
    /// Os sons que ficaram tocando depois de o tile sair (`SoundHandler.soundMap`): com `enableMachineSounds` desligado o `updateSound` do tile que sai
    /// volta antes do `stopTileSound`, e a instância segue sendo tickada pelo motor sem dono (fator 1) até o jogador sair do alcance. Um tile novo no mesmo
    /// bloco a encontra em `startTileSound` e a usa.
    static ORPHANS: RefCell<HashMap<(&'static str, [i32; 3]), Playing>> = RefCell::new(HashMap::new());
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
    playing: &'a mut Option<Playing>,
    factor: f32,
    /// `MekanismConfig.client.baseSoundVolume`, lido quando o som nasce (o construtor do `TileTickableSound`).
    base_volume: f32,
}

impl TileSoundEngine for Adapter<'_, '_> {
    fn start(&mut self, sound: &str, initial_volume: f32) -> bool {
        // Já há um som ativo neste ponto: o Java devolve a instância dele.
        if let Some((id, _)) = self.playing.as_ref()
            && self.game.audio.is_active(*id)
        {
            return true;
        }
        let instance = TileTickableSound::new(initial_volume, self.base_volume, self.factor, random_second());
        let center = sound_position(self.pos);
        // `Minecraft.getInstance().player.position()`: os pés do jogador local, que só existem no tick do cliente.
        let Some(player) = self.game.player_position() else {
            if TRACE {
                eprintln!("MEKSOUND no-player t={} pos={:?}", self.game.world.game_time, self.pos);
            }
            return false;
        };
        if !player_in_range(player, center, instance.volume, DEFAULT_ATTENUATION_DISTANCE) {
            if TRACE {
                eprintln!("MEKSOUND out-of-range t={} pos={:?} player={player:?}", self.game.world.game_time, self.pos);
            }
            return false;
        }
        let mut sound = SoundInstance::new(
            sound,
            SOUND_SOURCE_BLOCKS,
            instance.volume,
            TILE_SOUND_PITCH,
            SoundEngine::unseeded(),
            TILE_SOUND_LOOPING,
            TILE_SOUND_DELAY,
            Attenuation::Linear,
            center,
            false,
        );
        sound.ticking = Ticking::Native;
        sound.can_start_silent = TILE_SOUND_CAN_START_SILENT;
        let described = TRACE.then(|| {
            format!(
                "event={} source={} volume={} pitch={} looping={} delay={} attenuation={:?} relative={} silent_start={} at={:?}",
                sound.identifier,
                sound.source,
                sound.volume,
                sound.pitch,
                sound.looping,
                sound.delay,
                sound.attenuation,
                sound.relative,
                sound.can_start_silent,
                sound.position
            )
        });
        let (id, _) = self.game.audio.play_instance(sound, true);
        if let Some(described) = described {
            eprintln!("MEKSOUND start t={} pos={:?} id={id} {described}", self.game.world.game_time, self.pos);
        }
        *self.playing = Some((id, instance));
        true
    }

    fn stop(&mut self) {
        if let Some((id, _)) = self.playing.take() {
            if TRACE {
                eprintln!("MEKSOUND stop t={} pos={:?} id={id}", self.game.world.game_time, self.pos);
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
    pub(in crate::game) fn mb_name_open(&mut self, player: u128, dimension: &str, pos: [i32; 3], name: Option<&Tag>) {
        set_name(dimension, pos, name);
        let _ = self.native_net_send_to_player(player, NAME_CHANNEL, &encode_name(dimension, pos, name));
    }

    /// A contagem que o servidor tem agora, sem avisar o cliente (veja `MUFFLING_SERVER`).
    pub(in crate::game) fn mb_muffling_server(&mut self, dimension: &str, pos: [i32; 3], count: i32) {
        let mut guard = MUFFLING_SERVER.lock().unwrap_or_else(|error| error.into_inner());
        let table = guard.get_or_insert_with(HashMap::new);
        if count == 0 {
            table.remove(&(dimension.to_owned(), pos));
        } else {
            table.insert((dimension.to_owned(), pos), count);
        }
    }

    /// O tile em `pos` mudou o que o desenho precisa (a escala e os lados): o servidor avisa os jogadores.
    pub(in crate::game) fn mb_view_changed(&mut self, dimension: &str, pos: [i32; 3], scale: f32, sides: [u8; 6]) {
        view_table(|table| table.insert((dimension.to_owned(), pos), (scale, sides)));
        VIEWS_SERVER
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_or_insert_with(HashMap::new)
            .insert((dimension.to_owned(), pos), (scale, sides));
        let bytes = encode_view(dimension, pos, (scale, sides));
        let players: Vec<u128> = self.native_server_players().iter().map(|player| player.uuid).collect();
        for player in players {
            let _ = self.native_net_send_to_player(player, VIEW_CHANNEL, &bytes);
        }
    }

    /// Quem entrou depois recebe o desenho que o servidor tem.
    pub(in crate::game) fn mb_view_player_in(&mut self, player: u128) {
        let entries: Vec<(String, [i32; 3], (f32, [u8; 6]))> = {
            let mut guard = VIEWS_SERVER.lock().unwrap_or_else(|error| error.into_inner());
            guard.get_or_insert_with(HashMap::new).iter().map(|((dimension, pos), view)| (dimension.clone(), *pos, *view)).collect()
        };
        for (dimension, pos, view) in entries {
            let _ = self.native_net_send_to_player(player, VIEW_CHANNEL, &encode_view(&dimension, pos, view));
        }
    }

    /// Quem entrou depois recebe as contagens que o servidor tem.
    pub(in crate::game) fn mb_muffling_player_in(&mut self, player: u128) {
        let entries: Vec<(String, [i32; 3], i32)> = {
            let mut guard = MUFFLING_SERVER.lock().unwrap_or_else(|error| error.into_inner());
            guard.get_or_insert_with(HashMap::new).iter().map(|((dimension, pos), count)| (dimension.clone(), *pos, *count)).collect()
        };
        for (dimension, pos, count) in entries {
            let _ = self.native_net_send_to_player(player, MUFFLING_CHANNEL, &encode_muffling(&dimension, pos, count));
        }
    }

    /// `Minecraft.player.position()`: os pés do jogador local (não a câmera, que fica na altura dos olhos e, em terceira pessoa, atrás
    /// dele). Falta fora do tick do cliente com um mundo aberto.
    fn player_position(&self) -> Option<[f64; 3]> {
        self.native_player().map(|player| [player.x, player.y, player.z])
    }

    /// Um tick do cliente para os tiles com som de uma dimensão. Um tile que já estava aqui e não vem mais foi removido.
    pub(in crate::game) fn mb_tile_sounds(&mut self, dimension: &str, tiles: &[SoundTile], config: SoundConfig) {
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
            let mut adapter =
                Adapter { game: self, pos: key.1, playing: &mut entry.playing, factor: entry.last_factor, base_volume: config.base_volume };
            entry.tile.update(
                config,
                TileView { can_play: true, removed: true, fully_muffled: false, sound: "" },
                true,
                &mut adapter,
            );
            if let Some(orphan) = entry.playing.take() {
                ORPHANS.with_borrow_mut(|orphans| orphans.insert(key, orphan));
            }
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
            let mut entry = SOUNDS.with_borrow_mut(|sounds| sounds.remove(&tile.key)).unwrap_or_else(|| Entry {
                tile: TileSound::default(),
                playing: ORPHANS.with_borrow_mut(|orphans| orphans.remove(&tile.key)),
                last_factor: tile.volume_factor,
                seen: None,
            });
            entry.last_factor = tile.volume_factor;
            if TRACE && entry.seen != Some((tile.lit, tile.fully_muffled)) {
                entry.seen = Some((tile.lit, tile.fully_muffled));
                eprintln!("MEKSOUND tile t={now} pos={:?} lit={} fully_muffled={} factor={}", tile.key.1, tile.lit, tile.fully_muffled, tile.volume_factor);
            }
            let engine_active = entry.playing.as_ref().is_some_and(|(id, _)| self.audio.is_active(*id));
            let view = TileView { can_play: tile.lit, removed: false, fully_muffled: tile.fully_muffled, sound: &tile.sound };
            let mut adapter =
                Adapter { game: self, pos: tile.key.1, playing: &mut entry.playing, factor: tile.volume_factor, base_volume: config.base_volume };
            entry.tile.update(config, view, engine_active, &mut adapter);
            // A instância em laço, no tick do motor de som.
            self.tick_playing(tile.key.1, &mut entry.playing, tile.volume_factor, now, normal);
            SOUNDS.with_borrow_mut(|sounds| sounds.insert(tile.key, entry));
        }
        // Os sons sem tile: o motor os tica com o fator 1 (`getTileVolumeFactor` não acha tile).
        let orphans: Vec<(&'static str, [i32; 3])> =
            ORPHANS.with_borrow(|orphans| orphans.keys().filter(|(owner, _)| *owner == dimension).copied().collect());
        for key in orphans {
            let Some(orphan) = ORPHANS.with_borrow_mut(|orphans| orphans.remove(&key)) else {
                continue;
            };
            let mut playing = Some(orphan);
            self.tick_playing(key.1, &mut playing, 1.0, now, normal);
            if let Some(orphan) = playing {
                ORPHANS.with_borrow_mut(|orphans| orphans.insert(key, orphan));
            }
        }
    }

    /// `TileTickableSound.tick` no tick do motor de som: confere o alcance a cada intervalo, aplica o volume e solta a instância que parou.
    fn tick_playing(&mut self, pos: [i32; 3], playing: &mut Option<Playing>, factor: f32, now: i64, normal: bool) {
        let Some((id, instance)) = playing.as_mut() else {
            return;
        };
        let id = *id;
        let player = self.player_position();
        let center = sound_position(pos);
        instance.tick(
            Some(now),
            normal,
            |volume| player.is_some_and(|player| player_in_range(player, center, volume, DEFAULT_ATTENUATION_DISTANCE)),
            factor,
        );
        if instance.is_stopped() {
            *playing = None;
            if TRACE {
                eprintln!("MEKSOUND ended t={now} pos={pos:?} id={id}");
            }
            self.audio.stop_and_release(id);
        } else if let Some(sound) = self.audio.engine().instance_mut(id) {
            if TRACE && sound.volume != instance.volume {
                eprintln!("MEKSOUND volume t={now} pos={pos:?} {} -> {}", sound.volume, instance.volume);
            }
            sound.volume = instance.volume;
        }
    }
}

/// O dia de hoje, no fuso do jogador: `YearlyDate.now()` do feriado.
pub(in crate::game) fn today() -> (u32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now();
    (now.month(), now.day())
}
