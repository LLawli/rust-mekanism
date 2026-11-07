//! Porte de `ComposterWrapper` do NeoForge: o que o bloco da composteira expõe de itens a quem está do lado
//! (`Capabilities.Item.BLOCK`). Só as faces de cima e de baixo têm capacidade: quem a pede de um lado (`Direction.getAxis().isVertical()`
//! falso) não recebe nada, então um ejetor que sai por uma face lateral nunca chega a uma composteira.
//!
//! A face de cima (`Top`) aceita um item compostável por vez e sobe o nível em um, não em quantas camadas o componente
//! `compostable` sortear: o sorteio só decide se o item entra (`layers > 0`). A de baixo (`Bottom`) só entrega o farelo de osso do
//! nível `READY`. A mudança de nível entra num diário da transação, e o `onRootCommit` faz o que o bloco faz depois de uma mudança
//! (avisos, tick agendado e o evento de nível 1500), pelos métodos de [`ComposterBlock`].
use crate::containers::ItemResource;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::rc::Rc;

/// `ComposterBlock.MAX_LEVEL`: a partir dele o bloco não aceita mais itens e agenda a conversão em farelo.
pub const MAX_LEVEL: i32 = 7;
/// `ComposterBlock.READY`: o nível com farelo de osso para tirar.
pub const READY: i32 = 8;
/// `ComposterBlock.MIN_LEVEL`.
pub const MIN_LEVEL: i32 = 0;
/// Os ticks até o bloco ficar pronto (`scheduleTick(pos, block, SharedConstants.TICKS_PER_SECOND)`).
pub const READY_DELAY_TICKS: u32 = 20;

/// O bloco que o wrapper mexe. Quem usa implementa sobre o mundo (a lib não conhece o jogo).
pub trait ComposterBlock {
    /// O bloco ainda é uma composteira (`currentState.is(Blocks.COMPOSTER)`).
    fn exists(&self) -> bool;
    /// `state.getValue(ComposterBlock.LEVEL)`.
    fn level(&self) -> i32;
    /// Muda o nível. Sem `notify` é o `level.setBlock(pos, state, 0)`, que não avisa ninguém; com `notify`, o
    /// `setBlockAndUpdate` seguido do evento de jogo `BLOCK_CHANGE`.
    fn set_level(&self, level: i32, notify: bool);
    /// `resource.get(DataComponents.COMPOSTABLE).layers().get(lootContext, 0)`, calculado a cada chamada (o valor pode sortear):
    /// `None` quando o item não tem o componente.
    fn layers(&self, resource: &ItemResource) -> Option<i32>;
    /// `level.scheduleTick(pos, block, ticks)`.
    fn schedule_tick(&self, ticks: u32);
    /// `level.levelEvent(LevelEvent.COMPOSTER_FILL, pos, rose ? 1 : 0)`.
    fn level_event(&self, rose: bool);
}

/// O que o diário guarda do bloco: o nível, que o `SnapshotJournal<BlockState>` do Java guarda como o estado inteiro.
struct Journal {
    block: Rc<dyn ComposterBlock>,
}

impl Journaled for Journal {
    type Snapshot = i32;

    /// `revertToSnapshot`: `level.setBlock(pos, snapshot, 0)`.
    fn revert_to_snapshot(&mut self, snapshot: i32) {
        self.block.set_level(snapshot, false);
    }

    /// `onRootCommit(originalState)`.
    fn on_root_commit(&mut self, original: i32) -> Vec<Callback> {
        let block = self.block.clone();
        vec![Rc::new(move || {
            if !block.exists() {
                return;
            }
            let current = block.level();
            // Mesmo sem mudança o evento de enchimento sai, mais abaixo.
            if current != original {
                // Volta ao estado de antes, sem avisar, para a mudança de agora disparar as notificações dos vizinhos.
                block.set_level(original, false);
                block.set_level(current, true);
            }
            if original < MAX_LEVEL {
                if current == MAX_LEVEL {
                    block.schedule_tick(READY_DELAY_TICKS);
                }
                block.level_event(current > original);
            }
        })]
    }
}

/// Um wrapper por composteira: o diário e o recurso do farelo, que as duas faces compartilham.
pub struct ComposterWrapper {
    block: Rc<dyn ComposterBlock>,
    journal: Rc<JournalCell<Journal>>,
    bone_meal: ItemResource,
}

impl ComposterWrapper {
    pub fn new(block: Rc<dyn ComposterBlock>, bone_meal: ItemResource) -> Rc<Self> {
        Rc::new(Self {
            journal: JournalCell::new(Journal {
                block: block.clone(),
            }),
            block,
            bone_meal,
        })
    }

