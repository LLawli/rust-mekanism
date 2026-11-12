//! `BufferedTransmitter` (`content/network/transmitter/BufferedTransmitter.java`): o que o transmissor com buffer acrescenta ao
//! `Transmitter` quando a família pode ter redes incompatíveis (`canHaveIncompatibleNetworks`: o fluido, o químico e o calor; o cabo de
//! energia não pode, e então tudo isto cai na base). São as funções que o dono dos transmissores chama no lugar dos pontos virtuais do
//! `Transmitter` da `rede-conexao` (`extra_valid_transmitter`, `Effect::RequestsUpdate`, `Effect::RecheckConnections`,
//! `Effect::RecheckConnection`).
use super::connection::connection_map_contains_side;
use super::registry::TransmitterNetworkRegistry;
use super::transmitter::{Effect, Transmitter, TransmitterWorld, relative};
use super::{NetworkId, TransmitterHost, TransmitterId};
use crate::sides::Face;

/// O que a regra de `isValidTransmitter` pergunta de cada ponta: se a família pode ter redes incompatíveis, se tem rede e se é órfão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ends {
    pub can_have_incompatible_networks: bool,
    pub has_network: bool,
    pub is_orphan: bool,
}

/// `BufferedTransmitter.isValidTransmitter`, a parte acrescentada à regra básica: com redes incompatíveis nos dois lados, uma rede não se
/// liga a um órfão (nem o contrário) até o órfão ter tido a chance de achar o lugar dele.
pub fn extra_valid_transmitter(me: Ends, other: Ends) -> bool {
    if me.can_have_incompatible_networks
        && other.can_have_incompatible_networks
        && (me.has_network && other.is_orphan || other.has_network && me.is_orphan)
    {
        return false;
    }
    true
}

/// `BufferedTransmitter.requestsUpdate()`: com redes incompatíveis, relê as conexões e avisa os vizinhos dos lados que mudaram, antes do
/// `super.requestsUpdate()` (o pacote de atualização, que fecha a lista).
pub fn requests_update<A: Clone, W: TransmitterWorld<Acceptor = A>>(
    transmitter: &mut Transmitter<A>,
    can_have_incompatible_networks: bool,
    world: &W,
    out: &mut Vec<Effect>,
) {
    if can_have_incompatible_networks {
        let possible_transmitters = transmitter.get_possible_transmitter_connections(world);
        let possible_acceptors = transmitter.get_possible_acceptor_connections(world, out);
        let all_possible = possible_transmitters | possible_acceptors;
        let all_current = transmitter.get_all_current_connections();
        // Atualiza as conexões: um lado pode ter ido de transmissor a aceitador.
        transmitter.current_transmitter_connections = possible_transmitters;
        transmitter.acceptor_cache.current_acceptor_connections = possible_acceptors;
        if all_possible != all_current {
            let changed = all_possible ^ all_current;
            for side in Face::ALL {
                if connection_map_contains_side(changed, side) {
                    let pos = relative(transmitter.pos(), side);
                    if world.transmitter_at(pos, side).is_some() {
                        out.push(Effect::RefreshNeighbor {
                            pos,
                            side: side.opposite(),
                        });
                    }
                }
            }
        }
    }
    Transmitter::<A>::base_requests_update(out);
}

/// `BufferedTransmitter.recheckConnections(newlyEnabledTransmitters)`: com rede e redes incompatíveis, cada lado ligado agora é relido
/// (`recheckConnection(side)` de cada um, sem parar no primeiro bom); sem rede, a base. Devolve os lados a reler.
pub fn recheck_connections_sides(
    has_network: bool,
    can_have_incompatible_networks: bool,
    newly_enabled_transmitters: u8,
) -> Option<Vec<Face>> {
    if !has_network {
        return None;
    }
    let mut sides = Vec::new();
    if can_have_incompatible_networks {
        for side in Face::ALL {
            if connection_map_contains_side(newly_enabled_transmitters, side) {
                sides.push(side);
            }
        }
    }
    Some(sides)
}

/// Quem é a rede principal na fusão manual: se o transmissor não tem buffer (`noBufferOrFallback`), a outra rede passa a ser a principal.
pub fn primary_and_secondary(
    network: NetworkId,
    other: NetworkId,
    no_buffer_or_fallback: bool,
) -> (NetworkId, NetworkId) {
    if no_buffer_or_fallback {
        (other, network)
    } else {
        (network, other)
    }
}

/// O núcleo de `recheckConnectionPrechecked(side)`, depois das checagens: as duas redes são diferentes e compatíveis. A rede principal
/// adota a outra, a outra sai do registro e a principal confirma (`commit`). Devolve os transmissores adotados: quem chama então apara o
/// buffer (`clampBuffer`), manda o vizinho reler o lado (`refreshConnections(side.getOpposite())`) e chama `requestsUpdate` de cada um
/// deles.
pub fn merge_networks_manually<A: Clone + 'static>(
    registry: &mut TransmitterNetworkRegistry<A>,
    host: &mut dyn TransmitterHost<A>,
    network: NetworkId,
    other: NetworkId,
) -> Vec<TransmitterId> {
    let others = registry.adopt_transmitters_and_acceptors_from(host, network, other);
    registry.deregister_network(host, other);
    registry.commit_network(host, network);
    others
}
