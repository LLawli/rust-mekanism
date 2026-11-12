// Entra em `crates/horizon-client/src/entities.rs` como `pub mod machinery_item_effects` (`visibility = "pub"`). O que o fogo, a lava, a água, o fogo de bloco e o cacto fazem a um item solto
// em um tick, na ordem do `ItemEntity.tick` do original, que o `environment_tick` do Horizon não segue em três pontos (medidos em `rastros/fluido_item_parado.jsonl`):
//
// - **`noPhysics`**: o item que o `ItemEntity.tick` marcou com `noPhysics` (a caixa dele está dentro de uma colisão: um bloco posto onde ele estava, a célula do Heat Generator com lava)
//   não é afetado pelos blocos, porque `Entity.isAffectedByBlocks` é `!isRemoved() && !noPhysics` (`Entity.java:1030`) e é ele que deixa `applyEffectsFromBlocks` rodar o
//   `checkInsideBlocks`, de onde saem todos os `entityInside`: a lava que acende e fere (`LavaFluid.java:121-124`), a água que apaga (`WaterFluid.java:87`), o cacto e o fogo. Fica o
//   `baseTick`;
// - **`firstTick`**: `isInLava` é `!firstTick && ...` (`Entity.java:1782`), então no primeiro tick do item o `baseTick` fere 1 do fogo mesmo dentro da lava;
// - **a ordem**: o `baseTick` (o fogo fere 1 a cada 20 e desce 1) vem antes dos efeitos dos blocos (a água apaga, a lava acende), que rodam depois do movimento, e o Horizon apagava
//   primeiro, então o item em chamas dentro da água não levava o ferimento de 1 do primeiro tick.
//
// Só `std`: o teste do `machinery_basis_core` compila este arquivo como está (`tests/item_effects_reference.rs`).
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

/// O que o tick do item tem de entrada: o fogo que ele traz, o `noPhysics` e o primeiro tick, o fluido da célula e o que os blocos da caixa fazem, e o que o item resiste.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub fire_ticks: i32,
    pub no_physics: bool,
    pub first_tick: bool,
    /// A célula do item tem água / lava (o `FluidState` do bloco).
    pub in_water: bool,
    pub in_lava: bool,
    /// O ferimento do fogo de bloco na caixa (`fire_contact`), se há fogo.
    pub fire_damage: Option<i32>,
    /// A caixa toca um cacto.
    pub cactus: bool,
    pub fire_immune: bool,
    pub resists_on_fire: bool,
    pub resists_lava: bool,
    pub resists_in_fire: bool,
    pub resists_cactus: bool,
}

/// O que sai: o fogo novo e os ferimentos, na ordem em que o original os dá (`hurtServer(amount)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub fire_ticks: i32,
    pub hurts: Vec<i32>,
    /// A lava feriu (o som do `Entity.lavaHurt`).
    pub lava_hurt: bool,
}

/// Um tick do item: o `Entity.baseTick` (`Entity.java:561-575`) e, se os blocos o afetam, os `entityInside` da lava, da água, do fogo e do cacto.
pub fn tick(input: Input) -> Outcome {
    let mut fire = input.fire_ticks;
    let mut hurts = Vec::new();
    let mut lava_hurt = false;
    // `baseTick`: o item imune a fogo apaga; com fogo, `remainingFireTicks % 20 == 0 && !isInLava` fere 1 (a menos que o item resista a `on_fire`) e o fogo desce 1. A água e a lava do bloco
    // não entram aqui (são `entityInside`): `isInLava` vem da altura do fluido, e é falso no primeiro tick.
    if input.fire_immune {
        fire = 0;
    }
    if fire > 0 {
        let in_lava = input.in_lava && !input.first_tick;
        if fire % 20 == 0 && !in_lava && !input.resists_on_fire {
            hurts.push(1);
        }
        fire -= 1;
    }
    if affected_by_blocks(input.no_physics) {
        if input.in_water {
            fire = 0;
        }
        if input.in_lava && !input.fire_immune {
            fire = fire.max(300);
            if !input.resists_lava {
                hurts.push(4);
                lava_hurt = true;
            }
        }
        match input.fire_damage {
            Some(damage) if !input.fire_immune => {
                fire = fire.max(160);
                if !input.resists_in_fire {
                    hurts.push(damage);
                }
            }
            _ => {}
        }
        if input.cactus && !input.resists_cactus {
            hurts.push(1);
        }
    }
    Outcome { fire_ticks: fire, hurts, lava_hurt }
}
