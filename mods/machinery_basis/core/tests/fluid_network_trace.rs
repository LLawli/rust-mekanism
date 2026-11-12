//! A rede de fluido contra o rastro do original (`dev/referencia/testes/FluidNetworkTraceTest.java`, um `.jsonl` por cenário em
//! `dev/referencia/rastros/fluid_*.jsonl`); o motor é o de [`network_trace`], compartilhado com a rede de químico.
mod network_trace;
mod resource_world;
use resource_world::FluidKind;

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
    "heat_generator",
    "chunk_reload",
    "reopen",
];

#[test]
fn every_scenario_matches_the_first_run_of_the_original() {
    for scenario in SCENARIOS {
        network_trace::run::<FluidKind>("", scenario);
    }
}

#[test]
fn every_scenario_matches_the_second_run_of_the_original() {
    for scenario in SCENARIOS {
        network_trace::run::<FluidKind>("run2", scenario);
    }
}
