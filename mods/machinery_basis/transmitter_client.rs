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
    ClientKind, ClientNetwork, ClientTransmitters, ClientView, Op, OPS_CHANNEL, RESET_CHANNEL, Range3D, ScalePacket, TagMessage, decode_batch, encode_batches,
    player_in_range,
};
use machinery_basis_core::transmission::transmitter::Compound;
use std::collections::HashMap;
use std::sync::Mutex;

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

/// O que sai no começo do tick seguinte, na ordem do pedido.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// `sendUpdatePacket`.
    Update(&'static str, [i32; 3]),
    /// `setRemoved`.
    Gone(&'static str, [i32; 3]),
}

/// Quem recebe um pacote: os jogadores com o chunk dentro da distância de visão, ou os que estão no raio da rede (`PacketUtils.sendToAllTracking(network, ...)`).
enum Audience {
    Chunk([i32; 3]),
    Range(Range3D),
}

static SOURCES: Mutex<Vec<TagSource>> = Mutex::new(Vec::new());
static QUEUE: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
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
    let mut queue = lock(&QUEUE);
    let after_gone = queue.iter().rposition(|item| *item == Pending::Gone(dimension, pos)).map_or(0, |index| index + 1);
    if !queue[after_gone..].contains(&Pending::Update(dimension, pos)) {
        queue.push(Pending::Update(dimension, pos));
    }
}

fn view_distance(game: &Game<'_>) -> i32 {
    game.settings.render_distance.clamp(2, 32) as i32
}

fn accepts(audience: &Audience, dimension: Option<&str>, player: [f64; 3], view: i32) -> bool {
    match audience {
        Audience::Chunk(pos) => {
            let chunk = |value: f64| (value.floor() as i32) >> 4;
            (chunk(player[0]) - (pos[0] >> 4)).abs() <= view && (chunk(player[2]) - (pos[2] >> 4)).abs() <= view
        }
        Audience::Range(range) => player_in_range(range, range.dimension, player[0].floor() as i32, player[2].floor() as i32, view) && dimension.is_some(),
    }
}

/// Os pacotes saem: a janela do anfitrião (se está na dimensão e perto) os aplica na hora, sem pacote; cada convidado perto recebe os dele em lotes, na ordem.
fn distribute(game: &mut Game<'_>, dimension: &str, items: Vec<(Op, Audience)>) {
    if items.is_empty() {
        return;
    }
    let view = view_distance(game);
    let host = game.native_player().filter(|player| player.dimension == dimension).map(|player| [player.x, player.y, player.z]);
    if let Some(host) = host {
        let ops: Vec<&Op> = items.iter().filter(|(_, audience)| accepts(audience, Some(dimension), host, view)).map(|(op, _)| op).collect();
        client_run(|levels| {
            let level = levels.entry(dimension.to_owned()).or_default();
            ops.into_iter().for_each(|op| level.apply(op));
        });
    }
    for player in game.native_server_players() {
        if player.dimension != dimension {
            continue;
        }
        let ops: Vec<Op> = items.iter().filter(|(_, audience)| accepts(audience, Some(dimension), player.position, view)).map(|(op, _)| op.clone()).collect();
        for batch in encode_batches(&ops) {
            let _ = game.native_net_send_to_player(player.uuid, OPS_CHANNEL, &batch);
        }
    }
}

fn tag_message(dimension: &str, pos: [i32; 3], out: TagOut) -> TagMessage {
    TagMessage { dimension: dimension.to_owned(), pos: sim_pos(pos), kind: out.kind, capacity: out.capacity, tag: out.tag }
}

