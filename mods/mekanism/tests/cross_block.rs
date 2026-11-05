//! Uma transação que cobre dois blocos: a saída de uma máquina e o baú ao lado. Abortar a raiz volta os dois lados, sem
//! aviso nenhum, e commitar move os dois e avisa uma vez, só no commit da raiz.
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::item_handler::{ItemHandler, SlotHandler, VanillaContainer};
use machinery_basis_core::transaction::{Callback, Transaction};
use machinery_basis_core::transit::{HandlerTransitRequest, eject_item_map};
use std::cell::Cell;
use std::rc::Rc;

fn counter() -> (Callback, Rc<Cell<u32>>) {
    let count = Rc::new(Cell::new(0));
    let inner = count.clone();
    (Rc::new(move || inner.set(inner.get() + 1)), count)
}

fn clay_ball() -> ItemResource {
    ItemResource::new("clay_ball", 64)
}

/// A máquina e o baú, com um contador em cada aviso que o Java dispararia.
struct Blocks {
    output: InventorySlot,
    chest: Rc<VanillaContainer>,
    listener: Rc<Cell<u32>>,
    decreased: Rc<Cell<u32>>,
    set_changed: Rc<Cell<u32>>,
}

impl Blocks {
    fn new(output_amount: u32) -> Self {
        let (listener, listener_count) = counter();
        let (decrease, decreased) = counter();
        let (changed, set_changed) = counter();
        let output = InventorySlot::output(Some(listener), Some(decrease));
        output.set_contents(Some(clay_ball()), output_amount, None);
        // O `set_contents` de fora de transação já avisou: zera para contar só o envio.
        listener_count.set(0);
        Self {
            output,
            chest: VanillaContainer::new(27, changed),
            listener: listener_count,
            decreased,
            set_changed,
        }
    }

    /// Os avisos até agora: do slot, da queda de quantidade e do `setChanged` do baú.
    fn warnings(&self) -> [u32; 3] {
        [
            self.listener.get(),
            self.decreased.get(),
            self.set_changed.get(),
        ]
    }

    /// `getEjectItemMap`: o pedido sai antes de a raiz abrir, porque cada simulação dele abre a sua.
    fn request(&self) -> HandlerTransitRequest {
        let handler: Rc<dyn ItemHandler> = Rc::new(SlotHandler::new(vec![self.output.clone()]));
        eject_item_map(
            HandlerTransitRequest::new(Some(handler)),
            std::slice::from_ref(&self.output),
            &mut |_| 0,
            None,
        )
    }

    /// O resto de `outputItems`: a resposta do baú e o uso dela, na raiz que o teste decide fechar.
    fn send(&self, request: &HandlerTransitRequest, root: &Transaction) -> bool {
        let chest: Rc<dyn ItemHandler> = self.chest.clone();
        request.eject(Some(&chest), 1, root).use_all(root)
    }
}

#[test]
fn aborting_the_root_puts_both_blocks_back_without_a_warning() {
    let blocks = Blocks::new(40);
    let request = blocks.request();
    let root = Transaction::open_root();
    assert!(blocks.send(&request, &root));
    // Dentro da transação os dois lados já mudaram.
    assert_eq!(blocks.output.amount(), 0);
    assert_eq!(blocks.chest.stacks().len(), 1);
    drop(root);
    assert_eq!(blocks.output.amount(), 40);
    assert!(blocks.chest.stacks().is_empty());
    assert_eq!(blocks.warnings(), [0, 0, 0]);
}

#[test]
fn committing_the_root_moves_both_blocks_and_warns_once() {
    let blocks = Blocks::new(40);
    let request = blocks.request();
    let root = Transaction::open_root();
    assert!(blocks.send(&request, &root));
    assert_eq!(blocks.warnings(), [0, 0, 0], "nada avisa antes do commit");
    root.commit();
    assert_eq!(blocks.output.amount(), 0);
    assert_eq!(blocks.chest.stacks(), [(0, clay_ball(), 40)]);
    assert_eq!(blocks.warnings(), [1, 1, 1]);
}

#[test]
fn a_chest_with_room_for_part_takes_part_and_the_machine_keeps_the_rest() {
    let blocks = Blocks::new(5);
    for index in 0..27 {
        blocks
            .chest
            .set_item(index, Some(ItemResource::new("stone", 64)), 64);
    }
    blocks.chest.set_item(26, Some(clay_ball()), 62);
    let before = blocks.set_changed.get();
    let request = blocks.request();
    let root = Transaction::open_root();
    assert!(blocks.send(&request, &root));
    root.commit();
    assert_eq!(blocks.output.amount(), 3);
    assert_eq!(
        blocks.chest.stacks().last().map(|(_, _, amount)| *amount),
        Some(64)
    );
    assert_eq!(blocks.set_changed.get(), before + 1);
}

#[test]
fn a_full_chest_leaves_the_machine_untouched() {
    let blocks = Blocks::new(5);
    for index in 0..27 {
        blocks
            .chest
            .set_item(index, Some(ItemResource::new("stone", 64)), 64);
    }
    let request = blocks.request();
    let root = Transaction::open_root();
    assert!(!blocks.send(&request, &root));
    drop(root);
    assert_eq!(blocks.output.amount(), 5);
    assert_eq!(blocks.warnings(), [0, 0, 0]);
}
