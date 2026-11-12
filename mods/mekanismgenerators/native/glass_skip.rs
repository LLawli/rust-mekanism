// `skipRendering` do vidro do Mekanism e do Generators (decisão 30): antes de a regra do Horizon (`same_block`, só entre estados do mesmo bloco) decidir, o vidro estrutural, o vidro do reator e a
// matriz de foco do laser consultam a tabela do original (`glass_skip_table.rs`). Quem não é um dos três segue o corpo de `skips`.
fn mekanismgenerators_glass_skip(own: State, other: State) -> core::ops::ControlFlow<bool> {
    match crate::block_render::mekanismgenerators_glass::skips(own.block.0, other.block.0) {
        Some(skips) => core::ops::ControlFlow::Break(skips),
        None => core::ops::ControlFlow::Continue(()),
    }
}
