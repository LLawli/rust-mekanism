//! `CompatibleTransmitterValidator`: quem decide, na busca de um órfão, quais redes e quais outros órfãos podem se juntar a ele.
//!
//! A base aceita tudo. Os validadores de fluido e de químico (`CompatibleFluidTransmitterValidator`,
//! `CompatibleChemicalTransmitterValidator`) comparam o recurso guardado no buffer: são das famílias e implementam este trait.
use super::registry::TransmitterNetworkRegistry;
use super::{NetworkId, TransmitterHost, TransmitterId};
use std::any::Any;

/// `CompatibleTransmitterValidator<ACCEPTOR, NETWORK, TRANSMITTER>`. O validador de uma busca é mutável (`compareBuffers` guarda o
/// primeiro recurso que viu) e a rede guarda o último que lhe deram (`getTransmitterValidator`) até o `commit`.
pub trait CompatibleTransmitterValidator<A> {
    /// `isNetworkCompatible(net)`. O registro está à mão para ler `registry.network_validator(network)`, o validador que a rede guardou
    /// (o `instanceof` do original, por [`Self::as_any`]).
    fn is_network_compatible(
        &mut self,
        _host: &dyn TransmitterHost<A>,
        _registry: &TransmitterNetworkRegistry<A>,
        _network: NetworkId,
    ) -> bool {
        true
    }

    /// `isTransmitterCompatible(transmitter)`: o órfão a verificar.
    fn is_transmitter_compatible(
        &mut self,
        _host: &dyn TransmitterHost<A>,
        _transmitter: TransmitterId,
    ) -> bool {
        true
    }

    /// Para o `instanceof` e o cast que os validadores das famílias fazem no validador guardado na rede.
    fn as_any(&self) -> &dyn Any;
}

/// A base: tudo é compatível (`new CompatibleTransmitterValidator<>()`, o que `Transmitter.getNewOrphanValidator` devolve).
#[derive(Clone, Copy, Debug, Default)]
pub struct AlwaysCompatible;

impl<A> CompatibleTransmitterValidator<A> for AlwaysCompatible {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
