//! A entidade dentro de um fluido de mod (decisão 18 do plano, H47), as regras do `FluidType` do NeoForge 26.3 com o que o Mekanism declara
//! para os 22 fluidos dele: nada além do `getMekBaseBuilder` (só os sons do balde), ou seja, os padrões do `FluidType.Properties`:
//! `motionScale` 0,014 (`Entity.WATER_FLOW_SCALE`), `canPushEntity`, `canSwim` e `canDrown` verdadeiros, `fallDistanceModifier` 0,5,
//! `canExtinguish` falso, `isWaterLike` falso (`FluidType.java:80-130`, `FluidDeferredRegister.java:67`).
//!
//! O que sai disso, medido no rastro `fluids_entity.jsonl`:
//! - o `LivingEntity.travel` de uma entidade que está num fluido de mod que não é água nem lava não move nada: o 26.3 cai em
//!   `travelInFluid`, que chama `FluidType.move` (padrão `false`) e depois só trata `isInWater` e `isInLava`
//!   (`LivingEntity.java:2594-2607`, o `TODO - 26.3` do `MekanismFluidType`). Sem `move`, sem gravidade e sem atrito: a posição e a
//!   velocidade ficam como estão (a correnteza ainda soma à velocidade);
//! - `Entity.baseTick` multiplica a distância de queda por `fallDistanceModifier` a cada tick fora da água (`Entity.java:577-581`), em vez
//!   de zerá-la como a água faz;
//! - o ar cai 1 por tick com os olhos no fluido (`canDrown`), pelo mesmo `LivingBreatheEvent` da água;
//! - o item solto usa `FluidType.setItemMovement`, o mesmo `setFluidMovement(0,99)` da água, mas só quando a altura do fluido sobre ele
//!   passa de `FLOAT_HEIGHT` (0,1) (`ItemEntity.java:130-132`);
//! - não é "água": `wasTouchingWater` fica falso, então sem respingo, sem apagar o fogo e sem zerar a queda.

/// `FluidType.Properties.motionScale` padrão: `Entity.WATER_FLOW_SCALE`.
pub const MOTION_SCALE: f64 = 0.014;
/// `FluidType.Properties.fallDistanceModifier` padrão.
pub const FALL_DISTANCE_MODIFIER: f32 = 0.5;
/// `ItemEntity.FLOAT_HEIGHT`.
pub const ITEM_FLOAT_HEIGHT: f32 = 0.1;
/// O argumento de `FluidType.setItemMovement` (`entity.setFluidMovement(.99F)`).
pub const ITEM_FLUID_MOVEMENT: f32 = 0.99;
/// O `5.0E-4F` que `ItemEntity.setFluidMovement` soma à subida enquanto ela é menor que `0.06F`.
pub const ITEM_RISE: f32 = 5.0e-4;
/// O limite da subida (`0.06F`).
pub const ITEM_RISE_LIMIT: f32 = 0.06;
/// A folga da caixa de interação (`Entity.getFluidInteractionBox`: a caixa menos 0,001).
pub const INTERACTION_DEFLATE: f64 = 0.001;

/// A quantidade do `FluidState` de um bloco de líquido pelo `level` (`LiquidBlock.getFluidState`): 8 na fonte e no que cai, `8 - level` no
/// que corre. É a mesma conta do RETURN `fluid_state`.
pub fn amount(level: u8) -> u8 {
    match level {
        0 => 8,
        1..=7 => 8 - level,
        _ => 8,
    }
}

/// `FlowingFluid.getHeight`: o bloco cheio se o de cima é do mesmo fluido, senão `amount / 9` (`getOwnHeight`).
pub fn height(level: u8, same_above: bool) -> f32 {
    if same_above {
        1.0
    } else {
        f32::from(amount(level)) / 9.0
    }
}

