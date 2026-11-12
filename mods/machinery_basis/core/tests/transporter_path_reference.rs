//! O destino e o caminho de uma pilha num transportador logístico contra o original: `dev/referencia/rastros/transporter_path_<cenário>.jsonl`
//! (`testes/TransporterPathTraceTest.java`). Cada cenário monta uma rede de transportadores e baús; o teste monta a mesma no mundo
//! simulado daqui (o `Transmitter` da `rede-conexao`, o registro da `rede-registro`, os baús `VanillaContainer`), confere o estado de
//! cada transportador e repete cada consulta do rastro: os aceitadores na ordem da tabela, a lista de destinos com o custo, a resposta, o
//! caminho, o tipo, o `idleDir`, a casa, o NBT da pilha e o alvo do round robin.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transit::TransitRequest;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::inventory_network::{
    RegistryNetworkView, TransporterLookup, calculate_acceptors, on_network_event,
};
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::validator::CompatibleTransmitterValidator;
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use machinery_basis_core::transporter::nbt::{Nbt, to_snbt};
use machinery_basis_core::transporter::node::{
    DiversionControl, LogisticalTransporterBase, TransporterKind, TransporterTier,
};
use machinery_basis_core::transporter::pathfinder::{PathContext, RoundRobinEjector, get_paths};
use machinery_basis_core::transporter::pos::NO_POS;
use machinery_basis_core::transporter::world::{
    InventoryNetworkView, SideConfigView, TransportSorter, Transporter, TransporterWorld,
};
use machinery_basis_core::transporter::{
    PathfinderCache, SidedBlockPos, StackRef, TransporterManager, TransporterStack,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const DIMENSION: DimensionId = DimensionId(0);

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(format!("transporter_path_{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap_or_else(|| panic!("lado {name}"))
}

fn color_index(name: &str) -> u8 {
    machinery_basis_core::side_config::ENUM_COLOR_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or_else(|| panic!("cor {name}")) as u8
}

fn color_of(value: &Value) -> Option<u8> {
    value.as_str().map(color_index)
}

fn item_of(value: &Value) -> ItemResource {
    ItemResource::new(value.as_str().unwrap(), 64)
}

/// Um transportador do mundo simulado.
struct Node {
    pos: BlockPos,
    base: LogisticalTransporterBase,
    hash: i32,
    network: Option<NetworkId>,
    orphan: bool,
}

struct State {
    nodes: Vec<Node>,
    by_pos: HashMap<i64, TransmitterId>,
    chests: HashMap<i64, Rc<VanillaContainer>>,
}

impl State {
    fn node(&self, id: TransmitterId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    fn node_mut(&mut self, id: TransmitterId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    fn chest_handler(&self, pos: BlockPos) -> Option<Rc<dyn ItemHandler>> {
        self.chests
            .get(&pos.as_long())
            .map(|chest| chest.clone() as Rc<dyn ItemHandler>)
    }

    /// O bit de aceitador de um lado: o lado liga (`canConnect`, com o modo do desviador) e há um baú ali (`isValidAcceptor`).
    fn acceptor_bit(&self, id: TransmitterId, side: Face) -> bool {
        let node = self.node(id);
        let neighbor = node.pos.relative(side);
        !self.by_pos.contains_key(&neighbor.as_long())
            && Transporter::can_connect(&node.base, side)
            && self.chests.contains_key(&neighbor.as_long())
    }

    /// `refreshConnections()` de todos: as ligações a outros transportadores e a baús, lidas dos vizinhos.
    fn refresh_connections(&mut self) {
        for index in 0..self.nodes.len() {
            let id = TransmitterId(index as u32);
            let (mut tx_bits, mut acc_bits) = (0u8, 0u8);
            for (side_index, side) in Face::ALL.into_iter().enumerate() {
                let neighbor = self.node(id).pos.relative(side);
                if let Some(other) = self.by_pos.get(&neighbor.as_long()) {
                    if self
                        .node(id)
                        .base
                        .is_valid_transmitter_basic(&self.node(*other).base, side)
                    {
                        tx_bits |= 1 << side_index;
                    }
                } else if self.acceptor_bit(id, side) {
                    acc_bits |= 1 << side_index;
                }
            }
            let transmitter = &mut self.node_mut(id).base.transmitter;
            transmitter.current_transmitter_connections = tx_bits;
            transmitter.acceptor_cache.current_acceptor_connections = acc_bits;
        }
    }
}

struct Host<'a> {
    state: &'a mut State,
    cache: &'a PathfinderCache,
}

impl TransmitterHost<Rc<dyn ItemHandler>> for Host<'_> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.state.node(transmitter).pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        DIMENSION
    }

    fn is_valid(&self, _: TransmitterId) -> bool {
        true
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.state.node(transmitter).orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        self.state.node_mut(transmitter).orphan = orphan;
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.state.node(transmitter).network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        self.state.node_mut(transmitter).network = network;
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.state.node(transmitter).hash
    }

    fn request_update(&mut self, _: TransmitterId) {}

    fn supports_transmission_type(&self, _: TransmitterId, _: TransmitterId) -> bool {
        true
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        self.state
            .node(transmitter)
            .base
            .is_valid_transmitter_basic(&self.state.node(other).base, side)
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.state.by_pos.get(&pos.as_long()).copied()
    }

    fn new_orphan_validator(
        &self,
        _: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<Rc<dyn ItemHandler>>> {
        Box::new(machinery_basis_core::transmission::validator::AlwaysCompatible)
    }

    fn chunk_accessibility_change(
        &mut self,
        _: &mut TransmitterNetworkRegistry<Rc<dyn ItemHandler>>,
        _: TransmitterId,
        _: bool,
    ) {
        unreachable!("os chunks do cenário não mudam")
    }

    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face) {
        let bit = self.state.acceptor_bit(transmitter, side);
        let index = Face::ALL.iter().position(|face| *face == side).unwrap();
        let connections = &mut self
            .state
            .node_mut(transmitter)
            .base
            .transmitter
            .acceptor_cache
            .current_acceptor_connections;
        if bit {
            *connections |= 1 << index;
        } else {
            *connections &= !(1 << index);
        }
    }

    /// `canConnectToAcceptor(side) ? getAcceptor(side) : null`.
    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<Rc<dyn ItemHandler>> {
        let node = self.state.node(transmitter);
        if node
            .base
            .transmitter
            .get_connection_type_raw(side)
            .can_send_to()
            && self.state.acceptor_bit(transmitter, side)
        {
            self.state.chest_handler(node.pos.relative(side))
        } else {
            None
        }
    }

    fn network_event(
        &mut self,
        registry: &TransmitterNetworkRegistry<Rc<dyn ItemHandler>>,
        network: NetworkId,
        event: NetworkEvent,
    ) {
        on_network_event(self.cache, registry.network(network).uuid(), &event);
    }
}

struct Lookup<'a>(&'a State);

