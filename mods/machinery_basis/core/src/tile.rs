//! Porte de `TileEntityMekanism`, `TileEntityRecipeMachine`, `TileEntityProgressMachine` e `TileEntityElectricMachine`.
use crate::cached_recipe::{
    CachedRecipeHolder, ItemToItemRecipe, OneInputHolder, OneInputItemRecipe,
};
use crate::containers::{
    AutomationType, ContentsListener, InventorySlot, MachineEnergyContainer, redstone_signal,
};
use crate::operation_tracker::{RecipeError, RecipeErrors};
use crate::recipe_monitor::{RecipeCacheLookupMonitor, RecipeLookupHandler};
use crate::transaction::Transaction;

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
    pub recipes: Vec<ItemToItemRecipe>,
    /// `TRACKED_ERROR_TYPES`, na ordem da tela.
    pub tracked_errors: Vec<RecipeError>,
    /// O sorteio de `shouldRecheckAllErrors`, de 0 a [`RECIPE_CHECK_FREQUENCY`].
    pub check_offset: u64,
}

/// Tudo da máquina menos o monitor, que roda a receita em cima disto.
#[derive(Clone, Debug)]
pub struct ElectricMachineState {
    pub tile: TileBase,
    pub energy: MachineEnergyContainer,
    pub input: InventorySlot,
    pub output: InventorySlot,
    pub energy_slot: InventorySlot,
    config: ElectricMachineConfig,
    ticks_required: i32,
    operating_ticks: i32,
    tracked_errors: Vec<bool>,
    speed_upgrades: i32,
    energy_upgrades: i32,
    players_using: u32,
    game_time: u64,
    finished_operations: Option<i32>,
}

impl ElectricMachineState {
    pub fn ticks_required(&self) -> i32 {
        self.ticks_required
    }

    pub fn recipe_progress(&self) -> i32 {
        self.operating_ticks
    }

    /// `getWarningCheck`.
    pub fn has_error(&self, error: RecipeError) -> bool {
        let position = self
            .config
            .tracked_errors
            .iter()
            .position(|tracked| *tracked == error);
        position.is_some_and(|index| self.tracked_errors[index])
    }

    pub fn open(&mut self) {
        self.players_using += 1;
    }

    pub fn close(&mut self) {
        self.players_using = self.players_using.saturating_sub(1);
    }

    /// `getRedstoneLevel`, com os slots de `getInventorySlots`.
    pub fn redstone_level(&self) -> i32 {
        redstone_signal(&[&self.input, &self.output, &self.energy_slot])
    }

    /// `recalculateUpgrades` de velocidade e energia, na ordem do Java: tempo, consumo, capacidade.
    fn recalculate_upgrades(&mut self) {
        let rules = self.config.upgrades;
        self.ticks_required = rules.ticks(self.config.base_ticks_required, self.speed_upgrades);
        let base = self.energy.base_energy_per_tick();
        self.energy.set_energy_per_tick(rules.energy_per_tick(
            base,
            self.speed_upgrades,
            self.energy_upgrades,
        ));
        let buffer = STORAGE_MULTIPLIER.max(i64::from(self.ticks_required));
        self.energy
            .set_max_energy(i64::from(self.energy.energy_per_tick()) * buffer);
    }
}

impl CachedRecipeHolder for ElectricMachineState {
    fn can_function(&self) -> bool {
        self.tile.is_redstone_activated()
    }

    fn set_active(&mut self, active: bool) {
        self.tile.set_active(active);
    }

    fn required_ticks(&self) -> i32 {
        self.ticks_required
    }

    fn on_finish(&mut self, operations: i32) {
        self.finished_operations = Some(operations);
    }

    fn per_tick_energy(&self) -> i32 {
        self.energy.energy_per_tick()
    }

    fn stored_energy(&self) -> i32 {
        self.energy.amount_as_int()
    }

    fn use_energy(&mut self, energy: i32, transaction: Transaction) -> i32 {
        self.energy
            .extract(energy, transaction, AutomationType::Internal)
    }

    fn on_errors_change(&mut self, errors: RecipeErrors) {
        for (index, error) in self.config.tracked_errors.iter().enumerate() {
            self.tracked_errors[index] = errors.contains(*error);
        }
    }

    fn operating_ticks_changed(&mut self, operating_ticks: i32) {
        self.operating_ticks = operating_ticks;
    }

    fn recheck_all_errors(&self) -> bool {
        self.players_using > 0
            && self.game_time % RECIPE_CHECK_FREQUENCY == self.config.check_offset
    }

