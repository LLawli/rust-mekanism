fn machinery_basis_container_dim(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
) -> core::ops::ControlFlow<()> {
    // O fundo que a tela de contêiner pinta: a janela toda em `rgba(16, 16, 16, 160)`. Só quando a tela aberta é de uma máquina nossa (o `Game` o diz por
    // `machinery_container_dim`): o baú e a fornalha do vanilla seguem com o retângulo do Horizon. O original desenha o degradê de `0xC0101010` a `0xD0101010`.
    if machinery_container_dim::is_set()
        && color == rgba(16, 16, 16, 160)
        && x == 0.0
        && y == 0.0
        && w == batch.width
        && h == batch.height
    {
        batch.fill_gradient(atlas, (x, y, w, h), rgba(16, 16, 16, 0xC0), rgba(16, 16, 16, 0xD0));
        return core::ops::ControlFlow::Break(());
    }
    core::ops::ControlFlow::Continue(())
}
