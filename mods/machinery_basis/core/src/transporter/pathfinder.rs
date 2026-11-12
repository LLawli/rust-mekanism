//! `TransporterPathfinder`: o destino de uma pilha e o caminho até ele. O A* tem o custo dos transportadores (`getCost`), a busca de
//! destinos ordena por custo e depois por tamanho, o round robin guarda o próximo destino no ejetor, e o passeio ocioso (`IdlePath`)
//! anda em linha reta até onde há transportador.
use super::cache::PathfinderCache;
use super::manager::TransporterManager;
use super::pos::{NO_POS, block_pos_of, distance_between, relative_pos, side_difference};
use super::stack::{Path, SidedBlockPos, TransporterStack};
use super::world::{InventoryNetworkView, Transporter, TransporterWorld, can_insert};
use crate::containers::ItemResource;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::sim::open_hash::OpenHashSet;
use crate::transaction::Transaction;
use crate::transit::{TransitRequest, TransitResponse};
use crate::transmission::inventory_network::{AcceptorData, calculate_acceptors};
use crate::transmission::{DimensionId, LongKey};
use std::collections::HashMap;

/// O que a busca consulta além da rede: a dimensão (a chave de `GlobalPos` do gerenciador), o gerenciador, a cache e o mundo.
pub struct PathContext<'a> {
    pub dimension: DimensionId,
    pub manager: &'a TransporterManager,
    pub cache: &'a PathfinderCache,
    pub world: &'a dyn TransporterWorld,
}

/// A parte de `IAdvancedTransportEjector` que o round robin usa.
pub trait RoundRobinEjector {
    /// `getRoundRobinTarget`.
    fn round_robin_target(&self) -> Option<SidedBlockPos>;
    /// `setRoundRobinTarget(target)`.
    fn set_round_robin_target(&mut self, target: Option<SidedBlockPos>);
}

/// `Destination`.
#[derive(Clone)]
pub struct Destination {
    response: Option<TransitResponse>,
    path: Vec<i64>,
    score: f64,
    pub path_type: Path,
}

impl Destination {
    pub fn new(path: Vec<i64>, response: Option<TransitResponse>, score: f64) -> Self {
        Self {
            response,
            path,
            score,
            path_type: Path::None,
        }
    }

    /// `getPath`: do destino ao transportador de partida.
    pub fn path(&self) -> &[i64] {
        &self.path
    }

    pub fn score(&self) -> f64 {
        self.score
    }

    pub fn response(&self) -> Option<&TransitResponse> {
        self.response.as_ref()
    }

    /// `getResponseOrEmpty`.
    pub fn response_or_empty(&self) -> TransitResponse {
        self.response.clone().unwrap_or(TransitResponse::EMPTY)
    }

    /// `compareTo`: o custo, e num empate o caminho mais curto.
    fn compare(&self, other: &Destination) -> std::cmp::Ordering {
        if self.score < other.score {
            std::cmp::Ordering::Less
        } else if self.score > other.score {
            std::cmp::Ordering::Greater
        } else {
            self.path.len().cmp(&other.path.len())
        }
    }

    /// `SidedBlockPos.get(destination)`: o destino e o lado dele por onde o último transportador o toca.
    pub fn sided_pos(&self) -> SidedBlockPos {
        let pos = self.path[0];
        let side = side_difference(self.path[1], pos)
            .unwrap_or_else(|| panic!("Unable to calculate side of destination {pos}"));
        SidedBlockPos { pos, side }
    }
}

/// `IdlePathData`.
#[derive(Clone, Debug)]
pub struct IdlePathData {
    pub path: Vec<i64>,
    pub path_type: Path,
}

/// `getPaths(start, stack, request, min, transaction)`: os destinos que aceitam algo do pedido, do mais barato ao mais caro.
pub fn get_paths(
    ctx: &PathContext,
    network: Option<&dyn InventoryNetworkView>,
    start: BlockPos,
    stack: &TransporterStack,
    request: &TransitRequest,
    min: u32,
    transaction: Option<&Transaction>,
) -> Vec<Destination> {
    let Some(network) = network else {
        return Vec::new();
    };
    let acceptors = calculate_acceptors(ctx, network, request, stack, start, transaction);
    let mut paths = Vec::new();
    for data in &acceptors {
        if let Some(path) = get_path(ctx, network, data, start, stack, min, transaction) {
            paths.push(path);
        }
    }
    paths.sort_by(Destination::compare);
    paths
}

