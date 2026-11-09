//! As regras do `FlowingFluid` (`FlowingFluid.java`, FF) e o que o `LiquidBlock` (LB) faz quando o jogo o chama. Cada função recebe o
//! fluido que roda (`this` no Java, um [`LiquidId`]: a fonte e o corrente dele dividem as regras) e lê o mundo pelo [`LiquidWorld`].
use super::shape::OcclusionKey;
use super::{
    BlockKind, BlockPos, BlockView, CollisionShape, Fluid, FluidState, LegacyBlock, LiquidId,
    LiquidWorld, UPDATE_ALL_IMMEDIATE,
};
use crate::neighbors::flags::UPDATE_ALL;
use crate::sides::Face;
use std::collections::HashMap;

/// `Direction.Plane.HORIZONTAL` (`Direction.java:532`): a ordem em que o fluido olha os lados.
pub const HORIZONTAL: [Face; 4] = [Face::North, Face::East, Face::South, Face::West];

/// A ordem em que `spreadToSides` percorre o `EnumMap` de `getSpread` (FF:151): o `ordinal` de `Direction`.
pub const SPREAD_ORDER: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];

/// O `lowest` inicial de `getSpread` e `getSlopeDistance` (FF:286,362).
const NO_SLOPE: i32 = 1000;

/// `getFlow`: o `- 0.8888889F` da altura do vizinho de baixo (FF:72).
const BELOW_NEIGHBOR_OFFSET: f32 = 0.888_888_9;

/// `getFlow`: a queda que entra no vetor quando o fluido cai junto a uma face sólida (FF:92).
const FALLING_PULL: f64 = -6.0;

/// `Vec3.normalize` (`Vec3.java:87-90`): abaixo de `1.0E-5F` o vetor vira zero.
const NORMALIZE_EPSILON: f64 = 1.0e-5_f32 as f64;

/// `ServerLevel.tickFluid` (`ServerLevel.java:842-848`): o tick agendado de `fluid` em `pos` só roda se a célula ainda tem esse fluido
/// (a fonte e o corrente são fluidos diferentes).
pub fn tick_fluid<W: LiquidWorld>(world: &mut W, pos: BlockPos, fluid: Fluid) {
    let state = world.block(pos);
    if state.fluid.fluid() != fluid {
        return;
    }
    if let Some(liquid) = fluid.liquid() {
        tick(world, liquid, pos, state);
    }
}

/// `FlowingFluid.tick` (FF:421-439).
pub fn tick<W: LiquidWorld>(world: &mut W, liquid: LiquidId, pos: BlockPos, block: BlockView) {
    let mut state = block;
    let mut fluid_state = block.fluid;
    if !fluid_state.is_source() {
        let current = world.block(pos);
        let new_fluid_state = get_new_liquid(world, liquid, pos, &current);
        // getSpreadDelay (FF:417-419) é o getTickDelay.
        let tick_delay = world.params(liquid).tick_delay;
        if new_fluid_state.is_empty() {
            fluid_state = new_fluid_state;
            state = world.legacy_view(LegacyBlock::Air);
            world.set_block(pos, LegacyBlock::Air, UPDATE_ALL);
        } else if new_fluid_state != fluid_state {
            fluid_state = new_fluid_state;
            let legacy = fluid_state.legacy_block();
            state = world.legacy_view(legacy);
            world.set_block(pos, legacy, UPDATE_ALL);
            world.schedule_fluid_tick(pos, fluid_state.fluid(), tick_delay);
        }
    }
    spread(world, liquid, pos, &state, fluid_state);
}

/// `FlowingFluid.spread` (FF:117-140).
pub fn spread<W: LiquidWorld>(
    world: &mut W,
    liquid: LiquidId,
    pos: BlockPos,
    state: &BlockView,
    fluid_state: FluidState,
) {
    if fluid_state.is_empty() {
        return;
    }
    let below_pos = pos.relative(Face::Down);
    let below_state = world.block(below_pos);
    let below_fluid = below_state.fluid;
    if can_maybe_pass_through(
        world,
        liquid,
        pos,
        state,
        Face::Down,
        below_pos,
        &below_state,
        below_fluid,
    ) {
        let new_below_fluid = get_new_liquid(world, liquid, below_pos, &below_state);
        let new_below_type = new_below_fluid.fluid();
        if can_be_replaced_with(below_fluid, new_below_type, Face::Down)
            && can_hold_specific_fluid(world, below_pos, &below_state, new_below_type)
        {
            spread_to(world, below_pos, &below_state, new_below_fluid);
            if source_neighbor_count(world, liquid, pos) >= 3 {
                spread_to_sides(world, liquid, pos, fluid_state, state);
            }
            return;
        }
    }
    if fluid_state.is_source() || !is_water_hole(world, liquid, pos, state, below_pos, &below_state)
    {
        spread_to_sides(world, liquid, pos, fluid_state, state);
    }
}

