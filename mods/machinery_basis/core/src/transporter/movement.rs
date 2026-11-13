//! O movimento das pilhas num transportador logístico, tick a tick (`LogisticalTransporterBase`: `onUpdateServer`, `tryPull`,
//! `tickTransit`, `recalculate`, `drop`, `insert`, `insertUnchecked`, `emitStack`, `entityEntering`), o NBT e o pacote dele.
//!
//! # O que o original faz e como a lib o guarda
//!
//! O original guarda as pilhas em trânsito num `Int2ObjectOpenHashMap<TransporterStack>` (`transit`), as que precisam ir ao cliente
//! noutro (`needsSync`) e três contadores (`nextId`, `delay`, `delayCount`). Aqui o mesmo é o [`TransitState`], numa célula dentro do
//! `LogisticalTransporterBase`: o `tickTransit` do vizinho chama `entityEntering` neste, então o estado muda por referência
//! compartilhada, como o objeto Java. A ordem em que `tickTransit` e `write` percorrem as pilhas é a da tabela do fastutil
//! (`sim::open_hash`), com as chaves inteiras de `nextId`.
//!
//! O `emitStack` de uma inserção dentro de transação só roda no commit da raiz (`RootCommitJournal`), e o `drop` só solta os itens
//! no mundo nesse mesmo ponto (`DroppedItems`); aqui os dois passam pelo `transaction.rs`. Os pacotes (`PacketTransporterSync`,
//! `PacketTransporterBatch`) e o `markForSave` ficam em [`TransitState::events`], e o que cai no mundo em
//! [`LogisticalTransporterBase::take_world_drops`], para quem liga o jogo consumir.
use super::nbt::Nbt;
use super::node::LogisticalTransporterBase;
use super::pathfinder::{PathContext, RoundRobinEjector};
use super::pos::{NO_POS, relative_pos, side_difference};
use super::stack::{ItemDecode, ItemEncode, StackRef, TransporterStack, plain_item_encode};
use super::world::{InventoryNetworkView, OutputterTile, Transporter, can_insert};
use crate::containers::ItemResource;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::sim::open_hash::{OpenHashMap, OpenHashSet, OpenKey, mix_int};
use crate::transaction::{Callback, JournalCell, Journaled, RootCommitJournal, Transaction};
use crate::transit::{TransitRequest, TransitResponse};
use crate::transmission::connection::ConnectionType;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// `SerializationConstants.ITEMS` e `INDEX`.
pub const ITEMS_KEY: &str = "items";
pub const INDEX_KEY: &str = "index";

/// `MekanismUtils.TICKS_PER_HALF_SECOND`: a espera depois de um puxar que deu certo.
const PULL_DELAY: i32 = 10;
/// A espera de um tick sem puxar nada (`delay = 3`).
const IDLE_DELAY: i32 = 3;
/// `2 * SharedConstants.TICKS_PER_SECOND`: o teto da espera depois de uma falha.
const MAX_BACKOFF: i32 = 40;

/// A chave `int` de uma tabela do fastutil: `HashCommon.mix(int)`, e o zero é a chave nula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntKey(pub i32);

impl OpenKey for IntKey {
    fn mixed(self) -> i32 {
        mix_int(self.0)
    }

    const NULL_KEY: Option<Self> = Some(IntKey(0));
}

/// O que o transportador manda ao cliente ou ao disco, na ordem em que acontece.
#[derive(Clone, Debug)]
pub enum TransitEvent {
    /// `PacketTransporterSync.create(pos, stackId, stack)`, do `emitStack`: a pilha como estava quando o pacote saiu, com o `updateForPos(pos)` já feito.
    Sync { id: i32, stack: TransporterStack },
    /// `PacketTransporterBatch.create(pos, deletes, needsSync)`: as pilhas que saíram e as que mudaram (na ordem da tabela, cada uma como estava quando o pacote saiu, com o
    /// `updateForPos(pos)` feito).
    Batch {
        deletes: Vec<i32>,
        updates: Vec<(i32, TransporterStack)>,
    },
    /// `getTransmitterTile().markForSave()`.
    MarkForSave,
}

/// O que cai no mundo: um `Block.popResource` por pilha (as acima do limite do item saem em várias).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldDrop {
    pub pos: BlockPos,
    pub item: ItemResource,
    pub amount: u32,
}

