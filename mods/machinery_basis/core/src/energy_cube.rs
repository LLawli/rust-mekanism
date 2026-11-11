//! Porte de `TileEntityEnergyCube`: a energia em `long` com o limite de taxa só para quem é interno (`EnergyCubeEnergyContainer`), o
//! criativo que aceita sem guardar, os dois slots de energia (`EnergyInventorySlot.fillOrConvert` e `.drain`) contra o handler de
//! energia do item, a configuração de lados com `ENERGY` e `ITEM`, o ejetor de energia (`EnergyUtils.emit`, `EmitUtils.sendToAcceptors`),
//! a escala do núcleo, o comparador e o `CubeSideState` de cada lado.
//!
//! A lib não conhece nenhum item: o handler de energia de um item (`ItemEnergySpec`) e a escrita da energia nele vêm do mod.
use crate::containers::{
    Access, AutomationType, InventorySlot, ItemResource, SlotChecks, lerp_discrete,
};
use crate::ejector::{AttachedEjector, EjectEnv, EjectorData, TileComponentEjector};
use crate::energy_conversion::EnergyConversionCache;
use crate::item_handler::NoNeighbours;
use crate::last_energy::LastEnergyTracker;
use crate::side_config::{
    AttachedSideConfig, ConfigData, RelativeSide, TileComponentConfig, TransmissionType,
};
use crate::sides::Face;
use crate::tile::{ConfigurationData, SideConfigSpec, TileBase};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction, fire};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;

/// Os cinco tiers (`EnergyCubeTier.VALUES`), pelo índice.
pub const TIER_COUNT: usize = 5;
/// O índice do cubo criativo.
pub const CREATIVE: usize = 4;
/// `getInitialInventory`: o slot de descarga (`fillOrConvert`) vem primeiro, o de carga (`drain`) depois.
pub const DISCHARGE_SLOT: usize = 0;
pub const CHARGE_SLOT: usize = 1;
/// O índice do contêiner de energia, o único.
pub const ENERGY_CONTAINER: usize = 0;

