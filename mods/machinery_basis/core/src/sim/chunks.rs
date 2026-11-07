//! Chunks falsos do mundo simulado: carregar, descarregar, blocos e block entities que somem com o chunk e voltam com ele,
//! na ordem em que o jogo dá os eventos. Serve aos testes da rede de transmissores, que dependem de quando um chunk deixa
//! de ser "acessível" para o mod, sem abrir o jogo.
//!
//! Nada aqui conhece o Mekanism: o mod entra como `ChunkHooks` (o que o jogo lhe diz, na ordem em que diz) e os block
//! entities são o tipo que ele escolher. Cada gancho cita o código do Java (NeoForge 26.3.0.100, fontes já remendadas, em
//! `minecraft-patched-26.3.0.100-sources.jar` do checkout de referência) e o gancho do Horizon que o equivale (pin
//! `f319701e`, `crates/horizon-client/src/game/`).
//!
//! # A ordem de um tick (`ServerLevel.tick`, `ServerLevel.java:423` e `:478`)
//!
//! 1. `ServerChunkCache.tick` (`ServerChunkCache.java:319`): `runDistanceManagerUpdates` promove o mapa de chunks
//!    visíveis e completa as cargas pendentes (evento de carga); em seguida `ChunkMap.processUnloads` descarrega o que está
//!    em `toDrop` (evento de descarga).
//! 2. `Level.tickBlockEntities` (`Level.java:490`): `onLoad` dos block entities novos e o tick dos tickers.
//! 3. O passo de um `gameTest` (descarregar um chunk, trocar um bloco, mudar o nível de um holder): `tick_level` termina, o
//!    passo roda e só então vem o `ServerTickEvent.Post`. Os eventos do passo ficam entre os tickers e o `Post`.
//! 4. `ServerTickEvent.Post`, depois de todos os níveis: nele roda `TransmitterNetworkRegistry.onTick`.
//!
//! A ordem foi conferida contra o jogo: `dev/referencia/testes/ChunkTraceTest.java` grava os eventos reais de
//! `reloadIntermediary` e `inaccessibleNotUnloaded` (`dev/referencia/rastros/chunk_*.jsonl`), e `tests/sim_chunks.rs` repete os
//! passos com as posições do rastro e compara evento a evento.
//!
//! # O que o jogo faz e o simulador reproduz
//!
//! - O chunk "acessível" para o mod é o de nível de ticket `<= 32` (`MAX_VIEW_DISTANCE`). Um chunk no nível 33 (`FULL`) ainda
//!   responde a `hasChunk`, mas não é mais "de ticking": é o estado "inacessível mas não descarregado" que o
//!   `inaccessibleNotUnloaded` do Mekanism provoca. O nível 45 (`UNLOAD_LEVEL`) manda o chunk para `toDrop`.
//! - O nível do ticket (`ChunkHolder.ticketLevel`) e o nível de simulação (`DistanceManager.simulationChunkTracker`) são
//!   dois. `shouldTickBlocksAt` olha o de simulação, e o tick de cada block entity ainda exige `ticketLevel <= 32`
//!   (`LevelChunk.isTicking`, `LevelChunk.java:415`). O teste do Mekanism mexe só no primeiro, com
//!   `DistanceManager.updateChunkScheduling`; o outro continua como os tickets o deixaram.
//! - `hasChunk` lê o mapa visível, que só é promovido no começo do tick: um chunk recém-pedido não responde no mesmo tick.
//! - A descarga grava o chunk, avisa `ChunkEvent.Unload` com os block entities ainda lá, e só então chama `onChunkUnloaded`
//!   e `setRemoved` em cada um, na ordem do mapa do chunk (`ChunkMap.java:512`, `LevelChunk.clearAllBlockEntities`, `:684`).
//!   Todos recebem `onChunkUnloaded` antes de qualquer `setRemoved`.
//! - A gravação segue `ChunkAccess.getBlockEntitiesPos`, um `HashSet` do JDK (`java_hash`), e não a ordem do mapa do chunk. A
//!   carga recria os block entities nessa ordem gravada, sem tickers; depois `setLoaded`, o registro dos tickers na ordem do
//!   mapa novo (`registerAllBlockEntitiesAfterLevelLoad`, `:692`) e `ChunkEvent.Load` (`ChunkStatusTasks.java:203` a `:213`).
//! - A ordem do mapa do chunk e a do `toDrop` são as do fastutil (`open_hash`), não a da inserção. As duas dependem das
//!   posições absolutas, e as de um `gameTest` mudam a cada rodada.
//! - `ChunkTicketLevelUpdatedEvent` só sai quando o nível muda (`EventHooks.fireChunkTicketLevelUpdated`).
//! - Um nível posto à mão com `set_chunk_load_level` dura até o `DistanceManager` recalcular o chunk pelos tickets
//!   (`update_ticket_level`, `LoadingChunkTracker.setLevel`): no rastro, 2 ticks.
//!
//! # O que não é reproduzido
//!
//! - O tempo de uma carga. O jogo leva alguns ticks entre o pedido e o `ChunkEvent.Load` (5 num rastro, 6 em outro; a thread
//!   principal e o `ChunkHolder` encadeiam futuros), e o simulador não tem como saber quantos: são `set_load_delay` ticks,
//!   0 por padrão. Os testes esperam o evento, como o `thenWaitUntil` do Mekanism.
//! - Quando um ticket vizinho faz o `DistanceManager` recalcular um chunk (`update_ticket_level`): o teste diz quando.
//! - `areEntitiesLoaded`, o limite do mundo na altura e as entidades.
//! - O `Set` de `chunksToUpdateFutures` do jogo é de identidade, sem ordem definida: aqui é a ordem do pedido.
//! - Notificações de vizinhança (`onNeighborBlockChange`) são da Fase 3.
//! - Durante um gancho que recebe o block entity, ele está fora do mapa (o Horizon faz o mesmo em
//!   `native_block_entity_tick`, `native_hooks.rs:3198`); no Java ele continua no mapa.
use super::java_hash::{hash_set_order, initial_capacity_for};
use super::open_hash::{OpenHashMap, OpenKey, mix_int, mix_long};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};

