// Entra em `crates/horizon-client-render/src/block_entities.rs` como `pub mod machinery_transmitters` (`visibility = "pub"`), ao lado do `BlockEntities` que as injeções
// `transmitter_*` observam. Os eventos do jogo que a rede de transmissores do Mekanism recebe (`TileEntityTransmitter`, `TransmitterNetworkRegistry`, `AcceptorCache`), entregues na
// ordem do original (medida em `dev/referencia/rastros/transmitter_events_*.jsonl`, `TransmitterEventsTraceTest`):
//
// - `clearRemoved`: o block entity entrou no chunk (`LevelChunk.setBlockEntity`), posto ou lido do save. O NeoForge invalida na hora a capability da posição dele
//   (`BlockEntity.clearRemoved`), e quem vigia essa posição (o `AcceptorCache` de um transmissor vizinho) ouve logo depois do evento do próprio bloco;
// - `onAdded`: o bloco posto por um item (`BlockMekanism.setPlacedBy`), depois do `clearRemoved`; `/setblock` e o carregamento do chunk não o chamam;
// - `onChunkUnloaded` e `setRemoved`: o chunk sai (`LevelChunk.clearAllBlockEntities`): o `ChunkTicketLevelUpdatedEvent`, o `ChunkEvent.Unload` (que invalida as capabilities do chunk
//   inteiro), o `onChunkUnloaded` de todos os block entities e só então o `setRemoved` de todos, cada um invalidando a capability da própria posição. Um block entity que
//   sai por outro caminho (bloco quebrado ou trocado) só tem o `setRemoved`;
// - o tick (`onUpdateServer`), o aviso do vizinho (`BlockTransmitter.neighborChanged`: sem experimentos de redstone o `Orientation` é nulo, então sem lado) e, no fim do
//   tick do servidor, o `ServerTickEvent.Post` com `runsNormally`.
//
// O jogo entrega um evento por vez, na hora; o Horizon não tem esses pontos, então as injeções (`BlockEntities::insert`, `get_or_insert_with`, `load_chunk` e `remove`) e os ganchos
// (`block_entity_tick`, `neighbor_changed`, `place_block`, `chunk_load`, `chunk_unload`, `server_tick`, `world_lifecycle`) deixam a ocorrência numa fila, e a cola da `machinery_basis`
// a esvazia, com a dimensão do nível em que está, no fim da operação em andamento e nos pontos do tick (a ordem da fila é a das ocorrências). Diferença que fica: o dono do
// evento o recebe no fim da operação que o causou, e não no meio dela (decisão 187: o dono mostra o mesmo a cada tick em 15 000 cenários, menos o id da rede quando um cabo de
// redstone reativo pisca). A ordem em que o chunk que sai visita os block entities é a do mapa de todos eles (`BlockEntityOrder` da lib, decisão 188): `chunk_unloading` recebe
// todos, de qualquer tipo, nessa ordem.
//
// O chunk que o Horizon solta e o original ainda segura (decisão 199): o `LevelChunk` do original só sai da memória quando o ticket de carga passa do nível `RETAINED_LEVEL` (o
// `ChunkHolder` vai para `toDrop` com o nível acima de `ChunkLevel.MAX_LEVEL`); entre o 33 e esse nível ele é "inacessível" mas segue com os block entities, que não recebem
// `onChunkUnloaded` nem `setRemoved`, e o ticket de "carregado" os traz de volta à rede sem `clearRemoved` quando o chunk volta ao anel. O Horizon solta o chunk no raio residente
// dele, menor que esse: o `Tracker` segura os transmissores do chunk solto (`chunk_unloading_with`), não dá o `clearRemoved` de novo se o chunk volta (os que seguia continuam seguidos) e só
// entrega a saída de verdade quando o original soltaria o chunk (`release_held`).
//
// Só `std`: o teste do `machinery_basis_core` compila este arquivo como está (`tests/transmitter_events.rs`).
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

pub type Pos = [i32; 3];
/// Os block entities de um chunk: a posição e o tipo (`None` fora dos de mod).
pub type BlockList = Vec<(Pos, Option<String>)>;

/// Os seis lados na ordem de `Direction.from3DDataValue`: baixo, cima, norte, sul, oeste, leste.
pub const SIDE_NAMES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const OFFSETS: [Pos; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// O lado oposto (`Direction.getOpposite`).
pub const fn opposite(side: usize) -> usize {
    side ^ 1
}

fn step(pos: Pos, side: usize) -> Pos {
    [pos[0] + OFFSETS[side][0], pos[1] + OFFSETS[side][1], pos[2] + OFFSETS[side][2]]
}

/// O chunk de uma posição (`ChunkPos`, em chunks).
pub fn chunk_of(pos: Pos) -> [i32; 2] {
    [pos[0] >> 4, pos[2] >> 4]
}

/// O nível de ticket de carga em que o original ainda segura o `LevelChunk` com os block entities (`ChunkLevel.MAX_LEVEL`: 33 mais o raio de dependências até o estado `FULL`); o chunk de
/// nível maior é solto (`ChunkMap.processUnloads`: `setLoaded(false)`, `ChunkEvent.Unload` e `ServerLevel.unload`).
pub const RETAINED_LEVEL: i32 = 44;

/// Os tickets de carga de um nível (`DistanceManager`, o que o `ticket_levels` do Horizon modela): cada jogador segura o nível 31 no raio da distância de visão, e o nível sobe um por chunk
/// a partir dali; um chunk forçado segura o 31, um ticket de portal o 30 e a arena do dragão o 24, e o nível sobe um por chunk a partir do centro. O chunk toma o menor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadingTickets {
    pub players: Vec<[i32; 2]>,
    pub forced: Vec<[i32; 2]>,
    pub portal: Vec<[i32; 2]>,
    pub dragon: Option<[i32; 2]>,
    pub view_distance: i32,
}

