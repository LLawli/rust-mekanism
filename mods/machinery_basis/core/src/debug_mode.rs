//! `MekanismAPI.debug`: o campo estático que só o `/mek debug` muda, lido pelo registro de redes (os logs), pelo Network Reader e pelos outros blocos que escrevem
//! depuração. É do processo do servidor e não de um mundo: vale até o jogo fechar, como o campo da JVM.
use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG: AtomicBool = AtomicBool::new(false);

/// `MekanismAPI.debug`.
pub fn enabled() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

/// `MekanismAPI.debug = !MekanismAPI.debug`: o valor novo.
pub fn toggle() -> bool {
    !DEBUG.fetch_xor(true, Ordering::Relaxed)
}

/// `MekanismAPI.debug = on`.
pub fn set(on: bool) {
    DEBUG.store(on, Ordering::Relaxed);
}
