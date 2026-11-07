//! Repete, tick a tick, os rastros de efeitos da Enrichment Chamber original (`chamber_muffling`, `chamber_xp_containment` e
//! `chamber_xp_broken`): o volume do som com os upgrades de silêncio e a experiência guardada com o de contenção.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::recipe_monitor::Awarded;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{
    ENRICHMENT_USAGE, MUFFLING_UPGRADE, SPEED_UPGRADE, XP_CONTAINMENT_UPGRADE, electric_config,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const COAL_RECIPE: &str = "mekanism:enriching/enriched/carbon";
const CLAY_RECIPE: &str = "mekanism:enriching/clay_to_ball";
/// A energia que o cenário em Java devolve depois de cada tick: `refillTicks = 3`.
const REFILL_TICKS: i64 = 3;

struct Chamber(ProgressMachine);

impl SimBlockEntity for Chamber {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn recipe(id: &str, input: &str, output: &str, count: u32) -> Rc<ItemToItemRecipe> {
    Rc::new(ItemToItemRecipe {
        id: id.to_owned(),
        input: ItemIngredient::item(input),
        input_count: 1,
        output: ItemOutput {
            resource: item(output),
            count,
        },
    })
}

/// A `experience` dos dois JSON: o carvão dá 0,5 e a argila 0,1.
fn experience(id: &str) -> Option<f32> {
    match id {
        COAL_RECIPE => Some(0.5),
        CLAY_RECIPE => Some(0.1),
        _ => None,
    }
}

fn chamber() -> ProgressMachine {
    let machine = ProgressMachine::electric(electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![
            recipe(COAL_RECIPE, "coal", "enriched_carbon", 1),
            recipe(CLAY_RECIPE, "clay", "clay_ball", 4),
        ]),
        0,
        common::upgrade_data(),
    ));
    machine.set_experience_lookup(Rc::new(experience));
    machine
}

fn install(machine: &ProgressMachine, upgrade: &str, total: i32) {
    let upgrades = machine.upgrades();
    upgrades.add_upgrades(
        &format!("mekanism:{upgrade}"),
        total - upgrades.get_upgrades(&format!("mekanism:{upgrade}")),
    );
}

/// Os pontos que as bolas de experiência desta remessa dão. Os rastros só têm remessas inteiras: sem sorteio.
fn points(awarded: &Awarded) -> i64 {
    awarded
        .drops
        .iter()
        .map(|drop| {
            let whole = drop.points(0.0);
            assert_eq!(
                whole,
                drop.points(0.999),
                "a remessa do rastro tinha de ser inteira: {drop:?}"
            );
            i64::from(whole)
        })
        .sum()
}

struct Scene {
    world: SimWorld<Chamber>,
    popped: i64,
    refill: bool,
}

impl Scene {
    fn new(refill: bool) -> Self {
        Self {
            world: SimWorld::new(Chamber(chamber())),
            popped: 0,
            refill,
        }
    }

    fn machine(&self) -> &ProgressMachine {
        &self.world.block_entity.0
    }

    /// O que o upgrade que mudou soltou no `recalculateUpgrades`.
    fn collect_drops(&mut self) {
        let awarded = Awarded {
            recipes: Vec::new(),
            drops: self.machine().take_experience_drops(),
        };
        self.popped += points(&awarded);
    }

    fn apply(&mut self, mark: &str) {
        let machine = self.machine();
        match mark {
            _ if mark.starts_with("before ") || mark.starts_with("after ") => {}
            "speed 8, coal 2, no containment" => {
                install(machine, "speed", 8);
                machine.input().set_contents(Some(item("coal")), 2, None);
            }
            "speed 8, xp_containment 1, coal 2" => {
                install(machine, "speed", 8);
                install(machine, "xp_containment", 1);
                machine.input().set_contents(Some(item("coal")), 2, None);
            }
            "install xp_containment 1 with 2 operations done" => {
                install(machine, "xp_containment", 1);
            }
            "coal 2 again" => machine.input().set_contents(Some(item("coal")), 2, None),
            "open the menu and take the output with a click" => {
                // `InventoryContainerResultSlot`: o resultado sai do slot (e `outputDecreased` dispara) e depois vem o
                // `onTakeCheck`.
                machine.output().set_contents(None, 0, None);
                let awarded = machine.award_used_recipes();
                self.popped += points(&awarded);
            }
            "collect with the xp tab" | "collect with the xp tab, without containment" => {
                let awarded = machine.collect_xp();
                self.popped += points(&awarded);
            }
            "take the output again" => {
                machine.output().set_contents(None, 0, None);
                let awarded = machine.award_used_recipes();
                self.popped += points(&awarded);
            }
            "coal 2 again, then remove xp_containment" => {
                machine.input().set_contents(Some(item("coal")), 2, None)
            }
            "remove xp_containment with 2 operations stored" => machine
                .upgrades()
                .remove_upgrade(XP_CONTAINMENT_UPGRADE, true),
            "install xp_containment 1 again, coal 2" => {
                install(machine, "xp_containment", 1);
                machine.input().set_contents(Some(item("coal")), 2, None);
            }
            "another recipe, clay 2" => machine.input().set_contents(Some(item("clay")), 2, None),
            "break the block" | "break the block without containment" => {
                let awarded = machine.recipes_to_award_on_removal();
                self.popped += points(&awarded);
            }
            "place another, speed 8, coal 2, no containment" => {
                let fresh = chamber();
                install(&fresh, "speed", 8);
                fresh.input().set_contents(Some(item("coal")), 2, None);
                self.world.block_entity.0 = fresh;
            }
            "install muffling 5, only the maximum fits" => {
                machine.upgrades().add_upgrades(MUFFLING_UPGRADE, 5);
            }
            "remove muffling all" => machine.upgrades().remove_upgrade(MUFFLING_UPGRADE, true),
            "an energy upgrade does not change the volume" => install(machine, "energy", 3),
            other => panic!("marca desconhecida: {other}"),
        }
        self.collect_drops();
    }
}