impl TransporterLookup for Lookup<'_> {
    fn transporter(&self, id: TransmitterId) -> Option<&dyn Transporter> {
        Some(&self.0.node(id).base)
    }
}

/// O `Level` do cenário: só baús, sem configuração de lados nem ejetor avançado.
struct World<'a>(&'a State);

impl TransporterWorld for World<'_> {
    fn side_configuration(&self, _: i64) -> Option<SideConfigView> {
        None
    }

    fn sorter(&self, _: i64) -> Option<&dyn TransportSorter> {
        None
    }

    fn item_handler(&self, pos: i64, _side: Face) -> Option<Rc<dyn ItemHandler>> {
        self.0
            .chests
            .get(&pos)
            .map(|chest| chest.clone() as Rc<dyn ItemHandler>)
    }
}

#[derive(Default)]
struct Ejector(Option<SidedBlockPos>);

impl RoundRobinEjector for Ejector {
    fn round_robin_target(&self) -> Option<SidedBlockPos> {
        self.0
    }

    fn set_round_robin_target(&mut self, target: Option<SidedBlockPos>) {
        self.0 = target;
    }
}

struct Scenario {
    name: &'static str,
    origin: BlockPos,
    state: State,
    registry: TransmitterNetworkRegistry<Rc<dyn ItemHandler>>,
    manager: TransporterManager,
    cache: PathfinderCache,
    ejector: Ejector,
    last_stack: Option<StackRef>,
}

