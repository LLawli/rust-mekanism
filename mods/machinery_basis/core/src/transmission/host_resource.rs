//! O vizinho de um transmissor de recurso visto pelo jogo: um [`HostHandler`] lê o contêiner dele no começo de um evento (o [`ResourceSnapshot`]),
//! responde às consultas da rede (`insert`, `extract`, `size`...) sobre um espelho desse contêiner, dentro das transações dela, e o fim do evento
//! escreve de volta o que mudou ([`HostHandler::changed`]).
//!
//! O Horizon não desfaz uma inserção e a lib não tem o jogo na mão quando a rede distribui (decisão 122, a mesma conta da energia): o espelho é um
//! [`ResourceTank`] com as regras do contêiner de verdade (o que ele aceita, se deixa entrar ou sair de fora, a capacidade), então o
//! `insert` simulado, o `extract` e a reversão de uma transação saem como no original, e o jogo recebe só o conteúdo final.
use super::buffered_resource_network::{ResourceAcceptor, ResourceExtract};
use crate::containers::AutomationType;
use crate::distribution::ResourceInsert;
use crate::resource_tank::{Resource, ResourceTank, StackContents};
use crate::transaction::Transaction;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// `Predicate<R>` de um contêiner.
pub type Accepts<R> = Rc<dyn Fn(&R) -> bool>;

/// O que o contêiner de um vizinho dizia no começo do evento.
#[derive(Clone)]
pub struct ResourceSnapshot<R: Resource> {
    pub contents: StackContents<R>,
    pub capacity: u64,
    /// O validador do contêiner (`fluidStack -> fluidStack.is(FluidTags.LAVA)`).
    pub accepts: Accepts<R>,
    /// `canInsert` para quem vem de fora (`EXTERNAL`).
    pub can_insert: bool,
    /// `canExtract` para quem vem de fora.
    pub can_extract: bool,
    /// O tanque criativo (`isCreative`): `insert` e `extract` dão o resultado sem mudar nada.
    pub creative: bool,
    /// O passo em que o contêiner aceita e entrega (1: qualquer quantidade): o `CauldronWrapper` só move múltiplos de `totalAmount / gcd(maxLevel, totalAmount)`, e o que não completa um passo
    /// fica de fora.
    pub increment: u64,
}

impl<R: Resource> ResourceSnapshot<R> {
    /// Um vizinho sem contêiner que a rede alcance: não aceita nem entrega.
    pub fn closed() -> Self {
        Self {
            contents: StackContents::default(),
            capacity: 0,
            accepts: Rc::new(|_| false),
            can_insert: false,
            can_extract: false,
            creative: false,
            increment: 1,
        }
    }
}

struct Rules<R: Resource> {
    can_insert: Cell<bool>,
    can_extract: Cell<bool>,
    accepts: RefCell<Accepts<R>>,
}

/// A capability de recurso de um vizinho, como a rede a usa.
pub struct HostHandler<R: Resource> {
    tank: ResourceTank<R>,
    rules: Rc<Rules<R>>,
    creative: Cell<bool>,
    increment: Cell<u64>,
    /// O conteúdo do espelho quando o jogo o entregou.
    before: RefCell<StackContents<R>>,
}

impl<R: Resource> HostHandler<R> {
    pub fn new(snapshot: ResourceSnapshot<R>) -> Self {
        let rules = Rc::new(Rules {
            can_insert: Cell::new(snapshot.can_insert),
            can_extract: Cell::new(snapshot.can_extract),
            accepts: RefCell::new(snapshot.accepts.clone()),
        });
        let tank = ResourceTank::create(
            snapshot.capacity,
            {
                let rules = rules.clone();
                Rc::new(move |_, _| rules.can_extract.get())
            },
            {
                let rules = rules.clone();
                Rc::new(move |_, _| rules.can_insert.get())
            },
            {
                let rules = rules.clone();
                Rc::new(move |resource| (rules.accepts.borrow())(resource))
            },
            None,
        );
        let handler = Self {
            tank,
            rules,
            creative: Cell::new(snapshot.creative),
            increment: Cell::new(snapshot.increment),
            before: RefCell::new(StackContents::default()),
        };
        handler.refresh(snapshot);
        handler
    }