/// `FlowingFluid.spreadToSides` (FF:142-158).
fn spread_to_sides<W: LiquidWorld>(
    world: &mut W,
    liquid: LiquidId,
    pos: BlockPos,
    fluid_state: FluidState,
    state: &BlockView,
) {
    let mut neighbor = fluid_state.amount() - world.params(liquid).drop_off;
    if fluid_state.falling() {
        neighbor = 7;
    }
    if neighbor > 0 {
        let spreads = get_spread(world, liquid, pos, state);
        for (direction, new_neighbor_fluid) in spreads {
            let neighbor_pos = pos.relative(direction);
            let neighbor_state = world.block(neighbor_pos);
            spread_to(world, neighbor_pos, &neighbor_state, new_neighbor_fluid);
        }
    }
}

/// `FlowingFluid.getNewLiquid` (FF:160-195): o fluido que `pos` deve ter pelos vizinhos.
pub fn get_new_liquid<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    pos: BlockPos,
    state: &BlockView,
) -> FluidState {
    let mut highest_neighbor = 0;
    let mut neighbour_sources = 0;
    for direction in HORIZONTAL {
        let relative_pos = pos.relative(direction);
        let block_state = world.block(relative_pos);
        let fluid_state = block_state.fluid;
        if fluid_state.fluid().is_same_as(liquid)
            && can_pass_through_wall(world, direction, pos, state, relative_pos, &block_state)
        {
            if fluid_state.is_source() && can_create_fluid_source(world, &block_state) {
                neighbour_sources += 1;
            }
            highest_neighbor = highest_neighbor.max(fluid_state.amount());
        }
    }
    if neighbour_sources >= 2 {
        let below_state = world.block(pos.relative(Face::Down));
        if below_state.solid || below_state.fluid.is_source_of(liquid) {
            return FluidState::source(liquid);
        }
    }
    let above_pos = pos.relative(Face::Up);
    let above_state = world.block(above_pos);
    let above_fluid = above_state.fluid;
    if !above_fluid.is_empty()
        && above_fluid.fluid().is_same_as(liquid)
        && can_pass_through_wall(world, Face::Up, pos, state, above_pos, &above_state)
    {
        return FluidState::flowing(liquid, 8, true);
    }
    let amount = highest_neighbor - world.params(liquid).drop_off;
    if amount <= 0 {
        FluidState::Empty
    } else {
        // `getFlowing(amount, false)`: o `LEVEL_FLOWING` vai de 1 a 8, e o `setValue` do Java recusa o resto. Com `drop_off >= 0`,
        // `amount` nunca passa de 8.
        debug_assert!(amount <= 8, "LEVEL_FLOWING fora de 1 a 8: {amount}");
        FluidState::flowing(liquid, amount as u8, false)
    }
}

/// `EventHooks.canCreateFluidSource` (`EventHooks.java:739-741`): o `CreateFluidSourceEvent` começa com
/// `state.getFluidState().canConvertToSource(level, pos)`, que o `BaseFlowingFluid` passa ao `FluidType` (BFF:83-85).
fn can_create_fluid_source<W: LiquidWorld>(world: &W, state: &BlockView) -> bool {
    state
        .fluid
        .liquid()
        .is_some_and(|liquid| world.params(liquid).can_convert_to_source)
}

/// `FlowingFluid.canPassThroughWall` (FF:197-246), com o `OCCLUSION_CACHE`.
pub fn can_pass_through_wall<W: LiquidWorld>(
    world: &W,
    direction: Face,
    _source_pos: BlockPos,
    source_state: &BlockView,
    _target_pos: BlockPos,
    target_state: &BlockView,
) -> bool {
    if target_state.collision == CollisionShape::Block {
        return false;
    }
    if source_state.collision == CollisionShape::Block {
        return false;
    }
    if source_state.collision == CollisionShape::Empty
        && target_state.collision == CollisionShape::Empty
    {
        return true;
    }
    let cached = !source_state.dynamic_shape && !target_state.dynamic_shape;
    let key = OcclusionKey {
        first: source_state.state_id,
        second: target_state.state_id,
        direction,
    };
    if cached && let Some(result) = world.occlusion_cache().get_and_move_to_first(key) {
        return result;
    }
    let result = !world.merged_face_occludes(source_state, target_state, direction);
    if cached {
        world.occlusion_cache().put(key, result);
    }
    result
}

/// `FlowingFluid.spreadTo` (FF:271-281), com o `beforeDestroyingBlock` do `BaseFlowingFluid` (BFF:88-91).
fn spread_to<W: LiquidWorld>(world: &mut W, pos: BlockPos, state: &BlockView, target: FluidState) {
    if state.kind == BlockKind::Container {
        world.place_liquid(pos, state, target);
    } else {
        if !state.is_air() {
            world.drop_resources(pos, state);
        }
        world.set_block(pos, target.legacy_block(), UPDATE_ALL);
    }
}

