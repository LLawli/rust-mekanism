//! Repete, tick a tick, o rastro gravado do Osmium Compressor original em `dev/referencia/rastros/compressor_run.jsonl`: a
//! receita que gasta o osmium a cada tick (`per_tick_usage`), o tanque que seca e pausa a receita, o upgrade de velocidade que
//! faz o gasto alcançar o total, a energia que acaba e o item que vira osmium.
//!
//! A máquina é montada aqui, só com o que a lib entrega (`CachedRecipe`, o monitor, os contêineres e a receita de uso
//! constante), como o `TileEntityAdvancedElectricMachine` monta: o Osmium Compressor ainda não existe como máquina portada
//! (Fase 6). O Metallurgic Infuser, que o `ProgressMachine` monta e que escolhe a receita de uso constante, está em
//! `infuser_constant_trace.rs`.
use machinery_basis_core::cached_recipe::{CachedRecipe, ItemIngredient, ItemOutput};
use machinery_basis_core::chemical::{
    ChemicalConversion, ChemicalIngredient, ChemicalInputHandler, ChemicalResource, ChemicalTank,
    ConversionCache, fill_tank_or_convert,
};
use machinery_basis_core::containers::{
    AutomationType, EnergyContainer, InventorySlot, ItemResource,
};
use machinery_basis_core::item_chemical_constant_recipe::{
    ChemicalUsageMultiplier, ItemChemicalConstantUsageRecipe,
};
use machinery_basis_core::operation_tracker::{RecipeError, RecipeErrors};
use machinery_basis_core::recipe_handlers::{ItemInputHandler, ItemOutputHandler};
use machinery_basis_core::recipe_monitor::{
    MonitorFlags, RecipeCacheLookupMonitor, RecipeLookupHandler,
};
use machinery_basis_core::tile::{STORAGE_MULTIPLIER, UpgradeRules};
use machinery_basis_core::transaction::Callback;
use machinery_basis_core::two_input_recipe::{ItemChemicalCache, ItemChemicalToItemRecipe};
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 500;
/// `TileEntityAdvancedElectricMachine.BASE_TICKS_REQUIRED`.
const BASE_TICKS_REQUIRED: i32 = 200;
/// `TileEntityAdvancedElectricMachine.MAX_GAS`.
const MAX_GAS: u64 = 210;
/// `MekanismConfig.usage.osmiumCompressor`.
const USAGE: i32 = 15;
const UPGRADE_RULES: UpgradeRules = UpgradeRules {
    max_upgrade_multiplier: 10.0,
    max_speed: 8,
    max_energy: 8,
};
/// Uma pilha de item sem id próprio no teste.
const STACK: u32 = 64;

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, STACK)
}

fn osmium() -> ChemicalResource {
    ChemicalResource::new("osmium")
}

/// O que o tile guarda e as closures das receitas alcançam.
struct Shared {
    active: Cell<bool>,
    ticks_required: Cell<i32>,
    operations_per_tick: Cell<i32>,
    base_total_usage: Cell<i32>,
    used_so_far: Cell<i32>,
    operating_ticks: Cell<i32>,
    errors: Cell<RecipeErrors>,
}

struct Lookup {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    output: InventorySlot,
    tank: ChemicalTank,
    recipes: Rc<ItemChemicalCache>,
}

impl RecipeLookupHandler<ItemChemicalToItemRecipe> for Lookup {
    /// `getRecipe`: `findFirstRecipe(itemInputHandler, chemicalInputHandler)`.
    fn get_recipe(&self) -> Option<Rc<ItemChemicalToItemRecipe>> {
        self.recipes
            .find_first_recipe(&self.input.contents(), &self.tank.contents())
    }

