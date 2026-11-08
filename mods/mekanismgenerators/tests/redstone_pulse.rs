//! O Heat Generator recusa o PULSE como as máquinas: `TileEntityGenerator` não sobrescreve `ITileRedstone.supportsMode`, que devolve
//! `mode != PULSE`, e o tile cai em DISABLED em `setControlType`, no NBT salvo e no componente do item
//! (`dev/referencia/rastros/redstone_pulse.jsonl`, de `WarningsTraceTest.redstonePulse`).
use machinery_basis_core::tile::{RedstoneControl, SavedState};
use mekanismgenerators_rules::State;
use serde_json::Value;
use std::path::Path;

fn lines() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/redstone_pulse.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &Value| line["machine"] == "heat_generator")
        .collect()
}

#[test]
fn the_heat_generator_refuses_pulse_in_the_three_ways_the_original_does() {
    let lines = lines();
    assert_eq!(lines.len(), 8, "{lines:?}");
    for line in &lines {
        assert_eq!(line["supports_redstone"], true);
        assert_eq!(line["supports_pulse"], false);
        assert_eq!(line["mode"], "disabled", "{line}");
        let mut state = State::new(300.0);
        let before = match line["before"].as_str().unwrap() {
            "" | "pulse" => RedstoneControl::Disabled,
            other => RedstoneControl::from_serialized_name(other),
        };
        state.tile.set_control_type(before);
        match line["route"].as_str().unwrap() {
            "set" | "component" => state.tile.set_control_type(RedstoneControl::Pulse),
            "tag" => state.tile.restore(
                &SavedState {
                    control_type: RedstoneControl::from_serialized_name("pulse"),
                    ..SavedState::default()
                },
                false,
            ),
            "placed" => {}
            other => panic!("caminho {other}"),
        }
        assert_eq!(
            state.tile.control_type().serialized_name(),
            line["mode"].as_str().unwrap(),
            "{line}"
        );
    }
}
