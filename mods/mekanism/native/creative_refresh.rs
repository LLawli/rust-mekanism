// `Game::refresh_creative_tabs` por dentro de um invólucro (HEAD cancelável com marca de reentrada em `mekanism_glue`): depois dela, as abas seguem as opções
// `prefilled` de `general.toml`.
fn mekanism_creative_refresh(game: &mut crate::game::Game<'_>) -> core::ops::ControlFlow<()> {
    crate::game::mekanism_glue::creative_refresh(game)
}
