//! Data slots de menu trafegam em 16 bits. Um valor maior vai em dois slots.

/// Os 16 bits baixos e os 16 altos.
pub fn split(value: u32) -> [i32; 2] {
    [(value & 0xFFFF) as i32, (value >> 16) as i32]
}

/// Remonta o valor. O cliente pode receber o slot como negativo: só os 16 bits contam.
pub fn join(low: i32, high: i32) -> u32 {
    (low as u32 & 0xFFFF) | ((high as u32 & 0xFFFF) << 16)
}

/// Um texto curto em `N` data slots, dois bytes em cada (o primeiro nos 8 bits altos), zero no fim. Só serve a texto sem `\0`, como um
/// id de recurso (ASCII). O que não cabe em `2 * N` bytes é cortado.
pub fn pack_text<const N: usize>(text: &str) -> [u16; N] {
    let mut slots = [0u16; N];
    for (index, byte) in text.bytes().take(2 * N).enumerate() {
        slots[index / 2] |= u16::from(byte) << if index % 2 == 0 { 8 } else { 0 };
    }
    slots
}

/// O texto de [`pack_text`]: os bytes até o primeiro zero.
pub fn unpack_text(slots: &[u16]) -> String {
    let bytes: Vec<u8> = slots
        .iter()
        .flat_map(|slot| slot.to_be_bytes())
        .take_while(|byte| *byte != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
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

    #[test]
    fn a_short_text_round_trips_through_the_slots() {
        for text in [
            "",
            "a",
            "ab",
            "mekanism:carbon",
            "mekanism:refined_obsidian",
        ] {
            let packed = pack_text::<16>(text);
            // Pelo fio, cada slot é um `i16` com sinal: só os 16 bits contam.
            let wire: Vec<u16> = packed
                .iter()
                .map(|&slot| i32::from(slot as i16) as u16)
                .collect();
            assert_eq!(unpack_text(&wire), text);
        }
    }

    #[test]
    fn a_text_longer_than_the_slots_is_cut() {
        assert_eq!(unpack_text(&pack_text::<2>("abcdef")), "abcd");
    }
}
