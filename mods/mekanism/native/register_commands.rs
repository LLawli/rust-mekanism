fn mekanism_register_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    crate::game::mekanism_commands::register_commands(commands);
}