    /// `ComposterWrapper.get(level, pos, direction)`: o handler do lado `side` da composteira (o lado por onde o outro bloco a
    /// alcança). Só as faces vertical têm um: cima é a entrada, baixo é a saída do farelo.
    pub fn handler(self: &Rc<Self>, side: Face) -> Option<Rc<dyn ItemHandler>> {
        match side {
            Face::Up => Some(Rc::new(Top(self.clone()))),
            Face::Down => Some(Rc::new(Bottom(self.clone()))),
            _ => None,
        }
    }
}

struct Top(Rc<ComposterWrapper>);

impl ItemHandler for Top {
    fn size(&self) -> usize {
        1
    }

    fn resource(&self, _index: usize) -> Option<ItemResource> {
        None
    }

    fn amount(&self, _index: usize) -> u32 {
        0
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        assert!(index < 1, "índice {index} fora do handler");
        if amount < 1 {
            return 0;
        }
        let wrapper = &self.0;
        let level = wrapper.block.level();
        if level >= MAX_LEVEL {
            return 0;
        }
        // Um item sem o componente vale 0; um com ele sorteia as camadas, e só entra se der alguma.
        if wrapper.block.layers(resource).unwrap_or(0) <= 0 {
            return 0;
        }
        wrapper.journal.update_snapshots(transaction, || level);
        // Sobe um, como o vanilla: o sorteio das camadas só decide se entra.
        wrapper.block.set_level(level + 1, false);
        1
    }

    fn extract_at(
        &self,
        _index: usize,
        _resource: &ItemResource,
        _amount: u32,
        _transaction: &Transaction,
    ) -> u32 {
        0
    }
}

struct Bottom(Rc<ComposterWrapper>);

impl Bottom {
    fn ready(&self) -> bool {
        self.0.block.level() == READY
    }
}

impl ItemHandler for Bottom {
    fn size(&self) -> usize {
        1
    }

    fn resource(&self, _index: usize) -> Option<ItemResource> {
        self.ready().then(|| self.0.bone_meal.clone())
    }

    fn amount(&self, _index: usize) -> u32 {
        u32::from(self.ready())
    }

