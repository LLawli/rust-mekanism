//! O contêiner de itens de um vizinho de um transportador (um baú, um funil, uma máquina com slots), visto como o transportador o usa: um `ResourceHandler<ItemResource>` que o
//! jogo não deixa desfazer. O Horizon insere e tira na hora (`native_insert_item`, `native_extract_item`) e a lib não tem o jogo na mão quando simula uma inserção (a busca de
//! caminho, `getPredictedInsert`, o puxar), então o handler é um espelho (a decisão 137 fez isso com o fluido): uma cópia dos slots que o lado alcança, lida no começo do evento
//! ([`HostItemHandler::refresh`]), sobre a qual a lib faz as transações do original; o fim do evento aplica no jogo o que mudou ([`HostItemHandler::changes`]).
use crate::containers::ItemResource;
use crate::item_handler::{ItemHandler, VanillaContainer};
use crate::transaction::Transaction;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

/// Um slot do contêiner do jogo que o lado alcança.
#[derive(Clone, Debug)]
pub struct MirrorSlot {
    /// O índice do slot no jogo (`native_container_slots`).
    pub index: usize,
    /// O que ele guarda.
    pub contents: Option<(ItemResource, u32)>,
    /// O jogo deixa tirar do slot por este lado (`can_take`).
    pub can_take: bool,
    /// Os recursos que o jogo recusa neste slot (`isValid` do slot do bloco, a regra da máquina para a entrada). O Horizon não simula a inserção, então quem lê o contêiner
    /// pergunta de antemão pelos itens que a rede pode oferecer (os em trânsito e os que os vizinhos guardam).
    pub refuses: Vec<ItemResource>,
}

/// A regra de slot do contêiner do bloco `block` (`Container.canPlaceItem`) só depende do item e do slot, nunca do que os slots guardam nem de outro estado do block entity? Vale para o baú, o
/// baú preso, o barril, o funil, o lançador e o dispensador (`canPlaceItem` do `Container`: sempre `true`) e para a caixa de shulker (`ShulkerBoxBlockEntity.canPlaceItemThroughFace`: só
/// o item). Não vale para o forno (`AbstractFurnaceBlockEntity.canPlaceItem`: o slot do combustível olha o balde que já está nele), a bancada de poções (`BrewingStandBlockEntity.canPlaceItem`:
/// o slot tem de estar vazio), o toca-discos, a estante de livros e o crafter (os slots desligados), nem para uma máquina de mod: a regra deles muda com o conteúdo.
pub fn static_slot_rule(block: &str) -> bool {
    matches!(
        block,
        "minecraft:chest"
            | "minecraft:trapped_chest"
            | "minecraft:barrel"
            | "minecraft:hopper"
            | "minecraft:dropper"
            | "minecraft:dispenser"
    ) || block == "minecraft:shulker_box"
        || (block.starts_with("minecraft:") && block.ends_with("_shulker_box"))
}

/// O que cada contêiner recusa dos itens oferecidos, guardado enquanto vale: só para o bloco de regra estática ([`static_slot_rule`]), e só enquanto o bloco é o mesmo, o número de slots é o
/// mesmo e quem o usa o esvazia quando a lista de itens oferecidos muda ([`Self::clear`]). Um bloco de regra que muda com o conteúdo nunca entra: a pergunta é refeita a cada vez.
pub struct RefusalMemo<K> {
    entries: HashMap<K, (String, Vec<Vec<ItemResource>>)>,
}

impl<K: Hash + Eq> Default for RefusalMemo<K> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<K: Hash + Eq> RefusalMemo<K> {
    /// O que se guardou para `key`, se ainda vale para o bloco `block` com `slots` slots.
    pub fn get(&self, key: &K, block: &str, slots: usize) -> Option<&Vec<Vec<ItemResource>>> {
        let (cached_block, refuses) = self.entries.get(key)?;
        (static_slot_rule(block) && cached_block == block && refuses.len() == slots)
            .then_some(refuses)
    }

    /// Guarda o que `block` recusou, se a regra dele é estática; senão não guarda nada.
    pub fn put(&mut self, key: K, block: &str, refuses: Vec<Vec<ItemResource>>) {
        if static_slot_rule(block) {
            self.entries.insert(key, (block.to_owned(), refuses));
        }
    }

    /// Esquece tudo (os itens oferecidos mudaram).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// O que mudou num slot, para o jogo aplicar. Primeiro as retiradas, depois as colocações.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotChange {
    /// Tira `count` do slot `index` do jogo.
    Take { index: usize, count: u32 },
    /// Põe `count` do recurso no contêiner (o jogo escolhe o slot, como um funil).
    Put { resource: ItemResource, count: u32 },
}

/// O espelho de um contêiner de itens do jogo.
pub struct HostItemHandler {
    container: RefCell<Rc<VanillaContainer>>,
    /// O índice do jogo de cada slot do espelho.
    index: RefCell<Vec<usize>>,
    can_take: RefCell<Vec<bool>>,
    refuses: RefCell<Vec<Vec<ItemResource>>>,
    /// O que cada slot do espelho guardava quando foi lido.
    original: RefCell<Vec<Option<(ItemResource, u32)>>>,
}

