//! `Block.popResourceFromFace` do vanilla (`Block.java:416-430`): onde nasce e com que velocidade sai o item que um bloco solta por uma face (o `EMPTY` do
//! Configurator, `InventoryUtils.dropStack`). O item tem 0,25 de largura e de altura (`EntityTypes.ITEM`).

/// Metade da largura e da altura do item solto (`EntityTypes.ITEM.getWidth() / 2.0`, `getHeight() / 2.0`).
pub const ITEM_HALF: f64 = 0.125;

/// `Mth.nextDouble(random, min, max)`.
fn between(next_double: &mut impl FnMut() -> f64, min: f64, max: f64) -> f64 {
    if min >= max {
        min
    } else {
        next_double() * (max - min) + min
    }
}

/// A posição e a velocidade do item que sai do bloco em `pos` pela face `step` (`face.getStepX/Y/Z`), com os sorteios na ordem do Java: a posição em x, y e z,
/// depois a velocidade em x, y e z, cada um só no eixo que não é o da face.
pub fn from_face(
    pos: [i32; 3],
    step: [i32; 3],
    mut next_double: impl FnMut() -> f64,
) -> ([f64; 3], [f64; 3]) {
    let mut at = [0.0; 3];
    for axis in 0..3 {
        // `0.5 + halfWidth` em x e z e `0.5 + halfHeight` em y: as duas metades valem 0,125.
        at[axis] = f64::from(pos[axis])
            + 0.5
            + if step[axis] == 0 {
                between(&mut next_double, -0.25, 0.25)
            } else {
                f64::from(step[axis]) * (0.5 + ITEM_HALF)
            };
    }
    // `- halfHeight` só em y.
    at[1] -= ITEM_HALF;
    let mut motion = [0.0; 3];
    for axis in 0..3 {
        motion[axis] = match (axis, step[axis]) {
            (1, 0) => between(&mut next_double, 0.0, 0.1),
            (1, s) => f64::from(s) * 0.1 + 0.1,
            (_, 0) => between(&mut next_double, -0.1, 0.1),
            (_, s) => f64::from(s) * 0.1,
        };
    }
    (at, motion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_leaves_a_quarter_and_a_half_out_of_the_face_and_centered_on_its_half_height() {
        let mut draws = [0.0, 1.0, 0.5, 0.25].into_iter();
        // Pela face norte de (3, -60, 3): x e y sorteados, z a 0,625 do centro para fora; depois a velocidade em x e em y.
        let (at, motion) = from_face([3, -60, 3], [0, 0, -1], || draws.next().unwrap());
        assert_eq!(at, [3.25, -60.0 + 0.75 - 0.125, 2.875]);
        assert_eq!(motion, [0.0, 0.025, -0.1]);
        // Pela face de cima: y a 0,625 do centro menos a meia altura; a velocidade em y é 0,2.
        let mut draws = [0.5, 0.5, 0.5, 0.5].into_iter();
        let (at, motion) = from_face([0, 0, 0], [0, 1, 0], || draws.next().unwrap());
        assert_eq!(at, [0.5, 1.0, 0.5]);
        assert_eq!(motion, [0.0, 0.2, 0.0]);
    }
}