impl LoadingTickets {
    /// O nível de ticket de carga do chunk (`ChunkHolder.getTicketLevel`), sem teto: o `ticket_levels` do Horizon corta o nível em 33, e o que importa aqui é o de depois dele.
    pub fn level(&self, chunk: [i32; 2]) -> i32 {
        let distance = |at: &[i32; 2]| (chunk[0] - at[0]).abs().max((chunk[1] - at[1]).abs());
        let mut level = i32::MAX;
        for player in &self.players {
            level = level.min(31 + (distance(player) - self.view_distance).max(0));
        }
        for forced in &self.forced {
            level = level.min(31 + distance(forced));
        }
        for portal in &self.portal {
            level = level.min(30 + distance(portal));
        }
        if let Some(arena) = &self.dragon {
            level = level.min(24 + distance(arena));
        }
        level
    }

    /// O original ainda segura o chunk na memória, com os block entities?
    pub fn retains(&self, chunk: [i32; 2]) -> bool {
        self.level(chunk) <= RETAINED_LEVEL
    }
}

/// O que o dono de um transmissor recebe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `TileEntityTransmitter.clearRemoved` (a chamada começou: o `markJoined` vem dela).
    ClearRemoved,
    /// `TileEntityTransmitter.onAdded` (`setPlacedBy`).
    PlacedByItem,
    /// `TileEntityTransmitter.onChunkUnloaded`.
    ChunkUnloaded,
    /// `TileEntityTransmitter.setRemoved`.
    Removed,
    /// `TileEntityTransmitter.onUpdateServer` (o ticker do block entity).
    Update,
    /// `BlockTransmitter.neighborChanged` -> `onNeighborBlockChange(null)`.
    Neighbor,
    /// O `RefreshListener` do `AcceptorCache`: a capability do vizinho em `side` (índice de [`SIDE_NAMES`], contado a partir do transmissor) foi invalidada.
    Capability(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Transmitter { dim: String, pos: Pos, kind: Kind },
    /// `ChunkTicketLevelUpdatedEvent` cruzando `ChunkMap.MAX_VIEW_DISTANCE`: `loaded` é a ida de "descarregado" para carregado.
    Ticket { dim: String, chunk: [i32; 2], loaded: bool },
    /// `ServerTickEvent.Post`; `runs_normally` é o `tickRateManager().runsNormally()`.
    ServerTickPost { runs_normally: bool },
    /// `ServerStoppedEvent` (`TransmitterNetworkRegistry.reset`): o mundo fechou.
    Reset,
}

/// O que as injeções anotam, sem dimensão: a cola a descobre quando esvazia a fila.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Raw {
    Added { pos: Pos, id: Option<String> },
    /// `get_or_insert_with` pôs um block entity feito por uma closure: o tipo, a cola o lê no mundo.
    AddedLater { pos: Pos },
    Removed { pos: Pos, id: Option<String> },
    /// `load_chunk`: os block entities do chunk, na ordem em que o jogo os recebeu.
    LoadChunk(BlockList),
    Neighbor { dim: String, pos: Pos },
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// O endereço do `BlockEntities` do nível do servidor (o `world.block_entities` do jogo que roda o tick do servidor), ou 0 se ainda não se sabe. O processo do anfitrião tem outro
/// `BlockEntities`, o espelho do cliente da janela, que recebe do servidor os blocos e os chunks (`Game::server_block`, `remote_chunk`) e passa pelas mesmas injeções: o que ele anota é o
/// servidor mudando de novo o que já mudou (o bloco trocado pela liga voltava um tick depois como `setRemoved` e `clearRemoved`), e não é do dono da rede.
static SERVER_SOURCE: AtomicUsize = AtomicUsize::new(0);
static QUEUE: Mutex<Vec<Raw>> = Mutex::new(Vec::new());
/// Os avisos de vizinho que o fim da última operação segurou: saem no fim da operação seguinte, ou antes, no `place_block` e nos pontos do tick.
static HELD: Mutex<Vec<Raw>> = Mutex::new(Vec::new());
/// Os chunks que carregaram antes de haver dono (o primeiro tipo se registra quando a tabela de ticks nasce, e o spawn pode carregar antes): o que `load_chunk` trouxe deles espera o
/// registro, que o põe na fila, e o dono entrega o `ChunkEvent.Load` desses chunks na primeira passada (`take_early_chunks`).
static EARLY: Mutex<Vec<Raw>> = Mutex::new(Vec::new());
static EARLY_CHUNKS: Mutex<Vec<[i32; 2]>> = Mutex::new(Vec::new());
static TRACKER: LazyLock<Mutex<Tracker>> = LazyLock::new(|| Mutex::new(Tracker::new()));

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Há dono para os eventos? Sem ele, as injeções não anotam nada.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Um tipo de block entity que é transmissor (`id` do `ModBlockEntity`); liga a fila.
pub fn register(id: &str) {
    lock(&TRACKER).ids.insert(id.to_owned());
    if !ACTIVE.swap(true, Ordering::Relaxed) {
        let early = std::mem::take(&mut *lock(&EARLY));
        let mut chunks = lock(&EARLY_CHUNKS);
        for raw in &early {
            if let Raw::LoadChunk(blocks) = raw
                && let Some((pos, _)) = blocks.first()
                && !chunks.contains(&chunk_of(*pos))
            {
                chunks.push(chunk_of(*pos));
            }
        }
        lock(&QUEUE).extend(early);
    }
}

