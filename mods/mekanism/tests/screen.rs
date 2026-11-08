//! A tela dos avisos sem o jogo: o que o servidor manda na lista de dados do menu, o que o cliente lê do espelho e o que a tela decide
//! desenhar por máquina, contra os rastros do original.
mod common;

use common::Live;
use machinery_basis_core::container_sync::{
    ContainerSync, DirtyType, Mirror, PropertyData, StackValue,
};
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::tile::{RedstoneControl, RedstoneSync};
use machinery_basis_core::tile_trackers::source;
use machinery_basis_core::warning::ErrorMask;
use mekanism_rules::{
    ELECTRIC_SCREEN, ELECTRIC_TRACKED_ERRORS, INFUSER_SCREEN, INFUSER_TRACKED_ERRORS, ScreenData,
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

fn bio(amount: u64) -> StackValue {
    StackValue {
        resource: "mekanism:bio".to_owned(),
        amount,
    }
}

/// O que o servidor lê da máquina, com todos os dados diferentes do padrão e valores grandes (energia acima de 65535).
fn full() -> Live {
    Live {
        energy: 256_000_000,
        capacity: 256_000_000,
        energy_per_tick: 70_000,
        progress: 199,
        ticks_required: 100,
        chemical: bio(1_000),
        errors: vec![true, false, true, true, true],
        control_type: RedstoneControl::Low,
        powered: true,
        was_powered: true,
        xp_containment: true,
    }
}

/// O que a tela lê de `live` quando o espelho tem `errors` erros rastreados e (ou não) um tanque.
fn read_of(live: &Live, errors: usize, tank: bool) -> ScreenData {
    ScreenData {
        energy: live.energy as u32,
        capacity: live.capacity as u32,
        progress: live.progress,
        ticks_required: live.ticks_required,
        chemical: if tank {
            live.chemical.clone()
        } else {
            StackValue::default()
        },
        errors: ErrorMask::from_flags(&live.errors[..errors]),
        redstone: RedstoneSync {
            control_type: live.control_type,
            powered: live.powered,
            was_powered: live.was_powered,
        },
        xp_containment: live.xp_containment,
        energy_per_tick: live.energy_per_tick as u32,
    }
}

#[test]
fn the_electric_screen_reads_from_the_mirror_what_the_server_reads() {
    let live = full();
    let mirror = live.mirror(&ELECTRIC_SCREEN);
    assert_eq!(
        ScreenData::read(&ELECTRIC_SCREEN, &mirror),
        read_of(&live, ELECTRIC_TRACKED_ERRORS.len(), false)
    );
}

#[test]
fn the_infuser_screen_reads_from_the_mirror_what_the_server_reads() {
    let live = full();
    let mirror = live.mirror(&INFUSER_SCREEN);
    assert_eq!(
        ScreenData::read(&INFUSER_SCREEN, &mirror),
        read_of(&live, INFUSER_TRACKED_ERRORS.len(), true)
    );
}

/// A energia acima de 65535, o `u32::MAX` e os inteiros negativos chegam à tela iguais ao que o servidor lê.
#[test]
fn big_and_negative_values_reach_the_screen_whole() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        let indexes = (spec.indexes)();
        for energy in [0, 1, 65_535, 65_536, 1_000_000, i64::from(u32::MAX)] {
            let live = Live {
                energy,
                capacity: i64::from(u32::MAX),
                ..Live::default()
            };
            let mirror = live.mirror(spec);
            let read = ScreenData::read(spec, &mirror);
            assert_eq!(i64::from(read.energy), energy);
            assert_eq!(read.capacity, u32::MAX);
        }
        // O espelho guarda o `long` inteiro; é a tela que o corta ao `u32` que desenha.
        let live = Live {
            energy: 10_000_000_000,
            capacity: -1,
            ..Live::default()
        };
        let mirror = live.mirror(spec);
        assert_eq!(mirror.long(indexes.energy), 10_000_000_000);
        assert_eq!(mirror.long(indexes.capacity), -1);
        let read = ScreenData::read(spec, &mirror);
        assert_eq!((read.energy, read.capacity), (u32::MAX, 0));
        // Os inteiros mantêm o sinal.
        for value in [-1, -32_769, i32::MIN, i32::MAX] {
            let live = Live {
                progress: value,
                ticks_required: value,
                energy_per_tick: value,
                ..Live::default()
            };
            let mirror = live.mirror(spec);
            assert_eq!(mirror.int(indexes.progress), value);
            assert_eq!(mirror.int(indexes.ticks_required), value);
            assert_eq!(mirror.int(indexes.energy_per_tick), value);
            let read = ScreenData::read(spec, &mirror);
            assert_eq!((read.progress, read.ticks_required), (value, value));
        }
    }
}

