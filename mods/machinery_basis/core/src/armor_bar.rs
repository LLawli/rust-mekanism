//! A barra de armadura de um tipo de menu (`ContainerBuilder.armorSideBar`) e os elementos de tela que ficam fora da imagem dele.
//!
//! O mod declara por tipo de menu, uma vez, onde a coluna de armadura fica e quais retângulos de elementos laterais a tela tem (as
//! abas). O registro é um só no processo, porque o servidor (que monta o menu com os slots a mais) e o cliente (que os põe na tela e
//! os mapeia para os ids do jogo) rodam em threads diferentes e precisam da mesma resposta. A lib não conhece id de menu do Mekanism:
//! ele chega por parâmetro.
//!
//! No menu do servidor os 5 slots vêm depois dos 36 do inventário, na ordem do Java (`addArmorSlots`): capacete, peitoral, calças,
//! botas e a mão. No cliente o jogo os conhece como 500 a 503 (a armadura, da cabeça aos pés) e 504 (a mão), os ids que ele já usa
//! na tela do inventário.
use crate::armor_slots::{self, PlacedSlot};
use crate::gui::{Rect, contains};
use std::sync::Mutex;

/// O id de cliente do primeiro slot da barra (`Game::armor[0]`, a cabeça).
pub const CLIENT_BASE: usize = 500;
/// Os slots do jogador que o menu de um contêiner de mod põe depois dos do bloco (principal e barra de atalhos).
pub const PLAYER_SLOTS: usize = 36;
/// O lado do quadro de um slot (`GuiSlot`: `SlotType.SLOT_SIZE`), que começa um pixel antes do item.
pub const FRAME: f32 = 18.0;

/// O que o mod declara de um tipo de menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    /// `armorSideBar(x, y, offhandOffset)`.
    pub x: i32,
    pub y: i32,
    pub offhand_offset: i32,
    /// Os elementos da tela que recebem o mouse fora da imagem (as abas laterais): clicar neles não é clicar fora
    /// (`GuiMekanism.hasClickedOutside`). Retângulos relativos ao canto da tela.
    pub elements: Vec<Rect>,
}

static REGISTRY: Mutex<Vec<(String, Declaration)>> = Mutex::new(Vec::new());

fn with<R>(f: impl FnOnce(&mut Vec<(String, Declaration)>) -> R) -> R {
    let mut registry = REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut registry)
}

/// Declara (ou troca) a barra do tipo de menu `menu`. Pode ser chamada a qualquer momento e quantas vezes quiser.
pub fn declare(menu: &str, declaration: Declaration) {
    with(
        |registry| match registry.iter_mut().find(|(name, _)| name == menu) {
            Some((_, existing)) => *existing = declaration,
            None => registry.push((menu.to_owned(), declaration)),
        },
    );
}

/// Declara só se o tipo ainda não tem declaração: o mod pode chamar a cada consulta sem montar a declaração de novo.
pub fn declare_once(menu: &str, declaration: impl FnOnce() -> Declaration) {
    if !declared(menu) {
        declare(menu, declaration());
    }
}

/// Se o tipo de menu tem barra de armadura (`armorSlotsX != -1 && armorSlotsY != -1`).
pub fn declared(menu: &str) -> bool {
    with(|registry| registry.iter().any(|(name, _)| name == menu))
}

/// Os slots da barra, na ordem do menu (`armor_side_bar`). Vazio sem declaração.
pub fn slots(menu: &str) -> Vec<PlacedSlot> {
    with(|registry| {
        registry
            .iter()
            .find(|(name, _)| name == menu)
            .map(|(_, d)| armor_slots::armor_side_bar(d.x, d.y, d.offhand_offset))
            .unwrap_or_default()
    })
}

/// Quantos slots o menu ganha depois dos 36 do inventário. Sem alocar: o desenho pergunta a cada quadro.
pub fn extra_count(menu: &str) -> usize {
    with(|registry| {
        registry
            .iter()
            .find(|(name, _)| name == menu)
            .map_or(0, |(_, d)| {
                if d.x == armor_slots::NO_SIDE_BAR || d.y == armor_slots::NO_SIDE_BAR {
                    return 0;
                }
                armor_slots::ARMOR_SLOT_COUNT as usize
                    + usize::from(d.offhand_offset != armor_slots::NO_OFFHAND)
            })
    })
}

/// O id de cliente do slot `k` da barra.
pub fn client_id(k: usize) -> usize {
    CLIENT_BASE + k
}

/// O slot da barra (`0` a `extra_count - 1`) que um id de cliente é.
pub fn index_of_client(menu: &str, id: usize) -> Option<usize> {
    let k = id.checked_sub(CLIENT_BASE)?;
    (k < extra_count(menu)).then_some(k)
}

