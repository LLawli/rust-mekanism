// Entra em `crates/horizon-client-render/src/hud.rs` como `pub mod machinery_sprite_ties` (`visibility = "pub"`). Guarda os retângulos do
// atlas do HUD que as telas de mod desenham, para `animate_sprites` saber quais sprites animados são de mod. O `HudRenderer` não tem os
// nomes: só o `Rect` de cada animação.
use super::Rect;
use std::sync::RwLock;

/// A revisão do atlas a que os retângulos pertencem, e os retângulos (os bits dos quatro `f32`).
static SCREEN_SPRITES: RwLock<(u64, Vec<[u32; 4]>)> = RwLock::new((0, Vec::new()));

fn key(rect: &Rect) -> [u32; 4] {
    [rect.u0.to_bits(), rect.v0.to_bits(), rect.u1.to_bits(), rect.v1.to_bits()]
}

/// Uma tela de mod desenhou `rect`, um retângulo do atlas de `revision`. Outro atlas (recarga de recursos) apaga a lista.
pub fn register(revision: u64, rect: &Rect) {
    let key = key(rect);
    if let Ok(known) = SCREEN_SPRITES.read()
        && known.0 == revision
        && known.1.contains(&key)
    {
        return;
    }
    if let Ok(mut known) = SCREEN_SPRITES.write() {
        if known.0 != revision {
            known.0 = revision;
            known.1.clear();
        }
        if !known.1.contains(&key) {
            known.1.push(key);
        }
    }
}

pub fn contains(rect: &Rect) -> bool {
    SCREEN_SPRITES.read().is_ok_and(|known| known.1.contains(&key(rect)))
}
