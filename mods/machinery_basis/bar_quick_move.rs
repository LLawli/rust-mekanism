// Entra em `crates/horizon-client/src/game/native_quick_move.rs` como `machinery_bar_quick_move`, para chamar o que o shift-clique do
// jogo usa e é privado dali. O `quick_move_slot` do Horizon só reconhece o clique num slot do bloco ou do inventário, e dá `None` para o
// que passa de `size + 36`: o shift-clique que parte de um slot da barra de armadura (ou da mão) cairia no motor de menus, que não conhece
// a regra do Mekanism. Aqui ele vai ao mod do mesmo jeito que o do inventário, com as guardas de `lan_mod_quick_move`.
use super::{BlockEntity, Game, LanHost, QUICK_MOVE, Slots};
use horizon_protocol::packets::play;
use machinery_basis_core::armor_bar;
use machinery_basis_core::armor_slots;

impl Game<'_> {
    /// `AbstractContainerMenu.doClick` de um QUICK_MOVE que parte de um slot da barra, ao mod dono do menu (injeção `bar_quick_move`, no
    /// começo de `lan_mod_quick_move`). Devolve se tomou o clique; falso deixa o resto do jogo responder, como antes.
    pub(in crate::game) fn machinery_bar_quick_move(&mut self, host: &mut LanHost, conn: u32, p: &play::ContainerClick) -> bool {
        if p.input != QUICK_MOVE || !(0..=1).contains(&p.button) {
            return false;
        }
        let Some(g) = host.hub.guest(conn) else {
            return false;
        };
        let Some(menu) = g.menu.as_ref().filter(|m| p.container != 0 && m.window == p.container) else {
            return false;
        };
        let (Slots::Block(pos), Some(menu_type), Ok(slot)) = (menu.slots, menu.mod_type, usize::try_from(p.slot)) else {
            return false;
        };
        let Some(client) = armor_bar::client_of_menu_index(menu_type.id, menu.size, slot) else {
            return false;
        };
        // As mesmas guardas de `lan_mod_quick_move`: quem não pode agir, a bola de arrasto e o menu que já não vale ficam com o caminho do jogo.
        if g.spectator() || g.dead || menu.drag.is_some() || !menu.slots.still_valid(&self.world, g, menu.pos) {
            return false;
        }
        let (size, window) = (menu.size, menu.window);
        if host
            .play
            .get(&conn)
            .and_then(|play| play.crafting.machine.as_ref())
            .is_some_and(|(w, m)| *w == window && m.quick_crafting())
        {
            return false;
        }
        if !matches!(self.world.block_entities.get(&pos), Some(BlockEntity::Mod(m)) if !m.id.is_empty()) {
            return false;
        }
        let k = client - armor_bar::CLIENT_BASE;
        let Some(placed) = armor_bar::slots(menu_type.id).get(k).copied() else {
            return false;
        };
        let held = if k < 4 { g.armor[k].clone() } else { g.offhand.clone() };
        // `Slot.mayPickup`: a peça com maldição do vínculo não sai por shift-clique, fora do criativo. `doClick` volta antes de `quickMoveStack`.
        let binding = held.as_ref().is_some_and(|stack| stack.enchantment_level("minecraft:binding_curse") > 0);
        if !armor_slots::may_pickup(placed.kind, held.is_none(), g.creative(), binding) {
            host.hub.resend_menu(conn, window, &self.world);
            return true;
        }
        let handled = self
            .as_guest(host, conn, |game| game.machinery_bar_origin(pos, size, slot, k))
            .unwrap_or(false);
        if !handled {
            return false;
        }
        // O motor de menus do tipo recomeça do bloco no clique seguinte.
        if let Some(play) = host.play.get_mut(&conn)
            && play.crafting.machine.as_ref().is_some_and(|(w, _)| *w == window)
        {
            play.crafting.machine = None;
        }
        host.hub.resend_menu(conn, window, &self.world);
        true
    }

    /// `mod_menu_quick_move` para a pilha do slot `k` da barra (da cabeça aos pés, e a mão): o que o mod deixa fica no slot de origem.
    /// Nenhum efeito de equipar: o `quickMoveStack` do Java move por `insert` e `remove`, sem `setByPlayer`, então não há som nem evento.
    fn machinery_bar_origin(&mut self, pos: horizon_client::block_entities::Pos, size: usize, slot: usize, k: usize) -> bool {
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&pos) else {
            return false;
        };
        let held = if k < self.armor.len() { self.armor[k].clone() } else { self.offhand.clone() };
        let Some(mut stack) = held.filter(|s| s.count > 0) else {
            return false;
        };
        let mut taken = std::mem::take(&mut **m);
        let items = taken.items.clone();
        let mut handled = false;
        self.native_menu_quick_move([pos.0, pos.1, pos.2], &mut taken, size, slot, &mut stack, &mut handled);
        if handled {
            let left = (stack.count > 0).then_some(stack);
            if k < self.armor.len() {
                self.armor[k] = left;
            } else {
                self.offhand = left;
            }
        } else {
            items.clone_into(&mut taken.items);
        }
        self.put_back_mod_block_entity(pos, taken, &items);
        handled
    }
}
