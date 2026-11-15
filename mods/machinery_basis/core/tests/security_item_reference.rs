//! O dono e o modo de uma máquina posta, contra o original: `dev/referencia/rastros/item_content_<máquina>.jsonl` (`ItemContentTraceTest.java`) traz, para as quatro
//! máquinas e o Heat Generator, o NBT do tile (`component_security` e a frequência de segurança em `component_frequency`), o item solto (`mekanism:owner`, e nenhum
//! `mekanism:security` com o modo `PUBLIC`, o padrão do item) e as duas linhas do tooltip (`Owner:` e `Security:`) com as cores. O modo `PRIVATE` e `TRUSTED` e o acesso
//! estão em `security_reference.rs` (`SecurityTraceTest.java`).
use machinery_basis_core::security::{
    self as sec, Arg, SecurityConfig, SecurityMode, SecurityState,
};
use serde_json::Value;
use std::path::PathBuf;

const MACHINES: [&str; 5] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
    "heat_generator",
];

fn rows(machine: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/item_content_{machine}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn color_of(value: &Value) -> u32 {
    let text = value.as_str().expect("cor");
    u32::from_str_radix(text.trim_start_matches('#'), 16).expect("cor em hexadecimal")
}

#[test]
fn the_saved_security_of_every_row_is_what_the_original_wrote() {
    for machine in MACHINES {
        for row in rows(machine) {
            let nbt = &row["tile"]["nbt"];
            let state = sec::deserialize(nbt.get(sec::COMPONENT_SECURITY));
            assert!(
                state.owner.is_some(),
                "{machine}: o tile do rastro tem dono"
            );
            assert_eq!(state.mode, SecurityMode::Public, "{machine}");
            let (security, frequency) = sec::serialize(&state);
            assert_eq!(
                security.as_ref(),
                nbt.get(sec::COMPONENT_SECURITY),
                "{machine} {}",
                row["scenario"]
            );
            assert_eq!(
                frequency.as_ref(),
                nbt.get(sec::COMPONENT_FREQUENCY),
                "{machine} {}",
                row["scenario"]
            );
        }
    }
}

#[test]
fn the_dropped_item_carries_the_owner_and_no_default_mode() {
    for machine in MACHINES {
        for row in rows(machine) {
            let state = sec::deserialize(row["tile"]["nbt"].get(sec::COMPONENT_SECURITY));
            let (owner, mode) = sec::collect(&state);
            let components = &row["drops"][0]["stack"]["components"];
            assert_eq!(
                owner.as_ref(),
                components.get(sec::OWNER_COMPONENT),
                "{machine} {}",
                row["scenario"]
            );
            assert_eq!(
                mode.as_ref(),
                components.get(sec::SECURITY_COMPONENT),
                "{machine} {}",
                row["scenario"]
            );
        }
    }
}

#[test]
fn the_item_placed_back_gives_the_same_owner_and_mode() {
    for machine in MACHINES {
        let rows = rows(machine);
        let full = &rows[2];
        let round = &rows[3];
        let components = &full["drops"][0]["stack"]["components"];
        let placed = sec::apply(
            SecurityState::default(),
            components.get(sec::OWNER_COMPONENT),
            components.get(sec::SECURITY_COMPONENT),
        );
        assert_eq!(
            Some(placed),
            Some(sec::deserialize(
                round["tile"]["nbt"].get(sec::COMPONENT_SECURITY)
            )),
            "{machine}"
        );
    }
}

#[test]
fn the_tooltip_lines_are_the_owner_then_the_security_with_the_original_colors() {
    for machine in MACHINES {
        for row in rows(machine) {
            let state = sec::deserialize(row["tile"]["nbt"].get(sec::COMPONENT_SECURITY));
            let owner = state.owner.expect("dono");
            let tooltip = row["drops"][0]["tooltip"].as_array().expect("tooltip");
            let lines = sec::tooltip_lines(
                SecurityConfig::default(),
                Some(owner),
                Some((owner, "test-mock-player")),
                state.mode,
            );
            assert_eq!(lines.len(), 2);
            for (line, original) in lines.iter().zip(&tooltip[1..3]) {
                let component = &original["component"];
                assert_eq!(component["translate"], line.key, "{machine}");
                assert_eq!(
                    color_of(&component["color"]),
                    line.color,
                    "{machine} {}",
                    line.key
                );
                let arg = &component["with"][0];
                assert_eq!(
                    color_of(&arg["color"]),
                    line.arg_color,
                    "{machine} {}",
                    line.key
                );
                match &line.arg {
                    Arg::Literal(name) => assert_eq!(arg["text"], name.as_str()),
                    Arg::Translate(key) => assert_eq!(arg["translate"], *key),
                }
            }
        }
    }
}
