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
    chunk_entered_view, chunk_in_view, chunks_left_view, ClientKind, ClientNetwork, ClientTransmitters, ClientView, Op, OPS_CHANNEL, RESET_CHANNEL, Range3D, ScalePacket, TagMessage, decode_batch, encode_batches,
    player_in_range,
};
use machinery_basis_core::transmission::inventory_owner::TransporterPacket;
use machinery_basis_core::transmission::transmitter::Compound;
use std::collections::HashMap;
use horizon_world::registry::BlockStateId;
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

/// Quem é observado: a janela do anfitrião ou um convidado.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Watcher {
    Host,
    Guest(u128),
}

static SOURCES: Mutex<Vec<TagSource>> = Mutex::new(Vec::new());
/// Onde cada jogador estava no último tick: a dimensão e o chunk. O `ChunkMap` do original manda os transmissores de um chunk a quem passa a vê-lo, ao andar ou ao trocar de dimensão.
static WATCHED: Mutex<Option<HashMap<Watcher, (String, i32, i32)>>> = Mutex::new(None);
static QUEUE: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
/// Os pacotes de movimento das pilhas (`PacketTransporterSync` e `PacketTransporterBatch`) que o tick mandou, para sair juntos no fim dele: o canal de mod aceita 32 mensagens por jogador
/// por tick, e cada transportador manda um ou dois pacotes por tick.
static PACKETS: Mutex<Vec<(&'static str, [i32; 3], Op)>> = Mutex::new(Vec::new());
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

/// `PacketUtils.sendToAllTracking(PacketTransporterSync | PacketTransporterBatch, tile)`: o pacote do transportador em `pos` sai no fim do tick, a quem vê o chunk dele.
pub(in crate::game) fn transporter_packet(dimension: &'static str, pos: [i32; 3], packet: TransporterPacket) {
    let op = match packet {
        TransporterPacket::Sync { id, stack } => Op::TransporterSync { dimension: dimension.to_owned(), pos: sim_pos(pos), id, stack },
        TransporterPacket::Batch { deletes, updates } => Op::TransporterBatch { dimension: dimension.to_owned(), pos: sim_pos(pos), deletes, updates },
    };
    lock(&PACKETS).push((dimension, pos, op));
}

/// O fim do tick: os pacotes de movimento das pilhas saem, na ordem em que o tick os mandou.
pub(in crate::game) fn flush_packets(game: &mut Game<'_>) {
    let packets = std::mem::take(&mut *lock(&PACKETS));
    if packets.is_empty() {
        return;
    }
    let mut by_dimension: Vec<(&'static str, Vec<(Op, Audience)>)> = Vec::new();
    for (dimension, pos, op) in packets {
        match by_dimension.iter_mut().find(|(known, _)| *known == dimension) {
            Some((_, items)) => items.push((op, Audience::Chunk(pos))),
            None => by_dimension.push((dimension, vec![(op, Audience::Chunk(pos))])),
        }
    }
    for (dimension, items) in by_dimension {
        distribute(game, dimension, items);
    }
}

/// `TileEntityTransmitter.tickClient` do transportador em `pos`: `onUpdateClient` avança as pilhas do cliente `speed` (o `getSpeed()` do tier).
pub(in crate::game) fn tick_transporter(dimension: &str, pos: [i32; 3], speed: i32) {
    client_run(|levels| {
        if let Some(level) = levels.get_mut(dimension) {
            level.tick_client_at(sim_pos(pos), speed);
        }
    });
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
            for op in ops {
                levels.entry(dimension.to_owned()).or_default().apply(op);
                probe("host", levels, dimension, op);
            }
        });
    }
    for player in game.native_server_players() {
        if player.dimension != dimension {
            continue;
        }
        let ops: Vec<Op> = items.iter().filter(|(_, audience)| accepts(audience, Some(dimension), player.position, view)).map(|(op, _)| op.clone()).collect();
        if crate::game::machinery_basis::transmitter_probe_enabled() {
            for op in &ops {
                if let Op::Scale(packet) = op {
                    eprintln!("MBN {{\"to\":\"{:x}\",\"net\":\"{:032x}\",\"scale\":{:?}}}", player.uuid, packet.network, packet.scale);
                }
            }
        }
        for batch in encode_batches(&ops) {
            let _ = game.native_net_send_to_player(player.uuid, OPS_CHANNEL, &batch);
        }
    }
}

