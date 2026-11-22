//! Um item com tanque de fluido usado como balde no mundo: porte de `FluidUtil.tryPickupFluid` e `tryPlaceFluid` do NeoForge
//! (`FluidUtil.java:224-327`) e do trecho de `ItemBlockFluidTank.use` que os chama (`ItemBlockFluidTank.java:149-179`).
//!
//! A lib não conhece o mundo nem o item: o mundo vem por [`FluidWorld`], e o item pelo handler de fluido dele (o do acesso um por um,
//! agindo como `MANUAL`).
use crate::fluid::FluidResource;
use crate::item_access::ItemResourceHandler;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::rc::Rc;

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

/// O que `tryClaimItem` e `canAccessOrDisplayError` responderam no começo de um uso do item (`ItemBlockFluidTank.java:82-87`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// O item não tinha dono e quem usa o tomou: o uso acaba em `SUCCESS`.
    Claimed,
    /// Quem usa não tem acesso: `FAIL`.
    Refused,
    /// O item já tem dono e quem usa tem acesso: o uso segue.
    Owned,
}

/// O que `interactLivingEntity` pede a quem tem o jogo.
pub trait LivingUse {
    /// `tryClaimItem` e, sem posse nova, `canAccessOrDisplayError`. `keep` diz se a posse fica na pilha da mão.
    fn claim(&mut self, keep: bool) -> Claim;
    /// `fluidHandler.insert(MILK, BUCKET_VOLUME)` no handler `oneByOne` da mão, confirmado se algo entrou: o que entrou, ou `None` sem handler.
    fn milk(&mut self) -> Option<i32>;
}

/// `ItemBlockFluidTank.interactLivingEntity` (`:76-109`). `mode` é o modo balde; `baby` e `milkable` (`getMilkSound` não nulo) são da entidade; `infinite` é
/// `player.hasInfiniteMaterials()`: `Player.interactOn` entrega então uma cópia da pilha (`Player.java:875-877`), e a posse tomada nela se perde.
/// Fora do modo balde ou com um filhote, `PASS`. O item sem dono só toma posse (`SUCCESS`), e quem não tem acesso falha. Depois, quem dá leite enche o item com
/// um balde ou o que couber, e falha se nada coube; os outros passam.
pub fn interact_living_entity(
    mode: bool,
    baby: bool,
    milkable: bool,
    infinite: bool,
    user: &mut dyn LivingUse,
) -> UseOutcome {
    if !mode || baby {
        return UseOutcome::Pass;
    }
    match user.claim(!infinite) {
        Claim::Claimed => return UseOutcome::Success,
        Claim::Refused => return UseOutcome::Fail,
        Claim::Owned => {}
    }
    if !milkable {
        return UseOutcome::Pass;
    }
    match user.milk() {
        // "If there isn't a fluid handler then there is something wrong with the stack, treat it as a normal stack and skip".
        None => UseOutcome::Pass,
        // "Fail if we can't insert any, we allow partial bucket amounts for milking though".
        Some(0) => UseOutcome::Fail,
        Some(_) => UseOutcome::Success,
    }
}

/// `Player.interactOn` (`Player.java:862-891`): o `interact` da entidade tem a vez primeiro, e se consome a ação o item nem é perguntado; senão o
/// `interactLivingEntity` do item responde, e só o `SUCCESS` dele consome (`FAIL` e `PASS` viram `PASS`).
pub fn interact_on(entity_consumed: bool, item: impl FnOnce() -> UseOutcome) -> UseOutcome {
    if entity_consumed {
        return UseOutcome::Success;
    }
    match item() {
        UseOutcome::Success => UseOutcome::Success,
        UseOutcome::Fail | UseOutcome::Pass => UseOutcome::Pass,
    }
}

