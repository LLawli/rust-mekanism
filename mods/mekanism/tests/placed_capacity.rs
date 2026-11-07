//! Repete o que o Java mediu da capacidade de energia, do consumo e dos ticks de uma máquina conforme o jeito de ela chegar ao mundo
//! (`dev/referencia/rastros/placed_<máquina>.jsonl`, escrito por `PlacedTraceTest.java`):
//!
//! - `template`: o `setBlock` de um gabarito de teste, de um comando ou de uma estrutura. O construtor cria o contêiner com
//!   `AttributeEnergy.getStorage()`, quatro ticks de consumo, e nada recalcula.
//! - `use_on`: um jogador põe o item. `applyImplicitComponents` recalcula cada tipo de upgrade que a máquina aceita (`recalculateUpgrades`
//!   → `updateMaxEnergy`), então a capacidade passa a ser a dos ticks da receita: 1.000 numa Enrichment Chamber sem upgrade.
//! - `*_loaded`: o tile lido do NBT (`BlockEntity.loadStatic`, o chunk que carrega). `ITileComponent.read` é `input.readChild(chave,
//!   this)`: só chama `deserialize` se o filho `component_upgrade` está no NBT, e o `write` apaga o filho quando não há upgrade
//!   nem item. Uma máquina sem nada guardado volta com a capacidade do construtor.
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
/// Os pares (velocidade, energia) que o item traz, e a máquina sem upgrade.
const COMBOS: usize = 9;

fn rows(machine: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("placed_{machine}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn number(row: &Value, key: &str) -> i64 {
    row[key]
        .as_i64()
        .unwrap_or_else(|| panic!("campo {key} ausente em {row}"))
}

/// O que o item traz: `UpgradeAware.upgrades` com os totais que não são zero.
fn item_upgrades(row: &Value) -> UpgradeSaved {
    let mut saved = UpgradeSaved::default();
    for (id, key) in [(SPEED_UPGRADE, "speed"), (ENERGY_UPGRADE, "energy")] {
        let total = number(row, key) as i32;
        if total > 0 {
            saved.upgrades.insert(id.to_owned(), total);
        }
    }
    saved
}

/// Capacidade (sem estoque, é a capacidade de fato), consumo por tick e ticks por operação.
fn measured(machine: &ProgressMachine) -> (i64, i64, i64) {
    assert_eq!(machine.energy().amount(), 0, "sem estoque");
    (
        machine.energy().capacity(),
        i64::from(machine.energy().energy_per_tick()),
        i64::from(machine.ticks_required()),
    )
}

fn java(row: &Value) -> (i64, i64, i64) {
    (
        number(row, "max_energy"),
        number(row, "energy_per_tick"),
        number(row, "ticks_required"),
    )
}

/// A máquina como o jogo a deixa depois de um `setBlock`: só o construtor.
fn template(name: &str) -> ProgressMachine {
    common::bare_machine(name, &common::upgrade_data())
}

/// A máquina como um jogador a põe, com o que o item traz.
fn placed(name: &str, item: &UpgradeSaved) -> ProgressMachine {
    let mut machine = template(name);
    machine.apply_item_components(item);
    machine
}

fn rows_of<'a>(rows: &'a [Value], how: &str) -> Vec<&'a Value> {
    rows.iter().filter(|row| row["how"] == how).collect()
}

#[test]
fn a_machine_set_by_setblock_keeps_the_constructor_capacity() {
    for name in MACHINES {
        let all = rows(name);
        let found = rows_of(&all, "template");
        assert_eq!(found.len(), 1, "{name}: uma linha de gabarito");
        assert_eq!(measured(&template(name)), java(found[0]), "{name}");
        // O número que importa: quatro ticks de consumo, e não 1.000.
        assert_eq!(java(found[0]).0, 4 * java(found[0]).1, "{name}");
    }
}

#[test]
fn a_machine_placed_by_a_player_recalculates_with_what_the_item_carries() {
    for name in MACHINES {
        let all = rows(name);
        let found = rows_of(&all, "use_on");
        assert_eq!(found.len(), COMBOS, "{name}: pares de upgrade do item");
        for row in found {
            assert_eq!(
                measured(&placed(name, &item_upgrades(row))),
                java(row),
                "{name} velocidade {} energia {}",
                row["speed"],
                row["energy"]
            );
        }
    }
}

/// O jogador que põe a máquina sem upgrade tem 1.000 (e 1.600 no Crusher), 50 vezes o do gabarito.
#[test]
fn the_plain_machine_of_a_player_holds_the_ticks_of_its_recipe() {
    let expected = [
        ("enrichment_chamber", 1_000),
        ("crusher", 1_600),
        ("energized_smelter", 1_000),
        ("metallurgic_infuser", 1_000),
    ];
    for (name, capacity) in expected {
        let all = rows(name);
        let plain = rows_of(&all, "use_on")
            .into_iter()
            .find(|row| number(row, "speed") == 0 && number(row, "energy") == 0)
            .expect("a máquina sem upgrade");
        assert_eq!(number(plain, "max_energy"), capacity, "{name}: Java");
        assert_eq!(
            measured(&placed(name, &UpgradeSaved::default())).0,
            capacity,
            "{name}: port"
        );
    }
}

