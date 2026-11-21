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

/// Os vizinhos que empurram (frente `tanques`, 9.ª rodada): o Chemical Tank que ejeta para uma linha de tubos, pelos três modos de descarte, e para uma máquina
/// (o tanque de infusão do Metallurgic Infuser), e o `incompatible` com tanques de verdade (decisão 144). O tanque é o `ChemicalTankTile` da lib e chega ao tubo
/// por `capability_insert_container`.
const TANK_SCENARIOS: [&str; 4] = [
    "tank_eject_tube",
    "tank_eject_alone",
    "tank_eject_machine",
    "tank_incompatible",
];

#[test]
fn the_tanks_that_eject_match_the_two_runs_of_the_original() {
    for run in ["", "run2"] {
        for scenario in TANK_SCENARIOS {
            network_trace::run::<ChemicalKind>(run, scenario);
        }
    }
}

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

#[test]
#[ignore = "mede para o jogo: ODD_ORIGINS=x,y,z;x,y,z cargo test ... -- --ignored --nocapture"]
fn predict_the_halves_for_the_game() {
    for (at, halves) in network_trace::predict_halves::<ChemicalKind>() {
        println!("ODD {at:?} {halves:?}");
    }
}