/// Os chunks cujos block entities chegaram antes do registro: o dono lhes entrega o `chunk_loaded` (uma vez).
pub fn take_early_chunks() -> Vec<[i32; 2]> {
    std::mem::take(&mut *lock(&EARLY_CHUNKS))
}

/// Um chunk de `take_early_chunks` que o jogo ainda não tem inteiro: espera a passada seguinte.
pub fn keep_early_chunk(chunk: [i32; 2]) {
    lock(&EARLY_CHUNKS).push(chunk);
}

pub fn is_registered(id: &str) -> bool {
    lock(&TRACKER).ids.contains(id)
}

/// O `BlockEntities` do servidor se apresenta (o começo de cada tick do servidor): dali em diante só o que ele anota entra na fila.
pub fn set_server_source(source: usize) {
    SERVER_SOURCE.store(source, Ordering::Relaxed);
}

/// A ocorrência de um `BlockEntities` em `source` é do servidor? Enquanto ele não se apresentou (os chunks do começo do mundo), qualquer um vale.
pub fn from_server(source: usize) -> bool {
    let known = SERVER_SOURCE.load(Ordering::Relaxed);
    known == 0 || known == source
}

/// `push` de uma injeção, que diz de que `BlockEntities` ela veio: o espelho do cliente não anota nada.
pub fn push_from(source: usize, raw: Raw) {
    if from_server(source) {
        push(raw);
    } else if DROP_PROBE {
        let (kind, at) = match &raw {
            Raw::Added { pos, .. } | Raw::AddedLater { pos } | Raw::Removed { pos, .. } => ("one", Some(*pos)),
            Raw::LoadChunk(blocks) => ("chunk", blocks.first().map(|(pos, _)| *pos)),
            Raw::Neighbor { pos, .. } => ("neighbor", Some(*pos)),
        };
        eprintln!("MBD {{\"drop\":\"{kind}\",\"at\":{at:?},\"source\":{source},\"known\":{}}}", SERVER_SOURCE.load(Ordering::Relaxed));
    }
}

/// Só na cópia de teste (`enviar_fontes.sh setups` liga): as ocorrências que o espelho do cliente da janela anotou e a fila descartou (`MBD`), para a decisão 198.
const DROP_PROBE: bool = false;

pub fn push(raw: Raw) {
    if DROP_PROBE && let Raw::LoadChunk(blocks) = &raw {
        let chunk = blocks.first().map(|(pos, _)| chunk_of(*pos));
        eprintln!("MBD {{\"push_load\":{chunk:?},\"n\":{},\"mods\":{},\"active\":{}}}", blocks.len(), blocks.iter().filter(|(_, id)| id.is_some()).count(), is_active());
    }
    if is_active() {
        lock(&QUEUE).push(raw);
    } else if matches!(&raw, Raw::LoadChunk(blocks) if blocks.iter().any(|(_, id)| id.is_some())) {
        lock(&EARLY).push(raw);
    }
}

/// Só na cópia de teste (`enviar_fontes.sh` liga; o repositório fica com `false`): de onde vem cada ocorrência que as injeções anotam, o ponteiro do `BlockEntities` que a recebeu e as funções
/// que a chamaram (`MBR`).
const SOURCE_TRACE: bool = false;

/// A sonda `MBR` de uma ocorrência (`kind`: `insert` ou `remove`) em `pos`, recebida pelo `BlockEntities` em `source`.
pub fn trace_source(kind: &str, pos: Pos, source: usize) {
    if !SOURCE_TRACE {
        return;
    }
    let text = std::backtrace::Backtrace::force_capture().to_string();
    let frames: Vec<&str> = text
        .lines()
        .filter(|line| line.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|line| line.split_once(": ").map_or(line, |(_, name)| name))
        .filter(|name| !name.contains("trace_source") && !name.contains("backtrace") && !name.starts_with("machinery_basis_transmitter"))
        .take(9)
        .collect();
    eprintln!("MBR {kind} {pos:?} src={source:x} {}", frames.join(" < "));
}

