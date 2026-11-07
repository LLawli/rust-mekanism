//! Porte de `TileEntityMekanism`, `TileEntityRecipeMachine`, `TileEntityProgressMachine`, `TileEntityElectricMachine` e
//! `TileEntityMetallurgicInfuser`, como objetos vivos: o tile, os contêineres e a receita em cache são compartilhados, e cada um avisa o outro por closure,
//! como no Java.
use crate::cached_recipe::{
    CachedRecipe, ItemToItemCache, ItemToItemRecipe, OneInputItemRecipe, RecipeLogic,
};
use crate::chemical::{
    ChemicalInputHandler, ChemicalResource, ChemicalTank, ConversionCache, can_fill_or_convert,
    fill_tank_or_convert,
};
use crate::containers::{
    Access, AutomationType, EnergyContainer, InventorySlot, ItemResource, ItemTest, SlotChecks,
    redstone_signal,
};
use crate::ejector::{EjectGroup, ItemEjector};
use crate::energy_conversion::{EnergyConversionCache, fill_container_or_convert};
use crate::item_chemical_constant_recipe::{
    ChemicalUsageMultiplier, ItemChemicalConstantUsageRecipe,
};
use crate::item_handler::ItemCapabilities;
use crate::operation_tracker::{RecipeError, RecipeErrors};
use crate::recipe_handlers::{ItemInputHandler, ItemOutputHandler};
use crate::recipe_monitor::{
    MonitorFlags, RecipeCacheLookupMonitor, RecipeId, RecipeLookupHandler,
};
use crate::transaction::Callback;
use crate::two_input_recipe::{
    ItemChemicalCache, ItemChemicalToItemRecipe, TwoInputItemChemicalRecipe,
};
use crate::upgrade::{UpgradeComponent, UpgradeData, UpgradeSaved};
use crate::warning::ErrorMask;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::BTreeMap;
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
    /// `getSerializedName`: o nome do enum em minúsculas, que `RedstoneControl.CODEC` grava.
    pub fn serialized_name(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::High => "high",
            Self::Low => "low",
            Self::Pulse => "pulse",
        }
    }

    /// O inverso; um nome desconhecido é `DISABLED`, como o `orElse` do Java.
    pub fn from_serialized_name(name: &str) -> Self {
        match name {
            "high" => Self::High,
            "low" => Self::Low,
            "pulse" => Self::Pulse,
            _ => Self::Disabled,
        }
    }

    /// `getNext` de quem não aceita PULSE, o padrão de `ITileRedstone`.
    fn supported(self) -> Self {
        match self {
            Self::Pulse => Self::Disabled,
            other => other,
        }
    }

    /// `ordinal`, que `SyncableEnum` manda: a ordem das variantes é a do `enum` do Java.
    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// `RedstoneControl.BY_ID`: um número fora da faixa é `DISABLED`, o valor que `SyncableEnum.create` recebe como padrão.
    pub fn from_ordinal(ordinal: i32) -> Self {
        match ordinal {
            1 => Self::High,
            2 => Self::Low,
            3 => Self::Pulse,
            _ => Self::Disabled,
        }
    }
}

/// O que o cliente recebe de redstone de um tile: `controlType`, `redstone` e `redstoneLastTick`, os três `container.track` de
/// `TileEntityMekanism.addContainerTrackers`. O Java manda cada um num sync próprio; aqui os três vão num data slot só.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RedstoneSync {
    pub control_type: RedstoneControl,
    pub powered: bool,
    pub was_powered: bool,
}

/// Os dois bits do modo, e os bits de `redstone` e de `redstoneLastTick`, acima deles.
const CONTROL_BITS: i32 = 0b11;
const POWERED_BIT: i32 = 1 << 2;
const WAS_POWERED_BIT: i32 = 1 << 3;

impl RedstoneSync {
    /// `isRedstoneActivated`, sobre o que o cliente tem (e o servidor, pelos mesmos campos).
    pub fn is_activated(self) -> bool {
        match self.control_type {
            RedstoneControl::Disabled => true,
            RedstoneControl::High => self.powered,
            RedstoneControl::Low => !self.powered,
            RedstoneControl::Pulse => self.powered && !self.was_powered,
        }
    }

    /// O valor do data slot: cabe em 16 bits, sem sinal.
    pub fn to_slot(self) -> i32 {
        self.control_type.ordinal()
            | if self.powered { POWERED_BIT } else { 0 }
            | if self.was_powered { WAS_POWERED_BIT } else { 0 }
    }

    /// O inverso. O cliente pode receber o slot como negativo: só os bits baixos contam.
    pub fn from_slot(slot: i32) -> Self {
        Self {
            control_type: RedstoneControl::from_ordinal(slot & CONTROL_BITS),
            powered: slot & POWERED_BIT != 0,
            was_powered: slot & WAS_POWERED_BIT != 0,
        }
    }
}

