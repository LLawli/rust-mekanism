//! Os slots de armadura e de mão secundária que um contêiner de máquina põe ao lado do inventário: `MekanismContainer.addArmorSlots`
//! (`MekanismContainer.java:231`), a escolha do lugar do `ContainerBuilder.armorSideBar` (`ContainerTypeDeferredRegister.java:145`),
//! `ArmorSlot`, `OffhandSlot` e o `insert` do `TransactionalSlot` que eles herdam. A lib não conhece id de bloco nem de item: quem
//! usa diz, por parâmetro, o que o jogo sabe de cada item (em que slot de equipar ele cabe, o tamanho da pilha, o efeito
//! `PREVENT_ARMOR_CHANGE`), e o desenho do fundo vazio é um nome ([`EmptyBackground`]) que o jogo traduz no sprite dele.

/// Quantos slots de armadura o Java põe (`int armorSlots = 4`).
pub const ARMOR_SLOT_COUNT: i32 = 4;
/// A distância entre dois slots da coluna (`y += 18`).
pub const SLOT_SPACING: i32 = 18;
/// O `offhandOffset` que diz "sem slot de mão" (`armorSideBar(x, y)` passa -1, e `addArmorSlots` testa `!= -1`).
pub const NO_OFFHAND: i32 = -1;
/// O valor que `armorSlotsX` e `armorSlotsY` têm sem `armorSideBar` (`private int armorSlotsX = -1, armorSlotsY = -1`).
pub const NO_SIDE_BAR: i32 = -1;
/// O índice do inventário do jogador onde a armadura começa (`FEET.getIndex(36)`): botas 36, calças 37, peitoral 38, capacete 39.
pub const INVENTORY_ARMOR_BASE: usize = 36;
/// `Inventory.SLOT_OFFHAND`.
pub const INVENTORY_OFFHAND: usize = 40;
/// `Container.getMaxStackSize()`, o que `Slot.getMaxStackSize()` devolve para o `OffhandSlot` (o inventário do jogador não o muda).
pub const CONTAINER_MAX_STACK_SIZE: u32 = 99;
/// `ArmorSlot.getMaxStackSize()`.
pub const ARMOR_MAX_STACK_SIZE: u32 = 1;

/// `EquipmentSlot`, só os de armadura de quem veste. Os valores de `EquipmentSlot.VALUES` são MAINHAND 0, OFFHAND 1, FEET 2, LEGS 3,
/// CHEST 4, HEAD 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentSlot {
    Feet,
    Legs,
    Chest,
    Head,
}

impl EquipmentSlot {
    /// `EquipmentSlot.VALUES.get(index)` para os quatro de armadura (2 a 5).
    pub fn from_values_index(index: i32) -> Option<Self> {
        match index {
            2 => Some(Self::Feet),
            3 => Some(Self::Legs),
            4 => Some(Self::Chest),
            5 => Some(Self::Head),
            _ => None,
        }
    }

    /// `EquipmentSlot.getIndex()`: 0 nas botas até 3 no capacete.
    pub fn index(self) -> usize {
        match self {
            Self::Feet => 0,
            Self::Legs => 1,
            Self::Chest => 2,
            Self::Head => 3,
        }
    }
}

/// O sprite do fundo de um slot vazio (`InventoryMenu.EMPTY_ARMOR_SLOT_*`); o jogo traduz no id dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyBackground {
    Boots,
    Leggings,
    Chestplate,
    Helmet,
    Shield,
}

impl EmptyBackground {
    /// `ArmorSlot.ARMOR_SLOT_TEXTURES[slotType.getIndex()]`.
    pub fn of_armor(slot: EquipmentSlot) -> Self {
        [Self::Boots, Self::Leggings, Self::Chestplate, Self::Helmet][slot.index()]
    }
}

/// Que slot é: o de armadura de um `EquipmentSlot`, ou o da mão secundária.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Armor(EquipmentSlot),
    Offhand,
}

/// Um slot posto no contêiner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedSlot {
    pub kind: SlotKind,
    /// O índice do inventário do jogador que o slot espelha (36 a 39 na armadura, 40 na mão).
    pub inventory_index: usize,
    pub x: i32,
    pub y: i32,
    pub background: EmptyBackground,
}