    fn insert_at(
        &self,
        _index: usize,
        _resource: &ItemResource,
        _amount: u32,
        _transaction: &Transaction,
    ) -> u32 {
        0
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        assert!(index < 1, "índice {index} fora do handler");
        if amount < 1 || *resource != self.0.bone_meal || !self.ready() {
            return 0;
        }
        self.0
            .journal
            .update_snapshots(transaction, || self.0.block.level());
        self.0.block.set_level(MIN_LEVEL, false);
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Uma composteira de mentira que anota o que lhe pedem.
    struct Fake {
        level: Cell<i32>,
        present: Cell<bool>,
        layers: Cell<Option<i32>>,
        events: RefCell<Vec<String>>,
    }

    impl Fake {
        fn at(level: i32) -> Rc<Self> {
            Rc::new(Self {
                level: Cell::new(level),
                present: Cell::new(true),
                layers: Cell::new(Some(1)),
                events: RefCell::new(Vec::new()),
            })
        }
    }

    impl ComposterBlock for Fake {
        fn exists(&self) -> bool {
            self.present.get()
        }

        fn level(&self) -> i32 {
            self.level.get()
        }

        fn set_level(&self, level: i32, notify: bool) {
            self.level.set(level);
            self.events
                .borrow_mut()
                .push(format!("set {level} {notify}"));
        }

        fn layers(&self, _resource: &ItemResource) -> Option<i32> {
            self.layers.get()
        }

        fn schedule_tick(&self, ticks: u32) {
            self.events.borrow_mut().push(format!("schedule {ticks}"));
        }

        fn level_event(&self, rose: bool) {
            self.events.borrow_mut().push(format!("event {rose}"));
        }
    }

    fn cake() -> ItemResource {
        ItemResource::new("cake", 64)
    }

    fn bone_meal() -> ItemResource {
        ItemResource::new("bone_meal", 64)
    }

    fn wrapper(fake: &Rc<Fake>) -> Rc<ComposterWrapper> {
        ComposterWrapper::new(fake.clone(), bone_meal())
    }

    fn insert(wrapper: &Rc<ComposterWrapper>, resource: &ItemResource) -> u32 {
        let top = wrapper.handler(Face::Up).unwrap();
        let root = Transaction::open_root();
        let inserted = top.insert(resource, 1, &root);
        root.commit();
        inserted
    }

    #[test]
    fn only_the_vertical_faces_have_a_handler() {
        let wrapper = wrapper(&Fake::at(0));
        for side in [Face::North, Face::South, Face::East, Face::West] {
            assert!(wrapper.handler(side).is_none(), "{side:?}");
        }
        assert!(wrapper.handler(Face::Up).is_some());
        assert!(wrapper.handler(Face::Down).is_some());
    }

    #[test]
    fn the_top_raises_the_level_by_one_and_commits_the_effects() {
        let fake = Fake::at(0);
        assert_eq!(insert(&wrapper(&fake), &cake()), 1);
        assert_eq!(fake.level(), 1);
        // O nível volta ao de antes, sem avisar, e a mudança sai com os avisos; e o evento de enchimento diz que subiu.
        assert_eq!(
            *fake.events.borrow(),
            ["set 1 false", "set 0 false", "set 1 true", "event true"]
        );
    }

    #[test]
    fn reaching_the_max_level_schedules_the_ready_tick() {
        let fake = Fake::at(6);
        assert_eq!(insert(&wrapper(&fake), &cake()), 1);
        assert!(fake.events.borrow().contains(&"schedule 20".to_owned()));
        let below = Fake::at(0);
        insert(&wrapper(&below), &cake());
        assert!(
            !below
                .events
                .borrow()
                .iter()
                .any(|e| e.starts_with("schedule"))
        );
    }

    #[test]
    fn a_full_or_ready_composter_takes_nothing() {
        for level in [MAX_LEVEL, READY] {
            let fake = Fake::at(level);
            assert_eq!(insert(&wrapper(&fake), &cake()), 0, "nível {level}");
            assert_eq!(fake.level(), level);
            assert!(fake.events.borrow().is_empty());
        }
    }

    #[test]
    fn an_item_that_is_not_compostable_or_rolls_zero_layers_stays_out() {
        let fake = Fake::at(0);
        fake.layers.set(None);
        assert_eq!(insert(&wrapper(&fake), &cake()), 0);
        fake.layers.set(Some(0));
        assert_eq!(insert(&wrapper(&fake), &cake()), 0);
        assert_eq!(fake.level(), 0);
        assert!(fake.events.borrow().is_empty());
    }

    #[test]
    fn more_layers_than_one_still_raise_the_level_by_one() {
        let fake = Fake::at(0);
        fake.layers.set(Some(5));
        assert_eq!(insert(&wrapper(&fake), &cake()), 1);
        assert_eq!(fake.level(), 1);
    }

    #[test]
    fn an_aborted_transaction_puts_the_level_back() {
        let fake = Fake::at(3);
        let wrapper = wrapper(&fake);
        let top = wrapper.handler(Face::Up).unwrap();
        {
            let root = Transaction::open_root();
            assert_eq!(top.insert(&cake(), 1, &root), 1);
            assert_eq!(fake.level(), 4);
        }
        assert_eq!(fake.level(), 3);
        assert!(!fake.events.borrow().iter().any(|e| e.starts_with("event")));
    }

    #[test]
    fn the_fill_event_says_whether_the_level_rose_by_the_commit() {
        // Se algo põe o nível de volta antes do commit, o evento sai sem o "subiu" e os avisos de mudança não saem.
        let fake = Fake::at(2);
        let wrapper = wrapper(&fake);
        let top = wrapper.handler(Face::Up).unwrap();
        let root = Transaction::open_root();
        top.insert(&cake(), 1, &root);
        fake.level.set(2);
        fake.events.borrow_mut().clear();
        root.commit();
        assert_eq!(*fake.events.borrow(), ["event false"]);
    }

    #[test]
    fn a_composter_removed_before_the_commit_gets_no_effects() {
        let fake = Fake::at(0);
        let wrapper = wrapper(&fake);
        let top = wrapper.handler(Face::Up).unwrap();
        let root = Transaction::open_root();
        top.insert(&cake(), 1, &root);
        fake.present.set(false);
        fake.events.borrow_mut().clear();
        root.commit();
        assert!(fake.events.borrow().is_empty());
    }

    #[test]
    fn the_bottom_gives_only_bone_meal_and_only_when_ready() {
        let fake = Fake::at(READY);
        let wrapper = wrapper(&fake);
        let bottom = wrapper.handler(Face::Down).unwrap();
        assert_eq!(bottom.resource(0), Some(bone_meal()));
        assert_eq!(bottom.amount(0), 1);
        let root = Transaction::open_root();
        assert_eq!(bottom.insert(&cake(), 1, &root), 0);
        assert_eq!(bottom.extract(&cake(), 1, &root), 0);
        assert_eq!(bottom.extract(&bone_meal(), 1, &root), 1);
        root.commit();
        assert_eq!(fake.level(), MIN_LEVEL);
        let waiting = Fake::at(MAX_LEVEL);
        let bottom = self::wrapper(&waiting).handler(Face::Down).unwrap();
        assert_eq!(bottom.resource(0), None);
        let root = Transaction::open_root();
        assert_eq!(bottom.extract(&bone_meal(), 1, &root), 0);
    }
}
