//! O shift-clique num menu com a coluna de armadura e a mão (o do Heat Generator: `armorSideBar(-20, 11, 0)`), com as regras reais dos
//! slots (`armor_slots::insert`) por trás do `quick_move`. Cada valor esperado sai de seguir `MekanismContainer.quickMoveStack` e
//! `insertItem` (`MekanismContainer.java:276-382`) à mão, linha a linha, e o motivo vem junto do caso.
//!
//! O fim do arquivo repete o rastro do Java (`dev/referencia/rastros/heat_generator_armor_*.jsonl`) contra as mesmas regras.

use machinery_basis_core::armor_slots::{self, EquipmentSlot, SlotKind};
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};

/// O que o jogo sabe de um item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Item {
    id: &'static str,
    max_stack: u32,
    /// Em que slot de armadura o item se veste (`ItemStack.canEquip`).
    equip: Option<EquipmentSlot>,
}

const IRON_HELMET: Item = wearable("iron_helmet", EquipmentSlot::Head);
const DIAMOND_HELMET: Item = wearable("diamond_helmet", EquipmentSlot::Head);
const IRON_CHESTPLATE: Item = wearable("iron_chestplate", EquipmentSlot::Chest);
const IRON_BOOTS: Item = wearable("iron_boots", EquipmentSlot::Feet);
/// Um capacete que empilha: serve para ver o que o Java faz com o resto quando a armadura só aceita um.
const STACKING_HELMET: Item = Item {
    id: "stacking_helmet",
    max_stack: 64,
    equip: Some(EquipmentSlot::Head),
};
const STONE: Item = plain("stone", 64);
const COAL: Item = plain("coal", 64);
const PEARL: Item = plain("ender_pearl", 16);
const TOTEM: Item = plain("totem_of_undying", 1);

const fn wearable(id: &'static str, slot: EquipmentSlot) -> Item {
    Item {
        id,
        max_stack: 1,
        equip: Some(slot),
    }
}

const fn plain(id: &'static str, max_stack: u32) -> Item {
    Item {
        id,
        max_stack,
        equip: None,
    }
}

/// A qual grupo do Java o slot pertence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// `InventoryContainerSlot`: o slot da máquina, que aceita o que a lista diz.
    Tile(&'static [&'static str]),
    Main,
    Hotbar,
    Armor(EquipmentSlot),
    Offhand,
}

struct Menu {
    classes: Vec<Class>,
    contents: Vec<Option<(Item, u32)>>,
}

const MAIN_SLOTS: usize = 27;
const HOTBAR_SLOTS: usize = 9;

impl Menu {
    /// O menu do Java: o slot da máquina (índice 0, aceita só carvão), o inventário principal (1 a 27), a barra (28 a 36), a armadura
    /// com o capacete primeiro (37 a 40) e a mão (41).
    fn heat_generator() -> Self {
        Self::with_tile(&["coal"])
    }

    fn with_tile(accepts: &'static [&'static str]) -> Self {
        let mut classes = vec![Class::Tile(accepts)];
        classes.extend([Class::Main; MAIN_SLOTS]);
        classes.extend([Class::Hotbar; HOTBAR_SLOTS]);
        for placed in armor_slots::armor_side_bar(-20, 11, 0) {
            classes.push(match placed.kind {
                SlotKind::Armor(slot) => Class::Armor(slot),
                SlotKind::Offhand => Class::Offhand,
            });
        }
        let contents = vec![None; classes.len()];
        Self { classes, contents }
    }

    fn first(&self, matching: impl Fn(&Class) -> bool) -> usize {
        self.classes.iter().position(matching).unwrap()
    }

    fn tile(&self) -> usize {
        0
    }

    /// O n-ésimo slot do inventário principal.
    fn main(&self, n: usize) -> usize {
        self.first(|c| *c == Class::Main) + n
    }

    fn hotbar(&self, n: usize) -> usize {
        self.first(|c| *c == Class::Hotbar) + n
    }

    fn armor(&self, slot: EquipmentSlot) -> usize {
        self.first(|c| *c == Class::Armor(slot))
    }

    fn offhand(&self) -> usize {
        self.first(|c| *c == Class::Offhand)
    }

    fn put(&mut self, slot: usize, item: Item, count: u32) {
        self.contents[slot] = Some((item, count));
    }

    fn fill_main_and_hotbar(&mut self, item: Item) {
        for slot in 0..self.classes.len() {
            if matches!(self.classes[slot], Class::Main | Class::Hotbar) {
                self.put(slot, item, 1);
            }
        }
    }
}

impl QuickMenu for Menu {
    type Item = Item;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        (0..self.classes.len())
            .filter(|slot| self.region(*slot) == region)
            .collect()
    }

    fn region(&self, slot: usize) -> Region {
        match self.classes[slot] {
            Class::Tile(_) => Region::Container,
            Class::Main => Region::Main,
            Class::Hotbar => Region::Hotbar,
            Class::Armor(_) => Region::Armor,
            Class::Offhand => Region::Offhand,
        }
    }

    fn exists(&self, _slot: usize) -> bool {
        true
    }

    fn stack(&self, slot: usize) -> Option<(Item, u32)> {
        self.contents[slot]
    }

    fn insert(&mut self, slot: usize, item: &Item, amount: u32) -> u32 {
        let held = self.contents[slot].map_or(0, |(_, count)| count);
        let same = self.contents[slot].is_none_or(|(held_item, _)| held_item == *item);
        let moved = match self.classes[slot] {
            Class::Tile(accepts) => {
                if !accepts.contains(&item.id) || held >= item.max_stack || !same {
                    0
                } else {
                    amount.min(item.max_stack - held)
                }
            }
            Class::Main | Class::Hotbar => {
                if held >= item.max_stack || !same {
                    0
                } else {
                    amount.min(item.max_stack - held)
                }
            }
            Class::Armor(slot_kind) => armor_slots::insert(
                SlotKind::Armor(slot_kind),
                held,
                same,
                amount,
                item.max_stack,
                |equip| item.equip == Some(equip),
            ),
            Class::Offhand => armor_slots::insert(
                SlotKind::Offhand,
                held,
                same,
                amount,
                item.max_stack,
                |_| false,
            ),
        };
        if moved > 0 {
            self.contents[slot] = Some((*item, held + moved));
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some((item, count)) = self.contents[slot] {
            self.contents[slot] = (count > amount).then_some((item, count - amount));
        }
    }
}

