//! O movimento das pilhas nos transportadores logísticos contra o original, tick a tick: `dev/referencia/rastros/transporter_move_<cenário>.jsonl`
//! (`testes/TransporterMoveTraceTest.java`). Cada cenário monta transportadores e contêineres (baús e droppers) e o original anda com eles; o teste monta
//! o mesmo mundo simulado (o `Transmitter` da `rede-conexao`, o registro da `rede-registro`, contêineres `VanillaContainer`), aplica as ações (`mark`) na hora
//! em que o original as aplicou e roda os transportadores na ordem do original (`onUpdateServer` de cada um, depois o `onTick` do registro, como o
//! `ServerTickEvent.Post`). Depois de cada tick confere cada transportador (as pilhas em trânsito na ordem da tabela do fastutil, com o id, o
//! `System.identityHashCode`, o progresso, o caminho, o próximo e o anterior, o NBT salvo, o de atualização e o `STREAM_CODEC` em hexadecimal), os
//! contêineres e o que caiu no mundo.
//!
//! Os `System.identityHashCode` das pilhas vêm do rastro (decisão 118): o teste os dá na ordem em que as pilhas aparecem.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::identity_hash::{feed_identity_hashes, pending_fed_identity_hashes};
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::inventory_network::{
    RegistryNetworkView, TransporterLookup, on_network_event,
};
use machinery_basis_core::transmission::registry::TransmitterNetworkRegistry;
use machinery_basis_core::transmission::validator::CompatibleTransmitterValidator;
use machinery_basis_core::transmission::{
    DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid,
};
use machinery_basis_core::transporter::movement::{
    MoveContext, TransporterEnv, TransporterItemHandler, WorldDrop,
};
use machinery_basis_core::transporter::nbt::to_snbt;
use machinery_basis_core::transporter::node::{
    DiversionControl, DiversionNeighbor, LogisticalTransporterBase, TransporterKind, TransporterTier,
};
use machinery_basis_core::transporter::pathfinder::PathContext;
use machinery_basis_core::transporter::stack::write_var_int;
use machinery_basis_core::transporter::world::{
    InventoryNetworkView, SideConfigView, TransportSorter, Transporter, TransporterWorld,
};
use machinery_basis_core::transporter::{PathfinderCache, TransporterManager};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

const DIMENSION: DimensionId = DimensionId(0);

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(format!("transporter_move_{name}.jsonl"));
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
    removed: bool,
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

    fn chest_handler_at(&self, pos: i64) -> Option<Rc<dyn ItemHandler>> {
        self.chests
            .get(&pos)
            .map(|chest| chest.clone() as Rc<dyn ItemHandler>)
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
            if self.nodes[index].removed {
                continue;
            }
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

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        !self.state.node(transmitter).removed
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

/// O `Level` do cenário: contêineres, sem configuração de lados nem ejetor avançado.
struct World<'a>(&'a State);

impl TransporterWorld for World<'_> {
    fn side_configuration(&self, _: i64) -> Option<SideConfigView> {
        None
    }

    fn sorter(&self, _: i64) -> Option<&dyn TransportSorter> {
        None
    }

    fn item_handler(&self, pos: i64, _side: Face) -> Option<Rc<dyn ItemHandler>> {
        self.0.chest_handler_at(pos)
    }

    fn neighbor_transmitter_can_connect(&self, pos: i64, side: Face) -> Option<bool> {
        let id = self.0.by_pos.get(&pos)?;
        Some(Transporter::can_connect(&self.0.node(*id).base, side))
    }
}

/// O que os transportadores e o `TransporterItemHandler` enxergam do mundo.
struct Shared {
    state: RefCell<State>,
    registry: RefCell<TransmitterNetworkRegistry<Rc<dyn ItemHandler>>>,
    manager: TransporterManager,
    cache: PathfinderCache,
}

impl TransporterEnv for Shared {
    fn with_transporter(
        &self,
        pos: i64,
        body: &mut dyn FnMut(&LogisticalTransporterBase, &MoveContext),
    ) {
        let state = self.state.borrow();
        let Some(id) = state.by_pos.get(&pos).copied() else {
            return;
        };
        let node = state.node(id);
        let world = World(&state);
        let ctx = PathContext {
            dimension: DIMENSION,
            manager: &self.manager,
            cache: &self.cache,
            world: &world,
        };
        let registry = self.registry.borrow();
        let lookup = Lookup(&state);
        match node.network {
            Some(network) => {
                let view = RegistryNetworkView {
                    registry: &registry,
                    network,
                    transporters: &lookup,
                };
                let view: &dyn InventoryNetworkView = &view;
                body(
                    &node.base,
                    &MoveContext {
                        path: &ctx,
                        network: Some(view),
                    },
                );
            }
            None => body(
                &node.base,
                &MoveContext {
                    path: &ctx,
                    network: None,
                },
            ),
        }
    }
}

