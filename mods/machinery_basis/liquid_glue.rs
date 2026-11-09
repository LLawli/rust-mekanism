// Os fluidos de mod como bloco de líquido, ligados ao Horizon: o fluxo (`machinery_basis_core::liquid`), o balde e o dispensador. O motor
// só conhece água e lava (`FluidKind`), então um fluido de mod é um bloco com a propriedade `level` do `LiquidBlock` e a propriedade `up`
// (o mesmo fluido na célula de cima, que a altura do modelo lê), e o `FlowingFluid` do original roda aqui por ticks agendados do bloco.
use crate::game::{Game, GameLevel};
use horizon_client::items::ItemStack;
use horizon_core::geometry::{Axis, Direction};
use horizon_core::position::BlockPos;
use horizon_world::level::{LevelAccess, Ticks};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::liquid::flow;
use machinery_basis_core::liquid::shape::{OcclusionCache, PixelShape};
use machinery_basis_core::liquid::{
    BlockKind, BlockView, CollisionShape, Fluid, FluidState, LegacyBlock, LiquidId, LiquidParams, LiquidWorld,
    MAX_LEGACY_LEVEL,
};
use machinery_basis_core::liquid_data::{self, LEVEL_PROPERTY, LiquidSpec, Liquids, UP_PROPERTY};
use machinery_basis_core::sides::Face;
use std::sync::{Arc, Mutex};

const EMPTY_BUCKET: &str = "minecraft:bucket";
const WATER: &str = "minecraft:water";
const LAVA: &str = "minecraft:lava";
/// `Block.UPDATE_ALL_IMMEDIATE`, as flags do balde e da coleta.
const BUCKET_FLAGS: i32 = 11;
const CREATIVE: i32 = 1;
/// O alcance do raio do balde vazio, em passos de 0,05 como o `pick_fluid` do Horizon.
const RAY_STEP: f64 = 0.05;

