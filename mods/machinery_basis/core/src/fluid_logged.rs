//! Fluido dentro de um bloco, como água numa escada (`IStateFluidLoggable`). Um bloco de qualquer mod entra nisso declarando
//! a propriedade `fluid_logged` com os valores `empty`, `water` e `lava` (`FluidLogType`).

pub const PROPERTY: &str = "fluid_logged";
pub const EMPTY: &str = "empty";
pub const WATER: &str = "water";
pub const LAVA: &str = "lava";

pub const WATER_BUCKET: &str = "minecraft:water_bucket";
pub const LAVA_BUCKET: &str = "minecraft:lava_bucket";
pub const EMPTY_BUCKET: &str = "minecraft:bucket";

/// O que um clique de balde troca: o valor novo da propriedade, e o balde que sai e o que entra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BucketSwap {
    pub value: &'static str,
    pub take: &'static str,
    pub give: &'static str,
}

/// O valor de `fluid_logged` que o balde cheio `item` põe (`IStateFluidLoggable.setState`, que acha o valor cujo fluido é o do balde).
pub fn value_of_bucket(item: &str) -> Option<&'static str> {
    match item {
        WATER_BUCKET => Some(WATER),
        LAVA_BUCKET => Some(LAVA),
        _ => None,
    }
}

/// O balde cheio que `pickupBlock` dá para o valor `value` (`FluidType.getBucket`): nenhum para `empty`.
pub fn bucket_of_value(value: &str) -> Option<&'static str> {
    match value {
        WATER => Some(WATER_BUCKET),
        LAVA => Some(LAVA_BUCKET),
        _ => None,
    }
}

/// Balde cheio enche um bloco vazio (`canPlaceLiquid`: vazio e fluido válido), e balde vazio tira o fluido (`pickupBlock`). O resto não troca nada.
pub fn bucket_swap(item: &str, current: &str) -> Option<BucketSwap> {
    if current == EMPTY {
        let value = value_of_bucket(item)?;
        return Some(BucketSwap {
            value,
            take: if value == WATER {
                WATER_BUCKET
            } else {
                LAVA_BUCKET
            },
            give: EMPTY_BUCKET,
        });
    }
    if item != EMPTY_BUCKET {
        return None;
    }
    let bucket = bucket_of_value(current)?;
    Some(BucketSwap {
        value: EMPTY,
        take: EMPTY_BUCKET,
        give: bucket,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_bucket_fills_an_empty_block() {
        let swap = bucket_swap(LAVA_BUCKET, EMPTY).unwrap();
        assert_eq!(
            (swap.value, swap.take, swap.give),
            (LAVA, LAVA_BUCKET, EMPTY_BUCKET)
        );
        let swap = bucket_swap(WATER_BUCKET, EMPTY).unwrap();
        assert_eq!(
            (swap.value, swap.take, swap.give),
            (WATER, WATER_BUCKET, EMPTY_BUCKET)
        );
    }

    #[test]
    fn an_empty_bucket_takes_back_the_fluid_that_is_there() {
        let swap = bucket_swap(EMPTY_BUCKET, LAVA).unwrap();
        assert_eq!(
            (swap.value, swap.take, swap.give),
            (EMPTY, EMPTY_BUCKET, LAVA_BUCKET)
        );
        let swap = bucket_swap(EMPTY_BUCKET, WATER).unwrap();
        assert_eq!(
            (swap.value, swap.take, swap.give),
            (EMPTY, EMPTY_BUCKET, WATER_BUCKET)
        );
    }

    #[test]
    fn nothing_swaps_when_the_bucket_has_no_use_here() {
        // `canPlaceLiquid` pede o bloco vazio: com fluido dentro, o balde cheio não troca (a água vai para o bloco ao lado).
        assert_eq!(bucket_swap(LAVA_BUCKET, LAVA), None);
        assert_eq!(bucket_swap(WATER_BUCKET, LAVA), None);
        assert_eq!(bucket_swap(LAVA_BUCKET, WATER), None);
        assert_eq!(bucket_swap(EMPTY_BUCKET, EMPTY), None);
        assert_eq!(bucket_swap("minecraft:powder_snow_bucket", EMPTY), None);
        assert_eq!(bucket_swap("minecraft:milk_bucket", WATER), None);
    }

    #[test]
    fn a_value_with_no_fluid_gives_no_bucket() {
        assert_eq!(bucket_of_value(EMPTY), None);
        assert_eq!(value_of_bucket(EMPTY_BUCKET), None);
    }
}
