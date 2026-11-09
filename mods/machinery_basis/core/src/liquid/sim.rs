//! Mundo de teste do fluido: uma grade de blocos, a fila de ticks de fluido com a ordem de `LevelTicks`, o `Level.setBlock` com os
//! avisos na ordem do `CollectingNeighborUpdater`, e o laço de um tick de servidor (`ServerLevel.tick`: o relógio anda, depois os
//! ticks de fluido devidos rodam).
//!
//! O mundo não conhece nenhum bloco: os blocos que não são ar nem fluido entram como [`SimBlockType`], com o que o fluido pergunta e,
//! quando o bloco reage a um vizinho (uma planta que cai sem chão, a metade de uma porta), uma função de `updateShape`.
use super::flow;
use super::shape::{OcclusionCache, PixelShape};
use super::ticks::{LevelTicks, MAX_TICKS_PER_LEVEL_TICK, NORMAL_PRIORITY, ScheduledTick};
use super::{
    BlockKind, BlockPos, BlockView, CollisionShape, Fluid, FluidState, LegacyBlock, LiquidId,
    LiquidParams, LiquidWorld,
};
use crate::neighbors::UPDATE_ORDER;
use crate::neighbors::flags::{UPDATE_ALL, UPDATE_KNOWN_SHAPE, UPDATE_NEIGHBORS};
use crate::sides::Face;
use std::collections::HashMap;

/// `BlockBehaviour.UPDATE_SHAPE_ORDER` (`BlockBehaviour.java:85-87`).
pub const UPDATE_SHAPE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::North,
    Face::South,
    Face::Down,
    Face::Up,
];

/// O `updateLimit` padrão de `Level.setBlock` (`Level.java`, 512).
pub const UPDATE_LIMIT: i32 = 512;

/// `Block.UPDATE_SUPPRESS_DROPS`.
const UPDATE_SUPPRESS_DROPS: i32 = 32;

/// `Block.UPDATE_SKIP_ON_PLACE`.
const UPDATE_SKIP_ON_PLACE: i32 = 512;

/// `updateFlags & -34`: as flags que `markAndNotifyBlock` passa às formas dos vizinhos (`Level.java:278`).
const NEIGHBOUR_SHAPE_FLAGS_MASK: i32 = -34;

/// Uma célula da grade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SimCell {
    Air,
    /// O `LiquidBlock` de um fluido, com `LEVEL` de 0 a 15.
    Liquid {
        liquid: LiquidId,
        level: u8,
    },
    /// Um bloco do catálogo ([`SimBlockType`]). `logged` é o `WATERLOGGED` de um bloco que aceita fluido dentro.
    Block {
        kind: usize,
        logged: bool,
    },
}

impl SimCell {
    pub const fn block(kind: usize) -> Self {
        SimCell::Block {
            kind,
            logged: false,
        }
    }
}

impl From<LegacyBlock> for SimCell {
    fn from(block: LegacyBlock) -> Self {
        match block {
            LegacyBlock::Air => SimCell::Air,
            LegacyBlock::Liquid { liquid, level } => SimCell::Liquid { liquid, level },
        }
    }
}

/// A forma de colisão de um bloco do catálogo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimShape {
    Empty,
    Block,
    Pixels(PixelShape),
}

/// `updateShape` de um bloco do catálogo: a célula nova (o próprio bloco quando nada muda, ar quando ele cai).
pub type ShapeUpdate =
    fn(world: &SimLiquidWorld, pos: BlockPos, direction: Face, neighbour: SimCell) -> SimCell;

/// Um bloco que não é ar nem fluido.
#[derive(Clone, Debug)]
pub struct SimBlockType {
    pub name: String,
    pub collision: SimShape,
    pub solid: bool,
    pub washed_away: bool,
    pub blocks_fluid_flow: bool,
    pub sturdy_faces: u8,
    pub ice: bool,
    /// `LiquidBlockContainer` que aceita a fonte deste fluido (`SimpleWaterloggedBlock.canPlaceLiquid`: `fluid == Fluids.WATER`).
    pub container: Option<LiquidId>,
    /// O que `Block.dropResources` solta, sem o que é aleatório.
    pub drops: Vec<(String, u32)>,
    pub update_shape: Option<ShapeUpdate>,
}