/// `addArmorSlots(inv, x, y, offhandOffset)`: quatro slots de armadura em coluna, o capacete em cima, e a mão logo abaixo da coluna
/// (`y + 72 + offhandOffset`) a menos que `offhand_offset` seja [`NO_OFFHAND`]. A ordem é a da lista do Java (`armorSlots` e
/// `offhandSlots` na ordem em que `addSlot` os recebe).
pub fn add_armor_slots(x: i32, mut y: i32, offhand_offset: i32) -> Vec<PlacedSlot> {
    let mut slots = Vec::new();
    for index in 0..ARMOR_SLOT_COUNT {
        let slot = EquipmentSlot::from_values_index(2 + ARMOR_SLOT_COUNT - index - 1)
            .expect("2 + 4 - i - 1 fica entre 2 e 5");
        slots.push(PlacedSlot {
            kind: SlotKind::Armor(slot),
            inventory_index: INVENTORY_ARMOR_BASE + (ARMOR_SLOT_COUNT - index - 1) as usize,
            x,
            y,
            background: EmptyBackground::of_armor(slot),
        });
        y += SLOT_SPACING;
    }
    if offhand_offset != NO_OFFHAND {
        slots.push(PlacedSlot {
            kind: SlotKind::Offhand,
            inventory_index: INVENTORY_OFFHAND,
            x,
            y: y + offhand_offset,
            background: EmptyBackground::Shield,
        });
    }
    slots
}

/// O `addInventorySlots` do `ContainerBuilder`: chama `addArmorSlots` só se `armorSideBar` deu um lugar (`armorSlotsX != -1 &&
/// armorSlotsY != -1`). Sem barra lateral, não há slot.
pub fn armor_side_bar(x: i32, y: i32, offhand_offset: i32) -> Vec<PlacedSlot> {
    if x == NO_SIDE_BAR || y == NO_SIDE_BAR {
        return Vec::new();
    }
    add_armor_slots(x, y, offhand_offset)
}

/// `Slot.mayPlace`. A armadura pede um item que `ItemStack.canEquip(slotType, player)` aceite (o jogo responde por `can_equip`); a
/// mão aceita qualquer item (`OffhandSlot` não muda `mayPlace`).
pub fn may_place(kind: SlotKind, can_equip: impl FnOnce(EquipmentSlot) -> bool) -> bool {
    match kind {
        SlotKind::Armor(slot) => can_equip(slot),
        SlotKind::Offhand => true,
    }
}

/// `Slot.getMaxStackSize()`: 1 na armadura (`ArmorSlot`), e o do contêiner (99) na mão.
pub fn slot_max_stack_size(kind: SlotKind) -> u32 {
    match kind {
        SlotKind::Armor(_) => ARMOR_MAX_STACK_SIZE,
        SlotKind::Offhand => CONTAINER_MAX_STACK_SIZE,
    }
}

/// `Slot.getMaxStackSize(ItemStack)`: o menor entre o do slot e o do item.
pub fn max_stack_size(kind: SlotKind, item_max_stack_size: u32) -> u32 {
    slot_max_stack_size(kind).min(item_max_stack_size)
}

/// `Slot.mayPickup(player)`. A armadura recusa um item com `PREVENT_ARMOR_CHANGE` (maldição do vínculo) de quem não está no criativo;
/// slot vazio e a mão sempre deixam.
pub fn may_pickup(
    kind: SlotKind,
    stack_empty: bool,
    creative: bool,
    prevent_armor_change: bool,
) -> bool {
    match kind {
        SlotKind::Armor(_) => stack_empty || creative || !prevent_armor_change,
        SlotKind::Offhand => true,
    }
}