/// `handleWindowProperty` entrega o dado à posição dele pela classe que mora lá: um pacote de outra classe, ou de posição que o menu não
/// tem, não muda nada.
#[test]
fn a_packet_of_the_wrong_class_or_for_a_missing_position_is_ignored() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        let indexes = (spec.indexes)();
        let live = full();
        let mut mirror = live.mirror(spec);
        let before = ScreenData::read(spec, &mirror);
        let wrong = [
            (indexes.energy, PropertyData::Boolean(false)),
            (indexes.energy, PropertyData::Int(7)),
            (indexes.progress, PropertyData::Long(7)),
            (indexes.xp_containment, PropertyData::Int(0)),
            (indexes.control_type, PropertyData::Boolean(false)),
            (indexes.errors[0], PropertyData::Long(0)),
        ];
        for (index, data) in wrong {
            assert!(!mirror.apply(index as u16, &data), "{index}: {data:?}");
        }
        let missing = mirror.count() as u16;
        for index in [missing, missing + 1, u16::MAX] {
            assert!(!mirror.apply(index, &PropertyData::Long(7)), "{index}");
            assert!(
                !mirror.apply(index, &PropertyData::Boolean(true)),
                "{index}"
            );
        }
        assert_eq!(ScreenData::read(spec, &mirror), before);
    }
}

/// A lista que a tela lê é a que o servidor sincroniza: cada posição de `ScreenIndexes` guarda a fonte do dado que a tela espera, e
/// nenhuma se repete.
#[test]
fn each_screen_value_has_its_own_position_in_the_menu_list() {
    for (spec, tank, errors) in [
        (&ELECTRIC_SCREEN, false, ELECTRIC_TRACKED_ERRORS.len()),
        (&INFUSER_SCREEN, true, INFUSER_TRACKED_ERRORS.len()),
    ] {
        let list = (spec.trackers)();
        let indexes = (spec.indexes)();
        let mut used = vec![
            (indexes.control_type, source::CONTROL_TYPE),
            (indexes.powered, source::POWERED),
            (indexes.was_powered, source::WAS_POWERED),
            (indexes.capacity, source::MAX_ENERGY),
            (indexes.energy_per_tick, source::ENERGY_PER_TICK),
            (indexes.energy, source::ENERGY),
            (indexes.xp_containment, source::HAS_XP_CONTAINMENT),
            (indexes.progress, source::OPERATING_TICKS),
            (indexes.ticks_required, source::TICKS_REQUIRED),
        ];
        assert_eq!(indexes.errors.len(), errors);
        used.extend(
            indexes
                .errors
                .iter()
                .enumerate()
                .map(|(error, index)| (*index, source::TRACKED_ERROR + error as u16)),
        );
        assert_eq!(indexes.chemical.is_some(), tank);
        used.extend(indexes.chemical.map(|index| (index, source::CHEMICAL_TANK)));
        for (index, expected) in &used {
            assert_eq!(list[*index].source, *expected, "posição {index}");
        }
        let mut positions: Vec<usize> = used.iter().map(|(index, _)| *index).collect();
        positions.sort_unstable();
        positions.dedup();
        assert_eq!(positions.len(), used.len(), "posições repetidas: {used:?}");
    }
}

/// A aba de xp lê o `hasXpContainment` do servidor: um valor só, na posição dele, que sobe e desce sem mexer nos outros.
#[test]
fn the_xp_containment_flag_travels_in_its_own_position() {
    for spec in [&ELECTRIC_SCREEN, &INFUSER_SCREEN] {
        let off = ScreenData::read(
            spec,
            &Live {
                xp_containment: false,
                ..full()
            }
            .mirror(spec),
        );
        let on = ScreenData::read(
            spec,
            &Live {
                xp_containment: true,
                ..full()
            }
            .mirror(spec),
        );
        assert!(!off.xp_containment);
        assert!(on.xp_containment);
        assert_eq!(
            ScreenData {
                xp_containment: true,
                ..off
            },
            on
        );
    }
    // Antes de o servidor mandar o primeiro valor o espelho está vazio: sem upgrade, sem aba.
    let empty = Mirror::new(&(ELECTRIC_SCREEN.trackers)());
    assert!(!ScreenData::read(&ELECTRIC_SCREEN, &empty).xp_containment);
}

