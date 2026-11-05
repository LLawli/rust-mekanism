//! Máquina de progresso: consome energia a cada tick e entrega o resultado no último, como o
//! `CachedRecipe.process` do Mekanism.

/// Como estão as entradas e a saída neste tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Supply {
    /// Falta entrada, ou ela não forma receita: o progresso volta a zero.
    Missing,
    /// A saída não tem espaço: o progresso espera.
    OutputFull,
    Ready,
}

/// O que aconteceu no tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub active: bool,
    /// A operação terminou: consuma as entradas e entregue a saída.
    pub finished: bool,
    pub energy_used: i32,
}

/// Um tick. `progress` conta os ticks já feitos da operação de `required` ticks.
pub fn step(
    progress: &mut u32,
    required: u32,
    supply: Supply,
    stored: i32,
    energy_per_tick: i32,
) -> Step {
    match supply {
        Supply::Missing => {
            *progress = 0;
            return Step::default();
        }
        Supply::OutputFull => return Step::default(),
        Supply::Ready => {}
    }
    if stored < energy_per_tick {
        return Step::default();
    }
    let finished = *progress + 1 >= required;
    *progress = if finished { 0 } else { *progress + 1 };
    Step {
        active: true,
        finished,
        energy_used: energy_per_tick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICKS: u32 = 200;
    const USAGE: i32 = 5;

    #[test]
    fn an_operation_takes_exactly_the_required_ticks() {
        let mut progress = 0;
        let mut energy = 0;
        for tick in 1..=TICKS {
            let step = step(&mut progress, TICKS, Supply::Ready, USAGE, USAGE);
            energy += step.energy_used;
            assert_eq!(step.finished, tick == TICKS, "tick {tick}");
        }
        assert_eq!(energy, 1_000);
        assert_eq!(progress, 0);
    }

    #[test]
    fn without_energy_it_waits_with_its_progress() {
        let mut progress = 50;
        let step = step(&mut progress, TICKS, Supply::Ready, USAGE - 1, USAGE);
        assert_eq!(step, Step::default());
        assert_eq!(progress, 50);
    }

    #[test]
    fn a_full_output_waits_and_uses_no_energy() {
        let mut progress = TICKS - 1;
        let step = step(&mut progress, TICKS, Supply::OutputFull, USAGE, USAGE);
        assert_eq!(step, Step::default());
        assert_eq!(progress, TICKS - 1);
    }

    #[test]
    fn a_missing_input_loses_the_progress() {
        let mut progress = 150;
        let step = step(&mut progress, TICKS, Supply::Missing, USAGE, USAGE);
        assert!(!step.active);
        assert_eq!(progress, 0);
    }

    #[test]
    fn a_one_tick_operation_finishes_at_once() {
        let mut progress = 0;
        assert!(step(&mut progress, 1, Supply::Ready, USAGE, USAGE).finished);
    }
}