use EquipmentSlot::{Chest, Feet, Head, Legs};

#[test]
fn the_menu_has_the_java_layout() {
    let menu = Menu::heat_generator();
    assert_eq!(menu.slots_of(Region::Container), [0]);
    assert_eq!(menu.slots_of(Region::Main).len(), 27);
    assert_eq!(menu.slots_of(Region::Hotbar).len(), 9);
    // O capacete é o primeiro da coluna, e a mão vem depois dos quatro.
    assert_eq!(menu.slots_of(Region::Armor), [37, 38, 39, 40]);
    assert_eq!(menu.armor(Head), 37);
    assert_eq!(menu.armor(Feet), 40);
    assert_eq!(menu.slots_of(Region::Offhand), [41]);
}

// ---- Da inventário (principal ou barra) para a máquina e a armadura ------------------------------------------------------------

#[test]
fn a_helmet_in_the_main_inventory_goes_to_the_empty_head_slot() {
    let mut menu = Menu::heat_generator();
    let from = menu.main(3);
    menu.put(from, IRON_HELMET, 1);
    // A máquina recusa; o ramo `MainInventorySlot` tenta `armorSlots` só nos vazios (`:308`): o capacete só cabe na cabeça.
    assert_eq!(quick_move(&mut menu, from), Some(1));
    assert_eq!(menu.contents[from], None);
    assert_eq!(menu.contents[menu.armor(Head)], Some((IRON_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], None);
}

#[test]
fn boots_and_a_chestplate_pick_their_own_slot() {
    let mut menu = Menu::heat_generator();
    let boots = menu.hotbar(2);
    menu.put(boots, IRON_BOOTS, 1);
    assert_eq!(quick_move(&mut menu, boots), Some(1));
    assert_eq!(menu.contents[menu.armor(Feet)], Some((IRON_BOOTS, 1)));
    for other in [Head, Chest, Legs] {
        assert_eq!(menu.contents[menu.armor(other)], None);
    }
    let chest = menu.main(0);
    menu.put(chest, IRON_CHESTPLATE, 1);
    assert_eq!(quick_move(&mut menu, chest), Some(1));
    assert_eq!(menu.contents[menu.armor(Chest)], Some((IRON_CHESTPLATE, 1)));
}

#[test]
fn a_helmet_does_not_replace_the_one_already_worn() {
    // `insertItem(armorSlots, ..., ignoreEmpty = false)` pula o slot que tem item, então o capacete vai para a barra.
    let mut menu = Menu::heat_generator();
    menu.put(menu.armor(Head), DIAMOND_HELMET, 1);
    let from = menu.main(0);
    menu.put(from, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, from), Some(1));
    assert_eq!(menu.contents[menu.armor(Head)], Some((DIAMOND_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((IRON_HELMET, 1)));
}

#[test]
fn a_stack_of_helmets_fills_the_head_with_one_and_sends_the_rest_to_the_other_group() {
    // `inserted = armor (1)`, depois `insertItem(hotBarSlots, ..., amount - inserted)` (`:310`) com o resto: 2 no total.
    let mut menu = Menu::heat_generator();
    let from = menu.main(0);
    menu.put(from, STACKING_HELMET, 3);
    assert_eq!(quick_move(&mut menu, from), Some(3));
    assert_eq!(menu.contents[menu.armor(Head)], Some((STACKING_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((STACKING_HELMET, 2)));
    assert_eq!(menu.contents[from], None);
    // Da barra, o resto vai para o principal (`:312`), e a primeira posição do principal é o próprio slot de origem esvaziado só no fim.
    let mut menu = Menu::heat_generator();
    let from = menu.hotbar(4);
    menu.put(from, STACKING_HELMET, 3);
    assert_eq!(quick_move(&mut menu, from), Some(3));
    assert_eq!(menu.contents[menu.armor(Head)], Some((STACKING_HELMET, 1)));
    assert_eq!(menu.contents[menu.main(0)], Some((STACKING_HELMET, 2)));
}

#[test]
fn an_item_that_cannot_be_worn_goes_to_the_other_group_and_never_to_the_offhand() {
    let mut menu = Menu::heat_generator();
    let from = menu.main(5);
    menu.put(from, STONE, 10);
    assert_eq!(quick_move(&mut menu, from), Some(10));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((STONE, 10)));
    assert_eq!(menu.contents[menu.offhand()], None);
    for slot in menu.slots_of(Region::Armor) {
        assert_eq!(menu.contents[slot], None);
    }
    // Da barra para o principal.
    let mut menu = Menu::heat_generator();
    let from = menu.hotbar(8);
    menu.put(from, STONE, 10);
    assert_eq!(quick_move(&mut menu, from), Some(10));
    assert_eq!(menu.contents[menu.main(0)], Some((STONE, 10)));
}

#[test]
fn the_offhand_is_not_a_destination_even_when_everything_else_is_full() {
    let mut menu = Menu::heat_generator();
    menu.fill_main_and_hotbar(TOTEM);
    let from = menu.main(0);
    menu.put(from, STONE, 1);
    assert_eq!(quick_move(&mut menu, from), None);
    assert_eq!(menu.contents[from], Some((STONE, 1)));
    assert_eq!(menu.contents[menu.offhand()], None);
}

#[test]
fn the_machine_takes_the_stack_before_the_armor_is_asked() {
    let mut menu = Menu::with_tile(&["coal", "iron_helmet"]);
    let from = menu.hotbar(0);
    menu.put(from, IRON_HELMET, 1);
    // A máquina tem o primeiro lugar (`insertItem(inventoryContainerSlots, ..., true)` e depois `false`): o capacete entra nela.
    assert_eq!(quick_move(&mut menu, from), Some(1));
    assert_eq!(menu.contents[menu.tile()], Some((IRON_HELMET, 1)));
    assert_eq!(menu.contents[menu.armor(Head)], None);
}

#[test]
fn coal_stacks_in_the_machine_and_then_fills_its_empty_slot() {
    let mut menu = Menu::heat_generator();
    menu.put(menu.tile(), COAL, 5);
    let from = menu.main(0);
    menu.put(from, COAL, 10);
    assert_eq!(quick_move(&mut menu, from), Some(10));
    assert_eq!(menu.contents[menu.tile()], Some((COAL, 15)));
    // Máquina vazia: a segunda passada (`ignoreEmpty = false`).
    let mut menu = Menu::heat_generator();
    let from = menu.main(0);
    menu.put(from, COAL, 10);
    assert_eq!(quick_move(&mut menu, from), Some(10));
    assert_eq!(menu.contents[menu.tile()], Some((COAL, 10)));
}

// ---- Da máquina para o inventário -------------------------------------------------------------------------------------------------

#[test]
fn from_the_machine_a_piece_of_armor_goes_to_the_empty_slot_before_the_hotbar() {
    // `insertItem(concat(armorSlots, hotBarSlots, mainInventorySlots), ...)`: nenhum slot tem o item (passada dos ocupados vazia), e
    // na dos vazios a armadura vem primeiro (`:295`).
    let mut menu = Menu::with_tile(&["iron_helmet", "coal"]);
    menu.put(menu.tile(), IRON_HELMET, 1);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), Some(1));
    assert_eq!(menu.contents[menu.armor(Head)], Some((IRON_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], None);
    assert_eq!(menu.contents[menu.tile()], None);
}

#[test]
fn from_the_machine_a_worn_slot_is_skipped_and_the_hotbar_takes_it() {
    let mut menu = Menu::with_tile(&["iron_helmet", "coal"]);
    menu.put(menu.armor(Head), DIAMOND_HELMET, 1);
    menu.put(menu.tile(), IRON_HELMET, 1);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), Some(1));
    assert_eq!(menu.contents[menu.armor(Head)], Some((DIAMOND_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((IRON_HELMET, 1)));
}

#[test]
fn from_the_machine_a_stack_joins_a_matching_stack_before_any_empty_slot() {
    // Primeira passada em armadura, barra e principal juntas: o carvão da última posição do principal vem antes da barra vazia.
    let mut menu = Menu::heat_generator();
    menu.put(menu.tile(), COAL, 20);
    let held = menu.main(26);
    menu.put(held, COAL, 40);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), Some(20));
    assert_eq!(menu.contents[held], Some((COAL, 60)));
    assert_eq!(menu.contents[menu.hotbar(0)], None);
    // O que não cabe no empilhado vai para o primeiro vazio da barra: 40 + 30 = 64 e sobram 6.
    let mut menu = Menu::heat_generator();
    menu.put(menu.tile(), COAL, 30);
    let held = menu.main(0);
    menu.put(held, COAL, 40);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), Some(30));
    assert_eq!(menu.contents[held], Some((COAL, 64)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((COAL, 6)));
}