    /// Lê o contêiner de novo (o começo de um evento): o que o jogo mexeu desde a última leitura vale.
    pub fn refresh(&self, snapshot: ResourceSnapshot<R>) {
        self.rules.can_insert.set(snapshot.can_insert);
        self.rules.can_extract.set(snapshot.can_extract);
        *self.rules.accepts.borrow_mut() = snapshot.accepts;
        self.creative.set(snapshot.creative);
        self.increment.set(snapshot.increment);
        self.tank.set_capacity(snapshot.capacity);
        self.tank.set_contents(
            snapshot.contents.resource.clone(),
            snapshot.contents.amount,
            None,
        );
        *self.before.borrow_mut() = snapshot.contents;
    }

    /// O conteúdo que o jogo tem de passar a ter, se a rede mudou o espelho; `None` se nada mudou desde a leitura ou desde o último
    /// [`Self::settle`].
    pub fn changed(&self) -> Option<StackContents<R>> {
        let now = self.tank.contents();
        (now != *self.before.borrow()).then_some(now)
    }

    /// O jogo recebeu o conteúdo de [`Self::changed`].
    pub fn settle(&self) {
        *self.before.borrow_mut() = self.tank.contents();
    }

    /// O conteúdo do espelho agora.
    pub fn contents(&self) -> StackContents<R> {
        self.tank.contents()
    }
}

impl<R: Resource> ResourceInsert<R> for HostHandler<R> {
    fn insert(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        if self.creative.get() {
            // Aceita sem mudar nada: o resultado de uma simulação.
            let simulation = Transaction::open(Some(transaction));
            return self
                .tank
                .insert(resource, amount, &simulation, AutomationType::External);
        }
        let step = self.increment.get();
        if step > 1 {
            // O que o contêiner aceitaria, em passos inteiros: um passo incompleto não entra (`CauldronWrapper.insert`).
            let fits = {
                let simulation = Transaction::open(Some(transaction));
                self.tank
                    .insert(resource, amount, &simulation, AutomationType::External)
            };
            let whole = (fits as u64 / step * step) as i32;
            if whole == 0 {
                return 0;
            }
            return self
                .tank
                .insert(resource, whole, transaction, AutomationType::External);
        }
        self.tank
            .insert(resource, amount, transaction, AutomationType::External)
    }
}

impl<R: Resource> ResourceExtract<R> for HostHandler<R> {
    fn size(&self) -> usize {
        1
    }

    fn resource_at(&self, _: usize) -> Option<R> {
        self.tank.resource()
    }

    fn amount_as_int_at(&self, _: usize) -> i32 {
        self.tank.amount_as_int()
    }

    fn extract_at(&self, _: usize, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        self.extract(resource, amount, transaction)
    }

    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        if self.creative.get() {
            let simulation = Transaction::open(Some(transaction));
            return self
                .tank
                .extract(resource, amount, &simulation, AutomationType::External);
        }
        let step = self.increment.get();
        if step > 1 {
            // O que o contêiner entregaria, em passos inteiros (`CauldronWrapper.extract`).
            let has = {
                let simulation = Transaction::open(Some(transaction));
                self.tank
                    .extract(resource, amount, &simulation, AutomationType::External)
            };
            let whole = (has as u64 / step * step) as i32;
            if whole == 0 {
                return 0;
            }
            return self
                .tank
                .extract(resource, whole, transaction, AutomationType::External);
        }
        self.tank
            .extract(resource, amount, transaction, AutomationType::External)
    }
}

