//! Repete, tick a tick, os rastros gravados da Enrichment Chamber original em `dev/referencia/rastros`.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::item_handler::{
    ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer,
};
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{ProgressMachine, RedstoneControl};
use machinery_basis_core::warning::ErrorMask;
use mekanism_rules::{
    CRUSHER_USAGE, ELECTRIC_TRACKED_ERRORS, ELECTRIC_WARNINGS, ENERGY_UPGRADE, ENRICHMENT_USAGE,
    SMELTER_USAGE, SPEED_UPGRADE, SyncedMachine, electric_config, machine_warnings, trace_line,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 70;

/// O baú do cenário de ejeção: 27 slots, como o `ChestBlockEntity`.
const CHEST_SLOTS: usize = 27;
/// `AbstractFurnaceBlockEntity.SLOT_FUEL`.
const FURNACE_FUEL_SLOT: usize = 1;

/// A máquina e o `game_time` do mundo em que o rastro foi gravado: o `SimWorld` conta de zero, e o recheck de erros depende de
/// `game_time % 100`, então o relógio do jogo (o do primeiro tick do rastro) entra somado ao do mundo simulado.
struct Chamber(ProgressMachine, u64);

impl SimBlockEntity for Chamber {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time + self.1, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn recipe(input: &str, input_count: u32, output: &str, count: u32) -> Rc<ItemToItemRecipe> {
    Rc::new(ItemToItemRecipe {
        id: format!("mekanism:enriching/{output}"),
        input: ItemIngredient::item(input),
        input_count,
        output: ItemOutput {
            resource: item(output),
            count,
        },
    })
}

/// A câmara e o baú ao lado dela, a leste, que só o cenário de ejeção usa.
struct Scene {
    world: SimWorld<Chamber>,
    chest: Rc<VanillaContainer>,
    /// O número do primeiro slot que o lado alcança: o `1` do combustível, no forno.
    first_slot: usize,
    /// O `refillTicks` e o `refillFraction` do `Trace` em Java: depois de cada tick gravado, a energia volta a tantos ticks de
    /// consumo, ou a uma fração de um tick. Zero nos dois deixa a energia como está.
    refill_ticks: i32,
    refill_fraction: i32,
}

impl Scene {
    /// `Trace.refill`.
    fn refill(&self) {
        let energy = self.world.block_entity.0.energy();
        let per_tick = energy.energy_per_tick();
        if self.refill_ticks > 0 {
            energy.set_energy(i64::from(self.refill_ticks) * i64::from(per_tick), None);
        } else if self.refill_fraction > 0 {
            energy.set_energy(i64::from(per_tick / self.refill_fraction), None);
        }
    }
}

fn scene() -> Scene {
    let chest = VanillaContainer::new(CHEST_SLOTS, Rc::new(|| {}));
    let mut world = chamber();
    world.neighbours.set(Face::East, chest.clone());
    Scene {
        world,
        chest,
        first_slot: 0,
        refill_ticks: 0,
        refill_fraction: 0,
    }
}

/// O forno pela lateral: só o slot de combustível, e só o que queima.
fn furnace_scene() -> Scene {
    let fuel = VanillaContainer::new(1, Rc::new(|| {}));
    let mut world = chamber();
    let burns = |_: usize, resource: &ItemResource| matches!(&*resource.id, "coal" | "charcoal");
    world
        .neighbours
        .set(Face::East, Rc::new(RuledHandler::new(fuel.clone(), burns)));
    Scene {
        world,
        chest: fuel,
        first_slot: FURNACE_FUEL_SLOT,
        refill_ticks: 0,
        refill_fraction: 0,
    }
}

fn machine(energy_per_tick: i32, recipes: Vec<Rc<ItemToItemRecipe>>) -> SimWorld<Chamber> {
    machine_at(energy_per_tick, recipes, 0, 0)
}

/// `check_offset` é o sorteio de `shouldRecheckAllErrors`, e `game_time` o relógio do jogo no primeiro tick.
fn machine_at(
    energy_per_tick: i32,
    recipes: Vec<Rc<ItemToItemRecipe>>,
    check_offset: u64,
    game_time: u64,
) -> SimWorld<Chamber> {
    SimWorld::new(Chamber(
        ProgressMachine::electric(electric_config(
            energy_per_tick,
            ItemToItemRecipe::cache_of(recipes),
            check_offset,
            common::upgrade_data(),
        )),
        game_time,
    ))
}

fn enrichment_recipes() -> Vec<Rc<ItemToItemRecipe>> {
    vec![
        recipe("clay", 1, "clay_ball", 4),
        recipe("stone", 1, "cracked_stone_bricks", 1),
        recipe("hdpe_pellet", 3, "hdpe_sheet", 1),
    ]
}

fn chamber() -> SimWorld<Chamber> {
    machine(ENRICHMENT_USAGE, enrichment_recipes())
}

/// A câmara com o sorteio e o relógio que o rastro gravou.
fn rechecking_scene(check_offset: u64, game_time: u64) -> Scene {
    alone(machine_at(
        ENRICHMENT_USAGE,
        enrichment_recipes(),
        check_offset,
        game_time,
    ))
}

/// Só a máquina: Crusher e Energized Smelter são a mesma `TileEntityElectricMachine` da câmara, com outro consumo e outras
/// receitas.
fn alone(world: SimWorld<Chamber>) -> Scene {
    Scene {
        world,
        chest: VanillaContainer::new(0, Rc::new(|| {})),
        first_slot: 0,
        refill_ticks: 0,
        refill_fraction: 0,
    }
}

/// Enche o baú como o cenário em Java: `fillChest`.
fn fill_chest(chest: &VanillaContainer, id: &str, count: u32) {
    for index in 0..CHEST_SLOTS {
        chest.set_item(index, Some(item(id)), count);
    }
}

/// `addUpgrades` até o total que a marca diz, como o cenário em Java faz com a diferença.
fn install(machine: &ProgressMachine, upgrade: &str, total: i32) {
    let upgrades = machine.upgrades();
    upgrades.add_upgrades(upgrade, total - upgrades.get_upgrades(upgrade));
}

/// `setUpgradeSlot`: o slot de entrada recebe `count` upgrades do tipo, sem passar pelo tick.
fn set_upgrade_slot(machine: &ProgressMachine, upgrade: &str, count: u32) {
    let upgrades = machine.upgrades();
    let resource = upgrades.data().resource_of(&format!("mekanism:{upgrade}"));
    upgrades
        .upgrade_slot()
        .set_contents(Some(resource), count, None);
}

/// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
fn apply(scene: &mut Scene, scenario: &str, action: &str) {
    let parts: Vec<&str> = action.split_whitespace().collect();
    let world = &mut scene.world;
    let machine = &mut world.block_entity.0;
    match parts.as_slice() {
        // `chamber_reduced_rate` instala a velocidade sem encher a energia: o rastro começa com ela vazia.
        ["speed", amount] if scenario == "chamber_reduced_rate" => {
            install(machine, SPEED_UPGRADE, amount.parse().unwrap());
        }
        ["speed", amount] => {
            install(machine, SPEED_UPGRADE, amount.parse().unwrap());
            machine
                .energy()
                .set_energy(machine.energy().capacity(), None);
        }
        ["energy", amount] if scenario == "chamber_upgrades" => {
            install(machine, ENERGY_UPGRADE, amount.parse().unwrap());
            machine
                .energy()
                .set_energy(machine.energy().capacity(), None);
        }
        ["upgrade", "energy", amount] => install(machine, ENERGY_UPGRADE, amount.parse().unwrap()),
        ["upgrade", "slot", "clear"] => machine
            .upgrades()
            .upgrade_slot()
            .set_contents(None, 0, None),
        ["upgrade", "slot", upgrade, count] => {
            set_upgrade_slot(machine, upgrade, count.parse().unwrap());
        }
        ["remove", upgrade @ ("speed" | "energy")] => machine
            .upgrades()
            .remove_upgrade(&format!("mekanism:{upgrade}"), false),
        ["remove", upgrade @ ("speed" | "energy"), "all"] => machine
            .upgrades()
            .remove_upgrade(&format!("mekanism:{upgrade}"), true),
        ["energy", amount] => machine.energy().set_energy(amount.parse().unwrap(), None),
        // `refillTicks` e `refillFraction` do `Trace`: a energia que volta depois de cada tick gravado.
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
            scene.refill_ticks = ticks.parse().unwrap();
        }
        [
            "energy",
            "topped",
            "up",
            "to",
            "a",
            "third",
            "of",
            "a",
            "tick",
            "every",
            "tick",
        ] => {
            scene.refill_ticks = 0;
            scene.refill_fraction = 3;
        }
        ["no", "more", "top", "ups"] => scene.refill_fraction = 0,
        ["insert", id, amount] | [id @ ("clay" | "hdpe_pellet"), amount] => {
            machine
                .input()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None);
        }
        ["output", id, amount] => {
            machine
                .output()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None)
        }
        ["item", id, amount] => {
            machine
                .input()
                .set_contents(Some(item(id)), amount.parse().unwrap(), None);
        }
        ["remove", "input"] => machine.input().set_contents(None, 0, None),
        ["clear", "output"] => machine.output().set_contents(None, 0, None),
        ["swap", "to", "stone"] => machine.input().set_contents(Some(item("stone")), 1, None),
        ["mode", mode] => machine.tile_mut().set_control_type(match *mode {
            "disabled" => RedstoneControl::Disabled,
            "high" => RedstoneControl::High,
            "low" => RedstoneControl::Low,
            "pulse" => RedstoneControl::Pulse,
            other => panic!("modo desconhecido: {other}"),
        }),
        // Só descrevem o cenário: o jogador entra pelo campo `players` do rastro, e o sinal já estava desligado.
        ["player", "opens" | "closes"] | ["no", "signal"] | ["with", ..] => {}
        ["signal", "on"] => world.set_signal(true),
        ["signal", "off"] => world.set_signal(false),
        // `ejectEast`: a saída vai para o lado do baú, e o ejetor liga.
        ["eject", "on"] => {
            let ejector = machine.ejector_mut();
            ejector.set_sides(0, vec![Face::East]);
            ejector.set_ejecting(true);
        }
        ["chest", "room", "2"] => {
            fill_chest(&scene.chest, "stone", 64);
            scene
                .chest
                .set_item(CHEST_SLOTS - 1, Some(item("clay_ball")), 62);
        }
        ["chest", "room", "0"] => fill_chest(&scene.chest, "stone", 64),
        ["chest", "clear"] => scene.chest.clear(),
        other => panic!("marca desconhecida: {other:?}"),
    }
}

