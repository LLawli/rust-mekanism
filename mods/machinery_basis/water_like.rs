// Entra em `crates/horizon-client/src/motion.rs` como `pub mod machinery_water_like` (`visibility = "pub"`). Os estados de bloco de líquido de mod, por número de
// estado, para a física de entidade (`FluidResources::fluid`, só água e lava): o nível mais 1 onde o estado é de um bloco de líquido declarado, 0 no resto. O
// `Game` (crate de cima) escreve a tabela a cada carga de dados e o RETURN de `FluidResources::fluid` (crate da biblioteca) a lê.
use std::sync::RwLock;

static TABLE: RwLock<Vec<u8>> = RwLock::new(Vec::new());

/// Troca a tabela: a cada carga de dados.
pub fn set(table: Vec<u8>) {
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = table;
}

/// O nível (0 a 15) do estado `state`, se ele é de um bloco de líquido de mod.
pub fn get(state: u32) -> Option<u8> {
    match TABLE.read().unwrap_or_else(|e| e.into_inner()).get(state as usize) {
        Some(&entry) if entry > 0 => Some(entry - 1),
        _ => None,
    }
}

// A física do `FluidType` de um fluido de mod, para a entidade (decisão 18): as contas puras ficam aqui para o `machinery_basis_core` testar este arquivo com
// `include!` contra o rastro do original (`tests/fluid_entity_module.rs`); as injeções `fluid_entity_*` só leem o resultado.

/// A caixa de interação da entidade é a caixa menos isto (`Entity.getFluidInteractionBox`).
const INTERACTION_DEFLATE: f64 = 0.001;

/// A quantidade do `FluidState` de um bloco de líquido pelo `level` (a mesma conta do RETURN `fluid_state`).
pub fn amount(level: u8) -> u8 {
    match level {
        1..=7 => 8 - level,
        _ => 8,
    }
}

/// `FlowingFluid.getHeight`: o bloco cheio sob o mesmo fluido, senão `amount / 9` em `f32`.
pub fn height(level: u8, same_above: bool) -> f64 {
    if same_above {
        1.0
    } else {
        f64::from(f32::from(amount(level)) / 9.0)
    }
}

/// O que a caixa de uma entidade toca de fluido de mod.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contact {
    /// Alguma célula de fluido de mod alcança a caixa (`EntityFluidInteraction.update`: o topo não fica abaixo do pé).
    pub touching: bool,
    /// Há também água ou lava do vanilla na caixa: aí valem a física e o `wasTouchingWater` dela.
    pub real: bool,
    /// O fluido da célula dos pés (`blockPosition`) é de mod e alcança a caixa: `isInFluidType(level.getFluidState(blockPosition()))`.
    pub feet: bool,
    /// Os olhos estão dentro do fluido de mod.
    pub eye: bool,
}

impl Contact {
    /// Só fluido de mod: a água do motor é uma água emprestada e a entidade não está na água.
    pub fn mod_only(self) -> bool {
        self.touching && !self.real
    }
}

/// O contato de uma caixa (`min`, `max`) com os fluidos de mod. `level_of` dá o `level` do bloco de líquido de mod de uma célula, `real_at` diz se
/// a célula tem água ou lava do vanilla.
pub fn contact(
    min: [f64; 3],
    max: [f64; 3],
    eye_y: f64,
    level_of: &dyn Fn(i32, i32, i32) -> Option<u8>,
    real_at: &dyn Fn(i32, i32, i32) -> bool,
) -> Contact {
    let lo = [
        min[0] + INTERACTION_DEFLATE,
        min[1] + INTERACTION_DEFLATE,
        min[2] + INTERACTION_DEFLATE,
    ];
    let hi = [
        max[0] - INTERACTION_DEFLATE,
        max[1] - INTERACTION_DEFLATE,
        max[2] - INTERACTION_DEFLATE,
    ];
    let reaches = |x: i32, y: i32, z: i32, level: u8| {
        let top = f64::from(y) + height(level, level_of(x, y + 1, z).is_some());
        top >= lo[1]
    };
    let mut out = Contact::default();
    for x in lo[0].floor() as i32..=(hi[0].ceil() as i32 - 1) {
        for y in lo[1].floor() as i32..=(hi[1].ceil() as i32 - 1) {
            for z in lo[2].floor() as i32..=(hi[2].ceil() as i32 - 1) {
                match level_of(x, y, z) {
                    Some(level) => out.touching |= reaches(x, y, z, level),
                    None => out.real |= real_at(x, y, z),
                }
            }
        }
    }
    let (fx, fy, fz) = (
        ((min[0] + max[0]) / 2.0).floor() as i32,
        min[1].floor() as i32,
        ((min[2] + max[2]) / 2.0).floor() as i32,
    );
    if let Some(level) = level_of(fx, fy, fz) {
        out.feet = reaches(fx, fy, fz, level);
    }
    let ey = eye_y.floor() as i32;
    if let Some(level) = level_of(fx, ey, fz) {
        out.eye = eye_y <= f64::from(ey) + height(level, level_of(fx, ey + 1, fz).is_some());
    }
    out
}

use std::cell::Cell;

thread_local! {
    /// O contato do jogador que está em `update_fluids`: o respingo, a queda e o `wasTouchingWater` dessa mesma chamada o leem.
    static PLAYER_UPDATE: Cell<bool> = const { Cell::new(false) };
}

/// `update_fluids` do jogador começou: só fluido de mod na caixa?
pub fn set_player_update(mod_only: bool) {
    PLAYER_UPDATE.with(|c| c.set(mod_only));
}

/// Dentro do `update_fluids` do jogador, a água que ele toca é só de fluido de mod.
pub fn player_update_mod_only() -> bool {
    PLAYER_UPDATE.with(Cell::get)
}

thread_local! {
    /// O mob cujos olhos estão num fluido de mod, na chamada de `update_fluids` que o `base_tick` dele acabou de fazer: o `LivingBreatheEvent` do mesmo
    /// `base_tick` (`breathing::breathe`) o lê. O `base_tick` do mob corre inteiro antes do seguinte, então o valor é sempre o do mob certo.
    static MOB_EYE: Cell<Option<u32>> = const { Cell::new(None) };
}

/// `update_fluids` do mob `id` terminou: `eye` diz se os olhos estão num fluido de mod.
pub fn set_mob_eye(id: u32, eye: bool) {
    MOB_EYE.with(|c| c.set(eye.then_some(id)));
}

/// Os olhos do mob `id` estão num fluido de mod (pelo último `update_fluids`).
pub fn mob_eye(id: u32) -> bool {
    MOB_EYE.with(|c| c.get() == Some(id))
}

static LOGGED: RwLock<Vec<u8>> = RwLock::new(Vec::new());

/// Troca a tabela dos estados com `fluid_logged`: 1 para a água e 2 para a lava, 0 no resto. Refeita a cada carga de dados.
pub fn set_logged(table: Vec<u8>) {
    *LOGGED.write().unwrap_or_else(|e| e.into_inner()) = table;
}

/// O fluido que o estado `state` guarda dentro do bloco, se guarda: 1 a água e 2 a lava.
pub fn logged(state: u32) -> Option<u8> {
    match LOGGED.read().unwrap_or_else(|e| e.into_inner()).get(state as usize) {
        Some(&kind) if kind > 0 => Some(kind),
        _ => None,
    }
}
