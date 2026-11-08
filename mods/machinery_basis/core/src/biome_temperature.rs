//! A temperatura de um bioma num bloco, como o vanilla a calcula (`Biome.getTemperature(pos, seaLevel)`): o modificador de
//! temperatura do bioma (`none` ou `frozen`) e, acima de `seaLevel + 17`, o ajuste por altura com o ruído `TEMPERATURE_NOISE`.
//! O `HeatAPI.getAmbientTemp(level, pos)` do Mekanism a leva para kelvin (`heat::ambient`). Tudo é determinístico: o ruído tem
//! semente fixa, então o resultado só depende do bioma, da posição e do nível do mar.
//!
//! O que fica de fora de propósito: o cache por posição do `Biome` (`temperatureCache`, 1024 entradas por bioma e por thread, com a
//! chave sem o nível do mar). Ele é compartilhado com todo chamador do vanilla (chuva, neve, gelo), que o Horizon tem por conta
//! própria; só muda o resultado para a mesma posição consultada com dois níveis do mar (o mesmo bioma em duas dimensões, nas mesmas
//! coordenadas). Está no plano, em "Comportamento do original que parece bug e fica".
use std::sync::OnceLock;

const MODULUS_MASK: i64 = (1 << 48) - 1;
const MULTIPLIER: i64 = 25_214_903_917;
const INCREMENT: i64 = 11;

/// `LegacyRandomSource`: o gerador linear congruente de 48 bits do `java.util.Random`, com os sorteios do `BitRandomSource`. O
/// `WorldgenRandom(new LegacyRandomSource(seed))` do vanilla sorteia pela mesma sequência.
#[derive(Clone, Debug)]
pub struct LegacyRandom {
    seed: i64,
}

impl LegacyRandom {
    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed ^ MULTIPLIER) & MODULUS_MASK,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT)) & MODULUS_MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `BitRandomSource.nextInt(bound)`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Bound must be positive");
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let sample = self.next(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }

    /// `BitRandomSource.nextDouble()`. A constante é um `float` e o produto de um `long` por um `float` é `float`: o resultado tem a
    /// precisão de um `float`, ampliado para `double`.
    pub fn next_double(&mut self) -> f64 {
        let upper = i64::from(self.next(26));
        let lower = i64::from(self.next(27));
        let combined = (upper << 27) + lower;
        f64::from(combined as f32 * DOUBLE_MULTIPLIER)
    }
}

/// `BitRandomSource.DOUBLE_MULTIPLIER`, um `float`.
const DOUBLE_MULTIPLIER: f32 = 1.110223E-16;

/// `GradientNoise.GRADIENT`.
const GRADIENT: [[i32; 3]; 16] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
    [1, 1, 0],
    [0, -1, 1],
    [-1, 1, 0],
    [0, -1, -1],
];

/// `SimplexNoise`, só a amostra em duas dimensões (`get(x, y)`), que é a que o clima usa.
#[derive(Clone, Debug)]
pub struct SimplexNoise {
    perms: [u8; 256],
    offset_x: f64,
    offset_y: f64,
}

impl SimplexNoise {
    /// `new SimplexNoise(random, discardNoiseOffset)`: os três deslocamentos saem do sorteio mesmo quando descartados (a escala vira 0).
    pub fn new(random: &mut LegacyRandom, discard_noise_offset: bool) -> Self {
        let scale = if discard_noise_offset { 0.0 } else { 256.0 };
        let offset_x = random.next_double() * scale;
        let offset_y = random.next_double() * scale;
        let _offset_z = random.next_double() * scale;
        let mut perms = [0_u8; 256];
        for (i, perm) in perms.iter_mut().enumerate() {
            *perm = i as u8;
        }
        for i in 0..256 {
            let offset = random.next_int(256 - i as i32) as usize;
            perms.swap(i, offset + i);
        }
        Self {
            perms,
            offset_x,
            offset_y,
        }
    }

    fn permute(&self, x: i32) -> i32 {
        i32::from(self.perms[(x & 0xFF) as usize])
    }

