//! O fluido que corre no mundo: `FlowingFluid` do Minecraft 26.3, com a especialização `BaseFlowingFluid` do NeoForge e as chamadas
//! que o `LiquidBlock` faz ao jogo. Um mod registra vários fluidos assim (o Mekanism registra 22, em `FluidDeferredRegister.java:113-142`),
//! todos com a mesma física: o fluido só muda o id. Aqui o fluido é um índice ([`LiquidId`]) e o que muda de um para outro entra como
//! dado ([`LiquidParams`]); a lib não conhece nenhum id de mod.
//!
//! As regras estão em [`flow`] (`tick`, `spread`, `getNewLiquid`, `getSpread`, `getSlopeDistance`, `canPassThroughWall`, `getFlow`, a
//! coleta e os três avisos do `LiquidBlock`), a fila de ticks agendados (`LevelTicks`) em [`ticks`], a oclusão de faces
//! (`Shapes.mergedFaceOccludes` e o `OCCLUSION_CACHE`) em [`shape`], e um mundo de teste com a ordem do `Level.setBlock` em [`sim`].
//!
//! # O que a cola (quem implementa [`LiquidWorld`]) responde
//!
//! Linhas de `FlowingFluid.java` (FF), `LiquidBlock.java` (LB), `BaseFlowingFluid.java` (BFF), `Level.java` e `LevelChunk.java` do
//! 26.3.0.100.
//!
//! - [`LiquidWorld::params`]: os números do fluido. `getTickDelay` (BFF:115, lido em FF:418 e LB:355,384,401), `getSlopeFindDistance`
//!   (BFF:94, FF:298), `getDropOff` (BFF:99, FF:143,193) e `FluidType.canConvertToSource` (`CreateFluidSourceEvent`, FF:170).
//! - [`LiquidWorld::block`]: `level.getBlockState(pos)` já classificado em [`BlockView`]. Cada campo responde uma pergunta do Java:
//!   `getFluidState()` (FF:121,168,180,188,292,352,369), `isAir()` (FF:275), `instanceof LiquidBlockContainer` (FF:272,404,412),
//!   a tag `WASHED_AWAY_BY_FLUIDS` (FF:404), `getCollisionShape(level, pos)` comparada por identidade com `Shapes.block()` e
//!   `Shapes.empty()` (FF:201-213), `getBlock().hasDynamicShape()` (FF:216), `Block.getId(state)` como identidade do estado na chave do
//!   cache de oclusão (FF:469-484), `isSolid()` (FF:181), e, só para `getFlow`, a tag `BLOCKS_FLUID_FLOW` (FF:66),
//!   `isFaceSturdy` por face (FF:113) e `instanceof IceBlock` (FF:113).
//! - [`LiquidWorld::legacy_view`]: o mesmo [`BlockView`] para o estado que o fluido acabou de escrever (`createLegacyBlock`, FF:432, ou
//!   `Blocks.AIR`, FF:428), porque o Java segue com o estado que calculou e não relê o mundo.
//! - [`LiquidWorld::merged_face_occludes`]: `Shapes.mergedFaceOccludes(sourceShape, targetShape, direction)` (FF:233) com as formas de
//!   colisão dos dois estados. Só é chamada quando nenhuma das duas é `Shapes.block()` e as duas não são `Shapes.empty()`.
//!   [`shape::PixelShape`] faz a conta para formas alinhadas a 1/16.
//! - [`LiquidWorld::occlusion_cache`]: o `OCCLUSION_CACHE` (FF:37-45), que no Java é um `ThreadLocal`: um por thread de servidor.
//! - [`LiquidWorld::can_place_liquid`] e [`LiquidWorld::place_liquid`]: `LiquidBlockContainer.canPlaceLiquid(null, level, pos, state,
//!   fluid)` (FF:412) e `placeLiquid(level, pos, state, fluidState)` (FF:273). Só são chamadas num bloco com
//!   [`BlockKind::Container`].
//! - [`LiquidWorld::set_block`]: `level.setBlock(pos, state, flags)` do jogo, com tudo o que ele faz (`LevelChunk.setBlockState`:
//!   `onPlace` do bloco novo, `LevelChunk.java:328-330`; `Level.markAndNotifyBlock`: `updateNeighborsAt` e `updateNeighbourShapes`,
//!   `Level.java:270-282`). Quando o bloco novo ou um vizinho é um bloco de fluido, o jogo chama [`flow::on_place`],
//!   [`flow::neighbor_changed`] e [`flow::update_shape`]. Chamado por FF:279,429,433 (`setBlockAndUpdate`, flags 3) e LB:453 (flags 11).
//! - [`LiquidWorld::schedule_fluid_tick`]: `level.scheduleTick(pos, fluid, delay)` (FF:434, LB:355,384,401), que cria o
//!   `ScheduledTick` com `getGameTime() + delay` e `nextSubTickCount()` (`LevelAccessor.java:31-32`) e o põe em `LevelTicks`, que
//!   descarta o repetido por `(pos, fluid)` (`LevelChunkTicks.schedule`). [`ticks::LevelTicks`] é o port dessa fila. O tempo do jogo
//!   não aparece no trait: o `FlowingFluid` nunca o lê, só o `createTick` do mundo.
//! - [`LiquidWorld::drop_resources`]: `beforeDestroyingBlock` (BFF:88-91): `Block.dropResources(state, level, pos, blockEntity)`.
//!
//! Do lado do jogo, a cola chama: [`flow::tick_fluid`] para cada tick de fluido que `LevelTicks` entrega (`ServerLevel.tickFluid`,
//! `ServerLevel.java:842-848`), [`flow::on_place`], [`flow::neighbor_changed`] e [`flow::update_shape`] quando o jogo chama esses
//! métodos num `LiquidBlock`, e [`flow::pickup_block`] no `BucketPickup.pickupBlock` (LB:451-458).
//!
//! # Comportamento do original que parece bug
//!
//! Tudo abaixo está portado igual e coberto por teste.
//!
//! - Um bloco de fluido de mod não está na tag `WASHED_AWAY_BY_FLUIDS` (só `minecraft:water` e `minecraft:lava` estão,
//!   `data/minecraft/tags/block/washed_away_by_fluids.json`; nem o Mekanism nem o NeoForge acrescentam os seus). Então
//!   `canHoldAnyFluid` (FF:402-405) é falso numa célula que já tem o fluido corrente do mesmo mod: o `spread` nunca reescreve uma célula
//!   corrente, nem para baixo nem para os lados. Ela só muda no próprio tick (FF:423-435). A água do vanilla, que está na tag, reescreve.
//! - `getSpread` decide em `Direction.Plane.HORIZONTAL` (norte, leste, sul, oeste: FF:366, `Direction.java:532`), mas o resultado é um
//!   `EnumMap` (FF:363) e `spreadToSides` espalha na ordem do `ordinal` (norte, sul, oeste, leste: FF:151). A ordem de escrita decide a
//!   ordem dos ticks agendados (`subTickOrder`) e aparece no rastro `waterfall`; a de decisão não muda nenhum resultado.
//! - `getSpread` só põe no mapa a direção cuja célula aceita o fluido (`canBeReplacedWith`, FF:389), mas o `lowest` cai mesmo quando ela
//!   não aceita (FF:393): uma célula com outro fluido, mais perto de um buraco, apaga as direções boas e não entra.
//! - `sourceNeighborCount` (FF:347-359) conta fontes ao lado sem olhar a parede entre as células (`getFluidState` puro), ao contrário de
//!   `getNewLiquid`, que olha (FF:169).
//! - O `SpreadContext` (FF:486-519) guarda estado e buraco por `(x, z)` relativos à origem, com 8 bits cada (`(rel + 128) & 0xFF`,
//!   FF:514-518), sem o `y`. Com `slope_find_distance` de 128 ou mais, duas células diferentes caem na mesma chave e a segunda lê o
//!   bloco da primeira.
//! - `getLegacyLevel` (FF:441-443) dá 0 (fonte) a um fluido corrente de quantidade 8 que não cai: com `drop_off` 0, o `tick` grava uma
//!   fonte onde calculou um corrente.
//! - `LiquidBlock.getFluidState` (LB:326-330) lê os níveis 8 a 15 como o mesmo corrente que cai (quantidade 8). Um bloco posto com nível
//!   9 a 15 não muda no tick, porque o estado de fluido calculado é igual ao lido (FF:430): o nível fica.
//! - `canConvertToSource` (FF:170) é perguntado ao fluido da célula vizinha, e só depois de ela passar no `isSame` e na parede; o
//!   `BaseFlowingFluid` responde com a propriedade do `FluidType`, falsa no Mekanism. O `belowState.isSolid()` (FF:181) é o `legacySolid`.
//! - `getSlopeDistance` usa o fluido corrente do tipo (`getFlowing()`, FF:293) no `canHoldSpecificFluid`, enquanto o primeiro passo de
//!   `getSpread` usa o fluido calculado por `getNewLiquid` (FF:372).
//! - `tick` lê `getNewLiquid` antes de agendar e grava com `setBlockAndUpdate`, que já agenda o tick pelo `onPlace` (LB:353-356); o
//!   `scheduleTick` de FF:434 sai repetido e `LevelTicks` o descarta, mas gasta um `subTickCount`.
//! - `spreadToSides` troca a quantidade do fluido que cai por 7 (FF:144-146), mas só o `> 0` (FF:148) usa o número: com `drop_off`
//!   abaixo de 8 a troca não muda nada.
//! - `getFlow` normaliza o vetor duas vezes quando o fluido cai junto a uma face sólida (FF:92,98), e soma `-6` depois da primeira.
//!
//! # O que fica de fora
//!
//! A luz, a cor, a textura, o som e as partículas. O que só a lava do vanilla faz (`LavaFluid`: o atraso sorteado de `getSpreadDelay`
//! quando uma célula sobe, `canBeReplacedWith` pela altura, o `spreadTo` que vira pedra sobre a água, o `fizz` e as interações do
//! `FluidInteractionRegistry`): nenhum fluido de mod usa. Os números da lava (atraso 30, 2 passos, perda 2) cabem em [`LiquidParams`],
//! e o resto da física dela é a deste módulo. O `canInteract` do NeoForge em
//! `neighborChanged` (LB:400) só tem interações registradas para a lava (`FluidInteractionRegistry.java:72-82`), então aqui ele é
//! sempre falso. A coluna de bolhas (LB:358-361,387-389,404-407): só a água a forma.
pub mod flow;
pub mod shape;
pub mod sim;
pub mod ticks;

