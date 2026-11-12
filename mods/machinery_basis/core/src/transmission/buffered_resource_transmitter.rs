//! `BufferedResourceTransmitter` (`content/network/transmitter/BufferedResourceTransmitter.java`): o transmissor com buffer de um
//! recurso (o cano de fluido, o tubo de químico). Guarda o buffer dele (`bufferContainer`, que a rede absorve quando o transmissor entra
//! nela e que só um `takeShare` ou um `read` enchem de novo), a parte dele no recurso que a rede guarda de volta (`SaveShareJournal`), a
//! capacidade e a taxa do tier, e o dado de upgrade de tier.
//!
//! É genérico sobre o recurso `R` (fluido, químico): a lib não conhece nenhum. O que depende da rede (o buffer com a volta ao da rede, o
//! `takeShare`, o `pullFromAcceptors`, a regra de `isValidTransmitter`) está em [`super::buffered_resource_network::ResourceFamily`],
//! que guarda os transmissores e as redes lado a lado, como a `EnergyFamily` faz com o `UniversalCable`. As conexões por lado, o
//! redstone e o `AcceptorCache` são do `Transmitter` da `rede-conexao`: o dono dos transmissores liga os dois.
use super::energy_container::not_external;
use crate::distribution::ResourceSaveShare;
use crate::resource_tank::{Resource, ResourceTank, StackContents};
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::Cell;
use std::rc::Rc;

struct ShareState<R: Resource> {
    save_share: StackContents<R>,
    /// `getTransmitterTile().markForSave()`.
    on_save: Callback,
}

impl<R: Resource> Journaled for ShareState<R> {
    type Snapshot = StackContents<R>;

    fn revert_to_snapshot(&mut self, snapshot: StackContents<R>) {
        self.save_share = snapshot;
    }

    fn on_root_commit(&mut self, original: StackContents<R>) -> Vec<Callback> {
        if self.save_share != original {
            vec![self.on_save.clone()]
        } else {
            Vec::new()
        }
    }
}

/// `BufferedResourceTransmitter.SaveShareJournal`: a parte do transmissor no recurso da rede, que o `takeShare` devolve ao buffer dele.
pub struct SaveShare<R: Resource> {
    cell: Rc<JournalCell<ShareState<R>>>,
    /// `getCapacity()`, a do tier do transmissor.
    capacity: i64,
}

impl<R: Resource> SaveShare<R> {
    fn new(capacity: i64, on_save: Callback) -> Self {
        Self {
            cell: JournalCell::new(ShareState {
                save_share: StackContents::default(),
                on_save,
            }),
            capacity,
        }
    }

    /// O `saveShare` de agora.
    pub fn save_share(&self) -> StackContents<R> {
        self.cell.borrow().save_share.clone()
    }

    /// `markForNewSave`: guarda o retrato e zera.
    pub fn mark_for_new_save(&self, transaction: &Transaction) {
        let share = self.save_share();
        self.cell.update_snapshots(transaction, || share);
        self.cell.borrow_mut().save_share = StackContents::default();
    }

    /// `accept(type, amount, transaction)`: sem nada a aceitar (o Java diz que não deve acontecer) ou com outro recurso já guardado, falha;
    /// senão fica com o que cabe na capacidade do tier.
    pub fn accept(&self, resource: &R, amount: i64, transaction: &Transaction) -> i64 {
        let share = self.save_share();
        if amount == 0 || !share.is_empty() && share.resource.as_ref() != Some(resource) {
            return 0;
        }
        let to_accept = amount.min(self.capacity - share.amount as i64);
        if to_accept > 0 {
            self.cell.update_snapshots(transaction, || share.clone());
            self.cell.borrow_mut().save_share =
                StackContents::new(Some(resource.clone()), share.amount + to_accept as u64);
        }
        to_accept
    }

    fn set_without_transaction(&self, share: StackContents<R>) {
        self.cell.borrow_mut().save_share = share;
    }
}

impl<R: Resource> ResourceSaveShare<R> for SaveShare<R> {
    fn accept(&self, resource: &R, amount: i64, transaction: &Transaction) -> i64 {
        SaveShare::accept(self, resource, amount, transaction)
    }
}

