//! Repete, tick a tick, os rastros do Metallurgic Infuser original com uma receita de `per_tick_usage`
//! (`dev/referencia/rastros/infuser_const_<cenário>.jsonl`, escritos por `dev/referencia/testes/ConstantUsageTraceTest.java`): o
//! `ItemStackConstantChemicalToObjectCachedRecipe` que o `createNewCachedRecipe` do tile escolhe, com o `usedSoFar` do tile e o
//! `ChemicalUsageMultiplier.constantUse(() -> ticksRequired, this::getTicksRequired)`.
//!
//! Nenhuma receita dos arquivos do original usa a opção no Infuser, então a receita vem de um arquivo de teste, o mesmo que o data
//! pack do teste em Java carrega (`dev/referencia/testes/dados/data/mekanismtests/recipe/metallurgic_infusing/`). A máquina é a do
//! jogador (`applyImplicitComponents` do item), e o cabo é um `refill` depois de cada amostra, como no `MachineTraceTest`.
mod common;

use machinery_basis_core::chemical::{ChemicalConversion, ChemicalResource};
use machinery_basis_core::containers::InventorySlot;
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::upgrade::UpgradeSaved;
use mekanism_rules::{
    ENERGY_UPGRADE, INFUSER_USAGE, SPEED_UPGRADE, infuser_config, infusing_cache_of,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::Path;

/// `Trace.refillTicks` do cenário: o cabo devolve energia para tantos ticks.
const REFILL_TICKS: i64 = 3;
/// `GeneralConfig.maxUpgradeMultiplier` dos cenários de mais de uma operação por tick (`OPS_MULTIPLIER` de `ConstantUsageTraceTest`). O padrão
/// é 10, e com ele o Infuser nunca passa de `200 / 10 = 20` ticks: `UpgradeUtils.getOperationsPerTick` só passa de 1 com `ticksD < 1`.
const OPS_MULTIPLIER: f64 = 1000.0;
const RECIPE_FILE: &str = "dev/referencia/testes/dados/data/mekanismtests/recipe/metallurgic_infusing/gravel_to_flint_per_tick.json";

thread_local! {
    /// O multiplicador do cenário que a thread do teste repete: cada teste roda na sua.
    static MULTIPLIER: Cell<f64> = const { Cell::new(10.0) };
}

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A receita do arquivo de teste, com o id que o jogo lhe daria.
fn recipe_file() -> Value {
    let text = std::fs::read_to_string(root().join(RECIPE_FILE)).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Um Infuser vazio, com a receita de uso por tick e nenhuma outra: gravel só casa com ela nos dois mundos.
fn infuser() -> ProgressMachine {
    let recipe = common::infusing_recipe(
        "mekanismtests:metallurgic_infusing/gravel_to_flint_per_tick",
        &recipe_file(),
    );
    assert!(
        recipe.per_tick_usage,
        "o arquivo de teste tem de ser de uso por tick"
    );
    let mut config = infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(vec![recipe]),
        ChemicalConversion::cache_of(vec![]),
        0,
        common::upgrade_data(),
    );
    config.energy_conversion = common::energy_conversions();
    config.machine.upgrades.max_upgrade_multiplier = MULTIPLIER.get();
    ProgressMachine::chemical(config)
}

/// A máquina posta por um jogador, com os upgrades que o item traz.
fn placed(speed: i32, energy: i32) -> ProgressMachine {
    let mut machine = infuser();
    let mut item = UpgradeSaved::default();
    if speed > 0 {
        item.upgrades.insert(SPEED_UPGRADE.to_owned(), speed);
    }
    if energy > 0 {
        item.upgrades.insert(ENERGY_UPGRADE.to_owned(), energy);
    }
    machine.apply_item_components(&item);
    machine
}

fn set_tank(machine: &ProgressMachine, id: &str, amount: u64) {
    let resource = (amount > 0).then(|| ChemicalResource::new(id));
    machine
        .chemical_side()
        .unwrap()
        .tank
        .set_contents(resource, amount, None);
}

/// `TileComponentUpgrade.addUpgrades` até o total, ou `removeUpgrade` de um em um até ele.
fn set_upgrades(machine: &ProgressMachine, id: &str, total: i32) {
    let upgrades = machine.upgrades();
    let installed = upgrades.get_upgrades(id);
    if total > installed {
        upgrades.add_upgrades(id, total - installed);
    }
    while upgrades.get_upgrades(id) > total {
        upgrades.remove_upgrade(id, false);
    }
}

/// O chunk que descarrega e carrega: o objeto nasce de novo do que o Java persiste (`saveWithFullMetadata` e `loadStatic`), com os
/// itens, o tanque e a energia que os contêineres guardam. Sem `used_so_far`, é um mundo gravado antes de o tile guardá-lo
/// (`getIntOr` dá 0).
fn reload(old: &ProgressMachine, without_usage: bool) -> ProgressMachine {
    let mut saved = old.saved_state();
    if without_usage {
        saved.used_so_far = 0;
    }
    let mut fresh = infuser();
    fresh.restore(&saved, old.tile().block_active());
    for (from, to) in [
        (old.input(), fresh.input()),
        (old.output(), fresh.output()),
        (
            &old.chemical_side().unwrap().slot,
            &fresh.chemical_side().unwrap().slot,
        ),
        (old.energy_slot(), fresh.energy_slot()),
    ] {
        let contents = from.contents();
        to.set_contents(contents.resource, contents.amount, None);
    }
    let tank = old.chemical_side().unwrap().tank.contents();
    fresh
        .chemical_side()
        .unwrap()
        .tank
        .set_contents(tank.resource, tank.amount, None);
    fresh.energy().set_energy(old.energy().amount(), None);
    fresh
}

/// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
fn apply(machine: &mut ProgressMachine, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    match parts.as_slice() {
        ["item", id, amount] => {
            let amount: u32 = amount.parse().unwrap();
            let resource = (amount > 0).then(|| common::item(id));
            machine.input().set_contents(resource, amount, None);
        }
        ["tank", id, amount] => set_tank(machine, id, amount.parse().unwrap()),
        ["speed", total] => set_upgrades(machine, SPEED_UPGRADE, total.parse().unwrap()),
        ["energy", total] => set_upgrades(machine, ENERGY_UPGRADE, total.parse().unwrap()),
        ["reload"] => *machine = reload(machine, false),
        ["reload-without-usage"] => *machine = reload(machine, true),
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn slot(slot: &InventorySlot) -> Value {
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
    json!([id, slot.amount()])
}

fn state(expected: &Value, machine: &ProgressMachine) -> Value {
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
        (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
    ];
    let errors: Vec<&str> = names
        .iter()
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| *name)
        .collect();
    let side = machine.chemical_side().unwrap();
    json!({
        "t": expected["t"],
        "marks": expected["marks"],
        "progress": machine.recipe_progress(),
        "required": machine.ticks_required(),
        "energy": machine.energy().amount(),
        "capacity": machine.energy().capacity(),
        "per_tick": machine.energy().energy_per_tick(),
        "active": machine.tile().active(),
        "block_active": machine.tile().block_active(),
        "powered": machine.tile().is_powered(),
        "comparator": machine.tile().current_redstone_level(),
        "input": slot(machine.input()),
        "output": slot(machine.output()),
        "errors": errors,
        "tank": side.tank.amount(),
        "infusion": slot(&side.slot),
        "used_so_far": machine.saved_scalars().used_so_far,
        "max_operations": machine.operations_per_tick(),
    })
}

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = root().join(format!("dev/referencia/rastros/{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Repete o cenário e devolve quantas marcas viu e quantos ticks teve a máquina com o químico acabado (`active` falso com o item
/// na entrada e o tanque vazio), para o teste conferir que o rastro tem o efeito que diz ter.
fn replay(scenario: &str, speed: i32, energy: i32, minimum_lines: usize) -> (usize, Vec<Value>) {
    replay_with(scenario, speed, energy, 10.0, minimum_lines)
}

/// O mesmo, com `maxUpgradeMultiplier` diferente do padrão.
fn replay_with(
    scenario: &str,
    speed: i32,
    energy: i32,
    multiplier: f64,
    minimum_lines: usize,
) -> (usize, Vec<Value>) {
    MULTIPLIER.set(multiplier);
    let lines = read_trace(scenario);
    assert!(
        lines.len() >= minimum_lines,
        "{scenario}: rastro curto demais: {}",
        lines.len()
    );
    let mut machine = placed(speed, energy);
    let mut seen_marks = 0;
    for expected in &lines {
        machine.tick_server(expected["t"].as_u64().unwrap(), &NoNeighbours);
        for mark in expected["marks"].as_array().unwrap() {
            seen_marks += 1;
            for action in mark.as_str().unwrap().split(", ") {
                apply(&mut machine, action);
            }
        }
        assert_eq!(
            &state(expected, &machine),
            expected,
            "{scenario}: tick {}",
            expected["t"]
        );
        // `Trace.refill`, depois da amostra: o cabo devolve energia para o tick seguinte.
        machine.energy().set_energy(
            REFILL_TICKS * i64::from(machine.energy().energy_per_tick()),
            None,
        );
    }
    (seen_marks, lines)
}

/// Quantos ticks o rastro tem em que `field` vale `value`.
fn count(lines: &[Value], field: &str, value: &Value) -> usize {
    lines.iter().filter(|line| &line[field] == value).count()
}

/// Duas operações inteiras com químico sobrando, e uma terceira que seca no meio e volta: o gasto é `amount` vezes o multiplicador
/// por tick (2 por tick), enquanto o `usedSoFar` conta só o multiplicador (1 por tick).
#[test]
fn the_infuser_spends_the_chemical_every_tick_like_the_original() {
    let (marks, lines) = replay("infuser_const_run", 0, 0, 700);
    assert_eq!(marks, 3, "as marcas do cenário");
    let flint = |line: &Value| line["output"][1].as_u64().unwrap();
    assert_eq!(flint(lines.last().unwrap()), 3, "três operações inteiras");
    assert!(
        lines
            .iter()
            .any(|line| line["used_so_far"] == 100 && line["tank"] == 0 && line["active"] == false),
        "o tanque seca a meio da operação"
    );
}

/// O químico acaba no meio, volta, é menos que a quantidade da receita, é outro químico, e a entrada some e volta.
#[test]
fn the_infuser_pauses_when_the_chemical_runs_out_and_goes_on_when_it_comes_back() {
    let (marks, lines) = replay("infuser_const_dry", 0, 0, 700);
    assert_eq!(marks, 9, "as marcas do cenário");
    assert!(count(&lines, "active", &json!(false)) > 100, "as pausas");
}

/// Upgrades de velocidade e de energia instalados e removidos no meio da operação: o `ticksRequired` muda e o total de uso do
/// multiplicador (`() -> ticksRequired`) muda junto, com o `usedSoFar` do que já foi gasto.
#[test]
fn the_infuser_changes_its_usage_with_the_upgrades_installed_in_the_middle_of_the_operation() {
    let (marks, lines) = replay("infuser_const_speed", 0, 0, 600);
    assert_eq!(marks, 10, "as marcas do cenário");
    let required: std::collections::BTreeSet<i64> = lines
        .iter()
        .map(|line| line["required"].as_i64().unwrap())
        .collect();
    assert!(
        required.len() >= 5,
        "ticks por operação vistos: {required:?}"
    );
}

/// A máquina posta com 4 de velocidade e 1 de energia no item.
#[test]
fn a_machine_placed_with_upgrades_in_the_item_uses_the_chemical_for_its_own_ticks() {
    let (marks, lines) = replay("infuser_const_placed_speed", 4, 1, 300);
    assert_eq!(marks, 2, "as marcas do cenário");
    assert!(lines.last().unwrap()["output"][1].as_u64().unwrap() >= 2);
}

/// Salvar e reabrir no meio da operação, com upgrades e com a receita parada: o `usedSoFar` volta do NBT e o monitor o entrega
/// à receita nova (`loadSavedData`).
#[test]
fn the_used_so_far_survives_saving_and_loading_the_tile() {
    let (marks, lines) = replay("infuser_const_reload", 0, 0, 600);
    assert_eq!(marks, 8, "as marcas do cenário");
    assert!(
        lines
            .iter()
            .any(|line| line["marks"] == json!(["reload"])
                && line["used_so_far"].as_i64().unwrap() > 0),
        "recarregar com o gasto no meio"
    );
}

/// Salvo com um progresso que já não cabe nos upgrades que voltam: o `loadSavedOperatingTicks` o recusa (`operatingTicks <
/// requiredTicks`) e o `loadSavedUsageSoFar` aceita o gasto, então o multiplicador (`baseRemaining < remainingTicks`) dá zero e a
/// operação termina sem gastar o químico. É assim no original e fica assim.
#[test]
fn a_saved_progress_that_no_longer_fits_the_upgrades_leaves_the_loaded_usage_stale() {
    let (marks, lines) = replay("infuser_const_reload_speed", 0, 0, 300);
    assert_eq!(marks, 5, "as marcas do cenário");
    // O efeito: de t=106 a t=124 o progresso anda 19 ticks e o tanque não gasta nada.
    let window = &lines[106..=124];
    assert!(
        window
            .iter()
            .all(|line| line["tank"] == 800 && line["used_so_far"] == 100)
    );
    assert_eq!(window[18]["progress"], 19);
}

/// Um mundo com o progresso e sem o `used_so_far`: o `loadSavedUsageSoFar` não carrega nada, o multiplicador (`baseRemaining /
/// remainingTicks`) passa a 2 por tick e o gasto alcança o total, com o tanque caindo 4 por tick (a quantidade da receita vezes 2).
#[test]
fn a_save_without_the_used_so_far_catches_up_with_more_than_one_per_tick() {
    let (marks, lines) = replay("infuser_const_reload_without_usage", 0, 0, 400);
    assert_eq!(marks, 3, "as marcas do cenário");
    assert_eq!(lines[106]["tank"], 796);
    assert_eq!(lines[106]["used_so_far"], 2);
    assert_eq!(lines[205]["output"], json!(["flint", 1]));
}

/// Com `maxUpgradeMultiplier` 1000, 7 de velocidade dá `ticksD = 200 * 1000^(-7/8) = 0,47` e `getOperationsPerTick` 2, e 8 dá 0,2 e 5
/// (`UpgradeUtils.getOperationsPerTick`, `max(1, 1 / ticksD)`); a receita de `per_tick_usage` gasta então `amount * operações * multiplicador`
/// por tick (`ItemStackConstantChemicalToObjectCachedRecipe.useResources`) e o tanque cai de acordo. O padrão do original (10) nunca chega
/// aqui, e o port não lê a configuração (`UPGRADE_MULTIPLIER` é uma constante): a prova em jogo não alcança este caso.
#[test]
fn the_infuser_with_more_than_one_operation_per_tick_spends_the_chemical_per_operation() {
    let (marks, lines) = replay_with("infuser_const_ops", 0, 0, OPS_MULTIPLIER, 400);
    assert_eq!(marks, 11, "as marcas do cenário");
    let operations: std::collections::BTreeSet<i64> = lines
        .iter()
        .map(|line| line["max_operations"].as_i64().unwrap())
        .collect();
    assert_eq!(operations, [1, 2, 5].into(), "operações por tick vistas");
    assert!(
        lines
            .iter()
            .any(|line| line["max_operations"] == 5 && line["required"] == 1),
        "o ticksRequired chega a 1"
    );
}

/// A máquina posta com 8 de velocidade e 2 de energia no item (5 operações por tick), com o químico acabando no meio das operações, e
/// depois com 7 de velocidade (2 por tick), e salva e relida.
#[test]
fn a_machine_placed_with_many_speed_upgrades_runs_five_operations_per_tick() {
    let (marks, lines) = replay_with("infuser_const_ops_placed", 8, 2, OPS_MULTIPLIER, 200);
    assert_eq!(marks, 5, "as marcas do cenário");
    assert!(
        lines.iter().any(|line| line["max_operations"] == 5),
        "5 operações por tick"
    );
    assert!(
        lines.iter().any(|line| line["max_operations"] == 2),
        "2 operações por tick depois de tirar um upgrade"
    );
}
