//! O menu de um tanque (`MekanismContainerTypes.FLUID_TANK` e `CHEMICAL_TANK`, com `armorSideBar()`) como o rastro `tank_quick_move_*.jsonl` o grava, para repetir
//! o `MekanismContainer.quickMoveStack` do original caso a caso com `machinery_basis_core::quick_move`. A primeira linha do rastro é o desenho do menu (a classe e o
//! índice de inventário de cada slot); cada caso traz os slots antes e depois. Quem usa diz o que cada slot do tile aceita (`isValidForInsertion(.., MANUAL)`).

use machinery_basis_core::armor_slots::{self, EquipmentSlot, SlotKind};
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use serde_json::Value;

/// Um item do rastro: o id (com `{recurso:quantidade}` no item de tanque cheio) e o tamanho da pilha que o Java deu (`getMaxStackSize`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub max_stack: u32,
}

/// `BasicInventorySlot.DEFAULT_LIMIT`: o slot do tile guarda até 64, ou a pilha do item se for menor.
const TILE_SLOT_LIMIT: u32 = 64;

pub struct Menu<'a> {
    classes: Vec<String>,
    inventory: Vec<i64>,
    contents: Vec<Option<(Item, u32)>>,
    /// O slot do tile (0 ou 1, na ordem do menu) aceita o item.
    accepts: &'a dyn Fn(usize, &Item) -> bool,
}

/// `Inventory`: 36 a 39 são os pés, as pernas, o peito e a cabeça.
fn armor_of(inventory: i64) -> EquipmentSlot {
    match inventory {
        36 => EquipmentSlot::Feet,
        37 => EquipmentSlot::Legs,
        38 => EquipmentSlot::Chest,
        39 => EquipmentSlot::Head,
        other => panic!("slot de armadura {other}"),
    }
}

/// `ItemStack.canEquip` dos itens do rastro.
fn equips(item: &Item, slot: EquipmentSlot) -> bool {
    let wanted = match slot {
        EquipmentSlot::Head => "_helmet",
        EquipmentSlot::Chest => "_chestplate",
        EquipmentSlot::Legs => "_leggings",
        EquipmentSlot::Feet => "_boots",
    };
    item.id.ends_with(wanted)
}

impl<'a> Menu<'a> {
    pub fn new(layout: &Value, accepts: &'a dyn Fn(usize, &Item) -> bool) -> Self {
        let slots = layout["slots"].as_array().unwrap();
        Menu {
            classes: slots.iter().map(|slot| slot[1].as_str().unwrap().to_owned()).collect(),
            inventory: slots.iter().map(|slot| slot[2].as_i64().unwrap()).collect(),
            contents: vec![None; slots.len()],
            accepts,
        }
    }

    fn of_class(&self, class: &str) -> Vec<usize> {
        (0..self.classes.len()).filter(|&slot| self.classes[slot] == class).collect()
    }

    /// Os slots de uma lista `[[índice, id, quantidade, pilha], ..]` do rastro.
    pub fn load(&mut self, slots: &Value) {
        self.contents.iter_mut().for_each(|slot| *slot = None);
        for entry in slots.as_array().unwrap() {
            let item = Item { id: entry[1].as_str().unwrap().to_owned(), max_stack: entry[3].as_u64().unwrap() as u32 };
            self.contents[entry[0].as_u64().unwrap() as usize] = Some((item, entry[2].as_u64().unwrap() as u32));
        }
    }

    pub fn dump(&self) -> Vec<(usize, String, u32)> {
        self.contents
            .iter()
            .enumerate()
            .filter_map(|(slot, held)| held.as_ref().map(|(item, count)| (slot, item.id.clone(), *count)))
            .collect()
    }
}

pub fn recorded(slots: &Value) -> Vec<(usize, String, u32)> {
    slots
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry[0].as_u64().unwrap() as usize, entry[1].as_str().unwrap().to_owned(), entry[2].as_u64().unwrap() as u32))
        .collect()
}