/// A sonda do cliente (`MBS` a escala de uma rede, `MBG` o pacote de atualização de um transmissor, `MBN` o que o servidor mandou), só na cópia de teste (`TRANSMITTER_PROBE`).
fn probe(who: &str, levels: &HashMap<String, ClientTransmitters>, dimension: &str, op: &Op) {
    if !crate::game::machinery_basis::transmitter_probe_enabled() {
        return;
    }
    match op {
        Op::Scale(packet) => {
            let network = levels.values().find_map(|level| level.get_client_network(packet.network));
            match network {
                Some(net) => eprintln!(
                    "MBS {{\"who\":\"{who}\",\"net\":\"{:032x}\",\"scale\":{:?},\"cap\":{},\"n\":{},\"energy\":{},\"type\":{}}}",
                    packet.network,
                    net.current_scale,
                    net.capacity,
                    net.transmitters_size(),
                    net.energy,
                    net.last_type.as_ref().map_or("null".to_owned(), |id| format!("{id:?}"))
                ),
                None => eprintln!("MBS {{\"who\":\"{who}\",\"net\":\"{:032x}\",\"scale\":{:?},\"known\":false}}", packet.network, packet.scale),
            }
        }
        Op::Tag(message) => {
            let kinds: Vec<&str> = levels
                .get(dimension)
                .and_then(|level| level.view_at(message.pos))
                .map(|view| view.kind.iter().map(|kind| kind.serialized_name()).collect())
                .unwrap_or_default();
            let net = levels.get(dimension).and_then(|level| level.network_at(message.pos));
            let extra = levels.get(dimension).and_then(|level| level.extra_at(message.pos));
            eprintln!(
                "MBG {{\"who\":\"{who}\",\"dim\":\"{dimension}\",\"p\":[{},{},{}],\"kind\":{:?},\"net\":{},\"cap\":{},\"n\":{},\"heat\":{},\"color\":{},\"transit\":[{}]}}",
                message.pos.x,
                message.pos.y,
                message.pos.z,
                kinds,
                net.map_or("null".to_owned(), |net| format!("\"{:032x}\"", net.uuid)),
                net.map_or(0, |net| net.capacity),
                net.map_or(0, ClientNetwork::transmitters_size),
                extra.and_then(|extra| extra.heat).map_or("null".to_owned(), |heat| format!("{heat:?}")),
                extra.and_then(|extra| extra.color).map_or("null".to_owned(), |color| color.to_string()),
                extra
                    .map(|extra| extra.transit.iter().map(|stack| format!("[{},{:?},{}]", stack.index, stack.item.as_ref().map_or("", |item| item.0.as_str()), stack.item.as_ref().map_or(0, |item| item.1))).collect::<Vec<_>>().join(","))
                    .unwrap_or_default()
            );
        }
        Op::TransporterSync { pos, .. } | Op::TransporterBatch { pos, .. } => {
            let kind = if matches!(op, Op::TransporterSync { .. }) { "sync" } else { "batch" };
            let transit = levels
                .get(dimension)
                .and_then(|level| level.extra_at(*pos))
                .map(|extra| extra.transit.iter().map(|stack| format!("[{},{:?},{},{}]", stack.index, stack.item.as_ref().map_or("", |item| item.0.as_str()), stack.item.as_ref().map_or(0, |item| item.1), stack.progress)).collect::<Vec<_>>().join(","))
                .unwrap_or_default();
            eprintln!("MBH {{\"who\":\"{who}\",\"dim\":\"{dimension}\",\"p\":[{},{},{}],\"k\":\"{kind}\",\"transit\":[{transit}]}}", pos.x, pos.y, pos.z);
        }
        Op::Forget { chunks, .. } => eprintln!("MBF {{\"who\":\"{who}\",\"dim\":\"{dimension}\",\"chunks\":{},\"tiles\":{}}}", chunks.as_ref().map_or(-1, |list| list.len() as i64), levels.get(dimension).map_or(0, ClientTransmitters::tile_count)),
        _ => {}
    }
}

