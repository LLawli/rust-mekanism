//! A energia guardada num item (`ContainerType.ENERGY`, `EnergyContainerType`, `ComponentBackedEnergyContainer`): o valor é um `long` no componente do item, a barra de
//! durabilidade e a linha do tooltip saem dele, e a máquina posta ou quebrada o copia entre o item e o tile (`copyToContainer`, `attachedCopyOf`). Aqui está a conta, sem
//! nada do jogo: quais itens guardam energia e com que capacidade é dado do mod.

/// `Item.MAX_BAR_WIDTH`.
pub const MAX_BAR_WIDTH: i32 = 13;

/// `MathUtils.divideToLevel(numerator, toDivide)`: `1` com capacidade zero ou com a energia no teto ou acima, senão a razão.
pub fn divide_to_level(amount: i64, capacity: i64) -> f64 {
    let (numerator, to_divide) = (amount as f64, capacity as f64);
    if to_divide == 0.0 || numerator >= to_divide {
        1.0
    } else {
        numerator / to_divide
    }
}

/// `StorageUtils.getBarWidth`: `Math.clamp(Math.round(MAX_BAR_WIDTH * (float) ratio), 0, MAX_BAR_WIDTH)`. A conta é em `float`, e `Math.round(float)` arredonda para cima nos empates.
pub fn bar_width(ratio: f64) -> i32 {
    let scaled = (MAX_BAR_WIDTH as f32) * (ratio as f32);
    let rounded = (f64::from(scaled) + 0.5).floor();
    (rounded as i64).clamp(0, i64::from(MAX_BAR_WIDTH)) as i32
}

/// `EnergyContainerType.isBarVisible` de um item que guarda energia: `StorageUtils.shouldDisplayBar` (a pilha é de um item só, senão a barra cobriria a contagem) e algo guardado.
pub fn is_bar_visible(count: i32, amount: i64) -> bool {
    count == 1 && amount > 0
}

/// `EnergyContainerType.getBarWidth` de um item que guarda energia.
pub fn item_bar_width(amount: i64, capacity: i64) -> i32 {
    bar_width(divide_to_level(amount, capacity))
}

/// `EnergyContainerType.copyToContainer`: a energia do item vai para o contêiner limitada à capacidade ("clamp contents to the max amount of energy that can be stored").
/// Não limita na ida para o item (`attachedCopyOf`: "should we clamp on pick up as well?").
pub fn copy_to_container(stored: i64, capacity: i64) -> i64 {
    stored.min(capacity)
}

/// `EnergyContainerType.getFilledVariant`: o item cheio guarda a capacidade.
pub fn filled(capacity: i64) -> i64 {
    capacity
}

/// `EnergyDisplay.getTextComponent`: `Infinite` com a energia no teto de `long`, a energia compacta sozinha com capacidade zero, senão `GENERIC_FRACTION` com a energia e a
/// capacidade (`Infinite` se ela está no teto). `fraction` é o texto de idioma com `%1$s` e `%2$s`.
pub fn energy_display(energy: i64, max: i64, unit: &str, fraction: &str, infinite: &str) -> String {
    let (energy, max) = (energy.max(0), max.max(0));
    if energy == i64::MAX {
        return infinite.to_owned();
    }
    let stored = crate::units::compact_display(energy as u64, unit);
    if max == 0 {
        return stored;
    }
    let capacity = if max == i64::MAX {
        infinite.to_owned()
    } else {
        crate::units::compact_display(max as u64, unit)
    };
    crate::units::format_lang(fraction, &[&stored, &capacity])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_is_one_with_no_capacity_or_a_full_buffer() {
        assert_eq!(divide_to_level(0, 0), 1.0);
        assert_eq!(divide_to_level(5, 5), 1.0);
        assert_eq!(divide_to_level(9, 5), 1.0);
        assert_eq!(divide_to_level(1, 4), 0.25);
        assert_eq!(divide_to_level(0, 4), 0.0);
    }

    #[test]
    fn the_bar_rounds_half_up_in_float() {
        assert_eq!(bar_width(0.0), 0);
        assert_eq!(bar_width(1.0), 13);
        // 13 * 0.5 = 6.5 rounds up to 7
        assert_eq!(bar_width(0.5), 7);
        // 13 * 0.03846 = 0.4999 rounds down to 0, 13 * 0.0385 = 0.5005 up to 1
        assert_eq!(bar_width(0.0384), 0);
        assert_eq!(bar_width(0.0385), 1);
        assert_eq!(bar_width(7.0), 13);
        assert_eq!(bar_width(-1.0), 0);
    }

    #[test]
    fn the_bar_shows_for_one_item_with_something_stored() {
        assert!(is_bar_visible(1, 1));
        assert!(!is_bar_visible(1, 0));
        assert!(!is_bar_visible(2, 100));
    }

    #[test]
    fn the_contents_are_clamped_only_on_the_way_into_the_container() {
        assert_eq!(copy_to_container(5_000, 1_000), 1_000);
        assert_eq!(copy_to_container(500, 1_000), 500);
        assert_eq!(copy_to_container(0, 0), 0);
    }

    #[test]
    fn the_display_is_a_fraction_unless_the_capacity_is_zero_or_the_energy_infinite() {
        let fraction = "%1$s/%2$s";
        assert_eq!(
            energy_display(500, 1_000, "FE", fraction, "Infinite"),
            "500 FE/1 kFE"
        );
        assert_eq!(energy_display(500, 0, "FE", fraction, "Infinite"), "500 FE");
        assert_eq!(
            energy_display(i64::MAX, i64::MAX, "FE", fraction, "Infinite"),
            "Infinite"
        );
        assert_eq!(
            energy_display(10, i64::MAX, "FE", fraction, "Infinite"),
            "10 FE/Infinite"
        );
        assert_eq!(
            energy_display(-5, 100, "FE", fraction, "Infinite"),
            "0 FE/100 FE"
        );
    }
}