/// As pilhas em trânsito de um transportador e os contadores do puxar.
#[derive(Default)]
pub struct TransitState {
    transit: OpenHashMap<IntKey, StackRef>,
    needs_sync: OpenHashMap<IntKey, StackRef>,
    next_id: i32,
    delay: i32,
    delay_count: i32,
    /// Os pacotes e avisos de salvar que o original mandaria, para quem liga o jogo.
    pub events: Vec<TransitEvent>,
}

impl TransitState {
    /// `addStack(id, stack)`.
    fn add_stack(&mut self, id: i32, stack: StackRef) {
        self.transit.insert(IntKey(id), stack);
    }

    /// As pilhas na ordem da tabela, com o id de cada uma (`transit.values()` com as chaves).
    pub fn stacks(&self) -> Vec<(i32, StackRef)> {
        self.transit
            .entries_in_order()
            .into_iter()
            .map(|(key, stack)| (key.0, stack.clone()))
            .collect()
    }

    /// As chaves de `needsSync` na ordem da tabela.
    pub fn pending_sync(&self) -> Vec<i32> {
        self.needs_sync
            .keys_in_order()
            .into_iter()
            .map(|key| key.0)
            .collect()
    }

    pub fn next_id(&self) -> i32 {
        self.next_id
    }

    pub fn delay(&self) -> i32 {
        self.delay
    }

    pub fn delay_count(&self) -> i32 {
        self.delay_count
    }

    /// O estado de um transportador que já existia (o rastro grava `delay`, `delayCount` e `nextId` antes do primeiro tick).
    pub fn restore_counters(&mut self, delay: i32, delay_count: i32, next_id: i32) {
        self.delay = delay;
        self.delay_count = delay_count;
        self.next_id = next_id;
    }

    pub fn is_empty(&self) -> bool {
        self.transit.is_empty()
    }
}

/// `TransporterManager`-parte do `DroppedItems`: uma entrada de `drop`.
struct DropInfo {
    resource: ItemResource,
    amount: u32,
    offset: [i32; 3],
}

/// `DroppedItems`: o que `drop` pediu para soltar e só cai no mundo quando a transação da raiz commita.
pub struct DroppedItems {
    entries: VecDeque<DropInfo>,
    pos: BlockPos,
    out: Rc<RefCell<Vec<WorldDrop>>>,
}

impl DroppedItems {
    pub(super) fn new(pos: BlockPos, out: Rc<RefCell<Vec<WorldDrop>>>) -> Rc<JournalCell<Self>> {
        JournalCell::new(Self {
            entries: VecDeque::new(),
            pos,
            out,
        })
    }
}

impl Journaled for DroppedItems {
    type Snapshot = usize;

    /// Cancela os `drop` desde o retrato.
    fn revert_to_snapshot(&mut self, snapshot: usize) {
        while self.entries.len() > snapshot {
            self.entries.pop_back();
        }
    }

    /// `onRootCommit`: solta de verdade, um por um, pelo `InventoryUtils.dropStack` (um `popResource` por pilha de até o limite do item).
    fn on_root_commit(&mut self, _original: usize) -> Vec<Callback> {
        while let Some(info) = self.entries.pop_front() {
            let at = BlockPos::new(
                self.pos.x + info.offset[0],
                self.pos.y + info.offset[1],
                self.pos.z + info.offset[2],
            );
            let max = info.resource.max_stack_size.max(1);
            let mut amount = info.amount;
            let mut out = self.out.borrow_mut();
            while amount > max {
                out.push(WorldDrop {
                    pos: at,
                    item: info.resource.clone(),
                    amount: max,
                });
                amount -= max;
            }
            if amount > 0 {
                out.push(WorldDrop {
                    pos: at,
                    item: info.resource.clone(),
                    amount,
                });
            }
        }
        Vec::new()
    }
}

/// O que o transportador lê do mundo e da rede a cada tick: `getLevel()` e `getTransmitterNetwork()`.
pub struct MoveContext<'a> {
    pub path: &'a PathContext<'a>,
    /// A rede do transportador, ou nada se ele não tem uma (`getTransmitterNetwork() == null`).
    pub network: Option<&'a dyn InventoryNetworkView>,
}