#[test]
fn from_the_machine_a_stack_of_armor_never_stacks_in_a_worn_slot() {
    // `ArmorSlot.getMaxStackSize()` é 1: o slot ocupado do mesmo item não aceita mais, e o resto cai na barra.
    let mut menu = Menu::with_tile(&["stacking_helmet"]);
    menu.put(menu.armor(Head), STACKING_HELMET, 1);
    menu.put(menu.tile(), STACKING_HELMET, 2);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), Some(2));
    assert_eq!(menu.contents[menu.armor(Head)], Some((STACKING_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((STACKING_HELMET, 2)));
}

#[test]
fn from_the_machine_nothing_goes_to_the_offhand() {
    let mut menu = Menu::with_tile(&["ender_pearl"]);
    menu.fill_main_and_hotbar(TOTEM);
    menu.put(menu.tile(), PEARL, 4);
    let tile = menu.tile();
    assert_eq!(quick_move(&mut menu, tile), None);
    assert_eq!(menu.contents[menu.offhand()], None);
    assert_eq!(menu.contents[menu.tile()], Some((PEARL, 4)));
}

// ---- Da armadura e da mão para o inventário ----------------------------------------------------------------------------------------

#[test]
fn a_worn_piece_goes_to_the_hotbar_first() {
    // O Java usa `playerSlots = concat(hotBarSlots, mainInventorySlots)` (`:93,306`); o `quick_move` usava o principal primeiro.
    let mut menu = Menu::heat_generator();
    let from = menu.armor(Head);
    menu.put(from, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, from), Some(1));
    assert_eq!(menu.contents[from], None);
    assert_eq!(menu.contents[menu.hotbar(0)], Some((IRON_HELMET, 1)));
    assert_eq!(menu.contents[menu.main(0)], None);
}

#[test]
fn a_worn_piece_never_goes_into_another_armor_slot() {
    // Com a barra e o principal cheios, a peça fica onde está: a armadura não está em `playerSlots`.
    let mut menu = Menu::heat_generator();
    menu.fill_main_and_hotbar(TOTEM);
    let from = menu.armor(Head);
    menu.put(from, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, from), None);
    assert_eq!(menu.contents[from], Some((IRON_HELMET, 1)));
    // Nem as botas, que cabem no slot dos pés, vão de um slot de armadura a outro.
    let mut menu = Menu::heat_generator();
    menu.fill_main_and_hotbar(TOTEM);
    let from = menu.armor(Chest);
    menu.put(from, IRON_BOOTS, 1);
    assert_eq!(quick_move(&mut menu, from), None);
    assert_eq!(menu.contents[menu.armor(Feet)], None);
}

#[test]
fn a_worn_piece_goes_to_the_machine_first_when_it_takes_it() {
    let mut menu = Menu::with_tile(&["iron_helmet"]);
    let from = menu.armor(Head);
    menu.put(from, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, from), Some(1));
    assert_eq!(menu.contents[menu.tile()], Some((IRON_HELMET, 1)));
    assert_eq!(menu.contents[menu.hotbar(0)], None);
}

#[test]
fn the_offhand_empties_into_the_hotbar_first_and_stacks_before_using_an_empty_slot() {
    let mut menu = Menu::heat_generator();
    let from = menu.offhand();
    menu.put(from, STONE, 20);
    assert_eq!(quick_move(&mut menu, from), Some(20));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((STONE, 20)));
    assert_eq!(menu.contents[menu.main(0)], None);
    // Empilha primeiro: as duas passadas correm a barra e depois o principal. A pilha do principal recebe antes de a barra vazia.
    let mut menu = Menu::heat_generator();
    let from = menu.offhand();
    menu.put(from, STONE, 20);
    menu.put(menu.main(10), STONE, 50);
    assert_eq!(quick_move(&mut menu, from), Some(20));
    assert_eq!(menu.contents[menu.main(10)], Some((STONE, 64)));
    assert_eq!(menu.contents[menu.hotbar(0)], Some((STONE, 6)));
    // Uma pilha na barra vem antes da do principal.
    let mut menu = Menu::heat_generator();
    let from = menu.offhand();
    menu.put(from, STONE, 20);
    menu.put(menu.hotbar(7), STONE, 10);
    menu.put(menu.main(0), STONE, 10);
    assert_eq!(quick_move(&mut menu, from), Some(20));
    assert_eq!(menu.contents[menu.hotbar(7)], Some((STONE, 30)));
    assert_eq!(menu.contents[menu.main(0)], Some((STONE, 10)));
}

