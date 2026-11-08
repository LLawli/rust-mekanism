//! Repete, contra o original, os avisos de entrada que não basta, o overlay da barra de energia e o PULSE
//! (`dev/referencia/testes/WarningsTraceTest.java`).
//!
//! O que o Java mostrou: `NOT_ENOUGH_INPUT` e `NOT_ENOUGH_SECONDARY_INPUT` só nascem de `InputResourceHandler.calculateOperationsCanSupport`,
//! e a entrada que não basta para a receita nem chega lá (`getRecipeInput` devolve vazio e o `CachedRecipe` marca `mismatchedRecipe`, que
//! limpa os erros). Os rastros `warnings_*` gravam o que isso dá na tela (nenhum aviso de entrada), e a varredura repete todas as receitas
//! de cada máquina com a entrada abaixo, na quantidade e acima do que a receita pede.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalResource};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::gui::{bar_warning, divide_to_level};
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{
    CHEMICAL_MACHINE_INPUT_ERRORS, ProgressMachine, RedstoneControl, SavedState,
};
use machinery_basis_core::upgrade::UpgradeSaved;
use machinery_basis_core::upgrade_window::UpgradeSync;
use mekanism_rules::{
    CRUSHER_USAGE, DataLayout, ELECTRIC_DATA, ELECTRIC_TRACKED_ERRORS, ELECTRIC_WARNINGS,
    ENRICHMENT_USAGE, INFUSER_DATA, INFUSER_TRACKED_ERRORS, INFUSER_USAGE, INFUSER_WARNINGS,
    SMELTER_USAGE, SPEED_UPGRADE, ScreenData, ScreenElement, ScreenWarnings, SyncedMachine,
    TileScreen, WarningType, electric_config, infuser_config, infusing_cache_of, machine_warnings,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const STACK: u32 = 64;
const SPEED: &str = "mekanism:speed";
/// O retângulo da barra de energia (`GuiVerticalPowerBar` em (164, 16), 4 de largura e 52 de altura, mais a moldura).
const ENERGY_BAR: [f32; 4] = [164.0, 16.0, 6.0, 54.0];

/// A máquina, o `game_time` do mundo em que o rastro foi gravado (o recheck de erros depende de `game_time % 100`), e os data slots da
/// tela com o desenho deles: o que o port escreve no `ServerTickEnd` e o `broadcastChanges` do original manda ao cliente.
struct Sim(ProgressMachine, u64, TileScreen, &'static DataLayout);

impl SimBlockEntity for Sim {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time + self.1, neighbours);
        self.2.machine_ticked(UpgradeSync::of(self.0.upgrades()));
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }

    /// `ServerTickEvent.Post`: a energia que o tile guarda agora, a de dentro do objeto na simulação.
    fn server_tick_end(&mut self) {
        let screen = ScreenData::of_machine(&self.0, self.0.energy().amount());
        self.2.server_tick_end(self.3, screen, false);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Chamber,
    Crusher,
    Smelter,
    Infuser,
}

impl Kind {
    fn chemical(self) -> bool {
        self == Self::Infuser
    }

    fn tracked(self) -> &'static [RecipeError] {
        if self.chemical() {
            &INFUSER_TRACKED_ERRORS
        } else {
            &ELECTRIC_TRACKED_ERRORS
        }
    }

    fn warnings(self) -> &'static ScreenWarnings {
        if self.chemical() {
            &INFUSER_WARNINGS
        } else {
            &ELECTRIC_WARNINGS
        }
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(
        id.trim_start_matches("minecraft:")
            .trim_start_matches("mekanism:"),
        STACK,
    )
}