/// `FlowingFluid.getSlopeDistance` (FF:285-309).
fn get_slope_distance<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    pos: BlockPos,
    pass: i32,
    from: Face,
    state: &BlockView,
    context: &mut SpreadContext,
) -> i32 {
    let mut lowest = NO_SLOPE;
    for direction in HORIZONTAL {
        if direction == from {
            continue;
        }
        let test_pos = pos.relative(direction);
        let test_state = context.block_state(world, test_pos);
        let test_fluid_state = test_state.fluid;
        if can_pass_through(
            world,
            liquid,
            Fluid::Flowing(liquid),
            pos,
            state,
            direction,
            test_pos,
            &test_state,
            test_fluid_state,
        ) {
            if context.is_hole(world, liquid, test_pos) {
                return pass;
            }
            if pass < world.params(liquid).slope_find_distance {
                let v = get_slope_distance(
                    world,
                    liquid,
                    test_pos,
                    pass + 1,
                    direction.opposite(),
                    &test_state,
                    context,
                );
                if v < lowest {
                    lowest = v;
                }
            }
        }
    }
    lowest
}

/// `FlowingFluid.isWaterHole` (FF:311-317): o fluido pode descer de `top` para `bottom`.
pub fn is_water_hole<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    top_pos: BlockPos,
    top_state: &BlockView,
    bottom_pos: BlockPos,
    bottom_state: &BlockView,
) -> bool {
    if !can_pass_through_wall(
        world,
        Face::Down,
        top_pos,
        top_state,
        bottom_pos,
        bottom_state,
    ) {
        false
    } else if bottom_state.fluid.fluid().is_same_as(liquid) {
        true
    } else {
        can_hold_fluid(world, bottom_pos, bottom_state, Fluid::Flowing(liquid))
    }
}

/// `FlowingFluid.canPassThrough` (FF:319-331).
#[allow(clippy::too_many_arguments)]
fn can_pass_through<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    fluid: Fluid,
    source_pos: BlockPos,
    source_state: &BlockView,
    direction: Face,
    test_pos: BlockPos,
    test_state: &BlockView,
    test_fluid_state: FluidState,
) -> bool {
    can_maybe_pass_through(
        world,
        liquid,
        source_pos,
        source_state,
        direction,
        test_pos,
        test_state,
        test_fluid_state,
    ) && can_hold_specific_fluid(world, test_pos, test_state, fluid)
}

/// `FlowingFluid.canMaybePassThrough` (FF:333-339).
#[allow(clippy::too_many_arguments)]
fn can_maybe_pass_through<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    source_pos: BlockPos,
    source_state: &BlockView,
    direction: Face,
    test_pos: BlockPos,
    test_state: &BlockView,
    test_fluid_state: FluidState,
) -> bool {
    !test_fluid_state.is_source_of(liquid)
        && can_hold_any_fluid(test_state)
        && can_pass_through_wall(
            world,
            direction,
            source_pos,
            source_state,
            test_pos,
            test_state,
        )
}

/// `FlowingFluid.sourceNeighborCount` (FF:347-359): sem olhar a parede.
fn source_neighbor_count<W: LiquidWorld>(world: &W, liquid: LiquidId, pos: BlockPos) -> usize {
    HORIZONTAL
        .iter()
        .filter(|&&direction| {
            world
                .block(pos.relative(direction))
                .fluid
                .is_source_of(liquid)
        })
        .count()
}

/// `FlowingFluid.getSpread` (FF:361-400): as direções para onde o fluido corre, com o fluido de cada uma, na ordem do `EnumMap`.
pub fn get_spread<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    pos: BlockPos,
    state: &BlockView,
) -> Vec<(Face, FluidState)> {
    let mut lowest = NO_SLOPE;
    // O `EnumMap<Direction, FluidState>`: uma vaga por direção horizontal, na ordem de SPREAD_ORDER.
    let mut result: [Option<FluidState>; 4] = [None; 4];
    let mut context: Option<SpreadContext> = None;
    for direction in HORIZONTAL {
        let test_pos = pos.relative(direction);
        let test_state = world.block(test_pos);
        let test_fluid_state = test_state.fluid;
        if !can_maybe_pass_through(
            world,
            liquid,
            pos,
            state,
            direction,
            test_pos,
            &test_state,
            test_fluid_state,
        ) {
            continue;
        }
        let new_fluid = get_new_liquid(world, liquid, test_pos, &test_state);
        if !can_hold_specific_fluid(world, test_pos, &test_state, new_fluid.fluid()) {
            continue;
        }
        let context = context.get_or_insert_with(|| SpreadContext::new(pos));
        let distance = if context.is_hole(world, liquid, test_pos) {
            0
        } else {
            get_slope_distance(
                world,
                liquid,
                test_pos,
                1,
                direction.opposite(),
                &test_state,
                context,
            )
        };
        if distance < lowest {
            result = [None; 4];
        }
        if distance <= lowest {
            if can_be_replaced_with(test_fluid_state, new_fluid.fluid(), direction) {
                result[spread_slot(direction)] = Some(new_fluid);
            }
            lowest = distance;
        }
    }
    SPREAD_ORDER
        .iter()
        .zip(result)
        .filter_map(|(&direction, fluid)| fluid.map(|fluid| (direction, fluid)))
        .collect()
}

