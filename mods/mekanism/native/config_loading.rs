fn mekanism_config_loading(config: &crate::game::NativeConfigFile) {
    crate::game::mekanism_glue::config_loaded(config);
    crate::game::mekanism_glue::config_opened(config);
}
