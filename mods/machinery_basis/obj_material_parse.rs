// Roda no fim de `parse_mtl` (o `.mtl` de um modelo `neoforge:obj`): o Horizon lê só o `map_Kd` e perde o `Kd` (a cor difusa que o NeoForge multiplica na cor de cada vértice, `ObjGeometry.makeQuad`)
// e o `neoforge_TintIndex`. O mapa de material para textura não tem lugar para eles: cada material com `Kd` útil ou índice de tinta ganha ali uma entrada extra, de chave
// `machinery_obj_material::key` (nenhuma face a procura) e valor com as duas propriedades, e `obj_material_quads` as aplica aos quads no fim de `bake_obj`.
fn machinery_obj_material_parse(value: BTreeMap<String, MtlTexture>, text: &str) -> BTreeMap<String, MtlTexture> {
    use self::machinery_obj_material as mtl;
    let mut materials = value;
    for (name, properties) in mtl::properties(text) {
        materials.insert(mtl::key(&name), MtlTexture::Slot(mtl::encode(properties)));
    }
    materials
}