/// O que o Java persiste de uma máquina de receita (`saveAdditional` de `TileEntityMekanism`, do monitor e do Infuser), sem o que vive
/// nos contêineres. `pausedForErrors`, os erros e a receita em cache não entram: o Java não os salva.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedState {
    pub redstone: bool,
    pub control_type: RedstoneControl,
    pub active: bool,
    pub update_delay: i32,
    pub current_redstone: i32,
    /// `ProgressRecipeCacheLookupMonitor.operatingTicks`.
    pub progress: i32,
    /// `TileEntityMetallurgicInfuser.usedSoFar`.
    pub used_so_far: i32,
    pub recipes_used: std::collections::BTreeMap<String, i32>,
    pub last_recipe_used: Option<String>,
    pub last_recipe_amount: i32,
    /// `ConfigInfo.isEjecting` dos itens.
    pub ejecting: bool,
    /// `TileComponentUpgrade.serialize`: os upgrades instalados e os dois slots.
    pub upgrades: UpgradeSaved,
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

    pub fn update_delay(&self) -> i32 {
        self.update_delay
    }

    /// `loadAdditional` do `TileEntityMekanism`: `redstone`, `controlType` (já ajustado ao que o bloco aceita),
    /// `currentActive`, `updateDelay` e `currentRedstoneLevel`. `block_active` vem do estado do bloco no mundo.
    pub fn restore(&mut self, saved: &SavedState, block_active: bool) {
        self.redstone = saved.redstone;
        self.set_control_type(saved.control_type);
        self.current_active = saved.active;
        self.update_delay = saved.update_delay;
        self.current_redstone_level = saved.current_redstone;
        self.block_active = block_active;
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
        self.redstone_sync().is_activated()
    }

    /// O que o servidor manda ao cliente de redstone: o modo, `redstone` e `redstoneLastTick`.
    pub fn redstone_sync(&self) -> RedstoneSync {
        RedstoneSync {
            control_type: self.control_type,
            powered: self.redstone,
            was_powered: self.redstone_last_tick,
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

/// O que a máquina precisa dos upgrades: a configuração (`maxUpgradeMultiplier`), quais tipos mexem no tempo e no consumo
/// (`UpgradeIds.SPEED` e `UpgradeIds.ENERGY`, que o mod diz) e os dados dos tipos e do que a máquina aceita.
#[derive(Clone, Debug)]
pub struct UpgradeConfig {
    pub max_upgrade_multiplier: f64,
    pub speed: Rc<str>,
    pub energy: Rc<str>,
    pub data: Rc<UpgradeData>,
}

/// O que define uma máquina de progresso, qualquer que seja a receita. Os valores vêm do mod.
#[derive(Clone, Debug)]
pub struct MachineConfig {
    pub energy_per_tick: i32,
    pub base_ticks_required: i32,
    pub deactivation_delay: i32,
    pub upgrades: UpgradeConfig,
    /// `TRACKED_ERROR_TYPES`, na ordem da tela.
    pub tracked_errors: Vec<RecipeError>,
    /// O sorteio de `shouldRecheckAllErrors`, de 0 a [`RECIPE_CHECK_FREQUENCY`].
    pub check_offset: u64,
}

/// `TileEntityElectricMachine`: um item vira um item. `recipes` é o `getInputCache` do tipo de receita, que todas as máquinas do
/// mesmo tipo compartilham.
#[derive(Clone)]
pub struct ElectricMachineConfig {
    pub machine: MachineConfig,
    pub recipes: Rc<ItemToItemCache>,
    /// O `getInputCache` de `ENERGY_CONVERSION`, que o slot de energia consulta (`canFillOrConvert`) e converte
    /// (`fillContainerOrConvert`).
    pub energy_conversion: Rc<EnergyConversionCache>,
}

/// Uma máquina de progresso com um item, um tanque de químico e o slot que o enche (`TileEntityMetallurgicInfuser`). `recipes` e
/// `conversions` são os `getInputCache` dos tipos `metallurgic_infusing` e `chemical_conversion`.
#[derive(Clone)]
pub struct ChemicalMachineConfig {
    pub machine: MachineConfig,
    pub recipes: Rc<ItemChemicalCache>,
    pub conversions: Rc<ConversionCache>,
    /// `MAX_INFUSE`.
    pub tank_capacity: u64,
    /// Como em [`ElectricMachineConfig::energy_conversion`].
    pub energy_conversion: Rc<EnergyConversionCache>,
}

/// O que o tile guarda e as closures das receitas e dos contêineres alcançam.
struct Shared {
    tile: RefCell<TileBase>,
    upgrade_config: UpgradeConfig,
    base_ticks_required: i32,
    tracked_error_kinds: Vec<RecipeError>,
    check_offset: u64,
    /// `invalidateCache`: lido uma vez por quem o recarregou.
    cache_invalid: Cell<bool>,
    ticks_required: Cell<i32>,
    /// `operationsPerTick`, que sobe com os upgrades de velocidade.
    operations_per_tick: Cell<i32>,
    operating_ticks: Cell<i32>,
    /// `usedSoFar`: só a receita de `per_tick_usage` o muda.
    used_so_far: Cell<i32>,
    tracked_errors: RefCell<Vec<bool>>,
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

    /// `clearRecipeErrors`.
    fn clear_recipe_errors(&self) {
        self.tracked_errors.borrow_mut().fill(false);
    }
}

/// A `CachedRecipe` com o que todo tile de progresso liga nela: erros, `canFunction`, `setActive`, energia, ticks e o
/// progresso, como `createNewCachedRecipe` faz nas duas máquinas.
fn base_cached_recipe(
    shared: &Rc<Shared>,
    energy: &EnergyContainer,
    logic: Box<dyn RecipeLogic>,
) -> CachedRecipe {
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
        let energy = energy.clone();
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
    CachedRecipe::new(logic, Box::new(recheck))
        .set_errors_changed(errors_changed)
        .set_can_holder_function(can_function)
        .set_active(set_active)
        .set_energy_requirements(per_tick, energy.clone())
        .set_required_ticks(required_ticks)
        .set_operating_ticks_changed(operating_ticks)
}

/// O `IRecipeLookupHandler` da Enrichment Chamber e das outras máquinas de um item: acha a receita e monta a `CachedRecipe`
/// com os callbacks do tile.
struct ElectricLookup {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
    recipes: Rc<ItemToItemCache>,
}

impl RecipeLookupHandler<ItemToItemRecipe> for ElectricLookup {
    /// `findFirstRecipe(inputHandler)`: o índice acha pelo item e pela quantidade (`SizedIngredient.test`), nesta ordem: as
    /// receitas que o mapa tem para o item, e depois as complexas.
    fn get_recipe(&self) -> Option<Rc<ItemToItemRecipe>> {
        self.recipes.find_first_recipe(&self.input.contents())
    }

    fn create_new_cached_recipe(&self, recipe: Rc<ItemToItemRecipe>) -> CachedRecipe {
        let logic = OneInputItemRecipe::new(recipe, self.input.clone(), self.output.clone());
        base_cached_recipe(&self.shared, &self.energy, Box::new(logic))
    }

    fn saved_operating_ticks(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    fn invalidate_cache(&self) -> bool {
        self.shared.cache_invalid.take()
    }

    fn on_cached_recipe_changed(&self) {
        self.shared.clear_recipe_errors();
    }
}

/// O `IRecipeLookupHandler` do Metallurgic Infuser (`ItemChemicalRecipeLookupHandler`).
struct ChemicalLookup {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
    tank: ChemicalTank,
    recipes: Rc<ItemChemicalCache>,
}

impl RecipeLookupHandler<ItemChemicalToItemRecipe> for ChemicalLookup {
    /// `findFirstRecipe(itemInputHandler, infusionInputHandler)`.
    fn get_recipe(&self) -> Option<Rc<ItemChemicalToItemRecipe>> {
        self.recipes
            .find_first_recipe(&self.input.contents(), &self.tank.contents())
    }

    fn create_new_cached_recipe(&self, recipe: Rc<ItemChemicalToItemRecipe>) -> CachedRecipe {
        // O item é a entrada principal e usa o erro de entrada secundária, e o químico usa o de entrada.
        let logic: Box<dyn RecipeLogic> = if recipe.per_tick_usage {
            // `ItemStackConstantChemicalToObjectCachedRecipe.create(.., chemicalUsageMultiplier, used -> usedSoFar = used, ..)`,
            // com `ChemicalUsageMultiplier.constantUse(() -> ticksRequired, this::getTicksRequired)`.
            let usage = {
                let (total, required) = (self.shared.clone(), self.shared.clone());
                ChemicalUsageMultiplier::constant_use(
                    move || total.ticks_required.get(),
                    move || required.ticks_required.get(),
                )
            };
            let shared = self.shared.clone();
            Box::new(ItemChemicalConstantUsageRecipe::new(
                recipe,
                ItemInputHandler::new(self.input.clone(), RecipeError::NOT_ENOUGH_SECONDARY_INPUT),
                ChemicalInputHandler::new(self.tank.clone(), RecipeError::NOT_ENOUGH_INPUT),
                usage,
                move |used| shared.used_so_far.set(used),
                ItemOutputHandler::new(self.output.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
            ))
        } else {
            // `new TwoInputCachedRecipe`.
            Box::new(TwoInputItemChemicalRecipe::new(
                recipe,
                self.input.clone(),
                RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
                self.tank.clone(),
                RecipeError::NOT_ENOUGH_INPUT,
                self.output.clone(),
                RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
            ))
        };
        let operations_per_tick = {
            let shared = self.shared.clone();
            move || shared.operations_per_tick.get()
        };
        base_cached_recipe(&self.shared, &self.energy, logic)
            .set_baseline_max_operations(operations_per_tick)
    }

    fn saved_operating_ticks(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    /// `ConstantUsageRecipeLookupHandler.getSavedUsedSoFar`: o `usedSoFar` do tile.
    fn saved_used_so_far(&self) -> Option<i32> {
        Some(self.shared.used_so_far.get())
    }

    fn invalidate_cache(&self) -> bool {
        self.shared.cache_invalid.take()
    }

    fn on_cached_recipe_changed(&self) {
        self.shared.clear_recipe_errors();
    }
}

/// O que o monitor faz, sem o tipo da receita: a máquina guarda um só, seja qual for a receita dela.
trait MonitorOps {
    fn update_and_process(&mut self) -> bool;
    fn used_snapshot(&self) -> (BTreeMap<String, i32>, Option<String>, i32);
    fn restore_used(&self, recipes_used: BTreeMap<String, i32>, last: Option<String>, amount: i32);
    fn used_version(&self) -> u32;
}

impl<R: RecipeId + 'static> MonitorOps for RecipeCacheLookupMonitor<R> {
    fn update_and_process(&mut self) -> bool {
        RecipeCacheLookupMonitor::update_and_process(self)
    }

    fn used_snapshot(&self) -> (BTreeMap<String, i32>, Option<String>, i32) {
        RecipeCacheLookupMonitor::used_snapshot(self)
    }

    fn restore_used(&self, recipes_used: BTreeMap<String, i32>, last: Option<String>, amount: i32) {
        RecipeCacheLookupMonitor::restore_used(self, recipes_used, last, amount);
    }

    fn used_version(&self) -> u32 {
        RecipeCacheLookupMonitor::used_version(self)
    }
}

/// `EnergyInventorySlot.canFillOrConvert`. `fills` diz se o item tem um handler de energia de onde sai algum (`canFill`: nenhum
/// item portado tem) e `converts` se há receita de `ENERGY_CONVERSION` para o tipo dele. A conversão ainda pede que o contêiner
/// aceite inserção manual.
pub fn energy_can_fill_or_convert(
    container: &EnergyContainer,
    fills: &dyn Fn(&ItemResource) -> bool,
    converts: &dyn Fn(&ItemResource) -> bool,
    item: &ItemResource,
) -> bool {
    if fills(item) && container.is_valid_for_extraction(AutomationType::Internal) {
        return true;
    }
    converts(item) && container.is_valid_for_insertion(AutomationType::Manual)
}

/// `EnergyInventorySlot.canFillOrConvert` de uma máquina cujo contêiner é o `MachineEnergyContainer.input` (o das quatro portadas),
/// para quem pergunta sem ter a máquina na mão: o menu e as regras de inserção do jogo. O contêiner é o mesmo que `Parts::new`
/// cria, então a pergunta pela inserção manual dele é a verdadeira.
pub fn machine_energy_slot_accepts(
    conversions: &EnergyConversionCache,
    item: &ItemResource,
) -> bool {
    let container = EnergyContainer::input(0, 0, None);
    energy_can_fill_or_convert(
        &container,
        &|_| false,
        &|item| conversions.find_type_based_recipe(item).is_some(),
        item,
    )
}

/// O tanque de químico de uma máquina e o slot que o enche (`ChemicalInventorySlot.fillOrConvert`).
pub struct ChemicalSide {
    pub tank: ChemicalTank,
    pub slot: InventorySlot,
    /// `MekanismRecipeType.CHEMICAL_CONVERSION.getInputCache()`.
    pub conversions: Rc<ConversionCache>,
}

/// O que as duas máquinas partilham: o tile, a energia, a saída, o slot de energia e os listeners dos contêineres, como o
/// `TileEntityRecipeMachine` os entrega a `getInitial*`.
struct Parts {
    shared: Rc<Shared>,
    /// `listener`: o próprio tile, que só marca para salvar.
    tile_changed: Callback,
    /// `recipeCacheListener`: o monitor, que também diz que pode haver receita de novo.
    cache_listener: Callback,
    flags: Rc<MonitorFlags>,
    energy: EnergyContainer,
    output: InventorySlot,
    energy_slot: InventorySlot,
    energy_conversion: Rc<EnergyConversionCache>,
    upgrades: UpgradeComponent,
}

impl Parts {
    fn new(config: &MachineConfig, energy_conversion: Rc<EnergyConversionCache>) -> Self {
        let shared = Rc::new(Shared {
            tile: RefCell::new(TileBase::new(config.deactivation_delay)),
            upgrade_config: config.upgrades.clone(),
            base_ticks_required: config.base_ticks_required,
            check_offset: config.check_offset,
            cache_invalid: Cell::new(false),
            ticks_required: Cell::new(config.base_ticks_required),
            operations_per_tick: Cell::new(1),
            operating_ticks: Cell::new(0),
            used_so_far: Cell::new(0),
            tracked_errors: RefCell::new(vec![false; config.tracked_errors.len()]),
            tracked_error_kinds: config.tracked_errors.clone(),
            players_using: Cell::new(0),
            game_time: Cell::new(0),
        });
        let flags = MonitorFlags::new();
        // Os listeners do `TileEntityRecipeMachine`: o do monitor, o de salvar, e os que despausam.
        let tile_changed: Callback = {
            let shared = shared.clone();
            Rc::new(move || shared.tile.borrow_mut().set_changed())
        };
        let cache_listener: Callback = {
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
        let energy = EnergyContainer::input(
            STORAGE_MULTIPLIER * i64::from(config.energy_per_tick),
            config.energy_per_tick,
            Some(unpause_save_only),
        );
        let output = InventorySlot::output(Some(output_listener), Some(decreased));
        // `EnergyInventorySlot.fillOrConvert(energyContainer, .., listener, ..)`: o listener é o tile.
        let energy_slot = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks::fill_or_convert({
                let (energy, conversions) = (energy.clone(), energy_conversion.clone());
                Rc::new(move |item: &ItemResource| {
                    energy_can_fill_or_convert(
                        &energy,
                        &|_| false,
                        &|item| conversions.find_type_based_recipe(item).is_some(),
                        item,
                    )
                })
            }),
            Some(tile_changed.clone()),
            None,
        );
        // `new TileComponentUpgrade(this)`: os slots avisam o tile, e o tile recalcula o que depende de cada tipo.
        let recalculate = {
            let (shared, energy) = (shared.clone(), energy.clone());
            Box::new(
                move |upgrades: &UpgradeComponent, upgrade: &str, total: i32| {
                    recalculate_upgrades(&shared, &energy, upgrades, upgrade, total);
                },
            )
        };
        let upgrades = UpgradeComponent::new(
            config.upgrades.data.clone(),
            tile_changed.clone(),
            recalculate,
        );
        Self {
            shared,
            tile_changed,
            cache_listener,
            flags,
            energy,
            output,
            energy_slot,
            energy_conversion,
            upgrades,
        }
    }
}

/// `TileEntityProgressMachine`: a base da Enrichment Chamber, do Crusher, do Energized Smelter e do Metallurgic Infuser.
pub struct ProgressMachine {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
    energy_slot: InventorySlot,
    energy_conversion: Rc<EnergyConversionCache>,
    chemical: Option<ChemicalSide>,
    /// `getInventorySlots`, na ordem do Java: é a média deles que dá o nível do comparador.
    inventory_slots: Vec<InventorySlot>,
    monitor: Box<dyn MonitorOps>,
    ejector: ItemEjector,
    upgrades: UpgradeComponent,
}

impl ProgressMachine {
    /// `TileEntityElectricMachine`.
    pub fn electric(config: ElectricMachineConfig) -> Self {
        let parts = Parts::new(&config.machine, config.energy_conversion.clone());
        // `InputInventorySlot.at(this::containsRecipe, recipeCacheListener, ..)`: o item só entra se alguma receita o usa.
        let recipes = config.recipes;
        let validator: ItemTest = {
            let recipes = recipes.clone();
            Rc::new(move |item: &ItemResource| recipes.contains_input(item))
        };
        // `InputInventorySlot` passa `notExternal()` como `canExtract`: quem está de fora (funil, cano) não tira a entrada.
        let input = InventorySlot::with_checks(
            Access::NotExternal,
            Access::Any,
            SlotChecks {
                validator: Some(validator),
                can_insert: None,
                ..SlotChecks::default()
            },
            Some(parts.cache_listener.clone()),
            None,
        );
        let lookup = Rc::new(ElectricLookup {
            shared: parts.shared.clone(),
            energy: parts.energy.clone(),
            input: input.clone(),
            output: parts.output.clone(),
            recipes,
        });
        let monitor = RecipeCacheLookupMonitor::new(lookup.clone(), parts.flags.clone(), true);
        // `getInitialInventory`: entrada, saída e energia.
        let inventory_slots = vec![
            input.clone(),
            parts.output.clone(),
            parts.energy_slot.clone(),
        ];
        Self::assemble(parts, input, None, inventory_slots, Box::new(monitor))
    }

    /// `TileEntityMetallurgicInfuser`.
    pub fn chemical(config: ChemicalMachineConfig) -> Self {
        let parts = Parts::new(&config.machine, config.energy_conversion.clone());
        let recipes = config.recipes;
        // O slot de entrada pergunta pelo tanque e o tanque pelo slot: o tanque entra aqui quando existir.
        let tank_cell: Rc<RefCell<Option<ChemicalTank>>> = Rc::new(RefCell::new(None));
        // `InputInventorySlot.at((itemType, _) -> containsRecipeAB(itemType, infusionTank.resource()), this::containsRecipeA,
        // recipeCacheListener, ..)`.
        let validator: ItemTest = {
            let recipes = recipes.clone();
            Rc::new(move |item: &ItemResource| recipes.contains_input_a(item))
        };
        let can_insert: ItemTest = {
            let (recipes, tank_cell) = (recipes.clone(), tank_cell.clone());
            Rc::new(move |item: &ItemResource| {
                let held = tank_cell.borrow().as_ref().and_then(ChemicalTank::resource);
                recipes.contains_input_ab(item, &held)
            })
        };
        // `InputInventorySlot`: ver o slot da Enrichment Chamber, `canExtract` é `notExternal()`.
        let input = InventorySlot::with_checks(
            Access::NotExternal,
            Access::Any,
            SlotChecks {
                validator: Some(validator),
                can_insert: Some(can_insert),
                ..SlotChecks::default()
            },
            Some(parts.cache_listener.clone()),
            None,
        );
        // `getInitialChemicalTanks`: o tanque só recusa o que nenhuma receita usa (`containsRecipeB`), e só aceita o que combina
        // com o item que está no slot de entrada (`containsRecipeBA`). Quem o esvazia pelo lado de fora não pode (`alwaysTrueBi`
        // para extrair).
        let tank = {
            let validator = {
                let recipes = recipes.clone();
                Rc::new(move |chemical: &ChemicalResource| recipes.contains_input_b(chemical))
            };
            let can_insert = {
                let (recipes, input) = (recipes.clone(), input.downgrade());
                Rc::new(move |chemical: &ChemicalResource, _| {
                    let item = input.upgrade().and_then(|input| input.resource());
                    recipes.contains_input_ba(&item, chemical)
                })
            };
            ChemicalTank::create(
                config.tank_capacity,
                Rc::new(|_, _| true),
                can_insert,
                validator,
                Some(parts.cache_listener.clone()),
            )
        };
        *tank_cell.borrow_mut() = Some(tank.clone());
        // `ChemicalInventorySlot.fillOrConvert(infusionTank, .., listener, ..)`: o listener é o tile.
        let infusion_slot = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks::fill_or_convert({
                let (tank, conversions) = (tank.clone(), config.conversions.clone());
                Rc::new(move |item: &ItemResource| can_fill_or_convert(&tank, &conversions, item))
            }),
            Some(parts.tile_changed.clone()),
            None,
        );
        let lookup = ChemicalLookup {
            shared: parts.shared.clone(),
            energy: parts.energy.clone(),
            input: input.clone(),
            output: parts.output.clone(),
            tank: tank.clone(),
            recipes,
        };
        let monitor = RecipeCacheLookupMonitor::new(Rc::new(lookup), parts.flags.clone(), true);
        // `getInitialInventory`: infusão, entrada, saída e energia.
        let inventory_slots = vec![
            infusion_slot.clone(),
            input.clone(),
            parts.output.clone(),
            parts.energy_slot.clone(),
        ];
        let side = ChemicalSide {
            tank,
            slot: infusion_slot,
            conversions: config.conversions,
        };
        Self::assemble(parts, input, Some(side), inventory_slots, Box::new(monitor))
    }

    fn assemble(
        parts: Parts,
        input: InventorySlot,
        chemical: Option<ChemicalSide>,
        inventory_slots: Vec<InventorySlot>,
        monitor: Box<dyn MonitorOps>,
    ) -> Self {
        // O ejetor de itens sai pelo slot de saída, em nenhuma face até a configuração mandar.
        let ejector = ItemEjector::new(vec![EjectGroup::new(
            vec![parts.output.clone()],
            Vec::new(),
        )]);
        Self {
            shared: parts.shared,
            energy: parts.energy,
            input,
            output: parts.output,
            energy_slot: parts.energy_slot,
            energy_conversion: parts.energy_conversion,
            chemical,
            inventory_slots,
            monitor,
            ejector,
            upgrades: parts.upgrades,
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

    /// O tanque e o slot que o enche, nas máquinas que têm.
    pub fn chemical_side(&self) -> Option<&ChemicalSide> {
        self.chemical.as_ref()
    }

    pub fn ejector(&self) -> &ItemEjector {
        &self.ejector
    }

    pub fn ejector_mut(&mut self) -> &mut ItemEjector {
        &mut self.ejector
    }

    pub fn ticks_required(&self) -> i32 {
        self.shared.ticks_required.get()
    }

    /// `getOperationsPerTick`.
    pub fn operations_per_tick(&self) -> i32 {
        self.shared.operations_per_tick.get()
    }

    pub fn recipe_progress(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    /// `saveAdditional`, só do que a lib tem.
    pub fn saved_state(&self) -> SavedState {
        let mut saved = self.saved_scalars();
        (
            saved.recipes_used,
            saved.last_recipe_used,
            saved.last_recipe_amount,
        ) = self.monitor.used_snapshot();
        saved.upgrades = self.upgrades.saved();
        saved
    }

    /// `saved_state` sem as receitas usadas e sem os upgrades, que são mapas: quem salva a cada tick compara só isto e olha
    /// `used_version` e `upgrades_version` para saber se eles mudaram.
    pub fn saved_scalars(&self) -> SavedState {
        let tile = self.shared.tile.borrow();
        SavedState {
            redstone: tile.is_powered(),
            control_type: tile.control_type(),
            active: tile.active(),
            update_delay: tile.update_delay(),
            current_redstone: tile.current_redstone_level(),
            progress: self.shared.operating_ticks.get(),
            used_so_far: self.shared.used_so_far.get(),
            recipes_used: Default::default(),
            last_recipe_used: None,
            last_recipe_amount: 0,
            ejecting: self.ejector.is_ejecting(),
            upgrades: UpgradeSaved::default(),
        }
    }

    /// Muda quando as receitas usadas mudam.
    pub fn used_version(&self) -> u32 {
        self.monitor.used_version()
    }

    /// Muda quando os upgrades instalados ou os dois slots mudam.
    pub fn upgrades_version(&self) -> u32 {
        self.upgrades.version()
    }

    /// `TileEntityMekanism.getComponent`: o componente de upgrades.
    pub fn upgrades(&self) -> &UpgradeComponent {
        &self.upgrades
    }

    /// `loadAdditional`: o que `saved_state` devolveu volta ao objeto novo. Chame antes do primeiro tick.
    pub fn restore(&mut self, saved: &SavedState, block_active: bool) {
        // Os componentes leem antes do resto, e os upgrades recalculam o consumo e a capacidade antes de a energia voltar.
        self.upgrades.restore(&saved.upgrades);
        self.shared.tile.borrow_mut().restore(saved, block_active);
        self.shared.operating_ticks.set(saved.progress);
        self.shared.used_so_far.set(saved.used_so_far);
        self.monitor.restore_used(
            saved.recipes_used.clone(),
            saved.last_recipe_used.clone(),
            saved.last_recipe_amount,
        );
        self.ejector.set_ejecting(saved.ejecting);
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

    /// `trackedErrors` como bits para um data slot, na ordem de `TRACKED_ERROR_TYPES`. O cliente refaz o `getWarningCheck` com
    /// [`ErrorMask::has`] e a mesma lista.
    pub fn tracked_errors_mask(&self) -> ErrorMask {
        ErrorMask::from_flags(&self.shared.tracked_errors.borrow())
    }

    /// `shouldRecheckAllErrors`: com jogador na tela, uma vez a cada `RECIPE_CHECK_FREQUENCY` ticks, no sorteio da máquina. Lê o
    /// relógio do último `tick_server`.
    pub fn recheck_all_errors(&self) -> bool {
        self.shared.recheck_all_errors()
    }

    /// `invalidateCache`: os dados recarregaram (`MekanismRecipeType.clearCache`) e as tags também, então a receita em cache
    /// deixa de valer. Quem recarrega limpa os índices de receitas (`clear`); aqui o monitor larga a receita que tinha.
    pub fn invalidate_recipe_cache(&self) {
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
        redstone_level_of(&self.inventory_slots)
    }

    /// `tickServer`. Devolve se o comparador precisa avisar os vizinhos.
    pub fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) -> bool {
        self.shared.game_time.set(game_time);
        // Na ordem de `TileEntityMekanism.tickServer`: o componente de upgrades vem antes do atraso do estado ativo.
        self.upgrades.tick_server();
        self.shared.tile.borrow_mut().tick_delay();
        // `onUpdateServer`: o ejetor, do `TileEntityConfigurableMachine`, o slot de energia, o que enche o tanque e depois a receita.
        self.ejector.tick_server(neighbours);
        fill_container_or_convert(
            &self.energy_slot,
            &self.energy,
            &self.energy_conversion,
            None,
        );
        if let Some(side) = &self.chemical {
            fill_tank_or_convert(&side.slot, &side.tank, &side.conversions, None);
        }
        self.monitor.update_and_process();
        let slots = &self.inventory_slots;
        self.shared
            .tile
            .borrow_mut()
            .finish_tick(|| redstone_level_of(slots))
    }
}

/// `recalculateUpgrades` de `TileEntityProgressMachine` e depois de `TileEntityMekanism`, para o tipo que mudou: o de velocidade
/// refaz o tempo e as operações por tick, e os dois (velocidade e energia) refazem o consumo e a capacidade do contêiner.
fn recalculate_upgrades(
    shared: &Shared,
    energy: &EnergyContainer,
    upgrades: &UpgradeComponent,
    upgrade: &str,
    total: i32,
) {
    let config = &shared.upgrade_config;
    let data = upgrades.data();
    let (speed, energy_upgrade) = (&*config.speed, &*config.energy);
    let rules = |max_speed: i32, max_energy: i32| UpgradeRules {
        max_upgrade_multiplier: config.max_upgrade_multiplier,
        max_speed,
        max_energy,
    };
    if upgrade == speed
        && let Some(max_speed) = data.max(speed)
    {
        let rules = rules(max_speed, 1);
        shared
            .ticks_required
            .set(rules.ticks(shared.base_ticks_required, total));
        shared.operations_per_tick.set(rules.operations_per_tick(
            shared.base_ticks_required,
            1,
            total,
        ));
    }
    if upgrade != speed && upgrade != energy_upgrade {
        return;
    }
    update_energy_per_tick(shared, energy, upgrades, &data);
    update_max_energy(shared, energy, upgrades, &data);
}

/// `MachineEnergyContainer.updateEnergyPerTick`.
fn update_energy_per_tick(
    shared: &Shared,
    energy: &EnergyContainer,
    upgrades: &UpgradeComponent,
    data: &UpgradeData,
) {
    let config = &shared.upgrade_config;
    let (speed, energy_upgrade) = (&*config.speed, &*config.energy);
    let (Some(max_speed), Some(max_energy)) = (data.max(speed), data.max(energy_upgrade)) else {
        energy.set_energy_per_tick(energy.base_energy_per_tick());
        return;
    };
    if data.supports(energy_upgrade) || data.supports(speed) {
        let rules = UpgradeRules {
            max_upgrade_multiplier: config.max_upgrade_multiplier,
            max_speed,
            max_energy,
        };
        energy.set_energy_per_tick(rules.energy_per_tick(
            energy.base_energy_per_tick(),
            upgrades.get_upgrades(speed),
            upgrades.get_upgrades(energy_upgrade),
        ));
    }
}

/// `MachineEnergyContainer.updateMaxEnergy`.
fn update_max_energy(
    shared: &Shared,
    energy: &EnergyContainer,
    upgrades: &UpgradeComponent,
    data: &UpgradeData,
) {
    let config = &shared.upgrade_config;
    let (speed, energy_upgrade) = (&*config.speed, &*config.energy);
    if data.max(speed).is_some() && data.supports(speed) {
        let buffer = STORAGE_MULTIPLIER.max(i64::from(shared.ticks_required.get()));
        energy.set_max_energy(i64::from(energy.energy_per_tick()) * buffer);
    } else if let Some(max_energy) = data.max(energy_upgrade)
        && data.supports(energy_upgrade)
    {
        // `UpgradeUtils.getMaxEnergy`: `clampToLong` do `double`, que em Rust é a conversão que satura.
        let fraction = f64::from(upgrades.get_upgrades(energy_upgrade)) / f64::from(max_energy);
        let max = energy.base_max_energy() as f64 * config.max_upgrade_multiplier.powf(fraction);
        energy.set_max_energy(max as i64);
    }
}

fn redstone_level_of(slots: &[InventorySlot]) -> i32 {
    let slots: Vec<&InventorySlot> = slots.iter().collect();
    redstone_signal(&slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy_conversion::EnergyConversion;

    fn conversions() -> Rc<EnergyConversionCache> {
        EnergyConversion::cache_of(vec![EnergyConversion {
            input: crate::cached_recipe::ItemIngredient::item("redstone"),
            input_count: 1,
            output: 10_000,
        }])
    }

    #[test]
    fn the_energy_slot_of_a_machine_takes_what_converts_and_nothing_else() {
        let cache = conversions();
        assert!(machine_energy_slot_accepts(
            &cache,
            &ItemResource::new("redstone", 64)
        ));
        assert!(!machine_energy_slot_accepts(
            &cache,
            &ItemResource::new("stick", 64)
        ));
    }

    #[test]
    fn a_container_that_refuses_manual_insertion_makes_the_conversion_unacceptable() {
        let cache = conversions();
        let redstone = ItemResource::new("redstone", 64);
        let converts = |item: &ItemResource| cache.find_type_based_recipe(item).is_some();
        let open = EnergyContainer::input(20, 5, None);
        assert!(energy_can_fill_or_convert(
            &open,
            &|_| false,
            &converts,
            &redstone
        ));
        let shut = EnergyContainer::new(20, 5, Access::Any, Access::InternalOnly, None);
        assert!(!energy_can_fill_or_convert(
            &shut,
            &|_| false,
            &converts,
            &redstone
        ));
        // Um item que enche o contêiner entra pelo outro caminho, se o contêiner deixa tirar de dentro.
        assert!(energy_can_fill_or_convert(
            &open,
            &|_| true,
            &|_| false,
            &ItemResource::new("battery", 1)
        ));
    }

    const RULES: UpgradeRules = UpgradeRules {
        max_upgrade_multiplier: 10.0,
        max_speed: 8,
        max_energy: 8,
    };

    #[test]
    fn the_redstone_slot_round_trips_every_combination() {
        for ordinal in 0..4 {
            for powered in [false, true] {
                for was_powered in [false, true] {
                    let sync = RedstoneSync {
                        control_type: RedstoneControl::from_ordinal(ordinal),
                        powered,
                        was_powered,
                    };
                    let slot = sync.to_slot();
                    assert!((0..=0xF).contains(&slot), "cabe em quatro bits");
                    assert_eq!(RedstoneSync::from_slot(slot), sync);
                    // O Horizon manda 16 bits com sinal, e o cliente lê o que chega.
                    assert_eq!(RedstoneSync::from_slot(i32::from(slot as i16)), sync);
                }
            }
        }
        assert_eq!(RedstoneControl::from_ordinal(9), RedstoneControl::Disabled);
        assert_eq!(RedstoneControl::Pulse.ordinal(), 3);
    }

    #[test]
    fn the_client_activation_follows_the_server_one() {
        let mut tile = TileBase::new(3);
        for control in [
            RedstoneControl::Disabled,
            RedstoneControl::High,
            RedstoneControl::Low,
        ] {
            tile.set_control_type(control);
            for powered in [false, true] {
                tile.update_power(powered);
                assert_eq!(
                    RedstoneSync::from_slot(tile.redstone_sync().to_slot()).is_activated(),
                    tile.is_redstone_activated(),
                    "{control:?} powered={powered}"
                );
            }
        }
    }

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