/// A máquina do rastro: as receitas dos arquivos do port (a câmara e o triturador) ou as que o rastro da varredura descreve (o forno
/// usa as receitas de cozimento do jogo, que não estão nos arquivos do mod).
fn world(kind: Kind, offset: u64, base: u64, smelting: Vec<Rc<ItemToItemRecipe>>) -> SimWorld<Sim> {
    let machine = match kind {
        Kind::Chamber | Kind::Crusher | Kind::Smelter => {
            let (usage, recipes) = match kind {
                Kind::Chamber => (
                    ENRICHMENT_USAGE,
                    common::item_recipes_of_type("mekanism:enriching"),
                ),
                Kind::Crusher => (
                    CRUSHER_USAGE,
                    common::item_recipes_of_type("mekanism:crushing"),
                ),
                _ => (SMELTER_USAGE, smelting),
            };
            let mut config = electric_config(
                usage,
                ItemToItemRecipe::cache_of(recipes),
                offset,
                common::upgrade_data(),
            );
            config.energy_conversion = common::energy_conversions();
            ProgressMachine::electric(config)
        }
        Kind::Infuser => {
            let (recipes, conversions) = common::infuser_recipes_by_type();
            let mut config = infuser_config(
                INFUSER_USAGE,
                infusing_cache_of(recipes),
                ChemicalConversion::cache_of(conversions),
                offset,
                common::upgrade_data(),
            );
            config.energy_conversion = common::energy_conversions();
            ProgressMachine::chemical(config)
        }
    };
    // Como um jogador a põe (`applyImplicitComponents` recalcula os upgrades, e com eles a capacidade de energia).
    let mut machine = machine;
    machine.apply_item_components(&UpgradeSaved::default());
    let layout = if kind.chemical() {
        &INFUSER_DATA
    } else {
        &ELECTRIC_DATA
    };
    SimWorld::new(Sim(machine, base, TileScreen::default(), layout))
}

fn install(machine: &ProgressMachine, total: i32) {
    let upgrades = machine.upgrades();
    upgrades.add_upgrades(SPEED_UPGRADE, total - upgrades.get_upgrades(SPEED_UPGRADE));
}

// ------------------------------------------------------------------------------------------------------------ rastros tick a tick

struct Scene {
    world: SimWorld<Sim>,
    refill_ticks: i32,
    players: u64,
}

impl Scene {
    fn machine(&self) -> &ProgressMachine {
        &self.world.block_entity.0
    }

    /// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
    fn apply(&mut self, action: &str) {
        let parts: Vec<&str> = action.split_whitespace().collect();
        let machine = &self.world.block_entity.0;
        match parts.as_slice() {
            [id @ ("raw_iron" | "clay"), amount] => {
                machine
                    .input()
                    .set_contents(Some(item(id)), amount.parse().unwrap(), None);
            }
            ["item", id, amount] => {
                let amount: u32 = amount.parse().unwrap();
                machine
                    .input()
                    .set_contents((amount > 0).then(|| item(id)), amount, None);
            }
            ["tank", id, amount] => {
                let amount: u64 = amount.parse().unwrap();
                let resource = (amount > 0).then(|| ChemicalResource::new(id));
                machine
                    .chemical_side()
                    .unwrap()
                    .tank
                    .set_contents(resource, amount, None);
            }
            [
                "energy",
                "topped",
                "up",
                "to",
                ticks,
                "ticks",
                "every",
                "tick",
            ] => {
                self.refill_ticks = ticks.parse().unwrap();
            }
            ["energy", amount] => machine.energy().set_energy(amount.parse().unwrap(), None),
            ["speed", amount] => install(machine, amount.parse().unwrap()),
            ["remove", "input"] => machine.input().set_contents(None, 0, None),
            ["remove", "speed"] => machine.upgrades().remove_upgrade(SPEED, false),
            ["remove", "speed", "all"] => machine.upgrades().remove_upgrade(SPEED, true),
            // O jogador entra pelo campo `players` do rastro.
            ["player", "opens" | "closes"] => {}
            other => panic!("marca desconhecida: {other:?}"),
        }
    }

    /// `Trace.refill`: depois de gravar o tick, a energia volta a tantos ticks de consumo.
    fn refill(&self) {
        if self.refill_ticks > 0 {
            let energy = self.machine().energy();
            energy.set_energy(
                i64::from(self.refill_ticks) * i64::from(energy.energy_per_tick()),
                None,
            );
        }
    }
}

fn slot(slot: &InventorySlot) -> Value {
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
    serde_json::json!([id, slot.amount()])
}