/// Quem pede a inserção (`outputter` e `outputterPos` de `insert`).
pub enum Outputter<'a> {
    /// `outputter == null` (o puxar do próprio transportador).
    None,
    /// Um bloco qualquer.
    Tile(OutputterTile),
    /// Um `IAdvancedTransportEjector`: com o round robin ligado, o destino segue o ejetor.
    Ejector {
        tile: OutputterTile,
        round_robin: bool,
        ejector: &'a mut dyn RoundRobinEjector,
    },
}

/// `Math.floor(float)` (`Mth.floor`).
fn floor(value: f32) -> i32 {
    let truncated = value as i32;
    if value < truncated as f32 {
        truncated - 1
    } else {
        truncated
    }
}

/// `TransporterUtils.getStackPosition(transporter, stack, partial)`, em `float` como o Java: o ponto do bloco onde a pilha está.
pub fn stack_position(side: Face, progress: i32, partial: f32) -> [f32; 3] {
    let step = side.offset();
    let scale = ((progress as f32 + partial) / 100.0_f32) - 0.5_f32;
    [
        step[0] as f32 * scale + 0.5_f32,
        step[1] as f32 * scale + 0.25_f32,
        step[2] as f32 * scale + 0.5_f32,
    ]
}

/// O deslocamento do bloco em que `drop` solta a pilha: `Mth.floor` de cada coordenada de `getStackPosition(transporter, stack, 0)`.
pub fn stack_block_offset(side: Face, progress: i32) -> [i32; 3] {
    let position = stack_position(side, progress, 0.0);
    [floor(position[0]), floor(position[1]), floor(position[2])]
}

/// `LogisticalTransporterUpgradeData`, no que é do movimento: as pilhas em trânsito e pendentes, `nextId`, `delay` e `delayCount`.
pub struct TransitUpgrade {
    pub transit: Vec<(i32, StackRef)>,
    pub needs_sync: Vec<(i32, StackRef)>,
    pub next_id: i32,
    pub delay: i32,
    pub delay_count: i32,
}

/// `emitStack`: a pilha entra em trânsito com um id novo e o cliente é avisado.
fn emit_stack(transit: &Rc<RefCell<TransitState>>, pos: i64, stack: StackRef) {
    let mut state = transit.borrow_mut();
    let id = state.next_id;
    state.next_id += 1;
    state.add_stack(id, stack.clone());
    // `PacketTransporterSync.create`: `stack.updateForPos(pos)` e o pacote leva o que a pilha é agora.
    let mut sent = stack.borrow().clone();
    sent.update_for_pos(pos);
    state.events.push(TransitEvent::Sync { id, stack: sent });
    state.events.push(TransitEvent::MarkForSave);
}

impl LogisticalTransporterBase {
    fn pos_long(&self) -> i64 {
        self.block_pos().as_long()
    }

    /// `onUpdateServer(level)`: puxa a cada tantos ticks e anda as pilhas em trânsito.
    pub fn on_update_server(&self, ctx: &MoveContext) {
        let Some(network) = ctx.network else {
            return;
        };
        // Puxa para dentro do transportador.
        let pull_now = {
            let mut state = self.transit.borrow_mut();
            if state.delay > 0 {
                state.delay -= 1;
                false
            } else {
                // Se não houver o que inserir, tenta de novo em 3 ticks.
                state.delay = IDLE_DELAY;
                true
            }
        };
        if pull_now {
            self.try_pull(ctx);
        }
        if self.transit.borrow().transit.is_empty() {
            return;
        }
        let pos = self.pos_long();
        let deletes = {
            let transaction = Transaction::open_root();
            let deletes = self.tick_transit(ctx, pos, network, &transaction);
            transaction.commit();
            deletes
        };
        let mut state = self.transit.borrow_mut();
        if !deletes.is_empty() || !state.needs_sync.is_empty() {
            let deleted: Vec<i32> = deletes.in_order().into_iter().map(|key| key.0).collect();
            // `PacketTransporterBatch.create`: `updateForPos(pos)` em cada pilha de `needsSync` (a cópia do mapa guarda as mesmas pilhas).
            let updates: Vec<(i32, TransporterStack)> = state
                .needs_sync
                .entries_in_order()
                .into_iter()
                .map(|(key, stack)| {
                    let mut sent = stack.borrow().clone();
                    sent.update_for_pos(pos);
                    (key.0, sent)
                })
                .collect();
            state.events.push(TransitEvent::Batch {
                deletes: deleted.clone(),
                updates,
            });
            for id in deleted {
                state.transit.remove(IntKey(id));
            }
            state.needs_sync.clear_keeping_table();
            state.events.push(TransitEvent::MarkForSave);
        }
    }