use crate::sides::Face;
pub use crate::sim::chunks::BlockPos;

/// `Block.UPDATE_ALL_IMMEDIATE` (`Block.UPDATE_NEIGHBORS | UPDATE_CLIENTS | UPDATE_IMMEDIATE`): as flags de `LiquidBlock.pickupBlock`
/// (LB:453) e do balde (`BucketItem.java:157`).
pub const UPDATE_ALL_IMMEDIATE: i32 = 11;

/// O nível mais alto da propriedade `LiquidBlock.LEVEL` (`BlockStateProperties.LEVEL`, 0 a 15).
pub const MAX_LEGACY_LEVEL: u8 = 15;

/// Um fluido registrado: o par fonte e corrente que o `BaseFlowingFluid` liga pelo `isSame` (BFF:131-134).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LiquidId(pub u16);

/// O objeto `Fluid` do Java: `Fluids.EMPTY`, a fonte (`BaseFlowingFluid.Source`) ou o corrente (`BaseFlowingFluid.Flowing`) de um
/// fluido. É a chave do tick agendado (`ScheduledTick.type`), então fonte e corrente da mesma célula são ticks diferentes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fluid {
    Empty,
    Source(LiquidId),
    Flowing(LiquidId),
}

impl Fluid {
    /// O fluido registrado, se houver.
    pub fn liquid(self) -> Option<LiquidId> {
        match self {
            Fluid::Empty => None,
            Fluid::Source(liquid) | Fluid::Flowing(liquid) => Some(liquid),
        }
    }

