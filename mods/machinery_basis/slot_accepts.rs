// Entra em `game/machines/native_items.rs` para enxergar as regras de slot, que são privadas de `machines`. É o `isValid` que o
// `ResourceHandler` do NeoForge consulta: o Horizon não simula inserção, e a transação precisa saber a resposta antes de inserir.
use super::super::View;
use super::Game;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;

impl Game<'_> {
    /// Se `stack` pode entrar em `slot` do contêiner em `pos`, pela face `face`. Só lê: é a parte de `move_in_item` que decide,
    /// sem escrever no slot nem mexer no cooldown do hopper.
    pub(in crate::game) fn machinery_slot_accepts(
        &self,
        pos: [i32; 3],
        slot: usize,
        stack: &ItemStack,
        face: Direction,
    ) -> bool {
        let containers = self.containers();
        let Some(view): Option<View> = containers.view((pos[0], pos[1], pos[2]), false) else {
            return false;
        };
        containers.can_place(&view, slot, stack) && self.mod_container_accepts(&view, slot, stack, Some(face))
    }
}