    /// `tryPull(level)`.
    fn try_pull(&self, ctx: &MoveContext) {
        for side in Face::ALL {
            if !self
                .transmitter
                .is_connection_type(side, ConnectionType::Pull)
            {
                continue;
            }
            // `getCapForSide`: a capability de item do vizinho, pela face voltada para o transportador.
            let Some(inventory) = ctx
                .path
                .world
                .item_handler(relative_pos(self.pos_long(), side), side.opposite())
            else {
                continue;
            };
            let transaction = Transaction::open_root();
            let request = TransitRequest::any_item(
                &inventory,
                self.tier.pull_amount() as u32,
                Some(&transaction),
            );
            if request.is_empty() {
                continue;
            }
            let response = self.insert(
                ctx,
                Outputter::None,
                relative_pos(self.pos_long(), side),
                &request,
                self.get_color(),
                0,
                Some(&transaction),
            );
            if response.use_all(&transaction) {
                // Deu certo: tira o que entrou e tenta de novo em 10 ticks.
                transaction.commit();
                let mut state = self.transit.borrow_mut();
                state.delay = PULL_DELAY;
                state.delay_count = 0;
            } else {
                // A inserção ou a extração falhou: recua, com o teto de 40 ticks (quatro tentativas seguidas).
                drop(transaction);
                let mut state = self.transit.borrow_mut();
                state.delay_count += 1;
                state.delay = MAX_BACKOFF.min((f64::from(state.delay_count)).exp() as i32);
            }
        }
    }

    /// `tickTransit(level, pos, network, transaction)`: anda cada pilha e devolve os ids que saem de `transit`.
    fn tick_transit(
        &self,
        ctx: &MoveContext,
        pos: i64,
        network: &dyn InventoryNetworkView,
        transaction: &Transaction,
    ) -> OpenHashSet<IntKey> {
        let mut deletes = OpenHashSet::new();
        let stacks = self.transit.borrow().stacks();
        for (stack_id, stack) in stacks {
            if !stack.borrow().initiated_path {
                // Inicia os caminhos e tira o que não tem para onde ir.
                let empty = stack.borrow().is_empty();
                if empty || !self.recalculate(ctx, stack_id, &stack, NO_POS, transaction) {
                    deletes.insert(IntKey(stack_id));
                    continue;
                }
            }
            let prev_progress = stack.borrow().progress;
            stack.borrow_mut().progress += self.tier.speed();
            if stack.borrow().progress >= 100 {
                let mut prev_set = NO_POS;
                if stack.borrow().has_path() {
                    if stack.borrow().path()[0] == pos {
                        // Necessário por causa das transições (o original não sabe por quê).
                        deletes.insert(IntKey(stack_id));
                        continue;
                    }
                    let (next, is_final, path_type, side) = {
                        let state = stack.borrow();
                        let current_index = state
                            .path()
                            .iter()
                            .position(|candidate| *candidate == pos)
                            .expect("o caminho da pilha passa pelo transportador onde ela está");
                        (
                            state.path()[current_index - 1],
                            state.is_final(pos),
                            state.path_type(),
                            state.get_side(pos),
                        )
                    };
                    if next != NO_POS {
                        if !is_final {
                            // Não é o último transportador: tenta passar a pilha ao próximo.
                            let peer = network.transmitter(next);
                            let can_insert = stack.borrow().can_insert_to_transporter(
                                peer,
                                side,
                                Some(self as &dyn Transporter),
                            );
                            if can_insert
                                && let Some(logistical) = peer.and_then(Transporter::as_logistical)
                            {
                                let progress = stack.borrow().progress % 100;
                                logistical.entity_entering(stack.clone(), progress);
                                deletes.insert(IntKey(stack_id));
                                continue;
                            }
                            prev_set = next;
                        } else if path_type.has_target() {
                            // Senão, tenta pôr no inventário do destino, pelo aceitador que a rede guardou.
                            let acceptor_side = side.opposite();
                            let mut acceptor = cached_acceptor(network, next, acceptor_side);
                            if acceptor.is_none() && path_type.is_home() {
                                acceptor = ctx.path.world.item_handler(next, acceptor_side);
                            }
                            let request = {
                                let state = stack.borrow();
                                TransitRequest::simple(
                                    state.item_type().expect("pilha em trânsito sem item"),
                                    state.size(),
                                )
                            };
                            let response = request.add_to_inventory(
                                ctx.path.world,
                                next,
                                acceptor.as_ref(),
                                0,
                                path_type.is_home(),
                                transaction,
                            );
                            if !response.is_empty() {
                                // Coube ao menos parte da pilha.
                                let rejected = response.rejected();
                                if rejected == 0 {
                                    // Coube tudo: sai das pilhas em voo e da lista.
                                    ctx.path.manager.remove(
                                        ctx.path.dimension,
                                        &stack,
                                        Some(transaction),
                                    );
                                    deletes.insert(IntKey(stack_id));
                                    continue;
                                }
                                // Sobrou uma parte: guarda o resto e recalcula abaixo.
                                let item = response.item_type().cloned();
                                stack.borrow_mut().set_stack(item.as_ref(), rejected);
                            }
                            // Senão a pilha inteira foi recusada.
                            prev_set = next;
                        }
                    }
                }
                if !self.recalculate(ctx, stack_id, &stack, prev_set, transaction) {
                    deletes.insert(IntKey(stack_id));
                } else if prev_set == NO_POS {
                    stack.borrow_mut().progress = 50;
                } else {
                    stack.borrow_mut().progress = 0;
                }
            } else if prev_progress < 50 && stack.borrow().progress >= 50 {
                let try_recalculate =
                    self.must_recalculate_at_half(ctx, pos, network, &stack, transaction);
                if try_recalculate && !self.recalculate(ctx, stack_id, &stack, NO_POS, transaction)
                {
                    deletes.insert(IntKey(stack_id));
                }
            }
        }
        deletes
    }

