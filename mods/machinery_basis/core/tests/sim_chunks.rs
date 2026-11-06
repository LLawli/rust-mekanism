//! Os chunks falsos do mundo simulado (`sim::chunks`), com os cenários de chunk do `TransmitterNetworkTest` do Mekanism
//! (`reloadIntermediary` e `inaccessibleNotUnloaded`) e a ordem dos eventos que o jogo dá.
//!
//! O `Registry` abaixo é um dublê de teste do que o Mekanism faz com esses eventos (`TileEntityTransmitter`, o
//! `DimData` do `TransmitterNetworkRegistry`): serve para provar que os eventos do simulador bastam para o cenário. O porte
//! de verdade é a Fase 3.
use machinery_basis_core::sim::chunks::{
    BlockPos, ChunkHooks, ChunkPos, ENTITY_TICKING_LEVEL, INACCESSIBLE_LEVEL, LevelView,
    MAX_VIEW_DISTANCE, SimEvent, SimLevel, UNLOAD_LEVEL,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const CABLE: &str = "test:cable";
const STONE: &str = "test:stone";

/// A ordem em que o `Object2ObjectOpenHashMap` do fastutil 8.5.18 devolve os cabos de x = 16 a 31 (y = 64, z = 0),
/// inseridos em ordem crescente. Saída do `Ref3.java` rodado contra o jar real.
const CHUNK_ONE_ORDER: [i32; 16] = [
    28, 26, 24, 19, 18, 17, 16, 22, 23, 21, 20, 29, 25, 27, 30, 31,
];

/// A ordem depois de um descarregar e carregar: os mesmos cabos entram no mapa novo na ordem em que foram gravados, e as
/// colisões da tabela os acomodam de outro jeito (`Ref4.java`, segunda rodada).
const RELOADED_ORDER: [i32; 16] = [
    28, 26, 24, 16, 17, 18, 19, 22, 23, 21, 20, 29, 25, 27, 30, 31,
];

fn cable(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 0)
}

fn chunk(x: i32) -> ChunkPos {
    ChunkPos::new(x, 0)
}

/// O `TileEntityTransmitter`: o que ele guarda do mundo.
#[derive(Default)]
struct Tile {
    /// `loaded`.
    loaded: bool,
    /// `markJoined`.
    mark_joined: bool,
    /// Quantas vezes `validateAndTakeShare` rodou.
    shares_taken: u32,
    /// O que o NBT leva: sobrevive a descarregar o chunk.
    value: u32,
}

type TileRef = Rc<RefCell<Tile>>;

/// O `TransmitterNetworkRegistry` e o `DimData`, só no que toca chunks e a entrada e saída da rede.
#[derive(Default)]
struct Registry {
    /// `DimData.transmitters`.
    tracked: BTreeMap<ChunkPos, Vec<(BlockPos, TileRef)>>,
    /// `DimData.changedTicketChunks`: uma árvore, na ordem do `long` empacotado.
    changed: BTreeMap<ChunkPos, bool>,
    /// `newOrphanTransmitters` e `invalidTransmitters`.
    orphans: BTreeSet<BlockPos>,
    invalid: BTreeSet<BlockPos>,
    /// Quem está numa rede depois de `assignOrphans`.
    networked: BTreeSet<BlockPos>,
}

impl Registry {
    fn world_join(&mut self, pos: BlockPos, tile: &TileRef, was_present: bool) {
        if !was_present {
            self.tracked
                .entry(pos.chunk())
                .or_default()
                .push((pos, tile.clone()));
        }
        if !tile.borrow().loaded {
            tile.borrow_mut().loaded = true;
            // `registerOrphanTransmitter`.
            if !self.invalid.remove(&pos) {
                self.orphans.insert(pos);
            }
        }
    }

    fn world_separate(&mut self, pos: BlockPos, tile: &TileRef, still_present: bool) {
        if !still_present && let Some(list) = self.tracked.get_mut(&pos.chunk()) {
            list.retain(|(_, tracked)| !Rc::ptr_eq(tracked, tile));
            if list.is_empty() {
                self.tracked.remove(&pos.chunk());
            }
        }
        if tile.borrow().loaded {
            tile.borrow_mut().loaded = false;
            self.invalid.insert(pos);
            self.orphans.remove(&pos);
        }
    }
}

