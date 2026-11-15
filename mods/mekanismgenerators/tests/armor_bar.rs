//! A barra de armadura e mão da tela do Heat Generator sem o jogo: o que o mod declara (`armorSideBar(-20, 11, 0)`), onde cada slot cai no
//! menu do Java (40 a 44 depois dos 4 do tipo e dos 36 do inventário) e no jogo (500 a 504), o que a tela guarda fora da imagem
//! (`GuiMekanism.hasClickedOutside`), e o shift-clique que o glue monta sobre `machinery_basis_core::quick_move` (a ordem `first + 36 + k`).
//! Os valores são os do rastro do Java (`heat_generator_armor_layout`): índices 40 a 43 de `ArmorSlot` em (-20, 11 + 18 i), `OffhandSlot` em
//! (-20, 83) no 44; e os casos de `heat_generator_armor_quick_move` que valem sem o jogo.
mod common;

use machinery_basis_core::armor_bar;
use machinery_basis_core::armor_slots::{self, EquipmentSlot, SlotKind};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use machinery_basis_core::screen_elements;
use machinery_basis_core::side_tab;
use mekanismgenerators_rules as rules;
use std::path::{Path, PathBuf};

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

fn declare() {
    armor_bar::declare_once(rules::MENU, rules::armor_bar_declaration);
    screen_elements::declare_once(rules::MENU, rules::screen_elements);
}

#[test]
fn the_generator_declares_the_bar_of_the_container_type() {
    // `GeneratorsContainerTypes.java:29`: `.armorSideBar(-20, 11, 0)`.
    let declaration = rules::armor_bar_declaration();
    assert_eq!(
        (declaration.x, declaration.y, declaration.offhand_offset),
        (-20, 11, 0)
    );
    declare();
    let positions: Vec<(i32, i32)> = armor_bar::slots(rules::MENU)
        .iter()
        .map(|slot| (slot.x, slot.y))
        .collect();
    assert_eq!(
        positions,
        [(-20, 11), (-20, 29), (-20, 47), (-20, 65), (-20, 83)]
    );
}