    /// O bloco do `tickTransit` em que a pilha passa dos 50: se o caminho que ela tem ainda vale.
    fn must_recalculate_at_half(
        &self,
        ctx: &MoveContext,
        pos: i64,
        network: &dyn InventoryNetworkView,
        stack: &StackRef,
        transaction: &Transaction,
    ) -> bool {
        let state = stack.borrow();
        if state.is_final(pos) {
            let path_type = state.path_type();
            if path_type.has_target() {
                let side = state.get_side(pos);
                let connection_type = self.connection_type(side);
                !connection_type.can_send_to()
                    || !can_insert(
                        ctx.path.world,
                        state.get_dest(),
                        state.color,
                        state.item_type().expect("pilha em trânsito sem item"),
                        state.size(),
                        side,
                        path_type.is_home(),
                        Some(transaction),
                    )
            } else {
                // Os ociosos tentam de novo ao chegar ao destino.
                true
            }
        } else {
            let next_pos = state.get_next(pos);
            if next_pos == NO_POS {
                true
            } else {
                let next_side = TransporterStack::get_side_between(pos, next_pos);
                let next_transmitter = network.transmitter(next_pos);
                if next_transmitter.is_none()
                    && state.path_type().no_target()
                    && state.path().len() == 2
                {
                    // Sem próximo transportador e um caminho ocioso de dois: o passeio é dentro de um transportador só, que só recalcula
                    // aos 50 se não puder sair por aquela conexão.
                    !self.connection_type(next_side).can_send_to()
                } else {
                    !state.can_insert_to_transporter(
                        next_transmitter,
                        next_side,
                        Some(self as &dyn Transporter),
                    )
                }
            }
        }
    }

    /// `recalculate(level, stackId, stack, from, transaction)`: o caminho novo, ou um ocioso, ou solta a pilha. Falso se a pilha sai.
    fn recalculate(
        &self,
        ctx: &MoveContext,
        stack_id: i32,
        stack: &StackRef,
        from: i64,
        transaction: &Transaction,
    ) -> bool {
        let no_path = stack.borrow().path_type().no_target() || {
            let request = {
                let state = stack.borrow();
                TransitRequest::simple(
                    state.item_type().expect("pilha em trânsito sem item"),
                    state.size(),
                )
            };
            stack
                .recalculate_path(
                    ctx.path,
                    ctx.network,
                    self.block_pos(),
                    &request,
                    0,
                    Some(transaction),
                )
                .is_empty()
        };
        if no_path
            && !stack.calculate_idle(ctx.path, ctx.network, self.block_pos(), Some(transaction))
        {
            self.drop_stack(ctx, stack, Some(transaction));
            return false;
        }
        // Só entra em `needsSync` se devolve verdadeiro; senão vai para `deletes`.
        self.transit
            .borrow_mut()
            .needs_sync
            .insert(IntKey(stack_id), stack.clone());
        if from != NO_POS {
            stack.borrow_mut().original_location = from;
        }
        true
    }

