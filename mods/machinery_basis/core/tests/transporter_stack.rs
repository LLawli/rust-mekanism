//! A pilha do transportador (`TransporterStack`) e o gerenciador das pilhas em voo (`TransporterManager`), nas partes que o rastro de
//! caminhos não alcança: o NBT lido do disco e do pacote de atualização, o próximo, o anterior e o lado de uma pilha numa posição do
//! caminho, e o desfazer de uma transação. As contas vêm de `TransporterStack.java` e `TransporterManager.java`.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::DimensionId;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transporter::nbt::{Compound, Nbt, to_snbt};
use machinery_basis_core::transporter::pos::NO_POS;
use machinery_basis_core::transporter::stack::{Path, keys};
use machinery_basis_core::transporter::world::Transporter;
use machinery_basis_core::transporter::{StackRef, TransporterManager, TransporterStack};

fn at(x: i32) -> i64 {
    BlockPos::new(x, 0, 0).as_long()
}

fn stone() -> ItemResource {
    ItemResource::new("minecraft:stone", 64)
}

fn resolve(id: &str) -> Option<ItemResource> {
    (id == "minecraft:stone").then(stone)
}

/// Um caminho de (3) a (1) com a origem em (0): o destino em 3, os transportadores em 2 e 1.
fn on_a_path() -> TransporterStack {
    let mut stack = TransporterStack::for_insert(at(0), None);
    stack.set_stack(Some(&stone()), 10);
    stack
}

fn with_path(stack: &StackRef, path: Vec<i64>, path_type: Path) {
    // `setPath` com um gerenciador à parte: só o que a pilha guarda importa.
    let manager = TransporterManager::new();
    let cache = machinery_basis_core::transporter::PathfinderCache::new();
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
        ) -> Option<std::rc::Rc<dyn machinery_basis_core::item_handler::ItemHandler>> {
            None
        }
    }
    let ctx = machinery_basis_core::transporter::pathfinder::PathContext {
        dimension: DimensionId(0),
        manager: &manager,
        cache: &cache,
        world: &NoWorld,
    };
    stack.set_path(&ctx, path, path_type, None);
}

#[test]
fn the_saved_nbt_round_trips_and_leaves_out_what_is_null() {
    let mut stack = on_a_path();
    stack.color = Some(4);
    stack.progress = 37;
    stack.idle_dir = Some(Face::West);
    stack.home_location = at(5);
    let tag = stack.write();
    // Sem `path_type` guardado (nulo) a chave não existe.
    assert!(!tag.contains_key(keys::PATH_TYPE));
    let back = TransporterStack::read(&tag, &resolve);
    assert_eq!(back.color, Some(4));
    assert_eq!(back.progress, 37);
    assert_eq!(back.original_location, at(0));
    assert_eq!(back.idle_dir, Some(Face::West));
    assert_eq!(back.home_location, at(5));
    assert_eq!(back.size(), 10);
    assert_eq!(back.item_type(), Some(&stone()));
    assert_eq!(back.raw_path_type(), None);

    let bare = TransporterStack::new().write();
    assert_eq!(
        to_snbt(&Nbt::Compound(bare)),
        "{original_location:9223372036854775807L,progress:0}"
    );
}

#[test]
fn reading_a_broken_tag_gives_the_defaults() {
    let mut tag = Compound::new();
    tag.insert(keys::COLOR.into(), Nbt::String("not_a_color".into()));
    tag.insert(keys::PATH_TYPE.into(), Nbt::String("sideways".into()));
    tag.insert(keys::IDLE_DIR.into(), Nbt::String("sideways".into()));
    let stack = TransporterStack::read(&tag, &resolve);
    assert_eq!(stack.color, None);
    assert_eq!(stack.progress, 0);
    assert_eq!(stack.original_location, NO_POS);
    assert_eq!(stack.home_location, NO_POS);
    assert_eq!(stack.idle_dir, None);
    assert_eq!(stack.raw_path_type(), None);
    assert!(stack.is_empty());
    // Um item que o registro não conhece ou uma quantidade zerada deixa a pilha vazia.
    let mut item = Compound::new();
    let mut resource = Compound::new();
    resource.insert(keys::ID.into(), Nbt::String("minecraft:unknown".into()));
    item.insert(keys::TYPE.into(), Nbt::Compound(resource));
    item.insert(keys::AMOUNT.into(), Nbt::Long(3));
    tag.insert(keys::ITEM.into(), Nbt::Compound(item));
    assert!(TransporterStack::read(&tag, &resolve).is_empty());
}