fn saturated(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `TileEntityEnergyCube.CubeSideState`: a ordem é a do enum (o `EnergyCubeModel` conta com `INACTIVE` no zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeSideState {
    Inactive,
    ActiveLit,
    ActiveUnlit,
}

/// `AttributeSideConfig` do cubo (`withSideConfig(ENERGY, ITEM)`) e o que o construtor monta: `setupIOConfig(ITEM, chargeSlot, dischargeSlot, true)` com o item sem ejetar,
/// `setupIOConfig(ENERGY, energyContainer)` e o ejetor lendo só a energia.
pub fn side_config_spec() -> SideConfigSpec {
    SideConfigSpec {
        types: vec![TransmissionType::Energy, TransmissionType::Item],
        setup: |config| {
            config.setup_io_config_two(TransmissionType::Item, CHARGE_SLOT, DISCHARGE_SLOT, true, true);
            if let Some(info) = config.config_mut(TransmissionType::Item) {
                info.set_can_eject(false);
            }
            config.setup_io_config(TransmissionType::Energy, ENERGY_CONTAINER, false);
        },
        eject: vec![TransmissionType::Energy],
    }
}

/// `getModelData`: por `RelativeSide`, `ACTIVE_LIT` se o `DataType` do lado sai, `ACTIVE_UNLIT` se só entra, `INACTIVE` senão.
pub fn side_states(config: &TileComponentConfig) -> [CubeSideState; 6] {
    let mut states = [CubeSideState::Inactive; 6];
    let Some(info) = config.config(TransmissionType::Energy) else {
        return states;
    };
    for side in RelativeSide::VALUES {
        states[side.ordinal()] = match info.slot_info(side) {
            Some(slot) if slot.can_output => CubeSideState::ActiveLit,
            Some(slot) if slot.can_input => CubeSideState::ActiveUnlit,
            _ => CubeSideState::Inactive,
        };
    }
    states
}

/// `Mth.equal`.
fn mth_equal(a: f32, b: f32) -> bool {
    (b - a).abs() < 1.0e-5
}

/// `MekanismUtils.getScale(prevScale, IEnergyContainer)`.
pub fn get_scale(prev_scale: f32, stored: i64, capacity: i64) -> f32 {
    let target = if capacity == 0 {
        0.0
    } else {
        (stored as f64 / capacity as f64) as f32
    };
    scale_towards(prev_scale, target, stored == 0, stored == capacity)
}

/// `MekanismUtils.getScale(prevScale, targetScale, empty, full)`.
pub fn scale_towards(prev_scale: f32, target: f32, empty: bool, full: bool) -> f32 {
    let difference = (prev_scale - target).abs();
    if f64::from(difference) > 0.01 {
        return (9.0 * prev_scale + target) / 10.0;
    } else if !empty && full && difference > 0.0 {
        return target;
    } else if !empty && prev_scale == 0.0 {
        return target;
    }
    if empty && f64::from(prev_scale) < 0.01 {
        return 0.0;
    }
    prev_scale
}

/// `MekanismUtils.scaleChanged`.
pub fn scale_changed(scale: f32, prev_scale: f32) -> bool {
    if mth_equal(scale, prev_scale) {
        if scale == prev_scale {
            return false;
        }
        return scale == 0.0 || scale == 1.0 || prev_scale == 1.0 || prev_scale == 0.0;
    }
    true
}

/// `MekanismUtils.redstoneLevelFromContents`.
pub fn redstone_level_from_contents(amount: i64, capacity: i64) -> i32 {
    let fraction = if capacity == 0 {
        0.0
    } else {
        amount as f64 / capacity as f64
    };
    lerp_discrete(fraction as f32)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O limite de taxa por tick (`RateLimitIntegerJournal`) e o contêiner de energia do cubo.

struct RateState {
    /// `currentGameTime`, que nasce em zero como o `long` do Java.
    time: u64,
    remaining: i32,
}

impl Journaled for RateState {
    type Snapshot = i32;

    fn revert_to_snapshot(&mut self, snapshot: i32) {
        self.remaining = snapshot;
    }

    fn on_root_commit(&mut self, _original: i32) -> Vec<Callback> {
        Vec::new()
    }
}

/// `ITransactionHelper.createInternalOnlyRateLimit(gameTime, rate)`: só o tipo `INTERNAL` é limitado, e a conta volta ao teto quando o
/// tick muda.
#[derive(Clone)]
struct RateLimit {
    cell: Rc<JournalCell<RateState>>,
    game_time: Rc<Cell<u64>>,
    rate: i32,
}

impl RateLimit {
    fn new(game_time: Rc<Cell<u64>>, rate: i32) -> Self {
        Self {
            cell: JournalCell::new(RateState {
                time: 0,
                remaining: 0,
            }),
            game_time,
            rate,
        }
    }

    fn check_tick_changed(&self) {
        let now = self.game_time.get();
        let mut state = self.cell.borrow_mut();
        if state.time != now {
            state.time = now;
            state.remaining = self.rate;
        }
    }

    fn remaining(&self, automation: AutomationType) -> i32 {
        if automation == AutomationType::Internal {
            self.check_tick_changed();
            return self.cell.borrow().remaining;
        }
        i32::MAX
    }

    fn consume(&self, limit: i32, automation: AutomationType, transaction: &Transaction) {
        if automation != AutomationType::Internal {
            return;
        }
        self.check_tick_changed();
        let remaining = self.cell.borrow().remaining;
        assert!(limit <= remaining, "consumiu mais do limite que havia");
        self.cell.update_snapshots(transaction, || remaining);
        self.cell.borrow_mut().remaining -= limit;
    }
}

struct EnergyState {
    stored: i64,
    capacity: i64,
    listener: Option<Callback>,
}

impl Journaled for EnergyState {
    type Snapshot = i64;

    fn revert_to_snapshot(&mut self, snapshot: i64) {
        self.stored = snapshot;
    }

    fn on_root_commit(&mut self, original: i64) -> Vec<Callback> {
        if self.stored == original {
            return Vec::new();
        }
        self.listener.iter().cloned().collect()
    }
}

/// `EnergyCubeEnergyContainer`.
#[derive(Clone)]
pub struct CubeEnergy {
    cell: Rc<JournalCell<EnergyState>>,
    insertion: RateLimit,
    extraction: RateLimit,
    creative: bool,
}

impl CubeEnergy {
    fn new(
        capacity: i64,
        rate: i32,
        creative: bool,
        game_time: Rc<Cell<u64>>,
        listener: Option<Callback>,
    ) -> Self {
        Self {
            cell: JournalCell::new(EnergyState {
                stored: 0,
                capacity,
                listener,
            }),
            insertion: RateLimit::new(game_time.clone(), rate),
            extraction: RateLimit::new(game_time, rate),
            creative,
        }
    }

    pub fn amount(&self) -> i64 {
        self.cell.borrow().stored
    }

    pub fn capacity(&self) -> i64 {
        self.cell.borrow().capacity
    }

    pub fn is_empty(&self) -> bool {
        self.amount() == 0
    }

    /// `getNeededAsInt`.
    pub fn needed_as_int(&self) -> i32 {
        saturated((self.capacity() - self.amount()).max(0))
    }

    pub fn amount_as_int(&self) -> i32 {
        saturated(self.amount())
    }

    /// Mudar a capacidade de um tier (a configuração mudou): o tile segue o valor novo.
    pub fn set_capacity(&self, capacity: i64) {
        self.cell.borrow_mut().capacity = capacity;
    }

    /// `isValidForInsertion` e `isValidForExtraction`: `alwaysTrue` nos dois.
    pub fn is_valid_for_insertion(&self, _automation: AutomationType) -> bool {
        true
    }

    pub fn is_valid_for_extraction(&self, _automation: AutomationType) -> bool {
        true
    }

    /// `setEnergy`: sem transação o aviso dispara na hora.
    pub fn set_energy(&self, energy: i64, transaction: Option<&Transaction>) {
        let callbacks = {
            let mut state = self.cell.borrow_mut();
            if state.stored == energy {
                return;
            }
            match transaction {
                Some(transaction) => {
                    self.cell.update_snapshots(transaction, || state.stored);
                    state.stored = energy;
                    Vec::new()
                }
                None => {
                    state.stored = energy;
                    state.listener.iter().cloned().collect()
                }
            }
        };
        fire(callbacks);
    }

    fn insert_basic(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        if amount == 0 {
            return 0;
        }
        let (stored, capacity) = {
            let state = self.cell.borrow();
            (state.stored, state.capacity)
        };
        let mut needed = saturated(capacity - stored);
        needed = needed.min(self.insertion.remaining(automation));
        if needed <= 0 {
            return 0;
        }
        let to_add = amount.min(needed);
        self.set_energy(stored + i64::from(to_add), Some(transaction));
        self.insertion.consume(to_add, automation, transaction);
        to_add
    }

    fn extract_basic(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        let stored = self.amount();
        if stored == 0 || amount == 0 {
            return 0;
        }
        let mut to_remove = amount.min(saturated(stored));
        to_remove = to_remove.min(self.extraction.remaining(automation));
        if to_remove > 0 {
            self.set_energy(stored - i64::from(to_remove), Some(transaction));
            self.extraction.consume(to_remove, automation, transaction);
        }
        to_remove
    }

    /// `insert`: o criativo só diz o que aceitaria (a transação de dentro nunca commita).
    pub fn insert(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        if self.creative {
            let simulation = Transaction::open(Some(transaction));
            return self.insert_basic(amount, &simulation, automation);
        }
        self.insert_basic(amount, transaction, automation)
    }

    /// `extract`, com o mesmo cuidado do criativo.
    pub fn extract(&self, amount: i32, transaction: &Transaction, automation: AutomationType) -> i32 {
        if self.creative {
            let simulation = Transaction::open(Some(transaction));
            return self.extract_basic(amount, &simulation, automation);
        }
        self.extract_basic(amount, transaction, automation)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O handler de energia de um item (`Capabilities.ENERGY` do item): o que a pilha guarda, com a taxa e os predicados do contêiner
// dela (`ComponentBackedEnergyContainer`: `canExtract`, `canInsert` e o limite que o `MANUAL` não sofre).

/// O contêiner de energia de um item, por unidade (o handler multiplica pela pilha).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemEnergySpec {
    pub amount: i64,
    pub capacity: i64,
    /// O limite por chamada de quem não é `MANUAL` (o handler é novo a cada acesso, então a taxa é por chamada).
    pub rate: i32,
    pub can_extract: Access,
    pub can_insert: Access,
    /// `ComponentBackedEnergyCubeContainer` do cubo criativo: aceita e entrega sem mudar.
    pub creative: bool,
}

/// O handler de energia de uma pilha de `count` itens.
#[derive(Clone, Copy)]
struct ItemHandler {
    spec: ItemEnergySpec,
    count: i32,
}

impl ItemHandler {
    fn limit(&self, automation: AutomationType) -> i32 {
        if automation == AutomationType::Manual {
            i32::MAX
        } else {
            self.spec.rate
        }
    }

    /// `ComponentBackedEnergyHandler.insert`: devolve o que entrou na pilha e a energia nova por item.
    fn insert(&self, amount: i32, automation: AutomationType) -> (i32, i64) {
        let per_item = amount / self.count.max(1);
        if per_item == 0 || !self.spec.can_insert.allows(automation) {
            return (0, self.spec.amount);
        }
        let mut needed = saturated(self.spec.capacity - self.spec.amount);
        needed = needed.min(self.limit(automation));
        if needed <= 0 {
            return (0, self.spec.amount);
        }
        let to_add = per_item.min(needed);
        let after = if self.spec.creative {
            self.spec.amount
        } else {
            self.spec.amount + i64::from(to_add)
        };
        (self.count.wrapping_mul(to_add), after)
    }

    /// `ComponentBackedEnergyHandler.extract`.
    fn extract(&self, amount: i32, automation: AutomationType) -> (i32, i64) {
        let per_item = amount / self.count.max(1);
        if per_item == 0 || !self.spec.can_extract.allows(automation) || self.spec.amount == 0 {
            return (0, self.spec.amount);
        }
        let mut to_remove = per_item.min(saturated(self.spec.amount));
        to_remove = to_remove.min(self.limit(automation));
        if to_remove <= 0 {
            return (0, self.spec.amount);
        }
        let after = if self.spec.creative {
            self.spec.amount
        } else {
            self.spec.amount - i64::from(to_remove)
        };
        (self.count.wrapping_mul(to_remove), after)
    }
}

/// O que o mod diz de um item: o handler de energia dele, ou `None` se não tem.
pub type ItemEnergyLookup = Rc<dyn Fn(&ItemResource) -> Option<ItemEnergySpec>>;
/// O item com a energia nova (por unidade).
pub type ItemEnergyWrite = Rc<dyn Fn(&ItemResource, i64) -> Option<ItemResource>>;

/// A parte do mod que o cubo precisa.
#[derive(Clone)]
pub struct ItemEnergy {
    pub lookup: ItemEnergyLookup,
    pub write: ItemEnergyWrite,
    pub conversions: Rc<EnergyConversionCache>,
}

impl ItemEnergy {
    fn handler(&self, resource: &ItemResource, count: u32) -> Option<ItemHandler> {
        let spec = (self.lookup)(resource)?;
        Some(ItemHandler {
            spec,
            count: i32::try_from(count).unwrap_or(i32::MAX).max(1),
        })
    }

    /// `EnergyInventorySlot.canFill`.
    pub fn can_fill(&self, container: Option<&CubeEnergy>, item: &ItemResource) -> bool {
        let Some(handler) = self.handler(item, 1) else {
            return false;
        };
        if container.is_some_and(|c| !c.is_valid_for_extraction(AutomationType::Internal)) {
            return false;
        }
        handler.extract(i32::MAX, AutomationType::External).0 > 0
    }

    /// `EnergyInventorySlot.canDrain`.
    pub fn can_drain(&self, container: &CubeEnergy, item: &ItemResource) -> bool {
        let Some(handler) = self.handler(item, 1) else {
            return false;
        };
        if container.amount() == 0 {
            return handler.spec.amount < handler.spec.capacity;
        }
        if !container.is_valid_for_extraction(AutomationType::Internal) {
            return false;
        }
        handler
            .insert(saturated(container.capacity()), AutomationType::External)
            .0
            > 0
    }

    /// `EnergyInventorySlot.canFillOrConvert`.
    pub fn can_fill_or_convert(&self, container: &CubeEnergy, item: &ItemResource) -> bool {
        if self.can_fill(Some(container), item) {
            return true;
        }
        self.conversions.find_type_based_recipe(item).is_some()
            && container.is_valid_for_insertion(AutomationType::Manual)
    }

    /// `fillContainerFromSlot`.
    pub fn fill_container_from_slot(
        &self,
        slot: &InventorySlot,
        container: &CubeEnergy,
        transaction: Option<&Transaction>,
    ) -> bool {
        let contents = slot.contents();
        let Some(resource) = contents.resource.clone() else {
            return false;
        };
        if container.amount() >= container.capacity() {
            return false;
        }
        let Some(handler) = self.handler(&resource, contents.amount) else {
            return false;
        };
        let room_for = {
            let simulation = Transaction::open(transaction);
            let room_for = container.insert(
                container.needed_as_int(),
                &simulation,
                AutomationType::Internal,
            );
            if room_for == 0 {
                return false;
            }
            room_for
        };
        let sub = Transaction::open(transaction);
        let (extracted, after) = handler.extract(room_for, AutomationType::External);
        if extracted > 0 {
            if !handler.spec.creative
                && let Some(written) = (self.write)(&resource, after)
            {
                slot.set_contents(Some(written), contents.amount, Some(&sub));
            }
            if container.insert(extracted, &sub, AutomationType::Internal) == extracted {
                sub.commit();
                return true;
            }
        }
        false
    }

    /// `fillContainerOrConvert`.
    pub fn fill_container_or_convert(
        &self,
        slot: &InventorySlot,
        container: &CubeEnergy,
        transaction: Option<&Transaction>,
    ) {
        if self.fill_container_from_slot(slot, container, transaction) {
            return;
        }
        let contents = slot.contents();
        let Some(resource) = contents.resource.clone() else {
            return;
        };
        let Some(conversion) = self.conversions.find_first_recipe(&contents) else {
            return;
        };
        let sub = Transaction::open(transaction);
        let needed = conversion.input_count;
        let output = conversion.output;
        if slot.extract(&resource, needed, &sub, AutomationType::Internal) == needed
            && container.insert(output, &sub, AutomationType::Manual) == output
        {
            sub.commit();
        }
    }

    /// `drainContainerIntoSlot`.
    pub fn drain_container_into_slot(
        &self,
        slot: &InventorySlot,
        container: &CubeEnergy,
        transaction: Option<&Transaction>,
    ) {
        let contents = slot.contents();
        let Some(resource) = contents.resource.clone() else {
            return;
        };
        if container.is_empty() {
            return;
        }
        let Some(handler) = self.handler(&resource, contents.amount) else {
            return;
        };
        let available = {
            let simulation = Transaction::open(transaction);
            let available = container.extract(
                container.amount_as_int(),
                &simulation,
                AutomationType::Internal,
            );
            if available == 0 {
                return;
            }
            available
        };
        let sub = Transaction::open(transaction);
        let (inserted, after) = handler.insert(available, AutomationType::External);
        if inserted > 0 {
            if !handler.spec.creative
                && let Some(written) = (self.write)(&resource, after)
            {
                slot.set_contents(Some(written), contents.amount, Some(&sub));
            }
            if container.extract(inserted, &sub, AutomationType::Internal) == inserted {
                sub.commit();
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Entregar a energia aos vizinhos (`EnergyUtils.emit`, `EmitUtils.sendToAcceptors`, `Target`, `SplitInfo`).

/// Os vizinhos de energia: o `EnergyHandler` que cada face toca. O Horizon não desfaz uma inserção, então a simulação é pedida à parte.
pub trait EnergyNeighbours {
    /// Se há handler de energia do outro lado da face (`BlockCapabilityCache.getCapability` não nulo).
    fn has_handler(&self, face: Face) -> bool;
    /// `EnergyHandler.insert`: o que o vizinho aceita; `simulate` não muda nada.
    fn insert(&self, face: Face, amount: i32, simulate: bool) -> i32;
}

/// Sem vizinho nenhum.
pub struct NoEnergyNeighbours;

impl EnergyNeighbours for NoEnergyNeighbours {
    fn has_handler(&self, _face: Face) -> bool {
        false
    }

    fn insert(&self, _face: Face, _amount: i32, _simulate: bool) -> i32 {
        0
    }
}

/// `SplitInfo`.
struct SplitInfo {
    amount_per_changed: bool,
    to_split_among: i64,
    amount_to_split: i64,
    amount_per_target: i64,
    sent_so_far: i64,
    remainder: i64,
}

impl SplitInfo {
    fn new(amount_to_split: i64, total_targets: i64) -> Self {
        Self {
            amount_per_changed: false,
            to_split_among: total_targets,
            amount_to_split,
            amount_per_target: if total_targets == 0 { 0 } else { amount_to_split / total_targets },
            sent_so_far: 0,
            remainder: if total_targets == 0 { 0 } else { amount_to_split % total_targets },
        }
    }

    fn send(&mut self, amount_needed: i64, decrement_targets: bool) {
        self.amount_to_split -= amount_needed;
        self.sent_so_far += amount_needed;
        if !decrement_targets {
            let difference = amount_needed - self.amount_per_target;
            if difference > 0 {
                self.remainder -= difference;
            }
            return;
        }
        self.to_split_among -= 1;
        if amount_needed != self.amount_per_target && self.to_split_among != 0 {
            let last = self.amount_per_target;
            self.amount_per_target = self.amount_to_split / self.to_split_among;
            self.remainder = self.amount_to_split % self.to_split_among;
            if !self.amount_per_changed && self.amount_per_target != last {
                self.amount_per_changed = true;
            }
        }
    }

    fn share_amount(&self) -> i64 {
        self.amount_per_target
    }

    fn remainder_amount(&self) -> i64 {
        if self.to_split_among != 0 && self.remainder > 0 {
            return self.amount_per_target + 1;
        }
        self.amount_per_target
    }

    fn unsent(&self) -> i64 {
        self.amount_to_split
    }
}

/// `EnergyHandlerTarget.accept`: `handler.insert(saturatedCast(amount))`, de verdade.
fn accept(neighbours: &dyn EnergyNeighbours, face: Face, amount: i64) -> i64 {
    i64::from(neighbours.insert(face, saturated(amount), false))
}

/// `EmitUtils.sendToAcceptors` para os alvos de energia, na ordem em que o ejetor os deu. O diário do Java desfaz o que uma consulta
/// fez e não vira envio (`needed`); aqui a consulta é uma simulação, e só o envio de verdade muda o vizinho.
pub fn send_to_acceptors(neighbours: &dyn EnergyNeighbours, targets: &[Face], amount: i64) -> i64 {
    if amount == 0 || targets.is_empty() {
        return 0;
    }
    if targets.len() == 1 {
        return accept(neighbours, targets[0], amount);
    }
    let mut split = SplitInfo::new(amount, targets.len() as i64);
    // `needed`: o que cada um quis além da parte, na ordem em que apareceram.
    let mut needed: Vec<(Face, i64)> = Vec::new();
    for &face in targets {
        let amount_needed = i64::from(neighbours.insert(face, saturated(split.unsent()), true));
        if amount_needed <= split.share_amount() {
            split.send(accept(neighbours, face, amount_needed), true);
        } else {
            needed.retain(|(known, _)| *known != face);
            needed.push((face, amount_needed));
        }
    }
    while split.amount_per_changed {
        split.amount_per_changed = false;
        if needed.is_empty() || split.share_amount() == 0 {
            continue;
        }
        let mut index = 0;
        while index < needed.len() {
            let (face, amount_needed) = needed[index];
            if amount_needed <= split.share_amount() {
                split.send(accept(neighbours, face, amount_needed), true);
                needed.remove(index);
            } else {
                index += 1;
            }
        }
    }
    if !needed.is_empty() && split.unsent() > 0 {
        if needed.len() == 1 {
            let accepted = accept(neighbours, needed[0].0, split.unsent());
            split.send(accepted, false);
            return split.sent_so_far;
        }
        let mut index = 0;
        while index < needed.len() {
            let remainder_amount = split.remainder_amount();
            if remainder_amount == 0 {
                return split.sent_so_far;
            }
            let (face, amount_needed) = needed[index];
            if amount_needed <= remainder_amount {
                split.send(accept(neighbours, face, amount_needed), true);
                needed.remove(index);
            } else {
                split.send(accept(neighbours, face, remainder_amount), false);
                index += 1;
            }
        }
        let mut unsent = split.unsent();
        if unsent > 0 {
            for &(face, _) in &needed {
                let accepted = accept(neighbours, face, unsent);
                split.send(accepted, false);
                unsent -= accepted;
                if unsent == 0 {
                    return split.sent_so_far;
                }
            }
        }
    }
    split.sent_so_far
}

/// `EnergyUtils.emit(caches, container, maxOutput, null)`.
pub fn emit_energy(
    neighbours: &dyn EnergyNeighbours,
    container: &CubeEnergy,
    max_output: i32,
    sides: &[Face],
) -> i32 {
    if container.is_empty() || max_output <= 0 {
        return 0;
    }
    let to_send = {
        let simulation = Transaction::open(None);
        let to_send = container.extract(max_output, &simulation, AutomationType::Internal);
        if to_send == 0 {
            return 0;
        }
        to_send
    };
    let targets: Vec<Face> = sides
        .iter()
        .copied()
        .filter(|&face| neighbours.has_handler(face))
        .collect();
    // Os vizinhos não têm diário (o Horizon não desfaz uma inserção), então o envio roda antes de a transação do contêiner abrir.
    let sent = saturated(send_to_acceptors(neighbours, &targets, i64::from(to_send)));
    let sub = Transaction::open(None);
    if sent > 0 && container.extract(sent, &sub, AutomationType::Internal) == sent {
        sub.commit();
        return sent;
    }
    0
}

/// O ejetor do cubo vê só o contêiner de energia.
struct CubeEject<'a> {
    energy: &'a CubeEnergy,
    rate: i32,
    neighbours: &'a dyn EnergyNeighbours,
    sent: i32,
}

impl EjectEnv for CubeEject<'_> {
    fn is_empty(&self, kind: TransmissionType, _container: usize) -> bool {
        kind != TransmissionType::Energy || self.energy.is_empty()
    }

    fn emit(&mut self, kind: TransmissionType, _container: usize, sides: &[Face]) {
        if kind == TransmissionType::Energy {
            self.sent += emit_energy(self.neighbours, self.energy, self.rate, sides);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------

/// O que define um cubo: o tier (a capacidade e a taxa vêm da configuração) e o que o mod diz dos itens.
#[derive(Clone)]
pub struct EnergyCubeConfig {
    pub tier: usize,
    pub capacity: i64,
    pub transfer_rate: i32,
    pub items: ItemEnergy,
}

/// `TileEntityEnergyCube`.
pub struct EnergyCube {
    tile: Rc<RefCell<TileBase>>,
    tier: usize,
    transfer_rate: i32,
    energy: CubeEnergy,
    discharge_slot: InventorySlot,
    charge_slot: InventorySlot,
    inventory_slots: Vec<InventorySlot>,
    items: ItemEnergy,
    config: TileComponentConfig,
    ejector: TileComponentEjector,
    facing: Face,
    prev_scale: f32,
    last_energy: LastEnergyTracker,
    game_time: Rc<Cell<u64>>,
    /// `canFunction()` do ejetor (`setCanEject(_ -> canFunction())`).
    sent_last_tick: i32,
}

impl EnergyCube {
    pub fn new(config: EnergyCubeConfig) -> Self {
        let tile = Rc::new(RefCell::new(TileBase::new(0)));
        let game_time = Rc::new(Cell::new(0));
        // `getListener(ENERGY, ..)` é o próprio tile (o comparador lê a energia); os slots só marcam para salvar.
        let tile_changed: Callback = {
            let tile = tile.clone();
            Rc::new(move || tile.borrow_mut().set_changed())
        };
        let energy = CubeEnergy::new(
            config.capacity,
            config.transfer_rate,
            config.tier == CREATIVE,
            game_time.clone(),
            Some(tile_changed),
        );
        let items = config.items;
        // `EnergyInventorySlot.fillOrConvert`: quem está de fora só tira o que o slot não consegue encher ou converter.
        let discharge_slot = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks::fill_or_convert({
                let (items, energy) = (items.clone(), energy.clone());
                Rc::new(move |item: &ItemResource| items.can_fill_or_convert(&energy, item))
            }),
            None,
            None,
        );
        // `EnergyInventorySlot.drain`.
        let can_drain = {
            let (items, energy) = (items.clone(), energy.clone());
            move |item: &ItemResource| items.can_drain(&energy, item)
        };
        let extract_by: crate::containers::AutomationItemTest = {
            let can_drain = can_drain.clone();
            Rc::new(move |item, automation| {
                automation != AutomationType::External || !can_drain(item)
            })
        };
        let insert_by: crate::containers::AutomationItemTest = Rc::new(move |item, automation| {
            automation == AutomationType::Internal || can_drain(item)
        });
        let charge_slot = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks {
                extract_by: Some(extract_by),
                insert_by: Some(insert_by),
                ..SlotChecks::default()
            },
            None,
            None,
        );
        let inventory_slots = vec![discharge_slot.clone(), charge_slot.clone()];
        // `setupIOConfig(ITEM, chargeSlot, dischargeSlot, true).setCanEject(false)` e `setupIOConfig(ENERGY, energyContainer)`.
        let spec = side_config_spec();
        let mut component = TileComponentConfig::new(&spec.types);
        (spec.setup)(&mut component);
        let mut ejector = TileComponentEjector::new();
        ejector.set_output_data(&component, &spec.eject);
        // `setCanEject(_ -> canFunction())`.
        let function_tile = tile.clone();
        ejector.set_can_eject(move |_| function_tile.borrow().is_redstone_activated());
        Self {
            tile,
            tier: config.tier,
            transfer_rate: config.transfer_rate,
            energy,
            discharge_slot,
            charge_slot,
            inventory_slots,
            items,
            config: component,
            ejector,
            facing: Face::North,
            prev_scale: 0.0,
            last_energy: LastEnergyTracker::default(),
            game_time,
            sent_last_tick: 0,
        }
    }

    pub fn tier(&self) -> usize {
        self.tier
    }

    pub fn is_creative(&self) -> bool {
        self.tier == CREATIVE
    }

    pub fn transfer_rate(&self) -> i32 {
        self.transfer_rate
    }

    pub fn tile(&self) -> Ref<'_, TileBase> {
        self.tile.borrow()
    }

    pub fn tile_mut(&self) -> RefMut<'_, TileBase> {
        self.tile.borrow_mut()
    }

    pub fn energy(&self) -> &CubeEnergy {
        &self.energy
    }

    pub fn charge_slot(&self) -> &InventorySlot {
        &self.charge_slot
    }

    pub fn discharge_slot(&self) -> &InventorySlot {
        &self.discharge_slot
    }

    pub fn inventory_slots(&self) -> &[InventorySlot] {
        &self.inventory_slots
    }

    pub fn items(&self) -> &ItemEnergy {
        &self.items
    }

    pub fn side_config(&self) -> &TileComponentConfig {
        &self.config
    }

    pub fn side_config_mut(&mut self) -> &mut TileComponentConfig {
        &mut self.config
    }

    pub fn ejector(&self) -> &TileComponentEjector {
        &self.ejector
    }

    pub fn ejector_mut(&mut self) -> &mut TileComponentEjector {
        &mut self.ejector
    }

    pub fn facing(&self) -> Face {
        self.facing
    }

    pub fn set_facing(&mut self, facing: Face) {
        self.facing = facing;
    }

    /// `getEnergyScale`.
    pub fn energy_scale(&self) -> f32 {
        self.prev_scale
    }

    /// `handleUpdateTag`: a escala que o servidor mandou.
    pub fn set_energy_scale(&mut self, scale: f32) {
        self.prev_scale = scale;
    }

    pub fn side_states(&self) -> [CubeSideState; 6] {
        side_states(&self.config)
    }

    /// `getRedstoneLevel`.
    pub fn redstone_level(&self) -> i32 {
        redstone_level_from_contents(self.energy.amount(), self.energy.capacity())
    }

    /// O que a capability de energia entregou (`TrackingEnergyHandler.insert`).
    pub fn energy_received(&self, inserted: i64, game_time: u64) {
        self.last_energy.received(inserted, game_time);
    }

    pub fn last_energy_received(&self) -> i64 {
        self.last_energy.last_energy_received()
    }

    /// `getInputRate`-like: o que o ejetor mandou no tick.
    pub fn sent_last_tick(&self) -> i32 {
        self.sent_last_tick
    }

    pub fn apply_item_side_config(
        &mut self,
        side_config: Option<&AttachedSideConfig>,
        ejector: Option<&AttachedEjector>,
        loaded: bool,
    ) {
        self.config
            .apply_implicit_components(side_config, self.facing, loaded);
        self.ejector.apply_implicit_components(ejector);
    }

    pub fn collect_item_side_config(&self) -> (AttachedSideConfig, AttachedEjector) {
        (
            self.config.collect_implicit_components(),
            self.ejector.collect_implicit_components(),
        )
    }

    pub fn restore_side_config(
        &mut self,
        config: Option<&ConfigData>,
        ejector: Option<&EjectorData>,
        loaded: bool,
    ) {
        if let Some(config) = config {
            self.config.deserialize(config, self.facing, loaded);
        }
        if let Some(ejector) = ejector {
            self.ejector.deserialize(ejector);
        }
    }

    pub fn write_configuration_data(&self) -> ConfigurationData {
        let ejector = self.ejector.serialize();
        ConfigurationData {
            control_type: Some(self.tile.borrow().control_type()),
            config: Some(self.config.serialize()),
            ejector: (!ejector.is_empty()).then_some(ejector),
        }
    }

    pub fn set_configuration_data(&mut self, data: &ConfigurationData) {
        self.tile
            .borrow_mut()
            .set_control_type(data.control_type.unwrap_or_default());
        self.restore_side_config(data.config.as_ref(), data.ejector.as_ref(), true);
    }

    pub fn take_side_changes(&mut self) -> Vec<crate::side_config::SideChange> {
        self.config.take_changes()
    }

    /// `TileEntityMekanism.tickServer` do cubo. Devolve se o comparador mudou (o tile avisa os vizinhos) e, em `scale_changed`, se a
    /// escala pede um pacote de atualização.
    pub fn tick_server(&mut self, game_time: u64, neighbours: &dyn EnergyNeighbours) -> TickResult {
        self.game_time.set(game_time);
        self.tile.borrow_mut().tick_delay();
        // `TileEntityConfigurableMachine.onUpdateServer`: o ejetor.
        let mut eject = CubeEject {
            energy: &self.energy,
            rate: self.transfer_rate,
            neighbours,
            sent: 0,
        };
        self.ejector.tick_server(
            &self.config,
            self.facing,
            &self.inventory_slots,
            &NoNeighbours,
            &mut eject,
        );
        self.sent_last_tick = eject.sent;
        // `TileEntityEnergyCube.onUpdateServer`.
        self.items
            .drain_container_into_slot(&self.charge_slot, &self.energy, None);
        self.items
            .fill_container_or_convert(&self.discharge_slot, &self.energy, None);
        let new_scale = get_scale(self.prev_scale, self.energy.amount(), self.energy.capacity());
        let mut scale_update = false;
        if scale_changed(new_scale, self.prev_scale) {
            self.prev_scale = new_scale;
            scale_update = true;
        }
        self.last_energy.check_tick_changed(game_time);
        let level = self.redstone_level();
        let comparator_changed = self.tile.borrow_mut().finish_tick(|| level);
        TickResult {
            comparator_changed,
            scale_update,
        }
    }
}

/// O que o tick avisa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickResult {
    pub comparator_changed: bool,
    pub scale_update: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::side_config::{DataType, LightConfigInfo};

    #[test]
    fn the_scale_walks_a_tenth_of_the_way_and_snaps_when_close() {
        // `getScale`: longe do alvo anda um décimo; perto dele cai nele só se cheio, ou se estava em zero e há algo.
        assert_eq!(get_scale(0.0, 500, 1000), 0.05);
        let step = get_scale(0.5, 1000, 1000);
        assert!((step - 0.55).abs() < 1.0e-6, "{step}");
        assert_eq!(get_scale(0.995, 1000, 1000), 1.0);
        assert_eq!(get_scale(0.005, 0, 1000), 0.0);
        assert_eq!(get_scale(0.3, 300, 1000), 0.3);
        assert_eq!(get_scale(0.0, 0, 0), 0.0);
    }

    #[test]
    fn a_scale_change_is_a_difference_or_a_move_to_or_from_the_ends() {
        assert!(!scale_changed(0.5, 0.5));
        assert!(scale_changed(0.6, 0.5));
        assert!(scale_changed(1.0, 0.999_999));
        assert!(scale_changed(0.0, 0.000_001));
        assert!(!scale_changed(0.5, 0.500_001));
    }

    #[test]
    fn the_comparator_level_is_the_discrete_lerp_of_the_fraction() {
        assert_eq!(redstone_level_from_contents(0, 100), 0);
        assert_eq!(redstone_level_from_contents(1, 1_000_000), 1);
        assert_eq!(redstone_level_from_contents(100, 100), 15);
        assert_eq!(redstone_level_from_contents(50, 100), 8);
        assert_eq!(redstone_level_from_contents(5, 0), 0);
    }

    #[test]
    fn the_item_default_config_lights_the_front_and_leaves_the_rest_unlit() {
        let spec = side_config_spec();
        let mut config = TileComponentConfig::new(&spec.types);
        (spec.setup)(&mut config);
        let attached = AttachedSideConfig::new(&[
            (TransmissionType::Item, LightConfigInfo::create(Some(RelativeSide::Front), None, None, false)),
            (TransmissionType::Energy, LightConfigInfo::create(Some(RelativeSide::Front), None, None, true)),
        ]);
        config.apply_implicit_components(Some(&attached), Face::North, false);
        let states = side_states(&config);
        assert_eq!(states[RelativeSide::Front.ordinal()], CubeSideState::ActiveLit);
        for side in [RelativeSide::Left, RelativeSide::Right, RelativeSide::Back, RelativeSide::Top, RelativeSide::Bottom] {
            assert_eq!(states[side.ordinal()], CubeSideState::ActiveUnlit, "{side:?}");
        }
        config.config_mut(TransmissionType::Energy).unwrap().set_data_type(DataType::None, RelativeSide::Top);
        assert_eq!(side_states(&config)[RelativeSide::Top.ordinal()], CubeSideState::Inactive);
    }

    struct Receivers(RefCell<Vec<(Face, i64, i64)>>);

    impl EnergyNeighbours for Receivers {
        fn has_handler(&self, face: Face) -> bool {
            self.0.borrow().iter().any(|(at, _, _)| *at == face)
        }

        fn insert(&self, face: Face, amount: i32, simulate: bool) -> i32 {
            let mut all = self.0.borrow_mut();
            let Some((_, stored, capacity)) = all.iter_mut().find(|(at, _, _)| *at == face) else {
                return 0;
            };
            let accepted = amount.min(saturated(*capacity - *stored));
            if !simulate {
                *stored += i64::from(accepted);
            }
            accepted
        }
    }

    #[test]
    fn a_split_gives_the_small_want_what_it_asks_and_shares_the_rest() {
        // 1000 para quatro, dois pedem 100 e 300: os outros dois dividem as 600 que sobram.
        let receivers = Receivers(RefCell::new(vec![
            (Face::North, 0, 100),
            (Face::South, 0, 300),
            (Face::West, 0, 10_000),
            (Face::East, 0, 10_000),
        ]));
        let sent = send_to_acceptors(&receivers, &[Face::North, Face::South, Face::West, Face::East], 1000);
        assert_eq!(sent, 1000);
        let all = receivers.0.borrow();
        assert_eq!(all.iter().map(|(_, stored, _)| *stored).collect::<Vec<_>>(), vec![100, 300, 300, 300]);
    }

    #[test]
    fn one_target_takes_all_it_can() {
        let receivers = Receivers(RefCell::new(vec![(Face::North, 0, 700)]));
        assert_eq!(send_to_acceptors(&receivers, &[Face::North], 1000), 700);
    }
}
