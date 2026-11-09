// O `transform` do NeoForge, aplicado ao cozinhar (`UnbakedCuboidGeometry.bake`, `CompositeUnbakedGeometry.bake` e `ObjGeometry.bake` juntam a transformação de raiz ao
// `ModelState`). Este HEAD cancelável deixa o `bake_into` original cozinhar o modelo (elementos, filhos e `.obj`) com a rotação do blockstate e depois move os vértices que
// ele acrescentou pela matriz do modelo, conjugada pela rotação (`machinery_model_transform::placement`). Um filho de `composite` que só herdou a transformação do
// `composite` não a aplica de novo (`enter`); o filho com a própria soma a sua por dentro da do `composite`, como `pai.compose(raiz)`. As faces que mudam de direção
// (rotação ou escala no `transform`) levam a direção e a face de corte recalculadas, como o `FaceBakery`.
fn machinery_model_transform_bake(
    model: &ResolvedCuboidModel,
    transform: BlockModelTransform,
    skip_flat: bool,
    output: &mut Vec<ModelQuad>,
) -> core::ops::ControlFlow<Result<()>> {
    use crate::math::machinery_model_transform as moved;
    let address = model as *const ResolvedCuboidModel as usize;
    if moved::bake_reentered(address) {
        return core::ops::ControlFlow::Continue(());
    }
    let Some(root) = model.textures.get(moved::SLOT).and_then(|slot| moved::enter(&slot.sprite)) else {
        return core::ops::ControlFlow::Continue(());
    };
    // A chamada abaixo volta a esta injeção com o mesmo modelo: a marca a faz seguir para o corpo original.
    moved::bake_mark(address);
    let start = output.len();
    let baked = bake_into(model, transform, skip_flat, output);
    moved::leave();
    if baked.is_err() {
        return core::ops::ControlFlow::Break(baked);
    }
    let placement = moved::placement(&transform.matrix_rows(), &root);
    let only_moves = moved::moves_only(&placement);
    for quad in &mut output[start..] {
        quad.positions = quad.positions.map(|vertex| moved::position(&placement, vertex));
        if !only_moves {
            quad.facing = facing(&quad.positions).unwrap_or(Direction::Up);
            quad.cull = quad
                .cull
                .map(|side| Direction::ALL[moved::rotate_direction(&placement, side as usize)]);
        }
    }
    core::ops::ControlFlow::Break(Ok(()))
}