    /// `Fluid.isSame` do fluido `liquid` (BFF:131-134): a fonte ou o corrente dele.
    pub fn is_same_as(self, liquid: LiquidId) -> bool {
        self.liquid() == Some(liquid)
    }
}

/// O `FluidState` de uma célula: vazio, fonte, ou corrente com `LEVEL_FLOWING` (1 a 8). `falling` é a propriedade `FALLING`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FluidState {
    Empty,
    Source {
        liquid: LiquidId,
        falling: bool,
    },
    Flowing {
        liquid: LiquidId,
        amount: u8,
        falling: bool,
    },
}

impl FluidState {
    /// `getSource(false)` (FF:256-258).
    pub const fn source(liquid: LiquidId) -> Self {
        FluidState::Source {
            liquid,
            falling: false,
        }
    }

    /// `getFlowing(amount, falling)` (FF:250-252).
    pub const fn flowing(liquid: LiquidId, amount: u8, falling: bool) -> Self {
        FluidState::Flowing {
            liquid,
            amount,
            falling,
        }
    }

    /// `getType()`.
    pub fn fluid(self) -> Fluid {
        match self {
            FluidState::Empty => Fluid::Empty,
            FluidState::Source { liquid, .. } => Fluid::Source(liquid),
            FluidState::Flowing { liquid, .. } => Fluid::Flowing(liquid),
        }
    }

