//! A tela dos avisos sem o jogo: o que o servidor escreve nos data slots, o que o cliente lê deles e o que a tela decide
//! desenhar por máquina, contra os rastros do original.
use machinery_basis_core::data_slots;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::tile::{RedstoneControl, RedstoneSync};
use machinery_basis_core::warning::ErrorMask;
use mekanism_rules::{
    CHEMICAL_ID_SLOTS, ChemicalSlots, DataLayout, ELECTRIC_DATA, ELECTRIC_SCREEN,
    ELECTRIC_TRACKED_ERRORS, INFUSER_DATA, INFUSER_SCREEN, INFUSER_TRACKED_ERRORS, ScreenData,
    ScreenElement, ScreenSpec, WarningType, tab_tooltip,
};
use serde_json::Value;
use std::path::Path;

fn trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O que o Horizon faz com um data slot: manda 16 bits com sinal, e o cliente lê o que chega.
fn over_the_wire(data: &[i32]) -> Vec<i32> {
    data.iter().map(|&value| i32::from(value as i16)).collect()
}

fn full() -> ScreenData {
    ScreenData {
        energy: 256_000_000,
        capacity: 256_000_000,
        progress: 199,
        ticks_required: 100,
        chemical_amount: 1_000,
        chemical_tint: 0xFF_59_C1_5F,
        chemical_icon: 2,
        chemical_id: data_slots::pack_text("mekanism:bio"),
        errors: ErrorMask::from_flags(&[true, false, true, true, true]),
        redstone: RedstoneSync {
            control_type: RedstoneControl::Low,
            powered: true,
            was_powered: true,
        },
    }
}

#[test]
fn the_electric_layout_round_trips_through_sixteen_bits() {
    let data = ScreenData {
        chemical_amount: 0,
        chemical_tint: 0,
        chemical_icon: 0,
        chemical_id: [0; CHEMICAL_ID_SLOTS],
        ..full()
    };
    let written = data.write(&ELECTRIC_DATA);
    assert_eq!(written.len(), ELECTRIC_DATA.len);
    assert_eq!(
        ScreenData::read(&ELECTRIC_DATA, &over_the_wire(&written)),
        data
    );
}

#[test]
fn the_infuser_layout_round_trips_through_sixteen_bits() {
    let data = full();
    let written = data.write(&INFUSER_DATA);
    assert_eq!(written.len(), INFUSER_DATA.len);
    assert_eq!(
        ScreenData::read(&INFUSER_DATA, &over_the_wire(&written)),
        data
    );
}

#[test]
fn the_old_slots_keep_their_place() {
    // O que a tela lia antes dos avisos: energia (0 e 1), progresso e capacidade nas mesmas posições.
    assert_eq!(
        (
            ELECTRIC_DATA.energy,
            ELECTRIC_DATA.progress,
            ELECTRIC_DATA.capacity
        ),
        (0, 2, 3)
    );
    assert_eq!(
        INFUSER_DATA.chemical,
        Some(ChemicalSlots {
            amount: 2,
            tint: 3,
            icon: 11,
            id: 12
        })
    );
    assert_eq!(
        (
            INFUSER_DATA.energy,
            INFUSER_DATA.progress,
            INFUSER_DATA.capacity
        ),
        (0, 5, 6)
    );
}

/// Cada valor tem os seus slots, sem sobreposição, e todos cabem no que o menu declara.
fn assert_layout_is_disjoint(layout: &DataLayout) {
    let mut used = vec![
        layout.energy,
        layout.energy + 1,
        layout.progress,
        layout.capacity,
    ];
    used.push(layout.capacity + 1);
    used.push(layout.errors);
    used.push(layout.redstone);
    used.push(layout.ticks_required);
    if let Some(chemical) = layout.chemical {
        used.extend([
            chemical.amount,
            chemical.tint,
            chemical.tint + 1,
            chemical.icon,
        ]);
        used.extend(chemical.id..chemical.id + CHEMICAL_ID_SLOTS);
    }
    let mut sorted = used.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), used.len(), "slots repetidos: {used:?}");
    assert_eq!(
        *sorted.last().unwrap() + 1,
        layout.len,
        "sobra ou falta slot: {used:?}"
    );
}

