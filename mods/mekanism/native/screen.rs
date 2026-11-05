fn mekanism_screen(block: &str, screen: &mut Option<horizon_client::mod_api::NativeScreen>) {
    crate::game::mekanism_glue::screen(block, screen);
}