impl ChunkHooks for Registry {
    type BlockEntity = TileRef;
    type Saved = u32;

    fn new_block_entity(
        &mut self,
        _: BlockPos,
        block: &str,
        saved: Option<&u32>,
    ) -> Option<TileRef> {
        (block == CABLE).then(|| {
            Rc::new(RefCell::new(Tile {
                value: saved.copied().unwrap_or(0),
                ..Tile::default()
            }))
        })
    }

    fn save_block_entity(&mut self, _: BlockPos, entity: &mut TileRef) -> u32 {
        entity.borrow().value
    }

    fn block_entity_ticks(&self, block: &str) -> bool {
        block == CABLE
    }

    /// `TransmitterNetworkRegistry.onTicketLevelChange`, `TransmitterNetworkRegistry.java:139`.
    fn ticket_level_updated(
        &mut self,
        _: &LevelView<TileRef>,
        chunk: ChunkPos,
        old: i32,
        new: i32,
    ) {
        let loaded = if old > MAX_VIEW_DISTANCE && new <= MAX_VIEW_DISTANCE {
            true
        } else if new > MAX_VIEW_DISTANCE && old <= MAX_VIEW_DISTANCE {
            false
        } else {
            return;
        };
        if self.tracked.contains_key(&chunk) {
            if self.changed.get(&chunk).copied().unwrap_or(loaded) != loaded {
                self.changed.remove(&chunk);
            } else {
                self.changed.insert(chunk, loaded);
            }
        }
    }

    /// `TileEntityTransmitter.clearRemoved`, `:146`, no servidor.
    fn block_entity_added(&mut self, _: &LevelView<TileRef>, _: BlockPos, entity: &mut TileRef) {
        entity.borrow_mut().mark_joined = true;
    }

    /// `TileEntityTransmitter.onUpdateServer`, `:93`.
    fn block_entity_tick(&mut self, _: &LevelView<TileRef>, pos: BlockPos, entity: &mut TileRef) {
        if entity.borrow().mark_joined {
            self.world_join(pos, entity, false);
            entity.borrow_mut().mark_joined = false;
        }
    }

    /// `TileEntityTransmitter.onChunkUnloaded`, `:156`.
    fn block_entity_chunk_unloaded(
        &mut self,
        _: &LevelView<TileRef>,
        _: BlockPos,
        entity: &mut TileRef,
    ) {
        entity.borrow_mut().shares_taken += 1;
    }

    /// `TileEntityTransmitter.setRemoved`, `:165`.
    fn block_entity_removed(
        &mut self,
        _: &LevelView<TileRef>,
        pos: BlockPos,
        entity: &mut TileRef,
    ) {
        self.world_separate(pos, entity, false);
    }

    /// `TransmitterNetworkRegistry.onTick`, `:126`.
    fn server_tick_post(&mut self, _: &LevelView<TileRef>) {
        // `handleChangedChunks`.
        for (chunk, loaded) in std::mem::take(&mut self.changed) {
            for (pos, tile) in self.tracked.get(&chunk).cloned().unwrap_or_default() {
                // `chunkAccessibilityChange`, `:211`.
                if loaded {
                    self.world_join(pos, &tile, true);
                } else {
                    tile.borrow_mut().shares_taken += 1;
                    self.world_separate(pos, &tile, true);
                }
            }
        }
        // `removeInvalidTransmitters` e `assignOrphans`.
        for pos in std::mem::take(&mut self.invalid) {
            self.networked.remove(&pos);
        }
        for pos in std::mem::take(&mut self.orphans) {
            self.networked.insert(pos);
        }
    }
}

/// O `STRAIGHT_CABLE` do Mekanism: 48 cabos em linha, atravessando os chunks 0, 1 e 2.
fn straight_cable() -> SimLevel<Registry> {
    let mut level = SimLevel::new(Registry::default());
    for x in 0..3 {
        level.set_ticket_level(chunk(x), ENTITY_TICKING_LEVEL);
    }
    level.tick();
    for x in 0..48 {
        assert!(level.set_block(cable(x), Some(CABLE)));
    }
    level.tick_n(3);
    level.take_events();
    level
}

fn all_cables() -> BTreeSet<BlockPos> {
    (0..48).map(cable).collect()
}

