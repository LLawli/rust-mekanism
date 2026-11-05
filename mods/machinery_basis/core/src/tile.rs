//! Porte de `TileEntityMekanism`, `TileEntityRecipeMachine`, `TileEntityProgressMachine` e `TileEntityElectricMachine`,
//! como objetos vivos: o tile, os contêineres e a receita em cache são compartilhados, e cada um avisa o outro por closure,
//! como no Java.
use crate::cached_recipe::{CachedRecipe, ItemToItemRecipe, OneInputItemRecipe};
use crate::containers::{EnergyContainer, InventorySlot, redstone_signal};
use crate::ejector::{EjectGroup, ItemEjector};
use crate::item_handler::ItemCapabilities;
use crate::operation_tracker::{RecipeError, RecipeErrors};
use crate::recipe_monitor::{MonitorFlags, RecipeCacheLookupMonitor, RecipeLookupHandler};
use crate::transaction::Callback;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;

/// `TileEntityRecipeMachine.RECIPE_CHECK_FREQUENCY`.
pub const RECIPE_CHECK_FREQUENCY: u64 = 100;
/// `AttributeEnergy.STORAGE_MULTIPLIER`.
pub const STORAGE_MULTIPLIER: i64 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedstoneControl {
    #[default]
    Disabled,
    High,
    Low,
    Pulse,
}

impl RedstoneControl {
    /// `getNext` de quem não aceita PULSE, o padrão de `ITileRedstone`.
    fn supported(self) -> Self {
        match self {
            Self::Pulse => Self::Disabled,
            other => other,
        }
    }
}

/// O que todo tile faz a cada tick, além da máquina em si.
#[derive(Clone, Debug)]
pub struct TileBase {
    pub ticker: u32,
    redstone: bool,
    redstone_last_tick: bool,
    control_type: RedstoneControl,
    current_redstone_level: i32,
    update_comparators: bool,
    current_active: bool,
    update_delay: i32,
    deactivation_delay: i32,
    block_active: bool,
}

impl TileBase {
    /// `deactivation_delay` é o `blockDeactivationDelay` da configuração.
    pub fn new(deactivation_delay: i32) -> Self {
        Self {
            ticker: 0,
            redstone: false,
            redstone_last_tick: false,
            control_type: RedstoneControl::Disabled,
            current_redstone_level: 0,
            update_comparators: false,
            current_active: false,
            update_delay: 0,
            deactivation_delay,
            block_active: false,
        }
    }

    pub fn active(&self) -> bool {
        self.current_active
    }

    /// O estado do bloco, que atrasa em relação a [`TileBase::active`] ao desligar.
    pub fn block_active(&self) -> bool {
        self.block_active
    }

    pub fn set_active(&mut self, active: bool) {
        if active == self.current_active {
            return;
        }
        self.current_active = active;
        if self.block_active != active {
            if active {
                self.block_active = true;
            } else {
                if self.update_delay == 0 {
                    self.block_active = false;
                }
                self.update_delay = self.deactivation_delay;
            }
        }
    }

    /// O trecho de `tickServer` antes de `onUpdateServer`.
    pub fn tick_delay(&mut self) {
        if self.update_delay > 0 {
            self.update_delay -= 1;
            if self.update_delay == 0 && self.block_active != self.current_active {
                self.block_active = self.current_active;
            }
        }
    }

    pub fn control_type(&self) -> RedstoneControl {
        self.control_type
    }

    pub fn set_control_type(&mut self, control_type: RedstoneControl) {
        self.control_type = control_type.supported();
    }

    pub fn is_powered(&self) -> bool {
        self.redstone
    }

    /// `updatePower`, com o sinal que os vizinhos dão.
    pub fn update_power(&mut self, power: bool) {
        self.redstone = power;
    }

    pub fn is_redstone_activated(&self) -> bool {
        match self.control_type {
            RedstoneControl::Disabled => true,
            RedstoneControl::High => self.redstone,
            RedstoneControl::Low => !self.redstone,
            RedstoneControl::Pulse => self.redstone && !self.redstone_last_tick,
        }
    }

    pub fn current_redstone_level(&self) -> i32 {
        self.current_redstone_level
    }

    /// `setChanged`: o comparador só recalcula no fim do tick.
    pub fn set_changed(&mut self) {
        self.update_comparators = true;
    }

    /// O trecho de `tickServer` depois de `onUpdateServer`. Devolve se o comparador precisa avisar os vizinhos.
    pub fn finish_tick(&mut self, redstone_level: impl FnOnce() -> i32) -> bool {
        let mut notify = false;
        if self.update_comparators {
            let new_level = redstone_level();
            if new_level != self.current_redstone_level {
                self.current_redstone_level = new_level;
                notify = true;
            }
            self.update_comparators = false;
        }
        self.ticker += 1;
        self.redstone_last_tick = self.redstone;
        notify
    }
}