fn tag_message(dimension: &str, pos: [i32; 3], out: TagOut) -> TagMessage {
    TagMessage { dimension: dimension.to_owned(), pos: sim_pos(pos), kind: out.kind, capacity: out.capacity, tag: out.tag }
}

/// O começo do tick do servidor: os `sendUpdatePacket` e as saídas do tick anterior vão agora, com o estado de agora (a transmissão de mudanças dos chunks).
pub(in crate::game) fn flush(game: &mut Game<'_>) {
    watch(game);
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
    // A forma de colisão por instância do transmissor (`machinery_transmitter_collision`) some com ele.
    horizon_client::collisions::machinery_transmitter_collision::set(dimension, pos, None);
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

/// O chunk de uma posição do jogador.
fn chunk_of(position: [f64; 3]) -> (i32, i32) {
    ((position[0].floor() as i32) >> 4, (position[2].floor() as i32) >> 4)
}

/// Os pacotes de atualização de todos os transmissores de `dimension` que `wanted` aceita (a posição do transmissor).
fn collect_tags(game: &Game<'_>, dimension: &str, wanted: &dyn Fn([i32; 3]) -> bool) -> Vec<Op> {
    let sources: Vec<TagSource> = lock(&SOURCES).clone();
    let mut ops = Vec::new();
    for source in sources {
        for (dim, pos) in (source.positions)() {
            if dim != dimension || !wanted(pos) {
                continue;
            }
            if let Some(out) = (source.tag)(game, dim, pos) {
                ops.push(Op::Tag(tag_message(dim, pos, out)));
            }
        }
    }
    ops
}

fn watched_run<R>(f: impl FnOnce(&mut HashMap<Watcher, (String, i32, i32)>) -> R) -> R {
    f(lock(&WATCHED).get_or_insert_with(HashMap::new))
}

/// O jogador entrou: o cliente dele começa vazio e recebe o que o servidor tem (a transmissão dos chunks que ele vê).
pub(in crate::game) fn player_in(game: &mut Game<'_>, player: u128) {
    let _ = game.native_net_send_to_player(player, RESET_CHANNEL, &[]);
    let Some(who) = game.native_server_players().into_iter().find(|candidate| candidate.uuid == player) else {
        return;
    };
    let view = view_distance(game);
    let center = chunk_of(who.position);
    watched_run(|watched| watched.insert(Watcher::Guest(player), (who.dimension.clone(), center.0, center.1)));
    let ops = collect_tags(game, &who.dimension, &|pos| chunk_in_view((pos[0] >> 4, pos[2] >> 4), center, view));
    for batch in encode_batches(&ops) {
        let _ = game.native_net_send_to_player(player, OPS_CHANNEL, &batch);
    }
}

/// O `ChunkMap` do original manda o pacote de atualização de cada block entity de um chunk a quem passa a vê-lo, ao andar até perto dele ou ao trocar de dimensão (o
/// `PlayerChangedDimensionEvent` e a carga dos chunks da dimensão nova). A cada tick, quem mudou de chunk ou de dimensão recebe os transmissores dos chunks que acabaram de entrar na
/// distância de visão dele (todos, se a dimensão é outra).
fn watch(game: &mut Game<'_>) {
    let view = view_distance(game);
    let mut now: Vec<(Watcher, String, [f64; 3])> = Vec::new();
    if let Some(host) = game.native_player() {
        now.push((Watcher::Host, host.dimension.to_string(), [host.x, host.y, host.z]));
    }
    for guest in game.native_server_players() {
        now.push((Watcher::Guest(guest.uuid), guest.dimension.to_string(), guest.position));
    }
    watched_run(|watched| watched.retain(|key, _| now.iter().any(|(candidate, _, _)| candidate == key)));
    for (who, dimension, position) in now {
        let center = chunk_of(position);
        let before = watched_run(|watched| watched.insert(who, (dimension.clone(), center.0, center.1)));
        // `ChunkMap`: o que sai da vista é esquecido (`ClientboundForgetLevelChunkPacket`, e o `ClientChunkCache` solta os block entities); trocar de dimensão larga o nível inteiro da
        // dimensão que ficou para trás.
        let (before_chunk, forget) = match before {
            Some((before_dimension, x, z)) if before_dimension == dimension => {
                if (x, z) == center {
                    continue;
                }
                (Some((x, z)), Some(Op::Forget { dimension: dimension.clone(), chunks: Some(chunks_left_view(center, (x, z), view)) }))
            }
            Some((before_dimension, _, _)) => (None, Some(Op::Forget { dimension: before_dimension, chunks: None })),
            // Um convidado novo recebe tudo no `player_in`; a janela do anfitrião, que não tem pacote de entrada, e quem chegou de outra dimensão recebem o que veem agora.
            None if matches!(who, Watcher::Guest(_)) => continue,
            None => (None, None),
        };
        let wanted = move |pos: [i32; 3]| chunk_entered_view((pos[0] >> 4, pos[2] >> 4), center, before_chunk, view);
        let mut ops: Vec<Op> = forget.into_iter().collect();
        ops.extend(collect_tags(game, &dimension, &wanted));
        if ops.is_empty() {
            continue;
        }
        match who {
            Watcher::Host => client_run(|levels| {
                for op in &ops {
                    let at = if let Op::Forget { dimension: old, .. } = op { old.clone() } else { dimension.clone() };
                    levels.entry(at.clone()).or_default().apply(op);
                    probe("host", levels, &at, op);
                }
            }),
            Watcher::Guest(uuid) => {
                for batch in encode_batches(&ops) {
                    let _ = game.native_net_send_to_player(uuid, OPS_CHANNEL, &batch);
                }
            }
        }
    }
}

/// O mundo fechou: o `clientNetworks` do anfitrião esvazia e o dos convidados também.
pub(in crate::game) fn reset(game: &mut Game<'_>) {
    horizon_client::collisions::machinery_transmitter_collision::clear();
    lock(&QUEUE).clear();
    watched_run(HashMap::clear);
    client_run(HashMap::clear);
    model_reset();
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
            COLLISION_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            client_run(|levels| {
                for op in &ops {
                    let mut at = String::new();
                    match op {
                        Op::Tag(TagMessage { dimension, .. })
                        | Op::Gone { dimension, .. }
                        | Op::TransporterSync { dimension, .. }
                        | Op::TransporterBatch { dimension, .. }
                        | Op::Forget { dimension, .. } => {
                            levels.entry(dimension.clone()).or_default().apply(op);
                            at = dimension.clone();
                        }
                        // Os pacotes de escala e de conteúdo não dizem a dimensão: o `clientNetworks` é um só, e a rede com este UUID está no nível dela.
                        Op::Scale(_) | Op::Contents { .. } => levels.values_mut().for_each(|level| level.apply(op)),
                    }
                    probe("guest", levels, &at, op);
                }
            });
        }
        RESET_CHANNEL => {
            COLLISION_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            client_run(HashMap::clear);
            model_reset();
        }
        _ => {}
    }
}

