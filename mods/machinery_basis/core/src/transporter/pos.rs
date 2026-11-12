//! `BlockPos` empacotado (`BlockPos.asLong`) e as contas de `WorldUtils` sobre ele.
use crate::sides::Face;
use crate::sim::chunks::BlockPos;

/// `Long.MAX_VALUE`: o valor de "nenhuma posição" que o original guarda nas posições empacotadas.
pub const NO_POS: i64 = i64::MAX;

/// `BlockPos.getX(long)`.
pub fn x_of(packed: i64) -> i32 {
    (packed >> 38) as i32
}

/// `BlockPos.getY(long)`.
pub fn y_of(packed: i64) -> i32 {
    ((packed << 52) >> 52) as i32
}

/// `BlockPos.getZ(long)`.
pub fn z_of(packed: i64) -> i32 {
    ((packed << 26) >> 38) as i32
}

/// `BlockPos.of(long)`.
pub fn block_pos_of(packed: i64) -> BlockPos {
    BlockPos::new(x_of(packed), y_of(packed), z_of(packed))
}

/// `WorldUtils.relativePos(long, direction)`.
pub fn relative_pos(packed: i64, side: Face) -> i64 {
    block_pos_of(packed).relative(side).as_long()
}

/// `WorldUtils.sideDifference(long pos, long other)`: o lado cujo passo é `pos - other`, ou nada se não são vizinhos.
pub fn side_difference(pos: i64, other: i64) -> Option<Face> {
    let diff = [
        x_of(pos).wrapping_sub(x_of(other)),
        y_of(pos).wrapping_sub(y_of(other)),
        z_of(pos).wrapping_sub(z_of(other)),
    ];
    Face::ALL.into_iter().find(|side| side.offset() == diff)
}

/// `WorldUtils.distanceBetween(BlockPos, BlockPos)`: `Math.sqrt(start.distSqr(end))`.
pub fn distance_between(start: BlockPos, end: BlockPos) -> f64 {
    let dx = f64::from(start.x) - f64::from(end.x);
    let dy = f64::from(start.y) - f64::from(end.y);
    let dz = f64::from(start.z) - f64::from(end.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_round_trips_negative_coordinates() {
        for pos in [
            BlockPos::new(0, 0, 0),
            BlockPos::new(-9_022_674, 5, -1_203_382),
            BlockPos::new(33_554_431, -64, -33_554_432),
            BlockPos::new(-1, -1, -1),
        ] {
            assert_eq!(block_pos_of(pos.as_long()), pos);
        }
    }

    #[test]
    fn side_difference_is_the_step_from_other_to_pos() {
        let a = BlockPos::new(10, 5, -3).as_long();
        let b = BlockPos::new(11, 5, -3).as_long();
        assert_eq!(side_difference(b, a), Some(Face::East));
        assert_eq!(side_difference(a, b), Some(Face::West));
        assert_eq!(side_difference(a, a), None);
        assert_eq!(relative_pos(a, Face::East), b);
    }

    #[test]
    fn distance_is_the_euclidean_one() {
        let d = distance_between(BlockPos::new(0, 0, 0), BlockPos::new(3, 4, 0));
        assert_eq!(d, 5.0);
    }
}