/// `checkPath(network, path, stack)`: todos os transportadores do caminho (menos o destino) ainda existem e deixam a pilha passar.
pub fn check_path(network: &dyn InventoryNetworkView, path: &[i64], stack: &TransporterStack) -> bool {
    for index in (1..path.len()).rev() {
        let Some(transmitter) = network.transmitter(path[index]) else {
            return false;
        };
        let color = transmitter.color();
        if color.is_some() && color != stack.color {
            return false;
        }
    }
    true
}

fn get_path(
    ctx: &PathContext,
    network: &dyn InventoryNetworkView,
    data: &AcceptorData,
    start: BlockPos,
    stack: &TransporterStack,
    min: u32,
    transaction: Option<&Transaction>,
) -> Option<Destination> {
    let response = &data.response;
    if response.sending_amount() < min {
        return None;
    }
    let dest = data.location;
    let uuid = network.uuid();
    if let Some(test) = ctx.cache.get_cache(uuid, start, dest, &data.sides)
        && check_path(network, &test.path, stack)
    {
        return Some(Destination::new(
            test.path,
            Some(response.clone()),
            test.cost,
        ));
    }
    let item = response.item_type().expect("resposta sem item").clone();
    let mut pathfinder = Pathfinder::new(
        ctx,
        network,
        dest,
        start,
        stack,
        item,
        response.sending_amount(),
        false,
    );
    pathfinder.find(transaction);
    if pathfinder.has_path() {
        let cached = ctx.cache.add_cached_path(
            uuid,
            start,
            dest,
            pathfinder.side,
            pathfinder.get_path(),
            pathfinder.final_score,
        );
        return Some(Destination::new(
            cached.path,
            Some(response.clone()),
            cached.cost,
        ));
    }
    None
}

/// `getNewBasePath`: o destino mais barato.
pub fn get_new_base_path(
    ctx: &PathContext,
    network: Option<&dyn InventoryNetworkView>,
    start: BlockPos,
    stack: &TransporterStack,
    request: &TransitRequest,
    min: u32,
    transaction: Option<&Transaction>,
) -> Option<Destination> {
    get_paths(ctx, network, start, stack, request, min, transaction)
        .into_iter()
        .next()
}

/// `getNewRRPath`: o destino que o ejetor guardou como o próximo, ou o primeiro; e guarda o seguinte.
#[allow(clippy::too_many_arguments)]
pub fn get_new_rr_path(
    ctx: &PathContext,
    network: Option<&dyn InventoryNetworkView>,
    start: BlockPos,
    stack: &TransporterStack,
    request: &TransitRequest,
    outputter: &mut dyn RoundRobinEjector,
    min: u32,
    transaction: Option<&Transaction>,
) -> Option<Destination> {
    let destinations = get_paths(ctx, network, start, stack, request, min, transaction);
    let count = destinations.len();
    if count == 0 {
        return None;
    }
    if count > 1
        && let Some(rr_target) = outputter.round_robin_target()
    {
        // Com mais de um destino e um "próximo" guardado, procura entre eles o que bate.
        for (index, destination) in destinations.iter().enumerate() {
            let path = destination.path();
            let pos = path[0];
            if rr_target.pos == pos {
                let side_of_dest = side_difference(path[1], pos);
                if Some(rr_target.side) == side_of_dest {
                    // Devolve o que bateu, e não o seguinte: se ele encheu, na próxima não acharia o alvo e o ciclo recomeçaria.
                    let next = if index == count - 1 {
                        &destinations[0]
                    } else {
                        &destinations[index + 1]
                    };
                    outputter.set_round_robin_target(Some(next.sided_pos()));
                    return Some(destination.clone());
                }
            }
        }
        // Sem o alvo em lugar nenhum (o destino foi quebrado antes de receber), recomeça pelo primeiro.
    }
    let destination = destinations[0].clone();
    if count > 1 {
        outputter.set_round_robin_target(Some(destinations[1].sided_pos()));
    } else {
        outputter.set_round_robin_target(Some(destination.sided_pos()));
    }
    Some(destination)
}

