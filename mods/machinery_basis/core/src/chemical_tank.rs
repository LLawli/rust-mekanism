//! Porte de `TileEntityChemicalTank`: o tanque (`ChemicalTankChemicalTank`: o limite de taxa só para quem é interno e o criativo que
//! aceita e entrega sem mudar), os dois slots (`ChemicalInventorySlot.drain` e `.fill`), a configuração de lados com `CHEMICAL` e
//! `ITEM`, o ejetor de químico (`ResourceUtils.emit` entre as faces de saída), o modo de descarte (`GasMode`, `ChemicalUtils.dump`)
//! e o comparador.
//!
//! A lib não conhece nenhum químico nem item: os valores do tier, os atributos dos químicos e a capability dos itens vêm de quem usa.
use crate::chemical::{ChemicalAttributeValidator, ChemicalAttributes, ChemicalResource};
use crate::containers::{
    Access, AutomationType, InventorySlot, ItemResource, SlotChecks, SlotContents,
    redstone_signal_of_container,
};
use crate::distribution::{
    ResourceHandlerKind, ResourceInsert, Target, saturated_cast, send_to_acceptors,
};
use crate::ejector::{AttachedEjector, EjectEnv, EjectorData, TileComponentEjector};
use crate::handler_slot::{HandlerSlot, ResourceContainer, can_drain, can_fill};
use crate::item_access::ItemHandlerLookup;
use crate::item_handler::NoNeighbours;
use crate::rate_limit::RateLimit;
use crate::resource_tank::{ResourceTank, StackContents};
use crate::side_config::{
    AttachedSideConfig, ConfigData, DataType, RelativeSide, TileComponentConfig, TransmissionType,
};
use crate::sides::Face;
use crate::tile::{ConfigurationData, SideConfigSpec, TileBase};
use crate::transaction::{Callback, Transaction};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;

/// `getInitialInventory`: o slot que enche o item (`drain`) vem primeiro, o que o esvazia (`fill`) depois.
pub const DRAIN_SLOT: usize = 0;
pub const FILL_SLOT: usize = 1;
/// O índice do tanque de químico, o único.
pub const CHEMICAL_TANK: usize = 0;
/// `tier.getCapacity() / 400`: o divisor do que o modo `DUMPING` joga fora por tick.
pub const DUMP_DIVISOR: u64 = 400;

/// `TileEntityChemicalTank.GasMode`, na ordem do enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GasMode {
    #[default]
    Idle,
    DumpingExcess,
    Dumping,
}

impl GasMode {
    pub const VALUES: [Self; 3] = [Self::Idle, Self::DumpingExcess, Self::Dumping];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `BY_ID`, com `OutOfBoundsStrategy.ZERO`.
    pub fn from_ordinal(ordinal: usize) -> Self {
        Self::VALUES.get(ordinal).copied().unwrap_or_default()
    }

    /// `getNext`.
    pub fn next(self) -> Self {
        Self::VALUES[(self.ordinal() + 1) % 3]
    }

    /// `getPrevious`.
    pub fn previous(self) -> Self {
        Self::VALUES[(self.ordinal() + 2) % 3]
    }

    pub fn serialized_name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::DumpingExcess => "dumping_excess",
            Self::Dumping => "dumping",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|mode| mode.serialized_name() == name)
    }
}

/// `ChemicalUtils.dump(chemicalTank, dumpMode, dumpingAmount, dumpExcessRate)`. `keep_ratio` é `general.dumpExcessKeepRatio`.
pub fn dump(
    tank: &ResourceTank<ChemicalResource>,
    mode: GasMode,
    dumping_amount: u64,
    dump_excess_rate: u64,
    keep_ratio: f64,
) {
    let Some(resource) = tank.resource() else {
        return;
    };
    let amount = tank.amount();
    let to_dump = match mode {
        GasMode::Idle => return,
        GasMode::Dumping => dumping_amount,
        GasMode::DumpingExcess => {
            // `MathUtils.clampToLong(capacity * ratio)`: a conta em `double`, e o `as` do Rust satura como o clamp.
            let target = (tank.capacity(&resource) as f64 * keep_ratio) as u64;
            if target < amount {
                (amount - target).min(dump_excess_rate)
            } else {
                0
            }
        }
    };
    if to_dump > 0 {
        tank.set_contents(Some(resource), amount.saturating_sub(to_dump), None);
    }
}