    pub fn liquid(self) -> Option<LiquidId> {
        self.fluid().liquid()
    }

    pub fn is_empty(self) -> bool {
        self == FluidState::Empty
    }

    /// `isSource()`.
    pub fn is_source(self) -> bool {
        matches!(self, FluidState::Source { .. })
    }

    /// `isSourceBlockOfThisType` (FF:341-343).
    pub fn is_source_of(self, liquid: LiquidId) -> bool {
        self.is_source() && self.liquid() == Some(liquid)
    }

    /// `getAmount()`: 8 na fonte (BFF:166-168), `LEVEL` no corrente (BFF:152-154), 0 no vazio (`EmptyFluid.java:67`).
    pub fn amount(self) -> i32 {
        match self {
            FluidState::Empty => 0,
            FluidState::Source { .. } => 8,
            FluidState::Flowing { amount, .. } => i32::from(amount),
        }
    }

    /// `getValue(FALLING)`. O vazio não tem a propriedade; o Java nunca a lê nele.
    pub fn falling(self) -> bool {
        match self {
            FluidState::Empty => false,
            FluidState::Source { falling, .. } | FluidState::Flowing { falling, .. } => falling,
        }
    }

    /// `getOwnHeight()`: `getAmount() / 9.0F` (FF:455-457); 0 no vazio (`EmptyFluid.java:52`).
    pub fn own_height(self) -> f32 {
        match self {
            FluidState::Empty => 0.0,
            _ => self.amount() as f32 / 9.0,
        }
    }

    /// `LiquidBlock.getFluidState` (LB:326-330) com o `stateCache` do construtor (LB:272-279): 0 é a fonte, 1 a 7 o corrente de
    /// quantidade `8 - nível`, e 8 a 15 o corrente que cai.
    pub fn from_legacy_level(liquid: LiquidId, level: u8) -> Self {
        match level.min(8) {
            0 => FluidState::source(liquid),
            8 => FluidState::flowing(liquid, 8, true),
            level => FluidState::flowing(liquid, 8 - level, false),
        }
    }

    /// `getLegacyLevel` (FF:441-443).
    pub fn legacy_level(self) -> u8 {
        match self {
            FluidState::Empty | FluidState::Source { .. } => 0,
            FluidState::Flowing {
                amount, falling, ..
            } => 8 - amount.min(8) + if falling { 8 } else { 0 },
        }
    }

    /// `createLegacyBlock()`: o bloco do fluido com `LEVEL = getLegacyLevel` (BFF:125-129), ou ar no vazio (`EmptyFluid`).
    pub fn legacy_block(self) -> LegacyBlock {
        match self.liquid() {
            None => LegacyBlock::Air,
            Some(liquid) => LegacyBlock::Liquid {
                liquid,
                level: self.legacy_level(),
            },
        }
    }
}