/// Posições onde o jogo trocou um bloco de fluido ou avisou um vizinho, esperando o fim da operação para agendar o tick
/// (`LiquidBlock.onPlace` e `neighborChanged`): os dois ganchos só têm `&GameLevel`, sem a fila de ticks.
static PENDING: Mutex<Vec<(&'static str, [i32; 3])>> = Mutex::new(Vec::new());

fn face_of(direction: Direction) -> Face {
    match direction {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}

fn core_pos(pos: BlockPos) -> machinery_basis_core::liquid::BlockPos {
    machinery_basis_core::liquid::BlockPos::new(pos.x, pos.y, pos.z)
}

fn game_pos(pos: machinery_basis_core::liquid::BlockPos) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

/// O fluido do bloco em `state`, se for um bloco de líquido declarado, e o nível dele.
fn liquid_of(level: &GameLevel<'_>, liquids: &Liquids, state: BlockStateId) -> Option<(usize, u8)> {
    let block = level.registry.state(state)?;
    let index = liquids.index_of_block(&block.name)?;
    let level_value = block
        .properties
        .get(LEVEL_PROPERTY)
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|v| *v <= MAX_LEGACY_LEVEL)?;
    Some((index, level_value))
}

/// O `LiquidId` da água e da lava do vanilla: depois dos fluidos declarados.
fn water_id(liquids: &Liquids) -> LiquidId {
    LiquidId(liquids.len() as u16)
}

fn lava_id(liquids: &Liquids) -> LiquidId {
    LiquidId(liquids.len() as u16 + 1)
}

/// O mundo visto pelo `FlowingFluid`.
struct LevelLiquids<'a, 'g> {
    level: &'a mut GameLevel<'g>,
    ticks: &'a mut Ticks,
    liquids: Arc<Liquids>,
    cache: OcclusionCache,
}

impl<'a, 'g> LevelLiquids<'a, 'g> {
    fn new(level: &'a mut GameLevel<'g>, ticks: &'a mut Ticks) -> Self {
        Self {
            level,
            ticks,
            liquids: liquid_data::current(),
            cache: OcclusionCache::new(),
        }
    }

    fn view_of(&self, state: BlockStateId) -> BlockView {
        let level: &GameLevel<'_> = self.level;
        let b = level.behaviours;
        let (kind, fluid, washed_away) = match liquid_of(level, &self.liquids, state) {
            Some((index, value)) => {
                let id = LiquidId(index as u16);
                (BlockKind::Liquid { liquid: id, level: value }, FluidState::from_legacy_level(id, value), false)
            }
            None => self.foreign_view(state),
        };
        let collision = match b.collision_shape(state) {
            Some(shape) if shape.is_block() => CollisionShape::Block,
            Some(shape) if shape.is_empty_identity() => CollisionShape::Empty,
            Some(_) => CollisionShape::Partial(state.0),
            None => CollisionShape::Partial(state.0),
        };
        let block = b.block(state);
        let washed = washed_away
            || b.tag("minecraft:washed_away_by_fluids").is_some_and(|set| set.contains(block));
        let kind = if matches!(kind, BlockKind::Other) && b.is_air(state) {
            BlockKind::Air
        } else if matches!(kind, BlockKind::Other) && b.is_liquid_container(state) {
            BlockKind::Container
        } else {
            kind
        };
        let mut sturdy = 0u8;
        for face in Face::ALL {
            if b.is_face_sturdy(state, direction_of(face)) {
                sturdy |= 1 << face as u8;
            }
        }
        BlockView {
            state_id: state.0,
            kind,
            fluid,
            collision,
            dynamic_shape: b.collision_shape(state).is_none(),
            washed_away: washed,
            solid: b.is_solid(state),
            blocks_fluid_flow: b.tag("minecraft:blocks_fluid_flow").is_some_and(|set| set.contains(block)),
            sturdy_faces: sturdy,
            ice: false,
        }
    }

    /// A água e a lava do vanilla, e a água dentro de um bloco (`waterlogged`): o fluido da célula que não é de mod.
    fn foreign_view(&self, state: BlockStateId) -> (BlockKind, FluidState, bool) {
        let Some(block) = self.level.registry.state(state) else {
            return (BlockKind::Other, FluidState::Empty, false);
        };
        let amount_level = block.properties.get(LEVEL_PROPERTY).and_then(|v| v.parse::<u8>().ok());
        match (&*block.name, amount_level) {
            (WATER, Some(value)) => {
                let id = water_id(&self.liquids);
                (BlockKind::Liquid { liquid: id, level: value.min(MAX_LEGACY_LEVEL) }, FluidState::from_legacy_level(id, value), true)
            }
            (LAVA, Some(value)) => {
                let id = lava_id(&self.liquids);
                (BlockKind::Liquid { liquid: id, level: value.min(MAX_LEGACY_LEVEL) }, FluidState::from_legacy_level(id, value), true)
            }
            _ if block.properties.get("waterlogged").map(String::as_str) == Some("true") => {
                (BlockKind::Other, FluidState::source(water_id(&self.liquids)), false)
            }
            _ => (BlockKind::Other, FluidState::Empty, false),
        }
    }

    /// O estado que `LegacyBlock` pede, com `up` conforme a célula de cima.
    fn state_for(&self, block: LegacyBlock, pos: Option<machinery_basis_core::liquid::BlockPos>) -> Option<BlockStateId> {
        let level: &GameLevel<'_> = self.level;
        match block {
            LegacyBlock::Air => level.registry.default_state("minecraft:air"),
            LegacyBlock::Liquid { liquid, level: value } => {
                let spec = self.liquids.iter().nth(liquid.0 as usize)?;
                let base = level.registry.default_state(&spec.block)?;
                let with_level = level.native_state_with_property(base, LEVEL_PROPERTY, &value.to_string())?;
                let up = pos.is_some_and(|p| {
                    let above = level.block_state(game_pos(p.relative(Face::Up)));
                    liquid_of(level, &self.liquids, above).is_some_and(|(index, _)| index == liquid.0 as usize)
                });
                level.native_state_with_property(with_level, UP_PROPERTY, if up { "true" } else { "false" })
            }
        }
    }
}

fn direction_of(face: Face) -> Direction {
    match face {
        Face::Down => Direction::Down,
        Face::Up => Direction::Up,
        Face::North => Direction::North,
        Face::South => Direction::South,
        Face::West => Direction::West,
        Face::East => Direction::East,
    }
}

fn pixel_shape(level: &GameLevel<'_>, state: u32) -> PixelShape {
    let boxes: Vec<[u8; 6]> = level
        .behaviours
        .collision_shape(BlockStateId(state))
        .map(|shape| {
            shape
                .boxes()
                .iter()
                .map(|b| {
                    let px = |v: f64| (v * 16.0).round().clamp(0.0, 16.0) as u8;
                    [
                        px(b.min(Axis::X)),
                        px(b.min(Axis::Y)),
                        px(b.min(Axis::Z)),
                        px(b.max(Axis::X)),
                        px(b.max(Axis::Y)),
                        px(b.max(Axis::Z)),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    PixelShape::new(&boxes)
}

impl LiquidWorld for LevelLiquids<'_, '_> {
    fn params(&self, liquid: LiquidId) -> LiquidParams {
        if let Some(spec) = self.liquids.iter().nth(liquid.0 as usize) {
            return LiquidParams {
                tick_delay: spec.tick_delay,
                slope_find_distance: spec.slope_find_distance,
                drop_off: spec.drop_off,
                can_convert_to_source: spec.can_convert_to_source,
            };
        }
        if liquid == water_id(&self.liquids) {
            LiquidParams { can_convert_to_source: true, ..LiquidParams::BASE_FLOWING_FLUID }
        } else {
            LiquidParams { tick_delay: 30, slope_find_distance: 2, drop_off: 2, can_convert_to_source: false }
        }
    }

    fn block(&self, pos: machinery_basis_core::liquid::BlockPos) -> BlockView {
        let state = self.level.block_state(game_pos(pos));
        self.view_of(state)
    }

    fn legacy_view(&self, block: LegacyBlock) -> BlockView {
        match self.state_for(block, None) {
            Some(state) => self.view_of(state),
            None => BlockView::air(0),
        }
    }

    fn merged_face_occludes(&self, source: &BlockView, target: &BlockView, direction: Face) -> bool {
        let level: &GameLevel<'_> = self.level;
        PixelShape::merged_face_occludes(&pixel_shape(level, source.state_id), &pixel_shape(level, target.state_id), direction)
    }

    fn occlusion_cache(&self) -> &OcclusionCache {
        &self.cache
    }

    fn can_place_liquid(&self, _pos: machinery_basis_core::liquid::BlockPos, _state: &BlockView, _fluid: Fluid) -> bool {
        // `SimpleWaterloggedBlock.canPlaceLiquid`: só a água entra num bloco que aceita fluido dentro.
        false
    }

    fn place_liquid(&mut self, _pos: machinery_basis_core::liquid::BlockPos, _state: &BlockView, _fluid: FluidState) -> bool {
        false
    }

    fn set_block(&mut self, pos: machinery_basis_core::liquid::BlockPos, block: LegacyBlock, flags: i32) -> bool {
        let Some(state) = self.state_for(block, Some(pos)) else {
            return false;
        };
        let b = self.level.behaviours;
        b.set_block(&mut *self.level, &mut *self.ticks, game_pos(pos), state, flags)
    }

    fn schedule_fluid_tick(&mut self, pos: machinery_basis_core::liquid::BlockPos, fluid: Fluid, delay: i32) {
        let Some(liquid) = fluid.liquid() else {
            return;
        };
        let Some(spec) = self.liquids.iter().nth(liquid.0 as usize) else {
            return;
        };
        self.level.native_schedule_block_tick(&mut *self.ticks, [pos.x, pos.y, pos.z], &spec.block, delay);
    }

    fn drop_resources(&mut self, pos: machinery_basis_core::liquid::BlockPos, _state: &BlockView) {
        // `BaseFlowingFluid.beforeDestroyingBlock`: `Block.dropResources`. O motor só solta ao quebrar o bloco.
        let b = self.level.behaviours;
        b.destroy_block(&mut *self.level, &mut *self.ticks, game_pos(pos), true);
    }
}

/// `ServerLevel.tickFluid`: um tick agendado de um bloco de líquido.
pub(in crate::game) fn tick_block(
    level: &mut GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    handled: &mut bool,
) {
    let liquids = liquid_data::current();
    if liquid_of(level, &liquids, state).is_none() {
        return;
    }
    *handled = true;
    let mut world = LevelLiquids::new(level, ticks);
    let view = world.view_of(state);
    flow::tick_fluid(&mut world, core_pos(pos), view.fluid.fluid());
}

/// `LiquidBlock.updateShape`: agenda o tick quando este bloco ou o vizinho é uma fonte, e mantém `up` (o mesmo fluido em cima).
pub(in crate::game) fn update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    direction: Direction,
    neighbour: BlockStateId,
    new: &mut Option<BlockStateId>,
) {
    let liquids = liquid_data::current();
    let Some((index, value)) = liquid_of(level, &liquids, state) else {
        return;
    };
    let own = FluidState::from_legacy_level(LiquidId(index as u16), value);
    let neighbour_is_source = match liquid_of(level, &liquids, neighbour) {
        Some((other, other_level)) => other_level == 0 && other < liquids.len(),
        None => false,
    };
    if own.is_source() || neighbour_is_source {
        if let Some(spec) = liquids.iter().nth(index) {
            level.native_schedule_block_tick(ticks, [pos.x, pos.y, pos.z], &spec.block, spec.tick_delay);
        }
    }
    if direction == Direction::Up {
        let same = liquid_of(level, &liquids, neighbour).is_some_and(|(other, _)| other == index);
        if let Some(changed) = level.native_state_with_property(state, UP_PROPERTY, if same { "true" } else { "false" }) {
            if changed != state {
                *new = Some(changed);
            }
        }
    }
}

/// O jogo trocou um bloco para um de líquido (`LiquidBlock.onPlace`) ou avisou um vizinho (`neighborChanged`): o tick espera o fim da
/// operação, quando há fila de ticks.
pub(in crate::game) fn note(level: &GameLevel<'_>, dimension: &'static str, pos: BlockPos, state: BlockStateId) {
    let liquids = liquid_data::current();
    if liquid_of(level, &liquids, state).is_some() {
        PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((dimension, [pos.x, pos.y, pos.z]));
    }
}

/// Agenda os ticks anotados em `note`, no nível onde eles estão.
pub(in crate::game) fn flush(game: &mut Game<'_>) {
    let pending = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    if pending.is_empty() {
        return;
    }
    let here = game.native_dimension();
    let mut later = Vec::new();
    for entry in pending {
        if entry.0 != here {
            later.push(entry);
            continue;
        }
        let pos = entry.1;
        game.with_level(|_, level, ticks| {
            let liquids = liquid_data::current();
            let state = level.block_state(BlockPos::new(pos[0], pos[1], pos[2]));
            let Some((index, _)) = liquid_of(level, &liquids, state) else {
                return;
            };
            if let Some(spec) = liquids.iter().nth(index) {
                level.native_schedule_block_tick(ticks, pos, &spec.block, spec.tick_delay);
            }
        });
    }
    if !later.is_empty() {
        PENDING.lock().unwrap_or_else(|e| e.into_inner()).extend(later);
    }
}

// ---- O balde ----

fn spec_of_bucket(item: &str) -> Option<LiquidSpec> {
    liquid_data::current().by_bucket(item).cloned()
}

fn is_creative(game: &Game<'_>) -> bool {
    game.native_acting_player().is_some_and(|player| player.game_mode == CREATIVE)
}

/// O estado de fonte (`level` 0) do bloco do fluido em `pos`, com `up` conforme a célula de cima.
fn source_state(game: &mut Game<'_>, spec: &LiquidSpec, pos: [i32; 3]) -> Option<BlockStateId> {
    let block = spec.block.clone();
    game.with_level(|_, level, _| {
        let liquids = liquid_data::current();
        let base = level.registry.default_state(&block)?;
        let above = level.block_state(BlockPos::new(pos[0], pos[1] + 1, pos[2]));
        let same = liquids.index_of_block(&block).is_some_and(|i| liquid_of(level, &liquids, above).is_some_and(|(o, _)| o == i));
        level.native_state_with_property(base, UP_PROPERTY, if same { "true" } else { "false" })
            .or(Some(base))
    })
}

/// `BucketItem.emptyContents` para o balde de um fluido de mod: a célula aceita o fluido quando é ar, ou substituível, ou não sólida
/// (`BlockState.canBeReplaced(Fluid)`). Devolve se pôs.
fn empty_contents(game: &mut Game<'_>, spec: &LiquidSpec, pos: [i32; 3]) -> bool {
    let at = BlockPos::new(pos[0], pos[1], pos[2]);
    let (replaceable, air, own_liquid, own_source) = game.with_level(|_, level, _| {
        let state = level.block_state(at);
        let b = level.behaviours;
        let liquids = liquid_data::current();
        let own = liquid_of(level, &liquids, state);
        (b.is_replaceable(state) || !b.is_solid(state), b.is_air(state), own.is_some(), own.is_some_and(|(_, v)| v == 0))
    });
    if !(air || replaceable) {
        return false;
    }
    let Some(state) = source_state(game, spec, pos) else {
        return false;
    };
    if !air && !own_liquid {
        // `if (!level.isClientSide() && mayReplace && !blockState.liquid()) level.destroyBlock(pos, true)`
        game.with_level(|_, level, ticks| {
            let b = level.behaviours;
            b.destroy_block(level, ticks, at, true);
        });
    }
    let changed = game.with_level(|_, level, ticks| {
        let b = level.behaviours;
        b.set_block(level, ticks, at, state, BUCKET_FLAGS)
    });
    let _ = game.process_level_events();
    // `if (!level.setBlock(...) && !blockState.getFluidState().isSource()) return false`
    if !changed && !own_source {
        return false;
    }
    game.audio.play(&spec.empty_sound, Some([pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5]), 1.0, 1.0);
    game.post_block_game_event("minecraft:fluid_place", pos, Some(horizon_client::mobs::Who::Player), None);
    true
}

/// O balde cheio de um fluido de mod, o uso do item (`BucketItem.use`): esvazia na célula que a face clicada leva.
fn use_filled(game: &mut Game<'_>, stack: &ItemStack, spec: &LiquidSpec, claimed: &mut bool) {
    // `ItemNutritionalPasteBucket.use`: com fome o balde é bebido, e só sem fome ele esvazia.
    if spec.drink_when_hungry && game.can_eat(stack, false) {
        return;
    }
    let Some(hit) = game.pick() else {
        return;
    };
    if !game.may_build() {
        return;
    }
    let target = hit.beside();
    let mut canceled = false;
    game.native_fill_bucket(stack, target, &mut canceled);
    if canceled || !empty_contents(game, spec, target) {
        return;
    }
    game.trigger_block("minecraft:placed_block", target, stack);
    game.award_stat("used", &stack.item, 1);
    if !is_creative(game) && game.native_take_items(&stack.item, 1) == 1 {
        game.native_give_items(EMPTY_BUCKET, 1);
    }
    *claimed = true;
}

/// O raio do balde vazio (`ClipContext.Fluid.SOURCE_ONLY`): a primeira fonte de fluido de mod antes do primeiro bloco com forma.
fn source_hit(game: &Game<'_>) -> Option<[i32; 3]> {
    let (origin, dir) = game.aim()?;
    let reach = game.reach();
    let block_distance = game
        .pick()
        .map(|hit| (0..3).map(|i| (hit.point[i] - origin[i]).powi(2)).sum::<f64>().sqrt())
        .unwrap_or(reach);
    let liquids = liquid_data::current();
    let mut t = 0.0;
    let mut last = origin.map(|v| v.floor() as i32);
    let mut first = true;
    while t <= reach.min(block_distance) {
        let p = [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t];
        let cell = p.map(|v| v.floor() as i32);
        if first || cell != last {
            let state = game.world.block(cell[0], cell[1], cell[2]);
            if let Some(block) = game.pack.assets.registry.state(state) {
                let source = block.properties.get(LEVEL_PROPERTY).map(String::as_str) == Some("0");
                if source && liquids.by_block(&block.name).is_some() {
                    return Some(cell);
                }
            }
            last = cell;
            first = false;
        }
        t += RAY_STEP;
    }
    None
}

/// O balde vazio apontado para uma fonte (`BucketItem.use` com `Fluids.EMPTY`, `LiquidBlock.pickupBlock`).
fn use_empty(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if liquid_data::current().is_empty() || !game.may_build() {
        return;
    }
    let Some(cell) = source_hit(game) else {
        return;
    };
    let mut canceled = false;
    game.native_fill_bucket(stack, cell, &mut canceled);
    if canceled {
        return;
    }
    let at = BlockPos::new(cell[0], cell[1], cell[2]);
    let taken = game.with_level(|_, level, ticks| {
        let mut world = LevelLiquids::new(level, ticks);
        flow::pickup_block(&mut world, core_pos(at))
    });
    let _ = game.process_level_events();
    let Some(liquid) = taken else {
        return;
    };
    let Some(spec) = liquid_data::current().iter().nth(liquid.0 as usize).cloned() else {
        return;
    };
    game.award_stat("used", EMPTY_BUCKET, 1);
    game.audio.play(&spec.fill_sound, Some([cell[0] as f64 + 0.5, cell[1] as f64 + 0.5, cell[2] as f64 + 0.5]), 1.0, 1.0);
    game.post_block_game_event("minecraft:fluid_pickup", cell, Some(horizon_client::mobs::Who::Player), None);
    if is_creative(game) {
        // `ItemUtils.createFilledResult`: o criativo ganha o balde cheio se ainda não o tem, e o vazio fica.
        if game.native_count_items(&spec.bucket) == 0 {
            game.native_give_items(&spec.bucket, 1);
        }
    } else if game.native_take_items(EMPTY_BUCKET, 1) == 1 {
        game.native_give_items(&spec.bucket, 1);
    }
    *claimed = true;
}

/// O uso do item na mão: o balde cheio de um fluido de mod esvazia, o vazio do vanilla pega a fonte de um.
pub(in crate::game) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if let Some(spec) = spec_of_bucket(&stack.item) {
        use_filled(game, stack, &spec, claimed);
    } else if &*stack.item == EMPTY_BUCKET {
        use_empty(game, stack, claimed);
    }
}

/// `FluidDeferredRegister.BUCKET_DISPENSE_BEHAVIOR`: o dispensador esvazia o balde na célula da frente; se não der, solta o item.
pub(in crate::game) fn dispense(
    game: &mut Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    facing: Direction,
    stack: &ItemStack,
    answer: &mut Option<horizon_client::mod_api::NativeDispensed>,
) {
    let Some(spec) = spec_of_bucket(&stack.item) else {
        return;
    };
    let offset = face_of(facing).offset();
    let target = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    if empty_contents(game, &spec, target) {
        *answer = Some((Some(ItemStack::new(EMPTY_BUCKET, 1)), 1000));
    } else {
        *answer = Some((game.native_dispense_default(dimension, pos, facing, stack.clone()), 1000));
    }
}
