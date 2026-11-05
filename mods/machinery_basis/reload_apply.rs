fn machinery_basis_reload_apply(
    listener: &crate::game::NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    crate::game::machinery_basis::reload_apply(listener, files);
}
