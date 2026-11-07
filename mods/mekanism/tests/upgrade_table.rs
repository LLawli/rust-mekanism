//! Confere o port com a tabela de upgrades medida no Java (`dev/referencia/rastros/upgrade_table.jsonl`, gravada pelo cenário
//! `upgradeTable` de `MachineTraceTest.java`). Cada linha de dados vira uma máquina nova, carregada como o tile faz, com a
//! energia e a velocidade da linha instaladas. Compara capacidade, consumo por tick e ticks com o que o Java leu.
mod common;

use machinery_basis_core::cached_recipe::ItemToItemRecipe;
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::upgrade::UpgradeData;
use mekanism_rules::{
    CRUSHER_USAGE, ENERGY_UPGRADE, ENRICHMENT_USAGE, INFUSER_USAGE, SMELTER_USAGE, SPEED_UPGRADE,
    electric_config, infuser_config, infusing_cache_of,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;

/// As quatro máquinas, cada uma com energia de 0 a 8 e velocidade de 0 a 8: 81 pares por máquina.
const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];
const EXPECTED_ROWS: usize = MACHINES.len() * 81;

/// Lê o `upgrade_table.jsonl` inteiro, com as linhas de meta (`upgrade_max`, `config`, `storage`, `consumption`) e as de dados.
fn all_rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/upgrade_table.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn field(row: &Value, key: &str) -> i64 {
    row[key]
        .as_i64()
        .unwrap_or_else(|| panic!("campo {key} ausente ou não inteiro em {row}"))
}

/// A máquina nova da lib, com os valores do mod: o uso de energia e a receita vazia (o upgrade não depende dela).
fn machine(name: &str, data: &Rc<UpgradeData>) -> ProgressMachine {
    match name {
        "enrichment_chamber" => ProgressMachine::electric(electric_config(
            ENRICHMENT_USAGE,
            ItemToItemRecipe::cache_of(Vec::new()),
            0,
            data.clone(),
        )),
        "crusher" => ProgressMachine::electric(electric_config(
            CRUSHER_USAGE,
            ItemToItemRecipe::cache_of(Vec::new()),
            0,
            data.clone(),
        )),
        "energized_smelter" => ProgressMachine::electric(electric_config(
            SMELTER_USAGE,
            ItemToItemRecipe::cache_of(Vec::new()),
            0,
            data.clone(),
        )),
        "metallurgic_infuser" => ProgressMachine::chemical(infuser_config(
            INFUSER_USAGE,
            infusing_cache_of(Vec::new()),
            ChemicalConversion::cache_of(Vec::new()),
            0,
            data.clone(),
        )),
        other => panic!("máquina fora da tabela: {other}"),
    }
}

/// Carrega a máquina como o tile faz ao ser colocado ou aberto: `restore` recalcula todos os tipos de upgrade com o total que
/// há, zero incluído, como o `deserialize` do Java. Sem isso a base seria a do construtor (4 vezes o uso), não a da máquina pronta.
fn loaded(name: &str, data: &Rc<UpgradeData>) -> ProgressMachine {
    let mut machine = machine(name, data);
    let saved = machine.saved_state();
    machine.restore(&saved, false);
    machine
}

/// Instala o total de um tipo, pelo caminho que o jogo usa (`addUpgrades` com a diferença).
fn install(machine: &ProgressMachine, upgrade: &str, total: i32) {
    let upgrades = machine.upgrades();
    upgrades.add_upgrades(upgrade, total - upgrades.get_upgrades(upgrade));
}

#[test]
fn the_port_reproduces_every_row_of_the_java_table() {
    let rows = all_rows();
    let data_rows: Vec<&Value> = rows
        .iter()
        .filter(|row| row.get("meta").is_none())
        .collect();
    assert_eq!(
        data_rows.len(),
        EXPECTED_ROWS,
        "a tabela do Java tem {EXPECTED_ROWS} linhas de dados, e o teste leu {}",
        data_rows.len()
    );

    let data = common::upgrade_data();
    let mut seen = BTreeSet::new();
    let mut mismatches = Vec::new();
    for row in &data_rows {
        let name = row["machine"].as_str().expect("máquina em texto");
        let energy = field(row, "energy_upgrades") as i32;
        let speed = field(row, "speed_upgrades") as i32;
        assert!(MACHINES.contains(&name), "máquina fora da tabela: {name}");
        assert!(
            seen.insert((name.to_owned(), energy, speed)),
            "par repetido na tabela: {name} energia {energy} velocidade {speed}"
        );

        let machine = loaded(name, &data);
        install(&machine, ENERGY_UPGRADE, energy);
        install(&machine, SPEED_UPGRADE, speed);
        // Sem estoque, `capacity()` é a capacidade máxima de fato: ele devolve o maior entre a capacidade e o que está guardado.
        assert_eq!(
            machine.energy().amount(),
            0,
            "o estoque tem de estar vazio para a capacidade ser a real"
        );

        let port = (
            machine.energy().capacity(),
            i64::from(machine.energy().energy_per_tick()),
            i64::from(machine.ticks_required()),
        );
        let java = (
            field(row, "max_energy"),
            field(row, "energy_per_tick"),
            field(row, "ticks_required"),
        );
        if port != java {
            mismatches.push(format!(
                "{name} energia {energy} velocidade {speed}: capacidade, consumo e ticks do Java {java:?}, do port {port:?}"
            ));
        }
    }
    assert_eq!(
        seen.len(),
        EXPECTED_ROWS,
        "os pares da tabela não cobrem as {EXPECTED_ROWS} combinações"
    );
    assert!(
        mismatches.is_empty(),
        "{} divergência(s) entre o port e o Java:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// As linhas `storage` do Java trazem a base de cada máquina: o uso, a capacidade e o consumo base, e os ticks base. O port tem de
/// dar os mesmos, sem nenhum upgrade.
#[test]
fn the_port_base_values_match_the_java_storage_lines() {
    let rows = all_rows();
    let storage: Vec<&Value> = rows.iter().filter(|row| row["meta"] == "storage").collect();
    assert_eq!(
        storage.len(),
        MACHINES.len(),
        "a tabela tem uma linha de armazenamento por máquina"
    );
    let data = common::upgrade_data();
    let mut mismatches = Vec::new();
    for row in storage {
        let name = row["machine"].as_str().expect("máquina em texto");
        let machine = loaded(name, &data);
        let port = (
            machine.energy().base_max_energy(),
            i64::from(machine.energy().energy_per_tick()),
            i64::from(machine.ticks_required()),
        );
        let java = (
            field(row, "base_max_energy"),
            field(row, "base_energy_per_tick"),
            field(row, "base_ticks_required"),
        );
        if port != java {
            mismatches.push(format!("{name}: base do Java {java:?}, do port {port:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} divergência(s) na base:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
