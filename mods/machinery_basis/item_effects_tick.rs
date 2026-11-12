fn machinery_basis_item_effects(
    item: &mut ItemEntity,
    world: &World,
    facts: &BlockFacts,
    registry: &horizon_world::registry::BlockRegistry,
    items: &crate::items::Items,
) -> core::ops::ControlFlow<ItemEnvironmentTick> {
    // O `ItemEntity.tick` do original no que o `environment_tick` do Horizon faz: o `baseTick` do fogo antes dos efeitos dos blocos, o `isInLava` falso no primeiro tick e os efeitos
    // dos blocos só para quem não está com `noPhysics` (`machinery_item_effects::tick`). O `noPhysics` é o que `tick_item` deste item acabou de guardar.
    let at = world.block(
        item.position.x.floor() as i32,
        (item.position.y + 0.1).floor() as i32,
        item.position.z.floor() as i32,
    );
    let fluid = facts.get(at).fluid;
    let affected = machinery_item_effects::affected_by_blocks(machinery_item_effects::no_physics());
    let fire_damage = if affected { crate::motion::environment::fire_contact(world, registry, item.bounds()) } else { 0.0 };
    let outcome = machinery_item_effects::tick(machinery_item_effects::Input {
        fire_ticks: item.fire_ticks,
        no_physics: !affected,
        // `tick_move` já contou este tick: o primeiro é o de `ticks == 1`.
        first_tick: item.ticks <= 1,
        in_water: fluid == Fluid::Water,
        in_lava: fluid == Fluid::Lava,
        fire_damage: if fire_damage > 0.0 { Some(fire_damage as i32) } else { None },
        cactus: affected && crate::motion::environment::cactus_contact(world, registry, item.bounds()),
        fire_immune: items.resists_fire(&item.stack, "minecraft:in_fire"),
        resists_on_fire: items.resists_fire(&item.stack, "minecraft:on_fire"),
        resists_lava: items.resists_fire(&item.stack, "minecraft:lava"),
        resists_in_fire: items.resists_fire(&item.stack, "minecraft:in_fire"),
        resists_cactus: items.resists_cactus(&item.stack),
    });
    item.fire_ticks = outcome.fire_ticks;
    let mut spilled = Vec::new();
    for amount in outcome.hurts {
        spilled.extend(item.hurt(amount).unwrap_or_default());
    }
    core::ops::ControlFlow::Break(ItemEnvironmentTick {
        alive: item.health > 0,
        lava_hurt: outcome.lava_hurt,
        spilled,
    })
}