fn int3(value: &Value) -> [i32; 3] {
    let array = value.as_array().unwrap();
    [
        array[0].as_i64().unwrap() as i32,
        array[1].as_i64().unwrap() as i32,
        array[2].as_i64().unwrap() as i32,
    ]
}

impl Scenario {
    fn abs(&self, rel: &Value) -> BlockPos {
        let [x, y, z] = int3(rel);
        BlockPos::new(self.origin.x + x, self.origin.y + y, self.origin.z + z)
    }

    /// O `long` relativo do rastro (ou `null`) como o empacotado de verdade.
    fn abs_long(&self, rel: &Value) -> i64 {
        if rel.is_null() {
            NO_POS
        } else {
            self.abs(rel).as_long()
        }
    }

    fn rel_path(&self, rel: &Value) -> Vec<i64> {
        rel.as_array()
            .unwrap()
            .iter()
            .map(|pos| self.abs(pos).as_long())
            .collect()
    }

    fn build(name: &'static str, setup: &Value) -> Self {
        let origin = BlockPos::new(
            setup["x"].as_i64().unwrap() as i32,
            setup["y"].as_i64().unwrap() as i32,
            setup["z"].as_i64().unwrap() as i32,
        );
        let mut scenario = Scenario {
            name,
            origin,
            state: State {
                nodes: Vec::new(),
                by_pos: HashMap::new(),
                chests: HashMap::new(),
            },
            registry: TransmitterNetworkRegistry::new({
                let mut next = 0;
                move || {
                    next += 1;
                    Uuid::with_hash_code(next * 7919)
                }
            }),
            manager: TransporterManager::new(),
            cache: PathfinderCache::new(),
            ejector: Ejector::default(),
            last_stack: None,
        };
        for chest in setup["chests"].as_array().unwrap() {
            let pos = scenario.abs(&chest["p"]);
            scenario
                .state
                .chests
                .insert(pos.as_long(), VanillaContainer::new(27, Rc::new(|| {})));
        }
        for (index, transporter) in setup["transporters"].as_array().unwrap().iter().enumerate() {
            let pos = scenario.abs(&transporter["p"]);
            let tier = TransporterTier::by_id(transporter["tier"].as_i64().unwrap() as i32);
            let kind = match transporter["kind"].as_str().unwrap() {
                "logistical" => TransporterKind::Logistical {
                    color: color_of(&transporter["color"]),
                },
                "restrictive" => TransporterKind::Restrictive,
                "diversion" => TransporterKind::Diversion {
                    modes: std::array::from_fn(|side| {
                        DiversionControl::by_id(transporter["modes"][side].as_i64().unwrap() as i32)
                    }),
                },
                other => panic!("tipo {other}"),
            };
            // O sinal de redstone: o desviador do cenário é ligado pelo bloco de redstone que ele tem em cima. O rastro grava o `canConnect`
            // de cada lado, então o sinal é o que faz a conta do modo bater com ele.
            let powered = scenario_powered(name);
            let mut base =
                LogisticalTransporterBase::new(pos, tier, kind, Rc::new(move || powered));
            let raw: [ConnectionType; 6] = std::array::from_fn(|side| {
                ConnectionType::by_id(transporter["raw"][side].as_i64().unwrap() as i32)
            });
            base.transmitter.set_connection_types_raw(raw);
            let id = TransmitterId(index as u32);
            scenario.state.by_pos.insert(pos.as_long(), id);
            scenario.state.nodes.push(Node {
                pos,
                base,
                hash: 100_003 * (index as i32 + 1),
                network: None,
                orphan: true,
            });
        }
        scenario.state.refresh_connections();
        // O registro: cada transportador entra e o primeiro tick forma as redes e confirma os aceitadores.
        for index in 0..scenario.state.nodes.len() {
            let id = TransmitterId(index as u32);
            let host = Host {
                state: &mut scenario.state,
                cache: &scenario.cache,
            };
            scenario.registry.track_transmitter(&host, id);
            scenario.registry.register_orphan_transmitter(&host, id);
        }
        let mut host = Host {
            state: &mut scenario.state,
            cache: &scenario.cache,
        };
        scenario.registry.on_tick(&mut host, true);
        scenario
    }