fn spread_slot(direction: Face) -> usize {
    SPREAD_ORDER
        .iter()
        .position(|&face| face == direction)
        .expect("getSpread só olha direções horizontais")
}

/// `FluidState.canBeReplacedWith` (FF:125,389), despachado pelo fluido que já está na célula: `EmptyFluid` aceita tudo
/// (`EmptyFluid.java:22-24`); o `BaseFlowingFluid` (BFF:108-112) e a água (`WaterFluid.canBeReplacedWith`) só aceitam outro fluido
/// vindo de cima.
pub fn can_be_replaced_with(existing: FluidState, other: Fluid, direction: Face) -> bool {
    match existing.liquid() {
        None => true,
        Some(liquid) => direction == Face::Down && !other.is_same_as(liquid),
    }
}

/// `FlowingFluid.canHoldAnyFluid` (FF:402-405).
fn can_hold_any_fluid(state: &BlockView) -> bool {
    state.kind == BlockKind::Container || state.washed_away
}

/// `FlowingFluid.canHoldFluid` (FF:407-409).
fn can_hold_fluid<W: LiquidWorld>(
    world: &W,
    pos: BlockPos,
    state: &BlockView,
    new_fluid: Fluid,
) -> bool {
    can_hold_any_fluid(state) && can_hold_specific_fluid(world, pos, state, new_fluid)
}

/// `FlowingFluid.canHoldSpecificFluid` (FF:411-413).
fn can_hold_specific_fluid<W: LiquidWorld>(
    world: &W,
    pos: BlockPos,
    state: &BlockView,
    new_fluid: Fluid,
) -> bool {
    if state.kind == BlockKind::Container {
        world.can_place_liquid(pos, state, new_fluid)
    } else {
        true
    }
}

/// `FlowingFluid.getHeight` (FF:449-452): 1 com o mesmo fluido em cima, senão a altura própria.
pub fn get_height<W: LiquidWorld>(world: &W, fluid_state: FluidState, pos: BlockPos) -> f32 {
    let Some(liquid) = fluid_state.liquid() else {
        // EmptyFluid.getHeight (`EmptyFluid.java:47-49`).
        return 0.0;
    };
    if world
        .block(pos.relative(Face::Up))
        .fluid
        .fluid()
        .is_same_as(liquid)
    {
        1.0
    } else {
        fluid_state.own_height()
    }
}

/// `FlowingFluid.getFlow` (FF:53-99): o vetor da correnteza, normalizado, como `[x, y, z]`.
pub fn get_flow<W: LiquidWorld>(world: &W, pos: BlockPos, fluid_state: FluidState) -> [f64; 3] {
    let Some(liquid) = fluid_state.liquid() else {
        return [0.0; 3];
    };
    let mut flow_x = 0.0f64;
    let mut flow_z = 0.0f64;
    for direction in HORIZONTAL {
        let block_pos = pos.relative(direction);
        let neighbour_fluid = world.block(block_pos).fluid;
        if !affects_flow(liquid, neighbour_fluid) {
            continue;
        }
        let mut neighbor_height = neighbour_fluid.own_height();
        let mut distance = 0.0f32;
        if neighbor_height == 0.0 {
            if !world.block(block_pos).blocks_fluid_flow {
                let below_neighbor_state = world.block(block_pos.relative(Face::Down)).fluid;
                if affects_flow(liquid, below_neighbor_state) {
                    neighbor_height = below_neighbor_state.own_height();
                    if neighbor_height > 0.0 {
                        distance =
                            fluid_state.own_height() - (neighbor_height - BELOW_NEIGHBOR_OFFSET);
                    }
                }
            }
        } else if neighbor_height > 0.0 {
            distance = fluid_state.own_height() - neighbor_height;
        }
        if distance != 0.0 {
            let [step_x, _, step_z] = direction.offset();
            // `direction.getStepX() * distance` é int vezes float: a conta é em float e só depois soma no double.
            flow_x += f64::from(step_x as f32 * distance);
            flow_z += f64::from(step_z as f32 * distance);
        }
    }
    let mut flow = [flow_x, 0.0, flow_z];
    if fluid_state.falling() {
        for direction in HORIZONTAL {
            let block_pos = pos.relative(direction);
            if is_solid_face(world, liquid, block_pos, direction)
                || is_solid_face(world, liquid, block_pos.relative(Face::Up), direction)
            {
                let [x, y, z] = normalize(flow);
                flow = [x, y + FALLING_PULL, z];
                break;
            }
        }
    }
    normalize(flow)
}

/// `FlowingFluid.affectsFlow` (FF:101-103).
fn affects_flow(liquid: LiquidId, neighbour_fluid: FluidState) -> bool {
    neighbour_fluid.is_empty() || neighbour_fluid.fluid().is_same_as(liquid)
}

