fn mekanism_reload_apply(
    listener: &crate::game::NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    crate::game::mekanism_glue::reload_apply(listener, files);
}