    /// O estado de cada transportador contra o do original: custo, o que cada lado deixa passar e a rede.
    fn check_setup(&self, setup: &Value) {
        for transporter in setup["transporters"].as_array().unwrap() {
            let pos = self.abs(&transporter["p"]);
            let id = self.state.by_pos[&pos.as_long()];
            let node = self.state.node(id);
            let who = format!("{} {:?}", self.name, transporter["p"]);
            assert_eq!(
                node.base.get_cost(),
                transporter["cost"].as_f64().unwrap(),
                "{who} custo"
            );
            assert_eq!(
                i32::from(node.base.transmitter.current_transmitter_connections),
                transporter["currentTx"].as_i64().unwrap() as i32 & 0xFF,
                "{who} ligações a transportadores"
            );
            assert_eq!(
                i32::from(node.base.transmitter.get_all_current_connections()),
                transporter["currentAll"].as_i64().unwrap() as i32 & 0xFF,
                "{who} ligações"
            );
            for (index, side) in Face::ALL.into_iter().enumerate() {
                assert_eq!(
                    node.base.connection_type(side).ordinal() as i64,
                    transporter["types"][index].as_i64().unwrap(),
                    "{who} tipo de {side:?}"
                );
                assert_eq!(
                    Transporter::can_connect(&node.base, side),
                    transporter["connect"][index].as_bool().unwrap(),
                    "{who} canConnect {side:?}"
                );
                assert_eq!(
                    node.base.can_emit_to(side),
                    transporter["emit"][index].as_bool().unwrap(),
                    "{who} canEmitTo {side:?}"
                );
                assert_eq!(
                    node.base.can_receive_from(side),
                    transporter["receive"][index].as_bool().unwrap(),
                    "{who} canReceiveFrom {side:?}"
                );
            }
            match node.network {
                Some(network) => {
                    let data = self.registry.network(network);
                    assert_eq!(
                        data.transmitters_size() as i64,
                        transporter["netSize"].as_i64().unwrap(),
                        "{who} tamanho da rede"
                    );
                    assert_eq!(
                        data.acceptor_count() as i64,
                        transporter["netAcceptors"].as_i64().unwrap(),
                        "{who} aceitadores"
                    );
                }
                None => assert_eq!(
                    transporter["netSize"].as_i64().unwrap(),
                    0,
                    "{who} sem rede"
                ),
            }
        }
    }

    fn apply_chests(&self, chests: &Value) {
        for chest in chests.as_array().unwrap() {
            let container = &self.state.chests[&self.abs(&chest["p"]).as_long()];
            container.clear();
            for slot in chest["slots"].as_array().unwrap() {
                container.set_item(
                    slot[0].as_u64().unwrap() as usize,
                    Some(item_of(&slot[1])),
                    slot[2].as_u64().unwrap() as u32,
                );
            }
        }
    }

    fn with_view<R>(
        &self,
        start: BlockPos,
        body: impl FnOnce(&PathContext, Option<&dyn InventoryNetworkView>) -> R,
    ) -> R {
        let world = World(&self.state);
        let ctx = PathContext {
            dimension: DIMENSION,
            manager: &self.manager,
            cache: &self.cache,
            world: &world,
        };
        let lookup = Lookup(&self.state);
        let network = self
            .state
            .by_pos
            .get(&start.as_long())
            .and_then(|id| self.state.node(*id).network);
        match network {
            Some(network) => {
                let view = RegistryNetworkView {
                    registry: &self.registry,
                    network,
                    transporters: &lookup,
                };
                body(&ctx, Some(&view))
            }
            None => body(&ctx, None),
        }
    }