/// `LevelChunk.setBlockState` com outro bloco no lugar: o block entity antigo sai (`removeBlockEntity`, `setRemoved`) antes de o novo entrar (`clearRemoved`). O rastreio é por posição, então o
/// novo que entrasse antes da saída do antigo se perderia (a saída do antigo o tiraria de quem é seguido): foi o que o upgrade pela liga mostrou.
pub fn note_insert(source: usize, pos: Pos, new: Option<&str>, old: Option<Option<&str>>) {
    if !is_active() {
        return;
    }
    match old {
        // Um block entity do mesmo tipo reescrito no lugar (o jogo mantém o objeto): nada muda para a rede.
        Some(old) if old == new => {}
        Some(old) => {
            push_from(source, Raw::Removed { pos, id: old.map(str::to_owned) });
            push_from(source, Raw::Added { pos, id: new.map(str::to_owned) });
        }
        None => push_from(source, Raw::Added { pos, id: new.map(str::to_owned) }),
    }
}

/// `get_or_insert_with` anota só a posição (o block entity nasce de uma closure): o tipo vem do que está lá agora, que `kind_at` diz (`Some(Some(id))` num block entity de mod, `Some(None)`
/// num de outro tipo, `None` sem block entity). Se o block entity ainda não está lá (a ocorrência veio antes da inserção), a ocorrência cai: a inserção anota a sua, e uma entrada sem tipo
/// marcaria a posição como ocupada e esconderia o `clearRemoved` da inserção de verdade.
pub fn resolve(raws: Vec<Raw>, kind_at: impl Fn(Pos) -> Option<Option<String>>) -> Vec<Raw> {
    raws.into_iter()
        .filter_map(|raw| match raw {
            Raw::AddedLater { pos } => Some(Raw::Added { pos, id: kind_at(pos)? }),
            other => Some(other),
        })
        .collect()
}

/// Quantos ticks um `clearRemoved` espera o block entity entrar no mundo antes de se desistir dele (o bloco já não é um cabo).
pub const PENDING_TICKS: i64 = 200;

/// Os `clearRemoved` que chegaram antes de o block entity estar no mundo do Horizon (a fila anota o `insert` no começo dele): `(dimensão, posição, tick)`, tentados de novo nos eventos seguintes.
#[derive(Default)]
pub struct PendingClears {
    waiting: Vec<(String, Pos, i64)>,
}

impl PendingClears {
    pub fn new() -> Self {
        Self::default()
    }

    /// Um `clearRemoved` sem block entity ainda, no tick `now`.
    pub fn wait(&mut self, dim: &str, pos: Pos, now: i64) {
        self.waiting.push((dim.to_owned(), pos, now));
    }

    /// Os da dimensão `dim` cujo block entity já está (`present`) saem para serem entregues, na ordem em que esperaram; os que esperaram `PENDING_TICKS` ou mais caem, e os de outras
    /// dimensões e os que ainda esperam ficam como estavam.
    pub fn retry(&mut self, dim: &str, now: i64, present: impl Fn(Pos) -> bool) -> Vec<Pos> {
        let mut ready = Vec::new();
        let mut keep = Vec::new();
        for (waiting_dim, pos, since) in std::mem::take(&mut self.waiting) {
            if waiting_dim != dim {
                keep.push((waiting_dim, pos, since));
            } else if present(pos) {
                ready.push(pos);
            } else if now - since < PENDING_TICKS {
                keep.push((waiting_dim, pos, since));
            }
        }
        self.waiting = keep;
        ready
    }

