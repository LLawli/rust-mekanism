//! O Heat Generator não tem configuração de lados (`TileEntityGenerator` estende `TileEntityMekanism`): o rastro `config_lados_caps` grava as
//! faces dele nas quatro frentes, posto por `setBlock` e por um jogador, e o port as dá pela frente (`item_slot_of`, `has_energy_capability`).
use machinery_basis_core::sides::Face;
use mekanismgenerators_rules::{has_energy_capability, item_slot_of};
use serde_json::Value;
use std::path::Path;

const FACES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

#[test]
fn the_heat_generator_faces_follow_its_front() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/config_lados_caps.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let mut compared = 0;
    for line in text.lines() {
        let line: Value = serde_json::from_str(line).unwrap();
        if line["machine"] != "heat_generator" {
            continue;
        }
        let front = Face::from_property(line["facing"].as_str().unwrap()).unwrap();
        for name in FACES {
            let face = Face::from_property(name).unwrap();
            let slots: Vec<u64> = line["item"][name]["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_u64().unwrap())
                .collect();
            assert_eq!(
                slots,
                [item_slot_of(front, face) as u64],
                "{front:?} {name}"
            );
            assert_eq!(
                line["energy"][name]["present"],
                has_energy_capability(front, face),
                "{front:?} {name}"
            );
            compared += 1;
        }
    }
    assert!(compared >= 48, "só {compared} faces");
}
