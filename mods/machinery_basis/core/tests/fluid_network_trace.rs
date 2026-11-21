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

/// Os vizinhos que empurram (frente `tanques`, 9.ª rodada): o Fluid Tank ativo sobre uma linha de canos e sobre o Heat Generator, e o tanque debaixo de um cano (a
/// válvula do topo), e o `incompatible` com tanques de verdade (duas linhas que puxam água e lava de dois tanques, a ponte entre elas, o tanque que esvazia uma e
/// a ponte posta de novo; decisão 138). O tanque é o `FluidTankTile` da lib e chega ao cano por `capability_insert_container`.
const TANK_SCENARIOS: [&str; 5] = [
    "tank_push_pipe",
    "tank_push_alone",
    "tank_push_generator",
    "tank_valve_top",
    "tank_incompatible",
];

#[test]
fn the_tanks_that_push_match_the_two_runs_of_the_original() {
    for run in ["", "run2"] {
        for scenario in TANK_SCENARIOS {
            network_trace::run::<FluidKind>(run, scenario);
        }
    }
}

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

#[test]
fn the_odd_unit_of_break_end_odd_goes_where_the_position_puts_it() {
    network_trace::odd_unit_follows_the_position::<FluidKind>();
}

#[test]
fn the_ten_more_positions_of_break_end_odd_match_the_original() {
    for k in 0..network_trace::ODD_RUNS {
        network_trace::run::<FluidKind>("", &format!("odd_{k}"));
    }
}

#[test]
#[ignore = "mede para o jogo: ODD_ORIGINS=x,y,z;x,y,z cargo test ... -- --ignored --nocapture"]
fn predict_the_halves_for_the_game() {
    for (at, halves) in network_trace::predict_halves::<FluidKind>() {
        println!("ODD {at:?} {halves:?}");
    }
}
