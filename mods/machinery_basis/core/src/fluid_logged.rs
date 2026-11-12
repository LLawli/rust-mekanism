//! Fluido dentro de um bloco, como água numa escada (`IStateFluidLoggable`). Um bloco de qualquer mod entra nisso declarando
//! a propriedade `fluid_logged` com os valores `empty`, `water` e `lava` (`FluidLogType`).

pub const PROPERTY: &str = "fluid_logged";
pub const EMPTY: &str = "empty";
pub const WATER: &str = "water";
pub const LAVA: &str = "lava";

pub const WATER_BUCKET: &str = "minecraft:water_bucket";
pub const LAVA_BUCKET: &str = "minecraft:lava_bucket";
pub const EMPTY_BUCKET: &str = "minecraft:bucket";
const COD_BUCKET: &str = "minecraft:cod_bucket";
const SALMON_BUCKET: &str = "minecraft:salmon_bucket";
const PUFFERFISH_BUCKET: &str = "minecraft:pufferfish_bucket";
const TROPICAL_FISH_BUCKET: &str = "minecraft:tropical_fish_bucket";
const AXOLOTL_BUCKET: &str = "minecraft:axolotl_bucket";
const TADPOLE_BUCKET: &str = "minecraft:tadpole_bucket";
const POWDER_SNOW_BUCKET: &str = "minecraft:powder_snow_bucket";
const SULFUR_CUBE_BUCKET: &str = "minecraft:sulfur_cube_bucket";

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

/// O que o conteúdo de um balde cheio é para `BucketItem.emptyContents` (`DispensibleContainerItem`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// Água ou lava, ou um balde de peixe (que guarda água): o fluido entra num bloco que o aceita (`LiquidBlockContainer.placeLiquid`).
    Fluid(&'static str),
    /// O que só age num lugar vazio (`SolidBucketItem`, a neve em pó) ou não tem fluido (o balde do cubo de enxofre): num bloco com `fluid_logged` não esvazia.
    Other,
}

/// O conteúdo do balde cheio `item` (`DispensibleContainerItem`): nenhum para o que não é balde cheio.
pub fn content_of(item: &str) -> Option<Content> {
    match item {
        WATER_BUCKET | COD_BUCKET | SALMON_BUCKET | PUFFERFISH_BUCKET | TROPICAL_FISH_BUCKET | AXOLOTL_BUCKET | TADPOLE_BUCKET => Some(Content::Fluid(WATER)),
        LAVA_BUCKET => Some(Content::Fluid(LAVA)),
        POWDER_SNOW_BUCKET | SULFUR_CUBE_BUCKET => Some(Content::Other),
        _ => None,
    }
}

/// O que um dispenser com um balde faz contra o bloco da frente quando ele tem a propriedade `fluid_logged` (`IStateFluidLoggable`, que é `LiquidBlockContainer` e `BucketPickup`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispensed {
    /// O bloco não tem `fluid_logged`: o dispenser do Horizon decide.
    Pass,
    /// `emptyContents` pôs o fluido no bloco: o balde cheio vira o vazio (`consumeWithRemainder`).
    Fill { value: &'static str },
    /// `pickupBlock` tirou o fluido do bloco: o balde vazio vira o cheio `bucket`.
    Pickup { value: &'static str, bucket: &'static str },
    /// Não deu: o dispenser solta o item como o `DefaultDispenseItemBehavior`.
    Drop,
}

/// `DispenserBlock.registerBehavior` dos baldes (`DispenseItemBehavior`) contra um bloco com `fluid_logged` (`current` é o valor dele, `None` sem a propriedade; `declares` diz se o bloco
/// declara um valor, o Heat Generator só tem `empty` e `lava`). O dispenser chama `emptyContents(null, level, pos, null, stack)`: `canBeReplaced(fluid)` é falso nos blocos do
/// Mekanism (`BlockMekanism.java:79`), então só `canPlaceLiquid` (vazio e fluido válido) deixa o fluido entrar, e `hitResult` nulo impede o balde de ir para o bloco ao lado. O balde
/// vazio pergunta `instanceof BucketPickup` e `pickupBlock` (vazio sem fluido: solta o balde).
pub fn dispense_bucket(item: &str, current: Option<&str>, declares: impl Fn(&str) -> bool) -> Dispensed {
    let Some(current) = current else {
        return Dispensed::Pass;
    };
    if item == EMPTY_BUCKET {
        return match bucket_of_value(current) {
            Some(bucket) => Dispensed::Pickup { value: EMPTY, bucket },
            None => Dispensed::Drop,
        };
    }
    match content_of(item) {
        Some(Content::Fluid(value)) if current == EMPTY && declares(value) => Dispensed::Fill { value },
        Some(_) => Dispensed::Drop,
        None => Dispensed::Pass,
    }
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