/// `EntityFluidInteraction.update`: o fluido de uma célula conta para a entidade se o topo dele não fica abaixo do pé da caixa de
/// interação (`fluidTop < box.minY` pula a célula). `feet_min_y` é o `minY` da caixa da entidade.
pub fn reaches_box(cell_y: i32, height: f32, feet_min_y: f64) -> bool {
    let top = f64::from(cell_y) + f64::from(height);
    !(top < feet_min_y + INTERACTION_DEFLATE)
}

/// `LivingEntity.shouldTravelInFluid` seguido de `travelInFluid` para um fluido de mod: a entidade não vai a lugar nenhum quando o
/// fluido da célula dela (`blockPosition`) é de mod e alcança a caixa, ela não está na água nem na lava, o fluido pode afetá-la
/// (`isAffectedByFluids`: o jogador que voa, não) e nada a segura em cima do fluido (`canStandOnFluid`).
pub fn holds_still(
    in_water: bool,
    in_lava: bool,
    mod_fluid_reaches_box: bool,
    affected_by_fluids: bool,
    stands_on_fluid: bool,
) -> bool {
    mod_fluid_reaches_box && !in_water && !in_lava && affected_by_fluids && !stands_on_fluid
}

/// `Entity.baseTick` fora da água e da lava, dentro de um fluido de mod: a queda anda pelo `fallDistanceModifier`.
pub fn fall_distance_in_fluid(fall_distance: f64) -> f64 {
    fall_distance * f64::from(FALL_DISTANCE_MODIFIER)
}

/// `ItemEntity.tick` com a altura do fluido de mod sobre o item (`getFluidTypeHeight`): `Some` se `setItemMovement` vale, com a nova
/// velocidade (`setFluidMovement`), `None` se o item cai como no ar (ou na água do vanilla, que o Horizon já trata).
pub fn item_movement(height: f32, velocity: [f64; 3]) -> Option<[f64; 3]> {
    if height <= ITEM_FLOAT_HEIGHT {
        return None;
    }
    let drag = f64::from(ITEM_FLUID_MOVEMENT);
    let rise = if velocity[1] < f64::from(ITEM_RISE_LIMIT) {
        f64::from(ITEM_RISE)
    } else {
        0.0
    };
    Some([velocity[0] * drag, velocity[1] + rise, velocity[2] * drag])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_follows_the_liquid_block_level() {
        assert_eq!(amount(0), 8);
        assert_eq!(amount(3), 5);
        assert_eq!(amount(7), 1);
        assert_eq!(amount(8), 8);
        assert_eq!(amount(15), 8);
    }

    #[test]
    fn height_is_full_under_the_same_fluid() {
        assert_eq!(height(0, true), 1.0);
        assert_eq!(height(0, false), 8.0 / 9.0);
        assert_eq!(height(3, false), 5.0 / 9.0);
    }

    #[test]
    fn a_fluid_below_the_feet_does_not_reach_the_box() {
        assert!(reaches_box(3, 8.0 / 9.0, 3.0));
        assert!(!reaches_box(3, 8.0 / 9.0, 3.95));
        assert!(reaches_box(3, 1.0, 3.95));
    }

    #[test]
    fn only_a_mod_fluid_alone_holds_the_entity() {
        assert!(holds_still(false, false, true, true, false));
        assert!(!holds_still(true, false, true, true, false));
        assert!(!holds_still(false, true, true, true, false));
        assert!(!holds_still(false, false, false, true, false));
        assert!(!holds_still(false, false, true, false, false));
        assert!(!holds_still(false, false, true, true, true));
    }

    #[test]
    fn item_floats_only_deeper_than_a_tenth() {
        assert_eq!(item_movement(0.1, [0.0; 3]), None);
        let v = item_movement(0.5, [1.0, 0.0, -1.0]).unwrap();
        assert_eq!(v[0], f64::from(0.99_f32));
        assert_eq!(v[1], f64::from(5.0e-4_f32));
        // Acima do limite a subida não cresce mais.
        assert_eq!(item_movement(0.5, [0.0, 0.1, 0.0]).unwrap()[1], 0.1);
    }
}