/// `[x, y, z]` do rastro.
fn int3(value: &Value) -> [i32; 3] {
    let array = value.as_array().unwrap();
    [
        array[0].as_i64().unwrap() as i32,
        array[1].as_i64().unwrap() as i32,
        array[2].as_i64().unwrap() as i32,
    ]
}

struct Scenario {
    name: &'static str,
    origin: BlockPos,
    shared: Rc<Shared>,
    /// A ordem em que o original anda os transportadores num tick (a dos block entities, `y`, `x`, `z`).
    order: Vec<usize>,
    power: Rc<Cell<bool>>,
    /// A posição relativa de cada transportador do `setup`, na ordem dele.
    relative: Vec<[i32; 3]>,
    /// Os contêineres do `setup`: a posição relativa e a empacotada.
    containers: Vec<([i32; 3], i64)>,
    expected: HashMap<usize, Value>,
    statics: HashMap<(usize, i32, i64), Value>,
    expected_chests: Vec<Value>,
    drops: RefCell<Vec<WorldDrop>>,
    tick: u32,
}

impl Scenario {
    fn abs(&self, rel: [i32; 3]) -> BlockPos {
        BlockPos::new(
            self.origin.x + rel[0],
            self.origin.y + rel[1],
            self.origin.z + rel[2],
        )
    }

    fn abs_value(&self, rel: &Value) -> BlockPos {
        self.abs(int3(rel))
    }

    fn rel_long(&self, packed: i64) -> Value {
        if packed == machinery_basis_core::transporter::pos::NO_POS {
            return Value::Null;
        }
        let pos = machinery_basis_core::transporter::pos::block_pos_of(packed);
        serde_json::json!([
            pos.x - self.origin.x,
            pos.y - self.origin.y,
            pos.z - self.origin.z
        ])
    }

    fn rel_block(&self, pos: BlockPos) -> Value {
        serde_json::json!([
            pos.x - self.origin.x,
            pos.y - self.origin.y,
            pos.z - self.origin.z
        ])
    }

    fn build(name: &'static str, setup: &Value) -> Self {
        let origin = BlockPos::new(
            setup["x"].as_i64().unwrap() as i32,
            setup["y"].as_i64().unwrap() as i32,
            setup["z"].as_i64().unwrap() as i32,
        );
        let power = Rc::new(Cell::new(false));
        let mut state = State {
            nodes: Vec::new(),
            by_pos: HashMap::new(),
            chests: HashMap::new(),
        };
        let mut containers = Vec::new();
        for container in setup["containers"].as_array().unwrap() {
            let rel = int3(&container["p"]);
            let pos = BlockPos::new(origin.x + rel[0], origin.y + rel[1], origin.z + rel[2]);
            state.chests.insert(
                pos.as_long(),
                VanillaContainer::new(container["size"].as_u64().unwrap() as usize, Rc::new(|| {})),
            );
            containers.push((rel, pos.as_long()));
        }
        let mut relative = Vec::new();
        for (index, transporter) in setup["transporters"].as_array().unwrap().iter().enumerate() {
            let rel = int3(&transporter["p"]);
            relative.push(rel);
            let pos = BlockPos::new(origin.x + rel[0], origin.y + rel[1], origin.z + rel[2]);
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
            let signal = power.clone();
            let mut base = LogisticalTransporterBase::new(
                pos,
                tier,
                kind,
                Rc::new(move || signal.get()),
            );
            let raw: [ConnectionType; 6] = std::array::from_fn(|side| {
                ConnectionType::by_id(transporter["raw"][side].as_i64().unwrap() as i32)
            });
            base.transmitter.set_connection_types_raw(raw);
            let id = TransmitterId(index as u32);
            state.by_pos.insert(pos.as_long(), id);
            state.nodes.push(Node {
                pos,
                base,
                hash: 100_003 * (index as i32 + 1),
                network: None,
                orphan: true,
                removed: false,
            });
        }
        state.refresh_connections();
        let cache = PathfinderCache::new();
        let mut registry = TransmitterNetworkRegistry::new({
            let mut next = 0;
            move || {
                next += 1;
                Uuid::with_hash_code(next * 7919)
            }
        });
        for index in 0..state.nodes.len() {
            let id = TransmitterId(index as u32);
            let host = Host {
                state: &mut state,
                cache: &cache,
            };
            registry.track_transmitter(&host, id);
            registry.register_orphan_transmitter(&host, id);
        }
        let mut host = Host {
            state: &mut state,
            cache: &cache,
        };
        registry.on_tick(&mut host, true);
        let mut order: Vec<usize> = (0..relative.len()).collect();
        order.sort_by_key(|index| (relative[*index][1], relative[*index][0], relative[*index][2]));
        Scenario {
            name,
            origin,
            shared: Rc::new(Shared {
                state: RefCell::new(state),
                registry: RefCell::new(registry),
                manager: TransporterManager::new(),
                cache,
            }),
            order,
            power,
            relative,
            containers,
            expected: HashMap::new(),
            statics: HashMap::new(),
            expected_chests: Vec::new(),
            drops: RefCell::new(Vec::new()),
            tick: 0,
        }
    }