/// `FluidTankItemDispenseBehavior.execute` depois do handler (`ItemBlockFluidTank.java:240-249`): diante de uma fonte o item tira um
/// balde dela, e diante de qualquer outra coisa (ar, fluido correndo, um bloco) ele tenta pôr um. `source` é
/// `!fluidState.isEmpty() && fluidState.isSource()` da célula da frente. Quem chama confirma `transaction` se algo passou.
pub fn dispense_as_bucket(
    handler: &dyn ItemResourceHandler<FluidResource>,
    world: &mut dyn FluidWorld,
    pos: [i32; 3],
    source: bool,
    transaction: &Transaction,
) -> Option<FluidResource> {
    if source {
        try_pickup_fluid(handler, world, pos, transaction)
    } else {
        try_place_fluid(handler, world, pos, true, transaction)
    }
}

/// `CauldronFluidContent`: o fluido de um caldeirão, o total em mB e o número de níveis. Os do vanilla
/// (`CauldronFluidContent.init`): a água em três níveis e a lava num só, um balde cada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CauldronContent {
    pub fluid: &'static str,
    pub total_amount: i32,
    pub max_level: i32,
}

pub const CAULDRON_CONTENTS: [CauldronContent; 2] = [
    CauldronContent {
        fluid: "minecraft:water",
        total_amount: BUCKET_VOLUME,
        max_level: 3,
    },
    CauldronContent {
        fluid: "minecraft:lava",
        total_amount: BUCKET_VOLUME,
        max_level: 1,
    },
];

fn cauldron_content(fluid: &FluidResource) -> Option<CauldronContent> {
    CAULDRON_CONTENTS
        .into_iter()
        .find(|content| content.fluid == &*fluid.id)
}

/// O estado de um caldeirão: o fluido (`None` é o caldeirão vazio) e o nível (`currentLevel`: 0 no vazio, 1 no sem níveis).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CauldronLevel {
    pub fluid: Option<FluidResource>,
    pub level: i32,
}

impl Journaled for CauldronLevel {
    type Snapshot = CauldronLevel;

    fn revert_to_snapshot(&mut self, snapshot: Self::Snapshot) {
        *self = snapshot;
    }

    fn on_root_commit(&mut self, _original: Self::Snapshot) -> Vec<Callback> {
        Vec::new()
    }
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// `CauldronWrapper` (NeoForge): a capability de fluido de um caldeirão, sobre o estado do bloco. Quem chama grava o estado final
/// no mundo depois de confirmar a transação (`onRootCommit`).
pub struct CauldronHandler {
    cell: Rc<JournalCell<CauldronLevel>>,
}

impl CauldronHandler {
    pub fn new(state: CauldronLevel) -> Self {
        Self {
            cell: JournalCell::new(state),
        }
    }

    pub fn state(&self) -> CauldronLevel {
        self.cell.borrow().clone()
    }

    /// `getResource(0)`.
    pub fn resource(&self) -> Option<FluidResource> {
        self.cell.borrow().fluid.clone()
    }

    /// `getAmountAsInt(0)`: `totalAmount * currentLevel / maxLevel`, arredondado para baixo.
    pub fn amount(&self) -> i32 {
        let state = self.cell.borrow();
        match state.fluid.as_ref().and_then(cauldron_content) {
            Some(content) => content.total_amount * state.level / content.max_level,
            None => 0,
        }
    }

    fn set(&self, fluid: Option<FluidResource>, level: i32, transaction: &Transaction) {
        let snapshot = self.cell.borrow().clone();
        self.cell.update_snapshots(transaction, || snapshot);
        *self.cell.borrow_mut() = if level == 0 {
            CauldronLevel::default()
        } else {
            CauldronLevel { fluid, level }
        };
    }

