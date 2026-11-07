//! O shift-clique num menu com a coluna de armadura e a mão (o do Heat Generator: `armorSideBar(-20, 11, 0)`), com as regras reais dos
//! slots (`armor_slots::insert`) por trás do `quick_move`. Cada valor esperado sai de seguir `MekanismContainer.quickMoveStack` e
//! `insertItem` (`MekanismContainer.java:276-382`) à mão, linha a linha, e o motivo vem junto do caso.
//!
//! O fim do arquivo tem o teste do rastro do Java (`dev/referencia/rastros/heat_generator_armor_*.jsonl`), desligado até a
//! outra frente gravá-lo.

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

/// O item pelo id que o rastro grava.
#[allow(dead_code)]
fn item_named(id: &str) -> Item {
    [
        IRON_HELMET,
        DIAMOND_HELMET,
        IRON_CHESTPLATE,
        IRON_BOOTS,
        STONE,
        COAL,
        PEARL,
        TOTEM,
    ]
    .into_iter()
    .find(|item| item.id == id)
    .unwrap_or_else(|| panic!("o item {id:?} do rastro não está na tabela do teste"))
}

/// Repete `dev/referencia/rastros/heat_generator_armor_*.jsonl` contra o porte. O rastro segue o formato do
/// `heat_generator_upgrade_slot_quick_move.jsonl`: a primeira linha traz o `layout` (índice e classe de cada slot do menu do Java),
/// e cada passo seguinte traz `from`, `returned` e os `slots` com algo. O que o cenário em Java pôs no menu antes de cada passo só
/// está nas `marks`, então a tabela `setup` abaixo se liga ao rastro quando ele existir, como em `upgrade_window.rs`.
#[test]
#[ignore = "rastro do Java ainda não gravado"]
fn the_armor_slots_follow_the_original_trace() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("heat_generator_armor_") && name.ends_with(".jsonl")
                })
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "nenhum heat_generator_armor_*.jsonl em {}",
        dir.display()
    );
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let classes: Vec<&str> = rows[0]["layout"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry[1].as_str().unwrap())
            .collect();
        // O layout do Java: a coluna de armadura tem quatro `ArmorSlot` seguidos e uma `OffhandSlot` logo depois.
        let first_armor = classes
            .iter()
            .position(|class| *class == "ArmorSlot")
            .expect("o rastro não tem ArmorSlot");
        assert_eq!(
            &classes[first_armor..first_armor + 5],
            [
                "ArmorSlot",
                "ArmorSlot",
                "ArmorSlot",
                "ArmorSlot",
                "OffhandSlot"
            ],
            "{}",
            file.display()
        );
        // Ainda sem o setup de cada passo: o teste para no primeiro, dizendo o que falta ligar.
        if let Some(row) = rows.get(1) {
            let marks = row["marks"].as_array().unwrap();
            panic!(
                "{}: ligar o cenário do passo 1 ({marks:?}) e dos seguintes ao menu do teste e comparar `returned` e `slots`",
                file.display()
            );
        }
    }
}