    fn pos_long(&self, index: usize) -> i64 {
        self.abs(self.relative[index]).as_long()
    }

    /// Um tick do servidor: os block entities na ordem, e o `onTick` do registro no `ServerTickEvent.Post`.
    fn run_tick(&mut self) {
        self.tick += 1;
        for index in self.order.clone() {
            let pos = self.pos_long(index);
            let drops = &self.drops;
            self.shared.with_transporter(pos, &mut |base, ctx| {
                base.on_update_server(ctx);
                drops.borrow_mut().extend(base.take_world_drops());
            });
        }
        let mut state = self.shared.state.borrow_mut();
        let mut registry = self.shared.registry.borrow_mut();
        let mut host = Host {
            state: &mut state,
            cache: &self.shared.cache,
        };
        registry.on_tick(&mut host, true);
    }

    fn container(&self, rel: &Value) -> Rc<VanillaContainer> {
        let pos = self.abs_value(rel).as_long();
        self.shared.state.borrow().chests[&pos].clone()
    }

    fn apply_mark(&mut self, row: &Value) {
        let who = format!("{} t{} {}", self.name, row["t"], row["k"]);
        match row["k"].as_str().unwrap() {
            "fill" => self.container(&row["p"]).set_item(
                row["slot"].as_u64().unwrap() as usize,
                Some(item_of(&row["item"])),
                row["n"].as_u64().unwrap() as u32,
            ),
            "cap_insert" => {
                let index = self
                    .relative
                    .iter()
                    .position(|rel| *rel == int3(&row["p"]))
                    .unwrap();
                let side = face(row["side"].as_str().unwrap());
                let item = item_of(&row["item"]);
                let amount = row["n"].as_u64().unwrap() as u32;
                let env: Rc<dyn TransporterEnv> = self.shared.clone();
                let handler = {
                    let state = self.shared.state.borrow();
                    TransporterItemHandler::for_side(env, &state.node(TransmitterId(index as u32)).base, side)
                };
                let inserted = handler.map_or(-1_i64, |handler| {
                    let transaction = Transaction::open_root();
                    let inserted = handler.insert(&item, amount, &transaction);
                    transaction.commit();
                    i64::from(inserted)
                });
                assert_eq!(inserted, row["inserted"].as_i64().unwrap(), "{who} inserido");
            }
            "power" => {
                self.power.set(row["on"].as_bool().unwrap());
                self.neighbor_changed(&row["p"]);
            }
            "break" => self.break_transporter(&row["p"]),
            "finishing" | "finish" => {}
            other => panic!("ação {other}"),
        }
    }