/// `ChunkMap.MAX_VIEW_DISTANCE`, `ChunkMap.java:125`. Acima disso o Mekanism trata o chunk como "descarregado".
pub const MAX_VIEW_DISTANCE: i32 = 32;
/// `ChunkLevel.ENTITY_TICKING_LEVEL`, `ChunkLevel.java:12`.
pub const ENTITY_TICKING_LEVEL: i32 = 31;
/// `ChunkLevel.BLOCK_TICKING_LEVEL`, `ChunkLevel.java:11`.
pub const BLOCK_TICKING_LEVEL: i32 = 32;
/// `ChunkLevel.FULL_CHUNK_LEVEL`, `ChunkLevel.java:10`: o último nível em que `ServerChunkCache.hasChunk` responde sim.
pub const FULL_CHUNK_LEVEL: i32 = 33;
/// `ChunkLevel.MAX_LEVEL`, `ChunkLevel.java:15`: 33 mais o raio das dependências de `FULL`, que são 11 no
/// `ChunkPyramid.GENERATION_PYRAMID` do 26.3. Acima disso o chunk não está carregado.
pub const MAX_LEVEL: i32 = FULL_CHUNK_LEVEL + 11;
/// `MekGameTestHelper.INACCESSIBLE_LEVEL`, `MekGameTestHelper.java:55`.
pub const INACCESSIBLE_LEVEL: i32 = MAX_VIEW_DISTANCE + 1;
/// `MekGameTestHelper.UNLOAD_LEVEL`, `MekGameTestHelper.java:56`.
pub const UNLOAD_LEVEL: i32 = MAX_LEVEL + 1;

/// `Level.MAX_LEVEL_SIZE`, `Level.java:99`: o limite horizontal do mundo.
const MAX_LEVEL_SIZE: i32 = 30_000_000;
/// A altura do overworld, que é a dimensão dos testes.
const MIN_BUILD_HEIGHT: i32 = -64;
const MAX_BUILD_HEIGHT: i32 = 320;

/// `ChunkLevel.isLoaded`.
pub fn is_loaded_level(level: i32) -> bool {
    level <= MAX_LEVEL
}

/// `ChunkPos`: o chunk em coordenadas de chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// `ChunkPos.pack`, `x & 0xFFFFFFFF | (z & 0xFFFFFFFF) << 32`. A ordem das árvores do Mekanism
    /// (`Long2BooleanRBTreeMap`) é a deste `long` com sinal.
    pub fn pack(self) -> i64 {
        i64::from(self.x as u32) | (i64::from(self.z as u32) << 32)
    }

    /// `ChunkPos.unpack`.
    pub fn unpack(key: i64) -> Self {
        Self {
            x: key as i32,
            z: (key >> 32) as i32,
        }
    }
}

impl PartialOrd for ChunkPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChunkPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pack().cmp(&other.pack())
    }
}

impl OpenKey for ChunkPos {
    fn mixed(self) -> i32 {
        mix_long(self.pack())
    }

    const NULL_KEY: Option<Self> = Some(ChunkPos::new(0, 0));
}

/// `BlockPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// `ChunkPos.containing`: `SectionPos.blockToSectionCoord` é o deslocamento aritmético por 4.
    pub fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x >> 4, self.z >> 4)
    }

    /// `Vec3i.hashCode`, `(y + z * 31) * 31 + x`: a chave dos mapas do fastutil.
    fn hash_code(self) -> i32 {
        self.y
            .wrapping_add(self.z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(self.x)
    }
}

impl PartialOrd for BlockPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// `Vec3i.compareTo`: por y, depois z, depois x.
impl Ord for BlockPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.y, self.z, self.x).cmp(&(other.y, other.z, other.x))
    }
}

impl OpenKey for BlockPos {
    fn mixed(self) -> i32 {
        mix_int(self.hash_code())
    }
}

/// O que o mundo simulado registra, na ordem em que acontece. É o que um teste compara com o que o jogo dá.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimEvent {
    /// `ChunkTicketLevelUpdatedEvent`, postado por `ChunkMap.updateChunkScheduling` (`ChunkMap.java:392`).
    TicketLevelUpdated { chunk: ChunkPos, old: i32, new: i32 },
    /// `ChunkEvent.Load` (`ChunkStatusTasks.java:213`). `new` é o `isNewChunk`: o chunk nunca foi gravado.
    ChunkLoad { chunk: ChunkPos, new: bool },
    /// `ChunkEvent.Unload` (`ChunkMap.java:527`).
    ChunkUnload { chunk: ChunkPos },
    /// `LevelChunk.setBlockEntity` (`LevelChunk.java:426`): `setLevel` e `clearRemoved` do block entity.
    BlockEntityAdded { pos: BlockPos },
    /// `BlockEntity.onLoad`, chamado por `Level.tickBlockEntities` (`Level.java:497`).
    BlockEntityLoad { pos: BlockPos },
    /// O ticker do block entity (`LevelChunk.BoundTickingBlockEntity.tick`, `LevelChunk.java:763`).
    BlockEntityTick { pos: BlockPos },
    /// `saveAdditional`, quando o chunk é gravado (`SerializableChunkData.copyOf`).
    BlockEntitySaved { pos: BlockPos },
    /// `BlockEntity.onChunkUnloaded` (`LevelChunk.clearAllBlockEntities`, `LevelChunk.java:685`).
    BlockEntityChunkUnloaded { pos: BlockPos },
    /// `BlockEntity.setRemoved`.
    BlockEntityRemoved { pos: BlockPos },
    /// `Level.setBlock` confirmou um estado novo.
    BlockChanged {
        pos: BlockPos,
        old: Option<String>,
        new: Option<String>,
    },
    /// `ServerTickEvent.Post`.
    ServerTickPost { game_time: u64 },
}