    /// O mundo fechou: o que esperava um block entity dele não vale no mundo seguinte (o relógio recomeça e a espera, medida contra ele, nunca acabaria).
    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

/// Esvazia a fila para quem é o dono dela: o servidor (`owner`). O espelho do cliente da janela do anfitrião também passa pelos pontos que a esvaziam (o tick dos block entities dele, o fim de
/// cada operação no mundo dele, `flush_signals`), mas não tem dono dos transmissores: se esvaziasse a fila, levava o que o servidor acabara de anotar (o `load_chunk` de um chunk que ainda não
/// tinha dado o `ChunkEvent.Load`) e os cabos desse chunk nunca entravam na rede (decisão 198, medido em jogo: o chunk sem nenhum `clearRemoved`).
pub fn take_for(owner: bool) -> Vec<Raw> {
    if owner { take() } else { Vec::new() }
}

/// Quantas ocorrências a fila tem agora, e quantas delas são de `load_chunk` (a sonda `MBD` da decisão 198: o que um cliente que a esvaziasse levaria).
pub fn queue_load() -> (usize, usize) {
    let queue = lock(&QUEUE);
    (queue.len(), queue.iter().filter(|raw| matches!(raw, Raw::LoadChunk(_))).count())
}

/// Esvazia a fila, na ordem das ocorrências.
pub fn take() -> Vec<Raw> {
    if DROP_PROBE {
        let (queued, loads) = queue_load();
        if queued > 0 {
            eprintln!("MBD {{\"take\":{queued},\"loads\":{loads}}}");
        }
    }
    std::mem::take(&mut *lock(&QUEUE))
}

/// O original cria o block entity dentro do `setBlock` e avisa os vizinhos depois (medido em `transmitter_events_basic.jsonl`: `clearRemoved`, a capability do vizinho, o `onAdded` de
/// um bloco posto por item e só então o aviso do vizinho). O Horizon avisa dentro do `setBlock` e cria o block entity depois (o item do `PlaceContext` em `game.rs`): o aviso espera.
/// Devolve (o resto, os avisos), cada um na ordem em que estava.
pub fn split_neighbors(raws: Vec<Raw>) -> (Vec<Raw>, Vec<Raw>) {
    raws.into_iter().partition(|raw| !matches!(raw, Raw::Neighbor { .. }))
}

/// Traduz a fila para eventos, com a dimensão do nível em que o jogo está.
pub fn translate(dim: &str, raws: Vec<Raw>) -> Vec<Event> {
    let mut tracker = lock(&TRACKER);
    let mut out = Vec::new();
    for raw in raws {
        tracker.apply(dim, raw, &mut out);
    }
    out
}

/// `ChunkEvent.Load` (o gancho `chunk_load`): o `ChunkTicketLevelUpdatedEvent`, o `clearRemoved` de cada block entity que o chunk trouxe, e a invalidação do chunk inteiro.
pub fn chunk_loaded(dim: &str, chunk: [i32; 2]) -> Vec<Event> {
    chunk_loaded_with(dim, chunk, true)
}

/// O mesmo, com o chunk que carrega fora do anel acessível (`accessible` falso): o ticket só sai quando o chunk entra nele (`accessibility`), como o original, que só o dá ao cruzar o nível 32.
pub fn chunk_loaded_with(dim: &str, chunk: [i32; 2], accessible: bool) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).chunk_loaded_with(dim, chunk, accessible, &mut out);
    out
}

/// Os chunks que entraram e os que saíram do anel acessível desde a última chamada (`accessible` diz de cada chunk carregado da dimensão se ele está nele): o
/// `ChunkTicketLevelUpdatedEvent` do original, que o Horizon não dá (compara-se `tickets.levels().loaded_ticking` no fim do tick).
pub fn accessibility(dim: &str, accessible: impl Fn([i32; 2]) -> bool) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).accessibility(dim, accessible, &mut out);
    out
}

/// O chunk vai sair (o gancho `chunk_unload`, com os block entities ainda no lugar): `blocks` são todos os dele, de qualquer tipo, na ordem do mapa do chunk.
pub fn chunk_unloading(dim: &str, chunk: [i32; 2], blocks: &[(Pos, Option<String>)]) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).chunk_unloading(dim, chunk, blocks, &mut out);
    out
}

/// O mesmo, quando o original seguraria o chunk que o Horizon solta (`hold`, decisão 199): os transmissores ficam seguidos até `release_held`.
pub fn chunk_unloading_with(dim: &str, chunk: [i32; 2], blocks: &[(Pos, Option<String>)], hold: bool) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).chunk_unloading_with(dim, chunk, blocks, hold, &mut out);
    out
}

/// Os chunks segurados da dimensão (os que o Horizon soltou e o original ainda teria).
pub fn held_chunks(dim: &str) -> Vec<[i32; 2]> {
    lock(&TRACKER).held_chunks(dim)
}

/// O chunk está segurado (o Horizon o soltou e o original ainda o teria)?
pub fn is_held(dim: &str, chunk: [i32; 2]) -> bool {
    lock(&TRACKER).held.contains_key(&(dim.to_owned(), chunk))
}

/// O original solta os chunks segurados que `retains` já não segura: os chunks e a saída de verdade deles, em ordem de posição.
pub fn release_held(dim: &str, retains: impl Fn([i32; 2]) -> bool) -> (Vec<[i32; 2]>, Vec<Event>) {
    let mut out = Vec::new();
    let mut released = Vec::new();
    let mut tracker = lock(&TRACKER);
    for chunk in tracker.held_chunks(dim) {
        if !retains(chunk) {
            tracker.release_held(dim, chunk, &mut out);
            released.push(chunk);
        }
    }
    (released, out)
}

pub fn placed(dim: &str, pos: Pos) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).simple(dim, pos, Kind::PlacedByItem, &mut out);
    out
}

pub fn update(dim: &str, pos: Pos) -> Vec<Event> {
    let mut out = Vec::new();
    lock(&TRACKER).simple(dim, pos, Kind::Update, &mut out);
    out
}

/// Os avisos segurados pela última operação (e a fila deles fica vazia).
pub fn take_held() -> Vec<Raw> {
    std::mem::take(&mut *lock(&HELD))
}

/// Segura os avisos desta operação até a seguinte.
pub fn hold(raws: Vec<Raw>) {
    lock(&HELD).extend(raws);
}

