// `mekanism:component_item`: o modelo do item vem do componente. O Horizon não conhece esse tipo e cairia no modelo base, sem
// textura, então ele vira um `select` sobre o componente. Sem caso que bata, é o modelo ausente, como `missingItemModel`.
fn mekanism_component_item_model(
    kind: &str,
    m: &Map<String, Value>,
) -> core::ops::ControlFlow<ItemModelNode> {
    const UPGRADE_TYPES: [&str; 4] = ["speed", "energy", "muffling", "xp_containment"];
    if kind != "mekanism:component_item" || m.get("mek_data").and_then(Value::as_str) != Some("mekanism:upgrade") {
        return core::ops::ControlFlow::Continue(());
    }
    let cases = UPGRADE_TYPES
        .iter()
        .map(|upgrade| super::SwitchCase {
            values: vec![Value::String(format!("mekanism:{upgrade}"))],
            model: ItemModelNode::Model(ModelLeaf {
                model: format!("mekanism:item/upgrade/{upgrade}"),
                transformation: None,
                tints: Vec::new(),
            }),
        })
        .collect();
    core::ops::ControlFlow::Break(ItemModelNode::Select {
        transformation: None,
        property: super::SelectProperty::Component("mekanism:upgrade_type".to_owned()),
        cases,
        fallback: None,
    })
}
