//! Repete `dev/referencia/rastros/placed_fed_<máquina>.jsonl` (`PlacedTraceTest.fed`): uma máquina posta por um jogador ao lado de um Heat Generator cheio (240), com
//! e sem upgrades no item, em dois momentos do tick. O que o rastro mostra é a capacidade que o gerador encontra: `setPlacedBy` ->
//! `applyImplicitComponents` -> `recalculateUpgrades` -> `MachineEnergyContainer.updateMaxEnergy` roda na colocação, então a máquina já tem a capacidade do item
//! (1.000, ou a dos upgrades) quando o gerador empurra a energia, no mesmo tick (`pre`: o jogador age antes do tick, o gerador tica depois) ou no seguinte (`step`).
//!
//! O modelo do gerador é o do `EnergyUtils.emit` com um alvo só: cabe o que a capacidade deixa. A máquina é a da lib, e a colocação chama o mesmo
//! `apply_item_components` que o `place_block` do mod chama.
mod common;

use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::upgrade::UpgradeSaved;
use mekanism_rules::{ENERGY_UPGRADE, SPEED_UPGRADE};
use serde_json::Value;
use std::path::Path;

const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];
/// Os cinco pares de upgrade do item e as duas fases: dez colocações por máquina.
const PLACEMENTS: usize = 10;

fn rows(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("placed_fed_{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn number(row: &Value, key: &str) -> i64 {
    row[key]
        .as_i64()
        .unwrap_or_else(|| panic!("campo {key} ausente em {row}"))
}

fn item(speed: i32, energy: i32) -> UpgradeSaved {
    let mut saved = UpgradeSaved::default();
    for (id, total) in [(SPEED_UPGRADE, speed), (ENERGY_UPGRADE, energy)] {
        if total > 0 {
            saved.upgrades.insert(id.to_owned(), total);
        }
    }
    saved
}

/// Onde a capacidade da colocação passa a valer: `Placement` é o `place_block` do mod (a capacidade do item na hora), `FirstTick` o port antigo (a do
/// construtor, quatro ticks de consumo, até o primeiro tick da máquina).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capacity {
    Placement,
    FirstTick,
}

/// Repete uma colocação e devolve o que diverge. O tick `since = 0` é o da colocação.
fn replay(name: &str, when: Capacity) -> Vec<String> {
    let lines = rows(name);
    let mut wrong = Vec::new();
    let mut placements = 0;
    let mut index = 0;
    while index < lines.len() {
        let start = &lines[index];
        assert!(start.get("at_placement").is_some(), "{name}: linha {index}");
        placements += 1;
        let label = start["placed"].as_str().unwrap().to_owned();
        let pre = start["phase"] == "pre";
        let mut machine: ProgressMachine = common::bare_machine(name, &common::upgrade_data());
        // O gerador está cheio quando o jogador põe a máquina.
        let mut generator = number(&start["at_placement"], "generator");
        // `setPlacedBy`: o item aplica os componentes. No port antigo isso esperava o primeiro tick.
        let upgrades = item(
            number(start, "speed") as i32,
            number(start, "energy_upgrades") as i32,
        );
        if when == Capacity::Placement {
            machine.apply_item_components(&upgrades);
        }
        let at = &start["at_placement"];
        let got = (machine.energy().amount(), machine.energy().capacity());
        if when == Capacity::Placement && got != (number(at, "energy"), number(at, "capacity")) {
            wrong.push(format!(
                "{label}: na colocação {got:?}, o rastro diz ({}, {})",
                number(at, "energy"),
                number(at, "capacity")
            ));
        }
        let mut next = index + 1;
        while next < lines.len() && lines[next].get("after_tick").is_some() {
            let line = &lines[next];
            let since = number(line, "since");
            // O gerador empurra no tick da colocação só se ela veio antes dele (`pre`); na fase `step` o primeiro empurrão é o do tick seguinte.
            if (pre || since > 0) && generator > 0 {
                let room = machine.energy().capacity() - machine.energy().amount();
                let moved = generator.min(room);
                machine
                    .energy()
                    .set_energy(machine.energy().amount() + moved, None);
                generator -= moved;
            }
            // O primeiro tick da máquina: o port antigo só então recalcula a capacidade.
            if when == Capacity::FirstTick && since == 0 {
                machine.apply_item_components(&upgrades);
            }
            let tick = &line["after_tick"];
            let got = (
                generator,
                machine.energy().amount(),
                machine.energy().capacity(),
            );
            let want = (
                number(tick, "generator"),
                number(tick, "energy"),
                number(tick, "capacity"),
            );
            if got != want {
                wrong.push(format!("{label} depois do tick {since}: (gerador, energia, capacidade) {got:?}, o rastro diz {want:?}"));
            }
            next += 1;
        }
        index = next;
    }
    assert_eq!(placements, PLACEMENTS, "{name}: colocações no rastro");
    wrong
}

#[test]
fn the_capacity_of_the_item_is_there_when_the_generator_fills_the_machine() {
    for name in MACHINES {
        let wrong = replay(name, Capacity::Placement);
        assert!(
            wrong.is_empty(),
            "{name}: {} diferenças\n{}",
            wrong.len(),
            wrong
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn the_constructor_capacity_until_the_first_tick_would_not_match_the_original() {
    // A mutação: a capacidade do bloco (quatro ticks de consumo) até o primeiro tick. O rastro tem de pegá-la nas quatro máquinas.
    for name in MACHINES {
        assert!(
            !replay(name, Capacity::FirstTick).is_empty(),
            "{name}: o rastro não distingue a capacidade da colocação da do primeiro tick"
        );
    }
}