/// `FlowingFluid.isSolidFace` (FF:105-115).
fn is_solid_face<W: LiquidWorld>(
    world: &W,
    liquid: LiquidId,
    pos: BlockPos,
    direction: Face,
) -> bool {
    let state = world.block(pos);
    if state.fluid.fluid().is_same_as(liquid) {
        false
    } else if direction == Face::Up {
        true
    } else if state.ice {
        false
    } else {
        state.is_face_sturdy(direction)
    }
}

/// `Vec3.normalize` (`Vec3.java:87-90`).
fn normalize([x, y, z]: [f64; 3]) -> [f64; 3] {
    let dist = (x * x + y * y + z * z).sqrt();
    if dist < NORMALIZE_EPSILON {
        [0.0; 3]
    } else {
        [x / dist, y / dist, z / dist]
    }
}

/// `LiquidBlock.onPlace` (LB:352-362): o bloco de fluido posto agenda o tick do próprio fluido. O `shouldSpreadLiquid` (LB:417-439) só
/// muda algo para a lava.
pub fn on_place<W: LiquidWorld>(world: &mut W, pos: BlockPos, state: &BlockView) {
    schedule_own_tick(world, pos, state);
}

/// `LiquidBlock.neighborChanged` (LB:398-408): um vizinho avisou. `FluidInteractionRegistry.canInteract` (LB:400) é falso para todo
/// fluido sem interação registrada (só a lava tem).
pub fn neighbor_changed<W: LiquidWorld>(world: &mut W, pos: BlockPos, state: &BlockView) {
    schedule_own_tick(world, pos, state);
}

/// `LiquidBlock.updateShape` (LB:372-392): a forma de um vizinho mudou. Só agenda se este bloco ou o vizinho é uma fonte (de qualquer
/// fluido). O estado não muda.
pub fn update_shape<W: LiquidWorld>(
    world: &mut W,
    pos: BlockPos,
    state: &BlockView,
    neighbour_state: &BlockView,
) {
    if state.fluid.is_source() || neighbour_state.fluid.is_source() {
        schedule_own_tick(world, pos, state);
    }
}

/// `level.scheduleTick(pos, state.getFluidState().getType(), this.fluid.getTickDelay(level))`.
fn schedule_own_tick<W: LiquidWorld>(world: &mut W, pos: BlockPos, state: &BlockView) {
    let BlockKind::Liquid { liquid, .. } = state.kind else {
        return;
    };
    let delay = world.params(liquid).tick_delay;
    world.schedule_fluid_tick(pos, state.fluid.fluid(), delay);
}

/// `LiquidBlock.pickupBlock` (LB:450-458): o balde só tira a fonte (`LEVEL == 0`), e troca por ar com as flags 11. Devolve o fluido
/// do balde cheio (`fluid.getBucket()`), ou `None` quando o balde sai vazio (`ItemStack.EMPTY`).
pub fn pickup_block<W: LiquidWorld>(world: &mut W, pos: BlockPos) -> Option<LiquidId> {
    let BlockKind::Liquid { liquid, level } = world.block(pos).kind else {
        return None;
    };
    if level == 0 {
        world.set_block(pos, LegacyBlock::Air, UPDATE_ALL_IMMEDIATE);
        Some(liquid)
    } else {
        None
    }
}

/// `FlowingFluid.SpreadContext` (FF:486-519): o cache de estados e de buracos de uma chamada de `getSpread`, pela chave curta de
/// `(x, z)` relativos à origem.
pub struct SpreadContext {
    origin: BlockPos,
    state_cache: HashMap<i16, BlockView>,
    hole_cache: HashMap<i16, bool>,
}

impl SpreadContext {
    fn new(origin: BlockPos) -> Self {
        Self {
            origin,
            state_cache: HashMap::new(),
            hole_cache: HashMap::new(),
        }
    }

    /// `getCacheKey` (FF:514-518): `(relX + 128 & 0xFF) << 8 | relZ + 128 & 0xFF`, sem o `y`, num `short`.
    pub fn cache_key(origin: BlockPos, pos: BlockPos) -> i16 {
        let relative_x = pos.x - origin.x;
        let relative_z = pos.z - origin.z;
        (((relative_x + 128) & 0xFF) << 8 | ((relative_z + 128) & 0xFF)) as i16
    }

    fn block_state<W: LiquidWorld>(&mut self, world: &W, pos: BlockPos) -> BlockView {
        let key = Self::cache_key(self.origin, pos);
        self.block_state_by_key(world, pos, key)
    }

    fn block_state_by_key<W: LiquidWorld>(
        &mut self,
        world: &W,
        pos: BlockPos,
        key: i16,
    ) -> BlockView {
        *self
            .state_cache
            .entry(key)
            .or_insert_with(|| world.block(pos))
    }