    /// `SimplexNoise.getCornerNoise3D` com `z` zero e a base 0,5 da amostra em duas dimensões.
    fn corner(index: i32, x: f64, y: f64) -> f64 {
        let mut t0 = 0.5 - x * x - y * y - 0.0 * 0.0;
        if t0 < 0.0 {
            return 0.0;
        }
        t0 *= t0;
        let gradient = GRADIENT[index as usize];
        t0 * t0
            * (f64::from(gradient[0]) * x
                + f64::from(gradient[1]) * y
                + f64::from(gradient[2]) * 0.0)
    }

    /// `SimplexNoise.get(x, y)`.
    pub fn get(&self, x: f64, y: f64) -> f32 {
        let sqrt_3 = 3.0_f64.sqrt();
        let f2 = 0.5 * (sqrt_3 - 1.0);
        let g2 = (3.0 - sqrt_3) / 6.0;
        let xin = x + self.offset_x;
        let yin = y + self.offset_y;
        let s = (xin + yin) * f2;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let t = f64::from(i.wrapping_add(j)) * g2;
        let x0 = xin - (f64::from(i) - t);
        let y0 = yin - (f64::from(j) - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - f64::from(i1) + g2;
        let y1 = y0 - f64::from(j1) + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let ii = i & 0xFF;
        let jj = j & 0xFF;
        let gi0 = self.permute(ii + self.permute(jj)) % 12;
        let gi1 = self.permute(ii + i1 + self.permute(jj + j1)) % 12;
        let gi2 = self.permute(ii + 1 + self.permute(jj + 1)) % 12;
        let n0 = Self::corner(gi0, x0, y0);
        let n1 = Self::corner(gi1, x1, y1);
        let n2 = Self::corner(gi2, x2, y2);
        (70.0 * (n0 + n1 + n2)) as f32
    }
}

/// O ruído de `Biome.FROZEN_TEMPERATURE_NOISE`: três camadas (`NoiseStack`), cada uma um `SimplexNoise` sorteado em seguida do mesmo
/// gerador, com a frequência e a amplitude dela.
struct FrozenNoise {
    layers: [(SimplexNoise, f64, f32); 3],
}

impl FrozenNoise {
    fn get(&self, x: f64, y: f64) -> f32 {
        let mut value = 0.0_f32;
        for (noise, frequency, amplitude) in &self.layers {
            value += *amplitude * noise.get(x * frequency, y * frequency);
        }
        value
    }
}

/// Os ruídos fixos de `Biome`, com as sementes do vanilla.
pub struct Seeds {
    /// `Biome.TEMPERATURE_NOISE`.
    pub height: i64,
    /// `Biome.FROZEN_TEMPERATURE_NOISE`.
    pub frozen: i64,
    /// `Biome.BIOME_INFO_NOISE`.
    pub info: i64,
}

/// As sementes do vanilla (`new LegacyRandomSource(1234L)`, `3456L` e `2345L`).
pub const VANILLA_SEEDS: Seeds = Seeds {
    height: 1234,
    frozen: 3456,
    info: 2345,
};

/// Os três ruídos do clima de um bioma.
pub struct ClimateNoises {
    height: SimplexNoise,
    frozen: FrozenNoise,
    info: SimplexNoise,
}

impl ClimateNoises {
    pub fn new(seeds: &Seeds) -> Self {
        let mut frozen_random = LegacyRandom::new(seeds.frozen);
        let layers = [
            (1.0, 0.142_857_15_f32),
            (0.5, 0.285_714_3_f32),
            (0.25, 0.571_428_6_f32),
        ]
        .map(|(frequency, amplitude)| {
            (
                SimplexNoise::new(&mut frozen_random, true),
                frequency,
                amplitude,
            )
        });
        Self {
            height: SimplexNoise::new(&mut LegacyRandom::new(seeds.height), true),
            frozen: FrozenNoise { layers },
            info: SimplexNoise::new(&mut LegacyRandom::new(seeds.info), true),
        }
    }

    /// Os ruídos do vanilla, montados uma vez.
    pub fn vanilla() -> &'static Self {
        static NOISES: OnceLock<ClimateNoises> = OnceLock::new();
        NOISES.get_or_init(|| Self::new(&VANILLA_SEEDS))
    }
}

/// A altura a partir da qual o ar esfria, em blocos acima do nível do mar (`seaLevel + 17` em `getHeightAdjustedTemperature`).
pub const SNOW_LEVEL_ABOVE_SEA: i32 = 17;

