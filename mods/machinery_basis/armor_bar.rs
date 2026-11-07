// A barra de armadura de uma tela de contêiner de mod (`ContainerBuilder.armorSideBar`), do lado do cliente: onde os cinco slots ficam na
// tela, a lista de slots que o jogo desenha e aceita clique, e o que "clicar fora" quer dizer quando a tela tem slots ou elementos fora da
// imagem (`GuiMekanism.hasClickedOutside`; os elementos, as abas, são de `screen_elements` na lib). Este arquivo é o módulo `machinery_armor_bar`, enxertado em `game.rs`. Os ids de cliente 500 a 504 são
// os que o jogo já usa para a armadura e a mão (`Game::armor`, `Game::offhand`); o servidor monta o menu com os mesmos cinco slots depois
// dos 36 do inventário (módulo `machinery_bar_menu`), e `menu_slot` liga os dois lados.
use crate::game::Game;
use horizon_client::hud::HudBatch;
use machinery_basis_core::armor_bar;
use machinery_basis_core::armor_slots::{self, SlotKind};
use machinery_basis_core::screen_elements;
use std::ops::ControlFlow;

/// O mod declara, uma vez por tipo de menu, onde a barra fica. Pode repetir: só a primeira vale.
/// O registro da lib (`armor_bar`) serve ao cliente; o do menu (`machinery_bar_menu`, no crate de baixo, que não enxerga a lib) serve ao servidor
/// que monta o menu, então os dois recebem o mesmo, o do menu primeiro.
pub(crate) fn declare(menu: &str, declaration: impl FnOnce() -> armor_bar::Declaration) {
    if armor_bar::declared(menu) {
        return;
    }
    let declaration = declaration();
    let slots = armor_slots::armor_side_bar(declaration.x, declaration.y, declaration.offhand_offset)
        .iter()
        .map(|slot| {
            let equipment = match slot.kind {
                SlotKind::Armor(equipment) => Some(equipment.index()),
                SlotKind::Offhand => None,
            };
            (equipment, slot.inventory_index)
        })
        .collect();
    horizon_client::menu::machinery_bar_menu::declare(menu, slots);
    armor_bar::declare(menu, declaration);
}

impl Game<'_> {
    /// O tipo de menu da tela de mod aberta, se ele declarou uma barra.
    fn mb_bar_menu(&self) -> Option<&str> {
        if !self.native_screen_shown() {
            return None;
        }
        self.remote_menu_type().filter(|menu| armor_bar::declared(menu))
    }

    /// `Game::slots` com os slots da barra no fim (injeção `bar_slots`).
    pub(in crate::game) fn mb_bar_slots(&self, mut slots: Vec<usize>) -> Vec<usize> {
        if let Some(menu) = self.mb_bar_menu() {
            slots.extend((0..armor_bar::extra_count(menu)).map(armor_bar::client_id));
        }
        slots
    }

    /// `Game::slot_count` com os mesmos slots, sem alocar (injeção `bar_slot_count`).
    pub(in crate::game) fn mb_bar_slot_count(&self, count: usize) -> usize {
        match self.mb_bar_menu() {
            Some(menu) => count + armor_bar::extra_count(menu),
            None => count,
        }
    }

    /// `Game::slot_position` de um slot da barra: onde a tela do bloco o declarou, no lugar de onde o inventário do jogo põe a armadura
    /// (injeção `bar_slot_position`).
    pub(in crate::game) fn mb_bar_slot_position(&self, id: usize, found: (f32, f32)) -> (f32, f32) {
        if let Some(menu) = self.mb_bar_menu()
            && let Some(position) = armor_bar::slot_position(menu, id)
        {
            return position;
        }
        found
    }

    /// `GuiMekanism.hasClickedOutside`: `super.hasClickedOutside(..) && findElementOver(..) == null`. O ponto sobre um elemento da tela fora
    /// da imagem (um slot da barra, uma aba declarada em `screen_elements` e ativa com os data slots de agora, a janela de upgrades) não é
    /// "fora": a injeção `bar_clicked_outside` responde falso antes de o jogo olhar só o retângulo da imagem, e o que não está sobre nenhum
    /// elemento segue pela conta dele.
    pub(in crate::game) fn mb_bar_over_element(&self, batch: &HudBatch) -> bool {
        if !self.native_screen_shown() {
            return false;
        }
        let (ox, oy) = self.inventory_origin(batch);
        let at = [self.cursor.0 - ox, self.cursor.1 - oy];
        self.remote_menu_type().is_some_and(|menu| {
            armor_bar::over_slot(menu, at) || screen_elements::over_element(menu, at, &self.mb_screen_data())
        }) || self.mb_window_over(at)
    }
}

/// `Game::menu_slot` do servidor para o cliente: o índice do menu depois dos 36 do inventário é um slot da barra, que o jogo conhece como
/// 500 a 504 (injeção `bar_menu_slot`, no começo de `menu_slot`). `menu_slot_index` percorre os índices até o primeiro `None`, então os
/// cinco têm de vir logo depois do inventário, sem buraco.
pub(crate) fn menu_slot(menu: &str, slot: i32) -> ControlFlow<Option<usize>> {
    if !armor_bar::declared(menu) {
        return ControlFlow::Continue(());
    }
    let (Ok(index), Some(menu_type)) = (usize::try_from(slot), horizon_client::containers::mod_menu_type(menu)) else {
        return ControlFlow::Continue(());
    };
    match armor_bar::client_of_menu_index(menu, menu_type.slots, index) {
        Some(id) => ControlFlow::Break(Some(id)),
        None => ControlFlow::Continue(()),
    }
}