    fn is_hole<W: LiquidWorld>(&mut self, world: &W, liquid: LiquidId, pos: BlockPos) -> bool {
        let key = Self::cache_key(self.origin, pos);
        if let Some(&hole) = self.hole_cache.get(&key) {
            return hole;
        }
        let state = self.block_state_by_key(world, pos, key);
        let below = pos.relative(Face::Down);
        let below_state = world.block(below);
        let hole = is_water_hole(world, liquid, pos, &state, below, &below_state);
        self.hole_cache.insert(key, hole);
        hole
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::LiquidParams;
    use crate::liquid::shape::PixelShape;
    use crate::liquid::sim::{SimBlockType, SimCell, SimLiquid, SimLiquidWorld, SimShape};
    use crate::neighbors::flags::UPDATE_CLIENTS;

    const MOD: LiquidId = LiquidId(0);
    const WATERY: LiquidId = LiquidId(1);
    const STONE: usize = 0;
    /// Um bloco que aceita a fonte de `WATERY` dentro, com uma placa de 3 pixels no lado sul.
    const PANEL: usize = 1;
    /// Um bloco sem colisão que aceita a fonte de `WATERY` dentro, como a placa de madeira.
    const SIGN: usize = 2;
    const SIZE: i32 = 21;

    fn world() -> SimLiquidWorld {
        let liquids = vec![
            SimLiquid {
                name: "test:mod".into(),
                params: LiquidParams::BASE_FLOWING_FLUID,
                washed_away: false,
            },
            SimLiquid {
                name: "test:watery".into(),
                params: LiquidParams {
                    can_convert_to_source: true,
                    ..LiquidParams::BASE_FLOWING_FLUID
                },
                washed_away: true,
            },
        ];
        let panel = SimBlockType {
            collision: SimShape::Pixels(PixelShape::new(&[[0, 0, 13, 16, 16, 16]])),
            container: Some(WATERY),
            sturdy_faces: 0,
            ..SimBlockType::full("test:panel")
        };
        let sign = SimBlockType {
            container: Some(WATERY),
            washed_away: false,
            ..SimBlockType::washable("test:sign", &[])
        };
        let blocks = vec![SimBlockType::full("test:stone"), panel, sign];
        let mut world = SimLiquidWorld::new(liquids, blocks);
        for x in 0..SIZE {
            for z in 0..SIZE {
                world.put(BlockPos::new(x, 0, z), SimCell::block(STONE));
            }
        }
        world
    }

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn place(world: &mut SimLiquidWorld, pos: BlockPos, liquid: LiquidId) {
        world.set_cell(pos, SimCell::Liquid { liquid, level: 0 }, UPDATE_ALL);
    }

    fn run(world: &mut SimLiquidWorld, ticks: usize) {
        for _ in 0..ticks {
            world.tick();
        }
    }

    fn fluid(world: &SimLiquidWorld, pos: BlockPos) -> FluidState {
        world.block(pos).fluid
    }

    #[test]
    fn a_source_on_a_flat_floor_loses_one_level_per_block() {
        let mut world = world();
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 100);
        for d in 1..8 {
            assert_eq!(
                fluid(&world, at(10 + d, 1, 10)),
                FluidState::flowing(MOD, 8 - d as u8, false),
                "distância {d}"
            );
        }
        assert_eq!(fluid(&world, at(18, 1, 10)), FluidState::Empty);
        assert_eq!(
            fluid(&world, at(13, 1, 13)),
            FluidState::flowing(MOD, 2, false)
        );
        assert_eq!(world.pending_fluid_ticks(), 0);
    }

