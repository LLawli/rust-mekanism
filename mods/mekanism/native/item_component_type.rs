// O `select` por componente (`minecraft:component`) de um arquivo de item só aceita os componentes do vanilla (`DATA_COMPONENT_TYPES`), e o item do Configurator escolhe o
// modelo pelo `mekanism:configurator_mode`: sem a injeção o Horizon recusa o arquivo inteiro ("unknown data component type") e o item fica sem modelo. Um componente do
// Mekanism passa; qualquer outro volta ao caminho normal. Sem o componente na pilha nenhum caso bate e vale o `fallback`, o modelo do modo padrão, como no original.
fn mekanism_item_component_type(v: &Value) -> core::ops::ControlFlow<Result<String>> {
    match v.as_str() {
        Some(id) if id.starts_with("mekanism:") => core::ops::ControlFlow::Break(Ok(id.to_owned())),
        _ => core::ops::ControlFlow::Continue(()),
    }
}