    /// `createNewCachedRecipe` com `recipe.perTickUsage()` verdadeiro.
    fn create_new_cached_recipe(&self, recipe: Rc<ItemChemicalToItemRecipe>) -> CachedRecipe {
        let shared = &self.shared;
        let usage = {
            let (total, required) = (shared.clone(), shared.clone());
            ChemicalUsageMultiplier::constant_use(
                move || total.base_total_usage.get(),
                move || required.ticks_required.get(),
            )
        };
        let logic = ItemChemicalConstantUsageRecipe::new(
            recipe,
            ItemInputHandler::new(self.input.clone(), RecipeError::NOT_ENOUGH_INPUT),
            ChemicalInputHandler::new(self.tank.clone(), RecipeError::NOT_ENOUGH_SECONDARY_INPUT),
            usage,
            {
                let shared = shared.clone();
                move |used| shared.used_so_far.set(used)
            },
            ItemOutputHandler::new(self.output.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
        );
        let (errors, active, required, operating, operations) = (
            shared.clone(),
            shared.clone(),
            shared.clone(),
            shared.clone(),
            shared.clone(),
        );
        let energy = self.energy.clone();
        CachedRecipe::new(Box::new(logic), Box::new(|| false))
            .set_errors_changed(move |new| errors.errors.set(new))
            .set_active(move |value| active.active.set(value))
            .set_energy_requirements(
                {
                    let energy = energy.clone();
                    move || energy.energy_per_tick()
                },
                energy,
            )
            .set_required_ticks(move || required.ticks_required.get())
            .set_operating_ticks_changed(move |ticks| operating.operating_ticks.set(ticks))
            .set_baseline_max_operations(move || operations.operations_per_tick.get())
    }

    fn saved_operating_ticks(&self) -> i32 {
        self.shared.operating_ticks.get()
    }

    fn saved_used_so_far(&self) -> Option<i32> {
        Some(self.shared.used_so_far.get())
    }

    /// `clearRecipeErrors`.
    fn on_cached_recipe_changed(&self) {
        self.shared.errors.set(RecipeErrors::EMPTY);
    }
}

struct Compressor {
    shared: Rc<Shared>,
    energy: EnergyContainer,
    input: InventorySlot,
    secondary: InventorySlot,
    output: InventorySlot,
    tank: ChemicalTank,
    conversions: Rc<ConversionCache>,
    monitor: RecipeCacheLookupMonitor<ItemChemicalToItemRecipe>,
    refill_ticks: i32,
}

fn recipe(input: &str, chemical: &str, output: &str, count: u32) -> Rc<ItemChemicalToItemRecipe> {
    Rc::new(ItemChemicalToItemRecipe {
        id: format!("mekanism:compressing/{output}"),
        item_input: ItemIngredient::item(input),
        item_count: 1,
        chemical_input: ChemicalIngredient::chemical(chemical),
        chemical_amount: 1,
        output: ItemOutput {
            resource: item(output),
            count,
        },
        per_tick_usage: true,
    })
}

/// O Osmium Compressor: o `TileEntityAdvancedElectricMachine` sem o que a lib de tile já cobre (redstone, bloco, tela).
fn compressor(recipes: Vec<Rc<ItemChemicalToItemRecipe>>) -> Compressor {
    let shared = Rc::new(Shared {
        active: Cell::new(false),
        ticks_required: Cell::new(BASE_TICKS_REQUIRED),
        operations_per_tick: Cell::new(1),
        base_total_usage: Cell::new(BASE_TICKS_REQUIRED),
        used_so_far: Cell::new(0),
        operating_ticks: Cell::new(0),
        errors: Cell::new(RecipeErrors::EMPTY),
    });
    let recipes = ItemChemicalToItemRecipe::cache_of(recipes, "empty");
    let flags = MonitorFlags::new();
    let cache_listener: Callback = {
        let flags = flags.clone();
        Rc::new(move || flags.on_change())
    };
    let output_listener: Callback = {
        let flags = flags.clone();
        Rc::new(move || flags.unpause())
    };
    let decreased: Callback = {
        let flags = flags.clone();
        Rc::new(move || flags.output_decreased())
    };
    let unpause: Callback = {
        let flags = flags.clone();
        Rc::new(move || flags.unpause())
    };
    let energy =
        EnergyContainer::input(STORAGE_MULTIPLIER * i64::from(USAGE), USAGE, Some(unpause));
    let input = InventorySlot::new(Some(cache_listener.clone()));
    let secondary = InventorySlot::new(None);
    let output = InventorySlot::output(Some(output_listener), Some(decreased));
    // `getInitialChemicalTanks`: sem estatística, o tanque deixa extrair de fora, só aceita o que combina com o item da
    // entrada e recusa o que nenhuma receita usa.
    let tank = {
        let validator = {
            let recipes = recipes.clone();
            Rc::new(move |chemical: &ChemicalResource| recipes.contains_input_b(chemical))
        };
        let can_insert = {
            let (recipes, input) = (recipes.clone(), input.clone());
            Rc::new(move |chemical: &ChemicalResource, _: AutomationType| {
                recipes.contains_input_ba(&input.resource(), chemical)
            })
        };
        ChemicalTank::create(
            MAX_GAS,
            Rc::new(|_, _| true),
            can_insert,
            validator,
            Some(cache_listener),
        )
    };
    let lookup = Lookup {
        shared: shared.clone(),
        energy: energy.clone(),
        input: input.clone(),
        output: output.clone(),
        tank: tank.clone(),
        recipes,
    };
    let monitor = RecipeCacheLookupMonitor::new(Rc::new(lookup), flags, true);
    Compressor {
        shared,
        energy,
        input,
        secondary,
        output,
        tank,
        conversions: ChemicalConversion::cache_of(vec![ChemicalConversion {
            input: ItemIngredient::item("ingot_osmium"),
            input_count: 1,
            output: osmium(),
            amount: 200,
        }]),
        monitor,
        refill_ticks: 0,
    }
}

fn glowstone_compressor() -> Compressor {
    compressor(vec![recipe(
        "glowstone_dust",
        "osmium",
        "ingot_refined_glowstone",
        1,
    )])
}

impl Compressor {
    /// `onUpdateServer`: o slot de gás converte, e o monitor processa.
    fn tick(&mut self) {
        fill_tank_or_convert(&self.secondary, &self.tank, &self.conversions, None);
        self.monitor.update_and_process();
    }