/// Parâmetros de `UpgradeUtils`: vêm da configuração e do registro de upgrades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpgradeRules {
    pub max_upgrade_multiplier: f64,
    pub max_speed: i32,
    pub max_energy: i32,
}

impl UpgradeRules {
    fn ticks_d(&self, base_ticks: i32, speed: i32) -> f64 {
        let fraction = f64::from(speed) / f64::from(self.max_speed);
        f64::from(base_ticks) * self.max_upgrade_multiplier.powf(-fraction)
    }

    /// `getTicks`.
    pub fn ticks(&self, base_ticks: i32, speed: i32) -> i32 {
        (self.ticks_d(base_ticks, speed) as i32).max(1)
    }

    /// `getOperationsPerTick`.
    pub fn operations_per_tick(&self, base_ticks: i32, default_operations: i32, speed: i32) -> i32 {
        let ticks = self.ticks_d(base_ticks, speed);
        if ticks >= 1.0 {
            return default_operations;
        }
        ((1.0 / ticks).max(1.0) * f64::from(default_operations)) as i32
    }

    /// `getEnergyPerTick`.
    pub fn energy_per_tick(&self, base: i32, speed: i32, energy: i32) -> i32 {
        let exponent = 2.0 * f64::from(speed) / f64::from(self.max_speed)
            - f64::from(energy) / f64::from(self.max_energy);
        (f64::from(base) * self.max_upgrade_multiplier.powf(exponent)).ceil() as i32
    }
}

/// O que define uma máquina elétrica de uma entrada. Os valores vêm do mod.
#[derive(Clone, Debug)]
pub struct ElectricMachineConfig {
    pub energy_per_tick: i32,
    pub base_ticks_required: i32,
    pub deactivation_delay: i32,
    pub upgrades: UpgradeRules,
    pub recipes: Vec<Rc<ItemToItemRecipe>>,
    /// `TRACKED_ERROR_TYPES`, na ordem da tela.
    pub tracked_errors: Vec<RecipeError>,
    /// O sorteio de `shouldRecheckAllErrors`, de 0 a [`RECIPE_CHECK_FREQUENCY`].
    pub check_offset: u64,
}

/// O que o tile guarda e as closures das receitas e dos contêineres alcançam.
struct Shared {
    tile: RefCell<TileBase>,
    upgrades: UpgradeRules,
    base_ticks_required: i32,
    tracked_error_kinds: Vec<RecipeError>,
    check_offset: u64,
    recipes: RefCell<Rc<Vec<Rc<ItemToItemRecipe>>>>,
    /// `invalidateCache`: lido uma vez por quem o recarregou.
    cache_invalid: Cell<bool>,
    ticks_required: Cell<i32>,
    operating_ticks: Cell<i32>,
    tracked_errors: RefCell<Vec<bool>>,
    speed_upgrades: Cell<i32>,
    energy_upgrades: Cell<i32>,
    players_using: Cell<u32>,
    game_time: Cell<u64>,
}

impl Shared {
    /// `shouldRecheckAllErrors`.
    fn recheck_all_errors(&self) -> bool {
        self.players_using.get() > 0
            && self.game_time.get() % RECIPE_CHECK_FREQUENCY == self.check_offset
    }

    /// `onErrorsChanged`.
    fn on_errors_changed(&self, errors: RecipeErrors) {
        let mut tracked = self.tracked_errors.borrow_mut();
        for (index, error) in self.tracked_error_kinds.iter().enumerate() {
            tracked[index] = errors.contains(*error);
        }
    }
}

/// O `IRecipeLookupHandler` do tile: acha a receita e monta a `CachedRecipe` com os callbacks dele.
struct ChamberLookup {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
}

impl RecipeLookupHandler for ChamberLookup {
    fn get_recipe(&self) -> Option<Rc<ItemToItemRecipe>> {
        let resource = self.input.resource()?;
        // `findFirstRecipe` olha só o tipo do item: a quantidade quem confere é o `process`.
        let recipes = self.shared.recipes.borrow();
        recipes
            .iter()
            .find(|recipe| recipe.input.matches(&resource))
            .cloned()
    }