#[test]
fn the_offhand_does_not_go_into_the_armor_even_when_it_is_wearable() {
    let mut menu = Menu::heat_generator();
    menu.fill_main_and_hotbar(TOTEM);
    let from = menu.offhand();
    menu.put(from, IRON_HELMET, 1);
    assert_eq!(quick_move(&mut menu, from), None);
    assert_eq!(menu.contents[menu.armor(Head)], None);
}

// ---- O rastro do Java -------------------------------------------------------------------------------------------------------------

use armor_slots::{EmptyBackground, PlacedSlot};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Debug;

/// O que o jogo sabe de cada item que o rastro usa: o tamanho da pilha e o slot de armadura onde `ItemStack.canEquip` aceita. A
/// tabela é escrita à mão e conferida linha a linha com `heat_generator_armor_may_place.jsonl` (o `may_place` de cada slot e o
/// `max_stack_for_item` da mão, que é o tamanho da pilha do item).
const TRACE_ITEMS: &[Item] = &[
    wearable("leather_helmet", Head),
    wearable("leather_chestplate", Chest),
    wearable("leather_leggings", Legs),
    wearable("leather_boots", Feet),
    wearable("iron_helmet", Head),
    wearable("iron_chestplate", Chest),
    wearable("iron_leggings", Legs),
    wearable("iron_boots", Feet),
    wearable("elytra", Chest),
    wearable("turtle_helmet", Head),
    stacking_wearable("carved_pumpkin", 64, Head),
    stacking_wearable("skeleton_skull", 64, Head),
    stacking_wearable("player_head", 64, Head),
    plain("shield", 1),
    plain("lava_bucket", 1),
    plain("dirt", 64),
    plain("coal", 64),
    plain("stick", 64),
    plain("cobblestone", 64),
    plain("diamond", 64),
];

const fn stacking_wearable(id: &'static str, max_stack: u32, slot: EquipmentSlot) -> Item {
    Item {
        id,
        max_stack,
        equip: Some(slot),
    }
}

/// O sufixo que o rastro põe num item encantado com `PREVENT_ARMOR_CHANGE` (a maldição do vínculo).
const BINDING_SUFFIX: &str = "+binding";

/// O primeiro índice do menu do Java que é de armadura: a coluna e a mão ficam nos 5 últimos dos 45.
const FIRST_PLACED_SLOT: usize = 40;
const LAST_PLACED_SLOT: usize = 44;
/// O primeiro slot da barra (`inventory.setItem(0, ..)` nos cliques com a tecla de número).
const HOTBAR_ZERO_SLOT: usize = 31;

/// O que o slot de combustível do Heat Generator aceita de quem insere à mão (`FluidFuelInventorySlot.java:57`: `isFuel` ou
/// `canFill`) entre os itens do cenário: o balde de lava, o carvão e o graveto têm `COOKING_FUEL`. O slot de energia
/// (`EnergyInventorySlot.drain`, `:52`) só aceita item que guarda energia, e o cenário não tem nenhum.
const FUEL_SLOT_ITEMS: [&str; 3] = ["lava_bucket", "coal", "stick"];

/// As linhas mínimas de cada arquivo do rastro: um arquivo vazio ou cortado não pode passar.
const MIN_ROWS: [(&str, usize); 5] = [
    ("layout", 46),
    ("may_place", 105),
    ("clicks", 124),
    ("binding", 120),
    ("quick_move", 354),
];

fn base_id(id: &str) -> &str {
    id.strip_suffix(BINDING_SUFFIX).unwrap_or(id)
}

fn is_bound(id: &str) -> bool {
    id.ends_with(BINDING_SUFFIX)
}

fn trace_item(id: &str) -> Item {
    TRACE_ITEMS
        .iter()
        .find(|item| item.id == base_id(id))
        .copied()
        .unwrap_or_else(|| panic!("o item {id:?} do rastro não está na tabela do teste"))
}

fn armor_kind(menu_index: usize) -> Option<SlotKind> {
    (FIRST_PLACED_SLOT..=LAST_PLACED_SLOT)
        .contains(&menu_index)
        .then(|| armor_slots::armor_side_bar(-20, 11, 0)[menu_index - FIRST_PLACED_SLOT].kind)
}

fn can_equip(item: Item) -> impl Fn(EquipmentSlot) -> bool {
    move |slot| item.equip == Some(slot)
}

/// `["id", n]` do rastro, ou `None` para o vazio (`["", 0]`).
fn stack_of(value: &Value) -> Option<(String, u32)> {
    let id = value[0].as_str().unwrap();
    (!id.is_empty()).then(|| (id.to_owned(), value[1].as_u64().unwrap() as u32))
}

/// `[índice, classe, id, quantidade]` de `before` e `slots`.
type Entry = (usize, String, String, u32);

fn entries(value: &Value) -> Vec<Entry> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry[0].as_u64().unwrap() as usize,
                entry[1].as_str().unwrap().to_owned(),
                entry[2].as_str().unwrap().to_owned(),
                entry[3].as_u64().unwrap() as u32,
            )
        })
        .collect()
}

fn at(list: &[Entry], index: usize) -> Option<(String, u32)> {
    list.iter()
        .find(|entry| entry.0 == index)
        .map(|entry| (entry.2.clone(), entry.3))
}

