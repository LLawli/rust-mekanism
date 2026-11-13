//! O vizinho de baixo do Heat Generator (`ITileHeatHandler.simulate` de `TileEntityHeatGenerator`): o `simulateAdjacent` do lado `DOWN` vem depois da perda do `simulate` do gerador
//! (`handleHeat(-heatLost)`) e antes da perda para o ar (`simulateEnvironment`), e o que ele devolve é o `lastTransferLoss`. Os valores do calor que passa para um condutor
//! são os da troca genérica (`machinery_basis_core::heat`), que os rastros de `condutor-jogo` conferem; aqui se confere a ordem e o que chega à tela.
use machinery_basis_core::heat::{AMBIENT_TEMP, AdjacentSink};
use mekanismgenerators_rules::{State, Surroundings, step, step_with};

const PLAINS: Surroundings = Surroundings {
    lava_faces: 0,
    in_nether: false,
    ambient: AMBIENT_TEMP,
    face_ambient: [AMBIENT_TEMP; 6],
};

/// Um gerador quente: 5.000 de calor acima do ar.
fn hot() -> State {
    let mut state = State::new(AMBIENT_TEMP);
    state.heat.handle(5_000.0);
    state
}

/// Um vizinho de capacidade 1 e condução 5 na temperatura dada.
struct Sink {
    heat: f64,
}

impl AdjacentSink for Sink {
    fn temperature(&mut self) -> f64 {
        self.heat
    }
    fn heat_capacity(&mut self) -> f64 {
        1.0
    }
    fn inverse_conduction(&mut self) -> f64 {
        5.0
    }
    fn handle_heat(&mut self, transfer: f64) {
        self.heat += transfer;
    }
}

#[test]
fn without_a_neighbor_below_the_step_is_the_one_that_was_already_traced() {
    let mut plain = hot();
    let mut with = hot();
    let a = step(&mut plain, &PLAINS);
    let b = step_with(&mut with, &PLAINS, &mut |_| 0.0);
    assert_eq!(a, b);
    assert_eq!(plain.heat, with.heat);
    assert_eq!(a.transfer_loss, 0.0);
}

#[test]
fn the_neighbor_is_asked_after_the_generator_loss_and_before_the_air() {
    let mut state = hot();
    let heat_before = state.heat.heat;
    let mut seen = None;
    let produced = step_with(&mut state, &PLAINS, &mut |capacitor| {
        seen = Some(capacitor.heat);
        7.5
    });
    let heat_seen = seen.expect("o vizinho foi consultado");
    // O `simulate` do gerador já tirou `0.5 * (temperatura - ar)` e a perda para o ar ainda não aconteceu.
    let temperature = heat_before / 10.0;
    let lost = 0.5 * (temperature - AMBIENT_TEMP);
    assert_eq!(heat_seen, heat_before - lost);
    assert!(state.heat.heat < heat_seen, "a perda para o ar vem depois");
    assert_eq!(
        produced.transfer_loss, 7.5,
        "o que o vizinho devolve é o lastTransferLoss"
    );
    assert!(produced.environment_loss > 0.0);
}

#[test]
fn heat_given_to_the_neighbor_is_taken_from_the_generator_before_the_air_loss() {
    let mut without = hot();
    let none = step(&mut without, &PLAINS);
    let mut state = hot();
    let mut sink = Sink { heat: AMBIENT_TEMP };
    let produced = step_with(&mut state, &PLAINS, &mut |capacitor| {
        capacitor.simulate_adjacent(&mut sink)
    });
    assert!(produced.transfer_loss > 0.0);
    assert!(state.heat.heat < without.heat.heat, "o vizinho levou calor");
    assert!(sink.heat > AMBIENT_TEMP, "e o recebeu");
    // A energia e a perda para o ar são calculadas depois: o gerador mais frio perde menos para o ar.
    assert!(produced.environment_loss < none.environment_loss);
}