    fn create_new_cached_recipe(&self, recipe: Rc<ItemToItemRecipe>) -> CachedRecipe {
        let shared = &self.shared;
        let logic = OneInputItemRecipe::new(recipe, self.input.clone(), self.output.clone());
        let recheck = {
            let shared = shared.clone();
            move || shared.recheck_all_errors()
        };
        let errors_changed = {
            let shared = shared.clone();
            move |errors| shared.on_errors_changed(errors)
        };
        let can_function = {
            let shared = shared.clone();
            move || shared.tile.borrow().is_redstone_activated()
        };
        let set_active = {
            let shared = shared.clone();
            move |active| shared.tile.borrow_mut().set_active(active)
        };
        let per_tick = {
            let energy = self.energy.clone();
            move || energy.energy_per_tick()
        };
        let required_ticks = {
            let shared = shared.clone();
            move || shared.ticks_required.get()
        };
        let operating_ticks = {
            let shared = shared.clone();
            move |ticks| shared.operating_ticks.set(ticks)
        };
        CachedRecipe::new(Box::new(logic), Box::new(recheck))
            .set_errors_changed(errors_changed)
            .set_can_holder_function(can_function)
            .set_active(set_active)
            .set_energy_requirements(per_tick, self.energy.clone())
            .set_required_ticks(required_ticks)
            .set_operating_ticks_changed(operating_ticks)
    }

    fn saved_operating_ticks(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    fn invalidate_cache(&self) -> bool {
        self.shared.cache_invalid.take()
    }

    fn on_cached_recipe_changed(&self) {
        self.shared.tracked_errors.borrow_mut().fill(false);
    }
}

/// `TileEntityElectricMachine`.
pub struct ElectricMachine {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
    energy_slot: InventorySlot,
    monitor: RecipeCacheLookupMonitor,
    ejector: ItemEjector,
}

impl ElectricMachine {
    pub fn new(config: ElectricMachineConfig) -> Self {
        let shared = Rc::new(Shared {
            tile: RefCell::new(TileBase::new(config.deactivation_delay)),
            upgrades: config.upgrades,
            base_ticks_required: config.base_ticks_required,
            check_offset: config.check_offset,
            recipes: RefCell::new(Rc::new(config.recipes)),
            cache_invalid: Cell::new(false),
            ticks_required: Cell::new(config.base_ticks_required),
            operating_ticks: Cell::new(0),
            tracked_errors: RefCell::new(vec![false; config.tracked_errors.len()]),
            tracked_error_kinds: config.tracked_errors,
            speed_upgrades: Cell::new(0),
            energy_upgrades: Cell::new(0),
            players_using: Cell::new(0),
            game_time: Cell::new(0),
        });
        let flags = MonitorFlags::new();
        // Os listeners do `TileEntityRecipeMachine`: o do monitor, o de salvar, e os que despausam.
        let tile_changed = {
            let shared = shared.clone();
            move || shared.tile.borrow_mut().set_changed()
        };
        let input_listener: Callback = {
            let (flags, tile_changed) = (flags.clone(), tile_changed.clone());
            Rc::new(move || {
                tile_changed();
                flags.on_change();
            })
        };
        let output_listener: Callback = {
            let (flags, tile_changed) = (flags.clone(), tile_changed.clone());
            Rc::new(move || {
                tile_changed();
                flags.unpause();
            })
        };
        let decreased: Callback = {
            let flags = flags.clone();
            Rc::new(move || flags.output_decreased())
        };
        let unpause_save_only: Callback = {
            let flags = flags.clone();
            Rc::new(move || flags.unpause())
        };
        let energy_slot_listener: Callback = Rc::new(tile_changed);
        let energy = EnergyContainer::input(
            STORAGE_MULTIPLIER * i64::from(config.energy_per_tick),
            config.energy_per_tick,
            Some(unpause_save_only),
        );
        let input = InventorySlot::new(Some(input_listener));
        let output = InventorySlot::output(Some(output_listener), Some(decreased));
        let energy_slot = InventorySlot::new(Some(energy_slot_listener));
        let lookup = ChamberLookup {
            shared: shared.clone(),
            energy: energy.clone(),
            input: input.clone(),
            output: output.clone(),
        };
        let monitor = RecipeCacheLookupMonitor::new(Rc::new(lookup), flags, true);
        // O ejetor de itens sai pelo slot de saída, em nenhuma face até a configuração mandar.
        let ejector = ItemEjector::new(vec![EjectGroup::new(vec![output.clone()], Vec::new())]);
        Self {
            shared,
            energy,
            input,
            output,
            energy_slot,
            monitor,
            ejector,
        }
    }

    pub fn tile(&self) -> Ref<'_, TileBase> {
        self.shared.tile.borrow()
    }