    /// `drop(level, stack, transaction)`: tira a pilha das em voo e a solta no mundo quando a transação da raiz commita.
    pub fn drop_stack(
        &self,
        ctx: &MoveContext,
        stack: &StackRef,
        transaction: Option<&Transaction>,
    ) {
        if stack.borrow().is_empty() {
            // Pilhas vazias que chegam aqui por algum motivo são ignoradas.
            return;
        }
        let mut offset = [0, 0, 0];
        {
            let state = stack.borrow();
            if state.has_path() {
                offset = stack_block_offset(state.get_side(self.pos_long()), state.progress);
            }
        }
        let sub = Transaction::open(transaction);
        ctx.path
            .manager
            .remove(ctx.path.dimension, stack, Some(&sub));
        let (resource, amount) = {
            let state = stack.borrow();
            (
                state.item_type().expect("pilha sem item").clone(),
                state.size(),
            )
        };
        self.dropped
            .update_snapshots(&sub, || self.dropped.borrow().entries.len());
        self.dropped.borrow_mut().entries.push_back(DropInfo {
            resource,
            amount,
            offset,
        });
        sub.commit();
    }

    /// `TileEntityLogisticalTransporterBase.preRemoveSideEffects`: solta as pilhas em trânsito, a menos que o transportador esteja sendo
    /// trocado de nível (`isUpgrading`).
    pub fn pre_remove_side_effects(&self, ctx: &MoveContext) {
        if self.transmitter.is_upgrading() {
            return;
        }
        let transaction = Transaction::open_root();
        let stacks = self.transit.borrow().stacks();
        for (_, stack) in stacks {
            self.drop_stack(ctx, &stack, Some(&transaction));
        }
        transaction.commit();
    }

    /// `remove()`: tira as pilhas em trânsito das em voo do `TransporterManager`.
    pub fn remove_transit(&self, ctx: &PathContext) {
        let stacks = self.transit.borrow().stacks();
        for (_, stack) in stacks {
            ctx.manager.remove(ctx.dimension, &stack, None);
        }
    }