    fn close_transaction(&mut self, transaction: Transaction, commit: bool) {
        self.energy.close(transaction, commit);
        self.input.close(transaction, commit);
        self.output.close(transaction, commit);
        self.energy_slot.close(transaction, commit);
    }
}

impl OneInputHolder for ElectricMachineState {
    fn input_slot(&self) -> &InventorySlot {
        &self.input
    }

    fn input_slot_mut(&mut self) -> &mut InventorySlot {
        &mut self.input
    }

    fn output_slot_mut(&mut self) -> &mut InventorySlot {
        &mut self.output
    }
}

impl RecipeLookupHandler for ElectricMachineState {
    type Recipe = ItemToItemRecipe;
    type Logic = OneInputItemRecipe;

    fn find_recipe(&self) -> Option<(String, ItemToItemRecipe)> {
        let resource = self.input.resource()?;
        // `findFirstRecipe` olha só o tipo do item: a quantidade quem confere é o `process`.
        let recipe = self
            .config
            .recipes
            .iter()
            .find(|recipe| recipe.input == *resource)?;
        Some((recipe.id.clone(), recipe.clone()))
    }

    fn create_new_cached_recipe(&mut self, recipe: ItemToItemRecipe) -> OneInputItemRecipe {
        OneInputItemRecipe { recipe }
    }

    fn saved_operating_ticks(&self) -> i32 {
        self.operating_ticks
    }

    fn on_cached_recipe_changed(&mut self) {
        self.tracked_errors.fill(false);
    }
}

/// `TileEntityElectricMachine`.
#[derive(Clone, Debug)]
pub struct ElectricMachine {
    pub state: ElectricMachineState,
    monitor: RecipeCacheLookupMonitor<OneInputItemRecipe>,
}

impl ElectricMachine {
    pub fn new(config: ElectricMachineConfig) -> Self {
        let tracked_errors = vec![false; config.tracked_errors.len()];
        let state = ElectricMachineState {
            tile: TileBase::new(config.deactivation_delay),
            energy: MachineEnergyContainer::new(
                STORAGE_MULTIPLIER * i64::from(config.energy_per_tick),
                config.energy_per_tick,
                ContentsListener::RecipeCacheUnpauseSaveOnly,
            ),
            input: InventorySlot::new(ContentsListener::RecipeCache),
            output: InventorySlot::new(ContentsListener::RecipeCacheUnpause),
            energy_slot: InventorySlot::new(ContentsListener::Tile),
            ticks_required: config.base_ticks_required,
            operating_ticks: 0,
            tracked_errors,
            speed_upgrades: 0,
            energy_upgrades: 0,
            players_using: 0,
            game_time: 0,
            finished_operations: None,
            config,
        };
        Self {
            state,
            monitor: RecipeCacheLookupMonitor::new(true),
        }
    }

    pub fn monitor(&self) -> &RecipeCacheLookupMonitor<OneInputItemRecipe> {
        &self.monitor
    }

    pub fn set_upgrades(&mut self, speed: i32, energy: i32) {
        self.state.speed_upgrades = speed;
        self.state.energy_upgrades = energy;
        self.state.recalculate_upgrades();
    }

    /// Entrega os avisos dos contêineres, que no Java disparam na hora.
    pub fn flush_listeners(&mut self) {
        let state = &mut self.state;
        let changes = [
            state.energy.take_change(),
            state.input.take_change(),
            state.output.take_change(),
            state.energy_slot.take_change(),
        ];
        for listener in changes.into_iter().flatten() {
            match listener {
                ContentsListener::Tile => state.tile.set_changed(),
                ContentsListener::RecipeCache => {
                    state.tile.set_changed();
                    self.monitor.on_change();
                }
                ContentsListener::RecipeCacheUnpause => {
                    state.tile.set_changed();
                    self.monitor.unpause();
                }
                ContentsListener::RecipeCacheUnpauseSaveOnly => self.monitor.unpause(),
            }
        }
        if state.output.take_decrease() {
            self.monitor.output_decreased();
        }
    }

    /// `tickServer`. Devolve se o comparador precisa avisar os vizinhos.
    pub fn tick_server(&mut self, game_time: u64) -> bool {
        self.state.game_time = game_time;
        self.flush_listeners();
        self.state.tile.tick_delay();
        self.monitor.update_and_process(&mut self.state);
        self.flush_listeners();
        if let Some(operations) = self.state.finished_operations.take() {
            self.monitor.set_recipe_used(operations);
        }
        let level = self.state.redstone_level();
        self.state.tile.finish_tick(|| level)
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