    pub fn tile_mut(&self) -> RefMut<'_, TileBase> {
        self.shared.tile.borrow_mut()
    }

    pub fn energy(&self) -> &EnergyContainer {
        &self.energy
    }

    pub fn input(&self) -> &InventorySlot {
        &self.input
    }

    pub fn output(&self) -> &InventorySlot {
        &self.output
    }

    pub fn energy_slot(&self) -> &InventorySlot {
        &self.energy_slot
    }

    pub fn monitor(&self) -> &RecipeCacheLookupMonitor {
        &self.monitor
    }

    pub fn ejector_mut(&mut self) -> &mut ItemEjector {
        &mut self.ejector
    }

    pub fn ticks_required(&self) -> i32 {
        self.shared.ticks_required.get()
    }

    pub fn recipe_progress(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    /// Carrega o progresso salvo: a receita em cache o lê quando nasce (`getSavedOperatingTicks`).
    pub fn load_operating_ticks(&self, operating_ticks: i32) {
        self.shared.operating_ticks.set(operating_ticks);
    }

    /// `getWarningCheck`.
    pub fn has_error(&self, error: RecipeError) -> bool {
        let position = self
            .shared
            .tracked_error_kinds
            .iter()
            .position(|tracked| *tracked == error);
        position.is_some_and(|index| self.shared.tracked_errors.borrow()[index])
    }

    /// Troca as receitas, quando os dados recarregam: o cache se refaz no próximo tick.
    pub fn set_recipes(&self, recipes: Vec<Rc<ItemToItemRecipe>>) {
        *self.shared.recipes.borrow_mut() = Rc::new(recipes);
        self.shared.cache_invalid.set(true);
    }

    pub fn open(&self) {
        self.shared
            .players_using
            .set(self.shared.players_using.get() + 1);
    }

    pub fn close(&self) {
        let using = self.shared.players_using.get();
        self.shared.players_using.set(using.saturating_sub(1));
    }

    /// `getRedstoneLevel`, com os slots de `getInventorySlots`.
    pub fn redstone_level(&self) -> i32 {
        redstone_signal(&[&self.input, &self.output, &self.energy_slot])
    }

    pub fn set_upgrades(&self, speed: i32, energy: i32) {
        self.shared.speed_upgrades.set(speed);
        self.shared.energy_upgrades.set(energy);
        self.recalculate_upgrades();
    }

    /// `recalculateUpgrades` de velocidade e energia, na ordem do Java: tempo, consumo, capacidade.
    fn recalculate_upgrades(&self) {
        let shared = &self.shared;
        let rules = shared.upgrades;
        let (speed, energy) = (shared.speed_upgrades.get(), shared.energy_upgrades.get());
        let ticks_required = rules.ticks(shared.base_ticks_required, speed);
        shared.ticks_required.set(ticks_required);
        let per_tick = rules.energy_per_tick(self.energy.base_energy_per_tick(), speed, energy);
        self.energy.set_energy_per_tick(per_tick);
        let buffer = STORAGE_MULTIPLIER.max(i64::from(ticks_required));
        self.energy.set_max_energy(i64::from(per_tick) * buffer);
    }

    /// `tickServer`. Devolve se o comparador precisa avisar os vizinhos.
    pub fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) -> bool {
        self.shared.game_time.set(game_time);
        self.shared.tile.borrow_mut().tick_delay();
        // `onUpdateServer`: o ejetor, do `TileEntityConfigurableMachine`, e depois a receita.
        self.ejector.tick_server(neighbours);
        self.monitor.update_and_process();
        let (input, output, energy_slot) = (&self.input, &self.output, &self.energy_slot);
        self.shared
            .tile
            .borrow_mut()
            .finish_tick(|| redstone_signal(&[input, output, energy_slot]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: UpgradeRules = UpgradeRules {
        max_upgrade_multiplier: 10.0,
        max_speed: 8,
        max_energy: 8,
    };

    #[test]
    fn upgrades_match_the_recorded_values() {
        assert_eq!(RULES.ticks(200, 1), 149);
        assert_eq!(RULES.energy_per_tick(5, 1, 0), 9);
        assert_eq!(RULES.ticks(200, 8), 20);
        assert_eq!(RULES.energy_per_tick(5, 8, 0), 500);
        assert_eq!(RULES.energy_per_tick(5, 8, 8), 50);
    }

    #[test]
    fn turning_on_is_instant_and_turning_off_waits_for_the_delay() {
        let mut tile = TileBase::new(3);
        tile.set_active(true);
        assert!(tile.block_active());
        tile.set_active(false);
        assert!(!tile.block_active(), "sem atraso armado, desliga na hora");
        tile.set_active(true);
        tile.set_active(false);
        assert!(tile.block_active(), "dentro do atraso, o bloco espera");
        for _ in 0..3 {
            assert!(tile.block_active());
            tile.tick_delay();
        }
        assert!(!tile.block_active());
    }

    #[test]
    fn a_machine_without_pulse_falls_back_to_disabled() {
        let mut tile = TileBase::new(0);
        tile.set_control_type(RedstoneControl::Pulse);
        assert_eq!(tile.control_type(), RedstoneControl::Disabled);
    }
}
