//! A parte de `net.minecraft.util.Mth` que as telas e o HUD dos mods usam, com a mesma aproximação do original: a tabela de 65.536 senos e o `atan2`
//! rápido. O desenho depende do arredondamento deles (`GuiRadialSelector` escolhe a fatia pelo ângulo do mouse), então não se troca por `f64::sin`.

use std::sync::OnceLock;

/// `Mth.DEG_TO_RAD` e `Mth.RAD_TO_DEG` (`float`).
pub const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;
pub const RAD_TO_DEG: f32 = 180.0_f32 / std::f64::consts::PI as f32;

const SIN_SCALE: f64 = 10430.378350470453;
const LUT_SIZE: usize = 257;
/// `Double.longBitsToDouble(4805340802404319232L)`.
const FRAC_BIAS_BITS: u64 = 4805340802404319232;

struct Tables {
    sin: Vec<f32>,
    asin: [f64; LUT_SIZE],
    cos: [f64; LUT_SIZE],
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let sin = (0..65536)
            .map(|i| (f64::from(i) / SIN_SCALE).sin() as f32)
            .collect();
        let mut asin = [0.0; LUT_SIZE];
        let mut cos = [0.0; LUT_SIZE];
        for index in 0..LUT_SIZE {
            let v = index as f64 / 256.0;
            let a = v.asin();
            cos[index] = a.cos();
            asin[index] = a;
        }
        Tables { sin, asin, cos }
    })
}

/// `Mth.sin(double)`.
pub fn sin(i: f64) -> f32 {
    tables().sin[((i * SIN_SCALE) as i64 & 65535) as usize]
}

/// `Mth.cos(double)`.
pub fn cos(i: f64) -> f32 {
    tables().sin[((i * SIN_SCALE + 16384.0) as i64 & 65535) as usize]
}

/// `Mth.fastInvSqrt(double)`.
fn fast_inv_sqrt(x: f64) -> f64 {
    let half = 0.5 * x;
    let i = x.to_bits() as i64;
    let i = 6910469410427058090_i64.wrapping_sub(i >> 1);
    let x = f64::from_bits(i as u64);
    x * (1.5 - half * x * x)
}

/// `Mth.atan2(double y, double x)`.
pub fn atan2(y: f64, x: f64) -> f64 {
    let d2 = x * x + y * y;
    if d2.is_nan() {
        return f64::NAN;
    }
    let (mut y, mut x) = (y, x);
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    let rinv = fast_inv_sqrt(d2);
    x *= rinv;
    y *= rinv;
    let frac_bias = f64::from_bits(FRAC_BIAS_BITS);
    let yp = frac_bias + y;
    let index = yp.to_bits() as u32 as i32 as usize;
    let tables = tables();
    let phi = tables.asin[index];
    let c_phi = tables.cos[index];
    let s_phi = yp - frac_bias;
    let sd = y * c_phi - x * s_phi;
    let d = (6.0 + sd * sd) * sd * 0.16666666666666666;
    let mut theta = phi + d;
    if steep {
        theta = std::f64::consts::FRAC_PI_2 - theta;
    }
    if neg_x {
        theta = std::f64::consts::PI - theta;
    }
    if neg_y {
        theta = -theta;
    }
    theta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_sine_is_the_quantised_sine() {
        assert_eq!(sin(0.0), 0.0);
        // 65536 passos por volta: o erro máximo é meio passo (cerca de 4,8e-5 rad).
        for step in 0..720 {
            let angle = f64::from(step) * std::f64::consts::PI / 360.0;
            assert!((f64::from(sin(angle)) - angle.sin()).abs() < 1.0e-4);
            assert!((f64::from(cos(angle)) - angle.cos()).abs() < 1.0e-4);
        }
    }

    #[test]
    fn the_cosine_is_the_sine_a_quarter_turn_ahead() {
        // `SIN[(int)(i * SIN_SCALE + 16384)]`: 16384 de 65536 passos.
        assert_eq!(cos(0.0), sin(std::f64::consts::FRAC_PI_2 - 0.0));
        assert_eq!(cos(0.0), 1.0);
    }

    #[test]
    fn the_fast_atan2_stays_within_its_tolerance_in_every_quadrant() {
        for step in 0..3600 {
            let angle = (f64::from(step) / 10.0 - 180.0).to_radians();
            let (y, x) = (angle.sin() * 37.0, angle.cos() * 37.0);
            let fast = atan2(y, x);
            assert!((fast - y.atan2(x)).abs() < 1.0e-5, "{angle}: {fast}");
        }
    }

    #[test]
    fn the_axes_are_exact_enough() {
        assert!((atan2(0.0, 1.0)).abs() < 1.0e-9);
        assert!((atan2(1.0, 0.0) - std::f64::consts::FRAC_PI_2).abs() < 1.0e-6);
        assert!((atan2(0.0, -1.0) - std::f64::consts::PI).abs() < 1.0e-6);
        assert!((atan2(-1.0, 0.0) + std::f64::consts::FRAC_PI_2).abs() < 1.0e-6);
    }
}
