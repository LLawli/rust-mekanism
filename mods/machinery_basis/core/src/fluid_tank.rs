//! Porte de `TileEntityFluidTank`: o tanque (`FluidTankFluidTank`: o limite de taxa só para quem é interno, o criativo que aceita sem
//! guardar e o que não coube indo para o tanque de cima), o diário da válvula do topo (`ValveJournal`, `ValveFluidTankWrapper`), os
//! dois slots (`FluidInventorySlot.input` com `handleTank` e o `OutputInventorySlot`), o modo de edição, o empurrão para baixo de um
//! tanque ativo (`ResourceUtils.emit` no handler de baixo), a escala que anima, o atraso da luz e o comparador.
//!
//! A lib não conhece nenhum fluido nem item: os valores do tier e a capability de fluido dos itens vêm de quem usa.
use crate::containers::{
    Access, AutomationItemTest, AutomationType, InventorySlot, ItemResource, SlotChecks,
    SlotContents, redstone_signal_of_container,
};
use crate::energy_cube::{scale_changed, scale_towards};
use crate::fluid::FluidResource;
use crate::handler_slot::{ContainerEditMode, HandlerSlot, ResourceContainer, can_input};
use crate::item_access::ItemHandlerLookup;
use crate::rate_limit::RateLimit;
use crate::resource_tank::{ResourceTank, StackContents};
use crate::tile::TileBase;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;

/// `getInitialInventory`: o slot de entrada vem primeiro, o de saída depois.
pub const INPUT_SLOT: usize = 0;
pub const OUTPUT_SLOT: usize = 1;
/// `SharedConstants.TICKS_PER_SECOND`: quanto a válvula fica marcada depois de uma entrada pelo topo.
pub const VALVE_TICKS: i32 = 20;

/// Quem acha o tanque de cima na hora da inserção (`WorldUtils.getTileEntity(TileEntityFluidTank.class, level, pos.above())`).
pub type AboveTank = Rc<dyn Fn() -> Option<FluidTankTank>>;

/// `FluidTankFluidTank`.
#[derive(Clone)]
pub struct FluidTankTank {
    inner: ResourceTank<FluidResource>,
    creative: bool,
    above: Rc<RefCell<Option<AboveTank>>>,
}

impl FluidTankTank {
    /// `FluidTankFluidTank.create(tile, listener)`: `capacity` e `rate` são os do tier (`getCapacity`, `getTransferRate`).
    pub fn new(
        capacity: u64,
        rate: i32,
        creative: bool,
        game_time: Rc<Cell<u64>>,
        listener: Option<Callback>,
    ) -> Self {
        // Só o que é interno é limitado: a taxa com que um item num slot enche ou esvazia o tanque.
        let inner = ResourceTank::create_rate_limited(
            capacity,
            Rc::new(|_, _| true),
            Rc::new(|_, _| true),
            Rc::new(|_| true),
            Some(RateLimit::new(game_time.clone(), rate)),
            Some(RateLimit::new(game_time, rate)),
            listener,
        );
        Self {
            inner,
            creative,
            above: Rc::new(RefCell::new(None)),
        }
    }

    /// Diz como achar o tanque de cima. Sem isso, o que não cabe não vai a lugar nenhum.
    pub fn set_above(&self, above: AboveTank) {
        *self.above.borrow_mut() = Some(above);
    }