fn one_chunk_cables(order: &[i32]) -> Vec<BlockPos> {
    order.iter().map(|&x| cable(x)).collect()
}

fn ticks_of(events: &[SimEvent]) -> Vec<BlockPos> {
    events
        .iter()
        .filter_map(|event| match event {
            SimEvent::BlockEntityTick { pos } => Some(*pos),
            _ => None,
        })
        .collect()
}

#[test]
fn a_linha_de_cabos_entra_na_rede_no_primeiro_tick() {
    let mut level = SimLevel::new(Registry::default());
    for x in 0..3 {
        level.set_ticket_level(chunk(x), ENTITY_TICKING_LEVEL);
    }
    level.tick();
    for x in 0..48 {
        level.set_block(cable(x), Some(CABLE));
    }
    // `clearRemoved` marca `markJoined`; só o primeiro tick do block entity o junta ao registro (`onUpdateServer`).
    assert!(level.hooks.tracked.is_empty());
    level.tick();
    assert_eq!(level.hooks.networked, all_cables());
    assert!(
        (0..48).all(|x| level.view().block_entity(cable(x)).unwrap().borrow().loaded),
        "todo transmissor entrou no mundo"
    );
}

/// `reloadIntermediary`: descarrega o chunk do meio e carrega de novo, e a linha volta inteira.
#[test]
fn reload_intermediary_segue_a_ordem_do_jogo() {
    let mut level = straight_cable();
    let middle = chunk(1);
    let before: Vec<TileRef> = (16..32)
        .map(|x| level.view().block_entity(cable(x)).unwrap().clone())
        .collect();

    // `ChunkData.updateChunkLoading(helper, false, ...)`: o ticket vai a `UNLOAD_LEVEL` e `processUnloads` roda na hora.
    let old_level = level.unload_chunk(middle);
    assert_eq!(old_level, ENTITY_TICKING_LEVEL);
    let order = one_chunk_cables(&CHUNK_ONE_ORDER);
    let mut expected = vec![SimEvent::TicketLevelUpdated {
        chunk: middle,
        old: 31,
        new: UNLOAD_LEVEL,
    }];
    // Grava antes de avisar a descarga (`ChunkMap.java:525` e `:527`).
    expected.extend(order.iter().map(|&pos| SimEvent::BlockEntitySaved { pos }));
    expected.push(SimEvent::ChunkUnload { chunk: middle });
    // `clearAllBlockEntities`: todos recebem `onChunkUnloaded`, e só então `setRemoved`.
    expected.extend(
        order
            .iter()
            .map(|&pos| SimEvent::BlockEntityChunkUnloaded { pos }),
    );
    expected.extend(
        order
            .iter()
            .map(|&pos| SimEvent::BlockEntityRemoved { pos }),
    );
    assert_eq!(level.take_events(), expected);

    // O chunk saiu: nem bloco nem block entity respondem, e os transmissores saíram do registro.
    assert!(!level.view().is_chunk_loaded(middle));
    assert!(!level.view().has_level_chunk(middle));
    assert_eq!(level.view().block_if_loaded(cable(20)), None);
    assert!(level.view().block_entity(cable(20)).is_none());
    assert!(!level.hooks.tracked.contains_key(&middle));
    for tile in &before {
        let tile = tile.borrow();
        assert_eq!(
            tile.shares_taken, 1,
            "validateAndTakeShare no onChunkUnloaded"
        );
        assert!(!tile.loaded);
    }

    // Os vizinhos seguem andando, e a rede sem o meio fica em duas partes.
    level.tick_n(5);
    assert_eq!(
        level
            .take_events()
            .iter()
            .filter(|e| matches!(e, SimEvent::BlockEntityTick { .. }))
            .count(),
        32 * 5
    );
    let without_middle: BTreeSet<BlockPos> = all_cables()
        .into_iter()
        .filter(|pos| pos.chunk() != middle)
        .collect();
    assert_eq!(level.hooks.networked, without_middle);

    // `updateChunkLoading(helper, true, level)`: o holder nasce no nível antigo, mas só é visível no próximo tick.
    level.load_chunk(middle, old_level);
    assert!(
        !level.view().is_chunk_loaded(middle),
        "hasChunk lê o mapa visível"
    );
    assert_eq!(
        level.take_events(),
        vec![SimEvent::TicketLevelUpdated {
            chunk: middle,
            old: UNLOAD_LEVEL,
            new: 31
        }]
    );

    level.tick();
    let mut expected = Vec::new();
    // `runPostLoad`: recria os block entities na ordem em que foram gravados, ainda sem ticker.
    expected.extend(order.iter().map(|&pos| SimEvent::BlockEntityAdded { pos }));
    expected.push(SimEvent::ChunkLoad {
        chunk: middle,
        new: false,
    });
    // `Level.tickBlockEntities`: `onLoad` dos novos, depois os tickers na ordem da lista (os que já estavam, e os novos no fim),
    // tudo na ordem do mapa novo, que não é a mesma.
    let reloaded = one_chunk_cables(&RELOADED_ORDER);
    expected.extend(
        reloaded
            .iter()
            .map(|&pos| SimEvent::BlockEntityLoad { pos }),
    );
    expected.extend(
        (0..16)
            .chain(32..48)
            .map(|x| SimEvent::BlockEntityTick { pos: cable(x) }),
    );
    expected.extend(
        reloaded
            .iter()
            .map(|&pos| SimEvent::BlockEntityTick { pos }),
    );
    expected.push(SimEvent::ServerTickPost {
        game_time: level.view().game_time() - 1,
    });
    assert_eq!(level.take_events(), expected);

    // A linha voltou inteira, com os block entities novos e o que o NBT levou.
    assert!(level.view().is_chunk_loaded(middle));
    assert_eq!(level.hooks.networked, all_cables());
    for (index, old) in before.iter().enumerate() {
        let now = level.view().block_entity(cable(16 + index as i32)).unwrap();
        assert!(
            !Rc::ptr_eq(old, now),
            "o jogo cria um block entity novo ao carregar"
        );
        assert!(now.borrow().loaded);
    }
}