/// Os números que o `FlowingFluid` pergunta ao fluido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidParams {
    /// `getTickDelay` (BFF:115-117), que também é o `getSpreadDelay` (FF:417-419).
    pub tick_delay: i32,
    /// `getSlopeFindDistance` (BFF:94-96).
    pub slope_find_distance: i32,
    /// `getDropOff` (BFF:99-101).
    pub drop_off: i32,
    /// `FluidType.canConvertToSource` (`FluidType.java:412-414`), lido por `CreateFluidSourceEvent` (FF:170).
    pub can_convert_to_source: bool,
}

impl LiquidParams {
    /// Os valores padrão de `BaseFlowingFluid.Properties` (BFF:181-184) com o `FluidType.Properties` padrão
    /// (`canConvertToSource = false`, `FluidType.java:868`): os de todo fluido do Mekanism.
    pub const BASE_FLOWING_FLUID: Self = Self {
        tick_delay: 5,
        slope_find_distance: 4,
        drop_off: 1,
        can_convert_to_source: false,
    };
}

/// O bloco que o fluido escreve: ar, ou o `LiquidBlock` de um fluido com o nível 0 a 15.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyBlock {
    Air,
    Liquid { liquid: LiquidId, level: u8 },
}

/// O que o fluido precisa saber do tipo do bloco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// `isAir()`: ar, ar de caverna, ar do vazio.
    Air,
    /// Um `LiquidBlock`, com o `LEVEL`.
    Liquid { liquid: LiquidId, level: u8 },
    /// Um `LiquidBlockContainer` (bloco que aceita água dentro, como a escada).
    Container,
    /// Qualquer outro.
    Other,
}

/// A forma de colisão com `CollisionContext.empty()`, como `canPassThroughWall` a compara (FF:201-213).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionShape {
    /// `Shapes.empty()`.
    Empty,
    /// `Shapes.block()`, a mesma instância.
    Block,
    /// Outra forma; o número identifica a forma para a cola.
    Partial(u32),
}

/// Um estado de bloco como o fluido o vê. Ver o cabeçalho do módulo para a linha do Java de cada campo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockView {
    /// `Block.getId(state)`: a identidade do estado no `OCCLUSION_CACHE`. Dois estados com a mesma forma e o mesmo número têm de ser o
    /// mesmo estado.
    pub state_id: u32,
    pub kind: BlockKind,
    /// `getFluidState()`.
    pub fluid: FluidState,
    pub collision: CollisionShape,
    /// `getBlock().hasDynamicShape()`.
    pub dynamic_shape: bool,
    /// A tag `WASHED_AWAY_BY_FLUIDS`.
    pub washed_away: bool,
    /// `isSolid()` (o `legacySolid`).
    pub solid: bool,
    /// A tag `BLOCKS_FLUID_FLOW` (só `getFlow`).
    pub blocks_fluid_flow: bool,
    /// `isFaceSturdy(level, pos, face)` por face, um bit por `Face as u8` (só `getFlow`).
    pub sturdy_faces: u8,
    /// `getBlock() instanceof IceBlock` (só `getFlow`).
    pub ice: bool,
}

impl BlockView {
    /// `Blocks.AIR.defaultBlockState()`, com o número que o mundo dá ao ar.
    pub const fn air(state_id: u32) -> Self {
        Self {
            state_id,
            kind: BlockKind::Air,
            fluid: FluidState::Empty,
            collision: CollisionShape::Empty,
            dynamic_shape: false,
            washed_away: true,
            solid: false,
            blocks_fluid_flow: false,
            sturdy_faces: 0,
            ice: false,
        }
    }

    /// Um `LiquidBlock` com as propriedades da água (`FluidDeferredRegister.java:136-138`): sem colisão (LB:284-298 com o contexto
    /// vazio), não sólido, sem face firme. `washed_away` diz se o bloco está na tag (a água do vanilla está; os do Mekanism não).
    pub fn liquid(state_id: u32, liquid: LiquidId, level: u8, washed_away: bool) -> Self {
        Self {
            state_id,
            kind: BlockKind::Liquid { liquid, level },
            fluid: FluidState::from_legacy_level(liquid, level),
            collision: CollisionShape::Empty,
            dynamic_shape: false,
            washed_away,
            solid: false,
            blocks_fluid_flow: false,
            sturdy_faces: 0,
            ice: false,
        }
    }

