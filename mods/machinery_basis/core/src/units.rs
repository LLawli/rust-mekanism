//! Como as telas escrevem as unidades: `UnitDisplayUtils.getDisplayBase` e `TextUtils.format`.

/// `UnitDisplayUtils.MeasurementUnit` de `1` para cima: o símbolo e a partir de quanto vale.
const MEASUREMENTS: [(&str, f64); 9] = [
    ("", 1.0),
    ("k", 1_000.0),
    ("M", 1_000_000.0),
    ("G", 1_000_000_000.0),
    ("T", 1_000_000_000_000.0),
    ("P", 1_000_000_000_000_000.0),
    ("E", 1_000_000_000_000_000_000.0),
    ("Z", 1_000_000_000_000_000_000_000.0),
    ("Y", 1_000_000_000_000_000_000_000_000.0),
];

/// `UnitDisplayUtils.roundDecimals`: trunca, não arredonda (`(long) (d * 10^n) / 10^n`).
fn round_decimals(value: f64, places: i32) -> f64 {
    let multiplier = 10f64.powi(places);
    ((value * multiplier) as i64) as f64 / multiplier
}

/// `UnitDisplayUtils.getDisplayBase` com o espaço antes do símbolo, o `compactEnergyDisplay`: `500 FE`, `1.5 kFE`, `0 FE`. O valor
/// passa para a maior medida que ele alcança, com duas casas truncadas e sem o `.0` de um inteiro.
pub fn compact_display(value: u64, unit: &str) -> String {
    if value == 0 {
        return format!("0 {unit}");
    }
    let value = value as f64;
    let (symbol, scale) = MEASUREMENTS
        .iter()
        .rev()
        .find(|(_, scale)| value >= *scale)
        .copied()
        .unwrap_or(MEASUREMENTS[0]);
    let rounded = round_decimals(value / scale, 2);
    // `Double.toString` e o `{}` do Rust dão a mesma forma mais curta, e o `.0` de um inteiro sai nos dois.
    format!("{rounded} {symbol}{unit}")
}

/// `TextUtils.format`: o inteiro com o separador de milhar do idioma, aqui o do inglês (`1,000`).
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Um texto de idioma com seus argumentos (`%1$s`, `%2$s`), como `Component.translatable` os monta. `%%` é um `%`.
pub fn format_lang(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        if let Some(after) = tail.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        let digits = tail.chars().take_while(char::is_ascii_digit).count();
        let index = tail[..digits].parse::<usize>().ok();
        match (index, tail[digits..].strip_prefix("$s")) {
            (Some(index), Some(after)) if index >= 1 => {
                out.push_str(args.get(index - 1).copied().unwrap_or_default());
                rest = after;
            }
            _ => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `EnergyDisplay.getTextComponent` com a energia e a capacidade (`GENERIC_FRACTION`): `energia/capacidade`, cada uma em
/// [`compact_display`], ou só a energia se a capacidade for zero. `fraction` é o texto de idioma com `%1$s` e `%2$s`.
pub fn energy_fraction(energy: u64, capacity: u64, unit: &str, fraction: &str) -> String {
    let stored = compact_display(energy, unit);
    if capacity == 0 {
        return stored;
    }
    format_lang(fraction, &[&stored, &compact_display(capacity, unit)])
}

/// As linhas do tooltip de `GuiEnergyTab` (`updateTooltip`): cada `(modelo, valor)` com o valor como `EnergyDisplay.of(valor)` (compacto, na
/// unidade `unit`), e por último `MekanismLang.UNIT` com a unidade. Os modelos são textos de idioma com `%1$s`.
pub fn energy_info_lines(rows: &[(&str, u64)], unit: &str, unit_template: &str) -> Vec<String> {
    let mut lines: Vec<String> = rows
        .iter()
        .map(|(template, value)| format_lang(template, &[&compact_display(*value, unit)]))
        .collect();
    lines.push(format_lang(unit_template, &[unit]));
    lines
}

/// `GENERIC_STORED_MB`: o nome do que o tanque guarda e a quantidade em mB, com o separador de milhar.
pub fn stored_mb(template: &str, name: &str, amount: u64) -> String {
    format_lang(template, &[name, &grouped(amount)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compact_display_picks_the_largest_unit_and_truncates() {
        assert_eq!(compact_display(0, "FE"), "0 FE");
        assert_eq!(compact_display(1, "FE"), "1 FE");
        assert_eq!(compact_display(500, "FE"), "500 FE");
        assert_eq!(compact_display(999, "FE"), "999 FE");
        assert_eq!(compact_display(1_000, "FE"), "1 kFE");
        assert_eq!(compact_display(1_500, "FE"), "1.5 kFE");
        // Duas casas, truncadas: 1,239 vira 1,23, e 2,999 vira 2,99.
        assert_eq!(compact_display(1_239, "FE"), "1.23 kFE");
        assert_eq!(compact_display(2_999, "FE"), "2.99 kFE");
        assert_eq!(compact_display(160_000, "FE"), "160 kFE");
        assert_eq!(compact_display(256_000_000, "FE"), "256 MFE");
        assert_eq!(compact_display(8_000_000_000, "FE"), "8 GFE");
    }

    #[test]
    fn the_energy_tab_lists_each_value_compact_and_ends_with_the_unit() {
        let lines = energy_info_lines(
            &[("Using: %1$s/t", 50), ("Needed: %1$s", 1_500)],
            "FE",
            "Unit: %1$s",
        );
        assert_eq!(lines, ["Using: 50 FE/t", "Needed: 1.5 kFE", "Unit: FE"]);
        assert_eq!(energy_info_lines(&[], "FE", "Unit: %1$s"), ["Unit: FE"]);
        assert_eq!(
            energy_info_lines(&[("Needed: %1$s", 0)], "FE", "Unit: %1$s")[0],
            "Needed: 0 FE"
        );
    }

    #[test]
    fn thousands_are_grouped_like_the_english_number_format() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(12_345), "12,345");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn a_lang_template_takes_its_arguments_by_position() {
        assert_eq!(format_lang("%1$s/%2$s", &["5 FE", "20 FE"]), "5 FE/20 FE");
        assert_eq!(
            format_lang("%1$s: %2$s mB", &["Carbon", "1,000"]),
            "Carbon: 1,000 mB"
        );
        assert_eq!(
            format_lang("%2$s antes de %1$s", &["a", "b"]),
            "b antes de a"
        );
        assert_eq!(format_lang("%1$s%%", &["50"]), "50%");
        assert_eq!(format_lang("sem argumento", &[]), "sem argumento");
    }

    #[test]
    fn the_energy_fraction_has_both_values_or_just_the_stored_one() {
        assert_eq!(
            energy_fraction(1_500, 8_000, "FE", "%1$s/%2$s"),
            "1.5 kFE/8 kFE"
        );
        assert_eq!(energy_fraction(500, 0, "FE", "%1$s/%2$s"), "500 FE");
    }

    #[test]
    fn a_stored_amount_names_the_content_and_groups_the_thousands() {
        assert_eq!(
            stored_mb("%1$s: %2$s mB", "Lava", 12_000),
            "Lava: 12,000 mB"
        );
    }
}