/// O que o jogo diz ao mod, na ordem em que diz. Todo gancho tem corpo vazio: o mod implementa os que usa.
///
/// `E` é o block entity do mod (`ModBlockEntity`, no Horizon) e `Saved` o que ele grava (o NBT).
pub trait ChunkHooks {
    type BlockEntity;
    type Saved: Clone;

    /// `EntityBlock.newBlockEntity`, e `BlockEntity.loadStatic` quando há o que foi gravado (`saved`). `None`: o bloco não
    /// tem block entity. Horizon: `native_block_entity`, `native_hooks.rs:3062`.
    fn new_block_entity(
        &mut self,
        pos: BlockPos,
        block: &str,
        saved: Option<&Self::Saved>,
    ) -> Option<Self::BlockEntity>;

    /// `BlockEntity.saveWithFullMetadata`, a cada gravação do chunk. Horizon: o NBT do block entity de mod.
    fn save_block_entity(&mut self, pos: BlockPos, entity: &mut Self::BlockEntity) -> Self::Saved;

    /// Se o bloco tem ticker (`EntityBlock.getTicker`). Horizon: `native_block_entity_ticks`, `native_hooks.rs:3072`.
    fn block_entity_ticks(&self, _block: &str) -> bool {
        false
    }

    /// `ChunkTicketLevelUpdatedEvent`. O Horizon não dá: compara-se `tickets.levels().loaded_ticking` no fim do tick.
    fn ticket_level_updated(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _chunk: ChunkPos,
        _old: i32,
        _new: i32,
    ) {
    }

    /// `ChunkEvent.Load`. Horizon: `native_chunk_load`, `native_hooks.rs:7017`, que o dá antes de qualquer descarga do mesmo
    /// tick (`native_chunks_changed`, `native_hooks.rs:9532`), o que o Java não garante.
    fn chunk_load(&mut self, _level: &LevelView<Self::BlockEntity>, _chunk: ChunkPos, _new: bool) {}

    /// `ChunkEvent.Unload`: os block entities ainda estão lá, os blocos do chunk não deviam ser lidos. Horizon:
    /// `native_chunk_unload`, `native_hooks.rs:7026`.
    fn chunk_unload(&mut self, _level: &LevelView<Self::BlockEntity>, _chunk: ChunkPos) {}

    /// `setBlockEntity`: `setLevel` e `clearRemoved` (`TileEntityTransmitter.clearRemoved`, `:146`).
    fn block_entity_added(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _entity: &mut Self::BlockEntity,
    ) {
    }

    /// `BlockEntity.onLoad`, no primeiro `tickBlockEntities` depois de entrar no mundo.
    fn block_entity_loaded(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _entity: &mut Self::BlockEntity,
    ) {
    }

    /// Um tick do block entity. Horizon: `native_block_entity_tick`, `native_hooks.rs:3198`.
    fn block_entity_tick(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _entity: &mut Self::BlockEntity,
    ) {
    }

    /// `BlockEntity.onChunkUnloaded` (`TileEntityTransmitter.onChunkUnloaded`, `:156`).
    fn block_entity_chunk_unloaded(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _entity: &mut Self::BlockEntity,
    ) {
    }

    /// `BlockEntity.setRemoved` (`TileEntityTransmitter.setRemoved`, `:165`). No Horizon: o HEAD em
    /// `GameLevel::remove_entity`, `spill.rs:490`.
    fn block_entity_removed(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _entity: &mut Self::BlockEntity,
    ) {
    }

    /// `Level.setBlock` confirmou o estado novo (`None` é ar). Horizon: `native_block_changed`, `native_hooks.rs:1203`.
    fn block_changed(
        &mut self,
        _level: &LevelView<Self::BlockEntity>,
        _pos: BlockPos,
        _old: Option<&str>,
        _new: Option<&str>,
    ) {
    }

    /// `ServerTickEvent.Post`, onde roda `TransmitterNetworkRegistry.onTick`. Horizon: `server_tick`, fase `ServerTickEnd`.
    fn server_tick_post(&mut self, _level: &LevelView<Self::BlockEntity>) {}
}

/// Um block entity no mapa do chunk. `entity` fica `None` enquanto um gancho o tem.
struct Slot<E> {
    id: u64,
    entity: Option<E>,
}

/// `LevelChunk`: os block entities, na ordem do `Object2ObjectOpenHashMap`, e os tickers pelo `tickersInLevel`.
struct LevelChunk<E> {
    entities: OpenHashMap<BlockPos, Slot<E>>,
    tickers: BTreeMap<BlockPos, u64>,
}

