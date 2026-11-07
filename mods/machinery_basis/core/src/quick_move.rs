//! `MekanismContainer.quickMoveStack` e `insertItem`: para onde o shift-clique leva um item. A lib não conhece slot do jogo: quem
//! usa implementa [`QuickMenu`] sobre os slots do menu e diz, para cada região, em que ordem o Java os guarda.
//!
//! A ordem é parte do comportamento: os slots virtuais da janela de upgrades entram no Java ANTES dos da máquina em
//! `inventoryContainerSlots`, e a prioridade do shift-clique vem daí. No Horizon eles ficam no fim do menu (para não deslocar
//! o NBT de mundos antigos), então a ordem do Java é dita por [`QuickMenu::slots_of`], não pelo índice.

/// De que grupo um slot é (`inventoryContainerSlots`, `armorSlots`, `mainInventorySlots`, `hotBarSlots`, `offhandSlots`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// `InventoryContainerSlot`: os da máquina e os virtuais de uma janela.
    Container,
    Armor,
    Main,
    Hotbar,
    Offhand,
}

/// O menu visto pelo `quickMoveStack`.
pub trait QuickMenu {
    type Item: Clone;

    /// Os slots de uma região, na ordem em que o Java os adicionou.
    fn slots_of(&self, region: Region) -> Vec<usize>;

    /// A região de um slot.
    fn region(&self, slot: usize) -> Region;

    /// `ITransactionalSlot.exists(selectedWindow)`: um slot virtual só existe com a janela dele escolhida por quem clica.
    fn exists(&self, slot: usize) -> bool;

    /// O que o slot guarda (`hasItem`).
    fn stack(&self, slot: usize) -> Option<(Self::Item, u32)>;

    /// `slot.insert(resource, amount, transaction)`: quantos entraram. Segue a regra do slot (aceita ou não, o limite).
    fn insert(&mut self, slot: usize, item: &Self::Item, amount: u32) -> u32;

    /// `slot.remove(amount)`: tira do slot de origem o que saiu.
    fn remove(&mut self, slot: usize, amount: u32);
}

/// `insertItem(slots, itemType, amount, transaction, ignoreEmpty)`: uma passada. Com `ignore_empty`, só entra em slot que já tem item.
fn insert_pass<M: QuickMenu>(
    menu: &mut M,
    slots: &[usize],
    item: &M::Item,
    amount: u32,
    ignore_empty: bool,
) -> u32 {
    let mut inserted = 0;
    if amount == 0 {
        return 0;
    }
    for &slot in slots {
        if ignore_empty != menu.stack(slot).is_some() {
            continue;
        }
        if !menu.exists(slot) {
            continue;
        }
        inserted += menu.insert(slot, item, amount - inserted);
        if inserted == amount {
            break;
        }
    }
    inserted
}

/// `insertItem(slots, itemType, amount, transaction)`: primeiro empilha nos que têm item, depois usa os vazios.
fn insert_all<M: QuickMenu>(menu: &mut M, slots: &[usize], item: &M::Item, amount: u32) -> u32 {
    let mut inserted = insert_pass(menu, slots, item, amount, true);
    inserted += insert_pass(menu, slots, item, amount - inserted, false);
    inserted
}

