//! Como as telas escrevem as unidades.

/// Energia como a tela mostra: `160.0 kJ`, `200.0 J`.
pub fn format_energy(joules: u32) -> String {
    const KILO: f64 = 1_000.0;
    const MEGA: f64 = 1_000_000.0;
    let value = f64::from(joules);
    if value >= MEGA {
        format!("{:.1} MJ", value / MEGA)
    } else if value >= KILO {
        format!("{:.1} kJ", value / KILO)
    } else {
        format!("{value:.1} J")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_reads_as_on_the_screen() {
        assert_eq!(format_energy(160_000), "160.0 kJ");
        assert_eq!(format_energy(200), "200.0 J");
        assert_eq!(format_energy(0), "0.0 J");
        assert_eq!(format_energy(2_500_000), "2.5 MJ");
    }
}
