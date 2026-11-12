// Roda no começo de `bake_obj` e o chama de novo por dentro (a marca `bake_mark` faz a segunda passagem seguir para o corpo original; o corpo tem `?`, que o RETURN não aceita). Os quads
// que ele acabou de acrescentar a `output` são um por face visível do OBJ, na ordem das faces, e cada um leva do `.mtl` (as entradas extras de `obj_material_parse`) a cor `Kd` do material da
// face e o `neoforge_TintIndex`. Sem a entrada (o material não tem `Kd` útil nem índice de tinta, ou o OBJ não tem `.mtl`) o quad fica como o Horizon o assou. Um erro de `bake_obj` passa como veio.
fn machinery_obj_material_quads(
    model: &ResolvedCuboidModel,
    obj: &ObjModel,
    flip_v: bool,
    shade: bool,
    hidden: &std::collections::BTreeSet<String>,
    transform: BlockModelTransform,
    output: &mut Vec<ModelQuad>,
) -> core::ops::ControlFlow<Result<()>> {
    use crate::obj::machinery_obj_material as mtl;
    let address = obj as *const ObjModel as usize;
    if mtl::bake_reentered(address) {
        return core::ops::ControlFlow::Continue(());
    }
    mtl::bake_mark(address);
    let start = output.len();
    let value = bake_obj(model, obj, flip_v, shade, hidden, transform, &mut *output);
    if value.is_err() {
        return core::ops::ControlFlow::Break(value);
    }
    let faces: Vec<&ObjFace> = obj.faces.iter().filter(|face| !hidden.contains(&face.object)).collect();
    for (face, quad) in faces.iter().zip(output[start..].iter_mut()) {
        let Some(MtlTexture::Slot(text)) = obj.materials.get(&mtl::key(&face.material)) else {
            continue;
        };
        let Some((color, tint)) = mtl::decode(text) else {
            continue;
        };
        if color.is_some() {
            quad.color = color;
        }
        if tint != mtl::NO_TINT {
            quad.tint_index = tint;
        }
    }
    core::ops::ControlFlow::Break(value)
}