fn slot(slot: &InventorySlot) -> Value {
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
    serde_json::json!([id, slot.amount()])
}

/// Um slot de upgrade como o rastro o grava: o tipo sem o namespace e a quantidade.
fn upgrade_slot(slot: &InventorySlot) -> Value {
    let kind = slot
        .resource()
        .and_then(|resource| resource.variant)
        .map_or(String::new(), |variant| {
            variant.trim_start_matches("mekanism:").to_owned()
        });
    serde_json::json!([kind, slot.amount()])
}

fn errors(machine: &ProgressMachine) -> Value {
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
    ];
    names
        .iter()
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| Value::from(*name))
        .collect()
}

/// O baú como o rastro o grava: só os slots com algo, `[índice, id, quantidade]`.
fn chest(chest: &VanillaContainer, first_slot: usize) -> Value {
    chest
        .stacks()
        .into_iter()
        .map(|(index, resource, amount)| {
            serde_json::json!([index + first_slot, resource.id.to_string(), amount])
        })
        .collect()
}

fn replay(scenario: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    // Os cenários do recheck gravam o sorteio e o relógio do jogo: entram na máquina e no mundo simulado.
    let clock = lines[0].get("recheck_offset").map(|offset| {
        let game_time = lines[0]["game_time"].as_u64().unwrap();
        (
            offset.as_u64().unwrap(),
            game_time - lines[0]["t"].as_u64().unwrap(),
        )
    });
    let mut scene = match scenario {
        _ if clock.is_some() => {
            let (offset, base) = clock.unwrap();
            rechecking_scene(offset, base)
        }
        "chamber_eject_furnace" => furnace_scene(),
        "crusher_run" => alone(machine(
            CRUSHER_USAGE,
            vec![recipe("cobblestone", 1, "gravel", 1)],
        )),
        "smelter_run" => alone(machine(
            SMELTER_USAGE,
            vec![recipe("raw_iron", 1, "iron_ingot", 1)],
        )),
        _ => scene(),
    };
    let mut players = 0;
    for expected in &lines {
        // O que o `warnings`, o `warnings_shown` e o `recheck` do rastro dizem sai do objeto: o resto é comparado inteiro.
        let mut expected = expected.clone();
        let recorded: Vec<(&str, Value)> = RECHECK_FIELDS
            .iter()
            .filter_map(|key| Some((*key, expected.as_object_mut()?.remove(*key)?)))
            .collect();
        let expected = &expected;
        scene.world.tick();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                apply(&mut scene, scenario, action);
            }
        }
        let machine = &scene.world.block_entity.0;
        let field = |name: &str| {
            recorded
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, v)| v)
        };
        // `open` e `close` do core nos ticks em que o número de jogadores do rastro muda.
        if let Some(wanted) = field("players").and_then(Value::as_u64) {
            while players < wanted {
                machine.open();
                players += 1;
            }
            while players > wanted {
                machine.close();
                players -= 1;
            }
        }
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
            "errors": errors(machine),
        });
        // Só o cenário de ejeção grava o baú.
        if expected.get("chest").is_some() {
            actual["chest"] = chest(&scene.chest, scene.first_slot);
        }
        // Só o cenário dos slots de upgrade grava os upgrades.
        let with_upgrades = expected.get("speed").is_some();
        if with_upgrades {
            let upgrades = machine.upgrades();
            actual["speed"] = upgrades.get_upgrades(SPEED_UPGRADE).into();
            actual["energy_upgrades"] = upgrades.get_upgrades(ENERGY_UPGRADE).into();
            // O Java grava o `float` pelo texto mais curto que o lê de volta, que o `{:?}` do `f32` também dá.
            actual["upgrade_progress"] =
                serde_json::from_str(&format!("{:?}", upgrades.scaled_upgrade_progress())).unwrap();
            actual["upgrade_in"] = upgrade_slot(upgrades.upgrade_slot());
            actual["upgrade_out"] = upgrade_slot(upgrades.upgrade_output_slot());
        }
        // O `MEKTRACE` do jogo tem os mesmos campos dos rastros, com as marcas vazias.
        let stacks: Vec<(usize, String, u32)> = scene
            .chest
            .stacks()
            .into_iter()
            .map(|(index, resource, amount)| {
                (index + scene.first_slot, resource.id.to_string(), amount)
            })
            .collect();
        let with_chest = expected.get("chest").is_some().then_some(stacks.as_slice());
        let line = trace_line(
            expected["t"].as_i64().unwrap(),
            machine,
            with_chest,
            with_upgrades,
        );
        let mut traced: Value = serde_json::from_str(&line).unwrap();
        traced["marks"] = expected["marks"].clone();
        assert_eq!(
            traced, actual,
            "{scenario}, tick {}: MEKTRACE",
            expected["t"]
        );
        assert_eq!(&actual, expected, "{scenario}, tick {}", expected["t"]);
        if let Some((_, base)) = clock {
            let game_time = field("game_time").and_then(Value::as_u64).unwrap();
            assert_eq!(
                game_time,
                base + expected["t"].as_u64().unwrap(),
                "{scenario}: o relógio do rastro anda um por tick"
            );
            // O flag que a máquina leu no tick, com o sorteio, o relógio e os jogadores que ela recebeu.
            assert_eq!(
                Some(&Value::from(machine.recheck_all_errors())),
                field("recheck"),
                "{scenario}, tick {}: recheck",
                expected["t"]
            );
            assert_eq!(
                Some(&Value::from(
                    format!("{:?}", machine.tile().control_type()).to_lowercase()
                )),
                field("mode"),
                "{scenario}, tick {}: modo de redstone",
                expected["t"]
            );
            assert_warnings(scenario, expected["t"].as_u64().unwrap(), machine, &field);
        }
        // Depois de gravar o tick, como o `Trace` em Java: a máquina vê a energia devolvida no tick seguinte.
        scene.refill();
    }
}