/// `getIdlePath(start, stack, transaction)`: de volta para casa se a pilha tem uma e dá para chegar, senão o passeio ocioso. Zera a casa
/// que não alcança e mexe no `idleDir` da pilha.
pub fn get_idle_path(
    ctx: &PathContext,
    network: Option<&dyn InventoryNetworkView>,
    start: BlockPos,
    stack: &mut TransporterStack,
    transaction: Option<&Transaction>,
) -> Option<IdlePathData> {
    let network = network?;
    if stack.home_location != NO_POS {
        let item = stack.item_type().expect("pilha sem item").clone();
        let mut pathfinder = Pathfinder::new(
            ctx,
            network,
            block_pos_of(stack.home_location),
            start,
            stack,
            item,
            stack.size(),
            true,
        );
        pathfinder.find(transaction);
        if pathfinder.has_path() {
            return Some(IdlePathData {
                path: pathfinder.get_path(),
                path_type: Path::Home,
            });
        }
        stack.home_location = NO_POS;
    }
    let idle = IdlePath {
        ctx,
        network,
        start,
    };
    let dest = idle.find(stack, transaction)?;
    Some(IdlePathData {
        path: dest.path,
        path_type: dest.path_type,
    })
}

/// `IdlePath`.
struct IdlePath<'a> {
    ctx: &'a PathContext<'a>,
    network: &'a dyn InventoryNetworkView,
    start: BlockPos,
}

impl IdlePath<'_> {
    fn find(
        &self,
        stack: &mut TransporterStack,
        transaction: Option<&Transaction>,
    ) -> Option<Destination> {
        let mut ret = vec![self.start.as_long()];
        let start_transmitter = self.network.transmitter(self.start.as_long());
        let Some(idle_dir) = stack.idle_dir else {
            return self.get_destination(&mut ret, start_transmitter, stack);
        };
        let transmitter = self
            .network
            .transmitter(self.start.relative(idle_dir).as_long());
        if stack.can_insert_to_transporter(transmitter, idle_dir, start_transmitter) {
            self.loop_side(&mut ret, idle_dir, start_transmitter, stack);
            return Some(Self::create_destination(ret));
        }
        let item = stack.item_type().expect("pilha sem item").clone();
        let request = TransitRequest::simple(&item, stack.size());
        if start_transmitter.is_some() {
            let new_path = get_new_base_path(
                self.ctx,
                Some(self.network),
                self.start,
                stack,
                &request,
                0,
                transaction,
            );
            if let Some(mut new_path) = new_path
                && new_path.response().is_some()
            {
                stack.idle_dir = None;
                new_path.path_type = Path::Dest;
                return Some(new_path);
            }
        }
        self.get_destination(&mut ret, start_transmitter, stack)
    }

    fn get_destination(
        &self,
        ret: &mut Vec<i64>,
        start_transmitter: Option<&dyn Transporter>,
        stack: &mut TransporterStack,
    ) -> Option<Destination> {
        let Some(new_side) = self.find_side(start_transmitter, stack) else {
            if let Some(start_transmitter) = start_transmitter {
                // Com `idleDir` ele é o lado "mais perto"; sem, o lado em que a pilha está no caminho atual.
                let side_closest = stack
                    .idle_dir
                    .unwrap_or_else(|| stack.get_side(self.start.as_long()));
                // Todos menos o mais perto: se só há uma ligação, falha.
                for side in Face::ALL {
                    if side == side_closest {
                        continue;
                    }
                    if start_transmitter.connection_type(side)
                        != crate::transmission::connection::ConnectionType::None
                    {
                        // Anda para o lado ligado; um caminho que não acaba num transportador é tratado sem erro.
                        stack.idle_dir = Some(side);
                        ret.push(relative_pos(self.start.as_long(), side));
                        return Some(Self::create_destination(std::mem::take(ret)));
                    }
                }
            }
            return None;
        };
        stack.idle_dir = Some(new_side);
        self.loop_side(ret, new_side, start_transmitter, stack);
        Some(Self::create_destination(std::mem::take(ret)))
    }

    fn create_destination(mut ret: Vec<i64>) -> Destination {
        ret.reverse();
        Destination::new(ret, None, 0.0)
    }

    fn loop_side(
        &self,
        list: &mut Vec<i64>,
        side: Face,
        start_transmitter: Option<&dyn Transporter>,
        stack: &TransporterStack,
    ) {
        let mut last = start_transmitter;
        let mut pos = self.start.relative(side);
        let mut transmitter = self.network.transmitter(pos.as_long());
        while stack.can_insert_to_transporter(transmitter, side, last) {
            last = transmitter;
            list.push(pos.as_long());
            pos = pos.relative(side);
            transmitter = self.network.transmitter(pos.as_long());
        }
    }

    fn find_side(
        &self,
        start_transmitter: Option<&dyn Transporter>,
        stack: &TransporterStack,
    ) -> Option<Face> {
        let can_insert = |from: Face| {
            stack.can_insert_to_transporter(
                self.network
                    .transmitter(self.start.relative(from).as_long()),
                from,
                start_transmitter,
            )
        };
        match stack.idle_dir {
            None => Face::ALL.into_iter().find(|side| can_insert(*side)),
            Some(idle_dir) => {
                let opposite = idle_dir.opposite();
                Face::ALL
                    .into_iter()
                    .filter(|side| *side != opposite)
                    .find(|side| can_insert(*side))
                    .or_else(|| can_insert(opposite).then_some(opposite))
            }
        }
    }
}

