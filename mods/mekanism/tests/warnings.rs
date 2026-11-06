//! Os avisos da tela: o `WarningType` do Mekanism, os pares de cada máquina portada e o corte de `getWarnings`, sem jogo.
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::warning::ErrorMask;
use mekanism_rules::{
    ELECTRIC_TRACKED_ERRORS, ELECTRIC_WARNINGS, INFUSER_TRACKED_ERRORS, INFUSER_WARNINGS,
    SyncedMachine, WarningSource, WarningType, machine_warnings,
};
use serde_json::Value;
use std::path::Path;

/// `WarningTracker.WarningType` na ordem em que o Java declara.
const JAVA_ORDER: [(WarningType, &str); 11] = [
    (
        WarningType::InputDoesntProduceOutput,
        "input_doesnt_produce_output",
    ),
    (WarningType::NoMatchingRecipe, "no_matching_recipe"),
    (WarningType::NoSpaceInOutput, "no_space_in_output"),
    (
        WarningType::NoSpaceInOutputOverflow,
        "no_space_in_output_overflow",
    ),
    (WarningType::NotEnoughEnergy, "not_enough_energy"),
    (
        WarningType::NotEnoughEnergyReducedRate,
        "not_enough_energy_reduced_rate",
    ),
    (
        WarningType::InvalidOredictionificatorFilter,
        "invalid_oredictionificator_filter",
    ),
    (
        WarningType::FilterHasBlacklistedElement,
        "filter_has_blacklisted_element",
    ),
    (WarningType::RedstoneSignalAbsent, "redstone_signal_absent"),
    (
        WarningType::RedstoneSignalPresent,
        "redstone_signal_present",
    ),
    (
        WarningType::RedstonePulseRequired,
        "redstone_pulse_required",
    ),
];

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

fn kind_named(name: &str) -> WarningType {
    JAVA_ORDER
        .iter()
        .find(|(_, java)| *java == name)
        .unwrap_or_else(|| panic!("tipo desconhecido: {name}"))
        .0
}

fn names(kinds: Vec<WarningType>) -> Value {
    kinds
        .into_iter()
        .map(|kind| Value::from(kind.name()))
        .collect()
}

/// Uma máquina que só diz o que o teste mandou.
struct Fake {
    errors: &'static [RecipeError],
    control: RedstoneControl,
    activated: bool,
}

impl WarningSource for Fake {
    fn has_error(&self, error: RecipeError) -> bool {
        self.errors.contains(&error)
    }

    fn control_type(&self) -> RedstoneControl {
        self.control
    }

    fn is_redstone_activated(&self) -> bool {
        self.activated
    }
}

fn fake(errors: &'static [RecipeError]) -> Fake {
    Fake {
        errors,
        control: RedstoneControl::Disabled,
        activated: true,
    }
}

#[test]
fn the_kinds_are_in_the_order_the_java_declares_them() {
    for (index, (kind, name)) in JAVA_ORDER.iter().enumerate() {
        assert_eq!(kind.name(), *name);
        assert_eq!(
            JAVA_ORDER.iter().filter(|(other, _)| other < kind).count(),
            index,
            "{name} fora do lugar"
        );
    }
}

#[test]
fn every_kind_has_its_text_in_the_language_file() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    let lang: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for (kind, name) in JAVA_ORDER {
        assert!(
            lang.get(kind.lang_key()).is_some(),
            "{name}: {}",
            kind.lang_key()
        );
    }
}