/// `ResourceTransmitterUpgradeData`, a parte do buffer: o conteúdo que o transmissor antigo entrega ao novo (o redstone e as conexões são
/// do `Transmitter`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceTransmitterUpgradeData<R: Resource> {
    pub buffer: StackContents<R>,
}

/// O estado de buffer de um `BufferedResourceTransmitter`.
pub struct ResourcePipe<R: Resource> {
    capacity: i64,
    transfer_rate: i32,
    /// `bufferContainer`: `bufferCreator.create(getCapacity(), notExternal, alwaysTrueBi, alwaysTrue, this)`.
    pub buffer: ResourceTank<R>,
    save_share: Rc<SaveShare<R>>,
    /// Quantas vezes o `onContentsChanged` do transmissor (`getTransmitterTile().setChanged()`) disparou.
    changed: Rc<Cell<u32>>,
    /// Quantas vezes o `SaveShareJournal` pediu `markForSave`.
    saved: Rc<Cell<u32>>,
}

impl<R: Resource> ResourcePipe<R> {
    /// `new MechanicalPipe(blockProvider, tile)`, com a capacidade (`tier.getCapacity()`) e a taxa (`tier.getTransferRate()`) do tier.
    pub fn new(capacity: i64, transfer_rate: i32) -> Self {
        let changed = Rc::new(Cell::new(0));
        let saved = Rc::new(Cell::new(0));
        let listener: Callback = {
            let changed = changed.clone();
            Rc::new(move || changed.set(changed.get() + 1))
        };
        let on_save: Callback = {
            let saved = saved.clone();
            Rc::new(move || saved.set(saved.get() + 1))
        };
        Self {
            capacity,
            transfer_rate,
            buffer: ResourceTank::create(
                capacity as u64,
                Rc::new(|_, automation| not_external(automation)),
                Rc::new(|_, _| true),
                Rc::new(|_| true),
                Some(listener),
            ),
            save_share: Rc::new(SaveShare::new(capacity, on_save)),
            changed,
            saved,
        }
    }

    /// `getCapacity()`: `getTier().getCapacity()`.
    pub fn capacity(&self) -> i64 {
        self.capacity
    }

    /// `getTier().getTransferRate()`.
    pub fn transfer_rate(&self) -> i32 {
        self.transfer_rate
    }

    /// `getShare()`: `bufferContainer.asStack()`.
    pub fn share(&self) -> StackContents<R> {
        self.buffer.contents()
    }

    /// `releaseShare()`: devolve o buffer e o zera, sem transação.
    pub fn release_share(&self) -> StackContents<R> {
        let share = self.share();
        self.buffer.set_contents(None, 0, None);
        share
    }

    /// O `saveShare` guardado.
    pub fn save_share_contents(&self) -> StackContents<R> {
        self.save_share.save_share()
    }

    /// `startNewSaveShare(transaction)`: o diário, para o alvo de salvar.
    pub fn start_new_save_share(&self, transaction: &Transaction) -> Rc<SaveShare<R>> {
        self.save_share.mark_for_new_save(transaction);
        self.save_share.clone()
    }

    /// `read`, a parte do buffer: `saveShare = readOrEmpty(STORED)` e `bufferContainer.setContents(saveShare, null)`.
    pub fn read_stored(&self, stored: StackContents<R>) {
        self.save_share.set_without_transaction(stored.clone());
        self.buffer
            .set_contents(stored.resource, stored.amount, None);
    }

    /// `getUpgradeData()`, a parte do buffer.
    pub fn upgrade_data(&self) -> ResourceTransmitterUpgradeData<R> {
        ResourceTransmitterUpgradeData {
            buffer: self.buffer.contents(),
        }
    }

    /// `parseUpgradeData(data, transaction)`, a parte do buffer: `bufferContainer.copyContents(data.buffer, transaction)`.
    pub fn parse_upgrade_data(
        &self,
        data: &ResourceTransmitterUpgradeData<R>,
        transaction: &Transaction,
    ) {
        self.buffer.set_contents(
            data.buffer.resource.clone(),
            data.buffer.amount,
            Some(transaction),
        );
    }

    /// O `onContentsChanged` do transmissor disparou quantas vezes.
    pub fn changed_count(&self) -> u32 {
        self.changed.get()
    }

    /// `markForSave` foi pedido quantas vezes.
    pub fn saved_count(&self) -> u32 {
        self.saved.get()
    }
}
