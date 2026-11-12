// Roda no fim de `bake_obj`: os quads que ele acabou de acrescentar a `output` são um por face visível do OBJ, na ordem das faces, e cada um leva do `.mtl` (as entradas extras de
// `obj_material_parse`) a cor `Kd` do material da face e o `neoforge_TintIndex`. Sem a entrada (o material não tem `Kd` útil nem índice de tinta, ou o OBJ não tem `.mtl`) o quad fica como
// o Horizon o assou. Um erro de `bake_obj` passa como veio.
fn machinery_obj_material_quads(
    value: Result<()>,
    obj: &ObjModel,
    hidden: &std::collections::BTreeSet<String>,
    output: &mut Vec<ModelQuad>,
) -> Result<()> {
    use crate::obj::machinery_obj_material as mtl;
    if value.is_err() {
        return value;
    }
    let faces: Vec<&ObjFace> = obj.faces.iter().filter(|face| !hidden.contains(&face.object)).collect();
    let Some(start) = output.len().checked_sub(faces.len()) else {
        return value;
    };
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
    value
}
