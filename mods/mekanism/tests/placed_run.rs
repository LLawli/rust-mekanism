//! Repete, tick a tick, o que uma máquina posta por um jogador faz com uma só carga de energia
//! (`dev/referencia/rastros/placed_run_<máquina>_<variante>.jsonl`, escrito por `PlacedTraceTest.java`): a carga é o que a máquina
//! aceita (`insert` externo sem limite), a entrada é de 5 itens, e depois dela não entra mais energia. Sem upgrade a carga é a
//! capacidade de 1.000 (1.600 no Crusher) e dá justo uma operação de 200 ticks; com velocidade 2 (item com o componente de upgrades)
//! a capacidade é a de 112 ticks de 16 (26 no Crusher) e dá justo uma operação de 112 ticks. A segunda operação não anda: a máquina
//! gasta tudo e para. Numa máquina do gabarito a carga seria de 20, e a operação não passaria de 4 ticks.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::upgrade::UpgradeSaved;
use mekanism_rules::{
    CRUSHER_USAGE, ENRICHMENT_USAGE, SMELTER_USAGE, SPEED_UPGRADE, electric_config,
};
use serde_json::Value;
use std::path::Path;
use std::rc::Rc;

const ITEMS: u32 = 5;
const TANK: u64 = 500;
/// Linhas de tick de cada variante: 233 ticks, de 0 a 232.
const TICKS: usize = 233;

fn rows(machine: &str, variant: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("placed_run_{machine}_{variant}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn electric(usage: i32, input: &str, output: &str, count: u32) -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: format!("mekanism:test/{output}"),
        input: ItemIngredient::item(input),
        input_count: 1,
        output: ItemOutput {
            resource: common::item(output),
            count,
        },
    });
    let mut config = electric_config(
        usage,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        common::upgrade_data(),
    );
    config.energy_conversion = common::energy_conversions();
    ProgressMachine::electric(config)
}

/// A máquina do jogador: o construtor e depois o `applyImplicitComponents` do item.
fn placed(name: &str, speed: i32) -> (ProgressMachine, &'static str) {
    let (mut machine, input) = match name {
        "enrichment_chamber" => (electric(ENRICHMENT_USAGE, "clay", "clay_ball", 4), "clay"),
        "crusher" => (
            electric(CRUSHER_USAGE, "cobblestone", "gravel", 1),
            "cobblestone",
        ),
        "energized_smelter" => (electric(SMELTER_USAGE, "sand", "glass", 1), "sand"),
        "metallurgic_infuser" => (common::infuser(), "dirt"),
        other => panic!("{other}"),
    };
    let mut item = UpgradeSaved::default();
    if speed > 0 {
        item.upgrades.insert(SPEED_UPGRADE.to_owned(), speed);
    }
    machine.apply_item_components(&item);
    (machine, input)
}

fn replay(name: &str, variant: &str, speed: i32) {
    let all = rows(name, variant);
    let header = &all[0];
    let (mut machine, input) = placed(name, speed);
    assert_eq!(
        header["max_energy"],
        machine.energy().capacity(),
        "{name} {variant}: capacidade"
    );
    assert_eq!(
        header["energy_per_tick"],
        machine.energy().energy_per_tick(),
        "{name} {variant}: consumo"
    );
    assert_eq!(
        header["ticks_required"],
        machine.ticks_required(),
        "{name} {variant}: ticks"
    );

    if let Some(side) = machine.chemical_side() {
        side.tank
            .set_contents(Some(ChemicalResource::new("bio")), TANK, None);
    }
    machine
        .input()
        .set_contents(Some(common::item(input)), ITEMS, None);

    // A carga entra no passo do teste, depois do tick das máquinas e antes da amostra do mesmo tick: a linha do tick seguinte ao
    // `burst` já mostra a energia nova, e a máquina só a gasta no tick depois dela.
    let (mut ticks, mut bursts, mut pending) = (0, 0, None);
    for row in &all[1..] {
        if let Some(burst) = row.get("burst") {
            pending = Some(burst.clone());
            continue;
        }
        let t = row["t"].as_u64().unwrap();
        machine.tick_server(t, &NoNeighbours);
        if let Some(burst) = pending.take() {
            let root = Transaction::open_root();
            let accepted = machine
                .energy()
                .insert(i32::MAX, &root, AutomationType::External);
            root.commit();
            assert_eq!(
                burst,
                Value::from(accepted),
                "{name} {variant}: o que a máquina aceita"
            );
            bursts += 1;
        }
        let output = machine.output().contents();
        let made = [
            output
                .resource
                .map_or(String::new(), |resource| resource.id.to_string()),
            output.amount.to_string(),
        ];
        let found = (
            machine.energy().amount(),
            i64::from(machine.recipe_progress()),
            i64::from(machine.ticks_required()),
            i64::from(machine.energy().energy_per_tick()),
        );
        let expected = (
            row["energy"].as_i64().unwrap(),
            row["progress"].as_i64().unwrap(),
            row["required"].as_i64().unwrap(),
            row["per_tick"].as_i64().unwrap(),
        );
        assert_eq!(
            found, expected,
            "{name} {variant} tick {t}: energia, progresso, ticks e consumo"
        );
        assert_eq!(
            [
                row["output"][0].as_str().unwrap().to_owned(),
                row["output"][1].to_string()
            ],
            made,
            "{name} {variant} tick {t}: saída"
        );
        ticks += 1;
    }
    assert_eq!(ticks, TICKS, "{name} {variant}: ticks do rastro");
    assert_eq!(bursts, 1, "{name} {variant}: uma carga só");
}

#[test]
fn a_chamber_set_by_a_player_runs_one_operation_on_one_burst() {
    replay("enrichment_chamber", "plain", 0);
    replay("enrichment_chamber", "speed2", 2);
}

#[test]
fn a_crusher_set_by_a_player_runs_one_operation_on_one_burst() {
    replay("crusher", "plain", 0);
    replay("crusher", "speed2", 2);
}

#[test]
fn a_smelter_set_by_a_player_runs_one_operation_on_one_burst() {
    replay("energized_smelter", "plain", 0);
    replay("energized_smelter", "speed2", 2);
}

#[test]
fn an_infuser_set_by_a_player_runs_one_operation_on_one_burst() {
    replay("metallurgic_infuser", "plain", 0);
    replay("metallurgic_infuser", "speed2", 2);
}

/// O contraste que dá dentes ao teste: a mesma carga numa máquina do gabarito (20) gasta 4 ticks e para, e o Java gravou 1.000.
#[test]
fn the_same_burst_in_a_template_machine_would_stop_after_four_ticks() {
    let all = rows("enrichment_chamber", "plain");
    let burst = all
        .iter()
        .find_map(|row| row.get("burst"))
        .and_then(Value::as_i64)
        .unwrap();
    let template = electric(ENRICHMENT_USAGE, "clay", "clay_ball", 4);
    let root = Transaction::open_root();
    let accepted = template
        .energy()
        .insert(i32::MAX, &root, AutomationType::External);
    root.commit();
    assert_eq!(accepted, 20);
    assert_eq!(burst, 1_000);
    assert_eq!(
        i64::from(accepted) / i64::from(template.energy().energy_per_tick()),
        4
    );
}