#[test]
fn the_layouts_use_every_slot_once() {
    assert_layout_is_disjoint(&ELECTRIC_DATA);
    assert_layout_is_disjoint(&INFUSER_DATA);
}

#[test]
fn the_menu_files_declare_what_the_layouts_write() {
    // O Horizon corta `entity.data` para `data_slots`: um slot a mais no código e não no arquivo é um valor que não chega.
    let menus = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/horizon/menu");
    let machines = [
        ("enrichment_chamber", ELECTRIC_DATA.len),
        ("crusher", ELECTRIC_DATA.len),
        ("energized_smelter", ELECTRIC_DATA.len),
        ("metallurgic_infuser", INFUSER_DATA.len),
    ];
    for (name, expected) in machines {
        let file = std::fs::read_to_string(menus.join(format!("{name}.json"))).unwrap();
        let menu: Value = serde_json::from_str(&file).unwrap();
        assert_eq!(menu["data_slots"], expected, "{name}");
    }
}

#[test]
fn a_menu_without_values_yet_shows_nothing() {
    for spec in [ELECTRIC_SCREEN, INFUSER_SCREEN] {
        let view = spec.view(&[]);
        assert!(!view.tab());
        assert!(view.issues.is_empty());
        for element in ALL_ELEMENTS {
            assert!(!view.element(element));
        }
    }
}

const ALL_ELEMENTS: [ScreenElement; 5] = [
    ScreenElement::EnergyBar,
    ScreenElement::Progress,
    ScreenElement::ChemicalBar,
    ScreenElement::InputSlot,
    ScreenElement::OutputSlot,
];

fn view_of(spec: &ScreenSpec, data: &ScreenData) -> mekanism_rules::WarningView {
    spec.view(&over_the_wire(&data.write(spec.data)))
}

fn with_errors(tracked: &[RecipeError], present: &[RecipeError]) -> ErrorMask {
    let flags: Vec<bool> = tracked
        .iter()
        .map(|error| present.contains(error))
        .collect();
    ErrorMask::from_flags(&flags)
}

/// Cada erro liga o elemento que o Java registra para ele, e só esse. A aba lista o tipo.
#[test]
fn each_error_lights_its_own_element_in_the_electric_machines() {
    let cases = [
        (
            RecipeError::NOT_ENOUGH_ENERGY,
            ScreenElement::EnergyBar,
            WarningType::NotEnoughEnergy,
        ),
        (
            RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
            ScreenElement::Progress,
            WarningType::InputDoesntProduceOutput,
        ),
        (
            RecipeError::NOT_ENOUGH_INPUT,
            ScreenElement::InputSlot,
            WarningType::NoMatchingRecipe,
        ),
        (
            RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
            ScreenElement::OutputSlot,
            WarningType::NoSpaceInOutput,
        ),
    ];
    for (error, element, kind) in cases {
        let data = ScreenData {
            errors: with_errors(&ELECTRIC_TRACKED_ERRORS, &[error]),
            ..ScreenData::default()
        };
        let view = view_of(&ELECTRIC_SCREEN, &data);
        assert!(view.tab(), "{error:?}");
        assert_eq!(view.issues, [kind], "{error:?}");
        for other in ALL_ELEMENTS {
            assert_eq!(
                view.element(other),
                other == element,
                "{error:?} em {other:?}"
            );
        }
    }
}

#[test]
fn the_infuser_lights_the_chemical_bar_for_the_secondary_input_only() {
    let secondary = ScreenData {
        errors: with_errors(
            &INFUSER_TRACKED_ERRORS,
            &[RecipeError::NOT_ENOUGH_SECONDARY_INPUT],
        ),
        ..ScreenData::default()
    };
    let view = view_of(&INFUSER_SCREEN, &secondary);
    assert_eq!(view.issues, [WarningType::NoMatchingRecipe]);
    assert!(view.element(ScreenElement::ChemicalBar));
    assert!(!view.element(ScreenElement::InputSlot));
    // O erro de entrada acende o slot, não a barra, e o tipo na aba é o mesmo.
    let input = ScreenData {
        errors: with_errors(&INFUSER_TRACKED_ERRORS, &[RecipeError::NOT_ENOUGH_INPUT]),
        ..ScreenData::default()
    };
    let view = view_of(&INFUSER_SCREEN, &input);
    assert_eq!(view.issues, [WarningType::NoMatchingRecipe]);
    assert!(view.element(ScreenElement::InputSlot));
    assert!(!view.element(ScreenElement::ChemicalBar));
}