#[test]
fn the_update_tag_carries_the_type_the_next_and_the_previous() {
    let stack = StackRef::new(on_a_path());
    with_path(&stack, vec![at(3), at(2), at(1)], Path::Dest);
    let tag = stack.borrow().write_to_update_tag(at(2));
    assert_eq!(tag[keys::PATH_TYPE], Nbt::String("dest".into()));
    assert_eq!(tag[keys::NEXT], Nbt::Long(at(3)));
    assert_eq!(tag[keys::PREVIOUS], Nbt::Long(at(1)));
    // O último transportador do caminho tem a origem como anterior; o destino não tem próximo.
    let tag = stack.borrow().write_to_update_tag(at(1));
    assert_eq!(tag[keys::NEXT], Nbt::Long(at(2)));
    assert_eq!(tag[keys::PREVIOUS], Nbt::Long(at(0)));
    let tag = stack.borrow().write_to_update_tag(at(3));
    assert!(!tag.contains_key(keys::NEXT));
    assert_eq!(tag[keys::PREVIOUS], Nbt::Long(at(2)));
    // Uma posição fora do caminho: sem próximo e com a primeira posição do caminho como anterior (`indexOf` dá -1).
    let tag = stack.borrow().write_to_update_tag(at(9));
    assert!(!tag.contains_key(keys::NEXT));
    assert_eq!(tag[keys::PREVIOUS], Nbt::Long(at(3)));

    let back =
        TransporterStack::read_from_update(&stack.borrow().write_to_update_tag(at(2)), &resolve);
    assert_eq!(back.client_next(), at(3));
    assert_eq!(back.client_prev(), at(1));
    assert_eq!(back.raw_path_type(), Some(Path::Dest));
    // Sem tipo guardado, o pacote diz `none`.
    let fresh = TransporterStack::new().write_to_update_tag(at(0));
    assert_eq!(fresh[keys::PATH_TYPE], Nbt::String("none".into()));
}

#[test]
fn the_side_is_where_the_stack_came_from_in_the_first_half_and_where_it_goes_in_the_second() {
    let stack = StackRef::new(on_a_path());
    with_path(&stack, vec![at(3), at(2), at(1)], Path::Dest);
    let mut state = stack.borrow_mut();
    state.progress = 10;
    // Em (2), veio de (1): o lado é o de (2) para (1)... `sideDifference(pos, prev)` é `pos - prev`.
    assert_eq!(state.get_side(at(2)), Face::East);
    state.progress = 50;
    // Na segunda metade vai para (3): `sideDifference(next, pos)`.
    assert_eq!(state.get_side(at(2)), Face::East);
    // Sem vizinho que sirva de referência, o lado é `DOWN`.
    assert_eq!(state.get_side(at(9)), Face::Down);
    state.progress = 0;
    assert_eq!(state.get_side(at(1)), Face::East);
    assert_eq!(TransporterStack::get_side_between(at(2), at(3)), Face::East);
    assert_eq!(
        TransporterStack::get_side_between(at(2), NO_POS),
        Face::Down
    );
    assert_eq!(TransporterStack::get_side_between(at(2), at(8)), Face::Down);
}

#[test]
fn a_path_is_final_one_transporter_before_the_destination_or_at_the_only_one_of_an_idle_path() {
    let stack = StackRef::new(on_a_path());
    with_path(&stack, vec![at(3), at(2), at(1)], Path::Dest);
    {
        let state = stack.borrow();
        assert!(state.is_final(at(2)));
        assert!(!state.is_final(at(1)));
        assert_eq!(state.side_of_dest(), Some(Face::West));
        assert_eq!(state.get_dest(), at(3));
    }
    with_path(&stack, vec![at(2), at(1)], Path::None);
    let state = stack.borrow();
    assert!(state.is_final(at(2)));
    assert!(!state.is_final(at(1)));
}

#[test]
fn path_types_wrap_around_by_id_and_keep_their_names() {
    assert_eq!(Path::by_id(0), Path::Dest);
    assert_eq!(Path::by_id(4), Path::Home);
    assert_eq!(Path::by_id(-1), Path::None);
    assert!(Path::Home.is_home() && Path::Home.has_target());
    assert!(Path::None.no_target());
    for path in [Path::Dest, Path::Home, Path::None] {
        assert_eq!(
            Path::from_serialized_name(path.serialized_name()),
            Some(path)
        );
    }
}

struct Fixed(Option<u8>, bool);

impl Transporter for Fixed {
    fn color(&self) -> Option<u8> {
        self.0
    }
    fn cost(&self) -> f64 {
        10.0
    }
    fn can_connect(&self, _: Face) -> bool {
        self.1
    }
    fn connection_type(&self, _: Face) -> ConnectionType {
        ConnectionType::Normal
    }
}

