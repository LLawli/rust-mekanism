//! As regras de `TransporterUtils.canInsert` e do filtro de cor de `InventoryNetwork.calculateAcceptors` sobre um destino que é um
//! `ISideConfiguration` (entrada estrita, cor de entrada por lado) ou um `IAdvancedTransportEjector` (o Logistical Sorter). O rastro do
//! original não tem uma máquina de destino, então as contas são as de `TransporterUtils.java:77-106` e `InventoryNetwork.java:61-73`.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transit::{TransitRequest, TransitResponse};
use machinery_basis_core::transmission::DimensionId;
use machinery_basis_core::transmission::Uuid;
use machinery_basis_core::transmission::inventory_network::calculate_acceptors;
use machinery_basis_core::transporter::pathfinder::PathContext;
use machinery_basis_core::transporter::world::{
    AcceptorEntry, InventoryNetworkView, SideConfigView, TransportSorter, Transporter,
    TransporterWorld, can_insert,
};
use machinery_basis_core::transporter::{PathfinderCache, TransporterManager, TransporterStack};
use std::rc::Rc;

fn stone() -> ItemResource {
    ItemResource::new("minecraft:stone", 64)
}

const CHEST: BlockPos = BlockPos::new(5, 0, 0);

struct Machine {
    chest: Rc<VanillaContainer>,
    config: Option<SideConfigView>,
    sorter_answer: Option<Sorter>,
}

struct Sorter(bool);

impl TransportSorter for Sorter {
    fn can_send_home(&self, _: &ItemResource, _: u32, _: Option<&Transaction>) -> bool {
        self.0
    }

    fn send_home(&self, _: &TransitRequest, _: &Transaction) -> TransitResponse {
        TransitResponse::EMPTY
    }
}

impl TransporterWorld for Machine {
    fn side_configuration(&self, _: i64) -> Option<SideConfigView> {
        self.config
    }

    fn sorter(&self, _: i64) -> Option<&dyn TransportSorter> {
        self.sorter_answer
            .as_ref()
            .map(|answer| answer as &dyn TransportSorter)
    }

    fn item_handler(&self, _: i64, _: Face) -> Option<Rc<dyn ItemHandler>> {
        Some(self.chest.clone())
    }

    fn neighbor_transmitter_can_connect(&self, _: i64, _: Face) -> Option<bool> {
        None
    }
}

fn machine(config: Option<SideConfigView>, sorter_answer: Option<Sorter>) -> Machine {
    Machine {
        chest: VanillaContainer::new(27, Rc::new(|| {})),
        config,
        sorter_answer,
    }
}

/// A entrada estrita pede RED (índice 12) pelo lado oeste do destino, o lado por onde uma pilha vinda do oeste entra.
fn strict_red_on_the_west() -> SideConfigView {
    let mut colors = [None; 6];
    colors[4] = Some(12);
    SideConfigView {
        strict_input: true,
        input_colors: colors,
    }
}

#[test]
fn a_strict_input_only_takes_the_color_of_the_side_it_enters_by() {
    let world = machine(Some(strict_red_on_the_west()), None);
    let pos = CHEST.as_long();
    // A pilha anda para o leste e toca o destino pelo oeste (`side.opposite()`).
    let insert =
        |color, force| can_insert(&world, pos, color, &stone(), 5, Face::East, force, None);
    assert!(insert(Some(12), false));
    assert!(!insert(None, false));
    assert!(!insert(Some(3), false));
    // Voltando para casa (`force`), a entrada estrita não vale.
    assert!(insert(None, true));
    // Outro lado não tem cor: qualquer pilha entra.
    assert!(can_insert(
        &world,
        pos,
        None,
        &stone(),
        5,
        Face::South,
        false,
        None
    ));
    // Sem a entrada estrita, a cor do lado é só enfeite.
    let mut loose = strict_red_on_the_west();
    loose.strict_input = false;
    let world = machine(Some(loose), None);
    assert!(can_insert(
        &world,
        pos,
        None,
        &stone(),
        5,
        Face::East,
        false,
        None
    ));
}

#[test]
fn coming_home_to_a_sorter_asks_the_sorter_and_nothing_else() {
    let pos = CHEST.as_long();
    let yes = machine(None, Some(Sorter(true)));
    let no = machine(None, Some(Sorter(false)));
    assert!(can_insert(
        &yes,
        pos,
        None,
        &stone(),
        5,
        Face::East,
        true,
        None
    ));
    assert!(!can_insert(
        &no,
        pos,
        None,
        &stone(),
        5,
        Face::East,
        true,
        None
    ));
    // Indo para um destino (sem `force`) o sorter é um inventário como outro qualquer.
    assert!(can_insert(
        &no,
        pos,
        None,
        &stone(),
        5,
        Face::East,
        false,
        None
    ));
}

#[test]
fn an_empty_amount_always_fits_and_a_full_destination_never_does() {
    let world = machine(None, None);
    let pos = CHEST.as_long();
    assert!(can_insert(
        &world,
        pos,
        None,
        &stone(),
        0,
        Face::East,
        false,
        None
    ));
    for slot in 0..27 {
        world.chest.set_item(slot, Some(stone()), 64);
    }
    assert!(!can_insert(
        &world,
        pos,
        None,
        &stone(),
        1,
        Face::East,
        false,
        None
    ));
    assert!(can_insert(
        &world,
        pos,
        None,
        &stone(),
        0,
        Face::East,
        false,
        None
    ));
}

struct View(Rc<dyn ItemHandler>);

impl InventoryNetworkView for View {
    fn uuid(&self) -> Uuid {
        Uuid::with_hash_code(1)
    }
    fn transmitter(&self, _: i64) -> Option<&dyn Transporter> {
        None
    }
    fn acceptors(&self) -> Vec<AcceptorEntry> {
        // O aceitador do destino pelo lado oeste dele, o que a cor de entrada estrita vigia.
        vec![(CHEST.as_long(), vec![(Face::West, self.0.clone())])]
    }
}

#[test]
fn the_acceptor_list_skips_a_destination_whose_strict_input_color_differs() {
    let world = machine(Some(strict_red_on_the_west()), None);
    let view = View(world.chest.clone());
    let manager = TransporterManager::new();
    let cache = PathfinderCache::new();
    let ctx = PathContext {
        dimension: DimensionId(0),
        manager: &manager,
        cache: &cache,
        world: &world,
    };
    let request = TransitRequest::simple(&stone(), 5);
    let list = |color| {
        let mut stack = TransporterStack::for_insert(BlockPos::new(0, 0, 0).as_long(), color);
        stack.set_stack(Some(&stone()), 5);
        calculate_acceptors(&ctx, &view, &request, &stack, BlockPos::new(4, 0, 0), None).len()
    };
    assert_eq!(list(Some(12)), 1);
    assert_eq!(list(None), 0);
    assert_eq!(list(Some(3)), 0);
    // O destino que é a casa da pilha nunca entra.
    let mut home = TransporterStack::for_insert(CHEST.as_long(), Some(12));
    home.set_stack(Some(&stone()), 5);
    assert!(
        calculate_acceptors(&ctx, &view, &request, &home, BlockPos::new(4, 0, 0), None).is_empty()
    );
}