/// `ChemicalTankChemicalTank`.
#[derive(Clone)]
pub struct ChemicalTankTank {
    inner: ResourceTank<ChemicalResource>,
    creative: bool,
}

impl ChemicalTankTank {
    /// `ChemicalTankChemicalTank.create(tile, listener)`: o criativo aceita todo químico (`ALWAYS_ALLOW`), os outros recusam o
    /// que pede validação (`DEFAULT`).
    pub fn new(
        capacity: u64,
        rate: i32,
        creative: bool,
        attributes: ChemicalAttributes,
        game_time: Rc<Cell<u64>>,
        listener: Option<Callback>,
    ) -> Self {
        let attribute_validator = if creative {
            ChemicalAttributeValidator::always_allow()
        } else {
            ChemicalAttributeValidator::default_rule()
        };
        let inner = ResourceTank::create_rate_limited(
            capacity,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(move |resource| attribute_validator.process(resource, &attributes)),
            Some(RateLimit::new(game_time.clone(), rate)),
            Some(RateLimit::new(game_time, rate)),
            listener,
        );
        Self { inner, creative }
    }

    pub fn inner(&self) -> &ResourceTank<ChemicalResource> {
        &self.inner
    }

    pub fn is_creative(&self) -> bool {
        self.creative
    }

    pub fn amount(&self) -> u64 {
        self.inner.amount()
    }

    /// `capacityAsLong(resource())`.
    pub fn capacity(&self) -> u64 {
        self.inner.capacity_for(self.inner.resource().as_ref())
    }
}

impl ResourceContainer<ChemicalResource> for ChemicalTankTank {
    fn resource(&self) -> Option<ChemicalResource> {
        self.inner.resource()
    }