/// `inaccessibleNotUnloaded`: o chunk do meio vira "inacessível mas ainda carregado", a rede se quebra por um cabo e volta.
#[test]
fn inaccessible_not_unloaded_segue_a_ordem_do_jogo() {
    let mut level = straight_cable();
    let middle = chunk(1);
    let before: Vec<TileRef> = (16..32)
        .map(|x| level.view().block_entity(cable(x)).unwrap().clone())
        .collect();

    // `MekGameTestHelper.setChunkLoadLevel`: só o nível do holder muda; nada carrega nem descarrega.
    let old_level = level.set_chunk_load_level(middle, INACCESSIBLE_LEVEL);
    assert_eq!(old_level, ENTITY_TICKING_LEVEL);
    assert_eq!(
        level.take_events(),
        vec![SimEvent::TicketLevelUpdated {
            chunk: middle,
            old: 31,
            new: INACCESSIBLE_LEVEL
        }]
    );
    // O chunk responde a `hasChunk` e continua com o `LevelChunk`, mas o ticket já não simula blocos.
    assert!(level.view().is_chunk_loaded(middle));
    assert!(level.view().has_level_chunk(middle));
    assert!(level.view().block_entity(cable(20)).is_some());
    // O nível de simulação não mudou; quem trava o tick é o do holder (`LevelChunk.isTicking`).
    assert!(level.view().should_tick_blocks_at(middle));

    level.tick_n(5);
    let events = level.take_events();
    let ticked = ticks_of(&events);
    assert_eq!(ticked.len(), 32 * 5);
    assert!(
        ticked.iter().all(|pos| pos.chunk() != middle),
        "o chunk inacessível não anda"
    );
    assert!(
        events.iter().all(|e| !matches!(
            e,
            SimEvent::ChunkUnload { .. }
                | SimEvent::BlockEntityRemoved { .. }
                | SimEvent::BlockEntityChunkUnloaded { .. }
        )),
        "nada foi descarregado"
    );
    // `chunkAccessibilityChange(false)` no fim do tick: pega a parte, sai da rede, mas segue rastreado.
    for tile in &before {
        let tile = tile.borrow();
        assert_eq!(tile.shares_taken, 1);
        assert!(!tile.loaded);
    }
    assert_eq!(level.hooks.tracked.get(&middle).map(Vec::len), Some(16));
    let accessible: BTreeSet<BlockPos> = all_cables()
        .into_iter()
        .filter(|pos| pos.chunk() != middle)
        .collect();
    assert_eq!(level.hooks.networked, accessible);

    // Quebrar um cabo do chunk 0 força a reconstrução da rede (`helper.setBlock(0, 0, 0, AIR)`).
    assert!(level.set_block(cable(0), None));
    assert_eq!(
        level.take_events(),
        vec![
            SimEvent::BlockEntityRemoved { pos: cable(0) },
            SimEvent::BlockChanged {
                pos: cable(0),
                old: Some(CABLE.into()),
                new: None
            },
        ]
    );
    level.tick_n(5);
    level.take_events();
    let mut without_first = accessible.clone();
    without_first.remove(&cable(0));
    assert_eq!(level.hooks.networked, without_first);

    // E o recoloca.
    assert!(level.set_block(cable(0), Some(CABLE)));
    assert_eq!(
        level.take_events(),
        vec![
            SimEvent::BlockEntityAdded { pos: cable(0) },
            SimEvent::BlockChanged {
                pos: cable(0),
                old: None,
                new: Some(CABLE.into())
            },
        ]
    );
    level.tick_n(5);
    level.take_events();
    assert_eq!(level.hooks.networked, accessible);

    // `setChunkLoadLevel(relativeChunk, level)`: o chunk volta ao nível antigo. Entra de novo pelo `chunkAccessibilityChange`.
    assert_eq!(
        level.set_chunk_load_level(middle, old_level),
        INACCESSIBLE_LEVEL
    );
    level.tick_n(5);
    let events = level.take_events();
    assert!(
        events.iter().all(|e| !matches!(
            e,
            SimEvent::ChunkLoad { .. }
                | SimEvent::BlockEntityAdded { .. }
                | SimEvent::BlockEntityRemoved { .. }
        )),
        "o chunk nunca saiu, então não há carga nova"
    );
    assert_eq!(ticks_of(&events).len(), 48 * 5);
    assert_eq!(level.hooks.networked, all_cables());
    for (index, old) in before.iter().enumerate() {
        let now = level.view().block_entity(cable(16 + index as i32)).unwrap();
        assert!(
            Rc::ptr_eq(old, now),
            "são os mesmos block entities de antes"
        );
        assert!(now.borrow().loaded);
        assert_eq!(now.borrow().shares_taken, 1);
    }
}