/// `TransactionalSlot.insert(resource, amount, transaction)`: quantos itens entram. `held` é o que o slot já tem (0 se vazio) e
/// `same_item` diz se é o mesmo item que entra (só vale com `held > 0`). Recusa o que `mayPlace` recusa, completa até
/// [`max_stack_size`] e nunca mistura dois itens.
pub fn insert(
    kind: SlotKind,
    held: u32,
    same_item: bool,
    amount: u32,
    item_max_stack_size: u32,
    can_equip: impl FnOnce(EquipmentSlot) -> bool,
) -> u32 {
    if amount == 0 || !may_place(kind, can_equip) {
        return 0;
    }
    let max = max_stack_size(kind, item_max_stack_size);
    if held >= max || held > 0 && !same_item {
        return 0;
    }
    amount.min(max - held)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EquipmentSlot::*;

    fn pos(slot: &PlacedSlot) -> (i32, i32) {
        (slot.x, slot.y)
    }

    #[test]
    fn the_generator_side_bar_is_a_column_with_the_helmet_on_top() {
        // `GeneratorsContainerTypes.java:29`: `armorSideBar(-20, 11, 0)`.
        let slots = armor_side_bar(-20, 11, 0);
        assert_eq!(slots.len(), 5);
        let kinds: Vec<SlotKind> = slots.iter().map(|slot| slot.kind).collect();
        assert_eq!(
            kinds,
            [
                SlotKind::Armor(Head),
                SlotKind::Armor(Chest),
                SlotKind::Armor(Legs),
                SlotKind::Armor(Feet),
                SlotKind::Offhand,
            ]
        );
        let positions: Vec<(i32, i32)> = slots.iter().map(pos).collect();
        assert_eq!(
            positions,
            [(-20, 11), (-20, 29), (-20, 47), (-20, 65), (-20, 83)]
        );
    }

    #[test]
    fn each_slot_mirrors_its_player_inventory_index() {
        let slots = armor_side_bar(-20, 11, 0);
        let indices: Vec<usize> = slots.iter().map(|slot| slot.inventory_index).collect();
        // Capacete 39, peitoral 38, calças 37, botas 36 (`36 + 4 - i - 1`) e a mão em `SLOT_OFFHAND`.
        assert_eq!(indices, [39, 38, 37, 36, 40]);
    }

    #[test]
    fn the_empty_backgrounds_follow_the_equipment_slot() {
        let slots = armor_side_bar(-20, 11, 0);
        let backgrounds: Vec<EmptyBackground> = slots.iter().map(|slot| slot.background).collect();
        assert_eq!(
            backgrounds,
            [
                EmptyBackground::Helmet,
                EmptyBackground::Chestplate,
                EmptyBackground::Leggings,
                EmptyBackground::Boots,
                EmptyBackground::Shield,
            ]
        );
    }

    #[test]
    fn the_offhand_sits_72_below_the_first_slot_plus_the_offset() {
        // `Modification Station`: `armorSideBar(8, 8, 8)`; a mão em `8 + 72 + 8`.
        let station = armor_side_bar(8, 8, 8);
        assert_eq!(pos(station.last().unwrap()), (8, 88));
        // Um deslocamento diferente de -1 move só a mão.
        let moved = armor_side_bar(-20, 11, 5);
        assert_eq!(pos(&moved[3]), (-20, 65));
        assert_eq!(pos(&moved[4]), (-20, 88));
        // Só o -1 tira a mão: qualquer outro deslocamento, negativo também, a põe.
        assert_eq!(pos(armor_side_bar(0, 0, -2).last().unwrap()), (0, 70));
    }

    #[test]
    fn offhand_offset_minus_one_leaves_the_offhand_out() {
        // `armorSideBar(x, y)` sem o terceiro argumento: `MekanismContainer.addArmorSlots` não põe a mão.
        let slots = armor_side_bar(-20, 67, NO_OFFHAND);
        assert_eq!(slots.len(), 4);
        assert!(slots.iter().all(|slot| slot.kind != SlotKind::Offhand));
    }

    #[test]
    fn the_default_armor_side_bar_and_other_registered_ones() {
        // `armorSideBar()` é `(-20, 67, 0)`; o Energy Cube é `(180, 41, 0)` e a Induction Matrix `(-20, 41, 0)`.
        assert_eq!(pos(&armor_side_bar(-20, 67, 0)[0]), (-20, 67));
        assert_eq!(pos(armor_side_bar(-20, 67, 0).last().unwrap()), (-20, 139));
        assert_eq!(pos(armor_side_bar(180, 41, 0).last().unwrap()), (180, 113));
        assert_eq!(pos(&armor_side_bar(-20, 41, 0)[3]), (-20, 95));
    }

    #[test]
    fn without_a_side_bar_there_are_no_slots() {
        // `armorSlotsX != -1 && armorSlotsY != -1`: um deles em -1 e o contêiner não chama `addArmorSlots`.
        assert!(armor_side_bar(NO_SIDE_BAR, NO_SIDE_BAR, NO_OFFHAND).is_empty());
        assert!(armor_side_bar(NO_SIDE_BAR, 11, 0).is_empty());
        assert!(armor_side_bar(-20, NO_SIDE_BAR, 0).is_empty());
        // `addArmorSlots` em si não testa isso.
        assert_eq!(add_armor_slots(NO_SIDE_BAR, 11, 0).len(), 5);
    }

    #[test]
    fn equipment_slot_values_index_matches_the_vanilla_enum() {
        assert_eq!(EquipmentSlot::from_values_index(2), Some(Feet));
        assert_eq!(EquipmentSlot::from_values_index(5), Some(Head));
        // MAINHAND e OFFHAND (0 e 1) e o que passa do capacete não são de armadura.
        assert_eq!(EquipmentSlot::from_values_index(1), None);
        assert_eq!(EquipmentSlot::from_values_index(6), None);
        assert_eq!(
            [Feet, Legs, Chest, Head].map(EquipmentSlot::index),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn armor_only_takes_what_can_be_worn_in_that_slot() {
        // Um capacete cabe só no slot da cabeça.
        let helmet = |slot| slot == Head;
        assert!(may_place(SlotKind::Armor(Head), helmet));
        assert!(!may_place(SlotKind::Armor(Feet), helmet));
        assert!(!may_place(SlotKind::Armor(Chest), helmet));
        // Um item que não se veste não cabe em nenhum.
        for slot in [Feet, Legs, Chest, Head] {
            assert!(!may_place(SlotKind::Armor(slot), |_| false));
        }
    }

    #[test]
    fn the_offhand_takes_any_item_and_never_asks_the_equip_rule() {
        assert!(may_place(SlotKind::Offhand, |_| panic!("não pergunta")));
    }

    #[test]
    fn the_stack_size_is_one_for_armor_and_the_items_own_in_the_offhand() {
        assert_eq!(slot_max_stack_size(SlotKind::Armor(Head)), 1);
        assert_eq!(slot_max_stack_size(SlotKind::Offhand), 99);
        assert_eq!(max_stack_size(SlotKind::Armor(Head), 64), 1);
        assert_eq!(max_stack_size(SlotKind::Armor(Head), 1), 1);
        assert_eq!(max_stack_size(SlotKind::Offhand, 64), 64);
        assert_eq!(max_stack_size(SlotKind::Offhand, 16), 16);
        assert_eq!(max_stack_size(SlotKind::Offhand, 1), 1);
    }

    #[test]
    fn a_bound_piece_stays_in_the_armor_slot_outside_creative() {
        let armor = SlotKind::Armor(Head);
        // `stack.isEmpty() || player.isCreative() || !has(PREVENT_ARMOR_CHANGE)`.
        assert!(!may_pickup(armor, false, false, true));
        assert!(may_pickup(armor, false, true, true));
        assert!(may_pickup(armor, true, false, true));
        assert!(may_pickup(armor, false, false, false));
        // A mão não olha o efeito.
        assert!(may_pickup(SlotKind::Offhand, false, false, true));
    }

    #[test]
    fn inserting_into_an_armor_slot_moves_one_item_at_most() {
        let armor = SlotKind::Armor(Head);
        let helmet = |slot| slot == Head;
        // Vazio: entra um, mesmo de uma pilha de 64 e de um item que empilha.
        assert_eq!(insert(armor, 0, false, 1, 1, helmet), 1);
        assert_eq!(insert(armor, 0, false, 5, 64, helmet), 1);
        // Cheio, do mesmo item ou de outro: nada entra.
        assert_eq!(insert(armor, 1, true, 1, 64, helmet), 0);
        assert_eq!(insert(armor, 1, false, 1, 64, helmet), 0);
        // O item errado para o slot, nada.
        assert_eq!(insert(SlotKind::Armor(Feet), 0, false, 1, 1, helmet), 0);
        // Quantidade zero, nada.
        assert_eq!(insert(armor, 0, false, 0, 1, helmet), 0);
    }

    #[test]
    fn inserting_into_the_offhand_follows_the_item_stack_size() {
        let any = SlotKind::Offhand;
        let none = |_| false;
        assert_eq!(insert(any, 0, false, 64, 64, none), 64);
        assert_eq!(insert(any, 10, true, 64, 64, none), 54);
        assert_eq!(insert(any, 64, true, 1, 64, none), 0);
        // Outro item no slot ocupado: nada. Um item de pilha 16 enche em 16.
        assert_eq!(insert(any, 3, false, 1, 64, none), 0);
        assert_eq!(insert(any, 14, true, 5, 16, none), 2);
        // Um item que não empilha (pilha 1): um só.
        assert_eq!(insert(any, 0, false, 3, 1, none), 1);
    }
}