fn error_names(machine: &ProgressMachine) -> Value {
    [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
        (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
    ]
    .iter()
    .filter(|(error, _)| machine.has_error(*error))
    .map(|(_, name)| Value::from(*name))
    .collect()
}

fn names(kinds: Vec<WarningType>) -> Value {
    kinds
        .into_iter()
        .map(|kind| Value::from(kind.name()))
        .collect()
}

/// O overlay da barra de energia (`GuiBar.drawBackground`): o aviso de energia aceso na barra, e a barra com o nível que a lib dá a `bar_warning`.
fn bar_overlay(client: &SyncedMachine, energy: i64, capacity: i64) -> bool {
    let warning = client
        .errors
        .has(client.tracked, RecipeError::NOT_ENOUGH_ENERGY);
    let level = divide_to_level(energy as f64, capacity as f64);
    warning && level > 0.0 && bar_warning(ENERGY_BAR, level, false).overlay.is_some()
}

fn replay(scenario: &str, kind: Kind) -> Vec<Value> {
    let lines = read_trace(scenario);
    assert!(lines.len() >= 200, "rastro curto demais: {}", lines.len());
    let offset = lines[0]["recheck_offset"].as_u64().unwrap();
    let base = lines[0]["game_time"].as_u64().unwrap() - lines[0]["t"].as_u64().unwrap();
    let mut scene = Scene {
        world: world(kind, offset, base, Vec::new()),
        refill_ticks: 0,
        players: 0,
    };
    for expected in &lines {
        let mut expected = expected.clone();
        let object = expected.as_object_mut().unwrap();
        let mut recorded = std::collections::BTreeMap::new();
        for key in [
            "game_time",
            "players",
            "recheck",
            "recheck_offset",
            "mode",
            "warnings",
            "warnings_shown",
            "bar_overlay",
        ] {
            recorded.insert(
                key,
                object
                    .remove(key)
                    .unwrap_or_else(|| panic!("{scenario}: sem {key}")),
            );
        }
        // O tick do nível, o que o cenário fez depois dele (uma chegada de energia, um item) e o `ServerTickEvent.Post`, onde o port escreve
        // os data slots que o cliente recebe no tick seguinte.
        scene.world.tick_level();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                if action == "player opens" {
                    scene.machine().open();
                    scene.players += 1;
                } else if action == "player closes" {
                    scene.machine().close();
                    scene.players -= 1;
                }
                scene.apply(action);
            }
        }
        scene.world.finish_tick();
        let t = expected["t"].as_u64().unwrap();
        let machine = scene.machine();
        let mut actual = serde_json::json!({
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
            "errors": error_names(machine),
        });
        if kind.chemical() {
            let side = machine.chemical_side().unwrap();
            actual["tank"] = side.tank.amount().into();
            actual["infusion"] = slot(&side.slot);
        }
        assert_eq!(actual, expected, "{scenario}, tick {t}");
        assert_eq!(
            recorded["game_time"],
            base + t,
            "{scenario}: o relógio anda um por tick"
        );
        assert_eq!(
            recorded["players"], scene.players,
            "{scenario}, tick {t}: jogadores"
        );
        assert_eq!(recorded["recheck_offset"], offset);
        assert_eq!(
            recorded["recheck"],
            machine.recheck_all_errors(),
            "{scenario}, tick {t}: recheck"
        );
        assert_eq!(
            recorded["mode"],
            format!("{:?}", machine.tile().control_type()).to_lowercase(),
            "{scenario}, tick {t}: modo"
        );
        // O que chega ao cliente é o quadro do `ServerTickEnd` depois dos 16 bits com sinal de cada data slot: a energia, a capacidade e os erros
        // desse tick, e não os do tick da máquina.
        let sim = &scene.world.block_entity;
        let wire: Vec<i32> = sim
            .2
            .frame()
            .iter()
            .map(|&value| i32::from(value as i16))
            .collect();
        let screen = ScreenData::read(sim.3, &wire);
        let client = SyncedMachine {
            tracked: kind.tracked(),
            errors: screen.errors,
            redstone: screen.redstone,
        };
        assert_eq!(
            i64::from(screen.energy),
            machine.energy().amount(),
            "{scenario}, tick {t}: a energia do quadro do cliente"
        );
        for (side, tracker) in [
            ("servidor", machine_warnings(machine, kind.warnings())),
            ("cliente", machine_warnings(&client, kind.warnings())),
        ] {
            assert_eq!(
                recorded["warnings"],
                names(tracker.triggered()),
                "{scenario}, tick {t}: warnings ({side})"
            );
            assert_eq!(
                recorded["warnings_shown"],
                names(tracker.warnings()),
                "{scenario}, tick {t}: warnings_shown ({side})"
            );
        }
        assert_eq!(
            recorded["bar_overlay"],
            bar_overlay(
                &client,
                i64::from(screen.energy),
                i64::from(screen.capacity)
            ),
            "{scenario}, tick {t}: overlay da barra de energia"
        );
        scene.refill();
    }
    lines
}

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Quantas linhas do rastro têm o aviso (na lista de avisos ativos) ou o erro.
fn count_with(lines: &[Value], field: &str, wanted: &str) -> usize {
    lines
        .iter()
        .filter(|line| line[field].as_array().unwrap().iter().any(|v| v == wanted))
        .count()
}