    /// O contêiner por baixo, para ler e para o `setContents` de quem carrega o tile.
    pub fn inner(&self) -> &ResourceTank<FluidResource> {
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

impl ResourceContainer<FluidResource> for FluidTankTank {
    fn resource(&self) -> Option<FluidResource> {
        self.inner.resource()
    }

    fn amount_as_int(&self) -> i32 {
        self.inner.amount_as_int()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn capacity_as_int(&self, resource: Option<&FluidResource>) -> i32 {
        ResourceContainer::capacity_as_int(&self.inner, resource)
    }

    fn needed_as_int(&self, resource: &FluidResource) -> i32 {
        self.inner.needed_as_int(Some(resource))
    }

    fn is_valid_for_insertion(&self, resource: &FluidResource, automation: AutomationType) -> bool {
        self.inner.is_valid_for_insertion(resource, automation)
    }

    fn insert(
        &self,
        resource: &FluidResource,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        let mut inserted;
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
            // Aceita sem mudar o conteúdo.
            let simulation = Transaction::open(Some(transaction));
            inserted = self.inner.insert(resource, amount, &simulation, automation);
        } else {
            inserted = self.inner.insert(resource, amount, transaction, automation);
        }
        // O que sobrou, se o fluido guardado é o mesmo, tenta o tanque de cima, como externo para não passar pelo limite interno.
        if inserted < amount && self.inner.resource().as_ref() == Some(resource) {
            let above = self.above.borrow().clone();
            if let Some(above) = above.and_then(|find| find()) {
                inserted += above.insert(
                    resource,
                    amount - inserted,
                    transaction,
                    AutomationType::External,
                );
            }
        }
        inserted
    }

    fn extract(
        &self,
        resource: &FluidResource,
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

/// A capability de fluido do bloco numa face (`ValveFluidTankWrapper` no topo, o tanque nas outras), como um handler de um contêiner
/// só: o que `BlockFluidTank.useItemOn` entrega a `FluidUtil.interactWithFluidHandler`.
pub struct FaceHandler<'a> {
    pub tile: &'a FluidTankTile,
    /// A face é a de cima.
    pub top: bool,
}

impl crate::item_access::ItemResourceHandler<FluidResource> for FaceHandler<'_> {
    fn size(&self) -> usize {
        1
    }

    fn resource(&self, _index: usize) -> Option<FluidResource> {
        self.tile.tank.resource()
    }

    fn amount_as_int(&self, _index: usize) -> i32 {
        self.tile.tank.amount_as_int()
    }

    fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        self.tile
            .handler_insert(self.top, resource, amount, transaction)
    }

    fn extract(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32 {
        self.tile.handler_extract(resource, amount, transaction)
    }

    fn extract_at(
        &self,
        _index: usize,
        resource: &FluidResource,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        self.tile.handler_extract(resource, amount, transaction)
    }

    fn is_full(&self) -> bool {
        self.tile.tank.amount() >= self.tile.tank.capacity()
    }
}

/// `ValveJournal`.
struct ValveState {
    fluid: Option<FluidResource>,
    valve: i32,
    needs_packet: Rc<Cell<bool>>,
}

impl Journaled for ValveState {
    type Snapshot = (Option<FluidResource>, i32);

    fn revert_to_snapshot(&mut self, snapshot: Self::Snapshot) {
        (self.fluid, self.valve) = snapshot;
    }

    fn on_root_commit(&mut self, original: Self::Snapshot) -> Vec<Callback> {
        // A válvula estava em zero e agora tem conteúdo, ou o fluido dela mudou: o tile pede um pacote.
        if original.1 == 0 || original.0 != self.fluid {
            self.needs_packet.set(true);
        }
        Vec::new()
    }
}

/// O handler de fluido do bloco de baixo, pela face de cima dele (`BelowContainerCache.getHandler`).
pub trait BelowHandler {
    /// `ResourceHandler.insert(resource, amount, transaction)`.
    fn insert(&self, resource: &FluidResource, amount: i32, transaction: &Transaction) -> i32;
    /// `BelowContainerCache.getContainer(FluidTankFluidTank.class)`: o que falta no contêiner de baixo, se o handler dele é o de
    /// um contêiner só e esse contêiner é um `FluidTankFluidTank`. A face de cima de um Fluid Tank entrega o
    /// `ValveFluidTankWrapper`, que não é um `FluidTankFluidTank`: para ele a resposta é `None`.
    fn fluid_tank_needed(&self) -> Option<i32>;
}

/// O que o tile precisa para nascer.
pub struct FluidTankConfig {
    /// `tier.getCapacity()` e `tier.getTransferRate()`.
    pub capacity: u64,
    pub rate: i32,
    pub creative: bool,
    /// `MekanismConfig.general.blockDeactivationDelay`: o atraso da luz quando o tanque esvazia.
    pub deactivation_delay: i32,
    /// `Capabilities.FLUID.item()`.
    pub items: ItemHandlerLookup<FluidResource>,
}

/// O que o item do tanque leva do tile (`mekanism:fluids`, `mekanism:items` e `mekanism:edit_mode`): o estado ativo, a válvula, a
/// escala e a última direção de troca do slot não vão.
#[derive(Clone, Debug, PartialEq)]
pub struct FluidTankItem {
    pub fluid: StackContents<FluidResource>,
    /// O slot de entrada e o de saída.
    pub slots: [SlotContents; 2],
    pub edit_mode: ContainerEditMode,
}

/// O que o tick avisa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickResult {
    /// `sendUpdatePacket`.
    pub send_update: bool,
    pub comparator_changed: bool,
    /// `checkLight` chegou a zero: quem usa compara a luz do bloco com a última e refaz a do lugar se mudou.
    pub check_light: bool,
}

/// `TileEntityFluidTank`.
pub struct FluidTankTile {
    tile: Rc<RefCell<TileBase>>,
    tank: FluidTankTank,
    input: HandlerSlot,
    output: InventorySlot,
    inventory_slots: Vec<InventorySlot>,
    items: ItemHandlerLookup<FluidResource>,
    edit_mode: ContainerEditMode,
    valve: Rc<JournalCell<ValveState>>,
    needs_packet: Rc<Cell<bool>>,
    prev_scale: f32,
    light_update_delay: i32,
    deactivation_delay: i32,
    rate: i32,
    game_time: Rc<Cell<u64>>,
}

impl FluidTankTile {
    pub fn new(config: FluidTankConfig) -> Self {
        // `delaySupplier = NO_DELAY`.
        let tile = Rc::new(RefCell::new(TileBase::new(0)));
        let game_time = Rc::new(Cell::new(0));
        // `makesComparatorDirty(FLUID)`: só o tanque suja o comparador; os slots só marcam para salvar.
        let tile_changed: Callback = {
            let tile = tile.clone();
            Rc::new(move || tile.borrow_mut().set_changed())
        };
        let tank = FluidTankTank::new(
            config.capacity,
            config.rate,
            config.creative,
            game_time.clone(),
            Some(tile_changed),
        );
        let items = config.items;
        // `FluidInventorySlot.input(fluidTank, ..)`: por fora só entra o que o tanque pode usar, e só sai o que ele não pode mais.
        let input = HandlerSlot::new(None, |listener| {
            let test: AutomationItemTest = {
                let tank = tank.clone();
                let items = items.clone();
                Rc::new(
                    move |item: &ItemResource, automation: AutomationType| match automation {
                        AutomationType::Internal => true,
                        _ => can_input(&tank, item, &items),
                    },
                )
            };
            let leaves: AutomationItemTest = {
                let tank = tank.clone();
                let items = items.clone();
                Rc::new(move |item: &ItemResource, automation: AutomationType| {
                    automation != AutomationType::External || !can_input(&tank, item, &items)
                })
            };
            let checks = SlotChecks {
                extract_by: Some(leaves),
                insert_by: Some(test),
                ..SlotChecks::default()
            };
            InventorySlot::with_checks(Access::Any, Access::Any, checks, listener, None)
        });
        let output = InventorySlot::output(None, None);
        let inventory_slots = vec![input.slot().clone(), output.clone()];
        let needs_packet = Rc::new(Cell::new(false));
        let valve = JournalCell::new(ValveState {
            fluid: None,
            valve: 0,
            needs_packet: needs_packet.clone(),
        });
        Self {
            tile,
            tank,
            input,
            output,
            inventory_slots,
            items,
            edit_mode: ContainerEditMode::Both,
            valve,
            needs_packet,
            prev_scale: 0.0,
            light_update_delay: 0,
            deactivation_delay: config.deactivation_delay,
            rate: config.rate,
            game_time,
        }
    }