impl SimBlockType {
    /// Um bloco cheio e sólido, como a pedra.
    pub fn full(name: &str) -> Self {
        Self {
            name: name.to_string(),
            collision: SimShape::Block,
            solid: true,
            washed_away: false,
            blocks_fluid_flow: true,
            sturdy_faces: 0b11_1111,
            ice: false,
            container: None,
            drops: Vec::new(),
            update_shape: None,
        }
    }

    /// Um bloco sem colisão que o fluido leva (`WASHED_AWAY_BY_FLUIDS`), como a tocha e as plantas.
    pub fn washable(name: &str, drops: &[(&str, u32)]) -> Self {
        Self {
            name: name.to_string(),
            collision: SimShape::Empty,
            solid: false,
            washed_away: true,
            blocks_fluid_flow: false,
            sturdy_faces: 0,
            ice: false,
            container: None,
            drops: drops
                .iter()
                .map(|(name, count)| (name.to_string(), *count))
                .collect(),
            update_shape: None,
        }
    }
}

/// Um fluido do mundo de teste.
#[derive(Clone, Debug)]
pub struct SimLiquid {
    pub name: String,
    pub params: LiquidParams,
    /// O bloco do fluido está na tag `WASHED_AWAY_BY_FLUIDS`.
    pub washed_away: bool,
}

/// Um item solto no mundo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimDrop {
    pub pos: BlockPos,
    pub item: String,
    pub count: u32,
}

/// As entradas da pilha do `CollectingNeighborUpdater` (`CollectingNeighborUpdater.java`).
#[derive(Clone, Copy, Debug)]
enum NeighborUpdate {
    /// `MultiNeighborUpdate`: `updateNeighborsAt`, as seis faces em `UPDATE_ORDER`, uma por `runNext`.
    Multi { source: BlockPos, index: usize },
    /// `ShapeUpdate`, com o estado do vizinho guardado na hora do aviso.
    Shape {
        direction: Face,
        neighbour_state: SimCell,
        pos: BlockPos,
        neighbour_pos: BlockPos,
        flags: i32,
        limit: i32,
    },
}

/// O mundo de teste.
pub struct SimLiquidWorld {
    pub liquids: Vec<SimLiquid>,
    pub blocks: Vec<SimBlockType>,
    cells: HashMap<BlockPos, SimCell>,
    fluid_ticks: LevelTicks<Fluid>,
    game_time: i64,
    sub_tick_count: i64,
    occlusion: OcclusionCache,
    stack: Vec<NeighborUpdate>,
    added_this_layer: Vec<NeighborUpdate>,
    update_count: usize,
    pub drops: Vec<SimDrop>,
    /// Cada `setBlock` que mudou uma célula, na ordem.
    pub writes: Vec<(BlockPos, SimCell)>,
}