    /// `Trace.refill`.
    fn refill(&self) {
        if self.refill_ticks > 0 {
            self.energy.set_energy(
                i64::from(self.refill_ticks) * i64::from(self.energy.energy_per_tick()),
                None,
            );
        }
    }

    /// `recalculateUpgrades` para o upgrade de velocidade: o tempo, as operações por tick, o consumo e a capacidade. A
    /// máquina não aceita o upgrade de gás, então o total de uso (`baseTotalUsage`) segue o padrão.
    fn install_speed(&self, speed: i32) {
        let shared = &self.shared;
        shared
            .ticks_required
            .set(UPGRADE_RULES.ticks(BASE_TICKS_REQUIRED, speed));
        shared
            .operations_per_tick
            .set(UPGRADE_RULES.operations_per_tick(BASE_TICKS_REQUIRED, 1, speed));
        self.energy
            .set_energy_per_tick(UPGRADE_RULES.energy_per_tick(USAGE, speed, 0));
        let buffer = STORAGE_MULTIPLIER.max(i64::from(shared.ticks_required.get()));
        self.energy
            .set_max_energy(i64::from(self.energy.energy_per_tick()) * buffer);
    }

    fn set_osmium(&self, amount: u64) {
        let resource = (amount > 0).then(osmium);
        self.tank.set_contents(resource, amount, None);
    }

    /// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
    fn apply(&mut self, action: &str) {
        let parts: Vec<&str> = action.split_whitespace().collect();
        match parts.as_slice() {
            ["item", id, amount] => {
                self.input
                    .set_contents(Some(item(id)), amount.parse().unwrap(), None)
            }
            ["osmium", amount] => self.set_osmium(amount.parse().unwrap()),
            ["speed", amount] => self.install_speed(amount.parse().unwrap()),
            ["energy", amount] => self.energy.set_energy(amount.parse().unwrap(), None),
            ["no", "top", "ups"] => self.refill_ticks = 0,
            ["top", "ups", "back"] => self.refill_ticks = 3,
            ["gas", "slot", id, amount] => {
                self.secondary
                    .set_contents(Some(item(id)), amount.parse().unwrap(), None);
            }
            other => panic!("marca desconhecida: {other:?}"),
        }
    }