impl HostItemHandler {
    pub fn new(slots: Vec<MirrorSlot>) -> Self {
        let handler = Self {
            container: RefCell::new(VanillaContainer::new(0, Rc::new(|| {}))),
            index: RefCell::new(Vec::new()),
            can_take: RefCell::new(Vec::new()),
            refuses: RefCell::new(Vec::new()),
            original: RefCell::new(Vec::new()),
        };
        handler.refresh(slots);
        handler
    }

    /// Lê o contêiner de novo: o espelho passa a ter o que o jogo tem agora.
    pub fn refresh(&self, slots: Vec<MirrorSlot>) {
        let rebuild = self.container.borrow().size() != slots.len();
        if rebuild {
            *self.container.borrow_mut() = VanillaContainer::new(slots.len(), Rc::new(|| {}));
        }
        let container = self.container.borrow();
        for (mirror, slot) in slots.iter().enumerate() {
            match &slot.contents {
                Some((resource, amount)) => {
                    container.set_item(mirror, Some(resource.clone()), *amount)
                }
                None => container.set_item(mirror, None, 0),
            }
        }
        *self.index.borrow_mut() = slots.iter().map(|slot| slot.index).collect();
        *self.can_take.borrow_mut() = slots.iter().map(|slot| slot.can_take).collect();
        *self.refuses.borrow_mut() = slots.iter().map(|slot| slot.refuses.clone()).collect();
        *self.original.borrow_mut() = slots.into_iter().map(|slot| slot.contents).collect();
    }

    /// A leitura do meio de um evento (`getCapability` pelo `BlockCapabilityCache`, que no original devolve sempre o mesmo objeto, vivo): o espelho relê o jogo só se o evento ainda não
    /// mudou nada nele. Com uma mudança pendente (uma entrega que o fim do evento ainda vai aplicar) a releitura a desfaria, e a segunda pilha a chegar no mesmo tick ao mesmo baú faria a
    /// primeira sumir.
    pub fn refresh_if_clean(&self, slots: Vec<MirrorSlot>) {
        if self.changes().is_empty() {
            self.refresh(slots);
        }
    }

    /// O que o evento mudou no espelho, para o jogo aplicar: as retiradas e depois as colocações, na ordem dos slots. Vazio se nada mudou.
    pub fn changes(&self) -> Vec<SlotChange> {
        let container = self.container.borrow();
        let index = self.index.borrow();
        let original = self.original.borrow();
        let mut takes = Vec::new();
        let mut puts = Vec::new();
        for (mirror, before) in original.iter().enumerate() {
            let now = container
                .resource(mirror)
                .map(|resource| (resource, container.amount(mirror)));
            if *before == now {
                continue;
            }
            match (before, now) {
                (Some((resource, was)), Some((now_resource, now_amount)))
                    if *resource == now_resource =>
                {
                    if now_amount < *was {
                        takes.push(SlotChange::Take {
                            index: index[mirror],
                            count: was - now_amount,
                        });
                    } else {
                        puts.push(SlotChange::Put {
                            resource: now_resource,
                            count: now_amount - was,
                        });
                    }
                }
                (before, now) => {
                    if let Some((_, was)) = before {
                        takes.push(SlotChange::Take {
                            index: index[mirror],
                            count: *was,
                        });
                    }
                    if let Some((resource, amount)) = now {
                        puts.push(SlotChange::Put {
                            resource,
                            count: amount,
                        });
                    }
                }
            }
        }
        takes.extend(puts);
        takes
    }

    /// O jogo aplicou as mudanças: o espelho é o novo ponto de partida.
    pub fn settle(&self) {
        let container = self.container.borrow();
        *self.original.borrow_mut() = (0..container.size())
            .map(|mirror| {
                container
                    .resource(mirror)
                    .map(|resource| (resource, container.amount(mirror)))
            })
            .collect();
    }
}

impl ItemHandler for HostItemHandler {
    fn size(&self) -> usize {
        self.container.borrow().size()
    }

    fn resource(&self, index: usize) -> Option<ItemResource> {
        self.container.borrow().resource(index)
    }

    fn amount(&self, index: usize) -> u32 {
        self.container.borrow().amount(index)
    }

    fn insert_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        // O slot da máquina que o jogo recusa para este item (`isValid`) não recebe.
        if self
            .refuses
            .borrow()
            .get(index)
            .is_some_and(|refused| refused.contains(resource))
        {
            return 0;
        }
        self.container
            .borrow()
            .insert_at(index, resource, amount, transaction)
    }

    fn extract_at(
        &self,
        index: usize,
        resource: &ItemResource,
        amount: u32,
        transaction: &Transaction,
    ) -> u32 {
        // O jogo só deixa tirar do slot que o lado alcança e que aceita sair (`can_take`).
        if !self.can_take.borrow().get(index).copied().unwrap_or(false) {
            return 0;
        }
        self.container
            .borrow()
            .extract_at(index, resource, amount, transaction)
    }
}
