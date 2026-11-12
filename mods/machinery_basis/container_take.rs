// Roda no começo de `Containers::can_take` (`canTakeItemThroughFace`): o Horizon responde por um bloco de mod com a lista `outputs`, sem a face e
// sem a regra do slot. Quem registrou na `machinery_basis` (`register_item_instances`) responde pela instância: a face deixa sair e o slot, para
// quem está de fora (`isValidForExtraction` com `EXTERNAL`), deixa tirar (H2, `block_entity_container_instance`).
fn machinery_basis_container_take(containers: &Containers<'_>, view: &View, slot: usize, stack: &ItemStack, face: Direction) -> core::ops::ControlFlow<bool> {
    use core::ops::ControlFlow::{Break, Continue};
    if view.kind != Kind::Mod {
        return Continue(());
    }
    let Some((pos, index)) = view.locate(slot) else {
        return Continue(());
    };
    let Some(BlockEntity::Mod(entity)) = containers.world.block_entities.get(&pos) else {
        return Continue(());
    };
    let state = containers.world.block(pos.0, pos.1, pos.2);
    let own = containers.name(state).to_owned();
    let front = containers.prop(state, "facing").map(str::to_owned);
    let facts = crate::game::machinery_basis::ItemFacts {
        max_stack: containers.max_stack(stack),
        cooking_fuel: containers.items.get(&stack.item).is_some_and(|info| info.fuel.is_some()),
    };
    match crate::game::machinery_basis::container_take(&own, entity, front.as_deref(), face as usize, index, stack, facts) {
        Some(answer) => Break(answer),
        None => Continue(()),
    }
}