impl<R: Resource> ResourceAcceptor<R> for HostHandler<R> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::FluidResource;

    fn lava() -> FluidResource {
        FluidResource::new("minecraft:lava")
    }

    fn water() -> FluidResource {
        FluidResource::new("minecraft:water")
    }

    /// O `lavaTank` do Heat Generator: só lava, entra de fora, ninguém de fora tira.
    fn lava_tank(amount: u64) -> ResourceSnapshot<FluidResource> {
        ResourceSnapshot {
            contents: StackContents::new(Some(lava()), amount),
            capacity: 1_000,
            accepts: Rc::new(|fluid| &*fluid.id == "minecraft:lava"),
            can_insert: true,
            can_extract: false,
            creative: false,
            increment: 1,
        }
    }

    /// O caldeirão (`CauldronWrapper`): um balde de capacidade, água e lava, e só passos inteiros (`increment` 1.000).
    fn cauldron(resource: Option<FluidResource>, amount: u64) -> ResourceSnapshot<FluidResource> {
        ResourceSnapshot {
            contents: StackContents::new(resource, amount),
            capacity: 1_000,
            accepts: Rc::new(|fluid| matches!(&*fluid.id, "minecraft:water" | "minecraft:lava")),
            can_insert: true,
            can_extract: true,
            creative: false,
            increment: 1_000,
        }
    }

    #[test]
    fn a_cauldron_only_moves_whole_buckets() {
        // Vazio: 1.500 de água entram como um balde; 600 não completam o passo.
        let handler = HostHandler::new(cauldron(None, 0));
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&water(), 600, &transaction), 0);
        assert_eq!(handler.insert(&water(), 1_500, &transaction), 1_000);
        transaction.commit();
        assert_eq!(
            handler.changed(),
            Some(StackContents::new(Some(water()), 1_000))
        );
        // Cheio: tira o balde inteiro, nunca um pedaço, e a lava não entra na água.
        let handler = HostHandler::new(cauldron(Some(water()), 1_000));
        {
            let transaction = Transaction::open_root();
            assert_eq!(handler.insert(&lava(), 1_000, &transaction), 0);
            assert_eq!(handler.extract(&water(), 400, &transaction), 0);
            assert_eq!(handler.extract(&water(), 4_000, &transaction), 1_000);
        }
        // Um nível só (333): não completa um balde, não sai.
        let handler = HostHandler::new(cauldron(Some(water()), 333));
        let transaction = Transaction::open_root();
        assert_eq!(handler.extract(&water(), 4_000, &transaction), 0);
    }

    #[test]
    fn the_handler_takes_what_fits_and_only_the_fluid_the_tank_accepts() {
        let handler = HostHandler::new(lava_tank(900));
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&lava(), 250, &transaction), 100);
        assert_eq!(handler.insert(&water(), 250, &transaction), 0);
        transaction.commit();
        assert_eq!(
            handler.changed(),
            Some(StackContents::new(Some(lava()), 1_000))
        );
    }

    #[test]
    fn an_empty_lava_tank_still_refuses_water_by_its_validator() {
        let handler = HostHandler::new(ResourceSnapshot {
            contents: StackContents::default(),
            ..lava_tank(0)
        });
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&water(), 100, &transaction), 0);
        assert_eq!(handler.insert(&lava(), 100, &transaction), 100);
    }

    #[test]
    fn a_reverted_transaction_changes_nothing_in_the_game() {
        let handler = HostHandler::new(lava_tank(0));
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&lava(), 400, &transaction), 400);
        drop(transaction);
        assert_eq!(handler.changed(), None);
    }

    #[test]
    fn nobody_from_outside_takes_fluid_out_of_an_input_tank() {
        let handler = HostHandler::new(lava_tank(500));
        let transaction = Transaction::open_root();
        assert_eq!(handler.extract(&lava(), 100, &transaction), 0);
        transaction.commit();
        assert_eq!(handler.changed(), None);
    }

    #[test]
    fn a_tank_that_gives_is_drained_by_the_network_and_settles() {
        let mut snapshot = lava_tank(500);
        snapshot.can_extract = true;
        let handler = HostHandler::new(snapshot);
        let transaction = Transaction::open_root();
        assert_eq!(handler.extract(&lava(), 300, &transaction), 300);
        transaction.commit();
        assert_eq!(
            handler.changed(),
            Some(StackContents::new(Some(lava()), 200))
        );
        handler.settle();
        assert_eq!(handler.changed(), None);
    }

    #[test]
    fn a_creative_tank_answers_without_changing() {
        let mut snapshot = lava_tank(500);
        snapshot.can_extract = true;
        snapshot.creative = true;
        let handler = HostHandler::new(snapshot);
        let transaction = Transaction::open_root();
        assert_eq!(handler.extract(&lava(), 700, &transaction), 500);
        assert_eq!(handler.insert(&lava(), 100, &transaction), 100);
        transaction.commit();
        assert_eq!(handler.changed(), None);
        assert_eq!(handler.contents().amount, 500);
    }

    #[test]
    fn a_refresh_takes_what_the_game_did_in_between() {
        let handler = HostHandler::new(lava_tank(100));
        handler.refresh(lava_tank(800));
        assert_eq!(handler.contents().amount, 800);
        assert_eq!(handler.changed(), None);
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&lava(), 500, &transaction), 200);
        transaction.commit();
        assert_eq!(handler.contents().amount, 1_000);
    }

    #[test]
    fn a_closed_snapshot_neither_takes_nor_gives() {
        let handler = HostHandler::new(ResourceSnapshot::closed());
        let transaction = Transaction::open_root();
        assert_eq!(handler.insert(&lava(), 100, &transaction), 0);
        assert_eq!(handler.extract(&lava(), 100, &transaction), 0);
    }
}
