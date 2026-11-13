// O lado do cliente dos transmissores no jogo (frente `cabo-cliente`, Fase 3): o `sendUpdatePacket` dos cabos, canos, tubos e condutores, o `PacketNetworkScale` e o
// conteúdo da rede, e o `clientNetworks` de cada cliente. A conta é de `machinery_basis_core::transmission::client` (a rede do cliente, o `handleUpdateTag`, o retângulo
// do pacote e o raio); esta parte carrega os pacotes pelo canal de mod do Horizon e guarda o estado.
//
// O Horizon junta o pacote de atualização de um block entity de mod no `nbt` da cópia do cliente sem chamar o mod (`native_block_entity_update_tag`), então o
// `handleUpdateTag` não teria onde rodar: o mesmo compound (`writeReducedUpdatedTag`) vai por um canal próprio e roda o `handleUpdateTag` ao chegar. Como o
// `sendUpdatePacket` do original, o pacote sai no começo do tick seguinte (a transmissão de mudanças dos chunks), com o conteúdo daquela hora.
//
// O estado do cliente é do processo: a janela do anfitrião (que lê o mundo do servidor integrado) e o convidado de LAN (que recebe os pacotes) cada um tem o seu, e o
// anfitrião o preenche na hora, sem pacote, para quem está perto (a janela não é um convidado, `native_server_players` não a lista).
use crate::game::Game;
use machinery_basis_core::sim::chunks::BlockPos as SimPos;
use machinery_basis_core::transmission::client::{
    ClientKind, ClientNetwork, ClientTransmitters, ClientView, GONE_CHANNEL, Range3D, SCALE_CHANNEL, ScalePacket, TAG_CHANNEL,
    TagMessage, decode_contents, decode_gone, encode_contents, encode_gone, player_in_range,
};
use machinery_basis_core::transmission::transmitter::Compound;
use std::collections::HashMap;
use std::sync::Mutex;

/// O canal do conteúdo da rede (`PacketFluidNetworkContents` e `PacketChemicalNetworkContents`).
pub const CONTENTS_CHANNEL: &str = "machinery_basis:network_contents";
/// O canal que esvazia o cliente (`MekanismClient.reset`, `TransmitterNetworkRegistry.clearClientNetworks`): a entrada do jogador e o mundo que fecha.
pub const RESET_CHANNEL: &str = "machinery_basis:transmitter_reset";

/// O que uma família dá de um transmissor para o pacote de atualização.
pub struct TagOut {
    pub kind: ClientKind,
    pub capacity: i64,
    pub tag: Compound,
}