/// A câmara com a entrada que não basta (3 de ferro bruto): chega aos poucos, some no meio da operação, volta com upgrades de velocidade e sem
/// jogador na tela. Nenhum tick tem erro de entrada, nem o aviso `no_matching_recipe`, e a receita só anda com as 3.
#[test]
fn the_chamber_with_less_input_than_the_recipe_shows_no_warning() {
    let lines = replay("warnings_input_chamber", Kind::Chamber);
    assert_eq!(count_with(&lines, "errors", "input"), 0);
    assert_eq!(count_with(&lines, "warnings", "no_matching_recipe"), 0);
    // O rastro tem de ter o efeito: a máquina andou com 3 e parou ao baixar para 2.
    assert!(
        lines
            .iter()
            .any(|line| line["progress"].as_i64().unwrap() > 20)
    );
    assert!(
        lines
            .iter()
            .any(|line| line["players"].as_u64().unwrap() == 1 && line["recheck"] == true)
    );
}

/// O Infuser com menos químico do que a receita pede (9 de 10), o tanque que baixa no meio da operação, o item que some, e a receita de 3 itens.
#[test]
fn the_infuser_with_less_chemical_than_the_recipe_shows_no_warning() {
    let lines = replay("warnings_input_infuser", Kind::Infuser);
    assert_eq!(count_with(&lines, "errors", "input"), 0);
    assert_eq!(count_with(&lines, "errors", "secondary"), 0);
    assert_eq!(count_with(&lines, "warnings", "no_matching_recipe"), 0);
    assert!(
        lines
            .iter()
            .any(|line| line["progress"].as_i64().unwrap() > 20)
    );
    assert!(lines.iter().any(|line| line["tank"].as_u64().unwrap() == 9));
}