/// O que o desenho lê do transmissor em `pos`: as conexões por lado (com `PUSH` e `PULL`).
pub(in crate::game) fn view_at(dimension: &str, pos: [i32; 3]) -> Option<ClientView> {
    client_run(|levels| levels.get(dimension)?.view_at(sim_pos(pos)))
}

/// `DiversionTransporter.modes` que o cliente leu do pacote de atualização do transmissor em `pos` (`readModes`: sem a chave, todos `DISABLED`). `None` se o cliente não conhece o
/// transmissor.
pub(in crate::game) fn modes_at(dimension: &str, pos: [i32; 3]) -> Option<[machinery_basis_core::transporter::node::DiversionControl; 6]> {
    use machinery_basis_core::transporter::node::DiversionControl;
    client_run(|levels| {
        let extra = levels.get(dimension)?.extra_at(sim_pos(pos))?;
        Some(match extra.modes {
            Some(ids) => ids.map(DiversionControl::by_id),
            None => [DiversionControl::Disabled; 6],
        })
    })
}

/// `ThermodynamicConductor.buffer.getHeat()` que o cliente leu do pacote de atualização do condutor em `pos` (`handleUpdateTag`); `None` até o primeiro pacote ou se não é condutor.
pub(in crate::game) fn heat_at(dimension: &str, pos: [i32; 3]) -> Option<f64> {
    client_run(|levels| levels.get(dimension)?.extra_at(sim_pos(pos))?.heat)
}

