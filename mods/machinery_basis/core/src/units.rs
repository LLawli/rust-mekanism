//! Como as telas escrevem as unidades: `UnitDisplayUtils.getDisplayBase` e `TextUtils.format`.

/// `UnitDisplayUtils.MeasurementUnit`, do `FEMTO` ao `YOTTA`: o símbolo e a partir de quanto vale (`value`), na ordem do enum.
const MEASUREMENTS: [(&str, f64); 14] = [
    ("f", 0.000_000_000_000_001),
    ("p", 0.000_000_000_001),
    ("n", 0.000_000_001),
    ("\u{b5}", 0.000_001),
    ("m", 0.001),
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
pub fn round_decimals(value: f64, places: i32) -> f64 {
    let multiplier = 10f64.powi(places);
    ((value * multiplier) as i64) as f64 / multiplier
}

/// `Double.toString` do Java: a forma mais curta que volta ao mesmo `double`, em decimal de `10^-3` (inclusive) a `10^7` (exclusive)
/// e em notação científica (`1.0E-5`, `1.2345E7`) fora disso, sempre com ao menos um dígito depois do ponto.
pub fn java_double_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    let magnitude = value.abs();
    if magnitude == 0.0 || (1.0e-3..1.0e7).contains(&magnitude) {
        // `{:?}` escreve sempre o `.0` de um inteiro, e nesta faixa nunca usa expoente.
        return format!("{value:?}");
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_owned()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exponent}")
}

/// `Float.toString` do Java: como [`java_double_to_string`], para um `float` (a forma mais curta que volta ao mesmo `float`).
pub fn java_float_to_string(value: f32) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    let magnitude = value.abs();
    if magnitude == 0.0 || (1.0e-3..1.0e7).contains(&magnitude) {
        return format!("{value:?}");
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_owned()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exponent}")
}

/// `UnitDisplayUtils.getDisplayBase` sem a unidade: o valor na maior medida que ele alcança (de `f` a `Y`), com `decimals` casas truncadas e sem o `.0` de um inteiro,
/// seguido da medida (`24 k`, `328.22`, `0 `). `space` põe um espaço entre o número e a medida (`1.5 k`); com ele, zero vale `0 ` (o espaço fica). É o texto que o
/// `Component` leva; o símbolo da unidade vai depois, como outro componente (`unit.appendTo(roundedStr + name, false, singular)`).
pub fn display_number(value: f64, decimals: i32, space: bool) -> String {
    if value == 0.0 {
        return if space { "0 " } else { "0" }.to_owned();
    }
    let negative = value < 0.0;
    let value = value.abs();
    let size = MEASUREMENTS.len();
    let mut chosen = MEASUREMENTS[size - 1];
    for (index, lower) in MEASUREMENTS.iter().enumerate() {
        // O primeiro e abaixo dele (nenhuma abreviação antes), o último (nenhuma depois), ou entre este e o próximo.
        if (index == 0 && value < lower.1)
            || index + 1 >= size
            || (value >= lower.1 && value < MEASUREMENTS[index + 1].1)
        {
            chosen = *lower;
            break;
        }
    }
    let processed = value / chosen.1;
    let rounded = round_decimals(if negative { -processed } else { processed }, decimals);
    let text = java_double_to_string(rounded);
    let text = text.strip_suffix(".0").unwrap_or(&text);
    // O espaço vem antes da medida (`name = " " + name`), então `1.5 kFE` e não `1.5k FE`.
    if space {
        format!("{text} {}", chosen.0)
    } else {
        format!("{text}{}", chosen.0)
    }
}

/// `UnitDisplayUtils.getDisplayBase`: [`display_number`] seguido do símbolo da unidade.
pub fn display_base(value: f64, symbol: Option<&str>, decimals: i32, space: bool) -> String {
    format!(
        "{}{}",
        display_number(value, decimals, space),
        symbol.unwrap_or_default()
    )
}

/// `UnitDisplayUtils.getEnergyDisplay` com `compactEnergyDisplay` ligado, o padrão (`getDisplayBase(value, FORGE_ENERGY, 2, true)`):
/// `500 FE`, `1.5 kFE`, `0 FE`.
pub fn compact_display(value: u64, unit: &str) -> String {
    display_base(value as f64, Some(unit), 2, true)
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
    fn the_display_goes_down_to_femto_and_up_to_yotta() {
        let shown = |value: f64| display_base(value, Some("K"), 2, false);
        assert_eq!(shown(0.0), "0K");
        assert_eq!(shown(1.0), "1K");
        assert_eq!(shown(0.5), "500mK");
        assert_eq!(shown(0.001), "1mK");
        assert_eq!(shown(0.000_5), "500\u{b5}K");
        assert_eq!(shown(2.5e-9), "2.5nK");
        assert_eq!(shown(3.0e-12), "3pK");
        // 1,5e-15 não é exato em binário e fica um pouco abaixo: o truncamento dá 1,49, como no Java.
        assert_eq!(shown(1.5e-15), "1.49fK");
        // Abaixo do femto o número continua em femto, truncado a duas casas: `0.0` perde o `.0`.
        assert_eq!(shown(1.0e-20), "0fK");
        assert_eq!(shown(999.999), "999.99K");
        assert_eq!(shown(1_000.0), "1kK");
        assert_eq!(shown(2.0e24), "2YK");
        // Acima do yotta o número cresce, em notação científica a partir de 10^7.
        assert_eq!(shown(5.0e27), "5000YK");
        assert_eq!(shown(5.0e31), "5.0E7YK");
        assert_eq!(shown(-250.0), "-250K");
        assert_eq!(shown(-0.5), "-500mK");
    }

    #[test]
    fn a_space_goes_before_the_measure_and_zero_keeps_the_symbol() {
        assert_eq!(display_base(1_500.0, Some("FE"), 2, true), "1.5 kFE");
        assert_eq!(display_base(0.0, Some("FE"), 2, true), "0 FE");
        assert_eq!(display_base(7.0, None, 2, false), "7");
        assert_eq!(display_base(0.0, None, 2, false), "0");
        assert_eq!(display_base(12_345.678, None, 1, false), "12.3k");
    }

    #[test]
    fn rounding_truncates_toward_zero() {
        assert_eq!(round_decimals(1.239, 2), 1.23);
        assert_eq!(round_decimals(-1.239, 2), -1.23);
        assert_eq!(round_decimals(2.999, 2), 2.99);
        assert_eq!(round_decimals(5.0, 2), 5.0);
    }

    /// Saídas conhecidas de `Double.toString`.
    #[test]
    fn doubles_print_like_java() {
        for (value, text) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (300.0, "300.0"),
            (0.001, "0.001"),
            (0.000_999, "9.99E-4"),
            (1.0e-5, "1.0E-5"),
            (9_999_999.0, "9999999.0"),
            (1.0e7, "1.0E7"),
            (1.2345e7, "1.2345E7"),
            (1.0e10, "1.0E10"),
            (-2.5e-7, "-2.5E-7"),
            (123_456.789, "123456.789"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(java_double_to_string(value), text, "{value:e}");
        }
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