    fn check_acceptors(&self, row: &Value, stack: &TransporterStack, request: &TransitRequest) {
        let start = self.abs(&row["start"]);
        let who = format!("{} q{} {}", self.name, row["q"], row["note"]);
        let found: Vec<(i64, Vec<Face>, String, u32)> = self.with_view(start, |ctx, view| {
            match view {
                Some(view) => calculate_acceptors(ctx, view, request, stack, start, None),
                None => Vec::new(),
            }
            .into_iter()
            .map(|data| {
                (
                    data.location.as_long(),
                    data.sides,
                    data.response.item_type().unwrap().id.to_string(),
                    data.response.sending_amount(),
                )
            })
            .collect()
        });
        let expected = row["acceptors"].as_array().unwrap();
        assert_eq!(found.len(), expected.len(), "{who} aceitadores");
        for (found, expected) in found.iter().zip(expected) {
            assert_eq!(
                found.0,
                self.abs(&expected["p"]).as_long(),
                "{who} posição do aceitador"
            );
            let sides: Vec<Face> = expected["sides"]
                .as_array()
                .unwrap()
                .iter()
                .map(|side| face(side.as_str().unwrap()))
                .collect();
            assert_eq!(found.1, sides, "{who} lados do aceitador");
            assert_eq!(found.2, expected["item"].as_str().unwrap(), "{who} item");
            assert_eq!(
                u64::from(found.3),
                expected["amount"].as_u64().unwrap(),
                "{who} quantidade"
            );
        }
    }

    fn check_destinations(
        &self,
        row: &Value,
        stack: &TransporterStack,
        request: &TransitRequest,
        min: u32,
    ) {
        let start = self.abs(&row["start"]);
        let who = format!("{} q{} {}", self.name, row["q"], row["note"]);
        let found: Vec<(Vec<i64>, f64, u32)> = self.with_view(start, |ctx, view| {
            get_paths(ctx, view, start, stack, request, min, None)
                .into_iter()
                .map(|destination| {
                    (
                        destination.path().to_vec(),
                        destination.score(),
                        destination.response_or_empty().sending_amount(),
                    )
                })
                .collect()
        });
        let expected = row["destinations"].as_array().unwrap();
        assert_eq!(found.len(), expected.len(), "{who} destinos");
        for (found, expected) in found.iter().zip(expected) {
            assert_eq!(
                found.0,
                self.rel_path(&expected["path"]),
                "{who} caminho do destino"
            );
            assert_eq!(
                found.1,
                expected["score"].as_f64().unwrap(),
                "{who} custo do destino"
            );
            assert_eq!(
                u64::from(found.2),
                expected["amount"].as_u64().unwrap(),
                "{who} quantidade do destino"
            );
        }
    }

    fn rr_target(&self, value: &Value) -> Option<SidedBlockPos> {
        if value.is_null() {
            return None;
        }
        Some(SidedBlockPos {
            pos: self.abs(&value["p"]).as_long(),
            side: face(value["side"].as_str().unwrap()),
        })
    }

    /// O estado da pilha depois da consulta, contra o que o rastro gravou.
    fn check_stack(&self, row: &Value, stack: &StackRef) {
        let who = format!("{} q{} {}", self.name, row["q"], row["note"]);
        let stack = stack.borrow();
        assert_eq!(
            stack.path_type().serialized_name(),
            row["pathType"].as_str().unwrap(),
            "{who} tipo do caminho"
        );
        assert_eq!(stack.path(), self.rel_path(&row["path"]), "{who} caminho");
        assert_eq!(
            stack.idle_dir.map(|side| side.name()),
            row["idleDir"].as_str(),
            "{who} idleDir"
        );
        assert_eq!(
            stack.home_location,
            self.abs_long(&row["home"]),
            "{who} casa"
        );
        assert_eq!(
            stack.original_location,
            self.abs_long(&row["original"]),
            "{who} origem"
        );
        assert_eq!(
            i64::from(stack.progress),
            row["progress"].as_i64().unwrap(),
            "{who} progresso"
        );
        assert_eq!(
            stack.has_path(),
            row["hasPath"].as_bool().unwrap(),
            "{who} hasPath"
        );
        assert_eq!(
            stack.side_of_dest().map(|side| side.name()),
            row["sideOfDest"].as_str(),
            "{who} lado do destino"
        );
        assert_eq!(
            to_snbt(&Nbt::Compound(stack.write())),
            row["nbt"].as_str().unwrap(),
            "{who} NBT"
        );
    }

