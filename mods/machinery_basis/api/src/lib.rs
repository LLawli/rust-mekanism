//! A machinery_basis para mods tipados: o núcleo puro, com a mesma cópia que o lado mixin usa.
use horizon_client::mod_api::{Hooks, MenuSlotRulesEvent};

pub use machinery_basis_core as core;

struct Mod;

impl Hooks for Mod {
    /// `Slot.mayPlace` de um clique do cursor no menu de uma máquina (`InventoryContainerSlot.mayPlace`). O hook roda dentro do menu, sem
    /// o jogo, com o menu, o número do slot e o item: a regra do bloco que o jogador tem aberto, com o estado dele, foi deixada em
    /// `slot_rules` no começo do clique pelo mod da máquina (veja `result_take_before` no Mekanism). A regra só tira: o slot de resultado,
    /// que o tipo de menu já recusa, continua recusando.
    fn menu_slot_rules(&self, event: &mut MenuSlotRulesEvent<'_>) {
        let mut may_place = event.may_place;
        core::slot_rules::ask(event.menu, event.slot, &event.stack.item, event.stack, &mut may_place);
        event.may_place = may_place;
    }
}

/// O que o launcher chama.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}