impl QuickMenu for Menu<'_> {
    type Item = Item;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        self.of_class(match region {
            Region::Container => "InventoryContainerSlot",
            Region::Armor => "ArmorSlot",
            Region::Main => "MainInventorySlot",
            Region::Hotbar => "HotBarSlot",
            Region::Offhand => "OffhandSlot",
        })
    }

    fn region(&self, slot: usize) -> Region {
        match self.classes[slot].as_str() {
            "InventoryContainerSlot" => Region::Container,
            "ArmorSlot" => Region::Armor,
            "MainInventorySlot" => Region::Main,
            "HotBarSlot" => Region::Hotbar,
            "OffhandSlot" => Region::Offhand,
            other => panic!("classe de slot {other}"),
        }
    }

    fn exists(&self, _slot: usize) -> bool {
        true
    }

    fn stack(&self, slot: usize) -> Option<(Item, u32)> {
        self.contents[slot].clone()
    }

    fn insert(&mut self, slot: usize, item: &Item, amount: u32) -> u32 {
        let held = self.contents[slot].as_ref().map_or(0, |(_, count)| *count);
        let same = self.contents[slot].as_ref().is_none_or(|(held_item, _)| held_item == item);
        let plain = |limit: u32| if held >= limit || !same { 0 } else { amount.min(limit - held) };
        let moved = match self.region(slot) {
            Region::Container => {
                let index = self.of_class("InventoryContainerSlot").iter().position(|&other| other == slot).unwrap();
                if (self.accepts)(index, item) { plain(item.max_stack.min(TILE_SLOT_LIMIT)) } else { 0 }
            }
            Region::Main | Region::Hotbar => plain(item.max_stack),
            Region::Armor => {
                let kind = armor_of(self.inventory[slot]);
                armor_slots::insert(SlotKind::Armor(kind), held, same, amount, item.max_stack, |equip| equips(item, equip))
            }
            Region::Offhand => armor_slots::insert(SlotKind::Offhand, held, same, amount, item.max_stack, |_| false),
        };
        if moved > 0 {
            self.contents[slot] = Some((item.clone(), held + moved));
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some((item, count)) = self.contents[slot].take()
            && count > amount
        {
            self.contents[slot] = Some((item, count - amount));
        }
    }
}

/// Repete os casos de um rastro: `accepts_of(caso)` devolve a regra dos dois slots do tile para o tanque daquele caso. Devolve quantos casos conferiu.
pub fn replay(rows: &[Value], accepts_of: &dyn Fn(&Value) -> Box<dyn Fn(usize, &Item) -> bool>) -> usize {
    let layout = &rows[0];
    assert_eq!(layout["group"], "layout");
    let mut checked = 0;
    for row in &rows[1..] {
        let accepts = accepts_of(row);
        let mut menu = Menu::new(layout, &*accepts);
        menu.load(&row["before"]);
        let from = row["from"].as_u64().unwrap() as usize;
        let at = format!(
            "{} {} {} tanque {} slots [{}] [{}] inventário {} armadura {}",
            row["mode"], row["source"], row["item"], row["tank"], row["slot0"], row["slot1"], row["inventory"], row["armor"]
        );
        if row["mode"] == "direct" {
            let source = menu.stack(from);
            let moved = quick_move(&mut menu, from);
            // `transferSuccess` devolve o que saiu do slot; `ItemStack.EMPTY` se nada se moveu.
            let returned = match (moved, source) {
                (Some(moved), Some((item, _))) => (item.id, u64::from(moved)),
                _ => (String::new(), 0),
            };
            assert_eq!(returned, (row["returned"][0].as_str().unwrap().to_owned(), row["returned"][1].as_u64().unwrap()), "{at}: devolvido");
        } else {
            // `AbstractContainerMenu.doClick` com `QUICK_MOVE`: repete enquanto algo saiu e o slot ainda tem o mesmo item.
            while quick_move(&mut menu, from).is_some() && menu.stack(from).is_some() {}
        }
        assert_eq!(menu.dump(), recorded(&row["after"]), "{at}");
        checked += 1;
    }
    checked
}
