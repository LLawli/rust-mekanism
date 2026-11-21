//! Um item com tanque de fluido usado como balde no mundo: porte de `FluidUtil.tryPickupFluid` e `tryPlaceFluid` do NeoForge
//! (`FluidUtil.java:224-327`) e do trecho de `ItemBlockFluidTank.use` que os chama (`ItemBlockFluidTank.java:149-179`).
//!
//! A lib não conhece o mundo nem o item: o mundo vem por [`FluidWorld`], e o item pelo handler de fluido dele (o do acesso um por um,
//! agindo como `MANUAL`).
use crate::fluid::FluidResource;
use crate::item_access::ItemResourceHandler;
use crate::transaction::Transaction;

/// `FluidType.BUCKET_VOLUME`.
pub const BUCKET_VOLUME: i32 = 1000;

/// O que os dois movimentos pedem do mundo.
pub trait FluidWorld {
    /// O fluido que um balde tiraria de `pos`: `level.getFluidState(pos).getType()` quando o bloco é um `BucketPickup` e o fluido não é
    /// vazio (`FluidUtil.java:228-239`).
    fn fluid_to_pick(&self, pos: [i32; 3]) -> Option<FluidResource>;
    /// `bucketPickup.pickupBlock`: tira o fluido do mundo e diz o conteúdo do balde que saiu (`None` se não saiu balde nenhum).
    fn pick_up(&mut self, pos: [i32; 3]) -> Option<FluidResource>;
    /// O `tryPlaceFluid(resource, player, level, pos, validatePlaced)` do mundo (`FluidUtil.java:337-374`): se um balde de `resource`
    /// foi posto em `pos` (ou vaporizou ali).
    fn place(&mut self, resource: &FluidResource, pos: [i32; 3], validate_placed: bool) -> bool;
    /// `playSoundAndGameEvent` de um balde tirado do mundo: o de um balde posto é do próprio `place`.
    fn picked_up(&mut self, resource: &FluidResource, pos: [i32; 3]);
}

/// `FluidUtil.tryPickupFluid(destination, player, level, pos, transaction)`: um balde do fluido de `pos` entra em `destination`, e só
/// se coube inteiro o fluido sai do mundo. Devolve o que foi tirado.
pub fn try_pickup_fluid(
    destination: &dyn ItemResourceHandler<FluidResource>,
    world: &mut dyn FluidWorld,
    pos: [i32; 3],
    transaction: &Transaction,
) -> Option<FluidResource> {
    let resource = world.fluid_to_pick(pos)?;
    let tx = Transaction::open(Some(transaction));
    if destination.insert(&resource, BUCKET_VOLUME, &tx) != BUCKET_VOLUME {
        return None;
    }
    // "Inserting into destination caused type in the level to change, aborting".
    if world.fluid_to_pick(pos).as_ref() != Some(&resource) {
        return None;
    }
    let picked = world.pick_up(pos)?;
    if picked != resource {
        return None;
    }
    world.picked_up(&resource, pos);
    tx.commit();
    Some(picked)
}

/// `FluidUtil.tryPlaceFluid(source, player, level, pos, validatePlaced, transaction)`: o primeiro índice de `source` que entrega um
/// balde inteiro e cujo fluido o mundo aceita em `pos`. Devolve o que foi posto.
pub fn try_place_fluid(
    source: &dyn ItemResourceHandler<FluidResource>,
    world: &mut dyn FluidWorld,
    pos: [i32; 3],
    validate_placed: bool,
    transaction: &Transaction,
) -> Option<FluidResource> {
    for index in 0..source.size() {
        let Some(resource) = source.resource(index) else {
            continue;
        };
        let tx = Transaction::open(Some(transaction));
        if source.extract_at(index, &resource, BUCKET_VOLUME, &tx) != BUCKET_VOLUME {
            continue;
        }
        if world.place(&resource, pos, validate_placed) {
            tx.commit();
            return Some(resource);
        }
    }
    None
}

/// `InteractionResult` de um uso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseOutcome {
    Pass,
    Fail,
    Success,
}

/// O bloco que a mira acerta (`getPlayerPOVHitResult`) e o vizinho dele pela face acertada (`pos.relative(direction)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UseHit {
    pub pos: [i32; 3],
    pub offset: [i32; 3],
}

/// `ItemBlockFluidTank.use` depois do modo balde, da posse e do acesso (`:149-179`). `hit` é `None` sem bloco na mira, e quem chama
/// mira com `ClipContext.Fluid.NONE` se agachado e `SOURCE_ONLY` se não; `may_use` é `level.mayInteract` e `player.mayUseItemAt`.
/// Agachado o item põe um balde no bloco acertado e, se não deu, no vizinho; em pé ele tira um balde do bloco acertado. Quem chama
/// confirma `transaction` num `Success`.
pub fn use_as_bucket(
    handler: Option<&dyn ItemResourceHandler<FluidResource>>,
    sneaking: bool,
    hit: Option<UseHit>,
    may_use: bool,
    world: &mut dyn FluidWorld,
    transaction: &Transaction,
) -> UseOutcome {
    let Some(hit) = hit else {
        return UseOutcome::Pass;
    };
    if !may_use {
        return UseOutcome::Fail;
    }
    let Some(handler) = handler else {
        return UseOutcome::Fail;
    };
    if sneaking {
        if try_place_fluid(handler, world, hit.pos, true, transaction).is_none()
            && try_place_fluid(handler, world, hit.offset, true, transaction).is_none()
        {
            return UseOutcome::Fail;
        }
    } else if try_pickup_fluid(handler, world, hit.pos, transaction).is_none() {
        return UseOutcome::Fail;
    }
    UseOutcome::Success
}
