// O limite de 16 caixas por forma (`BlockShapes::check`, `content.rs`) deixa de valer: o original tem blocos de máquina com formas de até 47 caixas depois de cobertas por
// poucas caixas (o Digital Miner), e `VoxelShape` do Horizon une quantas vierem. Só entra quando uma forma passa do limite; abaixo dele a conferência do Horizon roda
// como sempre. As outras regras são as de `check`: propriedades distintas, um valor por propriedade em cada chave, caixas finitas e ordenadas.
fn mekanism_shape_limit(shapes: &BlockShapes) -> core::ops::ControlFlow<Result<(), String>> {
    const LIMIT: usize = 16;
    let over = shapes.values.values().chain(shapes.default.iter()).any(|boxes| boxes.len() > LIMIT);
    if !over {
        return core::ops::ControlFlow::Continue(());
    }
    let name = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    let names = shapes.property_names();
    if (!shapes.property.is_empty() && !shapes.properties.is_empty())
        || names.iter().any(|n| !name(n))
        || names.iter().collect::<std::collections::BTreeSet<_>>().len() != names.len()
        || (names.is_empty() && !shapes.values.is_empty())
        || (shapes.values.is_empty() && shapes.default.is_none())
    {
        return core::ops::ControlFlow::Break(Err("shapes needs distinct properties and values, or a default".into()));
    }
    for key in shapes.values.keys() {
        let values: Vec<_> = key.split(',').collect();
        if values.len() != names.len() || values.iter().any(|v| !name(v)) {
            return core::ops::ControlFlow::Break(Err(format!("invalid shape key {key:?}: one value per property")));
        }
    }
    for boxes in shapes.values.values().chain(shapes.default.iter()) {
        if boxes.iter().any(|bounds| bounds.iter().any(|v| !v.is_finite()) || (0..3).any(|axis| bounds[axis] > bounds[axis + 3])) {
            return core::ops::ControlFlow::Break(Err("shape needs finite boxes with ordered bounds".into()));
        }
    }
    core::ops::ControlFlow::Break(Ok(()))
}