/// Os avisos da tela: `warnings` são os tipos ativos e `warnings_shown` o que `getWarnings` lista, depois da ordem do enum e do
/// corte. A segunda conta é a do cliente, que só tem a máscara de erros num data slot e o modo de redstone.
fn assert_warnings<'a>(
    scenario: &str,
    tick: u64,
    machine: &ProgressMachine,
    field: &impl Fn(&str) -> Option<&'a Value>,
) {
    let synced = SyncedMachine::of(machine, &ELECTRIC_TRACKED_ERRORS);
    // O que passa pelo data slot (16 bits com sinal) é o que chega ao cliente.
    let client = SyncedMachine {
        errors: ErrorMask::from_slot(synced.errors.to_slot()),
        ..synced
    };
    for (side, tracker) in [
        ("servidor", machine_warnings(machine, &ELECTRIC_WARNINGS)),
        ("cliente", machine_warnings(&client, &ELECTRIC_WARNINGS)),
    ] {
        let names = |kinds: Vec<mekanism_rules::WarningType>| -> Value {
            kinds
                .into_iter()
                .map(|kind| Value::from(kind.name()))
                .collect()
        };
        let triggered = tracker.triggered();
        assert_eq!(
            Some(&names(triggered.clone())),
            field("warnings"),
            "{scenario}, tick {tick}: warnings ({side})"
        );
        assert_eq!(
            Some(&names(tracker.warnings())),
            field("warnings_shown"),
            "{scenario}, tick {tick}: warnings_shown ({side})"
        );
        // `hasWarning` é a aba aparecer: há aviso ativo se e só se algum tipo está ativo.
        assert_eq!(
            tracker.has_warning(),
            !triggered.is_empty(),
            "{scenario}, tick {tick}: hasWarning ({side})"
        );
    }
}