    #[test]
    fn the_first_spread_waits_the_tick_delay() {
        let mut world = world();
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 4);
        assert_eq!(fluid(&world, at(11, 1, 10)), FluidState::Empty);
        run(&mut world, 1);
        assert_eq!(
            fluid(&world, at(11, 1, 10)),
            FluidState::flowing(MOD, 7, false)
        );
    }

    #[test]
    fn spread_to_sides_writes_in_enum_map_order() {
        let mut world = world();
        place(&mut world, at(10, 1, 10), MOD);
        world.writes.clear();
        run(&mut world, 5);
        let order: Vec<BlockPos> = world.writes.iter().map(|(pos, _)| *pos).collect();
        assert_eq!(
            order,
            vec![at(10, 1, 9), at(10, 1, 11), at(9, 1, 10), at(11, 1, 10)]
        );
    }

    #[test]
    fn the_spread_runs_only_towards_the_nearest_hole_within_the_slope_distance() {
        // `getSlopeDistance` olha um bloco além do último passo: com 4 passos, o buraco a 5 blocos da fonte ainda conta.
        let mut world = world();
        world.put(at(15, 0, 10), SimCell::Air);
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 5);
        assert_eq!(
            fluid(&world, at(11, 1, 10)),
            FluidState::flowing(MOD, 7, false)
        );
        assert_eq!(fluid(&world, at(9, 1, 10)), FluidState::Empty);
        assert_eq!(fluid(&world, at(10, 1, 9)), FluidState::Empty);
        assert_eq!(fluid(&world, at(10, 1, 11)), FluidState::Empty);
    }

    #[test]
    fn a_hole_beyond_the_slope_distance_does_not_steer_the_spread() {
        let mut world = world();
        world.put(at(16, 0, 10), SimCell::Air);
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 5);
        for pos in [at(11, 1, 10), at(9, 1, 10), at(10, 1, 9), at(10, 1, 11)] {
            assert_eq!(fluid(&world, pos), FluidState::flowing(MOD, 7, false));
        }
    }

    #[test]
    fn a_source_over_air_falls_as_level_eight() {
        let mut world = world();
        place(&mut world, at(10, 5, 10), MOD);
        run(&mut world, 5);
        assert_eq!(
            world.cell(at(10, 4, 10)),
            SimCell::Liquid {
                liquid: MOD,
                level: 8
            }
        );
        // A fonte que cai não espalha para os lados (só com 3 fontes ao lado, FF:128).
        assert_eq!(fluid(&world, at(11, 5, 10)), FluidState::Empty);
    }

    #[test]
    fn removing_the_source_dries_the_flow() {
        let mut world = world();
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 60);
        world.set_cell(at(10, 1, 10), SimCell::Air, UPDATE_ALL);
        run(&mut world, 100);
        for x in 0..SIZE {
            for z in 0..SIZE {
                assert_eq!(fluid(&world, at(x, 1, z)), FluidState::Empty);
            }
        }
    }

    #[test]
    fn two_sources_make_a_third_only_when_the_fluid_converts() {
        for (liquid, expected) in [
            (MOD, FluidState::flowing(MOD, 7, false)),
            (WATERY, FluidState::source(WATERY)),
        ] {
            let mut world = world();
            place(&mut world, at(9, 1, 10), liquid);
            place(&mut world, at(11, 1, 10), liquid);
            run(&mut world, 40);
            assert_eq!(fluid(&world, at(10, 1, 10)), expected, "{liquid:?}");
        }
    }

    #[test]
    fn another_fluid_is_replaced_only_from_above() {
        let flowing = FluidState::flowing(WATERY, 3, false);
        assert!(can_be_replaced_with(
            flowing,
            Fluid::Flowing(MOD),
            Face::Down
        ));
        assert!(!can_be_replaced_with(
            flowing,
            Fluid::Flowing(MOD),
            Face::North
        ));
        assert!(!can_be_replaced_with(
            flowing,
            Fluid::Flowing(WATERY),
            Face::Down
        ));
        assert!(can_be_replaced_with(
            FluidState::Empty,
            Fluid::Flowing(MOD),
            Face::North
        ));
    }

    #[test]
    fn a_mod_fluid_falls_into_a_washable_fluid_below_but_not_into_its_own_flow() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: WATERY,
                level: 3,
            },
        );
        place(&mut world, at(10, 2, 10), MOD);
        run(&mut world, 5);
        assert_eq!(
            fluid(&world, at(10, 1, 10)),
            FluidState::flowing(MOD, 8, true)
        );

        // A célula corrente do próprio fluido não está na tag: o spread não a reescreve.
        let mut world = self::world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 3,
            },
        );
        place(&mut world, at(10, 2, 10), MOD);
        world.writes.clear();
        let state = world.block(at(10, 2, 10));
        spread(&mut world, MOD, at(10, 2, 10), &state, state.fluid);
        assert!(world.writes.iter().all(|(pos, _)| *pos != at(10, 1, 10)));
    }

    #[test]
    fn the_bucket_only_takes_a_source() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 0,
            },
        );
        world.put(
            at(11, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 3,
            },
        );
        assert_eq!(pickup_block(&mut world, at(11, 1, 10)), None);
        world.put(
            at(12, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 1,
            },
        );
        assert_eq!(pickup_block(&mut world, at(12, 1, 10)), None);
        assert_eq!(
            fluid(&world, at(11, 1, 10)),
            FluidState::flowing(MOD, 5, false)
        );
        assert_eq!(pickup_block(&mut world, at(10, 1, 10)), Some(MOD));
        assert_eq!(world.cell(at(10, 1, 10)), SimCell::Air);
        assert_eq!(pickup_block(&mut world, at(10, 1, 10)), None);
    }

    #[test]
    fn a_legacy_level_above_eight_stays_after_its_tick() {
        let mut world = world();
        world.put(
            at(10, 2, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 0,
            },
        );
        world.set_cell(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 12,
            },
            UPDATE_ALL,
        );
        run(&mut world, 5);
        assert_eq!(
            world.cell(at(10, 1, 10)),
            SimCell::Liquid {
                liquid: MOD,
                level: 12
            }
        );
    }

    #[test]
    fn the_spread_context_key_ignores_y_and_wraps_at_256() {
        let origin = at(0, 64, 0);
        assert_eq!(
            SpreadContext::cache_key(origin, at(3, 10, -2)),
            SpreadContext::cache_key(origin, at(3, 90, -2))
        );
        assert_eq!(
            SpreadContext::cache_key(origin, at(1, 64, 0)),
            SpreadContext::cache_key(origin, at(257, 64, 0))
        );
        assert_ne!(
            SpreadContext::cache_key(origin, at(1, 64, 0)),
            SpreadContext::cache_key(origin, at(0, 64, 1))
        );
    }

    #[test]
    fn the_flow_points_downhill() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 2,
            },
        );
        world.put(
            at(11, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 3,
            },
        );
        world.put(
            at(9, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 1,
            },
        );
        let [x, y, z] = get_flow(&world, at(10, 1, 10), fluid(&world, at(10, 1, 10)));
        assert!(x > 0.99 && y == 0.0 && z.abs() < 1e-9, "{x} {y} {z}");
        assert_eq!(
            get_height(&world, fluid(&world, at(10, 1, 10)), at(10, 1, 10)),
            6.0 / 9.0
        );
    }

    #[test]
    fn a_closer_hole_behind_another_fluid_stops_the_spread_everywhere() {
        // FF:388-394: a direção com outro fluido não entra no mapa, mas baixa o `lowest` e apaga as outras.
        let mut world = world();
        world.put(at(11, 0, 10), SimCell::Air);
        world.put(
            at(11, 1, 10),
            SimCell::Liquid {
                liquid: WATERY,
                level: 3,
            },
        );
        place(&mut world, at(10, 1, 10), MOD);
        run(&mut world, 5);
        for pos in [at(9, 1, 10), at(10, 1, 9), at(10, 1, 11)] {
            assert_eq!(fluid(&world, pos), FluidState::Empty, "{pos:?}");
        }
    }

    #[test]
    fn a_falling_source_spreads_to_the_sides_only_with_three_sources_around() {
        let mut world = world();
        // Três fontes em volta de (10, 5, 10), todas sobre o ar.
        for pos in [at(10, 5, 10), at(9, 5, 10), at(11, 5, 10), at(10, 5, 9)] {
            place(&mut world, pos, MOD);
        }
        // Duas fontes em volta de (4, 5, 4).
        for pos in [at(4, 5, 4), at(3, 5, 4), at(4, 5, 3)] {
            place(&mut world, pos, MOD);
        }
        run(&mut world, 5);
        assert_eq!(
            fluid(&world, at(10, 5, 11)),
            FluidState::flowing(MOD, 7, false)
        );
        assert_eq!(fluid(&world, at(4, 5, 5)), FluidState::Empty);
        assert_eq!(fluid(&world, at(5, 5, 4)), FluidState::Empty);
    }

    #[test]
    fn update_shape_schedules_only_next_to_a_source() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 3,
            },
        );
        // `setBlock(..., 2)`: só as formas dos vizinhos são avisadas.
        world.set_cell(at(11, 1, 10), SimCell::block(STONE), UPDATE_CLIENTS);
        assert_eq!(world.pending_fluid_ticks(), 0);
        world.set_cell(
            at(9, 1, 10),
            SimCell::Liquid {
                liquid: WATERY,
                level: 0,
            },
            UPDATE_CLIENTS,
        );
        assert!(world.has_scheduled_fluid_tick(at(10, 1, 10), Fluid::Flowing(MOD)));
    }

    #[test]
    fn a_tick_for_another_fluid_type_does_nothing() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Liquid {
                liquid: MOD,
                level: 0,
            },
        );
        world.schedule_fluid_tick(at(11, 1, 10), Fluid::Flowing(MOD), 1);
        run(&mut world, 1);
        assert_eq!(world.cell(at(11, 1, 10)), SimCell::Air);
    }

    #[test]
    fn a_full_face_of_the_cell_that_holds_the_fluid_closes_that_side() {
        let mut world = world();
        world.put(
            at(10, 1, 10),
            SimCell::Block {
                kind: PANEL,
                logged: true,
            },
        );
        world.schedule_fluid_tick(at(10, 1, 10), Fluid::Source(WATERY), 1);
        run(&mut world, 1);
        for pos in [at(10, 1, 9), at(9, 1, 10), at(11, 1, 10)] {
            assert_eq!(
                fluid(&world, pos),
                FluidState::flowing(WATERY, 7, false),
                "{pos:?}"
            );
        }
        assert_eq!(fluid(&world, at(10, 1, 11)), FluidState::Empty);
    }

    #[test]
    fn the_slope_search_does_not_pass_a_container_that_only_takes_sources() {
        // FF:293: `getSlopeDistance` pergunta pelo corrente (`getFlowing()`), que o bloco recusa.
        // O buraco em (13, 0, 10) só se alcança pelo bloco em (12, 1, 10): as pedras ao lado fecham o caminho em volta.
        let mut world = world();
        world.put(at(12, 1, 10), SimCell::block(SIGN));
        world.put(at(12, 1, 9), SimCell::block(STONE));
        world.put(at(12, 1, 11), SimCell::block(STONE));
        world.put(at(13, 0, 10), SimCell::Air);
        place(&mut world, at(10, 1, 10), WATERY);
        run(&mut world, 5);
        for pos in [at(11, 1, 10), at(9, 1, 10), at(10, 1, 9), at(10, 1, 11)] {
            assert_eq!(
                fluid(&world, pos),
                FluidState::flowing(WATERY, 7, false),
                "{pos:?}"
            );
        }
    }
}