#[test]
fn o_chunk_so_e_visivel_depois_da_promocao_e_o_evento_de_carga_diz_se_e_novo() {
    let mut level = SimLevel::new(Registry::default());
    level.set_ticket_level(chunk(0), ENTITY_TICKING_LEVEL);
    assert!(!level.view().is_chunk_loaded(chunk(0)));
    assert_eq!(level.view().ticket_level(chunk(0)), Some(31));
    level.tick();
    assert!(level.view().is_chunk_loaded(chunk(0)));
    assert!(level.view().is_block_loaded(cable(3)));
    assert!(
        !level.view().is_block_loaded(cable(16)),
        "outro chunk, fora do mundo carregado"
    );
    assert!(
        !level.view().is_block_loaded(BlockPos::new(0, 400, 0)),
        "acima da altura do mundo"
    );
    assert!(level.take_events().contains(&SimEvent::ChunkLoad {
        chunk: chunk(0),
        new: true
    }));

    level.unload_chunk(chunk(0));
    level.take_events();
    level.load_chunk(chunk(0), 31);
    level.tick();
    assert!(
        level.take_events().contains(&SimEvent::ChunkLoad {
            chunk: chunk(0),
            new: false
        }),
        "um chunk que já foi gravado não é mais novo"
    );
}

#[test]
fn o_que_o_nbt_leva_sobrevive_a_descarregar_o_chunk() {
    let mut level = straight_cable();
    level
        .view()
        .block_entity(cable(20))
        .unwrap()
        .borrow_mut()
        .value = 1234;
    let old = level.unload_chunk(chunk(1));
    level.load_chunk(chunk(1), old);
    level.tick();
    assert_eq!(
        level.view().block_entity(cable(20)).unwrap().borrow().value,
        1234
    );
    assert_eq!(
        level.view().block_entity(cable(21)).unwrap().borrow().value,
        0
    );
}

