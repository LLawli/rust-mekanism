// Entra em `crates/horizon-client-render/src/hud.rs` como `pub mod machinery_container_dim` (`visibility = "pub"`). Guarda se a tela de contêiner aberta é de
// uma máquina nossa (a `machinery_basis` a abriu): o `Game` o escreve (o `frame` e o começo de `draw_inventory`) e o HEAD de `HudBatch::fill` o lê, porque o
// `HudBatch` não conhece o `Game`.
use std::sync::atomic::{AtomicBool, Ordering};

static OWN_DIM: AtomicBool = AtomicBool::new(false);

/// A tela aberta é (ou deixou de ser) de uma máquina nossa.
pub fn set(own: bool) {
    OWN_DIM.store(own, Ordering::Relaxed);
}

pub fn is_set() -> bool {
    OWN_DIM.load(Ordering::Relaxed)
}