/// O mundo fechou: esquece o que seguia.
pub fn reset() -> Vec<Event> {
    let mut tracker = lock(&TRACKER);
    tracker.tracked.clear();
    tracker.present.clear();
    tracker.loading.clear();
    tracker.seen.clear();
    tracker.access.clear();
    tracker.held.clear();
    if DROP_PROBE {
        let (queued, loads) = queue_load();
        eprintln!("MBD {{\"reset_clears\":{queued},\"loads\":{loads}}}");
    }
    lock(&QUEUE).clear();
    lock(&HELD).clear();
    SERVER_SOURCE.store(0, Ordering::Relaxed);
    vec![Event::Reset]
}

/// Se algum transmissor já entrou na dimensão: o registro dela tem de receber o `onTick`.
pub fn sees_dimension(dim: &str) -> bool {
    lock(&TRACKER).seen.contains(dim)
}

/// Os níveis do servidor como a cola os vê no fim do tick: quais ele ticou, qual está no lugar, como entrar num e o que fazer com os eventos que saem.
pub trait ServerLevels {
    /// Os ids dos níveis que o servidor ticou neste tick, na ordem de `MinecraftServer.levels`.
    fn ticked(&self) -> Vec<String>;
    /// O id do nível que está no lugar.
    fn current(&self) -> String;
    /// Põe o nível no lugar; `false` se não deu.
    fn enter(&mut self, dim: &str) -> bool;
    /// Entrega os eventos aos donos, com o nível em que o jogo está.
    fn deliver(&mut self, events: Vec<Event>);
}

/// `TransmitterNetworkRegistry.onTick(ServerTickEvent.Post)`: no original é um só (`static`), depois do tick de todas as dimensões, e trata as redes de todas elas (`dimensionStore`).
/// Aqui cada dimensão com transmissor tem o registro dela, e todas recebem o `ServerTickPost` uma vez por tick do servidor: a cola entra em cada nível que o servidor ticou (o
/// `server_tick` dá só o último no lugar), deixa o nível que estava por último e volta a ele. A ordem entre as dimensões no original é a do `HashMap` de `ResourceKey`, que é por
/// `identityHashCode` e muda a cada execução; as dimensões não trocam nada entre si (cada uma tem o mundo, as máquinas e os buffers dela), então a ordem não aparece e a daqui é a de `levels`.
pub fn deliver_server_tick_post(levels: &mut impl ServerLevels, runs_normally: bool) {
    let back = levels.current();
    let mut dims: Vec<String> = levels.ticked().into_iter().filter(|dim| sees_dimension(dim)).collect();
    // O nível que já está no lugar fica por último: o caso comum, uma dimensão só, não troca nada.
    dims.sort_by_key(|dim| *dim == back);
    for dim in dims {
        if dim != levels.current() && !levels.enter(&dim) {
            continue;
        }
        levels.deliver(vec![Event::ServerTickPost { runs_normally }]);
    }
    if levels.current() != back {
        levels.enter(&back);
    }
}

/// As posições dos transmissores vivos de uma dimensão (para a cola e os testes).
pub fn tracked(dim: &str) -> Vec<Pos> {
    let mut list: Vec<Pos> = lock(&TRACKER).tracked.get(dim).map(|set| set.iter().copied().collect()).unwrap_or_default();
    list.sort_unstable();
    list
}

