//! O espelho de um inventário vizinho de um transportador (`HostItemHandler`): a rede insere e tira dele dentro de transações, e o fim do evento diz ao jogo o que mudou.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::ItemHandler;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::host_item::{
    HostItemHandler, MirrorSlot, RefusalMemo, SlotChange, static_slot_rule,
};

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
        refuses: Vec::new(),
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

#[test]
fn a_slot_the_game_refuses_an_item_in_does_not_receive_it() {
    let handler = HostItemHandler::new(vec![MirrorSlot {
        index: 3,
        contents: None,
        can_take: true,
        refuses: vec![stone()],
    }]);
    let root = Transaction::open_root();
    assert_eq!(handler.insert(&stone(), 5, &root), 0);
    assert_eq!(handler.insert(&dirt(), 5, &root), 5);
    root.commit();
    assert_eq!(
        handler.changes(),
        vec![SlotChange::Put {
            resource: dirt(),
            count: 5
        }]
    );
}

/// Pedir o handler no meio de um evento (`getCapability`) devolve o mesmo objeto com o que o evento já mudou nele: a releitura só vale se não há mudança pendente. Com uma entrega ainda
/// por aplicar, a releitura a desfazia e a pilha sumia (a segunda a chegar no mesmo tick ao mesmo baú).
#[test]
fn a_mid_event_read_keeps_what_the_event_already_changed() {
    let handler = HostItemHandler::new(vec![slot(0, Some((dirt(), 10)), true)]);
    let root = Transaction::open_root();
    assert_eq!(handler.insert(&dirt(), 5, &root), 5);
    root.commit();
    // O jogo ainda não recebeu as 5: a releitura do meio do evento não as apaga.
    handler.refresh_if_clean(vec![slot(0, Some((dirt(), 10)), true)]);
    assert_eq!(handler.amount(0), 15);
    assert_eq!(
        handler.changes(),
        vec![SlotChange::Put {
            resource: dirt(),
            count: 5
        }]
    );
    // Aplicado, o espelho está limpo e a releitura vale: o jogo tem 15.
    handler.settle();
    handler.refresh_if_clean(vec![slot(0, Some((dirt(), 20)), true)]);
    assert_eq!(handler.amount(0), 20);
    assert!(handler.changes().is_empty());
}

/// O modelo do forno do original: o slot do combustível (1) aceita um balde só se o que já está nele não é um balde (`AbstractFurnaceBlockEntity.canPlaceItem`). A recusa de um balde muda
/// quando o slot enche ou esvazia por fora (um funil, o jogador), sem que o bloco nem os itens oferecidos mudem: o que a guarda a deixaria velha.
fn furnace_refuses_bucket(fuel_slot_holds_bucket: bool) -> Vec<Vec<ItemResource>> {
    let bucket = ItemResource::new("minecraft:bucket", 16);
    vec![
        Vec::new(),
        if fuel_slot_holds_bucket {
            vec![bucket]
        } else {
            Vec::new()
        },
        Vec::new(),
    ]
}

#[test]
fn the_refusal_memo_never_keeps_the_answer_of_a_container_whose_rule_follows_its_contents() {
    let mut memo: RefusalMemo<u32> = RefusalMemo::default();
    // O forno com o combustível vazio: o balde entra. O memo recebe, e não guarda.
    memo.put(1, "minecraft:furnace", furnace_refuses_bucket(false));
    // Um funil põe um balde no slot do combustível: agora o balde é recusado. O memo não pode devolver o "entra" de antes.
    assert!(
        memo.get(&1, "minecraft:furnace", 3).is_none(),
        "o forno é perguntado a cada vez"
    );
    // O mesmo vale para a bancada de poções, o toca-discos, a estante, o crafter e a máquina de mod.
    for block in [
        "minecraft:brewing_stand",
        "minecraft:jukebox",
        "minecraft:chiseled_bookshelf",
        "minecraft:crafter",
        "mekanism:enrichment_chamber",
        "minecraft:blast_furnace",
    ] {
        memo.put(2, block, vec![Vec::new()]);
        assert!(memo.get(&2, block, 1).is_none(), "{block}");
        assert!(!static_slot_rule(block), "{block}");
    }
}

#[test]
fn the_refusal_memo_keeps_what_a_chest_a_hopper_or_a_shulker_box_refuses_while_the_block_stays() {
    let mut memo: RefusalMemo<u32> = RefusalMemo::default();
    for (key, block) in [
        "minecraft:chest",
        "minecraft:trapped_chest",
        "minecraft:barrel",
        "minecraft:hopper",
        "minecraft:dropper",
        "minecraft:dispenser",
        "minecraft:shulker_box",
        "minecraft:red_shulker_box",
    ]
    .into_iter()
    .enumerate()
    {
        let key = key as u32;
        let refused = vec![vec![dirt()], Vec::new()];
        memo.put(key, block, refused.clone());
        assert_eq!(memo.get(&key, block, 2), Some(&refused), "{block}");
        // Outro bloco no lugar, ou outro número de slots (um baú que virou duplo): a resposta não vale.
        assert!(memo.get(&key, "minecraft:furnace", 2).is_none(), "{block}");
        assert!(memo.get(&key, block, 3).is_none(), "{block}");
    }
    memo.clear();
    assert!(memo.get(&0, "minecraft:chest", 2).is_none());
}
