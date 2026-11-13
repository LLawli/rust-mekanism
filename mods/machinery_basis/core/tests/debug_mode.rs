//! `MekanismAPI.debug`: o campo estático que o `/mek debug` inverte (um teste só neste arquivo: o estado é do processo).
use machinery_basis_core::debug_mode;

#[test]
fn the_flag_starts_off_and_the_toggle_gives_the_new_value() {
    assert!(!debug_mode::enabled());
    assert!(debug_mode::toggle());
    assert!(debug_mode::enabled());
    assert!(!debug_mode::toggle());
    assert!(!debug_mode::enabled());
    debug_mode::set(true);
    assert!(debug_mode::enabled());
    debug_mode::set(false);
    assert!(!debug_mode::enabled());
}
