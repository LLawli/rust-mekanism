//! `UniversalCable` (`content/network/transmitter/UniversalCable.java`), na parte de energia: o buffer do cabo, a parte dele na energia que
//! a rede guarda de volta (`SaveShareJournal`), o que ele grava no NBT e o dado de upgrade de tier no lugar. A capacidade do tier é um
//! parâmetro (`CableTier.getCableCapacity`, que vem da configuração do mod): a lib não conhece os tiers.
//!
//! O que fica no `Transmitter` da frente `rede-conexao` (as conexões por lado, o redstone, o `AcceptorCache`) não está aqui; o dono dos
//! transmissores liga os dois (`EnergyFamily` guarda um `UniversalCable` por transmissor).
use super::energy_container::{BufferEnergyContainer, always, not_external};
use crate::containers::AutomationType;
use crate::distribution::EnergySaveShare;
use crate::transaction::{Callback, JournalCell, Journaled, Transaction};
use std::cell::Cell;
use std::rc::Rc;

struct ShareState {
    save_share: i64,
    /// `getTransmitterTile().markForSave()`.
    on_save: Callback,
}

impl Journaled for ShareState {
    type Snapshot = i64;

    fn revert_to_snapshot(&mut self, snapshot: i64) {
        self.save_share = snapshot;
    }

    fn on_root_commit(&mut self, original: i64) -> Vec<Callback> {
        if self.save_share != original {
            vec![self.on_save.clone()]
        } else {
            Vec::new()
        }
    }
}

/// `UniversalCable.SaveShareJournal`: a parte do cabo na energia da rede, que o `takeShare` devolve ao buffer dele.
pub struct SaveShare {
    cell: Rc<JournalCell<ShareState>>,
    /// `getCapacity()`, a do tier do cabo.
    capacity: i64,
}

impl SaveShare {
    fn new(capacity: i64, on_save: Callback) -> Self {
        Self {
            cell: JournalCell::new(ShareState {
                save_share: 0,
                on_save,
            }),
            capacity,
        }
    }

    /// O `saveShare` de agora.
    pub fn save_share(&self) -> i64 {
        self.cell.borrow().save_share
    }

    /// `markForNewSave`: guarda o retrato e zera.
    pub fn mark_for_new_save(&self, transaction: &Transaction) {
        let share = self.save_share();
        self.cell.update_snapshots(transaction, || share);
        self.cell.borrow_mut().save_share = 0;
    }

    /// `accept(amount, transaction)`.
    pub fn accept(&self, amount: i64, transaction: &Transaction) -> i64 {
        if amount == 0 {
            // Nada a aceitar (o Java diz que não deve acontecer): falha.
            return 0;
        }
        let share = self.save_share();
        let to_accept = amount.min(self.capacity - share);
        if to_accept > 0 {
            self.cell.update_snapshots(transaction, || share);
            self.cell.borrow_mut().save_share = share + to_accept;
        }
        to_accept
    }

    fn set_without_transaction(&self, share: i64) {
        self.cell.borrow_mut().save_share = share;
    }
}

impl EnergySaveShare for SaveShare {
    fn accept(&self, amount: i64, transaction: &Transaction) -> i64 {
        SaveShare::accept(self, amount, transaction)
    }
}

/// O dado de upgrade que o cabo antigo entrega ao novo (`UniversalCableUpgradeData`, a parte do buffer): o redstone e as conexões são do
/// `Transmitter`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniversalCableUpgradeData {
    pub buffer: i64,
}

/// O estado de energia de um `UniversalCable`.
pub struct UniversalCable {
    capacity: i64,
    /// `buffer`: `BufferEnergyContainer.create(getCapacity(), notExternal, alwaysTrue, this)`.
    pub buffer: BufferEnergyContainer,
    save_share: Rc<SaveShare>,
    /// Quantas vezes o `onContentsChanged` do cabo (`getTransmitterTile().setChanged()`) disparou.
    changed: Rc<Cell<u32>>,
    /// Quantas vezes o `SaveShareJournal` pediu `markForSave`.
    saved: Rc<Cell<u32>>,
}

impl UniversalCable {
    /// `new UniversalCable(blockProvider, tile)`, com a capacidade do tier.
    pub fn new(capacity: i64) -> Self {
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
            buffer: BufferEnergyContainer::new(capacity, not_external, always, Some(listener)),
            save_share: Rc::new(SaveShare::new(capacity, on_save)),
            changed,
            saved,
        }
    }

    /// `getCapacity()`: `tier.getCableCapacity()`.
    pub fn capacity(&self) -> i64 {
        self.capacity
    }

    /// `releaseShare()`: devolve o buffer e o zera, sem transação.
    pub fn release_share(&self) -> i64 {
        let energy = self.share();
        self.buffer.set_energy(0, None);
        energy
    }

    /// `getShare()`.
    pub fn share(&self) -> i64 {
        self.buffer.amount()
    }

    /// O `saveShare` guardado.
    pub fn save_share_amount(&self) -> i64 {
        self.save_share.save_share()
    }

    /// `startNewSaveShare(transaction)`: o diário, para o alvo de salvar.
    pub fn start_new_save_share(&self, transaction: &Transaction) -> Rc<SaveShare> {
        self.save_share.mark_for_new_save(transaction);
        self.save_share.clone()
    }

    /// `read`, a parte de energia: `saveShare = input.getLongOr(ENERGY, 0)` e `buffer.setEnergy(saveShare, null)`.
    pub fn read_energy(&self, energy: i64) {
        self.save_share.set_without_transaction(energy);
        self.buffer.set_energy(energy, None);
    }

    /// `getUpgradeData()`, a parte de energia.
    pub fn upgrade_data(&self) -> UniversalCableUpgradeData {
        UniversalCableUpgradeData {
            buffer: self.buffer.amount(),
        }
    }

    /// `parseUpgradeData(data, transaction)`, a parte de energia: `buffer.copyContents(data.buffer, transaction)`.
    pub fn parse_upgrade_data(&self, data: &UniversalCableUpgradeData, transaction: &Transaction) {
        self.buffer.set_energy(data.buffer, Some(transaction));
    }

    /// `pullFromAcceptors`, o que cabe por lado puxado: a energia que o aceitador entrega (`extract`) e o buffer (o `container` do
    /// cabo, o da rede se há uma) guarda. Devolve `true` quando o contêiner ficou cheio e o laço dos lados deve parar.
    ///
    /// `extract` é `connectedAcceptor.extract(getAvailablePull(), transaction)`; `container` o que `getContainer()` devolve.
    pub fn pull_from_side(
        container: &BufferEnergyContainer,
        extract: impl FnOnce(i32, &Transaction) -> i32,
    ) -> bool {
        let transaction = Transaction::open_root();
        let extracted = extract(Self::available_pull(container), &transaction);
        if extracted > 0
            && container.insert(extracted, &transaction, AutomationType::Internal) == extracted
        {
            // Primeiro simula o próprio contêiner e só então confirma: um aceitador que deu um valor errado não duplica energia.
            transaction.commit();
            return container.amount() >= container.capacity();
        }
        false
    }

    /// `getAvailablePull()`: `getContainer().getNeededAsInt()`.
    pub fn available_pull(container: &BufferEnergyContainer) -> i32 {
        super::energy_container::saturated((container.capacity() - container.amount()).max(0))
    }

    /// `onContentsChanged` do cabo disparou quantas vezes.
    pub fn changed_count(&self) -> u32 {
        self.changed.get()
    }

    /// `markForSave` foi pedido quantas vezes.
    pub fn saved_count(&self) -> u32 {
        self.saved.get()
    }
}
