// Quem esconde a face de quem entre os blocos de vidro do Mekanism e do Generators (`skipRendering`, decisão 30 do plano, H51). O original:
// - `BlockTileGlass.skipRendering`: o vizinho é um `BlockTileGlass` (o vidro estrutural e o vidro do reator, que herda dele);
// - `BlockReactorGlass.skipRendering`: isso, ou o vizinho é um `BlockLaserFocusMatrix`;
// - `BlockLaserFocusMatrix.skipRendering`: o vizinho é um `BlockReactorGlass` ou um `BlockLaserFocusMatrix` (e, senão, o `skipRendering` do pai, que é falso).
// A regra `same_block` do Horizon só pula a face entre estados do mesmo bloco, então o vidro do reator ao lado da matriz de foco desenhava uma linha de 1 a 2 pixels. O HEAD de `FaceVisibility::skips`
// consulta esta tabela, com o estado padrão de cada um dos três blocos (`State::block`), que `FaceVisibility::new` anota no RETURN.
use std::sync::atomic::{AtomicU32, Ordering};

/// Um estado padrão que não existe (o bloco não está no registro).
pub const MISSING: u32 = u32::MAX;

static STRUCTURAL: AtomicU32 = AtomicU32::new(MISSING);
static REACTOR: AtomicU32 = AtomicU32::new(MISSING);
static MATRIX: AtomicU32 = AtomicU32::new(MISSING);

/// Anota o estado padrão de `mekanism:structural_glass`, `mekanismgenerators:reactor_glass` e `mekanismgenerators:laser_focus_matrix` (`MISSING` quando o registro não os tem).
pub fn record(structural: u32, reactor: u32, matrix: u32) {
    STRUCTURAL.store(structural, Ordering::Relaxed);
    REACTOR.store(reactor, Ordering::Relaxed);
    MATRIX.store(matrix, Ordering::Relaxed);
}

/// Se o bloco `own` esconde a face voltada para o bloco `other`: `None` quando `own` não é um dos três e a regra do Horizon segue valendo.
pub fn skips(own: u32, other: u32) -> Option<bool> {
    let structural = STRUCTURAL.load(Ordering::Relaxed);
    let reactor = REACTOR.load(Ordering::Relaxed);
    let matrix = MATRIX.load(Ordering::Relaxed);
    if own == MISSING {
        return None;
    }
    if own == structural {
        return Some(other == structural || other == reactor);
    }
    if own == reactor {
        return Some(other == structural || other == reactor || other == matrix);
    }
    if own == matrix {
        return Some(other == reactor || other == matrix);
    }
    None
}