/// O que os rastros do recheck gravam além dos campos de sempre: saem do objeto antes da comparação inteira, e cada um é conferido
/// à parte (o recheck e os avisos, contra a máquina; o relógio e os jogadores, contra o que o teste entrega a ela).
const RECHECK_FIELDS: [&str; 7] = [
    "game_time",
    "players",
    "recheck",
    "recheck_offset",
    "mode",
    "warnings",
    "warnings_shown",
];

/// O primeiro tick em que o rastro tem o erro, e se o recheck estava ligado nele.
fn first_tick_with(scenario: &str, error: &str) -> Option<(u64, bool)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|line| {
            line["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e == error)
        })
        .map(|line| {
            (
                line["t"].as_u64().unwrap(),
                line["recheck"].as_bool().unwrap(),
            )
        })
}

#[test]
fn energy_running_out_and_coming_back() {
    replay("chamber_energy");
}

#[test]
fn input_removed_and_swapped() {
    replay("chamber_input_removed");
}

#[test]
fn input_below_what_the_recipe_needs() {
    replay("chamber_input_short");
}

#[test]
fn output_without_room_or_with_another_item() {
    replay("chamber_output");
}

#[test]
fn redstone_modes() {
    replay("chamber_redstone");
}

#[test]
fn deactivation_delay() {
    replay("chamber_active_delay");
}

