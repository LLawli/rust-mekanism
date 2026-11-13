//! O espelho de um inventário vizinho de um transportador (`HostItemHandler`): a rede insere e tira dele dentro de transações, e o fim do evento diz ao jogo o que mudou.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::ItemHandler;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::host_item::{HostItemHandler, MirrorSlot, SlotChange};

fn dirt() -> ItemResource {
    ItemResource::new("minecraft:dirt", 64)
}

fn stone() -> ItemResource {
    ItemResource::new("minecraft:stone", 64)
}

fn slot(index: usize, contents: Option<(ItemResource, u32)>, can_take: bool) -> MirrorSlot {
    MirrorSlot {
        index,
        contents,
        can_take,
    }
}

#[test]
fn what_the_network_inserts_and_takes_becomes_changes_for_the_game() {
    // Dois slots do jogo, 4 e 7 (um baú de lado só alcança alguns): terra no primeiro, o segundo vazio.
    let handler =
        HostItemHandler::new(vec![slot(4, Some((dirt(), 10)), true), slot(7, None, true)]);
    assert!(handler.changes().is_empty());
    let root = Transaction::open_root();
    assert_eq!(handler.extract(&dirt(), 3, &root), 3);
    assert_eq!(handler.insert(&stone(), 5, &root), 5);
    root.commit();
    assert_eq!(
        handler.changes(),
        vec![
            SlotChange::Take { index: 4, count: 3 },
            SlotChange::Put {
                resource: stone(),
                count: 5
            },
        ]
    );
    handler.settle();
    assert!(handler.changes().is_empty(), "o jogo aplicou");
}

#[test]
fn a_rolled_back_transaction_changes_nothing() {
    let handler = HostItemHandler::new(vec![slot(0, Some((dirt(), 10)), true)]);
    let root = Transaction::open_root();
    assert_eq!(handler.extract(&dirt(), 10, &root), 10);
    assert_eq!(handler.insert(&stone(), 1, &root), 1);
    drop(root);
    assert!(handler.changes().is_empty());
    assert_eq!(handler.amount(0), 10);
}

#[test]
fn a_slot_the_game_does_not_let_take_from_gives_nothing() {
    let handler = HostItemHandler::new(vec![slot(0, Some((dirt(), 10)), false)]);
    let root = Transaction::open_root();
    assert_eq!(handler.extract(&dirt(), 10, &root), 0);
    root.commit();
    assert!(handler.changes().is_empty());
}

#[test]
fn refresh_reads_the_container_again_and_follows_a_new_size() {
    let handler = HostItemHandler::new(vec![slot(0, Some((dirt(), 10)), true)]);
    handler.refresh(vec![slot(0, Some((dirt(), 4)), true), slot(1, None, true)]);
    assert_eq!(handler.size(), 2);
    assert_eq!(handler.amount(0), 4);
    assert!(handler.changes().is_empty());
}
