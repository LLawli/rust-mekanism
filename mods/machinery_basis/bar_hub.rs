// Entra em `crates/horizon-client/src/lan/containers.rs` como `machinery_bar_hub`. O menu de um contêiner de mod com barra de armadura tem
// cinco slots a mais que o hub não conhece (ele sincroniza `size + 36`): o clique que os muda corrige o cliente na hora, no lugar de um tick
// depois pela sincronização do menu do inventário, que deixaria um quadro com o item em lugar nenhum (cursor vazio, slot ainda vazio).
use super::Hub;
use horizon_protocol::packets::play::{self, Clientbound as C};

/// Os slots do menu do inventário (janela 0) que são a armadura (5 a 8, da cabeça aos pés) e a mão (45): o cliente os conhece como 500 a 504.
const EQUIPMENT_SLOTS: [usize; 5] = [5, 6, 7, 8, 45];

impl Hub {
    /// `AbstractContainerMenu.broadcastChanges` do menu do inventário, só para a armadura e a mão, e só quando o menu aberto de quem `i` é
    /// tem a barra. O que o cliente já sabe não se manda de novo.
    pub(crate) fn machinery_sync_bar(&mut self, i: usize) {
        let has_bar = self.guests[i]
            .menu
            .as_ref()
            .and_then(|menu| menu.mod_type)
            .is_some_and(|menu_type| crate::menu::machinery_bar_menu::declared(menu_type.id));
        if !has_bar {
            return;
        }
        for slot in EQUIPMENT_SLOTS {
            let now = self.guests[i].menu_slot(slot);
            if now == self.guests[i].menu_sent[slot] {
                continue;
            }
            let item = self.stack(&now);
            let guest = &mut self.guests[i];
            guest.menu_state = (guest.menu_state + 1) & 0x7FFF;
            guest.send(&C::ContainerSetSlot(play::ContainerSetSlot {
                container: 0,
                state: guest.menu_state,
                slot: slot as i16,
                item,
            }));
            guest.menu_sent[slot] = now;
        }
    }

    /// O mesmo, para o jogador da conexão `conn` (`resend_menu` não conhece o índice).
    pub(crate) fn machinery_sync_bar_of(&mut self, conn: u32) {
        if let Some(i) = self.guests.iter().position(|guest| guest.conn == conn) {
            self.machinery_sync_bar(i);
        }
    }
}