/// `ChunkHolder`.
struct Holder<E> {
    /// `ChunkHolder.ticketLevel`.
    level: i32,
    /// Está no `visibleChunkMap`, que só é copiado do mapa em atualização por `promoteChunkMap`.
    visible: bool,
    chunk: Option<LevelChunk<E>>,
}

/// O que um gancho pode ler do mundo. Os métodos têm o nome e a regra dos do Java.
pub struct LevelView<E> {
    game_time: u64,
    /// O `updatingChunkMap`: todo holder que existe.
    holders: BTreeMap<ChunkPos, Holder<E>>,
    /// `ChunkMap.modified`: há holders novos para promover ao mapa visível.
    modified: bool,
    /// `DistanceManager.simulationChunkTracker`: o nível de simulação de cada chunk, sem entrada quando não simula.
    simulation: HashMap<ChunkPos, i32>,
    /// Os blocos de cada chunk. Persistem com o chunk; só se lê o de um chunk acessível.
    blocks: HashMap<ChunkPos, BTreeMap<BlockPos, String>>,
}

impl<E> LevelView<E> {
    fn new() -> Self {
        Self {
            game_time: 0,
            holders: BTreeMap::new(),
            modified: false,
            simulation: HashMap::new(),
            blocks: HashMap::new(),
        }
    }

    /// O `gameTime` do tick em andamento.
    pub fn game_time(&self) -> u64 {
        self.game_time
    }

    /// `ChunkMap.getUpdatingChunkIfPresent(...).getTicketLevel()`: o nível do holder, se ele existe.
    pub fn ticket_level(&self, chunk: ChunkPos) -> Option<i32> {
        self.holders.get(&chunk).map(|holder| holder.level)
    }

    /// `WorldUtils.isChunkLoaded` (`WorldUtils.java:89`) no servidor: `ServerChunkCache.hasChunk` (`:263`), que olha o mapa
    /// visível e o nível `FULL`.
    pub fn is_chunk_loaded(&self, chunk: ChunkPos) -> bool {
        self.holders
            .get(&chunk)
            .is_some_and(|holder| holder.visible && holder.level <= FULL_CHUNK_LEVEL)
    }

    /// `WorldUtils.isBlockLoaded` (`WorldUtils.java:109`): dentro do mundo e com o chunk carregado.
    pub fn is_block_loaded(&self, pos: BlockPos) -> bool {
        in_world_bounds(pos) && self.is_chunk_loaded(pos.chunk())
    }

    /// O `LevelChunk` existe na memória, seja qual for o nível: um chunk "inacessível mas não descarregado" ainda o tem.
    pub fn has_level_chunk(&self, chunk: ChunkPos) -> bool {
        self.holders
            .get(&chunk)
            .is_some_and(|holder| holder.chunk.is_some())
    }

    /// `WorldUtils.getBlockStateIfLoaded`: o bloco, ou `None` se o chunk não está carregado. O ar vem como `minecraft:air`.
    pub fn block_if_loaded(&self, pos: BlockPos) -> Option<&str> {
        if !self.is_block_loaded(pos) {
            return None;
        }
        Some(self.block_unchecked(pos).unwrap_or(AIR))
    }

    fn block_unchecked(&self, pos: BlockPos) -> Option<&str> {
        self.blocks
            .get(&pos.chunk())
            .and_then(|blocks| blocks.get(&pos))
            .map(String::as_str)
    }

    /// `WorldUtils.getTileEntity(world, pos)` (`WorldUtils.java:331`): só se o bloco está carregado. É `None` também enquanto
    /// um gancho tem o block entity em mãos.
    pub fn block_entity(&self, pos: BlockPos) -> Option<&E> {
        if !self.is_block_loaded(pos) {
            return None;
        }
        self.block_entity_unchecked(pos)
    }

    /// O block entity que o `LevelChunk` guarda, esteja o chunk acessível ou não.
    pub fn block_entity_unchecked(&self, pos: BlockPos) -> Option<&E> {
        let chunk = self.holders.get(&pos.chunk())?.chunk.as_ref()?;
        chunk.entities.get(pos)?.entity.as_ref()
    }

    /// Os block entities do `LevelChunk`, na ordem do mapa.
    pub fn block_entity_positions(&self, chunk: ChunkPos) -> Vec<BlockPos> {
        self.holders
            .get(&chunk)
            .and_then(|holder| holder.chunk.as_ref())
            .map(|chunk| chunk.entities.keys_in_order())
            .unwrap_or_default()
    }

    /// `DistanceManager.inBlockTickingRange` (`DistanceManager.java:139`): o nível de simulação do chunk.
    pub fn should_tick_blocks_at(&self, chunk: ChunkPos) -> bool {
        self.simulation_level(chunk) <= BLOCK_TICKING_LEVEL
    }

    /// O nível do `simulationChunkTracker`; sem entrada, o chunk não simula.
    pub fn simulation_level(&self, chunk: ChunkPos) -> i32 {
        self.simulation.get(&chunk).copied().unwrap_or(UNLOAD_LEVEL)
    }
}

const AIR: &str = "minecraft:air";

/// `Level.isInWorldBounds`.
fn in_world_bounds(pos: BlockPos) -> bool {
    (MIN_BUILD_HEIGHT..MAX_BUILD_HEIGHT).contains(&pos.y)
        && (-MAX_LEVEL_SIZE..MAX_LEVEL_SIZE).contains(&pos.x)
        && (-MAX_LEVEL_SIZE..MAX_LEVEL_SIZE).contains(&pos.z)
}