#[test]
fn a_stack_enters_a_transporter_of_its_color_or_of_none_that_connects_both_ways() {
    let mut stack = TransporterStack::new();
    stack.color = Some(3);
    let open = Fixed(None, true);
    let same = Fixed(Some(3), true);
    let other = Fixed(Some(4), true);
    let closed = Fixed(None, false);
    assert!(stack.can_insert_to_transporter(Some(&open), Face::East, None));
    assert!(stack.can_insert_to_transporter(Some(&same), Face::East, Some(&open)));
    assert!(!stack.can_insert_to_transporter(Some(&other), Face::East, None));
    assert!(!stack.can_insert_to_transporter(None, Face::East, None));
    assert!(!stack.can_insert_to_transporter(Some(&closed), Face::East, None));
    // O de onde ela vem também tem de ligar para o lado dela.
    assert!(!stack.can_insert_to_transporter(Some(&open), Face::East, Some(&closed)));
    let plain = TransporterStack::new();
    assert!(!plain.can_insert_to_transporter(Some(&same), Face::East, None));
    assert!(plain.can_insert_to_transporter(Some(&open), Face::East, None));
}

fn flying(path: Vec<i64>) -> StackRef {
    let stack = StackRef::new(on_a_path());
    stack.borrow_mut().set_stack(Some(&stone()), 10);
    with_path(&stack, path, Path::Dest);
    stack
}

#[test]
fn the_manager_counts_a_flying_stack_at_its_destination_until_it_is_removed() {
    let manager = TransporterManager::new();
    let dimension = DimensionId(0);
    let a = flying(vec![at(3), at(2)]);
    let b = flying(vec![at(3), at(2)]);
    manager.add(dimension, &a, None);
    manager.add(dimension, &b, None);
    manager.add(dimension, &b, None);
    assert_eq!(manager.flowing_to(dimension, at(3)).len(), 2);
    assert_eq!(manager.flowing_destinations(), 1);
    manager.remove(dimension, &a, None);
    assert_eq!(manager.flowing_to(dimension, at(3)).len(), 1);
    manager.remove(dimension, &b, None);
    assert_eq!(manager.flowing_destinations(), 0);
    // Uma pilha sem alvo (o caminho ocioso) não está em lugar nenhum.
    let idle = flying(vec![at(3), at(2)]);
    with_path(&idle, vec![at(3), at(2)], Path::None);
    manager.remove(dimension, &idle, None);
}

#[test]
fn an_aborted_transaction_undoes_what_the_manager_added_and_removed() {
    let manager = TransporterManager::new();
    let dimension = DimensionId(0);
    let a = flying(vec![at(3), at(2)]);
    {
        let transaction = Transaction::open_root();
        manager.add(dimension, &a, Some(&transaction));
        assert_eq!(manager.flowing_to(dimension, at(3)).len(), 1);
        // Sem `commit`, desfaz.
    }
    assert!(manager.flowing_to(dimension, at(3)).is_empty());
    let b = flying(vec![at(3), at(2)]);
    manager.add(dimension, &a, None);
    manager.add(dimension, &b, None);
    {
        let transaction = Transaction::open_root();
        manager.remove(dimension, &a, Some(&transaction));
        assert_eq!(manager.flowing_to(dimension, at(3)).len(), 1);
    }
    assert_eq!(manager.flowing_to(dimension, at(3)).len(), 2);
    let transaction = Transaction::open_root();
    manager.remove(dimension, &a, Some(&transaction));
    transaction.commit();
    assert_eq!(manager.flowing_to(dimension, at(3)).len(), 1);
}

/// Comportamento do original que parece bug e fica: `FlowingJournal.remove` devolve verdadeiro quando tira a última pilha, e o
/// `TransporterManager.remove` tira o destino do mapa na hora, antes de a transação fechar. Se ela desfaz, o diário volta a ter a pilha,
/// mas já não está no mapa, e a pilha em voo deixa de contar na previsão (`TransporterManager.java:42-46`, `:127-130`).
#[test]
fn removing_the_last_stack_inside_a_transaction_drops_the_destination_even_if_it_is_undone() {
    let manager = TransporterManager::new();
    let dimension = DimensionId(0);
    let a = flying(vec![at(3), at(2)]);
    manager.add(dimension, &a, None);
    {
        let transaction = Transaction::open_root();
        manager.remove(dimension, &a, Some(&transaction));
    }
    assert!(manager.flowing_to(dimension, at(3)).is_empty());
    assert_eq!(manager.flowing_destinations(), 0);
}
