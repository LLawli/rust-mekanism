//! O que o leitor de rede (`ItemNetworkReader` do Mekanism) lê de uma rede de transmissores: o que a rede sabe de si (quantos transmissores, quantos aceitadores, o que
//! `getNeededInfo`, `getStoredInfo`, `getFlowInfo` e `getNetworkReaderCapacity` leem) e, para um bloco que não é transmissor, por onde cada rede vizinha o alcança
//! (`getAcceptorDirections`). A lib só entrega os números; os textos, as chaves de tradução e as cores são do mod (a lib não conhece nenhum id nem texto do Mekanism).
use super::NetworkId;
use crate::sides::Face;

/// O que a rede de um transmissor sabe de si.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkReading<R> {
    /// `transmittersSize()`.
    pub transmitters: usize,
    /// `getAcceptorCount()`: duas conexões à mesma posição contam duas.
    pub acceptors: usize,
    /// O que cada família acrescenta: as quatro informações do `INetworkDataHandler`.
    pub detail: ReadingDetail<R>,
}

/// As informações de cada família (`INetworkDataHandler`). Uma que o original deixa nula (`null`) não existe aqui.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadingDetail<R> {
    /// `EnergyNetwork`: `energyContainer.getNeededAsLong()`, `getAmountAsLong()`, `prevTransferAmount` e `getCapacity()`.
    Energy {
        needed: i64,
        stored: i64,
        flow: i64,
        capacity: i64,
    },
    /// `FluidNetwork` e `ChemicalNetwork` (`DynamicBufferedResourceNetwork`): o que falta para encher, o recurso guardado e a quantidade (`None` vazio),
    /// `prevTransferAmount` e `getCapacity()`.
    Resource {
        needed: i64,
        stored: Option<(R, i64)>,
        flow: i64,
        capacity: i64,
    },
    /// `HeatNetwork`: `meanTemp`, `heatTransferred` e `heatLost`, sem capacidade.
    Heat {
        mean_temp: f64,
        transferred: f64,
        lost: f64,
    },
    /// `InventoryNetwork`: nenhuma das quatro informações (o `INetworkDataHandler` devolve `null`).
    Inventory,
}

/// A rede de um transmissor, vista por quem a lê.
#[derive(Clone, Debug, PartialEq)]
pub enum TransmitterReading<R> {
    /// Não há transmissor nessa posição.
    NoTransmitter,
    /// O transmissor existe e não tem rede (`hasTransmitterNetwork()` falso: ainda sem rede, ou órfão).
    NoNetwork,
    /// A rede dele.
    Network(NetworkReading<R>),
}

impl<R> TransmitterReading<R> {
    /// O mesmo, com o recurso guardado trocado por `f` (o id do recurso no lugar do tipo do dono).
    pub fn map<S>(self, f: impl FnOnce(R) -> S) -> TransmitterReading<S> {
        match self {
            TransmitterReading::NoTransmitter => TransmitterReading::NoTransmitter,
            TransmitterReading::NoNetwork => TransmitterReading::NoNetwork,
            TransmitterReading::Network(network) => TransmitterReading::Network(NetworkReading {
                transmitters: network.transmitters,
                acceptors: network.acceptors,
                detail: match network.detail {
                    ReadingDetail::Energy {
                        needed,
                        stored,
                        flow,
                        capacity,
                    } => ReadingDetail::Energy {
                        needed,
                        stored,
                        flow,
                        capacity,
                    },
                    ReadingDetail::Resource {
                        needed,
                        stored,
                        flow,
                        capacity,
                    } => ReadingDetail::Resource {
                        needed,
                        stored: stored.map(|(resource, amount)| (f(resource), amount)),
                        flow,
                        capacity,
                    },
                    ReadingDetail::Heat {
                        mean_temp,
                        transferred,
                        lost,
                    } => ReadingDetail::Heat {
                        mean_temp,
                        transferred,
                        lost,
                    },
                    ReadingDetail::Inventory => ReadingDetail::Inventory,
                },
            }),
        }
    }
}

/// Por onde uma rede alcança um aceitador: `displayConnectedNetworks` mostra os lados de cada rede vizinha, uma vez por rede.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptorSides {
    /// Quem identifica a rede entre as outras da mesma família (o conjunto `iteratedNetworks`).
    pub network: NetworkId,
    /// `getAcceptorDirections(pos)`: os lados do aceitador por onde a rede entra, na ordem do `EnumMap` (a do `Direction`).
    pub sides: Vec<Face>,
}

/// A mesma leitura, para os quatro donos de transmissores.
pub trait NetworkReader {
    /// O recurso que o dono guarda (`()` onde não há).
    type Resource: Clone;

    /// `ItemNetworkReader.displayTransmitterInfo`: o que a rede do transmissor em `pos` sabe de si.
    fn read_network(&self, pos: crate::sim::chunks::BlockPos)
    -> TransmitterReading<Self::Resource>;

    /// `ItemNetworkReader.displayConnectedNetworks`, para um vizinho: se há um transmissor em `transmitter` com rede (órfã ou não: usa `getTransmitterNetwork()`) que tem o
    /// aceitador `acceptor`, a rede e os lados.
    fn acceptor_sides(
        &self,
        transmitter: crate::sim::chunks::BlockPos,
        acceptor: crate::sim::chunks::BlockPos,
    ) -> Option<AcceptorSides>;
}

/// `Math.max(0, capacity - amount)`: o que `getNeededAsLong` devolve quando o conteúdo passa da capacidade.
pub fn needed(capacity: i64, stored: i64) -> i64 {
    (capacity - stored).max(0)
}

#[cfg(test)]
mod tests {
    use super::needed;

    #[test]
    fn needed_never_goes_negative() {
        assert_eq!(needed(6000, 16000), 0);
        assert_eq!(needed(6000, 5999), 1);
        assert_eq!(needed(6000, 0), 6000);
    }
}
