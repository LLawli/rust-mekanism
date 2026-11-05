//! Fluido dentro de um bloco, como água numa escada. Um bloco de qualquer mod entra nisso declarando
//! a propriedade `fluid_logged` com os valores `empty` e `lava`.

pub const PROPERTY: &str = "fluid_logged";
pub const EMPTY: &str = "empty";
pub const LAVA: &str = "lava";

pub const LAVA_BUCKET: &str = "minecraft:lava_bucket";
pub const EMPTY_BUCKET: &str = "minecraft:bucket";

/// O que um clique de balde troca: o valor novo da propriedade, e o balde que sai e o que entra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BucketSwap {
    pub logged: bool,
    pub take: &'static str,
    pub give: &'static str,
}

/// Balde de lava enche um bloco vazio, e balde vazio tira a lava. O resto não troca nada.
pub fn bucket_swap(item: &str, logged: bool) -> Option<BucketSwap> {
    match item {
        LAVA_BUCKET if !logged => Some(BucketSwap {
            logged: true,
            take: LAVA_BUCKET,
            give: EMPTY_BUCKET,
        }),
        EMPTY_BUCKET if logged => Some(BucketSwap {
            logged: false,
            take: EMPTY_BUCKET,
            give: LAVA_BUCKET,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lava_bucket_fills_an_empty_block() {
        let swap = bucket_swap(LAVA_BUCKET, false).unwrap();
        assert!(swap.logged);
        assert_eq!((swap.take, swap.give), (LAVA_BUCKET, EMPTY_BUCKET));
    }

    #[test]
    fn an_empty_bucket_drains_a_logged_block() {
        let swap = bucket_swap(EMPTY_BUCKET, true).unwrap();
        assert!(!swap.logged);
        assert_eq!((swap.take, swap.give), (EMPTY_BUCKET, LAVA_BUCKET));
    }

    #[test]
    fn nothing_swaps_when_the_bucket_has_no_use_here() {
        assert_eq!(bucket_swap(LAVA_BUCKET, true), None);
        assert_eq!(bucket_swap(EMPTY_BUCKET, false), None);
        assert_eq!(bucket_swap("minecraft:water_bucket", false), None);
    }
}
