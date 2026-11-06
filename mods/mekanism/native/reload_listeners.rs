fn mekanism_reload_listeners(listeners: &mut Vec<crate::game::NativeReloadListener>) {
    crate::game::mekanism_glue::reload_listeners(listeners);
}
