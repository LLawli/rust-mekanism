//! `Slot.mayPlace` de um menu de contêiner de mod, decidido pelo mod com o estado do bloco que o jogador tem aberto.
//!
//! O Mekanism responde `InventoryContainerSlot.mayPlace` com `isValidForInsertion(item, MANUAL)` do slot da máquina, e as regras
//! de vários slots olham outros slots e tanques (o Metallurgic Infuser pergunta pelo químico do tanque e pelo item da entrada).
//! O hook do Horizon (`menu_slot_rules`) roda dentro do menu, sem o jogo, sem a posição do bloco e sem o outro slot: só sabe o
//! menu, o número do slot e o item. Então quem vê o jogo (o mod, no começo de cada clique de menu) deixa aqui a regra do bloco
//! que o jogador tem aberto, já com o estado dele, e o hook só a consulta.
//!
//! O contexto vale para um clique. Cada mod tem o seu (`owner`), e o clique seguinte o troca antes de qualquer pergunta.
use std::cell::RefCell;
use std::rc::Rc;

/// Se o slot de contêiner `slot` do menu aceita o item de id `item` (`isValidForInsertion(.., MANUAL)`).
pub type MayPlace = Rc<dyn Fn(usize, &str) -> bool>;

struct Entry {
    owner: &'static str,
    menu: Rc<str>,
    rule: MayPlace,
}

thread_local! {
    static CONTEXT: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// A regra do menu `menu` para o clique que vai rodar. Troca a que `owner` deixou antes.
pub fn set(owner: &'static str, menu: &str, rule: MayPlace) {
    CONTEXT.with_borrow_mut(|entries| {
        entries.retain(|entry| entry.owner != owner);
        entries.push(Entry {
            owner,
            menu: Rc::from(menu),
            rule,
        });
    });
}

/// O clique não é de um menu de `owner`: nenhuma regra dele vale.
pub fn clear(owner: &str) {
    CONTEXT.with_borrow_mut(|entries| entries.retain(|entry| entry.owner != owner));
}

/// A pergunta do hook: o jogo já respondeu o que o tipo de menu aceita em `may_place` (falso num slot de resultado), e a regra do
/// mod só pode tirar. Sem regra para o menu, a resposta do jogo fica.
pub fn ask(menu: &str, slot: usize, item: &str, may_place: &mut bool) {
    CONTEXT.with_borrow(|entries| {
        for entry in entries.iter().filter(|entry| &*entry.menu == menu) {
            if !(entry.rule)(slot, item) {
                *may_place = false;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(slot: usize, item: &'static str) -> MayPlace {
        Rc::new(move |asked, id| asked == slot && id == item)
    }

    #[test]
    fn the_rule_can_only_take_away() {
        set("a", "m:menu", only(1, "dirt"));
        let mut may = true;
        ask("m:menu", 1, "dirt", &mut may);
        assert!(may);
        ask("m:menu", 1, "stick", &mut may);
        assert!(!may);
        // Um slot que o jogo já recusou (resultado) não volta.
        let mut may = false;
        ask("m:menu", 1, "dirt", &mut may);
        assert!(!may);
        clear("a");
    }

    #[test]
    fn another_menu_is_left_alone() {
        set("a", "m:menu", only(1, "dirt"));
        let mut may = true;
        ask("other:menu", 0, "stick", &mut may);
        assert!(may);
        clear("a");
    }

    #[test]
    fn each_owner_has_its_own_rule_and_a_new_click_replaces_it() {
        set("a", "m:one", only(0, "dirt"));
        set("b", "m:two", only(0, "sand"));
        let mut may = true;
        ask("m:one", 0, "dirt", &mut may);
        assert!(may);
        ask("m:two", 0, "sand", &mut may);
        assert!(may);
        set("a", "m:one", only(0, "clay"));
        ask("m:one", 0, "dirt", &mut may);
        assert!(!may, "o clique novo trocou a regra de a");
        let mut may = true;
        ask("m:two", 0, "sand", &mut may);
        assert!(may, "e deixou a de b");
        clear("a");
        let mut may = true;
        ask("m:one", 0, "stick", &mut may);
        assert!(may, "sem regra, a resposta do jogo fica");
        clear("b");
    }
}
