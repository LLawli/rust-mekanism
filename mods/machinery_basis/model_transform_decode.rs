// O `transform` do NeoForge na raiz de um modelo e em cada filho de um `neoforge:composite` (`NeoForgeModelProperties.deserializeRootTransform`): o `CuboidModel` do Horizon
// não tem campo para ele e o `Loader::Obj` só lê `model`, `flip_v`, `shade_quads` e `mtl_override`. Este MODIFY_VARIABLE (o `let mut textures` de `CuboidModel::decode`) põe a
// transformação do JSON no espaço de textura `SLOT` do modelo; o `textures` o herda do pai e o modelo mais perto da folha vence, como a propriedade do NeoForge, e o cozimento
// (`model_transform_bake`) a aplica. Cada filho de um `composite` passa por aqui na própria `decode`, então leva a sua. Um `transform` que o NeoForge recusa
// (`JsonParseException`, o modelo não carrega) fica sem efeito, com um aviso: o valor de uma variável não pode devolver o erro. Só lê o JSON que cita `transform`.
fn machinery_model_transform_decode(
    raw: &RawValue,
    value: BTreeMap<String, TextureSlot>,
) -> BTreeMap<String, TextureSlot> {
    use crate::math::machinery_model_transform as transform;
    let mut textures = value;
    let text = raw.get();
    if transform::mentions(text) {
        match transform::slot_sprite(text) {
            Ok(Some(sprite)) => {
                textures.insert(
                    transform::SLOT.to_owned(),
                    TextureSlot::Material(Material {
                        sprite,
                        force_translucent: false,
                    }),
                );
            }
            Ok(None) => {}
            Err(message) => log::warn!("Horizon: model transform ignored: {message}"),
        }
    }
    textures
}