/// `quickMoveStack(player, slotID)`: quantos itens saíram do slot, ou `None` quando nada se moveu (o `ItemStack.EMPTY` do Java).
/// O que sai de `slot` já foi tirado dele quando a função volta.
pub fn quick_move<M: QuickMenu>(menu: &mut M, slot: usize) -> Option<u32> {
    let (item, amount) = menu.stack(slot)?;
    if !menu.exists(slot) {
        return None;
    }
    let region = menu.region(slot);
    let containers = menu.slots_of(Region::Container);
    let armor = menu.slots_of(Region::Armor);
    let main = menu.slots_of(Region::Main);
    let hotbar = menu.slots_of(Region::Hotbar);
    let inserted = if region == Region::Container {
        // Do contêiner para o inventário: armadura, barra de atalhos, inventário principal.
        let order: Vec<usize> = armor.iter().chain(&hotbar).chain(&main).copied().collect();
        insert_all(menu, &order, &item, amount)
    } else {
        // Do inventário: primeiro empilha nos slots do contêiner, depois usa os vazios deles.
        let mut inserted = insert_pass(menu, &containers, &item, amount, true);
        if inserted == 0 {
            inserted = insert_pass(menu, &containers, &item, amount, false);
            if inserted == 0 {
                match region {
                    Region::Armor | Region::Offhand => {
                        // `insertItem(playerSlots, ...)` (`MekanismContainer.java:306`), com `playerSlots = concat(hotBarSlots,
                        // mainInventorySlots)` (`:93`): a barra e depois o inventário principal, nunca a armadura.
                        let order: Vec<usize> = hotbar.iter().chain(&main).copied().collect();
                        inserted = insert_all(menu, &order, &item, amount);
                    }
                    Region::Main | Region::Hotbar => {
                        inserted = insert_pass(menu, &armor, &item, amount, false);
                        let other = if region == Region::Main {
                            &hotbar
                        } else {
                            &main
                        };
                        inserted += insert_all(menu, other, &item, amount - inserted);
                    }
                    Region::Container => {}
                }
            }
        }
        inserted
    };
    if inserted == 0 {
        return None;
    }
    menu.remove(slot, inserted);
    Some(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um menu de teste: cada slot tem região, uma regra de aceitação e o conteúdo.
    struct Menu {
        regions: Vec<Region>,
        contents: Vec<Option<(&'static str, u32)>>,
        /// Os slots do contêiner que aceitam cada item; o resto do menu aceita tudo.
        accepts: Vec<Option<&'static str>>,
        hidden: Vec<usize>,
        container_order: Vec<usize>,
    }

    impl QuickMenu for Menu {
        type Item = &'static str;

        fn slots_of(&self, region: Region) -> Vec<usize> {
            if region == Region::Container {
                return self.container_order.clone();
            }
            (0..self.regions.len())
                .filter(|slot| self.regions[*slot] == region)
                .collect()
        }

        fn region(&self, slot: usize) -> Region {
            self.regions[slot]
        }

        fn exists(&self, slot: usize) -> bool {
            !self.hidden.contains(&slot)
        }

        fn stack(&self, slot: usize) -> Option<(Self::Item, u32)> {
            self.contents[slot]
        }

        fn insert(&mut self, slot: usize, item: &Self::Item, amount: u32) -> u32 {
            if let Some(only) = self.accepts[slot]
                && only != *item
            {
                return 0;
            }
            if self.regions[slot] == Region::Container && self.accepts[slot].is_none() {
                // Um slot de contêiner sem regra é só de saída.
                return 0;
            }
            match self.contents[slot] {
                Some((held, count)) if held == *item => {
                    let moved = amount.min(64 - count);
                    self.contents[slot] = Some((held, count + moved));
                    moved
                }
                Some(_) => 0,
                None => {
                    let moved = amount.min(64);
                    self.contents[slot] = Some((*item, moved));
                    moved
                }
            }
        }

        fn remove(&mut self, slot: usize, amount: u32) {
            if let Some((item, count)) = self.contents[slot] {
                self.contents[slot] = (count > amount).then_some((item, count - amount));
            }
        }
    }

    /// Dois slots de contêiner, um virtual que só aceita "upgrade" (e some sem janela), mais 4 do inventário e 2 da barra.
    fn menu() -> Menu {
        let mut regions = vec![Region::Container, Region::Container, Region::Container];
        regions.extend([Region::Main; 4]);
        regions.extend([Region::Hotbar; 2]);
        let mut accepts = vec![Some("clay"), None, Some("upgrade")];
        accepts.extend([None; 6]);
        Menu {
            regions,
            contents: vec![None; 9],
            accepts,
            hidden: vec![2],
            // O virtual (2) é o primeiro na ordem do Java.
            container_order: vec![2, 0, 1],
        }
    }

    #[test]
    fn an_item_nothing_takes_goes_main_to_hotbar_and_back() {
        let mut menu = menu();
        menu.contents[3] = Some(("stone", 5));
        assert_eq!(quick_move(&mut menu, 3), Some(5));
        assert_eq!(menu.contents[3], None);
        assert_eq!(menu.contents[7], Some(("stone", 5)));
        assert_eq!(quick_move(&mut menu, 7), Some(5));
        assert_eq!(menu.contents[3], Some(("stone", 5)));
    }

    #[test]
    fn the_hotbar_stacks_before_it_uses_an_empty_slot() {
        let mut menu = menu();
        menu.contents[3] = Some(("stone", 5));
        menu.contents[8] = Some(("stone", 10));
        assert_eq!(quick_move(&mut menu, 3), Some(5));
        assert_eq!(menu.contents[8], Some(("stone", 15)));
        assert_eq!(menu.contents[7], None);
    }

    #[test]
    fn the_virtual_slot_takes_the_item_only_with_its_window() {
        let mut menu = menu();
        menu.contents[3] = Some(("upgrade", 3));
        assert_eq!(quick_move(&mut menu, 3), Some(3));
        assert_eq!(menu.contents[2], None, "sem janela o virtual não existe");
        assert_eq!(menu.contents[7], Some(("upgrade", 3)));
        menu.hidden.clear();
        menu.contents[7] = None;
        menu.contents[3] = Some(("upgrade", 3));
        assert_eq!(quick_move(&mut menu, 3), Some(3));
        assert_eq!(menu.contents[2], Some(("upgrade", 3)));
    }

    #[test]
    fn a_container_slot_empties_into_the_hotbar_first() {
        let mut menu = menu();
        menu.contents[0] = Some(("clay", 20));
        assert_eq!(quick_move(&mut menu, 0), Some(20));
        assert_eq!(menu.contents[0], None);
        assert_eq!(menu.contents[7], Some(("clay", 20)));
    }

    /// Um contêiner, uma armadura, uma mão, dois do inventário principal e dois da barra, nessa ordem de índice.
    fn menu_with_armor() -> Menu {
        let regions = vec![
            Region::Container,
            Region::Armor,
            Region::Offhand,
            Region::Main,
            Region::Main,
            Region::Hotbar,
            Region::Hotbar,
        ];
        Menu {
            regions,
            contents: vec![None; 7],
            accepts: vec![Some("clay"), None, None, None, None, None, None],
            hidden: Vec::new(),
            container_order: vec![0],
        }
    }

    #[test]
    fn an_armor_or_offhand_slot_empties_into_the_hotbar_then_the_main_inventory() {
        // `insertItem(playerSlots, ...)` com `playerSlots = concat(hotBarSlots, mainInventorySlots)` (`MekanismContainer.java:93,306`).
        for source in [1, 2] {
            let mut menu = menu_with_armor();
            menu.contents[source] = Some(("helmet", 1));
            assert_eq!(quick_move(&mut menu, source), Some(1));
            assert_eq!(menu.contents[source], None);
            assert_eq!(menu.contents[5], Some(("helmet", 1)), "a barra primeiro");
            assert_eq!(menu.contents[3], None);
        }
    }

    #[test]
    fn an_armor_slot_stacks_in_the_hotbar_and_main_before_any_empty_slot() {
        // As duas passadas correm a barra e depois o principal: o que já tem item vem antes de qualquer vazio, em qualquer um dos dois.
        let mut menu = menu_with_armor();
        menu.contents[2] = Some(("stone", 10));
        menu.contents[4] = Some(("stone", 60));
        assert_eq!(quick_move(&mut menu, 2), Some(10));
        assert_eq!(menu.contents[4], Some(("stone", 64)));
        assert_eq!(
            menu.contents[5],
            Some(("stone", 6)),
            "o resto na barra vazia"
        );
    }

    #[test]
    fn an_armor_or_offhand_slot_never_goes_into_another_armor_slot() {
        let mut menu = menu_with_armor();
        menu.contents[1] = Some(("helmet", 1));
        menu.contents[3] = Some(("x", 1));
        menu.contents[4] = Some(("x", 1));
        menu.contents[5] = Some(("x", 1));
        menu.contents[6] = Some(("x", 1));
        // Tudo cheio: a mão, vazia, não é destino da armadura, e a armadura não é da mão.
        assert_eq!(quick_move(&mut menu, 1), None);
        menu.contents[2] = Some(("stone", 1));
        menu.contents[1] = None;
        assert_eq!(quick_move(&mut menu, 2), None);
    }

    #[test]
    fn a_hidden_virtual_slot_moves_nothing() {
        let mut menu = menu();
        menu.contents[2] = Some(("upgrade", 2));
        assert_eq!(quick_move(&mut menu, 2), None);
        assert_eq!(menu.contents[2], Some(("upgrade", 2)));
    }

    #[test]
    fn an_empty_slot_or_a_full_destination_moves_nothing() {
        let mut menu = menu();
        assert_eq!(quick_move(&mut menu, 3), None);
        menu.contents[3] = Some(("stone", 5));
        menu.contents[7] = Some(("clay", 1));
        menu.contents[8] = Some(("clay", 1));
        assert_eq!(quick_move(&mut menu, 3), None);
        assert_eq!(menu.contents[3], Some(("stone", 5)));
    }
}