/// As máquinas elétricas não têm barra de químico: o erro do secundário não acende nada nelas.
#[test]
fn the_electric_machines_have_no_chemical_bar() {
    let data = ScreenData {
        errors: ErrorMask::from_flags(&[true, true, true, true, true]),
        ..ScreenData::default()
    };
    assert!(!view_of(&ELECTRIC_SCREEN, &data).element(ScreenElement::ChemicalBar));
}

fn redstone(control_type: RedstoneControl, powered: bool) -> ScreenData {
    ScreenData {
        redstone: RedstoneSync {
            control_type,
            powered,
            was_powered: powered,
        },
        ..ScreenData::default()
    }
}

/// A aba aparece pelo redstone, mas nenhum elemento desenhado acende: o elemento do redstone é a aba lateral dele, que a tela ainda
/// não tem.
#[test]
fn redstone_waiting_shows_the_tab_and_lights_no_element() {
    for spec in [ELECTRIC_SCREEN, INFUSER_SCREEN] {
        let high = view_of(&spec, &redstone(RedstoneControl::High, false));
        assert!(high.tab());
        assert_eq!(high.issues, [WarningType::RedstoneSignalAbsent]);
        let low = view_of(&spec, &redstone(RedstoneControl::Low, true));
        assert_eq!(low.issues, [WarningType::RedstoneSignalPresent]);
        for element in ALL_ELEMENTS {
            assert!(!high.element(element) && !low.element(element));
        }
        // Com o sinal que o modo pede, ou sem modo, nada.
        assert!(!view_of(&spec, &redstone(RedstoneControl::High, true)).tab());
        assert!(!view_of(&spec, &redstone(RedstoneControl::Low, false)).tab());
        assert!(!view_of(&spec, &redstone(RedstoneControl::Disabled, false)).tab());
    }
}

/// Sem energia e sem sinal: o aviso de energia fecha a lista, e o de redstone não aparece no tooltip. A aba e a barra, sim.
#[test]
fn no_energy_hides_the_redstone_line_but_not_the_tab() {
    let data = ScreenData {
        errors: with_errors(&ELECTRIC_TRACKED_ERRORS, &[RecipeError::NOT_ENOUGH_ENERGY]),
        ..redstone(RedstoneControl::High, false)
    };
    let view = view_of(&ELECTRIC_SCREEN, &data);
    assert!(view.tab());
    assert_eq!(view.issues, [WarningType::NotEnoughEnergy]);
    assert!(view.element(ScreenElement::EnergyBar));
}

fn error_named(name: &str) -> Option<RecipeError> {
    Some(match name {
        "energy" => RecipeError::NOT_ENOUGH_ENERGY,
        "input" => RecipeError::NOT_ENOUGH_INPUT,
        "output_space" => RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
        "wrong_output" => RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
        "secondary" => RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
        _ => return None,
    })
}