/// O id de cliente de um índice do menu do servidor (`size` slots do bloco, os 36 do inventário e depois a barra), ou `None` para os
/// slots que o jogo já mapeia sozinho e para o que passa do fim.
pub fn client_of_menu_index(menu: &str, size: usize, index: usize) -> Option<usize> {
    let k = index.checked_sub(size + PLAYER_SLOTS)?;
    (k < extra_count(menu)).then(|| client_id(k))
}

/// O índice do menu do servidor de um id de cliente da barra.
pub fn menu_index_of_client(menu: &str, size: usize, id: usize) -> Option<usize> {
    index_of_client(menu, id).map(|k| size + PLAYER_SLOTS + k)
}

/// Onde a tela põe o slot de id de cliente `id` (relativo ao canto da tela), se ele é da barra.
pub fn slot_position(menu: &str, id: usize) -> Option<(f32, f32)> {
    let k = index_of_client(menu, id)?;
    slots(menu)
        .get(k)
        .map(|slot| (slot.x as f32, slot.y as f32))
}

/// O quadro de um slot (`GuiSlot`): 18 por 18, um pixel antes do item. É também a área em que o mouse está sobre o slot
/// (`isHovering`: o slot de 16 cresce um pixel de cada lado).
pub fn frame(slot: &PlacedSlot) -> Rect {
    [(slot.x - 1) as f32, (slot.y - 1) as f32, FRAME, FRAME]
}

