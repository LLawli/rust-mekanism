fn machinery_basis_item_effects(
    item: &mut ItemEntity,
    world: &World,
    facts: &BlockFacts,
    items: &crate::items::Items,
) -> core::ops::ControlFlow<ItemEnvironmentTick> {
    // Um item com `noPhysics` neste tick (dentro de uma colisão) não é afetado pelos blocos (`Entity.isAffectedByBlocks`): nem a lava acende e fere, nem a água apaga, nem o
    // cacto, nem o fogo. Fica o `baseTick`, que `machinery_item_effects::burn` repete.
    if machinery_item_effects::affected_by_blocks(machinery_item_effects::no_physics()) {
        return core::ops::ControlFlow::Continue(());
    }
    let at = world.block(
        item.position.x.floor() as i32,
        (item.position.y + 0.1).floor() as i32,
        item.position.z.floor() as i32,
    );
    let burn = machinery_item_effects::burn(
        item.fire_ticks,
        facts.get(at).fluid == Fluid::Lava,
        // `tick_move` já contou este tick: o primeiro é o de `ticks == 1`.
        item.ticks <= 1,
        items.resists_fire(&item.stack, "minecraft:in_fire"),
        items.resists_fire(&item.stack, "minecraft:on_fire"),
    );
    item.fire_ticks = burn.fire_ticks;
    let spilled = if burn.hurt { item.hurt(1).unwrap_or_default() } else { Vec::new() };
    core::ops::ControlFlow::Break(ItemEnvironmentTick {
        alive: item.health > 0,
        lava_hurt: false,
        spilled,
    })
}