#[test]
fn the_menu_has_the_bar_at_indices_40_to_44_as_in_the_original() {
    // O tipo de menu tem 4 slots (os dois do gerador e os dois virtuais da janela de upgrades): a barra vem depois dos 36 do inventário.
    let menu: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            content().join("data/mekanismgenerators/horizon/menu/heat_generator.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let size = menu["slots"].as_u64().unwrap() as usize;
    assert_eq!(size, rules::MENU_SLOTS);
    declare();
    for (k, index) in (40..45).enumerate() {
        assert_eq!(
            armor_bar::client_of_menu_index(rules::MENU, size, index),
            Some(500 + k),
            "índice {index}"
        );
        assert_eq!(
            armor_bar::menu_index_of_client(rules::MENU, size, 500 + k),
            Some(index)
        );
    }
    assert_eq!(armor_bar::client_of_menu_index(rules::MENU, size, 39), None);
    assert_eq!(armor_bar::client_of_menu_index(rules::MENU, size, 45), None);
    // A armadura espelha os índices 39 a 36 do inventário do jogador e a mão o 40 (`containerSlot` do rastro).
    let mirrored: Vec<usize> = armor_bar::slots(rules::MENU)
        .iter()
        .map(|slot| slot.inventory_index)
        .collect();
    assert_eq!(mirrored, [39, 38, 37, 36, 40]);
}

#[test]
fn the_screen_keeps_the_tabs_outside_the_image_but_not_the_holder_margin() {
    declare();
    // Os elementos não dependem de dado nenhum do servidor: o espelho é o do menu recém-aberto.
    let (client, _) = common::client_of(&common::Server::new(0.0), false);
    // O que o jogo responde (`GuiMekanism.hasClickedOutside` mais o slot do NeoForge): os quadros da barra e os elementos declarados.
    let over = |x: f32, y: f32| {
        armor_bar::over_slot(rules::MENU, [x, y])
            || screen_elements::over_element(rules::MENU, [x, y], &client.mirror)
    };
    // A aba de calor (-26, 109) e a de energia (-26, 137), de 26 por 26, e os cinco quadros dos slots.
    assert!(over(-26.0, 109.0) && over(-1.0, 134.9));
    assert!(over(-26.0, 137.0) && over(-1.0, 162.9));
    assert!(over(-21.0, 10.0) && over(-4.1, 99.9));
    // O suporte (`GuiSideHolder.create(this, -26, 6, 98, ...)`) não é ativo: a margem dele e o que passa dos 26 de altura das abas não valem.
    assert!(!over(-24.0, 50.0) && !over(-2.0, 50.0) && !over(-15.0, 7.0));
    assert!(!over(-15.0, 135.5));
    assert!(!over(-15.0, 163.0));
    // `GuiHeatTab` e `GuiEnergyTab` são as mesmas constantes que o desenho e o tooltip usam.
    let rects: Vec<_> = rules::screen_elements().iter().map(|e| e.rect()).collect();
    // E os botões das abas de redstone (179, 141) e de segurança (179, 38) à direita, entre elas.
    assert_eq!(
        rects,
        [
            rules::HEAT_TAB.rect,
            [179.0, 141.0, 18.0, 18.0],
            [179.0, 38.0, 18.0, 18.0],
            rules::ENERGY_TAB.rect,
            // A aba de avisos à direita, que só responde com o aviso de redstone.
            [176.0, 109.0, 26.0, 26.0]
        ]
    );
}

#[test]
fn the_holder_is_the_left_nine_slice_of_the_mekanism_sprites() {
    let sprites = content().join("../../mekanism/content/assets/mekanism/textures/gui/sprites");
    let mcmeta = std::fs::read_to_string(sprites.join("holder_left.png.mcmeta")).unwrap();
    assert_eq!(
        NineSlice::from_mcmeta(&mcmeta).unwrap(),
        side_tab::HOLDER_SLICE
    );
    assert!(sprites.join("holder_left.png").exists());
    assert_eq!(
        rules::ARMOR_HOLDER_SPRITE,
        "mekanism:gui/sprites/holder_left"
    );
    // `GuiSideHolder.create(this, -26, 6, 98, true, true, TAB_ARMOR_SLOTS)`: o suporte cobre os cinco slots com folga, de 26 de largura.
    assert_eq!(rules::ARMOR_HOLDER, [-26.0, 6.0, 26.0, 98.0]);
    // `TAB_ARMOR_SLOTS` é o pixel 21 de `colormap/gui_objects.png` (o 22º `register()`), que o arquivo do Mekanism deixa branco.
    assert_eq!(rules::ARMOR_HOLDER_TINT, 0xFF_FF_FF_FF);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O shift-clique: o glue monta o menu como `GeneratorMenu` (`first` slots do bloco, os 36 do jogador, depois a barra) sobre o `quick_move` da lib.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Item {
    id: &'static str,
    max_stack: u32,
    equip: Option<EquipmentSlot>,
    fuel: bool,
}

const fn item(id: &'static str, max_stack: u32, equip: Option<EquipmentSlot>, fuel: bool) -> Item {
    Item {
        id,
        max_stack,
        equip,
        fuel,
    }
}

const IRON_HELMET: Item = item("iron_helmet", 1, Some(EquipmentSlot::Head), false);
const IRON_CHESTPLATE: Item = item("iron_chestplate", 1, Some(EquipmentSlot::Chest), false);
const CARVED_PUMPKIN: Item = item("carved_pumpkin", 64, Some(EquipmentSlot::Head), false);
const COAL: Item = item("coal", 64, None, true);
const LAVA_BUCKET: Item = item("lava_bucket", 1, None, true);
const DIRT: Item = item("dirt", 64, None, false);

type Cell = Option<(Item, u32)>;

/// O menu de `GeneratorMenu` sem o jogo: o slot de combustível (0), o de energia (1, que não aceita item), os dois virtuais (2 e 3, que
/// não existem sem a janela), 36 do jogador (a barra de atalhos primeiro, 0 a 8; depois o principal) e a barra.
struct Menu {
    tile: [Cell; 4],
    inventory: [Cell; 36],
    bar: [Cell; 5],
}

impl Menu {
    fn new() -> Self {
        declare();
        Self {
            tile: [None; 4],
            inventory: [None; 36],
            bar: [None; 5],
        }
    }

    fn first() -> usize {
        rules::MENU_SLOTS
    }

    fn bar_index(slot: usize) -> Option<usize> {
        slot.checked_sub(Self::first() + armor_bar::PLAYER_SLOTS)
            .filter(|k| *k < 5)
    }

    fn cell(&mut self, slot: usize) -> &mut Cell {
        if slot < Self::first() {
            &mut self.tile[slot]
        } else if let Some(k) = Self::bar_index(slot) {
            &mut self.bar[k]
        } else {
            &mut self.inventory[slot - Self::first()]
        }
    }

    /// Os índices do menu do Java para os do teste: 4 a 30 o principal, 31 a 39 a barra de atalhos, 40 a 44 a barra de armadura.
    fn java(index: usize) -> usize {
        match index {
            4..31 => Self::first() + 9 + (index - 4),
            31..40 => Self::first() + (index - 31),
            _ => index,
        }
    }
}

impl QuickMenu for Menu {
    type Item = Item;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        let first = Self::first();
        match region {
            Region::Container => rules::CONTAINER_ORDER.to_vec(),
            Region::Hotbar => (first..first + 9).collect(),
            Region::Main => (first + 9..first + 36).collect(),
            Region::Armor => (first + 36..first + 40).collect(),
            Region::Offhand => vec![first + 40],
        }
    }

    fn region(&self, slot: usize) -> Region {
        let first = Self::first();
        if slot < first {
            Region::Container
        } else if let Some(k) = Self::bar_index(slot) {
            if k < 4 {
                Region::Armor
            } else {
                Region::Offhand
            }
        } else if slot - first < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    fn exists(&self, slot: usize) -> bool {
        // Os virtuais da janela de upgrades não existem sem ela.
        slot != rules::UPGRADE_SLOT && slot != rules::UPGRADE_OUTPUT_SLOT
    }

    fn stack(&self, slot: usize) -> Option<(Item, u32)> {
        let first = Self::first();
        if slot < first {
            self.tile[slot]
        } else if let Some(k) = Self::bar_index(slot) {
            self.bar[k]
        } else {
            self.inventory[slot - first]
        }
    }

    fn insert(&mut self, slot: usize, item: &Item, amount: u32) -> u32 {
        let first = Self::first();
        let (held, same) = match self.stack(slot) {
            Some((held, count)) => (count, held == *item),
            None => (0, false),
        };
        let moved = if let Some(k) = Self::bar_index(slot) {
            let kind = armor_bar::slots(rules::MENU)[k].kind;
            armor_slots::insert(kind, held, same, amount, item.max_stack, |slot| {
                item.equip == Some(slot)
            })
        } else if slot < first {
            // O combustível aceita o que queima; o de energia nada (nenhum item portado tem handler de energia).
            if slot != rules::FUEL_SLOT || !item.fuel || (held > 0 && !same) {
                0
            } else {
                amount.min(item.max_stack.saturating_sub(held))
            }
        } else if held > 0 && !same {
            0
        } else {
            amount.min(item.max_stack.saturating_sub(held))
        };
        if moved > 0 {
            *self.cell(slot) = Some((*item, held + moved));
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        let cell = self.cell(slot);
        if let Some((held, count)) = *cell {
            *cell = (count > amount).then_some((held, count - amount));
        }
    }
}

fn menu_with(slot: usize, held: Item, count: u32) -> Menu {
    let mut menu = Menu::new();
    *menu.cell(Menu::java(slot)) = Some((held, count));
    menu
}

fn bar_of(menu: &Menu) -> [Cell; 5] {
    menu.bar
}

#[test]
fn an_armor_piece_from_the_inventory_goes_to_the_slot_of_its_kind() {
    // `direct_main_iron_helmetx1_armor_empty`: do principal (15) para a cabeça (40); e a peitoral para o 41 (`..._chestplate...`).
    let mut menu = menu_with(15, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, Menu::java(15)), Some(1));
    assert_eq!(
        bar_of(&menu),
        [Some((IRON_HELMET, 1)), None, None, None, None]
    );
    let mut menu = menu_with(39, IRON_CHESTPLATE, 1);
    assert_eq!(quick_move(&mut menu, Menu::java(39)), Some(1));
    assert_eq!(bar_of(&menu)[1], Some((IRON_CHESTPLATE, 1)));
    // `shift_hotbar_carved_pumpkin`: a abóbora vai à cabeça, de uma pilha que empilharia.
    let mut menu = menu_with(39, CARVED_PUMPKIN, 1);
    assert_eq!(quick_move(&mut menu, Menu::java(39)), Some(1));
    assert_eq!(bar_of(&menu)[0], Some((CARVED_PUMPKIN, 1)));
}

#[test]
fn a_stack_of_two_helmets_puts_one_on_and_the_other_in_the_other_group() {
    // Da barra de atalhos: 1 na armadura (só vazios) e 1 no inventário principal; a barra de atalhos esvazia.
    let mut menu = menu_with(
        31,
        item("iron_helmet", 4, Some(EquipmentSlot::Head), false),
        2,
    );
    assert_eq!(quick_move(&mut menu, Menu::java(31)), Some(2));
    assert_eq!(menu.bar[0].map(|(_, count)| count), Some(1));
    assert_eq!(menu.inventory[0], None, "a barra de atalhos esvazia");
    assert_eq!(
        menu.inventory[9].map(|(_, count)| count),
        Some(1),
        "o resto no principal"
    );
}

#[test]
fn fuel_goes_to_the_fuel_slot_before_the_armor_and_the_other_group() {
    let mut menu = menu_with(31, COAL, 5);
    assert_eq!(quick_move(&mut menu, Menu::java(31)), Some(5));
    assert_eq!(menu.tile[rules::FUEL_SLOT], Some((COAL, 5)));
    // Um balde de lava também: o slot de combustível aceita o que queima, e ele só leva um.
    let mut menu = menu_with(10, LAVA_BUCKET, 1);
    assert_eq!(quick_move(&mut menu, Menu::java(10)), Some(1));
    assert_eq!(menu.tile[rules::FUEL_SLOT], Some((LAVA_BUCKET, 1)));
    assert!(bar_of(&menu).iter().all(Option::is_none));
}

#[test]
fn what_does_not_wear_goes_to_the_other_group_and_never_into_the_offhand() {
    let mut menu = menu_with(31, DIRT, 5);
    assert_eq!(quick_move(&mut menu, Menu::java(31)), Some(5));
    assert_eq!(
        menu.inventory[9],
        Some((DIRT, 5)),
        "da barra de atalhos para o principal"
    );
    assert!(
        bar_of(&menu).iter().all(Option::is_none),
        "a mão nunca é destino"
    );
}

#[test]
fn a_worn_slot_takes_no_second_piece_and_a_full_target_moves_nothing() {
    let mut menu = menu_with(31, IRON_HELMET, 1);
    menu.bar[0] = Some((IRON_HELMET, 1));
    for slot in 4..40 {
        *menu.cell(Menu::java(slot)) = menu.cell(Menu::java(slot)).or(Some((DIRT, 64)));
    }
    *menu.cell(Menu::java(31)) = Some((IRON_HELMET, 1));
    assert_eq!(quick_move(&mut menu, Menu::java(31)), None);
}

#[test]
fn from_the_armor_slot_the_item_goes_to_the_hotbar_first_and_never_into_another_armor_slot() {
    let mut menu = Menu::new();
    menu.bar[1] = Some((IRON_CHESTPLATE, 1));
    assert_eq!(quick_move(&mut menu, Menu::java(41)), Some(1));
    assert_eq!(menu.bar[1], None);
    assert_eq!(
        menu.inventory[0],
        Some((IRON_CHESTPLATE, 1)),
        "a primeira vaga da barra de atalhos"
    );
    // Da mão para a barra de atalhos.
    let mut menu = Menu::new();
    menu.bar[4] = Some((DIRT, 20));
    assert_eq!(quick_move(&mut menu, Menu::java(44)), Some(20));
    assert_eq!(menu.inventory[0], Some((DIRT, 20)));
}

#[test]
fn the_bar_slot_kinds_follow_the_equipment_order() {
    declare();
    let kinds: Vec<SlotKind> = armor_bar::slots(rules::MENU)
        .iter()
        .map(|slot| slot.kind)
        .collect();
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
