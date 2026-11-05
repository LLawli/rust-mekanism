fn mekanism_container(
    block: &str,
    container: &mut Option<horizon_client::containers::ModContainer>,
) {
    crate::game::mekanism_glue::container(block, container);
}
