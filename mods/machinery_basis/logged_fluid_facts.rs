// Entra em `crates/horizon-client/src/game.rs` como `machinery_logged_fluid_facts`. O fluido que o estado `fluid_logged` de um bloco de mod declara
// (`IStateFluidLoggable.getFluid`): `water` e `lava` são a fonte de cada um, `empty` (e qualquer outro valor) não tem fluido. Só `std`: o teste do `machinery_basis_core`
// compila este arquivo como está (`tests/logged_fluid_facts.rs`).

/// A fonte que o valor de `fluid_logged` põe no estado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Water,
    Lava,
}

/// A quantidade de uma fonte no `StateFacts` do Horizon (`fluid_level`: 8 é a fonte).
pub const SOURCE_LEVEL: u8 = 8;

/// O fluido do valor de `fluid_logged` de um estado, se ele tem um.
pub fn source(value: Option<&str>) -> Option<Source> {
    match value {
        Some("water") => Some(Source::Water),
        Some("lava") => Some(Source::Lava),
        _ => None,
    }
}