#[test]
fn voltar_o_nivel_antes_do_process_unloads_cancela_a_descarga() {
    let mut level = straight_cable();
    // 45 coloca o chunk em `toDrop`; voltar a 31 antes do próximo `processUnloads` o tira de lá (`ChunkMap.java:374` e `:376`).
    level.set_chunk_load_level(chunk(1), UNLOAD_LEVEL);
    level.set_chunk_load_level(chunk(1), 31);
    level.tick_n(2);
    let events = level.take_events();
    assert!(events.iter().all(|e| !matches!(
        e,
        SimEvent::ChunkUnload { .. } | SimEvent::BlockEntityRemoved { .. }
    )));
    assert!(level.view().has_level_chunk(chunk(1)));
    // Sem a volta, o mesmo `tick` descarrega sozinho, pela mesma via de `toDrop`.
    level.set_chunk_load_level(chunk(1), UNLOAD_LEVEL);
    level.tick();
    assert!(!level.view().has_level_chunk(chunk(1)));
    assert!(
        level
            .take_events()
            .iter()
            .any(|e| matches!(e, SimEvent::ChunkUnload { chunk: c } if *c == chunk(1)))
    );
}

#[test]
fn mudar_entre_niveis_carregados_avisa_o_ticket_mesmo_assim() {
    let mut level = straight_cable();
    // O jogo posta o evento a cada `updateChunkScheduling`; quem filtra é o Mekanism (`TransmitterNetworkRegistry.java:143`).
    level.set_chunk_load_level(chunk(1), 32);
    assert_eq!(
        level.take_events(),
        vec![SimEvent::TicketLevelUpdated {
            chunk: chunk(1),
            old: 31,
            new: 32
        }]
    );
    assert!(level.hooks.changed.is_empty());
    // Dois lados descarregados: `ChunkMap.updateChunkScheduling` não faz nada nem posta evento.
    assert_eq!(
        level.set_chunk_load_level(chunk(9), UNLOAD_LEVEL),
        UNLOAD_LEVEL
    );
    assert!(level.take_events().is_empty());
}

#[test]
fn trocar_o_bloco_remove_o_block_entity_antes_de_avisar() {
    let mut level = straight_cable();
    assert!(level.set_block(cable(5), Some(STONE)));
    assert_eq!(
        level.take_events(),
        vec![
            SimEvent::BlockEntityRemoved { pos: cable(5) },
            SimEvent::BlockChanged {
                pos: cable(5),
                old: Some(CABLE.into()),
                new: Some(STONE.into())
            },
        ]
    );
    assert!(level.view().block_entity(cable(5)).is_none());
    assert_eq!(level.view().block_if_loaded(cable(5)), Some(STONE));
    assert_eq!(level.view().block_if_loaded(cable(6)), Some(CABLE));
    assert_eq!(
        level.view().block_if_loaded(BlockPos::new(5, 70, 0)),
        Some("minecraft:air")
    );
    // O mesmo bloco de novo é um `setBlock` sem efeito, e o ticker do cabo trocado não anda mais.
    assert!(!level.set_block(cable(5), Some(STONE)));
    level.tick();
    assert_eq!(ticks_of(&level.take_events()).len(), 47);
}

#[test]
fn um_block_entity_novo_recebe_o_on_load_no_proximo_tick_e_anda_no_mesmo() {
    let mut level = straight_cable();
    level.set_block(cable(0), None);
    level.take_events();
    level.set_block(cable(0), Some(CABLE));
    level.take_events();
    level.tick();
    let events = level.take_events();
    let load = events
        .iter()
        .position(|e| *e == SimEvent::BlockEntityLoad { pos: cable(0) })
        .expect("onLoad");
    let tick = events
        .iter()
        .position(|e| *e == SimEvent::BlockEntityTick { pos: cable(0) })
        .expect("tick");
    assert!(load < tick, "onLoad vem antes de qualquer ticker");
    // O ticker novo vai para o fim da lista, depois dos 47 que já estavam.
    assert_eq!(ticks_of(&events).last(), Some(&cable(0)));
}

#[test]
#[should_panic(expected = "não está carregado")]
fn set_block_num_chunk_descarregado_avisa_em_vez_de_carregar_calado() {
    let mut level = SimLevel::new(Registry::default());
    level.set_block(cable(0), Some(CABLE));
}