/// O erro de energia que fica numa máquina pausada quando os upgrades de velocidade saem: a energia passa da capacidade (`getCapacityAsLong` é o
/// maior entre o máximo e o guardado), a barra está em 1 e o aviso é desenhado por cima (`level >= 0,98`).
#[test]
fn the_error_left_in_a_paused_machine_draws_the_energy_bar_overlay() {
    let lines = replay("warnings_overlay", Kind::Chamber);
    let with_overlay: Vec<&Value> = lines
        .iter()
        .filter(|line| line["bar_overlay"] == true)
        .collect();
    assert!(!with_overlay.is_empty(), "o rastro nunca chega ao overlay");
    for line in &with_overlay {
        assert!(
            line["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e == "energy")
        );
        assert!(
            line["energy"].as_i64().unwrap() as f64
                >= 0.98 * line["capacity"].as_i64().unwrap() as f64
        );
    }
    // A chegada de energia fora do tick da máquina (t=105: `energy 9999`, depois do tick que acendeu o erro e antes do recálculo): o quadro do
    // `ServerTickEnd` já leva a energia nova com o erro velho, que é o que o `broadcastChanges` do tick seguinte manda ao cliente, e a
    // barra desenha o overlay. No tick seguinte o erro some. O `replay` confere o quadro do cliente (energia, capacidade, erros) nos dois.
    assert_eq!(lines[105]["marks"], serde_json::json!(["energy 9999"]));
    assert_eq!(lines[105]["bar_overlay"], true);
    assert_eq!(lines[106]["bar_overlay"], false);
    // E há ticks com o erro aceso e a barra abaixo do limiar, sem overlay.
    assert!(lines.iter().any(|line| {
        line["bar_overlay"] == false
            && line["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e == "energy")
    }));
}

// ------------------------------------------------------------------------------------------------------------ a varredura

fn sweep_cases(kind: Kind) -> Vec<Value> {
    let name = match kind {
        Kind::Chamber => "enrichment_chamber",
        Kind::Crusher => "crusher",
        Kind::Smelter => "energized_smelter",
        Kind::Infuser => "metallurgic_infuser",
    };
    read_trace(&format!("warnings_sweep_{name}"))
}

/// Uma receita de cozimento como a varredura a descreve: o item escolhido e a quantidade que ela pede.
fn smelting_recipes(cases: &[Value]) -> Vec<Rc<ItemToItemRecipe>> {
    let mut seen = std::collections::BTreeSet::new();
    let mut recipes = Vec::new();
    for case in cases {
        let id = case["recipe"].as_str().unwrap();
        if seen.insert(id.to_owned()) {
            recipes.push(Rc::new(ItemToItemRecipe {
                id: id.to_owned(),
                input: ItemIngredient::item(&item(case["item"].as_str().unwrap()).id),
                input_count: case["need"].as_u64().unwrap() as u32,
                output: ItemOutput {
                    resource: item("air"),
                    count: 1,
                },
            }));
        }
    }
    recipes
}

/// A pergunta "esta receita do port aceita este item?" de cada receita de um tipo que os arquivos do port trazem, pelo id.
type Accepts = Box<dyn Fn(&ItemResource) -> bool>;

fn content(kind: Kind) -> std::collections::BTreeMap<String, Accepts> {
    let mut map = std::collections::BTreeMap::<String, Accepts>::new();
    match kind {
        Kind::Chamber | Kind::Crusher => {
            let name = if kind == Kind::Chamber {
                "mekanism:enriching"
            } else {
                "mekanism:crushing"
            };
            for recipe in common::item_recipes_of_type(name) {
                let id = recipe.id.clone();
                map.insert(id, Box::new(move |item| recipe.input.matches(item)));
            }
        }
        Kind::Smelter => {}
        Kind::Infuser => {
            for recipe in common::infuser_recipes_by_type().0 {
                let id = recipe.id.clone();
                map.insert(id, Box::new(move |item| recipe.item_input.matches(item)));
            }
        }
    }
    map
}

/// O que a varredura de uma máquina deu no original, repetido na máquina do port: nenhum caso tem erro de entrada, e o resto (progresso por
/// tick, erros, atividade) bate nas receitas que os dois lados têm. Devolve quantos casos compararam e quantos o port não tem.
fn run_sweep(kind: Kind) -> (usize, usize) {
    let cases = sweep_cases(kind);
    assert!(
        cases.len() >= 60,
        "{kind:?}: varredura curta, {} casos",
        cases.len()
    );
    let mut world = world(kind, 0, 0, smelting_recipes(&cases));
    let ours = content(kind);
    let (mut compared, mut skipped) = (0, 0);
    let mut speed = 0;
    for case in &cases {
        let recipe = case["recipe"].as_str().unwrap();
        // Nenhum caso do original tem erro de entrada, em nenhuma receita: é a prova de que ele não acende.
        for error in case["errors"].as_array().unwrap() {
            assert!(
                error != "input" && error != "secondary",
                "{recipe}: o original acendeu {error} ({case})"
            );
        }
        let in_port = kind == Kind::Smelter
            || ours
                .get(recipe)
                .is_some_and(|accepts| accepts(&item(case["item"].as_str().unwrap())));
        let wanted_speed = case["speed"].as_i64().unwrap() as i32;
        let machine = &world.block_entity.0;
        if wanted_speed != speed {
            install(machine, wanted_speed);
            speed = wanted_speed;
        }
        let count = case["stacked"].as_u64().unwrap() as u32;
        let id = case["item"].as_str().unwrap();
        machine.input().set_contents(Some(item(id)), count, None);
        if kind.chemical() {
            let chemical = case["chemical"]
                .as_str()
                .unwrap()
                .trim_start_matches("mekanism:");
            let amount = case["amount"].as_u64().unwrap();
            machine.chemical_side().unwrap().tank.set_contents(
                Some(ChemicalResource::new(chemical)),
                amount,
                None,
            );
        }
        machine
            .energy()
            .set_energy(machine.energy().capacity(), None);
        let (mut errors, mut progress, mut active) = (Vec::new(), Vec::new(), Vec::new());
        for step in 1..=5 {
            world.tick();
            let machine = &world.block_entity.0;
            if step <= 3 {
                progress.push(Value::from(machine.recipe_progress()));
                active.push(Value::from(machine.tile().active()));
                for name in error_names(machine).as_array().unwrap() {
                    if !errors.contains(name) {
                        errors.push(name.clone());
                    }
                }
                machine
                    .energy()
                    .set_energy(machine.energy().capacity(), None);
            }
            if step == 3 {
                let machine = &world.block_entity.0;
                machine.input().set_contents(None, 0, None);
                if kind.chemical() {
                    machine
                        .chemical_side()
                        .unwrap()
                        .tank
                        .set_contents(None, 0, None);
                }
                machine.output().set_contents(None, 0, None);
            }
        }
        // As quatro ordens do Java para os erros: a lista é a de `ERRORS`, e o aviso do Infuser pode trazer os dois.
        let order = [
            "energy",
            "input",
            "output_space",
            "wrong_output",
            "secondary",
        ];
        errors.sort_by_key(|name| order.iter().position(|o| name == o));
        assert!(
            !errors.iter().any(|e| e == "input" || e == "secondary"),
            "{recipe}: o port acendeu um erro de entrada ({case})"
        );
        if !in_port {
            skipped += 1;
            continue;
        }
        assert_eq!(
            (
                Value::from(errors),
                Value::from(progress),
                Value::from(active)
            ),
            (
                case["errors"].clone(),
                case["progress"].clone(),
                case["active"].clone()
            ),
            "{recipe}: {case}"
        );
        compared += 1;
    }
    eprintln!("{kind:?}: {compared} casos comparados, {skipped} fora do port");
    (compared, skipped)
}

#[test]
fn no_recipe_of_the_enrichment_chamber_lights_a_missing_input_error() {
    let (compared, skipped) = run_sweep(Kind::Chamber);
    assert!(
        compared >= 600,
        "só {compared} casos comparados ({skipped} fora do port)"
    );
}

#[test]
fn no_recipe_of_the_crusher_lights_a_missing_input_error() {
    let (compared, skipped) = run_sweep(Kind::Crusher);
    assert!(
        compared >= 800,
        "só {compared} casos comparados ({skipped} fora do port)"
    );
}

#[test]
fn no_recipe_of_the_energized_smelter_lights_a_missing_input_error() {
    let (compared, _) = run_sweep(Kind::Smelter);
    assert!(compared >= 100, "só {compared} casos comparados");
}

#[test]
fn no_recipe_of_the_metallurgic_infuser_lights_a_missing_input_error() {
    let (compared, skipped) = run_sweep(Kind::Infuser);
    assert!(
        compared >= 300,
        "só {compared} casos comparados ({skipped} fora do port)"
    );
}

/// As receitas do port que pedem mais de um item na entrada, ou mais químico do que um ciclo gasta: a varredura tem de ter passado por elas,
/// para a prova valer para o que o port tem.
#[test]
fn the_sweeps_went_through_the_recipes_that_ask_for_more_than_one_item() {
    for (kind, minimum) in [(Kind::Chamber, 9), (Kind::Crusher, 6), (Kind::Infuser, 3)] {
        let cases = sweep_cases(kind);
        let ours = content(kind);
        let mut multi = std::collections::BTreeSet::new();
        for case in &cases {
            if case["need"].as_u64().unwrap() > 1
                && ours.contains_key(case["recipe"].as_str().unwrap())
            {
                // Abaixo, na quantidade e acima do que a receita pede.
                multi.insert(case["recipe"].as_str().unwrap().to_owned());
            }
        }
        assert!(
            multi.len() >= minimum,
            "{kind:?}: só {} receitas de mais de um item",
            multi.len()
        );
    }
}

// ------------------------------------------------------------------------------------------------------------ PULSE

/// PULSE é recusado pelas quatro máquinas portadas, nos três caminhos do Java (`setControlType`, o NBT salvo e o componente do item):
/// `ITileRedstone.supportsMode` devolve `mode != PULSE`, e `supportedOrNextType` troca o pedido por `getNext`, que dá `DISABLED`.
#[test]
fn the_ported_machines_refuse_pulse_as_the_original_does() {
    let lines = read_trace("redstone_pulse");
    assert!(lines.len() >= 5 * 8, "rastro curto: {}", lines.len());
    let machines = [
        "enrichment_chamber",
        "crusher",
        "energized_smelter",
        "metallurgic_infuser",
    ];
    let mut compared = 0;
    for line in &lines {
        let name = line["machine"].as_str().unwrap();
        // O original: quem aceita o PULSE é que o rastro tem de provar que não existe nas cinco.
        assert_eq!(line["supports_redstone"], true, "{line}");
        assert_eq!(line["supports_pulse"], false, "{line}");
        assert_eq!(line["supports_high"], true, "{line}");
        assert_eq!(line["mode"], "disabled", "{line}");
        if !machines.contains(&name) {
            continue;
        }
        let kind = match name {
            "enrichment_chamber" => Kind::Chamber,
            "crusher" => Kind::Crusher,
            "energized_smelter" => Kind::Smelter,
            _ => Kind::Infuser,
        };
        let machine = world(kind, 0, 0, Vec::new()).block_entity.0;
        let before = match line["before"].as_str().unwrap() {
            "" | "pulse" => RedstoneControl::Disabled,
            other => RedstoneControl::from_serialized_name(other),
        };
        machine.tile_mut().set_control_type(before);
        match line["route"].as_str().unwrap() {
            "set" | "component" => machine.tile_mut().set_control_type(RedstoneControl::Pulse),
            "tag" => {
                let saved = SavedState {
                    control_type: RedstoneControl::from_serialized_name("pulse"),
                    ..SavedState::default()
                };
                machine.tile_mut().restore(&saved, false);
            }
            "placed" => {}
            other => panic!("caminho {other}"),
        }
        assert_eq!(
            format!("{:?}", machine.tile().control_type()).to_lowercase(),
            line["mode"].as_str().unwrap(),
            "{line}"
        );
        compared += 1;
    }
    assert!(compared >= 4 * 8, "só {compared} linhas comparadas");
}

/// O Infuser cruza os erros de entrada (`TileEntityMetallurgicInfuser`): o handler do item dá `NOT_ENOUGH_SECONDARY_INPUT` e o do químico
/// `NOT_ENOUGH_INPUT`, e a tela os usa ao contrário do que o nome diz, a barra do químico pelo erro do item e o slot de entrada pelo do químico.
#[test]
fn the_infuser_crosses_its_input_errors_as_the_original_does() {
    assert_eq!(
        CHEMICAL_MACHINE_INPUT_ERRORS.item,
        RecipeError::NOT_ENOUGH_SECONDARY_INPUT
    );
    assert_eq!(
        CHEMICAL_MACHINE_INPUT_ERRORS.chemical,
        RecipeError::NOT_ENOUGH_INPUT
    );
    let error_of = |element: ScreenElement| {
        INFUSER_WARNINGS
            .iter()
            .find(|(candidate, ..)| *candidate == element)
            .map(|(_, kind, error)| (*kind, *error))
            .unwrap()
    };
    assert_eq!(
        error_of(ScreenElement::ChemicalBar),
        (
            WarningType::NoMatchingRecipe,
            CHEMICAL_MACHINE_INPUT_ERRORS.item
        )
    );
    assert_eq!(
        error_of(ScreenElement::InputSlot),
        (
            WarningType::NoMatchingRecipe,
            CHEMICAL_MACHINE_INPUT_ERRORS.chemical
        )
    );
}
