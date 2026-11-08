//! As partículas de um bloco aceso: `AttributeParticleFX` e `BlockTile.animateTick` (`BlockTile.java:80-103`). O cliente sorteia posições em volta do
//! jogador a cada tick (`ClientLevel.animateTick`) e chama o `animateTick` do bloco sorteado; o de `BlockTile`, com o bloco ativo e `machineEffects`
//! ligado, pede ao `AttributeParticleFX` uma posição para cada partícula, gira a posição pelo `facing` e a soma ao centro do bloco.
//!
//! Sem tipos do jogo: o mod dá a lista de partículas (o `AttributeParticleFX` do bloco), o jogo dá o centro, o `facing` e o gerador de números, e daqui
//! sai a partícula e a posição, uma por vez, na ordem em que o Java as sorteia (o jogo a cria antes de pedir a próxima, como o `addParticle`).

/// O que o `Level.addParticle` recebe: os `ParticleOptions` que um `AttributeParticleFX` usa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Particle {
    /// `ParticleTypes.SMOKE`.
    Smoke,
    /// `ParticleTypes.FLAME`.
    Flame,
    /// `DustParticleOptions`: cor `0xRRGGBB` e escala. `DustParticleOptions.REDSTONE` é `0xFF0000` e 1.
    Dust { rgb: u32, scale: f32 },
}

/// `DustParticleOptions.REDSTONE` (`REDSTONE_PARTICLE_COLOR = 16711680`, escala 1).
pub const REDSTONE: Particle = Particle::Dust {
    rgb: 0xFF_00_00,
    scale: 1.0,
};

/// `Function<RandomSource, Vec3>`: a posição de uma partícula relativa ao centro do bloco, virado para o sul (o `facing` zero do giro). O gerador
/// dá o próximo `nextDouble()`.
pub type PositionFn = fn(&mut dyn FnMut() -> f64) -> [f64; 3];

/// Uma entrada do `AttributeParticleFX`: `add(type, posSupplier)`.
#[derive(Clone, Copy, Debug)]
pub struct FxParticle {
    pub particle: Particle,
    pub position: PositionFn,
}

/// `Direction` da propriedade `facing`: só os horizontais giram a posição (`BlockTile.animateTick` compara com `WEST`, `EAST` e `NORTH`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    North,
    South,
    West,
    East,
    Up,
    Down,
}

impl Facing {
    /// O valor da propriedade `facing` do estado. Um valor que não é uma direção vale como o sul, que não gira.
    pub fn from_property(value: &str) -> Self {
        match value {
            "north" => Self::North,
            "west" => Self::West,
            "east" => Self::East,
            "up" => Self::Up,
            "down" => Self::Down,
            _ => Self::South,
        }
    }
}

/// Uma partícula pronta: o que o jogo cria e onde.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    pub particle: Particle,
    pub pos: [f64; 3],
}

/// `Mth.PI`, `Mth.HALF_PI`: `float`.
const PI: f32 = std::f32::consts::PI;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

/// `Mth.SIN[index]`: `(float) Math.sin(index / 10430.378350470453)`.
fn sin_table(index: i64) -> f32 {
    ((index & 65535) as f64 / 10_430.378_350_470_453).sin() as f32
}

/// `Mth.sin(double)`: o índice trunca para `long` antes da máscara.
fn mth_sin(radians: f32) -> f32 {
    sin_table((f64::from(radians) * 10_430.378_350_470_453) as i64)
}

/// `Mth.cos(double)`.
fn mth_cos(radians: f32) -> f32 {
    sin_table((f64::from(radians) * 10_430.378_350_470_453 + 16_384.0) as i64)
}

/// `Vec3.yRot(radians)`, com o seno e o cosseno da tabela do jogo.
fn y_rot(pos: [f64; 3], radians: f32) -> [f64; 3] {
    let cos = f64::from(mth_cos(radians));
    let sin = f64::from(mth_sin(radians));
    [
        pos[0] * cos + pos[2] * sin,
        pos[1],
        pos[2] * cos - pos[0] * sin,
    ]
}