    pub fn tile(&self) -> Ref<'_, TileBase> {
        self.tile.borrow()
    }

    pub fn tile_mut(&self) -> RefMut<'_, TileBase> {
        self.tile.borrow_mut()
    }

    pub fn tank(&self) -> &FluidTankTank {
        &self.tank
    }

    pub fn input_slot(&self) -> &HandlerSlot {
        &self.input
    }

    pub fn output_slot(&self) -> &InventorySlot {
        &self.output
    }

    pub fn inventory_slots(&self) -> &[InventorySlot] {
        &self.inventory_slots
    }

    pub fn items(&self) -> &ItemHandlerLookup<FluidResource> {
        &self.items
    }

    /// `prevScale`.
    pub fn scale(&self) -> f32 {
        self.prev_scale
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.prev_scale = scale;
    }

    /// `lightUpdateDelay`, o `delay` do NBT.
    pub fn light_update_delay(&self) -> i32 {
        self.light_update_delay
    }

    pub fn set_light_update_delay(&mut self, delay: i32) {
        self.light_update_delay = delay;
    }

    /// `getValveFluid`.
    pub fn valve_fluid(&self) -> Option<FluidResource> {
        self.valve.borrow().fluid.clone()
    }

    /// `getContainerEditMode`.
    pub fn edit_mode(&self) -> ContainerEditMode {
        self.edit_mode
    }

    /// O modo lido do NBT ou do componente do item, sem o que `editModeChanged` faz.
    pub fn restore_edit_mode(&mut self, mode: ContainerEditMode) {
        self.edit_mode = mode;
    }

    /// `nextMode`.
    pub fn next_mode(&mut self) {
        self.edit_mode = self.edit_mode.next();
        self.edit_mode_changed();
    }

    /// `previousMode`.
    pub fn previous_mode(&mut self) {
        self.edit_mode = self.edit_mode.previous();
        self.edit_mode_changed();
    }

    /// `editModeChanged`: a direção volta a `UNKNOWN`, para o modo `BOTH` decidir de novo.
    fn edit_mode_changed(&mut self) {
        self.input.reset_last_transfer_direction(None);
    }

    /// `collectImplicitComponents`: o que vai para o item.
    pub fn collect_item(&self) -> FluidTankItem {
        FluidTankItem {
            fluid: self.tank.inner().contents(),
            slots: [self.input.slot().contents(), self.output.contents()],
            edit_mode: self.edit_mode,
        }
    }

    /// `applyImplicitComponents`: o tile recém-posto recebe o que o item traz, fora de transação (cada contêiner avisa na hora).
    pub fn apply_item(&mut self, item: &FluidTankItem) {
        self.tank
            .inner()
            .set_contents(item.fluid.resource.clone(), item.fluid.amount, None);
        for (slot, contents) in self.inventory_slots.iter().zip(&item.slots) {
            slot.set_contents(contents.resource.clone(), contents.amount, None);
        }
        self.edit_mode = item.edit_mode;
    }

    /// `onSneakRightClick` do Configurator: liga e desliga o empurrão para baixo. Devolve o estado novo.
    pub fn toggle_active(&mut self) -> bool {
        let active = !self.tile.borrow().active();
        self.tile.borrow_mut().set_active(active);
        active
    }

    /// `getRedstoneLevel`.
    pub fn redstone_level(&self) -> i32 {
        redstone_signal_of_container(self.tank.amount(), self.tank.capacity())
    }

    /// O `insert` do handler de fluido do bloco numa face: o topo passa pelo `ValveFluidTankWrapper`.
    pub fn handler_insert(
        &self,
        top: bool,
        resource: &FluidResource,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        let inserted = self
            .tank
            .insert(resource, amount, transaction, AutomationType::External);
        if top && inserted > 0 {
            // `ValveJournal.onTransfer`.
            let snapshot = {
                let state = self.valve.borrow();
                (state.fluid.clone(), state.valve)
            };
            self.valve.update_snapshots(transaction, || snapshot);
            let mut state = self.valve.borrow_mut();
            state.valve = VALVE_TICKS;
            state.fluid = Some(resource.clone());
        }
        inserted
    }

    /// O `extract` do handler de fluido do bloco, igual em toda face.
    pub fn handler_extract(
        &self,
        resource: &FluidResource,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        self.tank
            .extract(resource, amount, transaction, AutomationType::External)
    }

    /// `ValveJournal.tick`.
    fn valve_tick(&self) -> bool {
        let mut state = self.valve.borrow_mut();
        if state.valve > 0 {
            state.valve -= 1;
            if state.valve == 0 {
                state.fluid = None;
                return true;
            }
        }
        false
    }

    /// `checkLight`.
    fn check_light(&mut self) -> bool {
        if self.light_update_delay > 0 {
            self.light_update_delay -= 1;
            return self.light_update_delay == 0;
        }
        false
    }

    /// `onUpdateServer`, e o trecho de `tickServer` depois dele. `below` é o handler de fluido do bloco de baixo, se há.
    pub fn tick_server(&mut self, game_time: u64, below: Option<&dyn BelowHandler>) -> TickResult {
        self.game_time.set(game_time);
        self.tile.borrow_mut().tick_delay();
        let mut send_update = self.valve_tick();
        let check_light = self.check_light();
        let stored = self.tank.amount();
        let capacity = self.tank.capacity();
        let target = if capacity == 0 {
            0.0
        } else {
            (stored as f64 / capacity as f64) as f32
        };
        let scale = scale_towards(
            self.prev_scale,
            target,
            self.tank.is_empty(),
            stored == capacity,
        );
        if scale_changed(scale, self.prev_scale) {
            // Ficou vazio ou deixou de estar: a luz do bloco pode ter mudado com o fluido.
            if (self.prev_scale == 0.0 || scale == 0.0) && self.light_update_delay == 0 {
                self.light_update_delay = if self.prev_scale == 0.0 {
                    1
                } else {
                    self.deactivation_delay
                };
            }
            self.prev_scale = scale;
            send_update = true;
        }
        self.input
            .handle_container(&self.tank, &self.output, self.edit_mode, &self.items, None);
        if self.tile.borrow().active()
            && let Some(below) = below
        {
            let mut to_emit = self.rate;
            if let Some(needed) = below.fluid_tank_needed() {
                // Só o que o tanque de baixo ainda aceita, para o resto não subir de volta pela coluna.
                to_emit = to_emit.min(needed);
            }
            emit(below, &self.tank, to_emit);
        }
        if self.needs_packet.replace(false) {
            send_update = true;
        }
        let level = self.redstone_level();
        let comparator_changed = self.tile.borrow_mut().finish_tick(|| level);
        TickResult {
            send_update,
            comparator_changed,
            check_light,
        }
    }
}

/// `ResourceUtils.emit(target, container, maxOutput, null)`.
pub fn emit(
    target: &dyn BelowHandler,
    container: &dyn ResourceContainer<FluidResource>,
    max_output: i32,
) -> i32 {
    let Some(resource) = container.resource() else {
        return 0;
    };
    if max_output <= 0 {
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
    let sent = target.insert(&resource, amount, &sub);
    if sent > 0 && container.extract(&resource, sent, &sub, AutomationType::Internal) == sent {
        sub.commit();
        return sent;
    }
    0
}
