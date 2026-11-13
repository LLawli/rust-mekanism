//! O contêiner de itens de um vizinho de um transportador (um baú, um funil, uma máquina com slots), visto como o transportador o usa: um `ResourceHandler<ItemResource>` que o
//! jogo não deixa desfazer. O Horizon insere e tira na hora (`native_insert_item`, `native_extract_item`) e a lib não tem o jogo na mão quando simula uma inserção (a busca de
//! caminho, `getPredictedInsert`, o puxar), então o handler é um espelho (a decisão 137 fez isso com o fluido): uma cópia dos slots que o lado alcança, lida no começo do evento
//! ([`HostItemHandler::refresh`]), sobre a qual a lib faz as transações do original; o fim do evento aplica no jogo o que mudou ([`HostItemHandler::changes`]).
use crate::containers::ItemResource;
use crate::item_handler::{ItemHandler, VanillaContainer};
use crate::transaction::Transaction;
use std::cell::RefCell;
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
    /// O que cada slot do espelho guardava quando foi lido.
    original: RefCell<Vec<Option<(ItemResource, u32)>>>,
}

impl HostItemHandler {
    pub fn new(slots: Vec<MirrorSlot>) -> Self {
        let handler = Self {
            container: RefCell::new(VanillaContainer::new(0, Rc::new(|| {}))),
            index: RefCell::new(Vec::new()),
            can_take: RefCell::new(Vec::new()),
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
        *self.original.borrow_mut() = slots.into_iter().map(|slot| slot.contents).collect();
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