pub struct Tracker {
    ids: HashSet<String>,
    /// Os transmissores que entraram no chunk (`clearRemoved`) e ainda não saíram, por dimensão.
    tracked: HashMap<String, HashSet<Pos>>,
    /// Todo block entity que entrou no chunk e ainda não saiu, de qualquer tipo, por dimensão: o Horizon passa por mais de um ponto ao pôr ou tirar o mesmo.
    present: HashMap<String, HashSet<Pos>>,
    /// Os block entities de chunks que `load_chunk` trouxe e o `chunk_load` ainda não anunciou.
    loading: HashMap<(String, [i32; 2]), BlockList>,
    /// As dimensões em que algum transmissor já entrou desde que o mundo abriu: o registro delas existe, e o `onTick` do registro (um só, estático) trata todas.
    seen: HashSet<String>,
    /// Se cada chunk carregado está no anel acessível do original (nível de ticket 32 ou menos, `ChunkMap.MAX_VIEW_DISTANCE`), pelo último ticket dado: o chunk que sai do anel (o jogador se
    /// afastou) fica carregado por mais tempo (nível 33), e os transmissores dele já saíram da rede (`chunkAccessibilityChange(false)`).
    access: HashMap<(String, [i32; 2]), bool>,
    /// Os chunks que o Horizon soltou e o original ainda segura (decisão 199), com os block entities de cada um, na ordem do mapa do chunk: os transmissores deles seguem seguidos (`tracked`,
    /// `present`), sem `onChunkUnloaded` nem `setRemoved`, até o chunk voltar ou o original soltá-lo.
    held: HashMap<(String, [i32; 2]), BlockList>,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            ids: HashSet::new(),
            tracked: HashMap::new(),
            present: HashMap::new(),
            loading: HashMap::new(),
            seen: HashSet::new(),
            access: HashMap::new(),
            held: HashMap::new(),
        }
    }

    pub fn with_ids(ids: &[&str]) -> Self {
        let mut tracker = Self::new();
        tracker.ids = ids.iter().map(|id| (*id).to_owned()).collect();
        tracker
    }

    fn is_transmitter(&self, id: &Option<String>) -> bool {
        id.as_ref().is_some_and(|id| self.ids.contains(id))
    }

    fn watched(&self, dim: &str, pos: Pos) -> bool {
        self.tracked.get(dim).is_some_and(|set| set.contains(&pos))
    }

    /// Cada transmissor vizinho de `pos` vigia a capability dela: `BlockEntity.invalidateCapabilities` avisa os seis, nesta ordem.
    pub fn capabilities_at(&self, dim: &str, pos: Pos, out: &mut Vec<Event>) {
        for side in 0..6 {
            let neighbor = step(pos, side);
            if self.watched(dim, neighbor) {
                out.push(Event::Transmitter { dim: dim.to_owned(), pos: neighbor, kind: Kind::Capability(opposite(side)) });
            }
        }
    }

    /// `ServerLevel.invalidateCapabilities(ChunkPos)`: todo transmissor que vigia uma posição do chunk, uma vez por lado.
    pub fn capabilities_in_chunk(&self, dim: &str, chunk: [i32; 2], out: &mut Vec<Event>) {
        let Some(set) = self.tracked.get(dim) else {
            return;
        };
        let mut list: Vec<Pos> = set.iter().copied().collect();
        list.sort_unstable();
        for pos in list {
            for side in 0..6 {
                if chunk_of(step(pos, side)) == chunk {
                    out.push(Event::Transmitter { dim: dim.to_owned(), pos, kind: Kind::Capability(side) });
                }
            }
        }
    }

    fn added(&mut self, dim: &str, pos: Pos, id: &Option<String>, out: &mut Vec<Event>) {
        // O jogo do Horizon passa por mais de um ponto ao pôr o mesmo block entity (`insert` e `get_or_insert_with`): quem já entrou e não saiu não entra de novo.
        if !self.present.entry(dim.to_owned()).or_default().insert(pos) {
            return;
        }
        if self.is_transmitter(id) {
            self.tracked.entry(dim.to_owned()).or_default().insert(pos);
            self.seen.insert(dim.to_owned());
            out.push(Event::Transmitter { dim: dim.to_owned(), pos, kind: Kind::ClearRemoved });
        }
        self.capabilities_at(dim, pos, out);
    }

    fn removed(&mut self, dim: &str, pos: Pos, id: &Option<String>, out: &mut Vec<Event>) {
        // O Horizon pode remover o mesmo block entity por mais de um caminho (`remove` e a troca do bloco): quem já saiu não sai de novo.
        if !self.present.get_mut(dim).is_some_and(|set| set.remove(&pos)) {
            return;
        }
        if self.is_transmitter(id) {
            if let Some(set) = self.tracked.get_mut(dim) {
                set.remove(&pos);
            }
            out.push(Event::Transmitter { dim: dim.to_owned(), pos, kind: Kind::Removed });
        }
        self.capabilities_at(dim, pos, out);
    }

    pub fn apply(&mut self, dim: &str, raw: Raw, out: &mut Vec<Event>) {
        match raw {
            Raw::Added { pos, id } => self.added(dim, pos, &id, out),
            // A cola troca esta por `Added` antes de traduzir; sem ela, o tipo é desconhecido e só as capabilities mudam.
            Raw::AddedLater { pos } => self.added(dim, pos, &None, out),
            Raw::Removed { pos, id } => self.removed(dim, pos, &id, out),
            Raw::LoadChunk(blocks) => {
                if DROP_PROBE && let Some((pos, _)) = blocks.first() {
                    let chunk = chunk_of(*pos);
                    eprintln!("MBD {{\"load_raw\":[{},{}],\"n\":{}}}", chunk[0], chunk[1], blocks.len());
                }
                if let Some((pos, _)) = blocks.first() {
                    self.loading.entry((dim.to_owned(), chunk_of(*pos))).or_default().extend(blocks);
                }
            }
            Raw::Neighbor { dim, pos } => self.simple(&dim, pos, Kind::Neighbor, out),
        }
    }

    /// Não resta transmissor seguido na dimensão.
    pub fn simple_is_empty(&self, dim: &str) -> bool {
        self.tracked.get(dim).is_none_or(HashSet::is_empty)
    }

    pub fn simple(&self, dim: &str, pos: Pos, kind: Kind, out: &mut Vec<Event>) {
        if self.watched(dim, pos) {
            out.push(Event::Transmitter { dim: dim.to_owned(), pos, kind });
        }
    }

    pub fn chunk_loaded(&mut self, dim: &str, chunk: [i32; 2], out: &mut Vec<Event>) {
        self.chunk_loaded_with(dim, chunk, true, out);
    }

    pub fn chunk_loaded_with(&mut self, dim: &str, chunk: [i32; 2], accessible: bool, out: &mut Vec<Event>) {
        // O chunk que o original segurou volta: os block entities dele seguem os mesmos (nada de `clearRemoved`: `added` não repete quem não saiu), e só o ticket os leva à rede.
        self.held.remove(&(dim.to_owned(), chunk));
        self.access.insert((dim.to_owned(), chunk), accessible);
        if accessible {
            out.push(Event::Ticket { dim: dim.to_owned(), chunk, loaded: true });
        }
        for (pos, id) in self.loading.remove(&(dim.to_owned(), chunk)).unwrap_or_default() {
            self.added(dim, pos, &id, out);
        }
        self.capabilities_in_chunk(dim, chunk, out);
    }

    /// Os chunks da dimensão que mudaram de lado do anel acessível, em ordem de posição (o original os dá na ordem em que o gerenciador de distância muda o nível, que o Horizon não tem).
    pub fn accessibility(&mut self, dim: &str, accessible: impl Fn([i32; 2]) -> bool, out: &mut Vec<Event>) {
        let mut changed: Vec<([i32; 2], bool)> = Vec::new();
        for ((chunk_dim, chunk), state) in &self.access {
            // O chunk segurado (o Horizon o soltou) não está no mundo: ele volta ao anel quando carregar de novo, com o ticket do `chunk_loaded_with`. Um ticket antes disso juntaria os
            // transmissores como órfãos sem os vizinhos no mundo (cada um numa rede de um: medido em jogo, 48 redes de um cabo no `tp` a x = 100).
            if chunk_dim == dim && !self.held.contains_key(&(chunk_dim.clone(), *chunk)) {
                let now = accessible(*chunk);
                if now != *state {
                    changed.push((*chunk, now));
                }
            }
        }
        changed.sort_unstable();
        for (chunk, now) in changed {
            self.access.insert((dim.to_owned(), chunk), now);
            out.push(Event::Ticket { dim: dim.to_owned(), chunk, loaded: now });
        }
    }

    pub fn chunk_unloading(&mut self, dim: &str, chunk: [i32; 2], blocks: &[(Pos, Option<String>)], out: &mut Vec<Event>) {
        self.chunk_unloading_with(dim, chunk, blocks, false, out);
    }

    /// O Horizon solta o chunk; `hold` diz que o original o seguraria (o nível de ticket até `RETAINED_LEVEL`): os transmissores dele ficam como estão, só o ticket de saída do anel sai se o
    /// chunk ainda estava dentro dele, e a saída de verdade espera `release_held`.
    pub fn chunk_unloading_with(&mut self, dim: &str, chunk: [i32; 2], blocks: &[(Pos, Option<String>)], hold: bool, out: &mut Vec<Event>) {
        if !hold {
            self.held.remove(&(dim.to_owned(), chunk));
            self.unload(dim, chunk, blocks, out);
            return;
        }
        if let Some(state) = self.access.get_mut(&(dim.to_owned(), chunk))
            && std::mem::replace(state, false)
        {
            out.push(Event::Ticket { dim: dim.to_owned(), chunk, loaded: false });
        }
        self.loading.remove(&(dim.to_owned(), chunk));
        self.held.insert((dim.to_owned(), chunk), blocks.to_vec());
    }

    /// Os chunks segurados da dimensão, em ordem de posição.
    pub fn held_chunks(&self, dim: &str) -> Vec<[i32; 2]> {
        let mut chunks: Vec<[i32; 2]> = self.held.keys().filter(|(held_dim, _)| held_dim == dim).map(|(_, chunk)| *chunk).collect();
        chunks.sort_unstable();
        chunks
    }

    /// O original solta o chunk segurado: a saída que o Horizon adiou (`ChunkEvent.Unload`, o `onChunkUnloaded` de todos e o `setRemoved` de todos).
    pub fn release_held(&mut self, dim: &str, chunk: [i32; 2], out: &mut Vec<Event>) {
        if let Some(blocks) = self.held.remove(&(dim.to_owned(), chunk)) {
            self.unload(dim, chunk, &blocks, out);
        }
    }

    fn unload(&mut self, dim: &str, chunk: [i32; 2], blocks: &[(Pos, Option<String>)], out: &mut Vec<Event>) {
        // O chunk que já saiu do anel acessível não dá o ticket outra vez ao sair (o nível já passou de 32); sem registro conta como dentro.
        if self.access.remove(&(dim.to_owned(), chunk)).unwrap_or(true) {
            out.push(Event::Ticket { dim: dim.to_owned(), chunk, loaded: false });
        }
        self.capabilities_in_chunk(dim, chunk, out);
        for (pos, id) in blocks {
            if self.is_transmitter(id) {
                out.push(Event::Transmitter { dim: dim.to_owned(), pos: *pos, kind: Kind::ChunkUnloaded });
            }
        }
        for (pos, id) in blocks {
            self.removed(dim, *pos, id, out);
        }
        self.loading.remove(&(dim.to_owned(), chunk));
        // Os de outros tipos que ficaram no chunk (um baú) saem com ele sem passar por `remove`.
        if let Some(set) = self.present.get_mut(dim) {
            set.retain(|pos| chunk_of(*pos) != chunk);
        }
    }
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}
