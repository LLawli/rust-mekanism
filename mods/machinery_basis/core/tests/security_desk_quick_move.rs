//! O shift-clique na tela da Security Desk: `MekanismContainer.quickMoveStack` (`:274-328`) sobre os dois slots do tile, com a regra de cada um
//! (`SecurityInventorySlot.canInsert` com `MANUAL`, conferida contra o rastro em `security_desk_reference`). Cada caso foi seguido no Java:
//! - do inventário, `insertItem(inventoryContainerSlots, ..., true)` (só slots com item) e depois `false` (só os vazios), na ordem de
//!   `getInitialInventory`: o de destravar (0) antes do de travar (1); o item do dono da mesa entra no 0, o sem dono no 1;
//! - o que os dois recusam (o item de outro dono, o item que não pode ter dono) vai para a outra região do inventário (`:307-313`);
//! - de um slot da mesa, a barra de atalhos e depois o inventário principal (`:295`), empilhando primeiro.
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use machinery_basis_core::security_desk::{DeskSlot, ItemSecurity, manual_accepts};

const DESK_OWNER: u128 = 7;
const STRANGER: u128 = 9;
const FIRST_PLAYER: usize = 2;
const LIMIT: u32 = 64;

/// Um item do teste: o id, se pode ter dono e o dono.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    id: &'static str,
    ownable: bool,
    owner: Option<u128>,
}

fn machine(owner: Option<u128>) -> Item {
    Item {
        id: "mekanism:crusher",
        ownable: true,
        owner,
    }
}

fn stone() -> Item {
    Item {
        id: "minecraft:stone",
        ownable: false,
        owner: None,
    }
}

/// O menu da mesa: os slots 0 e 1 do tile, a barra de atalhos (2 a 10) e o inventário principal (11 a 37).
struct DeskMenu {
    cells: Vec<Option<(Item, u32)>>,
    desk_owner: Option<u128>,
}

impl DeskMenu {
    fn new() -> DeskMenu {
        DeskMenu {
            cells: vec![None; FIRST_PLAYER + 36],
            desk_owner: Some(DESK_OWNER),
        }
    }

    fn with(mut self, slot: usize, item: Item, count: u32) -> DeskMenu {
        self.cells[slot] = Some((item, count));
        self
    }

    fn filled(&self) -> Vec<(usize, &'static str, u32)> {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(slot, cell)| cell.as_ref().map(|(item, count)| (slot, item.id, *count)))
            .collect()
    }
}

impl QuickMenu for DeskMenu {
    type Item = Item;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        match region {
            Region::Container => (0..DeskSlot::ORDER.len()).collect(),
            Region::Hotbar => (FIRST_PLAYER..FIRST_PLAYER + 9).collect(),
            Region::Main => (FIRST_PLAYER + 9..FIRST_PLAYER + 36).collect(),
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        if slot < FIRST_PLAYER {
            Region::Container
        } else if slot - FIRST_PLAYER < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    fn exists(&self, _: usize) -> bool {
        true
    }

    fn stack(&self, slot: usize) -> Option<(Item, u32)> {
        self.cells[slot].clone()
    }

    fn insert(&mut self, slot: usize, item: &Item, amount: u32) -> u32 {
        if slot < FIRST_PLAYER {
            let seen = ItemSecurity {
                ownable: item.ownable,
                securable: item.ownable,
                owner: item.owner,
                mode: None,
            };
            if !manual_accepts(slot, &seen, self.desk_owner) {
                return 0;
            }
        }
        match &mut self.cells[slot] {
            Some((held, count)) if held == item => {
                let moved = amount.min(LIMIT - *count);
                *count += moved;
                moved
            }
            Some(_) => 0,
            cell => {
                *cell = Some((item.clone(), amount.min(LIMIT)));
                amount.min(LIMIT)
            }
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some((_, count)) = &mut self.cells[slot] {
            *count -= amount;
            if *count == 0 {
                self.cells[slot] = None;
            }
        }
    }
}

const HOTBAR_0: usize = FIRST_PLAYER;
const MAIN_0: usize = FIRST_PLAYER + 9;

#[test]
fn the_unlock_slot_comes_before_the_lock_slot() {
    assert_eq!(DeskSlot::ORDER, [DeskSlot::Unlock, DeskSlot::Lock]);
    assert_eq!(DeskSlot::of_index(0), Some(DeskSlot::Unlock));
    assert_eq!(DeskSlot::of_index(1), Some(DeskSlot::Lock));
    assert_eq!(DeskSlot::of_index(2), None);
    // Um índice que não é dos dois slots (os virtuais que a janela põe no fim do tile) não aceita nada.
    let unowned = ItemSecurity {
        ownable: true,
        securable: true,
        owner: None,
        mode: None,
    };
    assert!(manual_accepts(1, &unowned, Some(DESK_OWNER)));
    assert!(!manual_accepts(2, &unowned, Some(DESK_OWNER)));
}

#[test]
fn an_unowned_item_goes_to_the_lock_slot_and_the_owners_item_to_the_unlock_slot() {
    let mut menu = DeskMenu::new().with(HOTBAR_0, machine(None), 1);
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(1));
    assert_eq!(menu.filled(), vec![(1, "mekanism:crusher", 1)]);

    let mut menu = DeskMenu::new().with(MAIN_0, machine(Some(DESK_OWNER)), 1);
    assert_eq!(quick_move(&mut menu, MAIN_0), Some(1));
    assert_eq!(menu.filled(), vec![(0, "mekanism:crusher", 1)]);
}

#[test]
fn a_whole_stack_of_unowned_items_enters_the_lock_slot() {
    let mut menu = DeskMenu::new().with(HOTBAR_0, machine(None), 5);
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(5));
    assert_eq!(menu.filled(), vec![(1, "mekanism:crusher", 5)]);
}

