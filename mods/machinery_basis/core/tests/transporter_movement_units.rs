//! O que o movimento do transportador faz sozinho, sem rede: onde a pilha cai ao quebrar, o `STREAM_CODEC`, o NBT da lista de pilhas, o
//! `TransporterItemHandler` e o que o desviador e a cor pedem ao dono do bloco. Os valores vêm de contas sobre o Java
//! (`LogisticalTransporterBase`, `TransporterStack`, `DiversionTransporter`, `TransporterUtils`); o movimento tick a tick, conferido com o
//! original, está em `transporter_move_reference.rs`.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transporter::movement::stack_block_offset;
use machinery_basis_core::transporter::nbt::{Nbt, to_snbt};
use machinery_basis_core::transporter::node::{
    CapabilityChange, DiversionControl, DiversionNeighbor, LogisticalTransporterBase,
    TransporterKind, TransporterTier, decrement_color, increment_color,
};
use machinery_basis_core::transporter::stack::{
    StackRef, TransporterStack, plain_item_encode, read_var_long, write_var_int,
};
use std::cell::Cell;
use std::rc::Rc;

fn stone() -> ItemResource {
    ItemResource::new("minecraft:stone", 64)
}

fn base(kind: TransporterKind, powered: Rc<Cell<bool>>) -> LogisticalTransporterBase {
    LogisticalTransporterBase::new(
        BlockPos::new(10, 64, -3),
        TransporterTier::Advanced,
        kind,
        Rc::new(move || powered.get()),
    )
}

fn logistical() -> LogisticalTransporterBase {
    base(
        TransporterKind::Logistical { color: None },
        Rc::new(Cell::new(false)),
    )
}

fn decode(tag: &machinery_basis_core::transporter::nbt::Compound) -> Option<ItemResource> {
    let id = tag.get("id")?.as_str()?;
    Some(ItemResource::new(id, 64))
}

/// `TransporterUtils.getStackPosition` em `float` e `Mth.floor`: o bloco onde `drop` solta a pilha.
#[test]
fn a_dropped_stack_lands_in_the_block_its_position_floors_to() {
    // Descendo (a pilha vem de cima, lado DOWN), a 80%: y = -1 * (0,8 - 0,5) + 0,25 = -0,05, um bloco abaixo.
    assert_eq!(stack_block_offset(Face::Down, 80), [0, -1, 0]);
    // Subindo, a 80%: y = 0,3 + 0,25 = 0,55.
    assert_eq!(stack_block_offset(Face::Up, 80), [0, 0, 0]);
    // A 10% descendo: y = 0,4 + 0,25 = 0,65.
    assert_eq!(stack_block_offset(Face::Down, 10), [0, 0, 0]);
    // Oeste, no começo: x = -1 * -0,5 + 0,5 = 1,0, um bloco a leste (o original solta ali).
    assert_eq!(stack_block_offset(Face::West, 0), [1, 0, 0]);
    // Leste, no começo: x = 1 * -0,5 + 0,5 = 0.
    assert_eq!(stack_block_offset(Face::East, 0), [0, 0, 0]);
    // Norte, a 99%: z = -1 * 0,49 + 0,5 = 0,01.
    assert_eq!(stack_block_offset(Face::North, 99), [0, 0, 0]);
}

fn read_item(bytes: &mut &[u8]) -> Option<ItemResource> {
    let id = read_var_long(bytes)?;
    assert_eq!(id, 1);
    let (added, removed) = (read_var_long(bytes)?, read_var_long(bytes)?);
    assert_eq!((added, removed), (0, 0));
    Some(stone())
}