/// `GuiMekanism.hasClickedOutside`: o ponto (relativo ao canto da tela) está sobre algo que impede o clique de ser "fora". Os
/// slots da barra contam, porque o NeoForge não deixa um slot sob o mouse ser clicado como fora
/// (`AbstractContainerScreen.mouseClicked`: `if (slot != null) clickedOutside = false`), e os elementos declarados.
pub fn over_element(menu: &str, at: [f32; 2]) -> bool {
    let (slots, elements) = with(|registry| {
        registry
            .iter()
            .find(|(name, _)| name == menu)
            .map(|(_, d)| {
                (
                    armor_slots::armor_side_bar(d.x, d.y, d.offhand_offset),
                    d.elements.clone(),
                )
            })
            .unwrap_or_default()
    });
    slots.iter().any(|slot| contains(frame(slot), at))
        || elements.into_iter().any(|rect| contains(rect, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armor_slots::{EquipmentSlot, SlotKind};

    /// Cada teste usa o próprio nome de menu: o registro é um só no processo e os testes rodam em paralelo.
    fn generator(menu: &str) {
        declare(
            menu,
            Declaration {
                x: -20,
                y: 11,
                offhand_offset: 0,
                elements: vec![[-26.0, 109.0, 26.0, 26.0], [-26.0, 137.0, 26.0, 26.0]],
            },
        );
    }

    #[test]
    fn a_menu_without_a_declaration_has_no_bar() {
        assert!(!declared("test:none"));
        assert_eq!(extra_count("test:none"), 0);
        assert!(slots("test:none").is_empty());
        assert_eq!(client_of_menu_index("test:none", 4, 40), None);
        assert_eq!(menu_index_of_client("test:none", 4, 500), None);
        assert_eq!(slot_position("test:none", 500), None);
        assert!(!over_element("test:none", [-15.0, 15.0]));
    }

    #[test]
    fn the_heat_generator_bar_has_five_slots_after_the_inventory() {
        generator("test:generator_a");
        assert!(declared("test:generator_a"));
        assert_eq!(extra_count("test:generator_a"), 5);
        let kinds: Vec<SlotKind> = slots("test:generator_a").iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                SlotKind::Armor(EquipmentSlot::Head),
                SlotKind::Armor(EquipmentSlot::Chest),
                SlotKind::Armor(EquipmentSlot::Legs),
                SlotKind::Armor(EquipmentSlot::Feet),
                SlotKind::Offhand,
            ]
        );
    }

    #[test]
    fn a_menu_without_an_offhand_has_four() {
        declare(
            "test:no_offhand",
            Declaration {
                x: -20,
                y: 67,
                offhand_offset: armor_slots::NO_OFFHAND,
                elements: Vec::new(),
            },
        );
        assert_eq!(extra_count("test:no_offhand"), 4);
        assert_eq!(client_of_menu_index("test:no_offhand", 4, 44), None);
        assert_eq!(client_of_menu_index("test:no_offhand", 4, 43), Some(503));
    }

    #[test]
    fn a_declaration_without_a_side_bar_only_declares_elements() {
        // `armorSlotsX` ou `armorSlotsY` em -1: o contêiner não chama `addArmorSlots`, mas a tela ainda pode ter abas fora da imagem.
        declare(
            "test:only_tabs",
            Declaration {
                x: armor_slots::NO_SIDE_BAR,
                y: armor_slots::NO_SIDE_BAR,
                offhand_offset: armor_slots::NO_OFFHAND,
                elements: vec![[-26.0, 137.0, 26.0, 26.0]],
            },
        );
        assert!(declared("test:only_tabs"));
        assert_eq!(extra_count("test:only_tabs"), 0);
        assert!(slots("test:only_tabs").is_empty());
        assert_eq!(client_of_menu_index("test:only_tabs", 4, 40), None);
        assert_eq!(slot_position("test:only_tabs", 500), None);
        assert!(over_element("test:only_tabs", [-10.0, 150.0]));
        assert!(!over_element("test:only_tabs", [-15.0, 15.0]));
    }

    #[test]
    fn the_menu_indices_map_to_the_client_ids_the_game_already_uses() {
        generator("test:generator_b");
        // O menu do Java: 0 e 1 virtuais, 2 e 3 do gerador (os 4 do tipo de menu), 4 a 39 o inventário e 40 a 44 a barra.
        let size = 4;
        for (index, id) in [(40, 500), (41, 501), (42, 502), (43, 503), (44, 504)] {
            assert_eq!(
                client_of_menu_index("test:generator_b", size, index),
                Some(id)
            );
            assert_eq!(
                menu_index_of_client("test:generator_b", size, id),
                Some(index)
            );
        }
        // Os slots do bloco e do inventário o jogo mapeia; o que passa da barra não existe.
        for index in [0, 3, 4, 39, 45, 46] {
            assert_eq!(client_of_menu_index("test:generator_b", size, index), None);
        }
        for id in [499, 505, 0, 35, 300] {
            assert_eq!(menu_index_of_client("test:generator_b", size, id), None);
        }
    }

    #[test]
    fn the_client_positions_are_the_declared_ones() {
        generator("test:generator_c");
        let positions: Vec<_> = (500..505)
            .map(|id| slot_position("test:generator_c", id).unwrap())
            .collect();
        assert_eq!(
            positions,
            [
                (-20.0, 11.0),
                (-20.0, 29.0),
                (-20.0, 47.0),
                (-20.0, 65.0),
                (-20.0, 83.0)
            ]
        );
        assert_eq!(slot_position("test:generator_c", 505), None);
        assert_eq!(index_of_client("test:generator_c", 504), Some(4));
        assert_eq!(index_of_client("test:generator_c", 505), None);
        assert_eq!(index_of_client("test:generator_c", 499), None);
    }

    #[test]
    fn a_frame_starts_one_pixel_before_the_item_and_is_18_wide() {
        let slot = &slots_of_generator("test:generator_d")[0];
        assert_eq!(frame(slot), [-21.0, 10.0, 18.0, 18.0]);
    }

    fn slots_of_generator(menu: &str) -> Vec<PlacedSlot> {
        generator(menu);
        slots(menu)
    }

    #[test]
    fn the_mouse_over_a_bar_slot_or_a_declared_tab_is_not_outside() {
        generator("test:generator_e");
        let over = |x: f32, y: f32| over_element("test:generator_e", [x, y]);
        // Os cinco quadros, inclusive o pixel entre dois (cada quadro tem 18 e o passo é 18).
        for y in [10.0, 27.9, 28.0, 45.0, 63.0, 81.0, 99.9] {
            assert!(over(-21.0, y), "y {y}");
            assert!(over(-4.1, y), "y {y}");
        }
        // As abas de energia e de calor.
        assert!(over(-26.0, 109.0));
        assert!(over(-1.0, 162.9));
        // O suporte em volta dos slots (`GuiSideHolder` não está ativo): a folga de 5 pixels à esquerda e a de 3 à direita, o alto e o baixo.
        assert!(!over(-22.0, 50.0));
        assert!(!over(-3.0, 50.0));
        assert!(!over(-15.0, 9.9));
        assert!(!over(-15.0, 100.0));
        // O resto de fora da imagem.
        assert!(!over(-40.0, 50.0));
        assert!(!over(-15.0, 108.9));
    }

    #[test]
    fn declaring_again_replaces_and_declare_once_keeps() {
        generator("test:generator_f");
        declare_once("test:generator_f", || panic!("já declarado"));
        declare(
            "test:generator_f",
            Declaration {
                x: 180,
                y: 41,
                offhand_offset: 0,
                elements: Vec::new(),
            },
        );
        assert_eq!(slot_position("test:generator_f", 500), Some((180.0, 41.0)));
        assert!(!over_element("test:generator_f", [-15.0, 15.0]));
        declare_once("test:generator_g", || Declaration {
            x: 1,
            y: 2,
            offhand_offset: 0,
            elements: Vec::new(),
        });
        assert_eq!(slot_position("test:generator_g", 500), Some((1.0, 2.0)));
    }
}