impl SimLiquidWorld {
    pub fn new(liquids: Vec<SimLiquid>, blocks: Vec<SimBlockType>) -> Self {
        Self {
            liquids,
            blocks,
            cells: HashMap::new(),
            fluid_ticks: LevelTicks::new(),
            game_time: 0,
            sub_tick_count: 0,
            occlusion: OcclusionCache::new(),
            stack: Vec::new(),
            added_this_layer: Vec::new(),
            update_count: 0,
            drops: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn game_time(&self) -> i64 {
        self.game_time
    }

    pub fn cell(&self, pos: BlockPos) -> SimCell {
        self.cells.get(&pos).copied().unwrap_or(SimCell::Air)
    }

    /// Põe a célula sem passar pelo jogo (sem `onPlace`, sem avisos), como um gabarito de estrutura.
    pub fn put(&mut self, pos: BlockPos, cell: SimCell) {
        if cell == SimCell::Air {
            self.cells.remove(&pos);
        } else {
            self.cells.insert(pos, cell);
        }
    }

    pub fn liquid_name(&self, liquid: LiquidId) -> &str {
        &self.liquids[usize::from(liquid.0)].name
    }

    /// O nome do bloco da célula (`BuiltInRegistries.BLOCK.getKey`).
    pub fn name(&self, cell: SimCell) -> &str {
        match cell {
            SimCell::Air => "minecraft:air",
            SimCell::Liquid { liquid, .. } => self.liquid_name(liquid),
            SimCell::Block { kind, .. } => &self.blocks[kind].name,
        }
    }

    /// Os ticks de fluido agendados dentro da caixa.
    pub fn pending_fluid_ticks_in(&self, min: BlockPos, max: BlockPos) -> usize {
        self.fluid_ticks.count_in(min, max)
    }

    pub fn pending_fluid_ticks(&self) -> usize {
        self.fluid_ticks.count()
    }

    pub fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.fluid_ticks.has_scheduled_tick(pos, fluid)
    }

    /// O `ServerLevel.tick` de um tick de servidor (`ServerLevel.java:402-414`): `tickTime` anda o relógio, depois os ticks de fluido
    /// devidos rodam em `ServerLevel.tickFluid`.
    pub fn tick(&mut self) {
        self.game_time += 1;
        let due = self
            .fluid_ticks
            .collect(self.game_time, MAX_TICKS_PER_LEVEL_TICK);
        for tick in due {
            flow::tick_fluid(self, tick.pos, tick.kind);
        }
    }

    /// `Level.setBlock(pos, state, flags)` com o `updateLimit` padrão.
    pub fn set_cell(&mut self, pos: BlockPos, cell: SimCell, flags: i32) -> bool {
        self.set_cell_limited(pos, cell, flags, UPDATE_LIMIT)
    }

    /// `Level.setBlock(pos, state, flags, updateLimit)` (`Level.java:221-252`), `LevelChunk.setBlockState` (`LevelChunk.java:270-330`)
    /// e `Level.markAndNotifyBlock` (`Level.java:256-287`), no que importa ao fluido.
    pub fn set_cell_limited(
        &mut self,
        pos: BlockPos,
        cell: SimCell,
        flags: i32,
        limit: i32,
    ) -> bool {
        let old = self.cell(pos);
        if old == cell {
            return false;
        }
        self.put(pos, cell);
        self.writes.push((pos, cell));
        // `onPlace` do bloco novo (`LevelChunk.java:328-330`). O `affectNeighborsAfterRemoval` do bloco velho não faz nada nos blocos
        // deste mundo.
        if flags & UPDATE_SKIP_ON_PLACE == 0 && matches!(cell, SimCell::Liquid { .. }) {
            let view = self.view(cell);
            flow::on_place(self, pos, &view);
        }
        // markAndNotifyBlock só avisa se a célula ainda é a que foi posta.
        if self.cell(pos) != cell {
            return true;
        }
        if flags & UPDATE_NEIGHBORS != 0 {
            self.add_and_run(NeighborUpdate::Multi {
                source: pos,
                index: 0,
            });
        }
        if flags & UPDATE_KNOWN_SHAPE == 0 && limit > 0 {
            let neighbour_flags = flags & NEIGHBOUR_SHAPE_FLAGS_MASK;
            for direction in UPDATE_SHAPE_ORDER {
                let neighbour = pos.relative(direction);
                self.add_and_run(NeighborUpdate::Shape {
                    direction: direction.opposite(),
                    neighbour_state: cell,
                    pos: neighbour,
                    neighbour_pos: pos,
                    flags: neighbour_flags,
                    limit: limit - 1,
                });
            }
        }
        true
    }

    /// `Level.destroyBlock` (`Level.java:299-318`).
    pub fn destroy_block(&mut self, pos: BlockPos, drop: bool, limit: i32) -> bool {
        let cell = self.cell(pos);
        if cell == SimCell::Air {
            return false;
        }
        if drop {
            let view = self.view(cell);
            self.drop_resources(pos, &view);
        }
        let fluid = self.view(cell).fluid;
        self.set_cell_limited(pos, fluid.legacy_block().into(), UPDATE_ALL, limit)
    }

    /// `CollectingNeighborUpdater.addAndRun` (sem o teto de `maxChainedNeighborUpdates`, que estes cenários não alcançam).
    fn add_and_run(&mut self, update: NeighborUpdate) {
        let running_already = self.update_count > 0;
        self.update_count += 1;
        if running_already {
            self.added_this_layer.push(update);
        } else {
            self.stack.push(update);
            self.run_updates();
        }
    }

    /// `CollectingNeighborUpdater.runUpdates`.
    fn run_updates(&mut self) {
        while !self.stack.is_empty() || !self.added_this_layer.is_empty() {
            let added: Vec<_> = self.added_this_layer.drain(..).rev().collect();
            self.stack.extend(added);
            let mut next = self.stack.pop().expect("a pilha não está vazia");
            loop {
                let more = self.run_next(&mut next);
                if !more {
                    break;
                }
                if !self.added_this_layer.is_empty() {
                    self.stack.push(next);
                    break;
                }
            }
        }
        self.update_count = 0;
    }

    /// `NeighborUpdates.runNext`: faz um aviso e diz se a entrada ainda tem avisos.
    fn run_next(&mut self, update: &mut NeighborUpdate) -> bool {
        match update {
            NeighborUpdate::Multi { source, index } => {
                let neighbour = source.relative(UPDATE_ORDER[*index]);
                *index += 1;
                self.execute_neighbor_changed(neighbour);
                *index < UPDATE_ORDER.len()
            }
            NeighborUpdate::Shape {
                direction,
                neighbour_state,
                pos,
                neighbour_pos,
                flags,
                limit,
            } => {
                self.execute_shape_update(
                    *direction,
                    *pos,
                    *neighbour_pos,
                    *neighbour_state,
                    *flags,
                    *limit,
                );
                false
            }
        }
    }

    /// `NeighborUpdater.executeUpdate`: `neighborChanged` do bloco em `pos`.
    fn execute_neighbor_changed(&mut self, pos: BlockPos) {
        let cell = self.cell(pos);
        if let SimCell::Liquid { .. } = cell {
            let view = self.view(cell);
            flow::neighbor_changed(self, pos, &view);
        }
    }

    /// `NeighborUpdater.executeShapeUpdate` e `Block.updateOrDestroy` (`Block.java:213-225`).
    fn execute_shape_update(
        &mut self,
        direction: Face,
        pos: BlockPos,
        _neighbour_pos: BlockPos,
        neighbour_state: SimCell,
        flags: i32,
        limit: i32,
    ) {
        let current = self.cell(pos);
        let new = match current {
            SimCell::Air => current,
            SimCell::Liquid { .. } => {
                let view = self.view(current);
                let neighbour_view = self.view(neighbour_state);
                flow::update_shape(self, pos, &view, &neighbour_view);
                current
            }
            SimCell::Block { kind, .. } => match self.blocks[kind].update_shape {
                Some(update_shape) => update_shape(self, pos, direction, neighbour_state),
                None => current,
            },
        };
        if new != current {
            if new == SimCell::Air {
                self.destroy_block(pos, flags & UPDATE_SUPPRESS_DROPS == 0, limit);
            } else {
                self.set_cell_limited(pos, new, flags & !UPDATE_SUPPRESS_DROPS, limit);
            }
        }
    }

    fn liquid_state_id(liquid: LiquidId, level: u8) -> u32 {
        1 + u32::from(liquid.0) * 16 + u32::from(level)
    }

    /// O número de estado de um bloco do catálogo, depois dos 16 níveis de cada fluido.
    fn block_state_id(&self, kind: usize, logged: bool) -> u32 {
        1 + self.liquids.len() as u32 * 16 + kind as u32 * 2 + u32::from(logged)
    }

    /// A célula vista pelo fluido.
    pub fn view(&self, cell: SimCell) -> BlockView {
        match cell {
            SimCell::Air => BlockView::air(0),
            SimCell::Liquid { liquid, level } => BlockView::liquid(
                Self::liquid_state_id(liquid, level),
                liquid,
                level,
                self.liquids[usize::from(liquid.0)].washed_away,
            ),
            SimCell::Block { kind, logged } => {
                let block = &self.blocks[kind];
                let collision = match &block.collision {
                    SimShape::Empty => CollisionShape::Empty,
                    SimShape::Block => CollisionShape::Block,
                    SimShape::Pixels(_) => CollisionShape::Partial(kind as u32),
                };
                let fluid = match (logged, block.container) {
                    (true, Some(liquid)) => FluidState::source(liquid),
                    _ => FluidState::Empty,
                };
                BlockView {
                    state_id: self.block_state_id(kind, logged),
                    kind: if block.container.is_some() {
                        BlockKind::Container
                    } else {
                        BlockKind::Other
                    },
                    fluid,
                    collision,
                    dynamic_shape: false,
                    washed_away: block.washed_away,
                    solid: block.solid,
                    blocks_fluid_flow: block.blocks_fluid_flow,
                    sturdy_faces: block.sturdy_faces,
                    ice: block.ice,
                }
            }
        }
    }

    fn pixel_shape(&self, view: &BlockView) -> PixelShape {
        match view.collision {
            CollisionShape::Partial(kind) => match &self.blocks[kind as usize].collision {
                SimShape::Pixels(shape) => shape.clone(),
                _ => PixelShape::default(),
            },
            CollisionShape::Empty => PixelShape::default(),
            CollisionShape::Block => PixelShape::new(&[[0, 0, 0, 16, 16, 16]]),
        }
    }

    /// O bloco do catálogo de um número de estado (o inverso de `block_state_id`).
    fn view_kind(&self, view: &BlockView) -> Option<usize> {
        let base = 1 + self.liquids.len() as u32 * 16;
        view.state_id
            .checked_sub(base)
            .map(|n| (n / 2) as usize)
            .filter(|&kind| kind < self.blocks.len())
    }
}

impl LiquidWorld for SimLiquidWorld {
    fn params(&self, liquid: LiquidId) -> LiquidParams {
        self.liquids[usize::from(liquid.0)].params
    }