/// A rede do cliente do transmissor em `pos` (o conteúdo que o desenho da Fase 4 lê: a escala, a energia e o último tipo).
pub(in crate::game) fn network_at(dimension: &str, pos: [i32; 3]) -> Option<ClientNetwork> {
    client_run(|levels| levels.get(dimension)?.network_at(sim_pos(pos)).cloned())
}

// ---------------------------------------------------------------------------------------------------------------------------
// As partes do modelo por instância (frente `cabo-modelo`, Fase 4). O `TransmitterBlockStateModel` do original escolhe, para cada lado, uma parte do OBJ pelo `ModelData` da block
// entity (`TransmitterModelData`: o `ConnectionType` de cada lado, com `PUSH` e `PULL`); o mesher do Horizon escolhe o modelo só pelo estado do bloco. O módulo `machinery_transmitter_model`
// (`transmitter_model.rs`, no `mesher.rs` do `horizon-client-render`) guarda o dado de cada posição e a tabela dos estados de transmissor, e o HEAD de `StateModels::state`
// (`transmitter_model_state`) pede a ele as partes do bloco que o mesher desenha. Aqui o dado vem do cliente dos transmissores (o do anfitrião e o do convidado): a cada tique, as posições que mudaram
// (`ClientTransmitters::take_model_changes`: o tile entrou ou saiu, ou o pacote de atualização mudou as conexões) levam o dado novo ao módulo, e a seção do chunk é refeita
// (`requestModelDataUpdate`).
// ---------------------------------------------------------------------------------------------------------------------------

/// Um transmissor para o desenho: o bloco e se a block entity dá o dado do desviador (`TileEntityDiversionTransporter.initModelData`: sem o `hideContiguousJoin`, o modelo dele não tem as
/// partes do trecho reto).
#[derive(Clone)]
pub struct ModelFamily {
    pub block: String,
    pub diversion: bool,
}

static MODEL_FAMILIES: Mutex<Vec<ModelFamily>> = Mutex::new(Vec::new());
/// A dimensão que o módulo desenha: o dado de uma posição vale para a dimensão que o jogador vê.
static MODEL_DIMENSION: Mutex<Option<String>> = Mutex::new(None);

/// O mod diz os transmissores dele (uma vez, no começo do jogo).
pub(in crate::game) fn register_model_families(families: Vec<ModelFamily>) {
    *lock(&MODEL_FAMILIES) = families;
}

/// A tabela dos estados de transmissor do módulo, refeita a cada carga de dados (`server_data_loaded`): cada estado de cada bloco registrado.
pub(in crate::game) fn index_model_states(game: &Game<'_>) {
    use horizon_client::mesher::machinery_transmitter_model as model;
    let families = lock(&MODEL_FAMILIES).clone();
    let registry = game.pack.assets.registry;
    let mut states = Vec::new();
    for index in 0..registry.len() {
        let Some(state) = registry.state(BlockStateId(index as u32)) else {
            continue;
        };
        if let Some(family) = families.iter().position(|family| family.block.as_str() == &*state.name) {
            states.push((index as u32, family as u16));
        }
    }
    model::set_families(families.iter().map(|family| family.diversion).collect(), states);
}

/// A seção de chunk que contém `pos`, para refazer o desenho (`requestModelDataUpdate`).
fn dirty_section(game: &mut Game<'_>, pos: [i32; 3]) {
    let layout = game.world.layout();
    let section = (pos[1] - layout.min_y()) >> 4;
    if (0..layout.height() / 16).contains(&section) {
        game.world.dirty_sections.insert((pos[0] >> 4, section, pos[2] >> 4));
    }
}