#[test]
fn a_paused_recipe_ignores_what_does_not_notify_it() {
    replay("chamber_paused");
}

#[test]
fn upgrades() {
    replay("chamber_upgrades");
}

/// Instalar leva 20 ticks; com 10 no slot instala só até o máximo e sobram 5; esvaziar o slot no meio zera o avanço; tirar um e
/// todos devolve ao slot de saída; e o slot de saída com outro tipo não aceita o que sai.
#[test]
fn the_upgrade_slots_install_and_remove() {
    replay("chamber_upgrade_slot");
}

#[test]
fn the_crusher_uses_its_own_energy_and_recipes() {
    replay("crusher_run");
}

#[test]
fn the_smelter_uses_its_own_energy_and_recipes() {
    replay("smelter_run");
}

#[test]
fn the_output_only_enters_a_furnace_side_as_fuel() {
    replay("chamber_eject_furnace");
}

#[test]
fn the_output_ejects_into_a_chest_with_room_for_all_part_and_none() {
    replay("chamber_eject");
}

/// Com jogador na tela o recheck volta a cada 100 ticks, no sorteio da máquina; a câmara está parada por erros e o conjunto de erros
/// não muda, então só o booleano e o relógio andam.
#[test]
fn a_player_looking_turns_the_full_recheck_on_once_every_hundred_ticks() {
    replay("chamber_recheck_looking");
}