    fn query(&mut self, row: &Value) {
        let who = format!("{} q{} {}", self.name, row["q"], row["note"]);
        self.apply_chests(&row["chests"]);
        let start = self.abs(&row["start"]);
        let item = item_of(&row["item"]);
        let amount = row["amount"].as_u64().unwrap() as u32;
        let min = row["min"].as_u64().unwrap() as u32;
        let color = color_of(&row["color"]);
        let home = self.abs(&row["homePos"]).as_long();
        let stack = StackRef::new(TransporterStack::for_insert(home, color));
        let request = TransitRequest::simple(&item, amount);
        let rr = row["kind"].as_str().unwrap() == "rr";
        self.ejector = Ejector(self.rr_target(&row["rrBefore"]));
        let local = stack.borrow().clone();
        self.check_acceptors(row, &local, &request);
        self.check_destinations(row, &local, &request, min);
        let response = {
            let world = World(&self.state);
            let ctx = PathContext {
                dimension: DIMENSION,
                manager: &self.manager,
                cache: &self.cache,
                world: &world,
            };
            let lookup = Lookup(&self.state);
            let network = self
                .state
                .by_pos
                .get(&start.as_long())
                .and_then(|id| self.state.node(*id).network);
            let view = network.map(|network| RegistryNetworkView {
                registry: &self.registry,
                network,
                transporters: &lookup,
            });
            let view: Option<&dyn InventoryNetworkView> =
                view.as_ref().map(|view| view as &dyn InventoryNetworkView);
            if rr {
                stack.recalculate_rr_path(&ctx, view, start, &request, &mut self.ejector, min, None)
            } else {
                stack.recalculate_path(&ctx, view, start, &request, min, None)
            }
        };
        assert_eq!(
            response.is_empty(),
            row["empty"].as_bool().unwrap(),
            "{who} vazia"
        );
        assert_eq!(
            u64::from(response.sending_amount()),
            row["respAmount"].as_u64().unwrap(),
            "{who} quantidade enviada"
        );
        if !response.is_empty() {
            assert_eq!(
                response.item_type().unwrap().id.as_ref(),
                row["respItem"].as_str().unwrap(),
                "{who} item enviado"
            );
            let sending = response.sending_amount();
            stack.borrow_mut().set_stack(response.item_type(), sending);
        }
        self.check_stack(row, &stack);
        assert_eq!(
            self.ejector.0,
            self.rr_target(&row["rrAfter"]),
            "{who} alvo do round robin"
        );
        if !row["keep"].as_bool().unwrap() {
            self.manager.remove(DIMENSION, &stack, None);
        }
    }

    fn idle(&mut self, row: &Value) {
        let who = format!("{} q{} {}", self.name, row["q"], row["note"]);
        self.apply_chests(&row["chests"]);
        let start = self.abs(&row["start"]);
        let stack = if row["reuse"].as_bool().unwrap() {
            self.last_stack.clone().expect("a pilha anterior")
        } else {
            let mut fresh = TransporterStack::new();
            fresh.set_stack(
                Some(&item_of(&row["item"])),
                row["amount"].as_u64().unwrap() as u32,
            );
            fresh.color = color_of(&row["inColor"]);
            fresh.home_location = self.abs_long(&row["inHome"]);
            fresh.idle_dir = row["inIdleDir"].as_str().map(face);
            fresh.progress = row["inProgress"].as_i64().unwrap() as i32;
            fresh.original_location = self.abs_long(&row["inOriginal"]);
            StackRef::new(fresh)
        };
        let found = self.with_view(start, |ctx, view| {
            stack.calculate_idle(ctx, view, start, None)
        });
        assert_eq!(found, row["found"].as_bool().unwrap(), "{who} achou");
        self.check_stack(row, &stack);
        self.manager.remove(DIMENSION, &stack, None);
        self.last_stack = Some(stack);
    }