/// Os dados da tela viajam na lista do menu (`PacketUpdateContainer`), e os arquivos de menu do Horizon não declaram `data_slots`.
#[test]
fn the_menu_files_declare_no_data_slots() {
    let menus = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/horizon/menu");
    for name in [
        "enrichment_chamber",
        "crusher",
        "energized_smelter",
        "metallurgic_infuser",
    ] {
        let file = std::fs::read_to_string(menus.join(format!("{name}.json"))).unwrap();
        let menu: Value = serde_json::from_str(&file).unwrap();
        assert!(menu.get("data_slots").is_none(), "{name}");
    }
}

#[test]
fn a_menu_without_values_yet_shows_nothing() {
    for spec in [ELECTRIC_SCREEN, INFUSER_SCREEN] {
        let view = spec.view(&Mirror::new(&(spec.trackers)()));
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

fn view_of(spec: &ScreenSpec, live: &Live) -> mekanism_rules::WarningView {
    spec.view(&live.mirror(spec))
}

fn with_errors(tracked: &[RecipeError], present: &[RecipeError]) -> Vec<bool> {
    tracked
        .iter()
        .map(|error| present.contains(error))
        .collect()
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
        let data = Live {
            errors: with_errors(&ELECTRIC_TRACKED_ERRORS, &[error]),
            ..Live::default()
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
    let secondary = Live {
        errors: with_errors(
            &INFUSER_TRACKED_ERRORS,
            &[RecipeError::NOT_ENOUGH_SECONDARY_INPUT],
        ),
        ..Live::default()
    };
    let view = view_of(&INFUSER_SCREEN, &secondary);
    assert_eq!(view.issues, [WarningType::NoMatchingRecipe]);
    assert!(view.element(ScreenElement::ChemicalBar));
    assert!(!view.element(ScreenElement::InputSlot));
    // O erro de entrada acende o slot, não a barra, e o tipo na aba é o mesmo.
    let input = Live {
        errors: with_errors(&INFUSER_TRACKED_ERRORS, &[RecipeError::NOT_ENOUGH_INPUT]),
        ..Live::default()
    };
    let view = view_of(&INFUSER_SCREEN, &input);
    assert_eq!(view.issues, [WarningType::NoMatchingRecipe]);
    assert!(view.element(ScreenElement::InputSlot));
    assert!(!view.element(ScreenElement::ChemicalBar));
}

/// As máquinas elétricas não têm barra de químico: o erro do secundário não acende nada nelas.
#[test]
fn the_electric_machines_have_no_chemical_bar() {
    let data = Live {
        errors: vec![true; 5],
        ..Live::default()
    };
    assert!(!view_of(&ELECTRIC_SCREEN, &data).element(ScreenElement::ChemicalBar));
}

fn redstone(control_type: RedstoneControl, powered: bool) -> Live {
    Live {
        control_type,
        powered,
        was_powered: powered,
        ..Live::default()
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
    let data = Live {
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

/// O que o rastro do original diz que está em aviso, passado pelo espelho do menu: a aba e a lista que `getWarnings` devolve batem
/// com as dele, e cada elemento acende quando o erro dele está no rastro.
#[test]
fn the_view_from_the_mirror_matches_the_original_traces() {
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
            let data = Live {
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

/// Esvaziar o tanque: no `broadcastChanges` seguinte o servidor manda só o tanque (a pilha vazia, que é outro tipo), e a tela do cliente
/// fica com ele vazio e o resto como estava.
#[test]
fn dumping_the_tank_reaches_the_screen_with_the_next_broadcast() {
    let spec = &INFUSER_SCREEN;
    let chemical = (spec.indexes)().chemical.unwrap();
    let live = full();
    // O servidor já mandou tudo ao abrir (`send_all` guarda o último valor de cada dado).
    let mut sync = ContainerSync::<u8>::new(&(spec.trackers)());
    sync.send_all(&live);
    let mut mirror = live.mirror(spec);
    let before = ScreenData::read(spec, &mirror);
    assert_eq!(before.chemical, bio(1_000));

    let emptied = Live {
        chemical: StackValue::default(),
        ..live
    };
    let dirty = sync.broadcast_changes(&emptied);
    assert_eq!(dirty.len(), 1, "{dirty:?}");
    assert_eq!(dirty[0].index as usize, chemical);
    assert_eq!(dirty[0].dirty, DirtyType::Dirty);
    for property in &dirty {
        assert!(mirror.apply(property.index, &property.data));
    }
    assert_eq!(
        ScreenData::read(spec, &mirror),
        ScreenData {
            chemical: StackValue::default(),
            ..before
        }
    );
    // Sem mudança nenhuma depois, o servidor não manda nada.
    assert!(sync.broadcast_changes(&emptied).is_empty());
}