    fn block(&self, pos: BlockPos) -> BlockView {
        self.view(self.cell(pos))
    }

    fn legacy_view(&self, block: LegacyBlock) -> BlockView {
        self.view(block.into())
    }

    fn merged_face_occludes(
        &self,
        source: &BlockView,
        target: &BlockView,
        direction: Face,
    ) -> bool {
        PixelShape::merged_face_occludes(
            &self.pixel_shape(source),
            &self.pixel_shape(target),
            direction,
        )
    }

    fn occlusion_cache(&self) -> &OcclusionCache {
        &self.occlusion
    }

    fn can_place_liquid(&self, _pos: BlockPos, state: &BlockView, fluid: Fluid) -> bool {
        self.view_kind(state)
            .and_then(|kind| self.blocks[kind].container)
            .is_some_and(|liquid| fluid == Fluid::Source(liquid))
    }

    /// `SimpleWaterloggedBlock.placeLiquid`: só a fonte do fluido aceito entra, num bloco ainda seco.
    fn place_liquid(&mut self, pos: BlockPos, _state: &BlockView, fluid: FluidState) -> bool {
        let SimCell::Block {
            kind,
            logged: false,
        } = self.cell(pos)
        else {
            return false;
        };
        let Some(liquid) = self.blocks[kind].container else {
            return false;
        };
        if fluid.fluid() != Fluid::Source(liquid) {
            return false;
        }
        self.set_cell(pos, SimCell::Block { kind, logged: true }, UPDATE_ALL);
        let delay = self.params(liquid).tick_delay;
        self.schedule_fluid_tick(pos, fluid.fluid(), delay);
        true
    }

    fn set_block(&mut self, pos: BlockPos, block: LegacyBlock, flags: i32) -> bool {
        self.set_cell(pos, block.into(), flags)
    }

    /// `LevelAccessor.createTick` (`getGameTime() + delay`, `nextSubTickCount()`) e `LevelTicks.schedule`.
    fn schedule_fluid_tick(&mut self, pos: BlockPos, fluid: Fluid, delay: i32) {
        let sub_tick_order = self.sub_tick_count;
        self.sub_tick_count += 1;
        self.fluid_ticks.schedule(ScheduledTick {
            kind: fluid,
            pos,
            trigger_tick: self.game_time + i64::from(delay),
            priority: NORMAL_PRIORITY,
            sub_tick_order,
        });
    }

    fn drop_resources(&mut self, pos: BlockPos, state: &BlockView) {
        let Some(kind) = self.view_kind(state) else {
            return;
        };
        for (item, count) in self.blocks[kind].drops.clone() {
            self.drops.push(SimDrop { pos, item, count });
        }
    }
}