fn read(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O que a máquina diz dos efeitos, com os nomes do rastro.
fn effects(machine: &ProgressMachine) -> Value {
    let saved = machine.saved_state();
    let used: BTreeMap<&String, &i32> = saved.recipes_used.iter().collect();
    json!({
        "volume_factor": machine.volume_factor(),
        "fully_muffled": machine.is_fully_muffled(),
        "xp_containment": machine.has_xp_containment(),
        "had_containment": saved.had_containment,
        "recipes_used": used,
        "last_recipe": saved.last_recipe_used,
        "last_amount": saved.last_recipe_amount,
        "reset_last": machine.reset_last_recipe(),
    })
}

/// Repete o rastro. `orb_points` do Java é o que existe em volta da máquina, e o jogador falso colhe as bolas aos poucos: o que
/// se compara é o que sobe, que tem de ser o que a regra solta naquele tick.
fn replay(scenario: &str, minimum: usize, refill: bool) {
    let lines = read(scenario);
    assert!(lines.len() >= minimum, "rastro curto: {}", lines.len());
    let mut scene = Scene::new(refill);
    let mut previous_orbs = 0;
    let mut broken_ticks = 0;
    for expected in &lines {
        scene.world.tick();
        scene.popped = 0;
        let marks: Vec<&str> = expected["marks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mark| mark.as_str().unwrap())
            .collect();
        // Os `before` e `after` são só do Java; o `break` e o `place` valem no mesmo tick do rastro.
        for mark in &marks {
            scene.apply(mark);
        }
        let tick = expected["t"].as_i64().unwrap();
        let orbs = expected["orb_points"].as_i64();
        if let Some(orbs) = orbs {
            if orbs > previous_orbs {
                assert_eq!(
                    orbs - previous_orbs,
                    scene.popped,
                    "{scenario} t={tick}: experiência solta"
                );
            } else {
                assert_eq!(
                    scene.popped, 0,
                    "{scenario} t={tick}: soltou sem a bola aparecer"
                );
            }
            previous_orbs = orbs;
        }
        if expected.get("broken").is_some() {
            broken_ticks += 1;
        } else {
            let actual = effects(scene.machine());
            for (key, value) in actual.as_object().unwrap() {
                assert_eq!(
                    value, &expected[key],
                    "{scenario} t={tick}: {key}, marcas {marks:?}"
                );
            }
            if scene.refill {
                let energy = scene.machine().energy();
                energy.set_energy(REFILL_TICKS * i64::from(energy.energy_per_tick()), None);
            }
        }
    }
    if scenario == "chamber_xp_broken" {
        assert_eq!(broken_ticks, 2);
    }
}

#[test]
fn muffling_silences_the_machine_only_at_the_maximum() {
    replay("chamber_muffling", 20, false);
}

#[test]
fn xp_containment_keeps_the_experience_until_the_output_is_taken() {
    replay("chamber_xp_containment", 200, true);
}

#[test]
fn without_the_upgrade_the_last_recipe_gives_its_experience_when_the_output_is_taken() {
    replay("chamber_xp_vanilla", 100, true);
}

#[test]
fn a_broken_machine_drops_the_stored_experience_only_with_the_upgrade() {
    replay("chamber_xp_broken", 80, true);
}

#[test]
fn the_speed_upgrade_constant_is_the_one_the_trace_installs() {
    assert_eq!(SPEED_UPGRADE, "mekanism:speed");
}

/// `xpContainmentUpgrade == null`: um datapack que tira o tipo do registro faz a contenção valer sempre (e o som não mudar).
#[test]
fn a_registry_without_the_xp_type_always_contains_experience() {
    use machinery_basis_core::upgrade::UpgradeData;
    let data = Rc::new(UpgradeData::from_parts(
        "mekanism:upgrade",
        64,
        &[(SPEED_UPGRADE, 8), ("mekanism:energy", 8)],
        &[SPEED_UPGRADE, "mekanism:energy"],
    ));
    let machine = ProgressMachine::electric(electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(Vec::new()),
        0,
        data,
    ));
    assert!(machine.has_xp_containment());
    assert_eq!(machine.volume_factor(), 1.0);
    assert!(!machine.is_fully_muffled());
    // Com o tipo no registro e nenhum instalado, não.
    assert!(!chamber().has_xp_containment());
}