#[test]
fn the_stream_codec_round_trips_and_a_zero_progress_arrives_as_five() {
    let mut stack = TransporterStack::new();
    stack.set_stack(Some(&stone()), 300);
    stack.color = Some(12);
    stack.progress = 55;
    stack.original_location = -2_938_024_806_227_243_003;
    stack.update_for_pos(0);
    let bytes = stack.write_stream(&|_, out| {
        write_var_int(out, 1);
        out.extend([0, 0]);
    });
    let mut cursor: &[u8] = &bytes;
    let decoded = TransporterStack::read_stream(&mut cursor, &read_item).unwrap();
    assert!(cursor.is_empty());
    assert_eq!(decoded.size(), 300);
    assert_eq!(decoded.color, Some(12));
    assert_eq!(decoded.progress, 55);
    assert_eq!(decoded.original_location, stack.original_location);
    assert_eq!(decoded.path_type(), machinery_basis_core::transporter::Path::None);

    stack.progress = 0;
    let bytes = stack.write_stream(&|_, out| {
        write_var_int(out, 1);
        out.extend([0, 0]);
    });
    let mut cursor: &[u8] = &bytes;
    let decoded = TransporterStack::read_stream(&mut cursor, &read_item).unwrap();
    assert_eq!(decoded.progress, 5);

    // A pilha vazia só leva a quantidade zero.
    let empty = TransporterStack::new();
    let bytes = empty.write_stream(&|_, _| panic!("a pilha vazia não escreve item"));
    let mut cursor: &[u8] = &bytes;
    let decoded = TransporterStack::read_stream(&mut cursor, &read_item).unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn the_items_list_is_written_in_table_order_and_read_with_new_ids() {
    let source = logistical();
    for (amount, progress) in [(5, 30), (7, 80)] {
        let mut stack = TransporterStack::for_insert(77, Some(1));
        stack.set_stack(Some(&stone()), amount);
        stack.progress = progress;
        stack.idle_dir = Some(Face::East);
        source.entity_entering(StackRef::new(stack), progress);
    }
    // Sem pilhas não há lista.
    assert!(logistical().write_items().is_none());
    let list = source.write_items().unwrap();
    assert_eq!(
        to_snbt(&list),
        "[{color:\"blue\",home_location:77L,idle_dir:\"east\",item:{amount:5L,type:{id:\"minecraft:stone\"}},\
         original_location:77L,progress:30},\
         {color:\"blue\",home_location:77L,idle_dir:\"east\",item:{amount:7L,type:{id:\"minecraft:stone\"}},\
         original_location:77L,progress:80}]"
    );
    let copy = logistical();
    copy.read_items(Some(&list), &decode);
    let stacks = copy.transit_stacks();
    assert_eq!(stacks.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(copy.counters().2, 2);
    assert_eq!(stacks[0].1.borrow().size(), 5);
    assert_eq!(stacks[1].1.borrow().progress, 80);
    assert_eq!(stacks[1].1.borrow().idle_dir, Some(Face::East));
    assert_eq!(stacks[1].1.borrow().home_location, 77);

    // O pacote de atualização leva o índice de cada pilha e a troca por inteiro.
    let update = source.write_reduced_updated_items().unwrap();
    let text = to_snbt(&update);
    assert!(text.starts_with("[{color:\"blue\",index:0,item:{amount:5L,"), "{text}");
    assert!(text.contains(",index:1,"), "{text}");
    let client = logistical();
    client.entity_entering(StackRef::new(TransporterStack::new()), 0);
    client.handle_update_items(Some(&update), &decode);
    let stacks = client.transit_stacks();
    assert_eq!(stacks.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(stacks[0].1.borrow().size(), 5);
    // Uma pilha sem `index` é ignorada.
    let mut without_index = match &update {
        Nbt::List(items) => items.clone(),
        _ => unreachable!(),
    };
    if let Nbt::Compound(tag) = &mut without_index[0] {
        tag.remove("index");
    }
    client.handle_update_items(Some(&Nbt::List(without_index)), &decode);
    assert_eq!(client.transit_stacks().len(), 1);
    // Sem a lista, o cliente fica sem nada.
    client.handle_update_items(None, &decode);
    assert!(client.transit_stacks().is_empty());
}

#[test]
fn the_item_resource_is_written_by_the_codec_the_caller_gives() {
    let mut stack = TransporterStack::new();
    stack.set_stack(Some(&stone()), 3);
    let tag = stack.write_with(&|item| {
        let mut resource = plain_item_encode(item);
        resource.insert("components".into(), Nbt::Compound(Default::default()));
        resource
    });
    assert_eq!(
        to_snbt(&Nbt::Compound(tag)),
        "{item:{amount:3L,type:{components:{},id:\"minecraft:stone\"}},original_location:9223372036854775807L,progress:0}"
    );
}

#[test]
fn colors_step_through_the_eighteen_and_back_to_none() {
    assert_eq!(increment_color(None), Some(0));
    assert_eq!(increment_color(Some(0)), Some(1));
    assert_eq!(increment_color(Some(16)), Some(17));
    assert_eq!(increment_color(Some(17)), None);
    assert_eq!(decrement_color(None), Some(17));
    assert_eq!(decrement_color(Some(1)), Some(0));
    assert_eq!(decrement_color(Some(0)), None);
    let mut transporter = logistical();
    assert_eq!(transporter.configure_color(), Some(Some(0)));
    assert_eq!(transporter.get_color(), Some(0));
    // Só o logístico tem cor.
    let mut restrictive = base(TransporterKind::Restrictive, Rc::new(Cell::new(false)));
    assert_eq!(restrictive.configure_color(), None);
}

fn diversion(powered: &Rc<Cell<bool>>) -> LogisticalTransporterBase {
    let mut modes = [DiversionControl::Disabled; 6];
    modes[2] = DiversionControl::High;
    modes[5] = DiversionControl::Low;
    base(TransporterKind::Diversion { modes }, powered.clone())
}

#[test]
fn the_diversion_modes_are_saved_as_an_int_array() {
    let power = Rc::new(Cell::new(false));
    let mut transporter = diversion(&power);
    let written = transporter.write_modes().unwrap();
    assert_eq!(to_snbt(&written), "[I;0,0,1,0,0,2]");
    // Sem a chave todos voltam a desligados; um vetor curto muda só os primeiros.
    transporter.read_modes(None);
    assert_eq!(to_snbt(&transporter.write_modes().unwrap()), "[I;0,0,0,0,0,0]");
    transporter.read_modes(Some(&Nbt::IntArray(vec![1, 2])));
    assert_eq!(to_snbt(&transporter.write_modes().unwrap()), "[I;1,2,0,0,0,0]");
    // Um índice fora da faixa cai em desligado (`OutOfBoundsStrategy.ZERO`).
    transporter.read_modes(Some(&Nbt::IntArray(vec![9, 1, 2, 0, 0, 0])));
    assert_eq!(to_snbt(&transporter.write_modes().unwrap()), "[I;0,1,2,0,0,0]");
    assert!(logistical().write_modes().is_none());
}

#[test]
fn the_diversion_checks_the_signal_once_and_then_only_refreshes() {
    let power = Rc::new(Cell::new(false));
    let transporter = diversion(&power);
    // A primeira vez conta como mudança, mesmo sem sinal.
    assert_eq!(
        transporter.diversion_neighbor_change(Some(Face::North)),
        DiversionNeighbor::PowerChanged
    );
    assert_eq!(
        transporter.diversion_neighbor_change(Some(Face::North)),
        DiversionNeighbor::Refresh(Some(Face::North))
    );
    assert_eq!(
        transporter.diversion_neighbor_change(None),
        DiversionNeighbor::Refresh(None)
    );
    power.set(true);
    assert_eq!(
        transporter.diversion_neighbor_change(None),
        DiversionNeighbor::PowerChanged
    );
}

#[test]
fn the_diversion_announces_or_hides_the_item_capability_by_the_mode() {
    let power = Rc::new(Cell::new(false));
    let mut transporter = diversion(&power);
    // Com todos os lados aceitando (Normal), cada um é anunciado se o modo vale e escondido se não.
    let changes = transporter.capability_changes_after_power();
    assert_eq!(changes.len(), 6);
    // Norte ALTO sem sinal: escondido; Leste BAIXO sem sinal: vale.
    assert_eq!(changes[2], CapabilityChange::Hidden(Face::North));
    assert_eq!(changes[5], CapabilityChange::Exposed);
    power.set(true);
    let changes = transporter.capability_changes_after_power();
    assert_eq!(changes[2], CapabilityChange::Exposed);
    assert_eq!(changes[5], CapabilityChange::Hidden(Face::East));
    // Trocar o modo do sul para ALTO, com sinal, passa a valer: anuncia; voltar a desligado vale igual: nada.
    let update = transporter
        .update_mode(Face::South, DiversionControl::Low)
        .unwrap();
    assert_eq!(update.capability, Some(CapabilityChange::Hidden(Face::South)));
    let update = transporter
        .update_mode(Face::South, DiversionControl::Disabled)
        .unwrap();
    assert_eq!(update.capability, Some(CapabilityChange::Exposed));
    let update = transporter
        .update_mode(Face::South, DiversionControl::High)
        .unwrap();
    assert_eq!(update.capability, None);
    // O mesmo modo não muda nada.
    assert!(transporter.update_mode(Face::South, DiversionControl::High).is_none());
    assert_eq!(transporter.next_mode(Face::South), Some(DiversionControl::Low));
    assert_eq!(DiversionControl::Low.next(), DiversionControl::Disabled);
}

/// Um mundo sem nada: só o que o gerenciador e o `drop` precisam.
struct NoWorld;

impl machinery_basis_core::transporter::world::TransporterWorld for NoWorld {
    fn side_configuration(
        &self,
        _: i64,
    ) -> Option<machinery_basis_core::transporter::world::SideConfigView> {
        None
    }

    fn sorter(
        &self,
        _: i64,
    ) -> Option<&dyn machinery_basis_core::transporter::world::TransportSorter> {
        None
    }

    fn item_handler(
        &self,
        _: i64,
        _: Face,
    ) -> Option<Rc<dyn machinery_basis_core::item_handler::ItemHandler>> {
        None
    }

    fn neighbor_transmitter_can_connect(&self, _: i64, _: Face) -> Option<bool> {
        None
    }
}

fn flying(hash: i32) -> StackRef {
    let mut stack = TransporterStack::new();
    stack.set_identity_hash(hash);
    stack.set_stack(Some(&stone()), 1);
    StackRef::new(stack)
}

/// Desfazer a transação restaura o conjunto como o `new HashSet<>(stacks)` do `createSnapshot` o copiou: a tabela do tamanho que o
/// conteúdo pede, não a que o conjunto tinha crescido.
#[test]
fn a_rolled_back_transaction_restores_the_copy_of_the_in_flight_set() {
    use machinery_basis_core::transaction::Transaction;
    use machinery_basis_core::transmission::DimensionId;
    use machinery_basis_core::transporter::pathfinder::PathContext;
    use machinery_basis_core::transporter::{Path, PathfinderCache, TransporterManager};
    let manager = TransporterManager::new();
    let cache = PathfinderCache::new();
    let world = NoWorld;
    let ctx = PathContext {
        dimension: DimensionId(0),
        manager: &manager,
        cache: &cache,
        world: &world,
    };
    let destination = BlockPos::new(0, 64, 0).as_long();
    let start = BlockPos::new(1, 64, 0).as_long();
    let put = |hash: i32, transaction: Option<&Transaction>| {
        let stack = flying(hash);
        stack.set_path(&ctx, vec![destination, start], Path::Dest, transaction);
        stack
    };
    // Treze entram e saem: a tabela cresce para 32 e fica.
    let crowd: Vec<StackRef> = (0..13).map(|i| put(103 + i * 16, None)).collect();
    let (a, c) = (put(17, None), put(2, None));
    for member in &crowd {
        manager.remove(DimensionId(0), member, None);
    }
    let order = |manager: &TransporterManager| -> Vec<i32> {
        manager
            .flowing_to(DimensionId(0), destination)
            .iter()
            .map(|stack| stack.borrow().identity_hash())
            .collect()
    };
    assert_eq!(order(&manager), [2, 17]);
    {
        let transaction = Transaction::open_root();
        let extra = flying(99);
        extra.set_path(&ctx, vec![destination, start], Path::Dest, Some(&transaction));
        assert_eq!(order(&manager).len(), 3);
        // Sem `commit`: desfaz.
    }
    assert_eq!(order(&manager), [17, 2]);
    drop((a, c));
}

#[test]
fn an_upgrading_transporter_keeps_its_stacks_when_the_block_goes() {
    use machinery_basis_core::transmission::DimensionId;
    use machinery_basis_core::transporter::movement::MoveContext;
    use machinery_basis_core::transporter::pathfinder::PathContext;
    use machinery_basis_core::transporter::{PathfinderCache, TransporterManager};
    let manager = TransporterManager::new();
    let cache = PathfinderCache::new();
    let world = NoWorld;
    let path = PathContext {
        dimension: DimensionId(0),
        manager: &manager,
        cache: &cache,
        world: &world,
    };
    let ctx = MoveContext {
        path: &path,
        network: None,
    };
    let mut transporter = logistical();
    let mut stack = TransporterStack::new();
    stack.set_stack(Some(&stone()), 4);
    transporter.entity_entering(StackRef::new(stack), 30);
    transporter.transmitter.set_upgrading(true);
    transporter.pre_remove_side_effects(&ctx);
    assert!(transporter.take_world_drops().is_empty());
    transporter.transmitter.set_upgrading(false);
    transporter.pre_remove_side_effects(&ctx);
    let drops = transporter.take_world_drops();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].amount, 4);
    assert_eq!(drops[0].pos, BlockPos::new(10, 64, -3));
}

#[test]
fn the_color_is_saved_by_name_and_read_back() {
    let mut transporter = logistical();
    assert!(transporter.write_color().is_none());
    transporter.configure_color();
    transporter.configure_color();
    assert_eq!(
        transporter.write_color().map(|tag| to_snbt(&tag)),
        Some("\"blue\"".into())
    );
    let tag = Nbt::String("red".into());
    assert!(transporter.read_color(Some(&tag)));
    assert!(!transporter.read_color(Some(&tag)));
    assert!(transporter.read_color(None));
    assert_eq!(transporter.get_color(), None);
}

#[test]
fn an_upgrade_carries_the_stacks_and_the_counters_over() {
    let old = logistical();
    for progress in [20, 60] {
        let mut stack = TransporterStack::new();
        stack.set_stack(Some(&stone()), 2);
        old.entity_entering(StackRef::new(stack), progress);
    }
    old.restore_counters(4, 2, 2);
    let new = logistical();
    new.parse_upgrade_transit(old.upgrade_transit());
    assert_eq!(new.counters(), (4, 2, 2));
    let ids: Vec<i32> = new.transit_stacks().iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, old.transit_stacks().iter().map(|(id, _)| *id).collect::<Vec<_>>());
    assert_eq!(new.pending_sync(), old.pending_sync());
}
