//! A rede de químico contra o rastro do original (`dev/referencia/testes/ChemicalNetworkTraceTest.java`, um `.jsonl` por cenário em
//! `dev/referencia/rastros/chemical_*.jsonl`); o motor é o de [`network_trace`], compartilhado com a rede de fluido. O que o rastro do químico
//! tem a mais: o tanque de químico (uma máquina configurável, posta com a configuração do item e `INPUT_OUTPUT` nos seis lados, sem ejetar) e os
//! `disperse` do `ChemicalNetwork` (o vazamento de radiação quando o último tubo some).
mod network_trace;
mod resource_world;
use resource_world::ChemicalKind;

const SCENARIOS: [&str; 13] = [
    "line_consumer",
    "two_consumers",
    "tiers",
    "incompatible",
    "incompatible_swap",
    "incompatible_direct",
    "incompatible_none",
    "merge_same",
    "split_middle",
    "break_end_odd",
    "radioactive",
    "chunk_reload",
    "reopen",
];

#[test]
fn every_scenario_matches_the_first_run_of_the_original() {
    for scenario in SCENARIOS {
        network_trace::run::<ChemicalKind>("", scenario);
    }
}

#[test]
fn every_scenario_matches_the_second_run_of_the_original() {
    for scenario in SCENARIOS {
        network_trace::run::<ChemicalKind>("run2", scenario);
    }
}

#[test]
fn the_odd_unit_of_break_end_odd_goes_where_the_position_puts_it() {
    network_trace::odd_unit_follows_the_position::<ChemicalKind>();
}

#[test]
fn the_ten_more_positions_of_break_end_odd_match_the_original() {
    for k in 0..network_trace::ODD_RUNS {
        network_trace::run::<ChemicalKind>("", &format!("odd_{k}"));
    }
}
