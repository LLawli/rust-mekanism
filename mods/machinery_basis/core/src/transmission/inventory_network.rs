//! `InventoryNetwork`: a rede de transportadores logísticos, sobre a topologia da `DynamicNetwork` (`dynamic_network.rs`). O que ela
//! acrescenta é `calculateAcceptors` (os destinos que aceitam algo de um pedido, com o que já está a caminho deles), e a cache de
//! caminhos (`PathfinderCache`) que o `commit` e o `deregister` esvaziam.
use super::registry::TransmitterNetworkRegistry;
use super::{NetworkEvent, NetworkId, TransmitterId, Uuid};
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::transaction::Transaction;
use crate::transit::{TransitRequest, TransitResponse};
use crate::transporter::cache::PathfinderCache;
use crate::transporter::pathfinder::PathContext;
use crate::transporter::pos::block_pos_of;
use crate::transporter::stack::TransporterStack;
use crate::transporter::world::{AcceptorEntry, InventoryNetworkView, Transporter};
use std::rc::Rc;

/// `InventoryNetwork.AcceptorData`: um destino, o que ele aceita do pedido e os lados dele por onde aceita (os do destino voltados para a
/// rede, de onde a pilha o toca: o oposto do lado do aceitador).
#[derive(Clone)]
pub struct AcceptorData {
    pub location: BlockPos,
    pub response: TransitResponse,
    /// O `EnumSet<Direction>`, na ordem do `Direction`.
    pub sides: Vec<Face>,
}

fn face_index(side: Face) -> usize {
    Face::ALL
        .iter()
        .position(|face| *face == side)
        .expect("Face::ALL tem os seis lados")
}

/// `calculateAcceptors(request, stack, chunkMap, start, transaction)`. O `chunkMap` do original é só uma cache de chunks.
pub fn calculate_acceptors(
    ctx: &PathContext,
    network: &dyn InventoryNetworkView,
    request: &TransitRequest,
    stack: &TransporterStack,
    start: BlockPos,
    transaction: Option<&Transaction>,
) -> Vec<AcceptorData> {
    let mut to_return: Vec<AcceptorData> = Vec::new();
    let uuid = network.uuid();
    for (pos, sides) in network.acceptors() {
        if pos == stack.home_location {
            continue;
        }
        let block_pos = block_pos_of(pos);
        let config = ctx.world.side_configuration(pos);
        // Cada resposta distinta do bloco vira um `AcceptorData`; as iguais juntam os lados.
        let mut data_map: Vec<usize> = Vec::new();
        for (side, handler) in sides {
            if let Some(cached_path) = ctx.cache.get_single_cache(uuid, start, block_pos, side)
                && !crate::transporter::pathfinder::check_path(network, &cached_path.path, stack)
            {
                // Caminho inválido: nem simula.
                continue;
            }
            if let Some(config) = config
                && config.strict_input
            {
                // Se o bloco é configurável, a cor tem de bater, e sai logo se não bate.
                let config_color = config.input_color(side);
                if config_color.is_some() && config_color != stack.color {
                    continue;
                }
            }
            let response = ctx.manager.get_predicted_insert(
                (ctx.dimension, pos),
                side,
                &handler,
                request,
                transaction,
            );
            if !response.is_empty() {
                let opposite = side.opposite();
                // Se já há dados para uma resposta igual no destino, soma o lado; senão cria.
                match data_map
                    .iter()
                    .copied()
                    .find(|index| to_return[*index].response.same_as(&response))
                {
                    None => {
                        data_map.push(to_return.len());
                        to_return.push(AcceptorData {
                            location: block_pos,
                            response,
                            sides: vec![opposite],
                        });
                    }
                    Some(index) => {
                        let sides = &mut to_return[index].sides;
                        if !sides.contains(&opposite) {
                            sides.push(opposite);
                            sides.sort_by_key(|side| face_index(*side));
                        }
                    }
                }
            }
        }
    }
    to_return
}

/// O que a `InventoryNetwork` acrescenta a `commit()` e a `deregister()`: esvazia a cache de caminhos da rede. Quem implementa
/// `TransmitterHost::network_event` para a família de itens chama isto com cada evento.
pub fn on_network_event(cache: &PathfinderCache, uuid: Uuid, event: &NetworkEvent) {
    if matches!(event, NetworkEvent::Committed | NetworkEvent::Deregistered) {
        cache.on_changed(uuid);
    }
}

/// Os transportadores de uma rede por `TransmitterId`: quem tem os transmissores (o jogo, ou o mundo de um teste) os dá aqui.
pub trait TransporterLookup {
    fn transporter(&self, id: TransmitterId) -> Option<&dyn Transporter>;
}

/// A [`InventoryNetworkView`] de uma rede do registro: os transportadores pela posição e os aceitadores da `NetworkAcceptorCache`.
pub struct RegistryNetworkView<'a, L: TransporterLookup> {
    pub registry: &'a TransmitterNetworkRegistry<Rc<dyn ItemHandler>>,
    pub network: NetworkId,
    pub transporters: &'a L,
}

impl<L: TransporterLookup> InventoryNetworkView for RegistryNetworkView<'_, L> {
    fn uuid(&self) -> Uuid {
        self.registry.network(self.network).uuid()
    }

    fn transmitter(&self, pos: i64) -> Option<&dyn Transporter> {
        let id = self
            .registry
            .network(self.network)
            .transmitter_at(block_pos_of(pos))?;
        self.transporters.transporter(id)
    }

    fn acceptors(&self) -> Vec<AcceptorEntry> {
        let cache = &self.registry.network(self.network).acceptor_cache;
        cache
            .positions_in_order()
            .into_iter()
            .map(|pos| {
                let block_pos = block_pos_of(pos);
                let sides = cache
                    .acceptor_directions(block_pos)
                    .into_iter()
                    .filter_map(|side| {
                        cache
                            .cached_acceptor(block_pos, side)
                            .map(|handler| (side, handler.clone()))
                    })
                    .collect();
                (pos, sides)
            })
            .collect()
    }
}

/// `toString()`: `[InventoryNetwork] N transmitters, M acceptors.`
pub fn describe(
    registry: &TransmitterNetworkRegistry<Rc<dyn ItemHandler>>,
    network: NetworkId,
) -> String {
    let data = registry.network(network);
    format!(
        "[InventoryNetwork] {} transmitters, {} acceptors.",
        data.transmitters_size(),
        data.acceptor_count()
    )
}