/// O começo do tick do servidor: os `sendUpdatePacket` e as saídas do tick anterior vão agora, com o estado de agora (a transmissão de mudanças dos chunks).
pub(in crate::game) fn flush(game: &mut Game<'_>) {
    let queue = std::mem::take(&mut *lock(&QUEUE));
    if queue.is_empty() {
        return;
    }
    let sources: Vec<TagSource> = lock(&SOURCES).clone();
    let mut by_dimension: Vec<(&'static str, Vec<(Op, Audience)>)> = Vec::new();
    for item in queue {
        let (dimension, pos, op) = match item {
            Pending::Update(dimension, pos) => {
                let Some(out) = sources.iter().find_map(|source| (source.tag)(game, dimension, pos)) else {
                    continue;
                };
                (dimension, pos, Op::Tag(tag_message(dimension, pos, out)))
            }
            Pending::Gone(dimension, pos) => (dimension, pos, Op::Gone { dimension: dimension.to_owned(), pos: sim_pos(pos) }),
        };
        match by_dimension.iter_mut().find(|(known, _)| *known == dimension) {
            Some((_, items)) => items.push((op, Audience::Chunk(pos))),
            None => by_dimension.push((dimension, vec![(op, Audience::Chunk(pos))])),
        }
    }
    for (dimension, items) in by_dimension {
        distribute(game, dimension, items);
    }
}

/// `setRemoved` de um transmissor: o cliente larga a rede dele no começo do tick seguinte, antes de qualquer atualização que o mesmo lugar peça depois.
pub(in crate::game) fn gone(dimension: &'static str, pos: [i32; 3]) {
    let mut queue = lock(&QUEUE);
    queue.retain(|item| *item != Pending::Update(dimension, pos));
    queue.push(Pending::Gone(dimension, pos));
}

/// `PacketUtils.sendToAllTracking(network, ...)`: os pacotes de escala (e do conteúdo, para o fluido e o químico) a quem está no raio da rede.
pub(in crate::game) fn scale(game: &mut Game<'_>, dimension: &'static str, packets: Vec<(ScalePacket, Range3D, Option<Option<String>>)>) {
    let mut items = Vec::new();
    for (packet, range, contents) in packets {
        items.push((Op::Scale(packet), Audience::Range(range)));
        if let Some(resource) = contents {
            items.push((Op::Contents { network: packet.network, resource }, Audience::Range(range)));
        }
    }
    distribute(game, dimension, items);
}

/// O jogador entrou: o cliente dele começa vazio e recebe o que o servidor tem (a transmissão dos chunks que ele vê).
pub(in crate::game) fn player_in(game: &mut Game<'_>, player: u128) {
    let _ = game.native_net_send_to_player(player, RESET_CHANNEL, &[]);
    let Some(who) = game.native_server_players().into_iter().find(|candidate| candidate.uuid == player) else {
        return;
    };
    let view = view_distance(game);
    let sources: Vec<TagSource> = lock(&SOURCES).clone();
    let mut ops = Vec::new();
    for source in sources {
        for (dimension, pos) in (source.positions)() {
            if dimension != who.dimension || !accepts(&Audience::Chunk(pos), Some(dimension), who.position, view) {
                continue;
            }
            if let Some(out) = (source.tag)(game, dimension, pos) {
                ops.push(Op::Tag(tag_message(dimension, pos, out)));
            }
        }
    }
    for batch in encode_batches(&ops) {
        let _ = game.native_net_send_to_player(player, OPS_CHANNEL, &batch);
    }
}

/// O mundo fechou: o `clientNetworks` do anfitrião esvazia e o dos convidados também.
pub(in crate::game) fn reset(game: &mut Game<'_>) {
    lock(&QUEUE).clear();
    client_run(HashMap::clear);
    let players: Vec<u128> = game.native_server_players().iter().map(|player| player.uuid).collect();
    for player in players {
        let _ = game.native_net_send_to_player(player, RESET_CHANNEL, &[]);
    }
}

/// Um pacote do servidor chegou ao cliente (`horizon:mod_net`): cada `Op` roda o `handle` dele no cliente da dimensão.
pub(in crate::game) fn net_from_server(channel: &str, data: &[u8]) {
    match channel {
        OPS_CHANNEL => {
            let Some(ops) = decode_batch(data) else {
                return;
            };
            client_run(|levels| {
                for op in &ops {
                    match op {
                        Op::Tag(TagMessage { dimension, .. }) | Op::Gone { dimension, .. } => levels.entry(dimension.clone()).or_default().apply(op),
                        // Os pacotes de escala e de conteúdo não dizem a dimensão: o `clientNetworks` é um só, e a rede com este UUID está no nível dela.
                        Op::Scale(_) | Op::Contents { .. } => levels.values_mut().for_each(|level| level.apply(op)),
                    }
                }
            });
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
