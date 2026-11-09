// Entra em `crates/horizon-client/src/mobs/anim.rs` como `pub mod machinery_basis_worn` (`visibility = "pub"`).
// As partes do corpo que o equipamento vestido esconde (`overrideModelPartVisibility` do `registerRenderStateModifiers` do original: o MekaSuit esconde a cabeça, o corpo, os braços e
// as pernas do modelo, e põe as peças dele no lugar). A pose das partes é montada em `anim::setup`, que o jogador da janela, os outros jogadores e os mobs passam: a injeção
// `worn_hide` esconde no começo dele o que o mob veste, e, como o jogador mostra de novo o corpo, os braços e as pernas ao montar a visibilidade do modelo
// (`player_model_visibility`), a injeção `worn_hide_player` esconde de novo no fim dela. O módulo é público porque quem diz o que cada item esconde é o crate do jogo
// (`horizon-client`), que depende deste (`horizon-client-lib`).
use std::cell::RefCell;
use std::sync::RwLock;

/// O slot de armadura (cabeça 0, peito 1, pernas 2, pés 3), o item e as partes do modelo que somem enquanto ele é vestido.
static HIDES: RwLock<Vec<(usize, &'static str, &'static [&'static str])>> = RwLock::new(Vec::new());

thread_local! {
    /// As partes que o mob da pose em montagem esconde: o `anim::setup` as acha e o fim de `player_model_visibility` as esconde de novo.
    static PENDING: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// Diz que o item `item`, vestido no slot `slot`, esconde `parts`. Dizer de novo o mesmo item não faz nada.
pub fn hide_when_worn(slot: usize, item: &'static str, parts: &'static [&'static str]) {
    let mut hides = HIDES.write().unwrap_or_else(|error| error.into_inner());
    if !hides.iter().any(|(known, id, _)| *known == slot && *id == item) {
        hides.push((slot, item, parts));
    }
}

/// Esconde as partes que o que `m` veste esconde (`ModelPart.visible = false`: a parte e as filhas, o chapéu, a jaqueta, as mangas e as calças, não se desenham), e as guarda.
pub fn begin(m: &crate::mobs::Mob, p: &mut super::Poses) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        pending.clear();
        let worn = [m.extra.head.as_deref(), m.extra.chest.as_deref(), m.extra.legs.as_deref(), m.extra.feet.as_deref()];
        if worn.iter().all(Option::is_none) {
            return;
        }
        let hides = HIDES.read().unwrap_or_else(|error| error.into_inner());
        for (slot, item, parts) in hides.iter() {
            if worn[*slot] == Some(*item) {
                for part in *parts {
                    if !pending.contains(part) {
                        pending.push(part);
                    }
                    p.visible(part, false);
                }
            }
        }
    });
}

/// Esconde de novo o que `begin` achou: o jogador mostra o corpo, os braços e as pernas depois de `begin`.
pub fn again(p: &mut super::Poses) {
    PENDING.with(|pending| {
        for part in pending.borrow().iter() {
            p.visible(part, false);
        }
    });
}