#[test]
fn a_machine_read_from_disk_recalculates_only_when_the_upgrade_child_was_saved() {
    for name in MACHINES {
        let all = rows(name);
        let mut seen = 0;
        for how in ["template_loaded", "use_on_loaded"] {
            for row in rows_of(&all, how) {
                // O NBT que o Java gravou: de uma máquina de `setBlock` ou de uma posta com o que o item trazia.
                let source = if how == "template_loaded" {
                    template(name)
                } else {
                    placed(name, &item_upgrades(row))
                };
                let saved = source.saved_state();
                let child_saved = row["component_upgrade"].as_str().unwrap() != "";
                assert_eq!(
                    !saved.upgrades.is_empty(),
                    child_saved,
                    "{name} {how} {}/{}: o filho `component_upgrade` só existe com upgrade ou item",
                    row["speed"],
                    row["energy"]
                );
                let mut loaded = template(name);
                loaded.restore(&saved, false);
                assert_eq!(
                    measured(&loaded),
                    java(row),
                    "{name} {how} velocidade {} energia {}",
                    row["speed"],
                    row["energy"]
                );
                seen += 1;
            }
        }
        assert_eq!(seen, COMBOS + 1, "{name}: linhas lidas do disco");
    }
}

/// O caso que a regra existe para pegar: lida do disco sem nada guardado, a máquina que o jogador pôs volta com 4 ticks de consumo.
#[test]
fn a_plain_machine_of_a_player_falls_back_to_the_constructor_capacity_when_the_chunk_reloads() {
    for name in MACHINES {
        let all = rows(name);
        let plain = rows_of(&all, "use_on_loaded")
            .into_iter()
            .find(|row| number(row, "speed") == 0 && number(row, "energy") == 0)
            .expect("a máquina sem upgrade, lida do disco");
        let base = number(plain, "energy_per_tick");
        assert_eq!(number(plain, "max_energy"), 4 * base, "{name}: Java");
        let mut loaded = template(name);
        loaded.restore(&placed(name, &UpgradeSaved::default()).saved_state(), false);
        assert_eq!(measured(&loaded).0, 4 * base, "{name}: port");
    }
}

/// O que a capacidade muda na prática: o slot de energia só converte o `redstone` (10.000) se o contêiner aceita tudo, e o
/// `redstone_block` (90.000) se aceita tudo também. Com a capacidade de uma máquina posta sem upgrade (1.000) nenhum dos dois converte,
/// mas com velocidade 8 a Enrichment Chamber, o Energized Smelter e o Metallurgic Infuser têm 10.000, e o Crusher 16.000 (e dois
/// outros pares de upgrade chegam a mais de 10.000): lá o `redstone` converte, com o contêiner vazio. O bloco nunca (o maior é 16.000).
/// Cada linha de `upgrade_table.jsonl` (324 pares, medidos no Java) diz a capacidade; o port tem de converter exatamente onde ela chega
/// a 10.000.
#[test]
fn redstone_converts_exactly_where_the_java_capacity_reaches_ten_thousand() {
    const REDSTONE: i64 = 10_000;
    const BLOCK: i64 = 90_000;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/upgrade_table.jsonl");
    let table: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect();
    assert_eq!(table.len(), 324);
    let mut converting = std::collections::BTreeSet::new();
    for row in &table {
        let name = row["machine"].as_str().unwrap();
        let (speed, energy) = (
            number(row, "speed_upgrades") as i32,
            number(row, "energy_upgrades") as i32,
        );
        let mut item = UpgradeSaved::default();
        for (id, total) in [(SPEED_UPGRADE, speed), (ENERGY_UPGRADE, energy)] {
            if total > 0 {
                item.upgrades.insert(id.to_owned(), total);
            }
        }
        for (stack, output) in [("redstone", REDSTONE), ("redstone_block", BLOCK)] {
            let mut machine = template(name);
            machine.apply_item_components(&item);
            assert_eq!(
                machine.energy().capacity(),
                number(row, "max_energy"),
                "{name} {speed}/{energy}"
            );
            machine
                .energy_slot()
                .set_contents(Some(common::item(stack)), 1, None);
            machine.tick_server(0, &machinery_basis_core::item_handler::NoNeighbours);
            let gained = machine.energy().amount();
            if gained > 0 {
                assert_eq!(gained, output, "{name} {speed}/{energy} {stack}");
                converting.insert((name.to_owned(), speed, energy, stack.to_owned()));
            }
            assert_eq!(
                gained > 0,
                number(row, "max_energy") >= output,
                "{name} {speed}/{energy} {stack}: converte onde a capacidade chega a {output}"
            );
        }
    }
    // Os pares do Java que chegam a 10.000, e nenhum que chegue a 90.000.
    let expected: Vec<(String, i32, i32, String)> = table
        .iter()
        .filter(|row| number(row, "max_energy") >= REDSTONE)
        .map(|row| {
            (
                row["machine"].as_str().unwrap().to_owned(),
                number(row, "speed_upgrades") as i32,
                number(row, "energy_upgrades") as i32,
                "redstone".to_owned(),
            )
        })
        .collect();
    assert_eq!(converting.into_iter().collect::<Vec<_>>(), {
        let mut sorted = expected;
        sorted.sort();
        sorted
    });
}

/// A tabela de upgrades (`upgrade_table.jsonl`, medida com `recalculateUpgrades` à mão) e a máquina que um jogador põe com o item (`use_on`)
/// dizem o mesmo nos 9 pares que as duas têm: a tabela vale para a máquina do jogador, e não para a do gabarito.
#[test]
fn the_upgrade_table_is_the_table_of_the_machine_a_player_places() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/upgrade_table.jsonl");
    let table: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut compared = 0;
    for name in MACHINES {
        for row in rows(name).iter().filter(|row| row["how"] == "use_on") {
            let found = table
                .iter()
                .find(|entry| {
                    entry["machine"] == name
                        && entry["speed_upgrades"] == row["speed"]
                        && entry["energy_upgrades"] == row["energy"]
                })
                .unwrap_or_else(|| panic!("{name}: {row} não está na tabela"));
            assert_eq!(java(found), java(row), "{name} {row}");
            compared += 1;
        }
    }
    assert_eq!(compared, MACHINES.len() * COMBOS);
}
