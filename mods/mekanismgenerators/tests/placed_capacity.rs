//! O Heat Generator tem um contêiner de energia comum (`BasicEnergyContainer.output`), sem `MachineEnergyContainer`: nenhum
//! `recalculateUpgrades` mexe na capacidade, então ela é a mesma posta por gabarito, por um jogador (com e sem o upgrade de silêncio no
//! item) e lida do disco (`dev/referencia/rastros/placed_heat_generator.jsonl`, escrito por `PlacedTraceTest.java`).
use mekanismgenerators_rules::ENERGY_CAPACITY;
use serde_json::Value;
use std::path::Path;

#[test]
fn the_capacity_of_the_heat_generator_does_not_depend_on_how_it_was_set() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/placed_heat_generator.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let hows: Vec<&str> = rows
        .iter()
        .map(|row| row["how"].as_str().unwrap())
        .collect();
    for expected in [
        "template",
        "template_loaded",
        "use_on",
        "use_on_loaded",
        "use_on_muffling",
        "use_on_muffling_loaded",
    ] {
        assert!(hows.contains(&expected), "falta {expected}: {hows:?}");
    }
    for row in &rows {
        assert_eq!(row["max_energy"], ENERGY_CAPACITY, "{row}");
        assert_eq!(row["capacity_getter"], ENERGY_CAPACITY, "{row}");
    }
}