    fn state(&self, expected: &Value) -> Value {
        let slot = |slot: &InventorySlot| {
            let id = slot
                .resource()
                .map_or(String::new(), |resource| resource.id.to_string());
            json!([id, slot.amount()])
        };
        let names = [
            (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
            (RecipeError::NOT_ENOUGH_INPUT, "input"),
            (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
            (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
            (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
        ];
        let errors: Vec<&str> = names
            .iter()
            .filter(|(error, _)| self.shared.errors.get().contains(*error))
            .map(|(_, name)| *name)
            .collect();
        json!({
            "t": expected["t"],
            "marks": expected["marks"],
            "progress": self.shared.operating_ticks.get(),
            "required": self.shared.ticks_required.get(),
            "energy": self.energy.amount(),
            "capacity": self.energy.capacity(),
            "per_tick": self.energy.energy_per_tick(),
            "active": self.shared.active.get(),
            "input": slot(&self.input),
            "output": slot(&self.output),
            "errors": errors,
            "tank": self.tank.amount(),
            "gas_slot": slot(&self.secondary),
            "used_so_far": self.shared.used_so_far.get(),
            "max_operations": self.shared.operations_per_tick.get(),
        })
    }
}

/// O rastro grava também o que é do tile (o bloco, o sinal e o comparador): aqui não há tile.
const TILE_FIELDS: [&str; 3] = ["block_active", "powered", "comparator"];

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| {
            let mut line: Value = serde_json::from_str(line).unwrap();
            for key in TILE_FIELDS {
                line.as_object_mut().unwrap().remove(key);
            }
            line
        })
        .collect()
}

#[test]
fn the_compressor_follows_the_recorded_run_tick_by_tick() {
    let lines = read_trace("compressor_run");
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    let mut machine = glowstone_compressor();
    // O `trace.refillTicks = 3` do cenário, posto antes do primeiro tick.
    machine.refill_ticks = 3;
    let mut seen_marks = 0;
    for expected in &lines {
        machine.tick();
        for mark in expected["marks"].as_array().unwrap() {
            seen_marks += 1;
            for action in mark.as_str().unwrap().split(", ") {
                machine.apply(action);
            }
        }
        assert_eq!(&machine.state(expected), expected, "tick {}", expected["t"]);
        machine.refill();
    }
    assert_eq!(seen_marks, 8, "as marcas do cenário");
}

/// Um tick com a energia sempre cheia, como um cabo que a mantém.
fn tick_with_energy(machine: &mut Compressor) {
    machine.energy.set_energy(machine.energy.capacity(), None);
    machine.tick();
}

/// Salvar e reabrir no meio de uma operação: o objeto nasce de novo do que o Java persiste (os itens, o tanque, a energia, o
/// progresso e o `usedSoFar`), e o monitor devolve os dois à receita nova (`loadSavedData`). O gasto segue de onde parou e a
/// operação termina com o total de 200, e não com 200 a mais.
#[test]
fn a_reborn_compressor_goes_on_from_the_saved_progress_and_the_saved_usage() {
    let mut before = glowstone_compressor();
    before
        .input
        .set_contents(Some(item("glowstone_dust")), 2, None);
    before.set_osmium(210);
    for _ in 0..60 {
        tick_with_energy(&mut before);
    }
    let (progress, used) = (
        before.shared.operating_ticks.get(),
        before.shared.used_so_far.get(),
    );
    assert_eq!((progress, used), (60, 60));

    let mut reborn = glowstone_compressor();
    reborn.shared.operating_ticks.set(progress);
    reborn.shared.used_so_far.set(used);
    reborn
        .input
        .set_contents(Some(item("glowstone_dust")), 2, None);
    reborn.set_osmium(before.tank.amount());
    for _ in 0..140 {
        tick_with_energy(&mut reborn);
    }
    // 60 + 140 = 200 ticks: a operação fechou, gastou 200 no total e deixou 10, e o gasto voltou a zero.
    assert_eq!(reborn.output.amount(), 1);
    assert_eq!(reborn.tank.amount(), 10);
    assert_eq!(reborn.shared.used_so_far.get(), 0);
}

/// Sem o `usedSoFar` salvo (o tile não é um `ConstantUsageRecipeLookupHandler`, ou perdeu o campo), a receita conta o gasto
/// do zero e gasta mais nos ticks que faltam: é o que o `loadSavedUsageSoFar` existe para evitar.
#[test]
fn without_the_saved_usage_the_operation_would_spend_more() {
    let spent_in_the_last_140_ticks = |saved_usage: i32| {
        let mut reborn = glowstone_compressor();
        reborn.shared.operating_ticks.set(60);
        reborn.shared.used_so_far.set(saved_usage);
        reborn
            .input
            .set_contents(Some(item("glowstone_dust")), 2, None);
        reborn.set_osmium(210);
        for _ in 0..140 {
            tick_with_energy(&mut reborn);
        }
        assert_eq!(reborn.output.amount(), 1);
        210 - reborn.tank.amount()
    };
    // Com os 60 já gastos, o ritmo é de 1 por tick e a operação fecha com 140 a mais. Sem eles, de 200 / 140 ticks, o
    // mesmo 1 por tick, e o último tick paga os 61 que "faltam" do total.
    assert_eq!(spent_in_the_last_140_ticks(60), 140);
    assert_eq!(spent_in_the_last_140_ticks(0), 200);
}
