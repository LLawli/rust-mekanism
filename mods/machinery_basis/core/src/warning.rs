//! Porte de `WarningTracker` e do `trackedErrors` de `TileEntityRecipeMachine`: os avisos da tela, sem desenho.
//!
//! A lib não conhece nenhum aviso do Mekanism. O tipo do aviso é do mod (`K`), e duas coisas do Java viram parâmetro:
//! - a ordem, que no Java é a do `enum` por causa do `EnumMap`. Aqui é a ordem de `K: Ord`, e um `enum` do mod com
//!   `#[derive(PartialOrd, Ord)]` tem a ordem da declaração, igual ao ordinal;
//! - o corte de `getWarnings`, que para na primeira mensagem de `NOT_ENOUGH_ENERGY`. Aqui é `stop_after`, o tipo que fecha a lista.
use crate::operation_tracker::RecipeError;
use std::collections::BTreeMap;

/// Uma verificação de aviso, como o `BooleanSupplier`: quem a registra guarda o que ela lê.
type Check<'a> = Box<dyn Fn() -> bool + 'a>;

/// `WarningTracker`: cada tipo tem uma lista de verificações, e o tipo está ativo se alguma delas diz que sim.
pub struct WarningTracker<'a, K> {
    warnings: BTreeMap<K, Vec<Check<'a>>>,
    stop_after: Option<K>,
}

impl<'a, K: Ord + Copy> WarningTracker<'a, K> {
    /// `stop_after` é o tipo depois do qual `getWarnings` não lista mais nada, ou nenhum.
    pub fn new(stop_after: Option<K>) -> Self {
        Self {
            warnings: BTreeMap::new(),
            stop_after,
        }
    }

    /// `trackWarning`.
    pub fn track(&mut self, kind: K, check: impl Fn() -> bool + 'a) {
        self.warnings.entry(kind).or_default().push(Box::new(check));
    }

    /// `clearTrackedWarnings`.
    pub fn clear(&mut self) {
        self.warnings.clear();
    }

    /// `hasWarning`: para na primeira verificação que diz que sim.
    pub fn has_warning(&self) -> bool {
        self.warnings
            .values()
            .any(|checks| checks.iter().any(|check| check()))
    }

    /// `getWarnings`: um item por tipo ativo, na ordem de `K`. Depois do tipo de `stop_after` não entra mais nenhum, mesmo que
    /// venham depois dele na ordem (no Mekanism, isso inclui os avisos de redstone).
    pub fn warnings(&self) -> Vec<K> {
        let mut shown = Vec::new();
        for (kind, checks) in &self.warnings {
            if checks.iter().any(|check| check()) {
                shown.push(*kind);
                if self.stop_after == Some(*kind) {
                    return shown;
                }
            }
        }
        shown
    }

    /// Todos os tipos com alguma verificação ativa, sem o corte. O Java não tem este método: os rastros o leem do mapa por
    /// reflexão, no campo `warnings`, e `getWarnings` vai em `warnings_shown`.
    pub fn triggered(&self) -> Vec<K> {
        self.warnings
            .iter()
            .filter(|(_, checks)| checks.iter().any(|check| check()))
            .map(|(kind, _)| *kind)
            .collect()
    }
}

/// Quantos erros rastreados cabem na máscara.
pub const MAX_TRACKED_ERRORS: usize = 16;

/// `trackedErrors` como bits: o bit `i` é o erro `i` da lista que o mod passou à máquina (`TRACKED_ERROR_TYPES`). O Java manda um
/// `SyncableBoolean` por erro, e o cliente remonta a máscara deles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorMask(u32);