fn read_rows(group: &str) -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(format!("heat_generator_armor_{group}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Junta as divergências em vez de parar na primeira, e conta quantas vezes cada conferência rodou (uma que nunca roda não prova nada).
#[derive(Default)]
struct Checker {
    failures: Vec<String>,
    hits: BTreeMap<&'static str, usize>,
}

impl Checker {
    fn hit(&mut self, what: &'static str) {
        *self.hits.entry(what).or_default() += 1;
    }

    fn eq<T: PartialEq + Debug>(&mut self, ctx: &str, field: &str, rust: T, java: T) {
        if rust != java {
            self.failures
                .push(format!("{ctx}: {field}: porte {rust:?}, Java {java:?}"));
        }
    }

    fn ok(&mut self, ctx: &str, field: &str, holds: bool) {
        if !holds {
            self.failures.push(format!("{ctx}: {field}"));
        }
    }
}

fn label(row: &Value) -> String {
    format!(
        "{}/{}/{}/{}",
        row["group"].as_str().unwrap_or("?"),
        row["case"].as_str().unwrap_or("?"),
        row["session"].as_str().unwrap_or("-"),
        row["slot"].as_u64().or(row["from"].as_u64()).unwrap_or(0)
    )
}

// ---- 1. layout ---------------------------------------------------------------------------------------------------------------------

fn empty_background_icon(background: EmptyBackground) -> &'static str {
    match background {
        EmptyBackground::Helmet => "minecraft:container/slot/helmet",
        EmptyBackground::Chestplate => "minecraft:container/slot/chestplate",
        EmptyBackground::Leggings => "minecraft:container/slot/leggings",
        EmptyBackground::Boots => "minecraft:container/slot/boots",
        EmptyBackground::Shield => "minecraft:container/slot/shield",
    }
}

fn equipment_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        Head => "HEAD",
        Chest => "CHEST",
        Legs => "LEGS",
        Feet => "FEET",
    }
}

fn check_layout(c: &mut Checker, rows: &[Value]) {
    let summary = &rows[0];
    c.eq(
        "layout/summary",
        "menu_slots",
        45,
        summary["menu_slots"].as_i64().unwrap(),
    );
    c.eq(
        "layout/summary",
        "armor_slots",
        i64::from(armor_slots::ARMOR_SLOT_COUNT),
        summary["armor_slots"].as_i64().unwrap(),
    );
    c.eq(
        "layout/summary",
        "offhand_slots",
        1,
        summary["offhand_slots"].as_i64().unwrap(),
    );
    let placed: Vec<PlacedSlot> = armor_slots::armor_side_bar(-20, 11, 0);
    c.eq("layout", "quantos slots a barra põe", placed.len(), 5);
    for row in &rows[1..] {
        let index = row["index"].as_u64().unwrap() as usize;
        let ctx = format!("layout/{index}");
        c.hit("layout_row");
        let Some(slot) = index
            .checked_sub(FIRST_PLACED_SLOT)
            .and_then(|i| placed.get(i))
        else {
            // Os 40 primeiros não são da lib: confere só que não são de armadura e que a mão é a última.
            let class = row["class"].as_str().unwrap();
            c.ok(
                &ctx,
                "classe fora da coluna",
                class != "ArmorSlot" && class != "OffhandSlot",
            );
            c.ok(&ctx, "sem equipment_slot", row["equipment_slot"].is_null());
            continue;
        };
        c.hit("layout_placed");
        let (class, equipment, mapping) = match slot.kind {
            SlotKind::Armor(equip) => (
                "ArmorSlot",
                Some(equipment_name(equip)),
                equipment_name(equip),
            ),
            SlotKind::Offhand => ("OffhandSlot", None, "OFFHAND"),
        };
        c.eq(&ctx, "class", class, row["class"].as_str().unwrap());
        c.eq(&ctx, "x", slot.x, row["x"].as_i64().unwrap() as i32);
        c.eq(&ctx, "y", slot.y, row["y"].as_i64().unwrap() as i32);
        c.eq(
            &ctx,
            "container_slot",
            slot.inventory_index,
            row["container_slot"].as_u64().unwrap() as usize,
        );
        c.ok(
            &ctx,
            "in_player_inventory",
            row["in_player_inventory"].as_bool().unwrap(),
        );
        c.eq(
            &ctx,
            "max_stack",
            armor_slots::slot_max_stack_size(slot.kind),
            row["max_stack"].as_u64().unwrap() as u32,
        );
        c.eq(
            &ctx,
            "max_stack_for_dirt",
            armor_slots::max_stack_size(slot.kind, 64),
            row["max_stack_for_dirt"].as_u64().unwrap() as u32,
        );
        c.eq(
            &ctx,
            "max_stack_for_helmet",
            armor_slots::max_stack_size(slot.kind, 1),
            row["max_stack_for_helmet"].as_u64().unwrap() as u32,
        );
        c.eq(
            &ctx,
            "no_item_icon",
            Some(empty_background_icon(slot.background)),
            row["no_item_icon"].as_str(),
        );
        c.eq(
            &ctx,
            "equipment_slot",
            equipment,
            row["equipment_slot"].as_str(),
        );
        c.eq(
            &ctx,
            "inventory_equipment_mapping",
            Some(mapping),
            row["inventory_equipment_mapping"].as_str(),
        );
    }
}

// ---- 2. may_place -------------------------------------------------------------------------------------------------------------------

fn check_may_place(c: &mut Checker, rows: &[Value]) {
    for row in rows {
        let index = row["slot"].as_u64().unwrap() as usize;
        let kind = armor_kind(index).expect("o rastro só pergunta nos slots de armadura e na mão");
        let id = row["item"].as_str().unwrap();
        let item = trace_item(id);
        let ctx = label(row);
        c.hit("may_place");
        c.eq(
            &ctx,
            "kind",
            ["head", "chest", "legs", "feet", "offhand"][index - FIRST_PLACED_SLOT],
            row["kind"].as_str().unwrap(),
        );
        c.eq(
            &ctx,
            "may_place",
            armor_slots::may_place(kind, can_equip(item)),
            row["may_place"].as_bool().unwrap(),
        );
        if let Some(java) = row["max_stack_for_item"].as_u64() {
            c.hit("max_stack_for_item");
            c.eq(
                &ctx,
                "max_stack_for_item",
                armor_slots::max_stack_size(kind, item.max_stack),
                java as u32,
            );
        }
        // O tamanho da pilha do item, da tabela, também vem da mão: ela devolve o do item até 99.
        if kind == SlotKind::Offhand
            && let Some(java) = row["max_stack_for_item"].as_u64()
        {
            c.hit("item_table_vs_offhand");
            c.eq(&ctx, "tabela de itens: pilha", item.max_stack, java as u32);
        }
    }
}

// ---- 3. o menu do rastro ------------------------------------------------------------------------------------------------------------

/// O menu de 45 slots do Java, montado a partir do `layout` do rastro, com a regra de cada classe de slot por trás do `insert`.
struct TraceMenu {
    classes: Vec<String>,
    /// O `SlotKind` dos slots de armadura e da mão (os 5 últimos, na ordem de `armorSideBar`).
    kinds: Vec<Option<SlotKind>>,
    /// Para os `InventoryContainerSlot` da máquina: 0 é o de combustível, 1 o de energia (a ordem de `getInitialInventory`).
    tile_order: Vec<Option<usize>>,
    contents: Vec<Option<(String, u32)>>,
}

