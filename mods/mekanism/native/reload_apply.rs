fn mekanism_reload_apply(
    game: &crate::game::Game<'_>,
    listener: &crate::game::NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    crate::game::mekanism_glue::reload_apply(game, listener, files);
}
