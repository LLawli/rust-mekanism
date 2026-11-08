fn mekanism_config_reloading(config: &crate::game::NativeConfigFile) {
    crate::game::mekanism_glue::config_loaded(config);
}
