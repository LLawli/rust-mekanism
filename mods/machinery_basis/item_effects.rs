// Entra em `crates/horizon-client/src/entities.rs` como `pub mod machinery_item_effects` (`visibility = "pub"`). O item solto que o `ItemEntity.tick` marcou com `noPhysics` (a caixa dele
// está dentro de uma colisão: um bloco posto onde ele estava, a célula do Heat Generator com lava) não é afetado pelos blocos: `Entity.isAffectedByBlocks` é `!isRemoved() && !noPhysics`
// (`Entity.java:1030`), e é ele que deixa `Entity.applyEffectsFromBlocks` rodar o `checkInsideBlocks`, de onde saem todos os `entityInside` (a lava que acende e fere,
// `LavaFluid.java:121-124`, a água que apaga, `WaterFluid.java:87`, o cacto, o fogo). Sem o efeito do bloco, o que sobra é o do `baseTick`: o fogo que já havia desce 1 por tick e fere 1 a
// cada 20 fora da lava (`Entity.java:561-575`), e `isInLava` (a altura do fluido, não o `entityInside`) continua verdadeiro. Só `std`: o teste do `machinery_basis_core` compila este arquivo
// como está (`tests/item_effects_reference.rs`).
use std::cell::Cell;

thread_local! {
    /// O `noPhysics` do item que `tick_item` acabou de andar: o `environment_tick` do mesmo item, logo depois no mesmo laço de `tick_dropped_items`, o lê. O laço anda um item inteiro
    /// antes do seguinte, então o valor é sempre o do item certo.
    static NO_PHYSICS: Cell<bool> = const { Cell::new(false) };
}

/// `tick_item` começou: o item está com `noPhysics` neste tick?
pub fn set_no_physics(value: bool) {
    NO_PHYSICS.with(|cell| cell.set(value));
}

/// O `noPhysics` do item que acabou de andar.
pub fn no_physics() -> bool {
    NO_PHYSICS.with(Cell::get)
}

/// `Entity.isAffectedByBlocks` (sem o `isRemoved`, que o laço do jogo já decidiu): os blocos só agem sobre quem não está com `noPhysics`.
pub fn affected_by_blocks(no_physics: bool) -> bool {
    !no_physics
}

/// O fogo de um item que os blocos não afetam, depois de um tick: `fire_ticks` é o novo valor e `hurt` diz se o `baseTick` fere 1 (`hurtServer(onFire, 1)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burn {
    pub fire_ticks: i32,
    pub hurt: bool,
}

/// O `Entity.baseTick` de um item que os blocos não afetam: `fireImmune` apaga o fogo; com fogo, `remainingFireTicks % 20 == 0 && !isInLava` fere 1 (a menos que o item resista a
/// `on_fire`) e o fogo desce 1. A água e a lava do bloco não mexem no fogo (são `entityInside`): `in_lava` só decide o ferimento, porque `isInLava` vem da altura do fluido, e no primeiro
/// tick do item ele é falso (`isInLava` é `!firstTick && ...`, `Entity.java:1782`): `first_tick`.
pub fn burn(fire_ticks: i32, in_lava: bool, first_tick: bool, fire_immune: bool, resists_on_fire: bool) -> Burn {
    let in_lava = in_lava && !first_tick;
    let mut fire_ticks = if fire_immune { 0 } else { fire_ticks };
    let mut hurt = false;
    if fire_ticks > 0 {
        hurt = fire_ticks % 20 == 0 && !in_lava && !resists_on_fire;
        fire_ticks -= 1;
    }
    Burn { fire_ticks, hurt }
}