/// O clima de um bioma que a temperatura lê: `climateSettings.temperature` e o `temperatureModifier`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Climate {
    pub temperature: f32,
    /// `Biome.TemperatureModifier.FROZEN` (os oceanos congelados); `NONE` senão.
    pub frozen: bool,
}

impl Climate {
    /// `TemperatureModifier.modifyTemperature(pos, baseTemperature)`.
    fn modified(&self, noises: &ClimateNoises, pos: [i32; 3]) -> f32 {
        if !self.frozen {
            return self.temperature;
        }
        let x = f64::from(pos[0]);
        let z = f64::from(pos[2]);
        let large_variation = f64::from(noises.frozen.get(x * 0.05, z * 0.05) * 7.0_f32);
        let edge_variation = f64::from(noises.info.get(x * 0.2, z * 0.2));
        let ice_patches = large_variation + edge_variation;
        if ice_patches < 0.3 {
            let small_variation = f64::from(noises.info.get(x * 0.09, z * 0.09));
            if small_variation < 0.8 {
                return 0.2;
            }
        }
        self.temperature
    }

    /// `Biome.getHeightAdjustedTemperature(pos, seaLevel)`, que `getTemperature(pos, seaLevel)` devolve (menos o cache): acima de
    /// `seaLevel + 17` a temperatura cai com a altura e com o ruído de `TEMPERATURE_NOISE` na posição.
    pub fn height_adjusted(&self, noises: &ClimateNoises, pos: [i32; 3], sea_level: i32) -> f32 {
        let adjusted = self.modified(noises, pos);
        let snow_level = sea_level.wrapping_add(SNOW_LEVEL_ABOVE_SEA);
        if pos[1] > snow_level {
            let variation = noises.height.get(
                f64::from(pos[0] as f32 / 8.0_f32),
                f64::from(pos[2] as f32 / 8.0_f32),
            ) * 8.0_f32;
            adjusted - ((variation + pos[1] as f32) - snow_level as f32) * 0.05_f32 / 40.0_f32
        } else {
            adjusted
        }
    }

    /// `Biome.getTemperature(pos, seaLevel)` com os ruídos do vanilla.
    pub fn temperature_at(&self, pos: [i32; 3], sea_level: i32) -> f32 {
        self.height_adjusted(ClimateNoises::vanilla(), pos, sea_level)
    }

    /// `HeatAPI.getAmbientTemp(level, pos)` para um bioma: a temperatura do ar, em kelvin, neste bloco.
    pub fn ambient_at(&self, pos: [i32; 3], sea_level: i32) -> f64 {
        crate::heat::ambient(f64::from(self.temperature_at(pos, sea_level)))
    }
}

/// O nível do mar de uma dimensão (`ChunkGenerator.getSeaLevel`): `noise_settings` do ambiente, do nether e do end; o gerador de
/// mundo plano (`FlatLevelSource`) devolve -63 qualquer que seja a camada.
pub fn sea_level(dimension: &str, superflat: bool) -> i32 {
    match dimension {
        "minecraft:the_nether" => 32,
        "minecraft:the_end" => 0,
        _ if superflat => -63,
        _ => 63,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Os valores de `java.util.Random(42)` (`nextInt()`), de que o sorteio do vanilla depende.
    #[test]
    fn the_random_is_java_util_random() {
        let mut random = LegacyRandom::new(42);
        assert_eq!(random.next(32), -1_170_105_035);
        assert_eq!(random.next(32), 234_785_527);
    }

    #[test]
    fn sea_levels_follow_the_dimension() {
        assert_eq!(sea_level("minecraft:overworld", false), 63);
        assert_eq!(sea_level("minecraft:overworld", true), -63);
        assert_eq!(sea_level("minecraft:the_nether", true), 32);
        assert_eq!(sea_level("minecraft:the_end", false), 0);
    }

    #[test]
    fn at_or_below_the_snow_level_nothing_changes() {
        let plains = Climate {
            temperature: 0.8,
            frozen: false,
        };
        assert_eq!(plains.temperature_at([10, 80, 10], 63), 0.8);
        assert!(plains.temperature_at([10, 81, 10], 63) < 0.8);
    }
}