/// `Pathfinder`: o A* de um transportador até um destino.
pub struct Pathfinder<'a> {
    ctx: &'a PathContext<'a>,
    network: &'a dyn InventoryNetworkView,
    stack: &'a TransporterStack,
    start: BlockPos,
    final_node: BlockPos,
    item: ItemResource,
    amount: u32,
    /// `TransporterUtils.canInsert(.., force, ..)`: falso para um destino, verdadeiro para a casa da pilha.
    force: bool,
    open_set: OpenHashSet<LongKey>,
    closed_set: OpenHashSet<LongKey>,
    nav_map: HashMap<i64, i64>,
    g_score: HashMap<i64, f64>,
    f_score: HashMap<i64, f64>,
    pub final_score: f64,
    pub side: Option<Face>,
    results: Vec<i64>,
}

impl<'a> Pathfinder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &'a PathContext<'a>,
        network: &'a dyn InventoryNetworkView,
        final_node: BlockPos,
        start: BlockPos,
        stack: &'a TransporterStack,
        item: ItemResource,
        amount: u32,
        force: bool,
    ) -> Self {
        Self {
            ctx,
            network,
            stack,
            start,
            final_node,
            item,
            amount,
            force,
            open_set: OpenHashSet::new(),
            closed_set: OpenHashSet::new(),
            nav_map: HashMap::new(),
            g_score: HashMap::new(),
            f_score: HashMap::new(),
            final_score: 0.0,
            side: None,
            results: Vec::new(),
        }
    }

    /// `Long2DoubleOpenHashMap.get`: zero para a chave que não está.
    fn g(&self, node: i64) -> f64 {
        self.g_score.get(&node).copied().unwrap_or(0.0)
    }

    fn f(&self, node: i64) -> f64 {
        self.f_score.get(&node).copied().unwrap_or(0.0)
    }

    pub fn find(&mut self, transaction: Option<&Transaction>) -> bool {
        let start_long = self.start.as_long();
        self.open_set.insert(LongKey(start_long));
        self.g_score.insert(start_long, 0.0);
        // `gScore` mais a estimativa; como o `gScore` do começo é zero, só a estimativa.
        let total_distance = distance_between(self.start, self.final_node);
        self.f_score.insert(start_long, total_distance);
        let mut has_valid_direction = false;
        let start_transmitter = self.network.transmitter(start_long);
        for direction in Face::ALL {
            let neighbor = self.start.relative(direction);
            let neighbor_transmitter = self.network.transmitter(neighbor.as_long());
            if self
                .stack
                .can_insert_to_transporter(neighbor_transmitter, direction, start_transmitter)
            {
                // Dá para entrar num transportador: há um caminho a tentar.
                has_valid_direction = true;
                break;
            } else if self.is_valid_destination(
                self.start,
                start_transmitter,
                direction,
                neighbor,
                transaction,
            ) {
                // Já está ao lado do destino e pode mandar a ele (ou, voltando para casa, ligar nele): é o melhor caminho.
                return true;
            }
        }
        if !has_valid_direction {
            return false;
        }
        // Com os blocos muito perto, ainda deixa buscar até quatro blocos de distância.
        let max_search_distance = (2.0 * total_distance).max(4.0);
        while !self.open_set.is_empty() {
            let mut current_node_long = i64::MAX;
            let mut lowest_f_score = 0.0;
            for node in self.open_set.in_order() {
                if current_node_long == i64::MAX || self.f(node.0) < lowest_f_score {
                    current_node_long = node.0;
                    lowest_f_score = self.f(node.0);
                }
            }
            if current_node_long == i64::MAX {
                break;
            }
            let current_node = block_pos_of(current_node_long);
            self.open_set.remove(LongKey(current_node_long));
            self.closed_set.insert(LongKey(current_node_long));
            if distance_between(self.start, current_node) > max_search_distance {
                // Longe demais para considerar; segue para outro caminho, mesmo com a pontuação um pouco pior.
                continue;
            }
            let current_transmitter = self.network.transmitter(current_node_long);
            let current_score = self.g(current_node_long);
            for direction in Face::ALL {
                let neighbor = current_node.relative(direction);
                let neighbor_long = neighbor.as_long();
                let neighbor_transmitter = self.network.transmitter(neighbor_long);
                if self.stack.can_insert_to_transporter(
                    neighbor_transmitter,
                    direction,
                    current_transmitter,
                ) {
                    let tentative_g = current_score
                        + neighbor_transmitter.expect("o vizinho é um transportador").cost();
                    if self.closed_set.contains(LongKey(neighbor_long))
                        && tentative_g >= self.g(neighbor_long)
                    {
                        continue;
                    }
                    if !self.open_set.contains(LongKey(neighbor_long))
                        || tentative_g < self.g(neighbor_long)
                    {
                        self.nav_map.insert(neighbor_long, current_node_long);
                        self.g_score.insert(neighbor_long, tentative_g);
                        self.f_score.insert(
                            neighbor_long,
                            tentative_g + distance_between(neighbor, self.final_node),
                        );
                        self.open_set.insert(LongKey(neighbor_long));
                    }
                } else if self.is_valid_destination(
                    current_node,
                    current_transmitter,
                    direction,
                    neighbor,
                    transaction,
                ) {
                    // O vizinho é o destino e dá para mandar a ele.
                    return true;
                }
            }
        }
        false
    }

    /// `isValidDestination`: se o vizinho é o destino e a ligação deixa a pilha entrar, guarda o lado e o caminho.
    fn is_valid_destination(
        &mut self,
        start: BlockPos,
        start_transporter: Option<&dyn Transporter>,
        direction: Face,
        neighbor: BlockPos,
        transaction: Option<&Transaction>,
    ) -> bool {
        let Some(start_transporter) = start_transporter else {
            return false;
        };
        if neighbor != self.final_node {
            return false;
        }
        if !can_insert(
            self.ctx.world,
            neighbor.as_long(),
            self.stack.color,
            &self.item,
            self.amount,
            direction,
            self.force,
            transaction,
        ) {
            return false;
        }
        // Pode mandar (normal ou empurrar), ou é a casa da pilha (voltando por não ter chegado) e liga (normal, empurrar ou puxar).
        if start_transporter.can_emit_to(direction)
            || (self.final_node.as_long() == self.stack.home_location
                && start_transporter.can_connect(direction))
        {
            self.side = Some(direction);
            self.results = self.reconstruct_path(start.as_long());
            self.final_score = self.g(start.as_long()) + distance_between(start, self.final_node);
            return true;
        }
        false
    }

    fn reconstruct_path(&self, mut next_node: i64) -> Vec<i64> {
        let mut path = Vec::new();
        loop {
            path.push(next_node);
            next_node = self.nav_map.get(&next_node).copied().unwrap_or(i64::MAX);
            if next_node == i64::MAX {
                return path;
            }
        }
    }

    pub fn has_path(&self) -> bool {
        !self.results.is_empty()
    }

    /// `getPath`: o destino e depois o caminho de volta até o transportador de partida.
    pub fn get_path(&self) -> Vec<i64> {
        let mut path = Vec::with_capacity(self.results.len() + 1);
        path.push(self.final_node.as_long());
        path.extend_from_slice(&self.results);
        path
    }
}