    fn amount_as_int(&self) -> i32 {
        self.inner.amount_as_int()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn capacity_as_int(&self, resource: Option<&ChemicalResource>) -> i32 {
        ResourceContainer::capacity_as_int(&self.inner, resource)
    }

    fn needed_as_int(&self, resource: &ChemicalResource) -> i32 {
        self.inner.needed_as_int(Some(resource))
    }

    fn is_valid_for_insertion(
        &self,
        resource: &ChemicalResource,
        automation: AutomationType,
    ) -> bool {
        self.inner.is_valid_for_insertion(resource, automation)
    }

    fn insert(
        &self,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if self.creative {
            if self.inner.is_empty() && automation != AutomationType::External {
                // Um jogador, ou um slot do próprio tanque, define o tipo do criativo vazio; para a automação ele é um vazio que some.
                {
                    let simulation = Transaction::open(Some(transaction));
                    if self.inner.insert(resource, amount, &simulation, automation) == 0 {
                        return 0;
                    }
                }
                self.inner.set_contents(
                    Some(resource.clone()),
                    self.inner.capacity(resource),
                    Some(transaction),
                );
                return amount;
            }
            let simulation = Transaction::open(Some(transaction));
            return self.inner.insert(resource, amount, &simulation, automation);
        }
        self.inner.insert(resource, amount, transaction, automation)
    }

    fn extract(
        &self,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if self.creative {
            let simulation = Transaction::open(Some(transaction));
            return self
                .inner
                .extract(resource, amount, &simulation, automation);
        }
        self.inner
            .extract(resource, amount, transaction, automation)
    }
}

/// Os handlers de químico dos vizinhos, pela face do tanque (`Capabilities.CHEMICAL` do bloco do outro lado, pela face oposta).
pub trait ChemicalNeighbours {
    fn handler(&self, side: Face) -> Option<Rc<dyn ResourceInsert<ChemicalResource>>>;
}

/// Sem vizinho nenhum.
pub struct NoChemicalNeighbours;

impl ChemicalNeighbours for NoChemicalNeighbours {
    fn handler(&self, _side: Face) -> Option<Rc<dyn ResourceInsert<ChemicalResource>>> {
        None
    }
}

/// `ResourceUtils.emit(targets, container, maxOutput, null)`: o que o contêiner entrega por dentro, dividido entre os handlers das
/// faces dadas.
pub fn emit_chemical(
    neighbours: &dyn ChemicalNeighbours,
    sides: &[Face],
    container: &dyn ResourceContainer<ChemicalResource>,
    max_output: i32,
) -> i32 {
    let Some(resource) = container.resource() else {
        return 0;
    };
    if max_output <= 0 || sides.is_empty() {
        return 0;
    }
    let amount = {
        let simulation = Transaction::open(None);
        container.extract(&resource, max_output, &simulation, AutomationType::Internal)
    };
    if amount == 0 {
        return 0;
    }
    let sub = Transaction::open(None);
    // `EmitUtils.getHandlersFromCaches`: os que têm handler agora, na ordem das faces.
    let handlers: Vec<Rc<dyn ResourceInsert<ChemicalResource>>> = sides
        .iter()
        .filter_map(|&side| neighbours.handler(side))
        .collect();
    let sent = match handlers.as_slice() {
        [] => 0,
        [only] => {
            let single = Transaction::open(Some(&sub));
            let sent = only.insert(&resource, amount, &single);
            single.commit();
            sent
        }
        _ => {
            let mut target = Target::with_handlers(ResourceHandlerKind::new(), handlers);
            saturated_cast(send_to_acceptors(
                Some(&mut target),
                i64::from(amount),
                &resource,
                Some(&sub),
            ))
        }
    };
    if sent > 0 && container.extract(&resource, sent, &sub, AutomationType::Internal) == sent {
        sub.commit();
        return sent;
    }
    0
}

/// `configComponent.setupIOConfig(ITEM, drainSlot, fillSlot, true).setCanEject(false)`, `setupIOConfig(CHEMICAL, chemicalTank)` e
/// `ejectorComponent.setOutputData(configComponent, CHEMICAL)`.
pub fn side_config_spec() -> SideConfigSpec {
    SideConfigSpec {
        types: vec![TransmissionType::Item, TransmissionType::Chemical],
        setup: |config| {
            config.setup_io_config_two(TransmissionType::Item, DRAIN_SLOT, FILL_SLOT, true, true);
            if let Some(info) = config.config_mut(TransmissionType::Item) {
                info.set_can_eject(false);
            }
            config.setup_io_config(TransmissionType::Chemical, CHEMICAL_TANK, false);
        },
        eject: vec![TransmissionType::Chemical],
    }
}

/// O que o tile precisa para nascer.
pub struct ChemicalTankConfig {
    /// `tier.getCapacity()` e `tier.getTransferRate()`.
    pub capacity: u64,
    pub rate: i32,
    pub creative: bool,
    /// `MekanismConfig.general.dumpExcessKeepRatio`.
    pub dump_excess_keep_ratio: f64,
    pub attributes: ChemicalAttributes,
    /// `Capabilities.CHEMICAL.item()`.
    pub items: ItemHandlerLookup<ChemicalResource>,
}

/// O que o item do tanque leva do tile além da configuração de lados: `mekanism:chemicals`, `mekanism:items` e `mekanism:dump_mode`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChemicalTankItem {
    pub chemical: StackContents<ChemicalResource>,
    /// O slot `drain` e o `fill`.
    pub slots: [SlotContents; 2],
    pub dumping: GasMode,
}

/// O que o tick avisa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickResult {
    pub comparator_changed: bool,
    /// O que o ejetor mandou neste tick.
    pub ejected: i32,
}