/// O que o rastro do original diz que está em aviso, passado pelos data slots: a aba e a lista que `getWarnings` devolve batem
/// com as dele, e cada elemento acende quando o erro dele está no rastro.
#[test]
fn the_view_from_the_data_slots_matches_the_original_traces() {
    let scenarios = [
        "chamber_warnings_redstone",
        "chamber_recheck_alone",
        "chamber_recheck_changes_errors",
        "chamber_recheck_changes_errors_alone",
        "chamber_recheck_looking",
        "chamber_recheck_open_close",
        "chamber_reduced_rate",
    ];
    let (mut compared, mut tabs, mut hidden, mut energy_bars, mut slots) = (0, 0, 0, 0, 0);
    for scenario in scenarios {
        for line in trace(scenario) {
            let Some(shown) = line.get("warnings_shown").and_then(Value::as_array) else {
                continue;
            };
            let present: Vec<RecipeError> = line["errors"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|name| error_named(name.as_str().unwrap()))
                .collect();
            let control = match line["mode"].as_str().unwrap() {
                "high" => RedstoneControl::High,
                "low" => RedstoneControl::Low,
                "pulse" => RedstoneControl::Pulse,
                _ => RedstoneControl::Disabled,
            };
            let powered = line["powered"].as_bool().unwrap();
            let data = ScreenData {
                errors: with_errors(&ELECTRIC_TRACKED_ERRORS, &present),
                ..redstone(control, powered)
            };
            let view = view_of(&ELECTRIC_SCREEN, &data);
            let context = format!("{scenario}, tick {}", line["t"]);
            let names: Vec<&str> = view.issues.iter().map(|kind| kind.name()).collect();
            let expected: Vec<&str> = shown.iter().map(|name| name.as_str().unwrap()).collect();
            assert_eq!(names, expected, "{context}: getWarnings");
            let triggered = line["warnings"].as_array().unwrap();
            assert_eq!(view.tab(), !triggered.is_empty(), "{context}: aba");
            for (element, error) in [
                (ScreenElement::EnergyBar, RecipeError::NOT_ENOUGH_ENERGY),
                (
                    ScreenElement::Progress,
                    RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
                ),
                (ScreenElement::InputSlot, RecipeError::NOT_ENOUGH_INPUT),
                (
                    ScreenElement::OutputSlot,
                    RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
                ),
            ] {
                assert_eq!(
                    view.element(element),
                    present.contains(&error),
                    "{context}: {element:?}"
                );
            }
            compared += 1;
            tabs += usize::from(view.tab());
            hidden += usize::from(
                present.contains(&RecipeError::NOT_ENOUGH_ENERGY)
                    && triggered
                        .iter()
                        .any(|kind| kind == "redstone_signal_absent")
                    && !shown.iter().any(|kind| kind == "redstone_signal_absent"),
            );
            energy_bars += usize::from(view.element(ScreenElement::EnergyBar));
            slots += usize::from(
                view.element(ScreenElement::InputSlot) || view.element(ScreenElement::OutputSlot),
            );
        }
    }
    // Os rastros têm de exercitar o que a comparação prova: com e sem aba, o corte da energia, a barra e os slots.
    assert!(compared >= 1_000, "{compared}");
    assert!(tabs > 0 && tabs < compared, "{tabs}");
    assert!(hidden > 0, "o corte da energia não aparece nos rastros");
    assert!(energy_bars > 0, "nenhum rastro acende a barra de energia");
    assert!(slots > 0, "nenhum rastro acende um slot");
}

/// Largura de um texto monoespaçado em seis pixels por caractere.
fn six(text: &str) -> f32 {
    text.chars().count() as f32 * 6.0
}

fn english(key: &str) -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    let lang: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    lang[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

#[test]
fn the_tab_tooltip_has_the_yellow_header_and_one_wrapped_block_per_issue() {
    let lines = tab_tooltip(
        &[WarningType::NoMatchingRecipe, WarningType::NoSpaceInOutput],
        &english,
        &six,
    );
    assert_eq!(lines[0].text, "Issues:");
    assert_eq!(lines[0].color, Some(0xFF_DD_4F));
    // Nenhuma linha passa dos 170 pixels do tooltip, e quebrar não perde texto: as linhas, com o espaço da quebra, refazem a frase.
    let body: Vec<_> = lines[1..].iter().collect();
    assert!(
        body.iter()
            .all(|line| line.color.is_none() && six(&line.text) <= 170.0)
    );
    let joined = body
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        joined,
        format!(
            "{} {}",
            english("gui.mekanism.issues.no_recipe"),
            english("gui.mekanism.issues.no_space")
        )
    );
    // E a primeira frase, de 41 caracteres a seis pixels, de fato quebrou.
    assert!(body.len() > 2);
}
