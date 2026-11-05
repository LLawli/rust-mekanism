//! Data slots de menu trafegam em 16 bits. Um valor maior vai em dois slots.

/// Os 16 bits baixos e os 16 altos.
pub fn split(value: u32) -> [i32; 2] {
    [(value & 0xFFFF) as i32, (value >> 16) as i32]
}

/// Remonta o valor. O cliente pode receber o slot como negativo: só os 16 bits contam.
pub fn join(low: i32, high: i32) -> u32 {
    (low as u32 & 0xFFFF) | ((high as u32 & 0xFFFF) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_sixteen_bits() {
        for value in [0, 1, 65_535, 65_536, 256_000_000, u32::MAX] {
            let [low, high] = split(value);
            let as_short = |v: i32| i32::from(v as i16);
            assert_eq!(join(as_short(low), as_short(high)), value);
        }
    }
}