    /// `insert`: só entra em passos do máximo divisor comum entre os níveis e o total (um terço de balde na água, um balde
    /// na lava), e só o fluido que o caldeirão já tem.
    pub fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        let Some(content) = cauldron_content(resource) else {
            return 0;
        };
        let state = self.state();
        if state.fluid.as_ref().is_some_and(|fluid| fluid != resource) {
            return 0;
        }
        let d = gcd(content.max_level, content.total_amount);
        let amount_increments = content.total_amount / d;
        let level_increments = content.max_level / d;
        let inserted =
            (amount / amount_increments).min((content.max_level - state.level) / level_increments);
        if inserted > 0 {
            self.set(
                Some(resource.clone()),
                state.level + inserted * level_increments,
                transaction,
            );
        }
        inserted * amount_increments
    }

    /// `extract`: os mesmos passos, só do fluido que está lá.
    pub fn extract(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        let state = self.state();
        if state.fluid.as_ref() != Some(resource) {
            return 0;
        }
        let Some(content) = cauldron_content(resource) else {
            return 0;
        };
        let d = gcd(content.max_level, content.total_amount);
        let level_increments = content.max_level / d;
        let amount_increments = content.total_amount / d;
        let extracted = (amount / amount_increments).min(state.level / level_increments);
        if extracted > 0 {
            self.set(
                Some(resource.clone()),
                state.level - extracted * level_increments,
                transaction,
            );
        }
        extracted * amount_increments
    }
}

/// O que `FluidTankCauldronInteraction.interact` devolve: `TRY_WITH_EMPTY_HAND`, ou `SUCCESS` com o fluido que passou e para onde
/// (o som e o evento são `BUCKET_EMPTY` e `FLUID_PLACE` se o caldeirão encheu, `BUCKET_FILL` e `FLUID_PICKUP` se esvaziou).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CauldronOutcome {
    TryWithEmptyHand,
    Success {
        fluid: FluidResource,
        filled_cauldron: bool,
    },
}

/// `FluidTankCauldronInteraction.interact` depois do modo balde (`ItemBlockFluidTank.java:283-336`): primeiro o caldeirão
/// esvazia no item, pelo que o item aceita do que ele tem; com o caldeirão vazio, o item esvazia nele, pelo que o item entrega.
/// `tryTransfer` só vale se o que entrou de um lado saiu inteiro do outro. Quem chama confirma `transaction` num `Success`.
pub fn cauldron_interact(
    handler: Option<&dyn ItemResourceHandler<FluidResource>>,
    cauldron: &CauldronHandler,
    transaction: &Transaction,
) -> CauldronOutcome {
    let Some(handler) = handler.filter(|handler| handler.size() > 0) else {
        return CauldronOutcome::TryWithEmptyHand;
    };
    let mut target = None;
    if let Some(resource) = cauldron.resource() {
        let simulation = Transaction::open(Some(transaction));
        let amount = handler.insert(&resource, cauldron.amount(), &simulation);
        drop(simulation);
        if amount > 0 {
            target = Some((resource, amount));
        }
    }
    let (fluid, amount, filled_cauldron) = match target {
        Some((fluid, amount)) => (fluid, amount, false),
        None => {
            let Some(resource) = handler.resource(0) else {
                return CauldronOutcome::TryWithEmptyHand;
            };
            let simulation = Transaction::open(Some(transaction));
            let amount = handler.extract(&resource, handler.amount_as_int(0), &simulation);
            drop(simulation);
            if amount <= 0 {
                return CauldronOutcome::TryWithEmptyHand;
            }
            (resource, amount, true)
        }
    };
    let (inserted, extracted) = if filled_cauldron {
        let inserted = cauldron.insert(&fluid, amount, transaction);
        (
            inserted,
            if inserted > 0 {
                handler.extract(&fluid, inserted, transaction)
            } else {
                0
            },
        )
    } else {
        let inserted = handler.insert(&fluid, amount, transaction);
        (
            inserted,
            if inserted > 0 {
                cauldron.extract(&fluid, inserted, transaction)
            } else {
                0
            },
        )
    };
    if inserted > 0 && extracted == inserted {
        CauldronOutcome::Success {
            fluid,
            filled_cauldron,
        }
    } else {
        CauldronOutcome::TryWithEmptyHand
    }
}