impl TraceMenu {
    fn new(classes: &[String]) -> Self {
        let mut placed = armor_slots::armor_side_bar(-20, 11, 0).into_iter();
        let mut tiles = 0;
        let mut kinds = Vec::new();
        let mut tile_order = Vec::new();
        for class in classes {
            let kind = matches!(class.as_str(), "ArmorSlot" | "OffhandSlot").then(|| {
                placed
                    .next()
                    .expect("mais slots de armadura que a barra")
                    .kind
            });
            kinds.push(kind);
            if class == "InventoryContainerSlot" {
                tile_order.push(Some(tiles));
                tiles += 1;
            } else {
                tile_order.push(None);
            }
        }
        assert!(placed.next().is_none(), "a barra tem mais slots que o menu");
        Self {
            classes: classes.to_vec(),
            kinds,
            tile_order,
            contents: vec![None; classes.len()],
        }
    }

    fn load(&mut self, list: &[Entry]) -> Result<(), String> {
        for (index, class, id, count) in list {
            if self.classes.get(*index) != Some(class) {
                return Err(format!(
                    "o slot {index} é {class} no rastro e {:?} no layout",
                    self.classes.get(*index)
                ));
            }
            self.contents[*index] = Some((id.clone(), *count));
        }
        Ok(())
    }

    fn snapshot(&self) -> Vec<Entry> {
        self.contents
            .iter()
            .enumerate()
            .filter_map(|(index, held)| {
                held.as_ref()
                    .map(|(id, count)| (index, self.classes[index].clone(), id.clone(), *count))
            })
            .collect()
    }
}

impl QuickMenu for TraceMenu {
    type Item = String;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        (0..self.classes.len())
            .filter(|slot| self.region(*slot) == region)
            .collect()
    }

    fn region(&self, slot: usize) -> Region {
        match self.classes[slot].as_str() {
            "VirtualInventoryContainerSlot" | "InventoryContainerSlot" => Region::Container,
            "MainInventorySlot" => Region::Main,
            "HotBarSlot" => Region::Hotbar,
            "ArmorSlot" => Region::Armor,
            "OffhandSlot" => Region::Offhand,
            other => panic!("classe de slot {other} fora do modelo"),
        }
    }

    /// O servidor não tem janela escolhida (`getSelectedWindow(player.getUUID())`), então os slots virtuais não existem.
    fn exists(&self, slot: usize) -> bool {
        self.classes[slot] != "VirtualInventoryContainerSlot"
    }

    fn stack(&self, slot: usize) -> Option<(String, u32)> {
        self.contents[slot].clone()
    }

    fn insert(&mut self, slot: usize, item: &String, amount: u32) -> u32 {
        let (held, same) = match &self.contents[slot] {
            Some((id, count)) => (*count, id == item),
            None => (0, true),
        };
        let facts = trace_item(item);
        let stackable = |accepts: bool| {
            if !accepts || held >= facts.max_stack || !same {
                0
            } else {
                amount.min(facts.max_stack - held)
            }
        };
        let moved = match self.classes[slot].as_str() {
            "InventoryContainerSlot" => {
                let fuel = self.tile_order[slot] == Some(0);
                stackable(fuel && FUEL_SLOT_ITEMS.contains(&base_id(item)))
            }
            "MainInventorySlot" | "HotBarSlot" => stackable(true),
            "ArmorSlot" | "OffhandSlot" => armor_slots::insert(
                self.kinds[slot].unwrap(),
                held,
                same,
                amount,
                facts.max_stack,
                can_equip(facts),
            ),
            other => panic!("insert num slot {other}"),
        };
        if moved > 0 {
            self.contents[slot] = Some((item.clone(), held + moved));
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some((item, count)) = self.contents[slot].clone() {
            self.contents[slot] = (count > amount).then_some((item, count - amount));
        }
    }
}

/// `Slot.mayPickup(player)` do slot do menu.
fn menu_may_pickup(menu: &TraceMenu, slot: usize, creative: bool) -> bool {
    match menu.kinds[slot] {
        Some(kind) => {
            let held = menu.contents[slot].as_ref();
            armor_slots::may_pickup(
                kind,
                held.is_none(),
                creative,
                held.is_some_and(|(id, _)| is_bound(id)),
            )
        }
        None => true,
    }
}

/// Uma chamada direta de `quickMoveStack`: o item e quantos saíram. Não olha `mayPickup`, como o original
/// (`MekanismContainer.java:276-330`; quem pergunta é o `doClick` do vanilla, antes de chamar).
fn direct_quick_move(menu: &mut TraceMenu, from: usize) -> Option<(String, u32)> {
    let (item, _) = menu.stack(from)?;
    quick_move(menu, from).map(|moved| (item, moved))
}

/// O shift-clique do vanilla (`AbstractContainerMenu.doClick`, `QUICK_MOVE`): sem `mayPickup` não faz nada, e repete o
/// `quickMoveStack` enquanto o slot ainda tiver o item que acabou de sair. Quem faz isso no jogo é o motor do Horizon; aqui só
/// fecha o estado final que o rastro grava.
fn shift_click(menu: &mut TraceMenu, from: usize, creative: bool) {
    if !menu_may_pickup(menu, from, creative) {
        return;
    }
    let mut moved = direct_quick_move(menu, from);
    while let Some((item, _)) = &moved {
        let still_there = menu.contents[from]
            .as_ref()
            .is_some_and(|(id, _)| base_id(id) == base_id(item));
        if !still_there {
            break;
        }
        moved = direct_quick_move(menu, from);
    }
}