#[test]
fn nobody_looking_never_runs_the_full_recheck() {
    replay("chamber_recheck_alone");
}

#[test]
fn the_recheck_stops_when_the_player_closes_and_comes_back_when_it_reopens() {
    replay("chamber_recheck_open_close");
}

/// Os modos de redstone com jogador na tela: PULSE cai em DISABLED, e o erro de energia pausa a receita por trás do modo.
#[test]
fn the_redstone_modes_with_a_player_looking() {
    replay("chamber_warnings_redstone");
}

/// A receita acordada só conhece o erro de energia: o erro de saída cheia só aparece no tick do recheck.
#[test]
fn the_full_recheck_finds_the_error_a_woken_recipe_did_not_know() {
    // O rastro tem de ter o efeito: o erro novo nasce no tick em que o recheck está ligado.
    assert_eq!(
        first_tick_with("chamber_recheck_changes_errors", "output_space")
            .map(|(_, recheck)| recheck),
        Some(true)
    );
    replay("chamber_recheck_changes_errors");
}

#[test]
fn without_a_player_the_woken_recipe_never_finds_the_other_error() {
    assert_eq!(
        first_tick_with("chamber_recheck_changes_errors_alone", "output_space"),
        None
    );
    replay("chamber_recheck_changes_errors_alone");
}

/// A câmara não registra `NOT_ENOUGH_ENERGY_REDUCED_RATE`: com velocidade 8 e energia para 3 ticks, para um terço de tick e para nada,
/// os avisos são só os de energia, e o separador, que registra o tipo, fica de fora porque ainda não está portado.
#[test]
fn the_chamber_never_reports_the_reduced_rate() {
    replay("chamber_reduced_rate");
}

/// O baú de `ItemHandler` serve também de alvo: confere que o `Rc` dele vira o handler que o mundo entrega.
#[test]
fn the_chest_is_the_handler_the_east_side_reaches() {
    let scene = scene();
    let handler: Option<Rc<dyn ItemHandler>> = scene.world.neighbours.item_handler(Face::East);
    assert_eq!(handler.map(|handler| handler.size()), Some(CHEST_SLOTS));
}

/// Salvar e reabrir no meio de uma operação: o objeto nasce de novo dos itens, da energia e do progresso salvos, que é o
/// que o glue faz com a `ModBlockEntity`, e a operação segue de onde parou.
#[test]
fn a_reborn_machine_goes_on_from_the_saved_progress() {
    let mut before = chamber();
    {
        let machine = &before.block_entity.0;
        machine.input().set_contents(Some(item("clay")), 3, None);
        machine.energy().set_energy(20, None);
    }
    for _ in 0..60 {
        tick_with_energy(&mut before);
    }
    let machine = &before.block_entity.0;
    let (progress, energy) = (machine.recipe_progress(), machine.energy().amount());
    let (input, output) = (machine.input().contents(), machine.output().contents());
    assert!(progress > 0 && progress < 200, "no meio da operação");

    let mut reborn = chamber();
    {
        let machine = &reborn.block_entity.0;
        machine.load_operating_ticks(progress);
        machine
            .input()
            .set_contents(input.resource, input.amount, None);
        machine
            .output()
            .set_contents(output.resource, output.amount, None);
        machine.energy().set_energy(energy, None);
    }
    tick_with_energy(&mut reborn);
    assert_eq!(reborn.block_entity.0.recipe_progress(), progress + 1);
}

/// Um tick com a energia sempre cheia, como um cabo que a mantém.
fn tick_with_energy(world: &mut SimWorld<Chamber>) {
    let energy = world.block_entity.0.energy();
    energy.set_energy(energy.capacity(), None);
    world.tick();
}