struct TankEject<'a> {
    tank: &'a ChemicalTankTank,
    rate: i32,
    neighbours: &'a dyn ChemicalNeighbours,
    sent: i32,
}

impl EjectEnv for TankEject<'_> {
    fn is_empty(&self, kind: TransmissionType, _container: usize) -> bool {
        kind != TransmissionType::Chemical || self.tank.is_empty()
    }

    fn emit(&mut self, kind: TransmissionType, _container: usize, sides: &[Face]) {
        if kind == TransmissionType::Chemical {
            self.sent += emit_chemical(self.neighbours, sides, self.tank, self.rate);
        }
    }
}

/// `TileEntityChemicalTank`.
pub struct ChemicalTankTile {
    tile: Rc<RefCell<TileBase>>,
    tank: ChemicalTankTank,
    drain: HandlerSlot,
    fill: HandlerSlot,
    inventory_slots: Vec<InventorySlot>,
    items: ItemHandlerLookup<ChemicalResource>,
    config: TileComponentConfig,
    ejector: TileComponentEjector,
    facing: Face,
    dumping: Rc<Cell<GasMode>>,
    capacity: u64,
    rate: i32,
    dump_excess_keep_ratio: f64,
    game_time: Rc<Cell<u64>>,
}

impl ChemicalTankTile {
    pub fn new(config: ChemicalTankConfig) -> Self {
        let tile = Rc::new(RefCell::new(TileBase::new(0)));
        let game_time = Rc::new(Cell::new(0));
        // `makesComparatorDirty(CHEMICAL)`: só o tanque suja o comparador.
        let tile_changed: Callback = {
            let tile = tile.clone();
            Rc::new(move || tile.borrow_mut().set_changed())
        };
        let tank = ChemicalTankTank::new(
            config.capacity,
            config.rate,
            config.creative,
            config.attributes,
            game_time.clone(),
            Some(tile_changed),
        );
        let items = config.items;
        // `ChemicalInventorySlot.drain` e `.fill`: por fora só entra o item que o slot pode encher (ou esvaziar), e só sai o que não pode mais.
        let slot = |accepts: fn(
            &ChemicalTankTank,
            &ItemResource,
            &ItemHandlerLookup<ChemicalResource>,
        ) -> bool| {
            let tank = tank.clone();
            let items = items.clone();
            HandlerSlot::new(None, move |listener| {
                let test: crate::containers::ItemTest =
                    Rc::new(move |item: &ItemResource| accepts(&tank, item, &items));
                InventorySlot::with_checks(
                    Access::Any,
                    Access::Any,
                    SlotChecks::fill_or_convert(test),
                    listener,
                    None,
                )
            })
        };
        let drain = slot(|tank, item, items| can_drain(tank, item, items));
        let fill = slot(|tank, item, items| can_fill(tank, item, items));
        let inventory_slots = vec![drain.slot().clone(), fill.slot().clone()];
        let spec = side_config_spec();
        let mut component = TileComponentConfig::new(&spec.types);
        (spec.setup)(&mut component);
        let mut ejector = TileComponentEjector::new();
        ejector.set_output_data(&component, &spec.eject);
        // `setCanEject(_ -> canFunction() && (tier.isCreative() || dumping != GasMode.DUMPING))`.
        let dumping = Rc::new(Cell::new(GasMode::Idle));
        {
            let tile = tile.clone();
            let dumping = dumping.clone();
            let creative = config.creative;
            ejector.set_can_eject(move |_| {
                tile.borrow().is_redstone_activated()
                    && (creative || dumping.get() != GasMode::Dumping)
            });
        }
        Self {
            tile,
            tank,
            drain,
            fill,
            inventory_slots,
            items,
            config: component,
            ejector,
            facing: Face::North,
            dumping,
            capacity: config.capacity,
            rate: config.rate,
            dump_excess_keep_ratio: config.dump_excess_keep_ratio,
            game_time,
        }
    }