fn layout_classes(layout: &[Value]) -> Vec<String> {
    let classes: Vec<String> = layout[1..]
        .iter()
        .enumerate()
        .map(|(position, row)| {
            assert_eq!(row["index"].as_u64().unwrap() as usize, position);
            row["class"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(classes.len(), 45);
    classes
}

/// Um `quickMoveStack` direto ou um shift-clique do rastro: `returned` (nos diretos) e o estado de todos os slots depois.
fn check_move_row(
    c: &mut Checker,
    classes: &[String],
    row: &Value,
    mode: &str,
    group: &'static str,
) {
    let ctx = label(row);
    let creative = row["session"].as_str() == Some("creative");
    let from = row["from"].as_u64().or(row["slot"].as_u64()).unwrap() as usize;
    let mut menu = TraceMenu::new(classes);
    if let Err(message) = menu.load(&entries(&row["before"])) {
        c.failures.push(format!("{ctx}: before: {message}"));
        return;
    }
    c.hit(group);
    match mode {
        "direct" => {
            c.hit("direct");
            let returned = direct_quick_move(&mut menu, from);
            c.eq(&ctx, "returned", returned, stack_of(&row["returned"]));
        }
        "shift" => {
            c.hit("shift");
            shift_click(&mut menu, from, creative);
        }
        other => panic!("modo {other}"),
    }
    let (rust, java) = (menu.snapshot(), entries(&row["slots"]));
    if rust != java {
        // Só os slots que diferem: o cenário de inventário cheio tem 36 pedras que não dizem nada.
        let only = |a: &[Entry], b: &[Entry]| {
            a.iter()
                .filter(|e| !b.contains(e))
                .cloned()
                .collect::<Vec<_>>()
        };
        c.failures.push(format!(
            "{ctx}: slots depois: só no porte {:?}, só no Java {:?}",
            only(&rust, &java),
            only(&java, &rust)
        ));
    }
}

// ---- 4. quick_move e binding --------------------------------------------------------------------------------------------------------

fn check_quick_move(c: &mut Checker, classes: &[String], rows: &[Value]) {
    let mut previous_slots: Option<&Value> = None;
    for row in rows {
        match row["group"].as_str().unwrap() {
            "quick_move" => {
                check_move_row(c, classes, row, row["mode"].as_str().unwrap(), "quick_move");
            }
            // O shift-clique que o rastro segue por dois ticks: o estado não muda nos ticks seguintes (sons e eventos são do jogo).
            "quick_move_then_ticks" if row.get("from").is_some() => {
                check_move_row(c, classes, row, "shift", "quick_move_then_ticks");
                previous_slots = Some(&row["slots"]);
            }
            "quick_move_then_ticks" => {
                c.hit("two_ticks_after");
                c.eq(
                    &label(row),
                    "slots dois ticks depois",
                    Some(&row["slots"]),
                    previous_slots,
                );
            }
            other => panic!("grupo {other}"),
        }
    }
}

fn check_binding(c: &mut Checker, classes: &[String], rows: &[Value]) {
    for row in rows {
        let ctx = label(row);
        let creative = row["session"].as_str() == Some("creative");
        let case = row["case"].as_str().unwrap();
        let index = row["slot"].as_u64().unwrap() as usize;
        let kind = armor_kind(index).unwrap();
        match case {
            "may_pickup_cursed" | "may_pickup_plain" | "may_pickup_empty" => {
                c.hit("may_pickup");
                c.eq(
                    &ctx,
                    "creative",
                    creative,
                    row["creative"].as_bool().unwrap(),
                );
                let rust = armor_slots::may_pickup(
                    kind,
                    case == "may_pickup_empty",
                    creative,
                    case == "may_pickup_cursed",
                );
                c.eq(
                    &ctx,
                    "may_pickup",
                    rust,
                    row["may_pickup"].as_bool().unwrap(),
                );
            }
            _ => match row["how"].as_str().unwrap() {
                // `shift_click_out*`: o `doClick` pergunta o `mayPickup` e depois chama o `quickMoveStack`.
                "shift" => check_move_row(c, classes, row, "shift", "binding_move"),
                // `quick_move_stack_out`: a chamada direta ignora o `mayPickup`, e a peça com maldição sai do slot em sobrevivência.
                "direct" => {
                    check_move_row(c, classes, row, "direct", "binding_move");
                    if !creative && case == "quick_move_stack_out" {
                        c.hit("direct_ignores_may_pickup");
                        let before = entries(&row["before"]);
                        let cursed = at(&before, index).is_some_and(|(id, _)| is_bound(&id));
                        c.ok(&ctx, "o caso tem a peça com maldição", cursed);
                        c.ok(
                            &ctx,
                            "a peça com maldição saiu",
                            at(&entries(&row["slots"]), index).is_none(),
                        );
                    }
                }
                _ => check_click_row(c, row),
            },
        }
    }
}

// ---- 5. cliques com cursor ----------------------------------------------------------------------------------------------------------

/// Do clique com cursor, a tecla de número, o arrasto e o clique duplo o núcleo não tem o `doClick` (é do motor do Horizon). Aqui
/// vai só o que ele modela, e cada conferência vale para o slot clicado e para os outros slots de armadura da linha:
/// 1. o tamanho máximo: nenhum slot de armadura ou da mão passa de `max_stack_size`;
/// 2. `may_place`: um item que chegou ao slot é aceito por ele;
/// 3. `may_pickup`: um item que saiu do slot podia sair, e um que não podia deixa o menu inteiro e o cursor como estavam;
/// 4. `insert` (`Slot.safeInsert`, cursor sobre slot vazio ou com o mesmo item): a quantidade que entra, e o que sobra no cursor;
/// 5. a tecla de número sobre slot vazio (`min(quantidade, getMaxStackSize(item))` quando `mayPlace`).
fn check_click_row(c: &mut Checker, row: &Value) {
    let ctx = label(row);
    let creative = row["session"].as_str() == Some("creative");
    let how = row["how"].as_str().unwrap();
    let clicked = row["slot"].as_u64().unwrap() as usize;
    let before = entries(&row["before"]);
    let after = entries(&row["slots"]);
    let carried_before = stack_of(&row["carried_before"]);
    c.hit("click");
    for index in FIRST_PLACED_SLOT..=LAST_PLACED_SLOT {
        let kind = armor_kind(index).unwrap();
        let was = at(&before, index);
        let now = at(&after, index);
        if let Some((id, count)) = &now {
            let item = trace_item(id);
            c.hit("click_max_stack");
            c.ok(
                &ctx,
                &format!("slot {index} passou do tamanho máximo ({count})"),
                *count <= armor_slots::max_stack_size(kind, item.max_stack),
            );
            if was.as_ref().map(|(was_id, _)| was_id) != Some(id) {
                c.hit("click_may_place");
                c.ok(
                    &ctx,
                    &format!("slot {index} recebeu {id} que may_place recusa"),
                    armor_slots::may_place(kind, can_equip(item)),
                );
            }
        }
        if let Some((was_id, was_count)) = &was {
            let left = match &now {
                None => true,
                Some((id, count)) => id != was_id || count < was_count,
            };
            if left {
                c.hit("click_may_pickup");
                c.ok(
                    &ctx,
                    &format!("slot {index} soltou {was_id} que may_pickup recusa"),
                    armor_slots::may_pickup(kind, false, creative, is_bound(was_id)),
                );
            }
        }
    }
    let kind = armor_kind(clicked).unwrap();
    let was = at(&before, clicked);
    let now = at(&after, clicked);
    if let Some((was_id, _)) = &was
        && !armor_slots::may_pickup(kind, false, creative, is_bound(was_id))
    {
        c.hit("click_blocked");
        c.eq(&ctx, "peça presa: slots", &row["slots"], &row["before"]);
        c.eq(
            &ctx,
            "peça presa: cursor",
            stack_of(&row["carried"]),
            carried_before.clone(),
        );
        return;
    }
    let placed_now = now.as_ref().map_or(0, |(_, count)| *count);
    match (how, &was, &carried_before) {
        ("left" | "right", None, Some((id, count))) => {
            c.hit("click_insert_empty");
            let item = trace_item(id);
            let increment = if how == "left" { *count } else { 1 };
            let moved = armor_slots::insert(
                kind,
                0,
                false,
                increment.min(*count),
                item.max_stack,
                can_equip(item),
            );
            c.eq(&ctx, "quantos entraram", moved, placed_now);
            c.eq(
                &ctx,
                "item que entrou",
                now.as_ref().map(|(i, _)| i.as_str()).filter(|_| moved > 0),
                (moved > 0).then_some(id.as_str()),
            );
            c.eq(
                &ctx,
                "o que sobra no cursor",
                stack_of(&row["carried"]),
                (count - moved > 0).then(|| (id.clone(), count - moved)),
            );
        }
        ("left" | "right", Some((was_id, held)), Some((id, count)))
            if was_id == id && armor_slots::may_place(kind, can_equip(trace_item(id))) =>
        {
            c.hit("click_insert_same");
            let item = trace_item(id);
            let increment = if how == "left" { *count } else { 1 };
            let moved = armor_slots::insert(
                kind,
                *held,
                true,
                increment.min(*count),
                item.max_stack,
                can_equip(item),
            );
            c.eq(&ctx, "quantos ficaram", held + moved, placed_now);
            c.eq(
                &ctx,
                "o que sobra no cursor",
                stack_of(&row["carried"]),
                (count - moved > 0).then(|| (id.clone(), count - moved)),
            );
        }
        ("swap0", None, None) => {
            if let Some((id, count)) = at(&before, HOTBAR_ZERO_SLOT) {
                c.hit("click_number_key_empty");
                let item = trace_item(&id);
                let moved = if armor_slots::may_place(kind, can_equip(item)) {
                    count.min(armor_slots::max_stack_size(kind, item.max_stack))
                } else {
                    0
                };
                c.eq(&ctx, "tecla de número: quantos entraram", moved, placed_now);
                c.eq(
                    &ctx,
                    "tecla de número: o que sobra na barra",
                    at(&after, HOTBAR_ZERO_SLOT).map_or(0, |(_, left)| left),
                    count - moved,
                );
            }
        }
        _ => {}
    }
}

fn check_clicks(c: &mut Checker, rows: &[Value]) {
    for row in rows {
        c.hit("clicks_row");
        check_click_row(c, row);
    }
}

/// Repete `dev/referencia/rastros/heat_generator_armor_*.jsonl` (gravados do Java por `HeatArmorTraceTest`) contra as regras
/// puras do núcleo: o layout da coluna, `may_place`, `may_pickup`, o tamanho máximo, o `insert` e o `quick_move` com o estado de
/// todos os slots depois. O que o núcleo não modela (`doClick` com cursor, arrasto, clique duplo, som e evento de jogo) fica
/// para o teste em jogo.
#[test]
fn the_armor_slots_follow_the_original_trace() {
    let started = std::time::Instant::now();
    let layout = read_rows("layout");
    let may_place = read_rows("may_place");
    let clicks = read_rows("clicks");
    let binding = read_rows("binding");
    let quick = read_rows("quick_move");
    let read_in = started.elapsed();
    for (group, rows) in [
        ("layout", &layout),
        ("may_place", &may_place),
        ("clicks", &clicks),
        ("binding", &binding),
        ("quick_move", &quick),
    ] {
        let minimum = MIN_ROWS.iter().find(|(name, _)| *name == group).unwrap().1;
        assert!(
            rows.len() >= minimum,
            "{group}: {} linhas, esperava ao menos {minimum}",
            rows.len()
        );
    }
    let classes = layout_classes(&layout);
    let mut c = Checker::default();
    check_layout(&mut c, &layout);
    check_may_place(&mut c, &may_place);
    check_clicks(&mut c, &clicks);
    check_binding(&mut c, &classes, &binding);
    check_quick_move(&mut c, &classes, &quick);
    eprintln!(
        "rastro: leitura e parse de {} linhas em {read_in:?}, tudo em {:?}; conferências: {:?}",
        layout.len() + may_place.len() + clicks.len() + binding.len() + quick.len(),
        started.elapsed(),
        c.hits
    );
    // Uma conferência que não rodou não prova nada: cada uma precisa ter rodado ao menos o tanto abaixo.
    for (what, minimum) in [
        ("layout_placed", 5),
        ("may_place", 100),
        ("max_stack_for_item", 80),
        ("item_table_vs_offhand", 15),
        ("clicks_row", 120),
        ("click_max_stack", 100),
        ("click_may_place", 50),
        ("click_may_pickup", 30),
        ("click_blocked", 20),
        ("click_insert_empty", 40),
        ("click_insert_same", 4),
        ("click_number_key_empty", 6),
        ("may_pickup", 24),
        ("direct_ignores_may_pickup", 4),
        ("direct", 150),
        ("shift", 150),
        ("quick_move", 350),
        ("binding_move", 24),
        ("two_ticks_after", 1),
    ] {
        let ran = c.hits.get(what).copied().unwrap_or(0);
        c.ok(
            "cobertura",
            &format!("{what}: rodou {ran} vezes, esperava ao menos {minimum}"),
            ran >= minimum,
        );
    }
    assert!(
        c.failures.is_empty(),
        "{} divergências do rastro do Java:\n{}",
        c.failures.len(),
        c.failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
