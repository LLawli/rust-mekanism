fn machinery_basis_multiply_draw(
    hud: &HudRenderer,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    batch: &HudBatch,
    size: [u32; 2],
    vertices: &std::ops::Range<u32>,
    timing: &mut Option<&mut crate::gfx::pack_timing::PassTiming>,
) -> core::ops::ControlFlow<()> {
    // Um lote com corrida marcada (`machinery_multiply::mark_since`) é desenhado em passos, na ordem: o que não tem marca pelo `draw_range` de sempre (que, sem marca no
    // pedaço, segue o corpo original), e cada marca com o blend `(DST_COLOR, ZERO)`. Sem marca, o corpo original.
    if machinery_multiply::draw(hud, encoder, target, depth, batch, size, vertices.clone(), timing.as_deref_mut()) {
        return core::ops::ControlFlow::Break(());
    }
    core::ops::ControlFlow::Continue(())
}