/// Um ticker na lista de `Level.blockEntityTickers`. Vive enquanto o `tickersInLevel` do chunk aponta para ele.
struct Ticker {
    id: u64,
    pos: BlockPos,
}

/// O mundo de um nível com chunks, e o mod por trás dele.
pub struct SimLevel<H: ChunkHooks> {
    pub hooks: H,
    view: LevelView<H::BlockEntity>,
    /// O que cada chunk gravou, na ordem em que gravou.
    saved: HashMap<ChunkPos, Vec<(BlockPos, H::Saved)>>,
    /// Os chunks que já foram gravados alguma vez: carregá-los de novo não é `isNewChunk`.
    persisted: HashSet<ChunkPos>,
    /// `DistanceManager.chunksToUpdateFutures`: os holders a completar, com os ticks que faltam para a carga terminar.
    futures: Vec<(ChunkPos, u32)>,
    /// Quantos ticks a carga de um chunk leva depois do pedido (`set_load_delay`).
    load_delay: u32,
    /// `ChunkMap.toDrop`, um `LongOpenHashSet`.
    to_drop: OpenHashMap<ChunkPos, ()>,
    /// `Level.blockEntityTickers`.
    tickers: Vec<Ticker>,
    /// `Level.freshBlockEntities`: o block entity e o `id` com que entrou, para não confundir com quem o trocou.
    fresh: Vec<(BlockPos, u64)>,
    next_id: u64,
    events: Vec<SimEvent>,
}

impl<H: ChunkHooks> SimLevel<H> {
    pub fn new(hooks: H) -> Self {
        Self {
            hooks,
            view: LevelView::new(),
            saved: HashMap::new(),
            persisted: HashSet::new(),
            futures: Vec::new(),
            load_delay: 0,
            to_drop: OpenHashMap::new(),
            tickers: Vec::new(),
            fresh: Vec::new(),
            next_id: 0,
            events: Vec::new(),
        }
    }

    pub fn view(&self) -> &LevelView<H::BlockEntity> {
        &self.view
    }

    /// Os eventos até agora, sem esvaziar.
    pub fn events(&self) -> &[SimEvent] {
        &self.events
    }