/// O tique do cliente: o dado do modelo das posições que mudaram vai ao módulo. Ao trocar de dimensão o dado todo é refeito do cliente da dimensão nova.
pub(in crate::game) fn model_tick(game: &mut Game<'_>) {
    use horizon_client::mesher::machinery_transmitter_model as model;
    let dimension = game.native_dimension().to_owned();
    let rebuilt = lock(&MODEL_DIMENSION).as_deref() != Some(dimension.as_str());
    let changes: Vec<SimPos> = client_run(|levels| {
        let Some(level) = levels.get_mut(&dimension) else {
            return Vec::new();
        };
        let changes = level.take_model_changes();
        if rebuilt { level.tile_positions() } else { changes }
    });
    if rebuilt {
        model::clear_data();
        *lock(&MODEL_DIMENSION) = Some(dimension.clone());
    }
    for pos in changes {
        let at = [pos.x, pos.y, pos.z];
        let connections = view_at(&dimension, at).map(|view| view.kind.map(|kind| kind.ordinal() as u8));
        if model::set_data(at, connections) {
            dirty_section(game, at);
        }
    }
}

/// O mundo fechou: o módulo esquece o dado de todas as posições.
fn model_reset() {
    horizon_client::mesher::machinery_transmitter_model::clear_data();
    *lock(&MODEL_DIMENSION) = None;
}

/// Quantas vezes o espelho do cliente de um convidado mudou (`net_from_server`): a forma de colisão por instância dele se refaz quando muda.
static COLLISION_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

thread_local! {
    /// A revisão que a thread já refez, e as posições de que ela guardou a forma.
    static COLLISION_SEEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COLLISION_SET: std::cell::RefCell<std::collections::HashSet<(&'static str, [i32; 3])>> = std::cell::RefCell::new(std::collections::HashSet::new());
}

/// A forma de colisão por instância (`machinery_transmitter_collision`) de quem não simula o mundo, o convidado de LAN: o anfitrião a publica no `publish_arms` do dono dele, que o convidado
/// não tem; os tipos de cada lado chegam pelo pacote de atualização (o espelho do cliente), e daqui a tabela do processo do convidado se refaz quando o espelho muda. O anfitrião e o
/// jogo de uma pessoa só (que simulam) ficam com o `publish_arms`.
pub(in crate::game) fn collision_refresh(game: &Game<'_>) {
    if game.serving.is_some() || game.options.dedicated.is_some() {
        return;
    }
    let revision = COLLISION_REVISION.load(std::sync::atomic::Ordering::Relaxed);
    if COLLISION_SEEN.with(|seen| seen.replace(revision)) == revision {
        return;
    }
    let dimension = game.native_dimension();
    let views = client_run(|levels| levels.get(dimension).map(|level| level.special_views()).unwrap_or_default());
    let mut now = std::collections::HashSet::new();
    let mut incomplete = false;
    for (pos, kinds) in views {
        let at = [pos.x, pos.y, pos.z];
        let state = game.world.block(at[0], at[1], at[2]);
        let name = game.res.facts().get(state).name.clone();
        if let Some(boxes) = crate::game::machinery_basis::transmitter_shape_boxes(&name, &kinds) {
            horizon_client::collisions::machinery_transmitter_collision::set(dimension, at, Some(boxes));
            now.insert((dimension, at));
        } else {
            // O bloco ainda não chegou ao mundo do convidado (o pacote do transmissor vem antes do chunk): tenta de novo no quadro seguinte.
            incomplete = true;
        }
    }
    if incomplete {
        COLLISION_SEEN.with(|seen| seen.set(revision.wrapping_sub(1)));
    }
    COLLISION_SET.with(|set| {
        let mut set = set.borrow_mut();
        for (level, at) in set.iter() {
            if !now.contains(&(*level, *at)) {
                horizon_client::collisions::machinery_transmitter_collision::set(level, *at, None);
            }
        }
        *set = now;
    });
}
