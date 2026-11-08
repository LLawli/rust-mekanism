// Entra em `crates/horizon-client/src/animate_tick.rs` como `pub mod machinery_animate` (`visibility = "pub"`).
// `Block.animateTick` dos blocos de mod (H39): o cliente sorteia, a cada tick, as posições em volta do jogador e chama o `animateTick` do bloco de cada uma
// (`ClientLevel.animateTick`, `animate_tick.rs` do Horizon). O Horizon só anima os blocos do vanilla, por nome. Este módulo guarda os animadores que os mods
// registram (um por mod, na primeira vez que o `tick` do cliente passa) e a injeção `animate_tick` os chama no começo de `block_animate_tick`, para cada
// posição sorteada, com o mesmo gerador de números do sorteio. O módulo é público porque quem registra é o crate do jogo (`horizon-client`), que depende
// deste (`horizon-client-lib`).
use super::AnimateContext;
use crate::particles::ParticleEngine;
use horizon_world::registry::BlockStateId;
use std::sync::Mutex;

/// O `animateTick` de um mod para a posição sorteada: o estado do bloco, a posição, o motor de partículas (que tem o gerador do sorteio) e o mundo.
pub type Animator = fn(&mut ParticleEngine, &AnimateContext<'_>, BlockStateId, [i32; 3]);

static ANIMATORS: Mutex<Vec<(&'static str, Animator)>> = Mutex::new(Vec::new());

/// Registra o animador do mod `owner`. Registrar de novo o mesmo `owner` não faz nada: o tick do cliente chama isto todo tick.
pub fn register(owner: &'static str, animator: Animator) {
    let mut animators = ANIMATORS.lock().unwrap_or_else(|error| error.into_inner());
    if !animators.iter().any(|(name, _)| *name == owner) {
        animators.push((owner, animator));
    }
}

/// Chama cada animador registrado para a posição sorteada.
pub fn animate(engine: &mut ParticleEngine, ctx: &AnimateContext<'_>, state: BlockStateId, pos: [i32; 3]) {
    let animators = ANIMATORS.lock().unwrap_or_else(|error| error.into_inner());
    for (_, animator) in animators.iter() {
        animator(engine, ctx, state, pos);
    }
}
