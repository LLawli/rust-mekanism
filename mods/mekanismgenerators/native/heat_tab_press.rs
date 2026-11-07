fn mekanismgenerators_heat_tab_press(
    game: &crate::game::Game<'_>,
    screen: &horizon_client::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    crate::game::mekanismgenerators_glue::heat_tab_press(game, screen, button, canceled);
}