    pub fn is_air(&self) -> bool {
        self.kind == BlockKind::Air
    }

    /// `isFaceSturdy(level, pos, face)`.
    pub fn is_face_sturdy(&self, face: Face) -> bool {
        self.sturdy_faces & (1 << face as u8) != 0
    }
}

/// O mundo como o `FlowingFluid` o pergunta. O que cada método responde, com a linha do Java, está no cabeçalho do módulo.
pub trait LiquidWorld {
    fn params(&self, liquid: LiquidId) -> LiquidParams;
    fn block(&self, pos: BlockPos) -> BlockView;
    fn legacy_view(&self, block: LegacyBlock) -> BlockView;
    fn merged_face_occludes(&self, source: &BlockView, target: &BlockView, direction: Face)
    -> bool;
    fn occlusion_cache(&self) -> &shape::OcclusionCache;
    fn can_place_liquid(&self, pos: BlockPos, state: &BlockView, fluid: Fluid) -> bool;
    fn place_liquid(&mut self, pos: BlockPos, state: &BlockView, fluid: FluidState) -> bool;
    fn set_block(&mut self, pos: BlockPos, block: LegacyBlock, flags: i32) -> bool;
    fn schedule_fluid_tick(&mut self, pos: BlockPos, fluid: Fluid, delay: i32);
    fn drop_resources(&mut self, pos: BlockPos, state: &BlockView);
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: LiquidId = LiquidId(3);

    #[test]
    fn the_legacy_level_maps_like_the_state_cache() {
        assert_eq!(FluidState::from_legacy_level(L, 0), FluidState::source(L));
        for level in 1..8 {
            assert_eq!(
                FluidState::from_legacy_level(L, level),
                FluidState::flowing(L, 8 - level, false)
            );
        }
        for level in 8..=MAX_LEGACY_LEVEL {
            assert_eq!(
                FluidState::from_legacy_level(L, level),
                FluidState::flowing(L, 8, true)
            );
        }
    }

    #[test]
    fn get_legacy_level_round_trips_what_the_fluid_writes() {
        for amount in 1..8 {
            let state = FluidState::flowing(L, amount, false);
            assert_eq!(state.legacy_level(), 8 - amount);
            assert_eq!(
                FluidState::from_legacy_level(L, state.legacy_level()),
                state
            );
        }
        assert_eq!(FluidState::flowing(L, 8, true).legacy_level(), 8);
        assert_eq!(FluidState::source(L).legacy_level(), 0);
        assert_eq!(FluidState::Empty.legacy_block(), LegacyBlock::Air);
    }

    #[test]
    fn a_full_flowing_state_that_does_not_fall_writes_a_source() {
        // getLegacyLevel: 8 - min(8, 8) + 0 = 0, o nível da fonte.
        let state = FluidState::flowing(L, 8, false);
        assert_eq!(state.legacy_level(), 0);
        assert_eq!(
            FluidState::from_legacy_level(L, state.legacy_level()),
            FluidState::source(L)
        );
    }

    #[test]
    fn heights_and_amounts_follow_the_java() {
        assert_eq!(FluidState::source(L).amount(), 8);
        assert_eq!(FluidState::Empty.amount(), 0);
        assert_eq!(FluidState::flowing(L, 3, false).own_height(), 3.0f32 / 9.0);
        assert_eq!(FluidState::Empty.own_height(), 0.0);
    }

    #[test]
    fn source_and_flowing_are_the_same_liquid_but_different_fluids() {
        assert!(Fluid::Source(L).is_same_as(L));
        assert!(Fluid::Flowing(L).is_same_as(L));
        assert!(!Fluid::Empty.is_same_as(L));
        assert_ne!(Fluid::Source(L), Fluid::Flowing(L));
    }
}