    fn set_color(&mut self, row: &Value) {
        let pos = self.abs(&row["p"]);
        let id = self.state.by_pos[&pos.as_long()];
        let network = self.state.node(id).network;
        self.state
            .node_mut(id)
            .base
            .set_color(color_of(&row["color"]));
        // `LogisticalTransporter.onConfigure`: a cor mudou, os caminhos guardados da rede não valem mais.
        if let Some(network) = network {
            self.cache.on_changed(self.registry.network(network).uuid());
        }
    }
}

/// Se há sinal de redstone no desviador do cenário: o nome diz (`diversion_<modo>_on`).
fn scenario_powered(name: &str) -> bool {
    name.ends_with("_on")
}

fn run(name: &'static str) {
    let rows = read(name);
    let setup = &rows[0];
    assert_eq!(setup["e"], "setup");
    let mut scenario = Scenario::build(name, setup);
    scenario.check_setup(setup);
    let mut queries = 0;
    for row in &rows[1..] {
        match row["e"].as_str().unwrap() {
            "query" => {
                scenario.query(row);
                queries += 1;
            }
            "idle" => {
                scenario.idle(row);
                queries += 1;
            }
            "color" => scenario.set_color(row),
            "mark" => {}
            "error" => panic!("o rastro de {name} acabou em erro: {}", row["m"]),
            other => panic!("linha {other}"),
        }
    }
    assert!(queries >= 3, "{name} tem consultas de menos: {queries}");
}

#[test]
fn a_line_with_a_chest_at_its_end() {
    run("line");
}

#[test]
fn two_chests_where_the_far_one_is_cheaper_to_reach() {
    run("two_ultimate");
}

#[test]
fn two_chests_at_different_distances() {
    run("two_basic");
}

#[test]
fn a_full_chest_a_nearly_full_one_and_stacks_in_flight() {
    run("full");
}

#[test]
fn the_round_robin_of_an_ejector_over_three_destinations() {
    run("round_robin");
}

#[test]
fn colored_transporters_only_let_the_right_stacks_through() {
    run("colors");
}

#[test]
fn a_restrictive_transporter_that_is_the_only_way() {
    run("restrictive_only");
}

#[test]
fn a_restrictive_transporter_that_can_be_avoided() {
    run("restrictive_ladder");
}

#[test]
fn a_diverter_on_high_without_signal() {
    run("diversion_high_off");
}

#[test]
fn a_diverter_on_high_with_signal() {
    run("diversion_high_on");
}

#[test]
fn a_diverter_on_low_without_signal() {
    run("diversion_low_off");
}

#[test]
fn a_diverter_on_low_with_signal() {
    run("diversion_low_on");
}

#[test]
fn no_destination_sends_the_stack_home_or_idling() {
    run("no_destination");
}

#[test]
fn a_single_transporter_between_two_chests() {
    run("single");
}

#[test]
fn a_ring_where_both_halves_cost_the_same() {
    run("loop_tie");
}

#[test]
fn a_ring_where_one_half_is_cheaper() {
    run("loop_ultimate");
}

#[test]
fn a_chest_touched_by_two_routes_keeps_a_cached_path_per_side_and_the_flight_of_one() {
    run("two_sides");
}

#[test]
fn two_destinations_with_the_same_cost_are_ordered_by_path_length_when_the_long_one_is_on_the_left()
{
    run("tie_length_left");
}

#[test]
fn two_destinations_with_the_same_cost_are_ordered_by_path_length_when_the_long_one_is_on_the_right()
 {
    run("tie_length_right");
}

#[test]
fn the_way_home_to_a_chest_whose_transporter_pulls() {
    run("pull_home");
}

#[test]
fn a_plane_of_transporters_where_many_paths_cost_the_same() {
    run("grid");
}

#[test]
fn a_destination_only_reachable_through_a_detour_beyond_the_search_floor() {
    run("detour");
}