/// O que `BlockTile.animateTick` faz com a posição de uma partícula e o centro do bloco (`Vec3.atCenterOf(pos)`).
pub fn place(
    entry: &FxParticle,
    facing: Facing,
    center: [f64; 3],
    next_double: &mut dyn FnMut() -> f64,
) -> Spawn {
    let pos = (entry.position)(next_double);
    let pos = match facing {
        Facing::West => y_rot(pos, HALF_PI),
        Facing::East => y_rot(pos, -HALF_PI),
        Facing::North => y_rot(pos, PI),
        Facing::South | Facing::Up | Facing::Down => pos,
    };
    Spawn {
        particle: entry.particle,
        pos: [pos[0] + center[0], pos[1] + center[1], pos[2] + center[2]],
    }
}

/// `BitRandomSource.nextDouble` (`RandomSource.create`, `createThreadLocalInstance`): `((long) next(26) << 27) + next(27)` vezes `1.110223E-16F`. A conta é
/// `long * float`, em `float`: o inteiro de 53 bits perde bits para 24 de mantissa, e o `double` que sai é um `float`. O `java.util.Random` e o
/// `next_double` do Horizon fazem a conta em `double` (2^-53), então a posição sorteada difere até 1,8e-8.
pub fn bit_random_double(high: i32, low: i32) -> f64 {
    let combined = (i64::from(high) << 27) + i64::from(low);
    f64::from(combined as f32 * 1.110_223e-16_f32)
}

/// O `Level` que `BlockTile.animateTick` usa: o sorteio (`RandomSource.nextDouble`) e o `addParticle`. O jogo implementa os dois sobre o mesmo
/// objeto, porque o Java sorteia a posição de uma partícula, a cria, e só depois sorteia a da próxima.
pub trait ParticleLevel {
    fn next_double(&mut self) -> f64;
    fn add_particle(&mut self, spawn: Spawn);
}

/// `BlockTile.animateTick` (`BlockTile.java:80-103`) de um bloco com `AttributeParticleFX`: com `machineEffects` ligado e o bloco ativo, uma
/// partícula por entrada, na ordem da lista.
pub fn animate_tick(
    fx: &[FxParticle],
    facing: Facing,
    center: [f64; 3],
    active: bool,
    machine_effects: bool,
    level: &mut dyn ParticleLevel,
) {
    if !machine_effects || !active {
        return;
    }
    for entry in fx {
        let spawn = place(entry, facing, center, &mut || level.next_double());
        level.add_particle(spawn);
    }
}

/// `Vec3.atCenterOf(BlockPos)`.
pub fn center_of(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}

/// `new Vec3(rand.nextDouble() * 0.6 - 0.3, rand.nextDouble() * 6.0 / 16.0, z)`: a face da frente de `Machine.java:23` (z = 0,52) e a de trás do
/// Heat Generator (`GeneratorsBlockTypes.java:69`, z = -0,52). A conta é a do Java, na mesma ordem (`x` sorteia primeiro).
pub fn face_position(z: f64, next_double: &mut dyn FnMut() -> f64) -> [f64; 3] {
    let x = next_double() * 0.6 - 0.3;
    let y = next_double() * 6.0 / 16.0;
    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_gives_the_quarter_turns_of_the_game() {
        // `Mth.cos(HALF_PI)` indexa a tabela em 32768 (pi), onde o seno vale `float(sin(pi))`, e não zero.
        assert_eq!(mth_sin(HALF_PI), 1.0);
        assert_eq!(mth_cos(HALF_PI), 1.224_646_9e-16);
        assert_eq!(mth_sin(-HALF_PI), -1.0);
        assert_eq!(mth_cos(PI), -1.0);
    }

    #[test]
    fn the_unrotated_position_is_just_added_to_the_center() {
        let mut values = [0.5, 0.5].into_iter();
        let entry = FxParticle {
            particle: Particle::Smoke,
            position: |next| face_position(0.52, next),
        };
        let spawn = place(&entry, Facing::South, [10.5, 20.5, 30.5], &mut || {
            values.next().unwrap()
        });
        assert_eq!(spawn.pos, [10.5, 20.5 + 0.5 * 6.0 / 16.0, 31.02]);
    }
}