impl ErrorMask {
    /// Os bits de `trackedErrors`, na ordem do vetor.
    pub fn from_flags(flags: &[bool]) -> Self {
        assert!(
            flags.len() <= MAX_TRACKED_ERRORS,
            "mais erros rastreados que bits na máscara: {}",
            flags.len()
        );
        Self(
            flags
                .iter()
                .enumerate()
                .filter(|(_, flag)| **flag)
                .fold(0, |bits, (index, _)| bits | 1 << index),
        )
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    /// `getWarningCheck` sobre os bits: o erro precisa estar na lista rastreada, e a posição dele nela é o bit. Fora da lista
    /// dá `false`, como o `ALWAYS_FALSE` do Java.
    pub fn has(self, tracked: &[RecipeError], error: RecipeError) -> bool {
        tracked
            .iter()
            .position(|kind| *kind == error)
            .is_some_and(|index| self.0 & 1 << index != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A ordem do `WarningType` do Mekanism que importa aqui: o que vem antes, o corte, e o que vem depois dele.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Kind {
        NoOutput,
        NoRecipe,
        NoSpace,
        NoEnergy,
        ReducedRate,
        SignalAbsent,
    }

    fn tracker(active: &[Kind]) -> WarningTracker<'_, Kind> {
        let mut tracker = WarningTracker::new(Some(Kind::NoEnergy));
        // Registrados fora da ordem do enum de propósito: quem manda é `Kind`, não a ordem de registro.
        for kind in [
            Kind::SignalAbsent,
            Kind::NoEnergy,
            Kind::ReducedRate,
            Kind::NoSpace,
            Kind::NoRecipe,
            Kind::NoOutput,
        ] {
            tracker.track(kind, move || active.contains(&kind));
        }
        tracker
    }

    #[test]
    fn nothing_active_shows_nothing() {
        let tracker = tracker(&[]);
        assert!(!tracker.has_warning());
        assert_eq!(tracker.warnings(), []);
        assert_eq!(tracker.triggered(), []);
    }

    #[test]
    fn warnings_come_in_the_order_of_the_kind_not_of_the_registration() {
        let active = [
            Kind::NoSpace,
            Kind::NoOutput,
            Kind::NoRecipe,
            Kind::NoEnergy,
        ];
        assert_eq!(
            tracker(&active).warnings(),
            [
                Kind::NoOutput,
                Kind::NoRecipe,
                Kind::NoSpace,
                Kind::NoEnergy
            ]
        );
    }

    #[test]
    fn the_list_stops_at_the_cut_and_drops_everything_after_it() {
        let active = [Kind::NoSpace, Kind::NoEnergy, Kind::SignalAbsent];
        let tracker = tracker(&active);
        assert_eq!(tracker.warnings(), [Kind::NoSpace, Kind::NoEnergy]);
        // O que está ativo continua inteiro, só a lista mostrada é cortada.
        assert_eq!(tracker.triggered(), active);
    }

    #[test]
    fn what_comes_before_the_cut_is_not_lost_without_it() {
        // Sem o tipo do corte ativo, o que vem depois dele aparece.
        let active = [Kind::NoSpace, Kind::ReducedRate, Kind::SignalAbsent];
        assert_eq!(tracker(&active).warnings(), active);
    }

    /// Os valores de `separator_reduced_rate`: o separador é a única máquina que registra `NOT_ENOUGH_ENERGY_REDUCED_RATE`.
    #[test]
    fn the_reduced_rate_of_the_separator_trace() {
        let cases: [(&[Kind], &[Kind]); 4] = [
            // t=11: só a taxa reduzida.
            (&[Kind::ReducedRate], &[Kind::ReducedRate]),
            // t=31: falta energia.
            (&[Kind::NoEnergy], &[Kind::NoEnergy]),
            // t=40: modo HIGH sem sinal com o erro de energia atrás, e a energia fecha a lista.
            (&[Kind::NoEnergy, Kind::SignalAbsent], &[Kind::NoEnergy]),
            // t=51: a energia voltou e só o redstone aparece.
            (&[Kind::SignalAbsent], &[Kind::SignalAbsent]),
        ];
        for (active, shown) in cases {
            assert_eq!(tracker(active).warnings(), shown, "{active:?}");
        }
    }

    /// Falta energia e a taxa reduzida juntas: o Java não lista a segunda (o comentário dele fala só dela).
    #[test]
    fn not_enough_energy_hides_the_reduced_rate() {
        let tracker = tracker(&[Kind::NoEnergy, Kind::ReducedRate]);
        assert_eq!(tracker.warnings(), [Kind::NoEnergy]);
    }

    #[test]
    fn a_kind_with_two_active_checks_counts_once() {
        let mut tracker = WarningTracker::new(None);
        tracker.track(Kind::NoSpace, || true);
        tracker.track(Kind::NoSpace, || true);
        assert_eq!(tracker.warnings(), [Kind::NoSpace]);
        assert_eq!(tracker.triggered(), [Kind::NoSpace]);
    }

    #[test]
    fn a_kind_is_active_if_any_of_its_checks_is() {
        let mut tracker = WarningTracker::new(None);
        tracker.track(Kind::NoRecipe, || false);
        tracker.track(Kind::NoRecipe, || true);
        assert!(tracker.has_warning());
        assert_eq!(tracker.warnings(), [Kind::NoRecipe]);
    }

    #[test]
    fn has_warning_stops_at_the_first_active_check() {
        let asked = Cell::new(0);
        let mut tracker = WarningTracker::new(None);
        tracker.track(Kind::NoOutput, || {
            asked.set(asked.get() + 1);
            true
        });
        tracker.track(Kind::NoSpace, || {
            asked.set(asked.get() + 1);
            true
        });
        assert!(tracker.has_warning());
        assert_eq!(asked.get(), 1);
    }

    #[test]
    fn clearing_drops_every_check() {
        let mut tracker = tracker(&[Kind::NoEnergy]);
        tracker.clear();
        assert!(!tracker.has_warning());
        assert_eq!(tracker.warnings(), []);
    }

    #[test]
    fn the_mask_has_one_bit_per_tracked_error_in_the_order_of_the_list() {
        let mask = ErrorMask::from_flags(&[true, false, true, false]);
        assert_eq!(mask.bits(), 0b0101);
        assert_eq!(ErrorMask::from_flags(&[]).bits(), 0);
    }

    #[test]
    fn the_mask_answers_by_the_position_in_the_tracked_list() {
        let tracked = [
            RecipeError::NOT_ENOUGH_ENERGY,
            RecipeError::NOT_ENOUGH_INPUT,
            RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
        ];
        let mask = ErrorMask::from_flags(&[false, true, true]);
        assert!(!mask.has(&tracked, RecipeError::NOT_ENOUGH_ENERGY));
        assert!(mask.has(&tracked, RecipeError::NOT_ENOUGH_INPUT));
        assert!(mask.has(&tracked, RecipeError::NOT_ENOUGH_OUTPUT_SPACE));
        // Fora da lista: nunca, como o `ALWAYS_FALSE` do Java.
        assert!(!mask.has(&tracked, RecipeError::NOT_ENOUGH_SECONDARY_INPUT));
    }

    #[test]
    #[should_panic(expected = "mais erros rastreados")]
    fn more_errors_than_bits_do_not_fit() {
        ErrorMask::from_flags(&[false; MAX_TRACKED_ERRORS + 1]);
    }
}
