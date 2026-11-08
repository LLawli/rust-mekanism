// Roda no começo de `block_animate_tick`, que `ClientLevel.doAnimateTick` chama para cada posição sorteada (`Block.animateTick`): chama o
// animador de cada mod que registrou um (`animate_registry.rs`). Os blocos de mod ficam `Animate::None` na tabela do Horizon, que só conhece o vanilla
// por nome, então nada mais acontece com eles nesta função.
fn machinery_basis_animate_tick(engine: &mut ParticleEngine, ctx: &AnimateContext<'_>, state: BlockStateId, pos: [i32; 3]) {
    machinery_animate::animate(engine, ctx, state, pos);
}