    /// Entrega os eventos registrados e esvazia a lista.
    pub fn take_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.events)
    }

    /// Um ticket que põe o chunk no nível `level` e o faz simular: o caminho do `DistanceManager`, que muda o holder e o
    /// `simulationChunkTracker` juntos e agenda a carga (`chunksToUpdateFutures`). Um nível acima de `MAX_LEVEL` solta o
    /// chunk. É o ticket que a estrutura do gametest mantém.
    pub fn set_ticket_level(&mut self, chunk: ChunkPos, level: i32) {
        if is_loaded_level(level) {
            self.view.simulation.insert(chunk, level);
        } else {
            self.view.simulation.remove(&chunk);
        }
        self.set_chunk_load_level(chunk, level);
        if is_loaded_level(level) {
            self.schedule_load(chunk);
        }
    }

    /// Quantos ticks a carga de um chunk leva depois de o `DistanceManager` pegar o pedido, o que ele faz no começo do tick
    /// seguinte ao pedido do teste (ou no mesmo tick, em `update_ticket_level`). O jogo não tem um número: os futuros do
    /// `ChunkHolder` passam pela thread principal e pelo pipeline de status, e os rastros de `reloadIntermediary` deram de 5 a
    /// 9 ticks entre o pedido e o `ChunkEvent.Load`. Com `0`, a carga acontece no tick em que o pedido é pego. Vale para os
    /// pedidos feitos depois da chamada.
    pub fn set_load_delay(&mut self, ticks: u32) {
        self.load_delay = ticks;
    }

    /// `chunksToUpdateFutures.add(holder)`: um holder só entra uma vez.
    fn schedule_load(&mut self, chunk: ChunkPos) {
        if !self.futures.iter().any(|(queued, _)| *queued == chunk) {
            self.futures.push((chunk, self.load_delay));
        }
    }

    /// `LoadingChunkTracker.update` e `setLevel` (`LoadingChunkTracker.java:34`): quando um ticket muda perto do chunk, o
    /// `DistanceManager` recalcula o nível dele pelos tickets e compara com o do holder. Se o teste do Mekanism pôs o holder em
    /// outro nível (`set_chunk_load_level`), o jogo o devolve ao dos tickets assim que o rastreador passa pelo chunk, e avisa o
    /// `ChunkTicketLevelUpdatedEvent`. O rastro de `inaccessibleNotUnloaded` mostra isso 2 ticks depois do pedido, porque os
    /// testes vizinhos mexem em tickets. O simulador não sabe quando; o teste diz.
    pub fn update_ticket_level(&mut self, chunk: ChunkPos) {
        let target = self
            .view
            .simulation
            .get(&chunk)
            .copied()
            .unwrap_or(UNLOAD_LEVEL);
        let old = self.view.ticket_level(chunk).unwrap_or(UNLOAD_LEVEL);
        if old == target {
            return;
        }
        self.set_chunk_load_level(chunk, target);
        if self.view.holders.contains_key(&chunk) {
            self.schedule_load(chunk);
        }
    }

    /// `MekGameTestHelper.setChunkLoadLevel` (`MekGameTestHelper.java:65`) e `DistanceManager.updateChunkScheduling`: muda o nível
    /// do holder sem carregar nem descarregar e devolve o antigo (`UNLOAD_LEVEL` sem holder). É como o teste do Mekanism
    /// simula o estado "inacessível mas não descarregado". O nível de simulação não muda.
    pub fn set_chunk_load_level(&mut self, chunk: ChunkPos, new: i32) -> i32 {
        let old = self.view.ticket_level(chunk).unwrap_or(UNLOAD_LEVEL);
        // `ChunkMap.updateChunkScheduling`, `ChunkMap.java:363`: sem holder e sem nível carregado nos dois lados, nada.
        if !is_loaded_level(old) && !is_loaded_level(new) {
            return old;
        }
        if let Some(holder) = self.view.holders.get_mut(&chunk) {
            holder.level = new;
            if is_loaded_level(new) {
                self.to_drop.remove(chunk);
            } else {
                self.to_drop.insert(chunk, ());
            }
        } else {
            self.view.holders.insert(
                chunk,
                Holder {
                    level: new,
                    visible: false,
                    chunk: None,
                },
            );
            self.view.modified = true;
        }
        // `EventHooks.fireChunkTicketLevelUpdated`: só avisa se o nível mudou.
        if old != new {
            self.events
                .push(SimEvent::TicketLevelUpdated { chunk, old, new });
            self.hooks.ticket_level_updated(&self.view, chunk, old, new);
        }
        old
    }

    /// `ChunkData.updateChunkLoading(helper, true, level)` (`TransmitterNetworkTest.java:166`): o holder nasce no nível
    /// `level` e entra em `chunksToUpdateFutures`, para carregar no próximo tick.
    pub fn load_chunk(&mut self, chunk: ChunkPos, level: i32) {
        self.set_chunk_load_level(chunk, level);
        if self.view.holders.contains_key(&chunk) {
            self.schedule_load(chunk);
        }
    }

    /// `ChunkData.updateChunkLoading(helper, false, ...)`: põe o holder em `UNLOAD_LEVEL` e roda `processUnloads` na hora.
    /// Devolve o nível que ele tinha, para o teste voltar a ele.
    pub fn unload_chunk(&mut self, chunk: ChunkPos) -> i32 {
        let level = self.view.ticket_level(chunk).unwrap_or(UNLOAD_LEVEL);
        self.set_chunk_load_level(chunk, UNLOAD_LEVEL);
        self.process_unloads();
        level
    }

    /// `ChunkMap.processUnloads` (`ChunkMap.java:471`): cada chunk em `toDrop` sai dos holders em atualização e é descarregado,
    /// na ordem do `LongOpenHashSet`.
    pub fn process_unloads(&mut self) {
        let order = self.to_drop.keys_in_order();
        self.to_drop.clear();
        let dropped: Vec<ChunkPos> = order
            .into_iter()
            .filter(|chunk| self.view.holders.contains_key(chunk))
            .collect();
        for chunk in dropped {
            self.unload_level_chunk(chunk);
        }
    }

    /// `scheduleUnload` (`ChunkMap.java:512`), depois de o chunk estar gravado em disco.
    fn unload_level_chunk(&mut self, chunk: ChunkPos) {
        if self.view.has_level_chunk(chunk) {
            self.save_chunk(chunk);
            self.events.push(SimEvent::ChunkUnload { chunk });
            self.hooks.chunk_unload(&self.view, chunk);
            // `ServerLevel.unload` -> `LevelChunk.clearAllBlockEntities`, `LevelChunk.java:684`.
            let order = self.view.block_entity_positions(chunk);
            for &pos in &order {
                self.with_entity(pos, |hooks, view, entity| {
                    hooks.block_entity_chunk_unloaded(view, pos, entity)
                });
                self.events.push(SimEvent::BlockEntityChunkUnloaded { pos });
            }
            for &pos in &order {
                self.with_entity(pos, |hooks, view, entity| {
                    hooks.block_entity_removed(view, pos, entity)
                });
                self.events.push(SimEvent::BlockEntityRemoved { pos });
            }
        }
        self.view.holders.remove(&chunk);
        self.view.modified = true;
    }

    /// `ChunkMap.save` e `SerializableChunkData.copyOf`: grava os block entities do chunk na ordem de
    /// `ChunkAccess.getBlockEntitiesPos`, um `HashSet` do JDK (`Sets.newHashSet(pendentes)` e `addAll` das chaves do mapa, que
    /// entram na ordem do mapa). Não é a ordem do mapa, e é a que o chunk recarregado usa para recriá-los.
    fn save_chunk(&mut self, chunk: ChunkPos) {
        let mut saved = Vec::new();
        let in_map_order = self.view.block_entity_positions(chunk);
        let in_save_order =
            hash_set_order(in_map_order, BlockPos::hash_code, initial_capacity_for(0));
        for pos in in_save_order {
            if let Some(data) =
                self.with_entity(pos, |hooks, _, entity| hooks.save_block_entity(pos, entity))
            {
                self.events.push(SimEvent::BlockEntitySaved { pos });
                saved.push((pos, data));
            }
        }
        self.saved.insert(chunk, saved);
        self.persisted.insert(chunk);
    }

    /// `ChunkMap.saveAllChunks`: grava todos os chunks carregados, sem descarregar.
    pub fn save_all(&mut self) {
        let chunks: Vec<ChunkPos> = self
            .view
            .holders
            .iter()
            .filter(|(_, holder)| holder.chunk.is_some())
            .map(|(chunk, _)| *chunk)
            .collect();
        for chunk in chunks {
            self.save_chunk(chunk);
        }
    }

    /// Chama `f` com o block entity de `pos` fora do mapa, como o Horizon faz, e o devolve. `None` se não há.
    fn with_entity<R>(
        &mut self,
        pos: BlockPos,
        f: impl FnOnce(&mut H, &LevelView<H::BlockEntity>, &mut H::BlockEntity) -> R,
    ) -> Option<R> {
        let slot = self
            .view
            .holders
            .get_mut(&pos.chunk())?
            .chunk
            .as_mut()?
            .entities
            .get_mut(pos)?;
        let mut entity = slot.entity.take()?;
        let result = f(&mut self.hooks, &self.view, &mut entity);
        if let Some(slot) = self
            .view
            .holders
            .get_mut(&pos.chunk())
            .and_then(|holder| holder.chunk.as_mut())
            .and_then(|chunk| chunk.entities.get_mut(pos))
        {
            slot.entity = Some(entity);
        }
        Some(result)
    }

    /// `Level.setBlock(pos, state, 3)` sobre `LevelChunk.setBlockState` (`LevelChunk.java:270`): troca o bloco (`None` é ar) e,
    /// nesta ordem, remove o block entity do bloco antigo, cria o do novo e avisa `block_changed`. `false` se o bloco já era
    /// aquele. O chunk tem de estar carregado: o `setBlock` do jogo carregaria um que não está, e o simulador não faz isso.
    pub fn set_block(&mut self, pos: BlockPos, block: Option<&str>) -> bool {
        let chunk = pos.chunk();
        assert!(
            self.view.is_chunk_loaded(chunk) && self.view.has_level_chunk(chunk),
            "set_block em {pos:?}: o chunk {chunk:?} não está carregado"
        );
        let blocks = self.view.blocks.entry(chunk).or_default();
        let old = blocks.get(&pos).cloned();
        if old.as_deref() == block {
            return false;
        }
        match block {
            Some(block) => blocks.insert(pos, block.to_owned()),
            None => blocks.remove(&pos),
        };
        // `LevelChunk.java:309`: o estado mudou de bloco, e o block entity do antigo sai.
        self.remove_block_entity(pos);
        if let Some(block) = block
            && let Some(entity) = self.hooks.new_block_entity(pos, block, None)
        {
            // `addAndRegisterBlockEntity`, `LevelChunk.java:398`.
            let id = self.set_block_entity(pos, entity);
            self.update_ticker(pos);
            self.fresh.push((pos, id));
        }
        self.events.push(SimEvent::BlockChanged {
            pos,
            old: old.clone(),
            new: block.map(str::to_owned),
        });
        self.hooks
            .block_changed(&self.view, pos, old.as_deref(), block);
        true
    }

    /// `LevelChunk.setBlockEntity` (`LevelChunk.java:426`): `setLevel` e `clearRemoved`, e entra no mapa. Quem estava lá sai
    /// com `setRemoved`. Devolve o `id` do novo.
    fn set_block_entity(&mut self, pos: BlockPos, mut entity: H::BlockEntity) -> u64 {
        self.events.push(SimEvent::BlockEntityAdded { pos });
        self.hooks.block_entity_added(&self.view, pos, &mut entity);
        let id = self.next_id;
        self.next_id += 1;
        let chunk = self
            .view
            .holders
            .get_mut(&pos.chunk())
            .and_then(|holder| holder.chunk.as_mut())
            .expect("o chunk do block entity existe");
        let previous = chunk.entities.insert(
            pos,
            Slot {
                id,
                entity: Some(entity),
            },
        );
        if let Some(Slot {
            entity: Some(mut previous),
            ..
        }) = previous
        {
            self.events.push(SimEvent::BlockEntityRemoved { pos });
            self.hooks
                .block_entity_removed(&self.view, pos, &mut previous);
        }
        id
    }

    /// `LevelChunk.removeBlockEntity` (`LevelChunk.java:480`): sai do mapa, `setRemoved`, e o ticker morre.
    fn remove_block_entity(&mut self, pos: BlockPos) {
        let Some(chunk) = self
            .view
            .holders
            .get_mut(&pos.chunk())
            .and_then(|holder| holder.chunk.as_mut())
        else {
            return;
        };
        let removed = chunk.entities.remove(pos);
        chunk.tickers.remove(&pos);
        if let Some(Slot {
            entity: Some(mut entity),
            ..
        }) = removed
        {
            self.events.push(SimEvent::BlockEntityRemoved { pos });
            self.hooks
                .block_entity_removed(&self.view, pos, &mut entity);
        }
    }

    /// `LevelChunk.updateBlockEntityTicker` (`LevelChunk.java:713`): cria o ticker, ou o troca de dono se já havia, ou o tira.
    fn update_ticker(&mut self, pos: BlockPos) {
        let ticks = self
            .view
            .block_unchecked(pos)
            .is_some_and(|block| self.hooks.block_entity_ticks(block));
        let Some(chunk) = self
            .view
            .holders
            .get_mut(&pos.chunk())
            .and_then(|holder| holder.chunk.as_mut())
        else {
            return;
        };
        if !ticks {
            chunk.tickers.remove(&pos);
        } else if let Entry::Vacant(vacant) = chunk.tickers.entry(pos) {
            let id = self.next_id;
            self.next_id += 1;
            vacant.insert(id);
            // `Level.addBlockEntityTicker`: no fim da lista.
            self.tickers.push(Ticker { id, pos });
        }
    }

    /// Um tick inteiro: o do nível e o fim do tick do servidor.
    pub fn tick(&mut self) {
        self.tick_level();
        self.finish_tick();
    }

    /// `ServerLevel.tick`: cargas, descargas e os block entities. O que um `gameTest` faz num passo (descarregar um chunk,
    /// trocar um bloco, mudar o nível de um holder) vem depois disso e antes de `finish_tick`: o rastro do `ChunkTraceTest`
    /// mostra os eventos do passo entre os tickers do nível e o `ServerTickEvent.Post`.
    pub fn tick_level(&mut self) {
        self.run_distance_manager_updates();
        self.process_unloads();
        self.tick_block_entities();
    }

    /// `ServerTickEvent.Post` (onde roda o `TransmitterNetworkRegistry.onTick`) e o relógio.
    pub fn finish_tick(&mut self) {
        self.events.push(SimEvent::ServerTickPost {
            game_time: self.view.game_time,
        });
        self.hooks.server_tick_post(&self.view);
        self.view.game_time += 1;
    }

    pub fn tick_n(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    /// `ServerChunkCache.runDistanceManagerUpdates` (`ServerChunkCache.java:284`): leva os holders novos ao mapa visível e
    /// completa as cargas pedidas, na ordem do pedido.
    fn run_distance_manager_updates(&mut self) {
        let futures = std::mem::take(&mut self.futures);
        if std::mem::take(&mut self.view.modified) {
            for holder in self.view.holders.values_mut() {
                holder.visible = true;
            }
        }
        for (chunk, remaining) in futures {
            if remaining > 0 {
                self.futures.push((chunk, remaining - 1));
                continue;
            }
            let needs_load =
                self.view.holders.get(&chunk).is_some_and(|holder| {
                    holder.level <= FULL_CHUNK_LEVEL && holder.chunk.is_none()
                });
            if needs_load {
                self.load_level_chunk(chunk);
            }
        }
    }

    /// A tarefa `full` de `ChunkStatusTasks` (`ChunkStatusTasks.java:185`): monta o `LevelChunk`.
    fn load_level_chunk(&mut self, chunk: ChunkPos) {
        let new = !self.persisted.contains(&chunk);
        self.view
            .holders
            .get_mut(&chunk)
            .expect("o holder existe")
            .chunk = Some(LevelChunk {
            entities: OpenHashMap::new(),
            tickers: BTreeMap::new(),
        });
        // `runPostLoad`, `:203`: `SerializableChunkData.postLoadChunk` recria os block entities na ordem gravada. O chunk
        // ainda não está carregado, então eles entram sem ticker.
        for (pos, saved) in self.saved.get(&chunk).cloned().unwrap_or_default() {
            let Some(block) = self.view.block_unchecked(pos).map(str::to_owned) else {
                continue;
            };
            if let Some(entity) = self.hooks.new_block_entity(pos, &block, Some(&saved)) {
                self.set_block_entity(pos, entity);
            }
        }
        // `setLoaded(true)` e `registerAllBlockEntitiesAfterLevelLoad`, `:207` e `:210`: primeiro a lista de novos, depois um
        // ticker por block entity, tudo na ordem do mapa.
        let order = self.view.block_entity_positions(chunk);
        for &pos in &order {
            if let Some(id) = self.entity_id(pos) {
                self.fresh.push((pos, id));
            }
        }
        for &pos in &order {
            self.update_ticker(pos);
        }
        self.events.push(SimEvent::ChunkLoad { chunk, new });
        self.hooks.chunk_load(&self.view, chunk, new);
    }

    fn entity_id(&self, pos: BlockPos) -> Option<u64> {
        self.view
            .holders
            .get(&pos.chunk())?
            .chunk
            .as_ref()?
            .entities
            .get(pos)
            .map(|slot| slot.id)
    }

    /// `Level.tickBlockEntities` (`Level.java:490`).
    fn tick_block_entities(&mut self) {
        // `onLoad` de quem entrou desde o último tick e ainda está no mundo.
        for (pos, id) in std::mem::take(&mut self.fresh) {
            if self.entity_id(pos) == Some(id) {
                self.events.push(SimEvent::BlockEntityLoad { pos });
                self.with_entity(pos, |hooks, view, entity| {
                    hooks.block_entity_loaded(view, pos, entity)
                });
            }
        }
        let tickers = std::mem::take(&mut self.tickers);
        let mut alive = Vec::with_capacity(tickers.len());
        for ticker in tickers {
            let chunk = ticker.pos.chunk();
            let bound = self
                .view
                .holders
                .get(&chunk)
                .and_then(|holder| holder.chunk.as_ref())
                .is_some_and(|level_chunk| {
                    level_chunk.tickers.get(&ticker.pos) == Some(&ticker.id)
                });
            // `ticker.isRemoved()`, `Level.java:516`: o ticker sai da lista.
            if !bound {
                continue;
            }
            // `shouldTickBlocksAt`, `Level.java:518`, e `LevelChunk.isTicking`, `LevelChunk.java:415`.
            let ticking = self.view.should_tick_blocks_at(chunk)
                && self
                    .view
                    .ticket_level(chunk)
                    .is_some_and(|level| level <= BLOCK_TICKING_LEVEL);
            if ticking {
                let pos = ticker.pos;
                self.events.push(SimEvent::BlockEntityTick { pos });
                self.with_entity(pos, |hooks, view, entity| {
                    hooks.block_entity_tick(view, pos, entity)
                });
            }
            alive.push(ticker);
        }
        alive.append(&mut self.tickers);
        self.tickers = alive;
    }
}