    pub fn tile(&self) -> Ref<'_, TileBase> {
        self.tile.borrow()
    }

    pub fn tile_mut(&self) -> RefMut<'_, TileBase> {
        self.tile.borrow_mut()
    }

    pub fn tank(&self) -> &ChemicalTankTank {
        &self.tank
    }

    pub fn drain_slot(&self) -> &HandlerSlot {
        &self.drain
    }

    pub fn fill_slot(&self) -> &HandlerSlot {
        &self.fill
    }

    pub fn inventory_slots(&self) -> &[InventorySlot] {
        &self.inventory_slots
    }

    pub fn side_config(&self) -> &TileComponentConfig {
        &self.config
    }

    pub fn side_config_mut(&mut self) -> &mut TileComponentConfig {
        &mut self.config
    }

    pub fn side_parts_mut(&mut self) -> (&mut TileComponentConfig, &mut TileComponentEjector) {
        (&mut self.config, &mut self.ejector)
    }

    pub fn ejector(&self) -> &TileComponentEjector {
        &self.ejector
    }

    pub fn facing(&self) -> Face {
        self.facing
    }

    pub fn set_facing(&mut self, facing: Face) {
        self.facing = facing;
    }

    /// `dumping`.
    /// `MekanismConfig.general.dumpExcessKeepRatio`, que o original lê a cada descarte: quem dá o tick entrega o valor de agora.
    pub fn set_dump_excess_keep_ratio(&mut self, ratio: f64) {
        self.dump_excess_keep_ratio = ratio;
    }

    pub fn dumping(&self) -> GasMode {
        self.dumping.get()
    }

    pub fn set_dumping(&mut self, mode: GasMode) {
        self.dumping.set(mode);
    }

    /// `nextMode(0)`.
    pub fn next_mode(&mut self) {
        self.dumping.set(self.dumping.get().next());
    }

    /// `getRedstoneLevel`.
    pub fn redstone_level(&self) -> i32 {
        redstone_signal_of_container(self.tank.amount(), self.tank.capacity())
    }

    /// O `DataType` de químico da face, ou `None` se a capability não existe ali (`TileComponentConfig.isCapabilityDisabled`).
    fn chemical_side(&self, face: Face) -> Option<DataType> {
        let info = self.config.config(TransmissionType::Chemical)?;
        let data = info.data_type(RelativeSide::from_directions(self.facing, face));
        (data != DataType::None).then_some(data)
    }

    fn slot_allows(&self, face: Face, input: bool) -> Option<bool> {
        let data = self.chemical_side(face)?;
        let info = self.config.config(TransmissionType::Chemical)?;
        Some(info.slot_info_of(data).is_some_and(|slot| {
            if input {
                slot.can_input
            } else {
                slot.can_output
            }
        }))
    }

    /// O `insert` do handler de químico do bloco numa face: `None` sem capability ali.
    pub fn handler_insert(
        &self,
        face: Face,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
    ) -> Option<i32> {
        let allowed = self.slot_allows(face, true)?;
        Some(if allowed {
            self.tank
                .insert(resource, amount, transaction, AutomationType::External)
        } else {
            0
        })
    }

    /// O `extract` do handler de químico do bloco numa face: `None` sem capability ali.
    pub fn handler_extract(
        &self,
        face: Face,
        resource: &ChemicalResource,
        amount: i32,
        transaction: &Transaction,
    ) -> Option<i32> {
        let allowed = self.slot_allows(face, false)?;
        Some(if allowed {
            self.tank
                .extract(resource, amount, transaction, AutomationType::External)
        } else {
            0
        })
    }

    /// Se o bloco tem capability de químico na face (`TileComponentConfig.isCapabilityDisabled` ao contrário).
    pub fn has_chemical_capability(&self, face: Face) -> bool {
        self.chemical_side(face).is_some()
    }

    /// Se a face deixa o químico entrar e sair, ou `None` sem capability ali.
    pub fn chemical_face(&self, face: Face) -> Option<(bool, bool)> {
        Some((
            self.slot_allows(face, true)?,
            self.slot_allows(face, false)?,
        ))
    }

    /// `ITileComponent.read` da configuração de lados e do ejetor (`component_config` e `component_ejector` do NBT).
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

    /// `writeConfigurationData`: o modo de redstone e o de descarte (`writeSustainedData`) e os dois componentes.
    pub fn write_configuration_data(&self) -> (ConfigurationData, GasMode) {
        let ejector = self.ejector.serialize();
        (
            ConfigurationData {
                control_type: Some(self.tile.borrow().control_type()),
                config: Some(self.config.serialize()),
                ejector: (!ejector.is_empty()).then_some(ejector),
            },
            self.dumping.get(),
        )
    }

    /// `setConfigurationData`: o modo de redstone volta a `DISABLED` e o de descarte a `IDLE` se faltavam (`readSustainedData`).
    pub fn set_configuration_data(&mut self, data: &ConfigurationData, dumping: Option<GasMode>) {
        self.tile
            .borrow_mut()
            .set_control_type(data.control_type.unwrap_or_default());
        self.dumping.set(dumping.unwrap_or_default());
        self.restore_side_config(data.config.as_ref(), data.ejector.as_ref(), true);
    }

    /// Quem foi mudando na configuração desde a última chamada.
    pub fn take_side_changes(&mut self) -> Vec<crate::side_config::SideChange> {
        self.config.take_changes()
    }

    /// `applyImplicitComponents` da configuração de lados e do ejetor (`mekanism:side_config` e `mekanism:ejector`).
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

    /// `collectImplicitComponents` da configuração de lados e do ejetor.
    pub fn collect_item_side_config(&self) -> (AttachedSideConfig, AttachedEjector) {
        (
            self.config.collect_implicit_components(),
            self.ejector.collect_implicit_components(),
        )
    }

    /// `collectImplicitComponents`, fora a configuração de lados e o ejetor.
    pub fn collect_item(&self) -> ChemicalTankItem {
        ChemicalTankItem {
            chemical: self.tank.inner().contents(),
            slots: [self.drain.slot().contents(), self.fill.slot().contents()],
            dumping: self.dumping.get(),
        }
    }

    /// `applyImplicitComponents`, fora a configuração de lados e o ejetor.
    pub fn apply_item(&mut self, item: &ChemicalTankItem) {
        self.tank
            .inner()
            .set_contents(item.chemical.resource.clone(), item.chemical.amount, None);
        for (slot, contents) in self.inventory_slots.iter().zip(&item.slots) {
            slot.set_contents(contents.resource.clone(), contents.amount, None);
        }
        self.dumping.set(item.dumping);
    }

    /// `onUpdateServer` (com o ejetor de `TileEntityConfigurableMachine`), e o trecho de `tickServer` depois dele.
    pub fn tick_server(
        &mut self,
        game_time: u64,
        neighbours: &dyn ChemicalNeighbours,
    ) -> TickResult {
        self.game_time.set(game_time);
        self.tile.borrow_mut().tick_delay();
        let mut eject = TankEject {
            tank: &self.tank,
            rate: self.rate,
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
        let ejected = eject.sent;
        self.drain
            .drain_container_into_slot(&self.tank, &self.items, None);
        self.fill
            .fill_container_from_slot(&self.tank, &self.items, None);
        let mode = self.dumping.get();
        if mode != GasMode::Idle && !self.tank.is_creative() && !self.tank.is_empty() {
            dump(
                self.tank.inner(),
                mode,
                self.capacity / DUMP_DIVISOR,
                u64::try_from(self.rate).unwrap_or(0),
                self.dump_excess_keep_ratio,
            );
        }
        let level = self.redstone_level();
        let comparator_changed = self.tile.borrow_mut().finish_tick(|| level);
        TickResult {
            comparator_changed,
            ejected,
        }
    }
}
