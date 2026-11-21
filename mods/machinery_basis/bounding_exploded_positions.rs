fn machinery_basis_bounding_exploded_positions(game: &mut crate::game::Game<'_>, center: [f64; 3], radius: f32, resists: Option<([i32; 3], f32)>) -> core::ops::ControlFlow<Vec<[i32; 3]>> {
    // `BlockBounding.getExplosionResistance`: a resistência do apoio é a do principal dele. Sem apoio ao alcance da explosão o corpo do jogo segue como é.
    if !crate::game::machinery_basis::bounding_near(game, center, radius) {
        return core::ops::ControlFlow::Continue(());
    }
    // O mesmo corpo de `Game::exploded_positions` (Horizon f319701, `game/explosion.rs:295-373`), com a resistência do principal no lugar da do estado do apoio.
    let (min_y, max_y) = game.with_level(|_, level, _| {
        use horizon_world::level::LevelAccess;
        (level.min_y(), level.max_y())
    });
    // WitherSkull.getBlockExplosionResistance: a dangerous skull's blast
    // caps a block the wither may destroy at 0.8.
    let cap = game.combat.blast_resistance_cap;
    let wither_immune = cap.and_then(|_| {
        game.res
            .items()
            .block_tags
            .get("minecraft:wither_immune")
            .cloned()
    });
    let mut set: JavaHashMap<Key, ()> = JavaHashMap::default();
    for xx in 0..16 {
        for yy in 0..16 {
            for zz in 0..16 {
                if !(xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                    continue;
                }
                let mut xd = f64::from(xx as f32 / 15.0 * 2.0 - 1.0);
                let mut yd = f64::from(yy as f32 / 15.0 * 2.0 - 1.0);
                let mut zd = f64::from(zz as f32 / 15.0 * 2.0 - 1.0);
                let d = (xd * xd + yd * yd + zd * zd).sqrt();
                xd /= d;
                yd /= d;
                zd /= d;
                let mut power = radius * (0.7 + game.world.random.next_float() * 0.6);
                let (mut xp, mut yp, mut zp) = (center[0], center[1], center[2]);
                while power > 0.0 {
                    let pos = [xp.floor() as i32, yp.floor() as i32, zp.floor() as i32];
                    if pos[1] < min_y
                        || pos[1] >= max_y
                        || !(-30_000_000..30_000_000).contains(&pos[0])
                        || !(-30_000_000..30_000_000).contains(&pos[2])
                    {
                        break;
                    }
                    let state = game.world.block(pos[0], pos[1], pos[2]);
                    let f = game.res.facts().get(state);
                    let fluid = if f.fluid != Fluid::None {
                        100.0f32
                    } else {
                        0.0
                    };
                    // A primed TNT minecart's rails and the blocks they
                    // stand on: no resistance, and never blown up.
                    let spared = game.combat.rail_proof_blast
                        && (game.rail(pos[0], pos[1], pos[2]).is_some()
                            || game.rail(pos[0], pos[1] + 1, pos[2]).is_some());
                    match resists {
                        Some((at, resistance)) if at == pos => {
                            power -= (resistance + 0.3) * 0.3;
                        }
                        _ if !game.res.facts().is_air(state) || f.fluid != Fluid::None => {
                            let base = if spared {
                                0.0
                            } else {
                                crate::game::machinery_basis::bounding_explosion_resistance(game, pos).unwrap_or(f.explosion_resistance).max(fluid)
                            };
                            let immune =
                                wither_immune.as_ref().is_some_and(|t| t.contains(&*f.name));
                            power -= (blast_resistance(base, cap, immune) + 0.3) * 0.3;
                        }
                        _ => {}
                    }
                    if power > 0.0 && !spared {
                        set.put(Key(pos), ());
                    }
                    xp += xd * f64::from(0.3f32);
                    yp += yd * f64::from(0.3f32);
                    zp += zd * f64::from(0.3f32);
                    power -= 0.225_000_01;
                }
            }
        }
    }
    core::ops::ControlFlow::Break(set.entries().map(|(k, _)| k.0).collect())
}