/// O separador ainda não está portado, mas o rastro dele tem os valores do corte de `NOT_ENOUGH_ENERGY_REDUCED_RATE`: cada tick
/// do rastro entra no tracker como os tipos ativos que ele grava, e a lista mostrada tem de ser a que ele grava.
#[test]
fn the_reduced_rate_of_the_separator_trace_is_cut_as_the_original_cuts_it() {
    let lines = trace("separator_reduced_rate");
    assert!(lines.len() >= 70);
    let mut reduced_rate_shown = 0;
    let mut energy_hides_redstone = 0;
    for line in &lines {
        let active: Vec<WarningType> = line["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| kind_named(name.as_str().unwrap()))
            .collect();
        let mut tracker = machinery_basis_core::warning::WarningTracker::new(Some(
            mekanism_rules::WARNINGS_STOP_AFTER,
        ));
        // Todos os tipos registrados, como a tela do separador mais a de redstone, e só os do rastro respondem que sim.
        for (kind, _) in JAVA_ORDER {
            let active = &active;
            tracker.track(kind, move || active.contains(&kind));
        }
        assert_eq!(
            Some(&names(tracker.triggered())),
            line.get("warnings"),
            "tick {}",
            line["t"]
        );
        assert_eq!(
            Some(&names(tracker.warnings())),
            line.get("warnings_shown"),
            "tick {}",
            line["t"]
        );
        let shown = line["warnings_shown"].as_array().unwrap();
        reduced_rate_shown +=
            usize::from(shown.contains(&Value::from("not_enough_energy_reduced_rate")));
        energy_hides_redstone += usize::from(
            active.contains(&WarningType::NotEnoughEnergy)
                && active.contains(&WarningType::RedstoneSignalAbsent)
                && !shown.contains(&Value::from("redstone_signal_absent")),
        );
    }
    // O rastro tem de ter o efeito que a comparação prova: a taxa reduzida aparece, e a energia esconde o redstone.
    assert!(reduced_rate_shown > 0);
    assert!(energy_hides_redstone > 0);
}

#[test]
fn the_electric_machines_never_track_the_reduced_rate() {
    for (kind, _) in ELECTRIC_WARNINGS.iter().chain(&INFUSER_WARNINGS) {
        assert_ne!(*kind, WarningType::NotEnoughEnergyReducedRate);
    }
    // Nem o erro que o separador usa está entre os que elas guardam: o bit nunca existiria.
    for error in ELECTRIC_TRACKED_ERRORS
        .iter()
        .chain(&INFUSER_TRACKED_ERRORS)
    {
        assert_ne!(*error, RecipeError::NOT_ENOUGH_ENERGY_REDUCED_RATE);
    }
}

/// Os dois `NO_MATCHING_RECIPE` do infuser (a entrada e a barra do químico) são um aviso só.
#[test]
fn the_infuser_has_two_checks_for_the_same_warning() {
    for error in [
        RecipeError::NOT_ENOUGH_INPUT,
        RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    ] {
        let source = fake(Box::leak(Box::new([error])));
        let tracker = machine_warnings(&source, &INFUSER_WARNINGS);
        assert_eq!(tracker.warnings(), [WarningType::NoMatchingRecipe]);
    }
    let both = fake(&[
        RecipeError::NOT_ENOUGH_INPUT,
        RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    ]);
    let tracker = machine_warnings(&both, &INFUSER_WARNINGS);
    assert_eq!(tracker.warnings(), [WarningType::NoMatchingRecipe]);
}

/// A câmara não tem o químico secundário: o erro dele não vira aviso.
#[test]
fn the_electric_machines_have_no_secondary_input_warning() {
    let source = fake(&[RecipeError::NOT_ENOUGH_SECONDARY_INPUT]);
    assert!(!machine_warnings(&source, &ELECTRIC_WARNINGS).has_warning());
    assert!(machine_warnings(&source, &INFUSER_WARNINGS).has_warning());
}

#[test]
fn every_error_of_the_infuser_has_a_warning() {
    let expected = [
        (RecipeError::NOT_ENOUGH_ENERGY, WarningType::NotEnoughEnergy),
        (RecipeError::NOT_ENOUGH_INPUT, WarningType::NoMatchingRecipe),
        (
            RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
            WarningType::NoMatchingRecipe,
        ),
        (
            RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
            WarningType::NoSpaceInOutput,
        ),
        (
            RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
            WarningType::InputDoesntProduceOutput,
        ),
    ];
    for (error, kind) in expected {
        let source = fake(Box::leak(Box::new([error])));
        assert_eq!(
            machine_warnings(&source, &INFUSER_WARNINGS).warnings(),
            [kind],
            "{error:?}"
        );
    }
}

#[test]
fn the_three_redstone_warnings_need_their_mode_and_no_activation() {
    let cases = [
        (RedstoneControl::High, WarningType::RedstoneSignalAbsent),
        (RedstoneControl::Low, WarningType::RedstoneSignalPresent),
        (RedstoneControl::Pulse, WarningType::RedstonePulseRequired),
    ];
    for (control, kind) in cases {
        let waiting = Fake {
            control,
            activated: false,
            ..fake(&[])
        };
        assert_eq!(
            machine_warnings(&waiting, &ELECTRIC_WARNINGS).warnings(),
            [kind]
        );
        let running = Fake {
            control,
            activated: true,
            ..fake(&[])
        };
        assert!(!machine_warnings(&running, &ELECTRIC_WARNINGS).has_warning());
    }
    // DISABLED nunca avisa, nem desativado.
    let disabled = Fake {
        activated: false,
        ..fake(&[])
    };
    assert!(!machine_warnings(&disabled, &ELECTRIC_WARNINGS).has_warning());
}

/// O que o cliente lê dos data slots dá os mesmos avisos que o objeto no servidor, para cada subconjunto de erros do infuser.
#[test]
fn the_client_warnings_from_the_mask_match_the_server_ones() {
    for bits in 0..1u32 << INFUSER_TRACKED_ERRORS.len() {
        let present: Vec<RecipeError> = INFUSER_TRACKED_ERRORS
            .iter()
            .enumerate()
            .filter(|(index, _)| bits & 1 << index != 0)
            .map(|(_, error)| *error)
            .collect();
        let server = Fake {
            errors: Box::leak(present.into_boxed_slice()),
            control: RedstoneControl::High,
            activated: false,
        };
        let flags: Vec<bool> = INFUSER_TRACKED_ERRORS
            .iter()
            .map(|error| server.has_error(*error))
            .collect();
        let client = SyncedMachine {
            tracked: &INFUSER_TRACKED_ERRORS,
            errors: ErrorMask::from_slot(ErrorMask::from_flags(&flags).to_slot()),
            control_type: server.control,
            redstone_activated: server.activated,
        };
        assert_eq!(
            machine_warnings(&client, &INFUSER_WARNINGS).warnings(),
            machine_warnings(&server, &INFUSER_WARNINGS).warnings(),
            "máscara {bits:#b}"
        );
    }
}
