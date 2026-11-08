fn mekanism_register_configs(configs: &mut Vec<crate::game::NativeConfig>) {
    crate::game::mekanism_glue::register_configs(configs);
}