/// Como uma família responde pelos seus transmissores: o pacote de atualização de um, e a posição de todos.
#[derive(Clone, Copy)]
pub struct TagSource {
    pub tag: fn(&Game<'_>, &'static str, [i32; 3]) -> Option<TagOut>,
    pub positions: fn() -> Vec<(&'static str, [i32; 3])>,
}

static SOURCES: Mutex<Vec<TagSource>> = Mutex::new(Vec::new());
/// Os transmissores que pediram `sendUpdatePacket` neste tick, na ordem do pedido e sem repetir.
static DIRTY: Mutex<Vec<(&'static str, [i32; 3])>> = Mutex::new(Vec::new());
/// O cliente de cada dimensão.
static CLIENT: Mutex<Option<HashMap<String, ClientTransmitters>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn sim_pos(pos: [i32; 3]) -> SimPos {
    SimPos::new(pos[0], pos[1], pos[2])
}

fn client_run<R>(f: impl FnOnce(&mut HashMap<String, ClientTransmitters>) -> R) -> R {
    f(lock(&CLIENT).get_or_insert_with(HashMap::new))
}

/// Uma família de transmissores se registra (uma vez cada).
pub fn register_source(source: TagSource) {
    let mut sources = lock(&SOURCES);
    if !sources.iter().any(|known| known.tag as usize == source.tag as usize) {
        sources.push(source);
    }
}

/// `TileEntityTransmitter.sendUpdatePacket`: o pacote sai no começo do tick seguinte, uma vez por transmissor.
pub fn dirty(dimension: &'static str, pos: [i32; 3]) {
    let mut list = lock(&DIRTY);
    if !list.contains(&(dimension, pos)) {
        list.push((dimension, pos));
    }
}

fn view_distance(game: &Game<'_>) -> i32 {
    game.settings.render_distance.clamp(2, 32) as i32
}

/// Quem recebe o pacote de um chunk: o jogador na mesma dimensão com o chunk dentro da distância de visão dele (`PacketDistributor.sendToPlayersTrackingChunk`).
fn tracks_chunk(player: [f64; 3], view: i32, pos: [i32; 3]) -> bool {
    let chunk = |value: f64| (value.floor() as i32) >> 4;
    (chunk(player[0]) - (pos[0] >> 4)).abs() <= view && (chunk(player[2]) - (pos[2] >> 4)).abs() <= view
}

/// Os convidados de LAN que `accept` aceita e a janela do anfitrião (`None` se `accept` a recusa), por dimensão. `host` é a posição dela se está nesta dimensão.
fn audience(game: &Game<'_>, dimension: &str, accept: impl Fn([f64; 3]) -> bool) -> (Vec<u128>, bool) {
    let guests = game
        .native_server_players()
        .into_iter()
        .filter(|player| player.dimension == dimension && accept(player.position))
        .map(|player| player.uuid)
        .collect();
    let host = game
        .native_player()
        .is_some_and(|player| player.dimension == dimension && accept([player.x, player.y, player.z]));
    (guests, host)
}

fn send(game: &mut Game<'_>, players: &[u128], channel: &str, bytes: &[u8]) {
    for player in players {
        let _ = game.native_net_send_to_player(*player, channel, bytes);
    }
}

fn apply_tag(message: &TagMessage) {
    client_run(|levels| {
        let level = levels.entry(message.dimension.clone()).or_default();
        if level.view_at(message.pos).is_none() {
            level.add_tile(message.pos, message.kind, message.capacity);
        }
        level.handle_update_tag(message.pos, &message.tag);
    });
}

/// O começo do tick do servidor: os `sendUpdatePacket` do tick anterior saem com o conteúdo de agora.
pub(in crate::game) fn flush(game: &mut Game<'_>) {
    let list = std::mem::take(&mut *lock(&DIRTY));
    if list.is_empty() {
        return;
    }
    let sources: Vec<TagSource> = lock(&SOURCES).clone();
    let view = view_distance(game);
    for (dimension, pos) in list {
        let Some(out) = sources.iter().find_map(|source| (source.tag)(game, dimension, pos)) else {
            continue;
        };
        let message = TagMessage { dimension: dimension.to_owned(), pos: sim_pos(pos), kind: out.kind, capacity: out.capacity, tag: out.tag };
        let (guests, host) = audience(game, dimension, |player| tracks_chunk(player, view, pos));
        if host {
            apply_tag(&message);
        }
        send(game, &guests, TAG_CHANNEL, &message.encode());
    }
}

/// `setRemoved` de um transmissor: o cliente larga a rede dele.
pub(in crate::game) fn gone(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    let view = view_distance(game);
    let (guests, host) = audience(game, dimension, |player| tracks_chunk(player, view, pos));
    if host {
        client_run(|levels| levels.entry(dimension.to_owned()).or_default().remove_tile(sim_pos(pos)));
    }
    send(game, &guests, GONE_CHANNEL, &encode_gone(dimension, sim_pos(pos)));
}

/// `PacketUtils.sendToAllTracking(network, ...)`: o pacote de escala (e o conteúdo, para o fluido e o químico) a quem está no raio da rede.
pub(in crate::game) fn scale(game: &mut Game<'_>, dimension: &'static str, packet: ScalePacket, range: Range3D, contents: Option<Option<String>>) {
    let view = view_distance(game);
    let in_range = |player: [f64; 3]| player_in_range(&range, range.dimension, player[0].floor() as i32, player[2].floor() as i32, view);
    let (guests, host) = audience(game, dimension, in_range);
    if host {
        client_run(|levels| {
            if let Some(level) = levels.get_mut(dimension) {
                level.on_scale_packet(&packet);
                if let Some(resource) = &contents {
                    level.on_contents_packet(packet.network, resource.clone());
                }
            }
        });
    }
    send(game, &guests, SCALE_CHANNEL, &packet.encode());
    if let Some(resource) = &contents {
        send(game, &guests, CONTENTS_CHANNEL, &encode_contents(packet.network, resource.as_deref()));
    }
}

/// O jogador entrou: o cliente dele começa vazio e recebe o que o servidor tem (a transmissão dos chunks que ele vê).
pub(in crate::game) fn player_in(game: &mut Game<'_>, player: u128) {
    let _ = game.native_net_send_to_player(player, RESET_CHANNEL, &[]);
    let Some(who) = game.native_server_players().into_iter().find(|candidate| candidate.uuid == player) else {
        return;
    };
    let view = view_distance(game);
    let sources: Vec<TagSource> = lock(&SOURCES).clone();
    for source in sources {
        for (dimension, pos) in (source.positions)() {
            if dimension != who.dimension || !tracks_chunk(who.position, view, pos) {
                continue;
            }
            if let Some(out) = (source.tag)(game, dimension, pos) {
                let message = TagMessage { dimension: dimension.to_owned(), pos: sim_pos(pos), kind: out.kind, capacity: out.capacity, tag: out.tag };
                let _ = game.native_net_send_to_player(player, TAG_CHANNEL, &message.encode());
            }
        }
    }
}

/// O mundo fechou: o `clientNetworks` do anfitrião esvazia e o dos convidados também.
pub(in crate::game) fn reset(game: &mut Game<'_>) {
    lock(&DIRTY).clear();
    client_run(HashMap::clear);
    let players: Vec<u128> = game.native_server_players().iter().map(|player| player.uuid).collect();
    send(game, &players, RESET_CHANNEL, &[]);
}

/// Um pacote do servidor chegou ao cliente (`horizon:mod_net`).
pub(in crate::game) fn net_from_server(channel: &str, data: &[u8]) {
    match channel {
        TAG_CHANNEL => {
            if let Some(message) = TagMessage::decode(data) {
                apply_tag(&message);
            }
        }
        GONE_CHANNEL => {
            if let Some((dimension, pos)) = decode_gone(data) {
                client_run(|levels| levels.entry(dimension).or_default().remove_tile(pos));
            }
        }
        SCALE_CHANNEL => {
            if let Some(packet) = ScalePacket::decode(data) {
                // O pacote não diz a dimensão: o `clientNetworks` é um só, e a rede com este UUID está no nível dela.
                client_run(|levels| levels.values_mut().for_each(|level| level.on_scale_packet(&packet)));
            }
        }
        CONTENTS_CHANNEL => {
            if let Some((network, resource)) = decode_contents(data) {
                client_run(|levels| levels.values_mut().for_each(|level| level.on_contents_packet(network, resource.clone())));
            }
        }
        RESET_CHANNEL => client_run(HashMap::clear),
        _ => {}
    }
}

/// O que o desenho lê do transmissor em `pos`: as conexões por lado (com `PUSH` e `PULL`).
pub(in crate::game) fn view_at(dimension: &str, pos: [i32; 3]) -> Option<ClientView> {
    client_run(|levels| levels.get(dimension)?.view_at(sim_pos(pos)))
}

/// A rede do cliente do transmissor em `pos` (o conteúdo que o desenho da Fase 4 lê: a escala, a energia e o último tipo).
pub(in crate::game) fn network_at(dimension: &str, pos: [i32; 3]) -> Option<ClientNetwork> {
    client_run(|levels| levels.get(dimension)?.network_at(sim_pos(pos)).cloned())
}
