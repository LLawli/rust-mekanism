// Roda no começo de `Containers::view`, o contêiner de itens do bloco em `pos`, e o chama de novo por dentro (a marca `view_enter` faz a segunda
// passagem seguir para o corpo original; o corpo tem `?`, que o RETURN não aceita). O `ModContainer` de um bloco de mod é o do tipo de bloco, e
// quem registrou na `machinery_basis` (`register_item_instances`) o troca pelo da instância, com as faces, as entradas e as saídas que a
// configuração de lados dá (H2, `block_entity_container_instance`). O que não é bloco de mod, ou ninguém reclama, passa como o Horizon o deu.
fn machinery_basis_container_instance(containers: &Containers<'_>, pos: Pos, blocked_chests: bool) -> core::ops::ControlFlow<Option<View>> {
    use core::ops::ControlFlow::{Break, Continue};
    if !crate::game::machinery_basis::view_enter() {
        return Continue(());
    }
    let found = containers.view(pos, blocked_chests);
    crate::game::machinery_basis::view_leave();
    let Some(mut view) = found else {
        return Break(None);
    };
    if view.kind == Kind::Mod
        && let Some(BlockEntity::Mod(entity)) = containers.world.block_entities.get(&pos)
    {
        let state = containers.world.block(pos.0, pos.1, pos.2);
        let own = containers.name(state).to_owned();
        let front = containers.prop(state, "facing").map(str::to_owned);
        if let Some(container) = crate::game::machinery_basis::container_instance(&own, entity, front.as_deref(), view.container.as_deref()) {
            view.container = Some(Arc::new(container));
        }
    }
    Break(Some(view))
}