    /// As ligações de cada transportador antes e depois de `refreshConnections()`: as de transportador (e o que isso pede à rede) e as de aceitador.
    fn refresh_and_notify(&mut self, diverter: Option<usize>) {
        let bits = |scenario: &Scenario| -> Vec<(u8, u8)> {
            scenario
                .shared
                .state
                .borrow()
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.base.transmitter.current_transmitter_connections,
                        node.base.transmitter.acceptor_cache.current_acceptor_connections,
                    )
                })
                .collect()
        };
        let before = bits(self);
        self.shared.state.borrow_mut().refresh_connections();
        let after = bits(self);
        for index in 0..before.len() {
            let id = TransmitterId(index as u32);
            let (tx_changed, acc_changed) = (
                before[index].0 != after[index].0,
                before[index].1 != after[index].1,
            );
            let mut state = self.shared.state.borrow_mut();
            let network = state.node(id).network;
            let mut registry = self.shared.registry.borrow_mut();
            let host = Host {
                state: &mut state,
                cache: &self.shared.cache,
            };
            // Os desviadores invalidam a rede por qualquer mudança de ligação (`markDirtyTransmitters`); os outros, só pelas de transportador.
            let dirty = if diverter == Some(index) {
                tx_changed || acc_changed
            } else {
                tx_changed
            };
            if dirty {
                registry.invalidate_transmitter(&host, id);
            } else if acc_changed && let Some(network) = network {
                for (side_index, side) in Face::ALL.into_iter().enumerate() {
                    if (before[index].1 ^ after[index].1) & (1 << side_index) != 0 {
                        registry.network_acceptor_changed(&host, network, id, side);
                    }
                }
            }
        }
    }

    /// O bloco de redstone muda de lugar: o desviador ao lado recebe `neighborChanged`.
    fn neighbor_changed(&mut self, redstone: &Value) {
        let at = self.abs_value(redstone);
        for side in Face::ALL {
            let neighbor = at.relative(side);
            let Some(index) = self
                .shared
                .state
                .borrow()
                .by_pos
                .get(&neighbor.as_long())
                .map(|id| id.0 as usize)
            else {
                continue;
            };
            let step = {
                let state = self.shared.state.borrow();
                state.node(TransmitterId(index as u32)).base.diversion_neighbor_change(None)
            };
            match step {
                DiversionNeighbor::PowerChanged | DiversionNeighbor::Refresh(_) => {
                    self.refresh_and_notify(Some(index));
                }
            }
        }
    }

    /// `setBlock(AIR)` num transportador: solta o que ele leva, esquece o que estava em voo e a rede o invalida.
    fn break_transporter(&mut self, p: &Value) {
        let rel = int3(p);
        let index = self.relative.iter().position(|r| *r == rel).unwrap();
        let pos = self.pos_long(index);
        let drops = &self.drops;
        self.shared.with_transporter(pos, &mut |base, ctx| {
            base.pre_remove_side_effects(ctx);
            drops.borrow_mut().extend(base.take_world_drops());
            base.remove_transit(ctx.path);
        });
        {
            let mut state = self.shared.state.borrow_mut();
            let id = TransmitterId(index as u32);
            state.node_mut(id).removed = true;
            state.by_pos.remove(&pos);
            let mut registry = self.shared.registry.borrow_mut();
            let host = Host {
                state: &mut state,
                cache: &self.shared.cache,
            };
            registry.untrack_transmitter(&host, id);
            registry.invalidate_transmitter(&host, id);
        }
        self.refresh_and_notify(None);
    }

    fn check_row(&mut self, row: &Value) {
        let who = format!("{} t{}", self.name, row["t"]);
        for entry in row["tx"].as_array().unwrap() {
            let index = self
                .relative
                .iter()
                .position(|rel| *rel == int3(&entry["p"]))
                .unwrap_or_else(|| panic!("{who}: transportador {}", entry["p"]));
            let mut entry = entry.clone();
            entry["fresh"] = Value::Bool(true);
            self.expected.insert(index, entry);
        }
        if let Some(chests) = row.get("chests") {
            self.expected_chests = chests.as_array().unwrap().clone();
        }
        let delays = row["dl"].as_array().unwrap();
        for index in 0..self.relative.len() {
            let expected = self.expected.get(&index).cloned();
            let delay = delays[index].as_i64().unwrap();
            let pos = self.pos_long(index);
            let me = format!("{who} {:?}", self.relative[index]);
            let mut checks: Vec<String> = Vec::new();
            let statics = &mut self.statics;
            let origin = self.origin;
            self.shared.with_transporter(pos, &mut |base, _| {
                let (actual_delay, delay_count, next_id) = base.counters();
                if delay >= 0 {
                    assert_eq!(i64::from(actual_delay), delay, "{me} delay");
                }
                let Some(expected) = &expected else {
                    return;
                };
                if expected.get("gone").is_some() {
                    return;
                }
                let fresh = expected.get("fresh").is_some();
                assert_eq!(
                    i64::from(delay_count),
                    expected["dc"].as_i64().unwrap(),
                    "{me} delayCount"
                );
                assert_eq!(i64::from(next_id), expected["nid"].as_i64().unwrap(), "{me} nextId");
                let rel = |packed: i64| -> Value {
                    if packed == machinery_basis_core::transporter::pos::NO_POS {
                        return Value::Null;
                    }
                    let p = machinery_basis_core::transporter::pos::block_pos_of(packed);
                    serde_json::json!([p.x - origin.x, p.y - origin.y, p.z - origin.z])
                };
                let stacks = base.transit_stacks();
                let expected_stacks = expected["transit"].as_array().unwrap();
                assert_eq!(
                    stacks.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    expected_stacks
                        .iter()
                        .map(|stack| stack["id"].as_i64().unwrap() as i32)
                        .collect::<Vec<_>>(),
                    "{me} ids em trânsito, na ordem da tabela"
                );
                for ((id, stack), want) in stacks.iter().zip(expected_stacks) {
                    let state = stack.borrow();
                    let hash = state.identity_hash();
                    assert_eq!(i64::from(hash), want["h"].as_i64().unwrap(), "{me} #{id} hash");
                    assert_eq!(
                        i64::from(state.progress),
                        want["prog"].as_i64().unwrap(),
                        "{me} #{id} progresso"
                    );
                    let key = (index, *id, i64::from(hash));
                    let full = if want.get("s").is_some() {
                        statics
                            .get(&key)
                            .unwrap_or_else(|| panic!("{me} #{id}: sem a pilha inteira antes"))
                            .clone()
                    } else {
                        statics.insert(key, want.clone());
                        want.clone()
                    };
                    let tag = format!("{me} #{id}");
                    assert_eq!(
                        state.item_type().map(|item| item.id.to_string()),
                        full["item"].as_str().map(str::to_owned),
                        "{tag} item"
                    );
                    assert_eq!(i64::from(state.size()), full["n"].as_i64().unwrap(), "{tag} quantidade");
                    assert_eq!(
                        state.color.map(|c| machinery_basis_core::side_config::ENUM_COLOR_NAMES[c as usize]),
                        full["color"].as_str(),
                        "{tag} cor"
                    );
                    assert_eq!(state.initiated_path, full["init"].as_bool().unwrap(), "{tag} initiatedPath");
                    assert_eq!(state.idle_dir.map(Face::name), full["idle"].as_str(), "{tag} idleDir");
                    assert_eq!(rel(state.original_location), full["orig"], "{tag} original");
                    assert_eq!(rel(state.home_location), full["home"], "{tag} casa");
                    assert_eq!(state.path_type().serialized_name(), full["pt"].as_str().unwrap(), "{tag} pathType");
                    assert_eq!(
                        serde_json::Value::Array(state.path().iter().map(|p| rel(*p)).collect()),
                        full["path"],
                        "{tag} caminho"
                    );
                    assert_eq!(rel(state.get_next(pos)), full["next"], "{tag} próximo");
                    assert_eq!(rel(state.get_prev(pos)), full["prev"], "{tag} anterior");
                    assert_eq!(state.get_side(pos).name(), full["side"].as_str().unwrap(), "{tag} lado");
                    drop(state);
                    if fresh && let Some(stream) = want.get("stream") {
                        let iid = want["iid"].as_i64().unwrap() as i32;
                        stack.borrow_mut().update_for_pos(pos);
                        let bytes = stack.borrow().write_stream(&|_, out| {
                            write_var_int(out, iid);
                            out.extend([0, 0]);
                        });
                        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                        assert_eq!(hex, stream.as_str().unwrap(), "{tag} STREAM_CODEC");
                    }
                }
                let sync: Vec<i64> = base.pending_sync().into_iter().map(i64::from).collect();
                let want_sync: Vec<i64> = expected["sync"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap())
                    .collect();
                assert_eq!(sync, want_sync, "{me} needsSync");
                if fresh {
                    for (key, text) in [("nbt", base.write_items()), ("upd", base.write_reduced_updated_items())] {
                        if let Some(want) = expected.get(key) {
                            checks.push(format!("{me} {key}"));
                            assert_eq!(
                                text.as_ref().map(to_snbt),
                                want.as_str().map(str::to_owned),
                                "{me} {key}"
                            );
                        }
                    }
                }
            });
        }
        // Contêineres.
        for expected in &self.expected_chests {
            let container = self.container(&expected["p"]);
            let actual: Vec<Value> = (0..container.size())
                .filter_map(|slot| {
                    container
                        .resource(slot)
                        .map(|item| serde_json::json!([slot, item.id.to_string(), container.amount(slot)]))
                })
                .collect();
            assert_eq!(
                Value::Array(actual),
                expected["slots"],
                "{who} contêiner {}",
                expected["p"]
            );
        }
        // O que caiu no mundo neste tick.
        let mut actual: Vec<String> = self
            .drops
            .borrow_mut()
            .drain(..)
            .map(|drop| format!("{} {} {}", drop.item.id, drop.amount, self.rel_block(drop.pos)))
            .collect();
        let mut expected: Vec<String> = row
            .get("drops")
            .map(|drops| {
                drops
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| format!("{} {} {}", d["item"].as_str().unwrap(), d["n"], d["p"]))
                    .collect()
            })
            .unwrap_or_default();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{who} itens soltos");
    }
}