#[test]
fn the_first_pass_stacks_on_the_slot_that_already_holds_the_item() {
    // O de travar já tem dois sem dono (o tick ainda não os travou): a primeira passada, só nos slots com item, empilha ali.
    let mut menu = DeskMenu::new()
        .with(1, machine(None), 2)
        .with(HOTBAR_0, machine(None), 3);
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(3));
    assert_eq!(menu.filled(), vec![(1, "mekanism:crusher", 5)]);
    // O de travar ocupado por um item já travado (outro recurso): o sem dono não tem para onde ir na mesa, porque o de destravar o recusa; vai para o
    // inventário principal (`:312`).
    let mut menu =
        DeskMenu::new()
            .with(1, machine(Some(DESK_OWNER)), 1)
            .with(HOTBAR_0, machine(None), 1);
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(1));
    assert_eq!(
        menu.filled(),
        vec![(1, "mekanism:crusher", 1), (MAIN_0, "mekanism:crusher", 1)]
    );
}

#[test]
fn what_both_slots_refuse_goes_to_the_other_part_of_the_inventory() {
    // O item de outro dono: o de destravar quer o dono da mesa, o de travar quer o item sem dono.
    let mut menu = DeskMenu::new().with(HOTBAR_0, machine(Some(STRANGER)), 1);
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(1));
    assert_eq!(menu.filled(), vec![(MAIN_0, "mekanism:crusher", 1)]);
    // O item que não pode ter dono (`SecurityInventorySlot.VALIDATOR`), do inventário principal para a barra.
    let mut menu = DeskMenu::new().with(MAIN_0, stone(), 64);
    assert_eq!(quick_move(&mut menu, MAIN_0), Some(64));
    assert_eq!(menu.filled(), vec![(HOTBAR_0, "minecraft:stone", 64)]);
    // A mesa sem dono: o de destravar não aceita ninguém (`ownerUUID.equals(null)`), o de travar segue aceitando o sem dono.
    let mut menu = DeskMenu::new().with(HOTBAR_0, machine(Some(DESK_OWNER)), 1);
    menu.desk_owner = None;
    assert_eq!(quick_move(&mut menu, HOTBAR_0), Some(1));
    assert_eq!(menu.filled(), vec![(MAIN_0, "mekanism:crusher", 1)]);
}

#[test]
fn from_the_desk_the_item_goes_to_the_hotbar_and_then_to_the_main_inventory() {
    let mut menu = DeskMenu::new().with(1, machine(Some(DESK_OWNER)), 1);
    assert_eq!(quick_move(&mut menu, 1), Some(1));
    assert_eq!(menu.filled(), vec![(HOTBAR_0, "mekanism:crusher", 1)]);
    // Com a barra cheia de outra coisa, o inventário principal; e empilha antes de usar um slot vazio.
    let mut menu = DeskMenu::new()
        .with(0, machine(None), 1)
        .with(MAIN_0 + 3, machine(None), 4);
    for cell in &mut menu.cells[HOTBAR_0..HOTBAR_0 + 9] {
        *cell = Some((stone(), 64));
    }
    assert_eq!(quick_move(&mut menu, 0), Some(1));
    assert_eq!(menu.cells[MAIN_0 + 3], Some((machine(None), 5)));
    assert_eq!(menu.cells[0], None);
    // Sem lugar nenhum nada se move.
    let mut menu = DeskMenu::new().with(0, machine(None), 1);
    for cell in &mut menu.cells[FIRST_PLAYER..FIRST_PLAYER + 36] {
        *cell = Some((stone(), 64));
    }
    assert_eq!(quick_move(&mut menu, 0), None);
    assert_eq!(menu.cells[0], Some((machine(None), 1)));
}