    /// `insert(level, outputter, outputterPos, request, color, min, transaction)`: o que vizinhos e ejetores chamam para pôr itens aqui.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &self,
        ctx: &MoveContext,
        outputter: Outputter,
        outputter_pos: i64,
        request: &TransitRequest,
        color: Option<u8>,
        min: u32,
        transaction: Option<&Transaction>,
    ) -> TransitResponse {
        let Some(from) = side_difference(self.pos_long(), outputter_pos) else {
            return TransitResponse::EMPTY;
        };
        if !self.can_receive_from(from.opposite()) {
            return TransitResponse::EMPTY;
        }
        let stack = StackRef::new(TransporterStack::for_insert(outputter_pos, color));
        let (tile, round_robin) = match outputter {
            Outputter::None => (OutputterTile::Unknown, None),
            Outputter::Tile(tile) => (tile, None),
            Outputter::Ejector {
                tile,
                round_robin,
                ejector,
            } => (tile, round_robin.then_some(ejector)),
        };
        let allowed = stack.borrow().can_insert_to_transporter_tile(
            Some(self as &dyn Transporter),
            from,
            tile,
            self.pos_long(),
            ctx.path.world,
        );
        if !allowed {
            return TransitResponse::EMPTY;
        }
        match round_robin {
            Some(ejector) => {
                self.insert_unchecked_with(ctx, request, &stack, min, transaction, |stack| {
                    stack.recalculate_rr_path(
                        ctx.path,
                        ctx.network,
                        self.block_pos(),
                        request,
                        &mut *ejector,
                        min,
                        transaction,
                    )
                })
            }
            None => self.insert_unchecked_with(ctx, request, &stack, min, transaction, |stack| {
                stack.recalculate_path(
                    ctx.path,
                    ctx.network,
                    self.block_pos(),
                    request,
                    min,
                    transaction,
                )
            }),
        }
    }

    /// `insertUnchecked(level, outputterPos, type, amountToInsert, transaction)`: o que o `TransporterItemHandler` chama, com o caminho
    /// comum, o mínimo 1 e a cor do transportador.
    pub fn insert_unchecked_from(
        &self,
        ctx: &MoveContext,
        outputter_pos: i64,
        resource: &ItemResource,
        amount: u32,
        transaction: Option<&Transaction>,
    ) -> u32 {
        let request = TransitRequest::simple(resource, amount);
        let stack = StackRef::new(TransporterStack::for_insert(
            outputter_pos,
            self.get_color(),
        ));
        self.insert_unchecked_with(ctx, &request, &stack, 1, transaction, |stack| {
            stack.recalculate_path(
                ctx.path,
                ctx.network,
                self.block_pos(),
                &request,
                1,
                transaction,
            )
        })
        .sending_amount()
    }

    /// O `insertUnchecked(level, outputter, request, stack, min, transaction, pathCalculator)` privado: calcula o caminho e, se achou, a
    /// pilha entra em trânsito (já, sem transação; ao fim da raiz, com ela).
    fn insert_unchecked_with(
        &self,
        _ctx: &MoveContext,
        _request: &TransitRequest,
        stack: &StackRef,
        _min: u32,
        transaction: Option<&Transaction>,
        calculate: impl FnOnce(&StackRef) -> TransitResponse,
    ) -> TransitResponse {
        let response = calculate(stack);
        if !response.is_empty() {
            stack
                .borrow_mut()
                .set_stack(response.item_type(), response.sending_amount());
            match transaction {
                // Sem transação, a pilha sai direto.
                None => emit_stack(&self.transit, self.pos_long(), stack.clone()),
                // Com ela, espera o commit da raiz.
                Some(transaction) => {
                    let transit = self.transit.clone();
                    let stack = stack.clone();
                    let pos = self.pos_long();
                    let on_root_commit = RootCommitJournal::new(Rc::new(move || {
                        emit_stack(&transit, pos, stack.clone());
                    }));
                    on_root_commit.touch(transaction);
                }
            }
        }
        response
    }

    /// `entityEntering(stack, progress)`: uma pilha que vem de um transportador vizinho. O envio ao cliente vai em lote, no `needsSync`.
    pub fn entity_entering(&self, stack: StackRef, progress: i32) {
        let mut state = self.transit.borrow_mut();
        let id = state.next_id;
        state.next_id += 1;
        stack.borrow_mut().progress = progress;
        state.add_stack(id, stack.clone());
        state.needs_sync.insert(IntKey(id), stack);
        // O original não marca o chunk como sujo aqui: o tick seguinte já salva.
    }

    /// O que caiu no mundo desde a última chamada, na ordem em que caiu.
    pub fn take_world_drops(&self) -> Vec<WorldDrop> {
        std::mem::take(&mut *self.world_drops.borrow_mut())
    }

    /// Os pacotes e avisos de salvar desde a última chamada.
    pub fn take_events(&self) -> Vec<TransitEvent> {
        std::mem::take(&mut self.transit.borrow_mut().events)
    }

    /// `getTransit()`: as pilhas em trânsito na ordem da tabela, com o id de cada uma.
    pub fn transit_stacks(&self) -> Vec<(i32, StackRef)> {
        self.transit.borrow().stacks()
    }

    /// Os ids em `needsSync`, na ordem da tabela.
    pub fn pending_sync(&self) -> Vec<i32> {
        self.transit.borrow().pending_sync()
    }

    /// Os contadores de `delay`, `delayCount` e `nextId`.
    pub fn counters(&self) -> (i32, i32, i32) {
        let state = self.transit.borrow();
        (state.delay, state.delay_count, state.next_id)
    }

    /// O estado de um transportador que já andou (o rastro grava os contadores antes do primeiro tick).
    pub fn restore_counters(&self, delay: i32, delay_count: i32, next_id: i32) {
        self.transit
            .borrow_mut()
            .restore_counters(delay, delay_count, next_id);
    }

    /// `getUpgradeData()` do `LogisticalTransporter`: o que a troca de nível leva para o transportador novo (as pilhas por referência).
    pub fn upgrade_transit(&self) -> TransitUpgrade {
        let state = self.transit.borrow();
        TransitUpgrade {
            transit: state.stacks(),
            needs_sync: state
                .needs_sync
                .entries_in_order()
                .into_iter()
                .map(|(key, stack)| (key.0, stack.clone()))
                .collect(),
            next_id: state.next_id,
            delay: state.delay,
            delay_count: state.delay_count,
        }
    }

    /// `parseUpgradeData(data, transaction)`: `transit.putAll`, `needsSync.putAll` e os contadores.
    pub fn parse_upgrade_transit(&self, data: TransitUpgrade) {
        let mut state = self.transit.borrow_mut();
        state.transit.put_all(
            data.transit
                .into_iter()
                .map(|(id, stack)| (IntKey(id), stack)),
        );
        state.needs_sync.put_all(
            data.needs_sync
                .into_iter()
                .map(|(id, stack)| (IntKey(id), stack)),
        );
        state.next_id = data.next_id;
        state.delay = data.delay;
        state.delay_count = data.delay_count;
    }

    // -----------------------------------------------------------------------------------------------------------------------------------
    // NBT e pacote
    // -----------------------------------------------------------------------------------------------------------------------------------

    /// `write(output)`: a lista `items` do disco, com o item só pelo id. Nada se não há pilhas.
    pub fn write_items(&self) -> Option<Nbt> {
        self.write_items_with(&plain_item_encode)
    }

    /// `write(output)`: a lista `items`, as pilhas na ordem de `getTransit()`.
    pub fn write_items_with(&self, encode: ItemEncode) -> Option<Nbt> {
        let stacks = self.transit_stacks();
        if stacks.is_empty() {
            return None;
        }
        Some(Nbt::List(
            stacks
                .iter()
                .map(|(_, stack)| Nbt::Compound(stack.borrow().write_with(encode)))
                .collect(),
        ))
    }

    /// `writeReducedUpdatedTag(output)`: a lista `items` do pacote de atualização, com o `index` de cada pilha.
    pub fn write_reduced_updated_items(&self) -> Option<Nbt> {
        self.write_reduced_updated_items_with(&plain_item_encode)
    }

    pub fn write_reduced_updated_items_with(&self, encode: ItemEncode) -> Option<Nbt> {
        let stacks = self.transit_stacks();
        if stacks.is_empty() {
            return None;
        }
        let pos = self.pos_long();
        Some(Nbt::List(
            stacks
                .iter()
                .map(|(id, stack)| {
                    let mut tag = stack.borrow().write_to_update_tag_with(pos, encode);
                    tag.insert(INDEX_KEY.into(), Nbt::Int(*id));
                    Nbt::Compound(tag)
                })
                .collect(),
        ))
    }

    /// `read(input)`: as pilhas do disco entram com ids novos, na ordem da lista.
    pub fn read_items(&self, items: Option<&Nbt>, decode: ItemDecode) {
        let Some(items) = items.and_then(Nbt::as_list) else {
            return;
        };
        let mut state = self.transit.borrow_mut();
        for item in items {
            if let Some(tag) = item.as_compound() {
                let id = state.next_id;
                state.next_id += 1;
                state.add_stack(id, StackRef::new(TransporterStack::read_with(tag, decode)));
            }
        }
    }

    /// `handleUpdateTag(input)`: troca o que há pelas pilhas do pacote, cada uma com o índice que veio. Sem índice, a pilha é ignorada.
    pub fn handle_update_items(&self, items: Option<&Nbt>, decode: ItemDecode) {
        let mut state = self.transit.borrow_mut();
        state.transit.clear_keeping_table();
        let Some(items) = items.and_then(Nbt::as_list) else {
            return;
        };
        for item in items {
            if let Some(tag) = item.as_compound()
                && let Some(index) = tag.get(INDEX_KEY).and_then(Nbt::as_int)
            {
                state.add_stack(
                    index,
                    StackRef::new(TransporterStack::read_from_update_with(tag, decode)),
                );
            }
        }
    }
}

/// `network.getCachedAcceptor(pos, side)`: o aceitador que a rede guardou para o lado do destino voltado para ela.
fn cached_acceptor(
    network: &dyn InventoryNetworkView,
    pos: i64,
    side: Face,
) -> Option<Rc<dyn ItemHandler>> {
    network
        .acceptors()
        .into_iter()
        .find(|(acceptor_pos, _)| *acceptor_pos == pos)
        .and_then(|(_, sides)| {
            sides
                .into_iter()
                .find(|(face, _)| *face == side)
                .map(|(_, handler)| handler)
        })
}