/// Tudo o que o Java persiste volta: redstone, modo, estado ativo e atraso, nível do comparador, progresso, receitas usadas,
/// a última e o ejetor. Nada de `pausedForErrors`: o objeto novo anda como o Java depois de carregar.
#[test]
fn the_saved_state_round_trips_everything_the_java_persists() {
    let mut before = chamber();
    {
        let machine = &mut before.block_entity.0;
        machine.input().set_contents(Some(item("clay")), 3, None);
        machine.tile_mut().set_control_type(RedstoneControl::High);
        machine.tile_mut().update_power(true);
        machine.ejector_mut().set_ejecting(true);
    }
    // Uma operação inteira (200 ticks) e mais um pouco da seguinte.
    for _ in 0..230 {
        tick_with_energy(&mut before);
    }
    let saved = before.block_entity.0.saved_state();
    assert_eq!(saved.control_type, RedstoneControl::High);
    assert!(saved.redstone && saved.active && saved.ejecting);
    assert!(saved.progress > 0 && saved.progress < 200);
    assert_eq!(
        saved.recipes_used.get("mekanism:enriching/clay_ball"),
        Some(&1)
    );
    assert_eq!(
        saved.last_recipe_used.as_deref(),
        Some("mekanism:enriching/clay_ball")
    );
    assert_eq!(saved.last_recipe_amount, 1);

    let mut reborn = chamber();
    reborn.block_entity.0.restore(&saved, true);
    assert_eq!(reborn.block_entity.0.saved_state(), saved);
    assert!(reborn.block_entity.0.tile().block_active());
}

/// Salvar e reabrir com upgrades instalados e nos dois slots: o objeto novo nasce do que o Java grava, recalcula tempo, consumo e
/// capacidade, e o que estava no slot de entrada segue a instalação de onde a máquina parou.
/// `chamber_placed`: colocada do item, como um jogador, a máquina recalcula os upgrades. O `setBlock` dos outros rastros não passa
/// por aí, e a capacidade deles começa em 4 ticks de consumo, não nos ticks da receita.
#[test]
fn a_machine_placed_or_loaded_recalculates_its_capacity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/chamber_placed.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    let mut world = chamber();
    let saved = chamber().block_entity.0.saved_state();
    world.block_entity.0.restore(&saved, false);
    let machine = &world.block_entity.0;
    assert_eq!(
        serde_json::json!(machine.energy().capacity()),
        first["capacity"]
    );
    assert_eq!(
        serde_json::json!(machine.ticks_required()),
        first["required"]
    );
}

#[test]
fn the_installed_upgrades_and_both_slots_survive_a_reopen() {
    let before = chamber();
    {
        let machine = &before.block_entity.0;
        install(machine, SPEED_UPGRADE, 5);
        install(machine, ENERGY_UPGRADE, 2);
        machine.upgrades().remove_upgrade(ENERGY_UPGRADE, false);
        set_upgrade_slot(machine, "speed", 7);
    }
    let machine = &before.block_entity.0;
    let saved = machine.saved_state();
    assert_eq!(saved.upgrades.upgrades.get(SPEED_UPGRADE), Some(&5));
    assert_eq!(saved.upgrades.upgrades.get(ENERGY_UPGRADE), Some(&1));
    assert_eq!(saved.upgrades.input.amount, 7);
    assert_eq!(saved.upgrades.output.amount, 1);
    let (required, per_tick, capacity) = (
        machine.ticks_required(),
        machine.energy().energy_per_tick(),
        machine.energy().capacity(),
    );
    // 200 * 10^(-5/8) = 47,4 e 5 * 10^(2 * 5/8 - 1/8) = 66,7: `getTicks` trunca e `getEnergyPerTick` arredonda para cima.
    assert_eq!((required, per_tick), (47, 67));

    let mut reborn = chamber();
    reborn.block_entity.0.restore(&saved, false);
    let machine = &reborn.block_entity.0;
    assert_eq!(machine.saved_state(), saved);
    assert_eq!(
        (
            machine.ticks_required(),
            machine.energy().energy_per_tick(),
            machine.energy().capacity()
        ),
        (required, per_tick, capacity),
        "o consumo e a capacidade seguem o que está instalado"
    );
    // O slot de entrada tem 7 de velocidade e a máquina já tem 5: 20 ticks depois entram os 3 que faltam, e sobram 4.
    for _ in 0..21 {
        reborn.tick();
    }
    let machine = &reborn.block_entity.0;
    assert_eq!(machine.upgrades().get_upgrades(SPEED_UPGRADE), 8);
    assert_eq!(machine.upgrades().upgrade_slot().amount(), 4);
    assert_eq!(machine.ticks_required(), 20);
}