/// Os `System.identityHashCode` das pilhas na ordem em que aparecem pela primeira vez no rastro.
fn hashes_in_order(rows: &[Value], order: &[usize], relative: &[[i32; 3]]) -> Vec<i32> {
    let mut seen = HashSet::new();
    let mut hashes = Vec::new();
    for row in rows {
        let Some(entries) = row.get("tx").and_then(Value::as_array) else {
            continue;
        };
        let mut entries: Vec<&Value> = entries.iter().collect();
        entries.sort_by_key(|entry| {
            let rel = int3(&entry["p"]);
            let index = relative.iter().position(|r| *r == rel).unwrap();
            order.iter().position(|o| *o == index).unwrap()
        });
        for entry in entries {
            let Some(transit) = entry.get("transit").and_then(Value::as_array) else {
                continue;
            };
            let mut stacks: Vec<&Value> = transit.iter().collect();
            stacks.sort_by_key(|stack| stack["id"].as_i64().unwrap());
            for stack in stacks {
                let hash = stack["h"].as_i64().unwrap() as i32;
                if seen.insert(hash) {
                    hashes.push(hash);
                }
            }
        }
    }
    hashes
}

fn run(name: &'static str) {
    let rows = read(name);
    assert_eq!(rows[0]["e"], "setup");
    let mut scenario = Scenario::build(name, &rows[0]);
    feed_identity_hashes(hashes_in_order(&rows, &scenario.order, &scenario.relative));
    assert_eq!(rows[1]["e"], "begin");
    // O estado de antes do primeiro tick: os contadores de cada transportador (a estrutura já andou os ticks de preparação).
    for entry in rows[1]["tx"].as_array().unwrap() {
        let index = scenario
            .relative
            .iter()
            .position(|rel| *rel == int3(&entry["p"]))
            .unwrap();
        let delay = rows[1]["dl"][index].as_i64().unwrap() as i32;
        let pos = scenario.pos_long(index);
        let (dc, nid) = (
            entry["dc"].as_i64().unwrap() as i32,
            entry["nid"].as_i64().unwrap() as i32,
        );
        scenario
            .shared
            .with_transporter(pos, &mut |base, _| base.restore_counters(delay, dc, nid));
        assert!(entry["transit"].as_array().unwrap().is_empty(), "{name}: começa com pilhas");
    }
    if let Some(chests) = rows[1].get("chests") {
        scenario.expected_chests = chests.as_array().unwrap().clone();
    }
    let mut ticks = 0;
    for row in &rows[2..] {
        match row["e"].as_str().unwrap() {
            "mark" => scenario.apply_mark(row),
            // O `ServerTickEvent.Post` do próprio tick em que a gravação começou (`t` 0): o estado de `begin`, sem tick no meio.
            "state" if row["t"] == 0 => {}
            "state" => {
                scenario.run_tick();
                scenario.check_row(row);
                ticks += 1;
            }
            other => panic!("evento {other}"),
        }
    }
    assert!(ticks > 50, "{name}: poucos ticks ({ticks})");
    assert_eq!(
        pending_fed_identity_hashes(),
        0,
        "{name}: sobraram hashes dados que nenhuma pilha pediu"
    );
}

macro_rules! scenarios {
    ($($test:ident => $name:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                run($name);
            }
        )*
    };
}

scenarios! {
    pull_line => "pull_line",
    tiers => "tiers",
    full_midway => "full_midway",
    full_midway_idle => "full_midway_idle",
    capability => "capability",
    restrictive => "restrictive",
    lone => "lone",
    lone_idle => "lone_idle",
    broken => "broken",
    diversion => "diversion",
    contested_1 => "contested_1",
    contested_2 => "contested_2",
    contested_3 => "contested_3",
    contested_4 => "contested_4",
}
