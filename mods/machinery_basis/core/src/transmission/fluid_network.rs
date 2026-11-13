//! `FluidNetwork` (`content/network/FluidNetwork.java`): a rede de fluido, a [`ResourceFamily`] genérica com o fluido como recurso.
//!
//! A topologia é do [`super::registry::TransmitterNetworkRegistry`] e a base de buffer de recurso é de
//! [`super::buffered_resource_network`] e [`super::buffered_resource_transmitter`]; aqui estão só os nomes do fluido. O `onUpdate` do Java (a escala, o `FluidTransferEvent` quando o tipo ou a escala mudou, e o
//! `tickEmit`) é o `ResourceFamily::on_update`. O dono dos canos o chama de `TransmitterHost::on_update`, repassa o `network_event`,
//! o `take_share` e o `validate_and_take_share`, e liga `TransmitterWorld::extra_valid_transmitter` a
//! [`ResourceFamily::is_valid_transmitter`] (a regra de `MechanicalPipe.isValidTransmitter`).
//!
//! `fluidScale` não é da rede: é o `currentScale` dela, que o renderizador do cano lê.
use super::buffered_resource_network::{
    CompatibleResourceValidator, ResourceFamily, ResourceNetwork, TransferEvent,
};
use crate::fluid::FluidResource;
use crate::sim::chunks::BlockPos;

/// `FluidNetwork`.
pub type FluidNetwork = ResourceNetwork<FluidResource>;
/// `FluidNetwork.FluidTransferEvent`.
pub type FluidTransferEvent = TransferEvent<FluidResource>;
/// `CompatibleTransmitterValidator.CompatibleFluidTransmitterValidator`.
pub type CompatibleFluidTransmitterValidator = CompatibleResourceValidator<FluidResource>;
/// As redes de fluido e os canos mecânicos de um dono de transmissores.
pub type FluidFamily<H> = ResourceFamily<FluidResource, H>;


/// O dono dos canos mecânicos de uma dimensão (`ResourceTransmitters` com o fluido como recurso).
pub type FluidTransmitters = super::resource_owner::ResourceTransmitters<FluidResource>;

/// A capability de fluido de um vizinho (`ResourceHandler<FluidResource>`).
pub type FluidHandler = super::resource_owner::Handler<FluidResource>;

/// Um dono de canos novo: a família de fluido (o `disperse` dela é vazio, não registra despejos), os UUID das redes e o `identityHashCode` de cada cano.
pub fn fluid_transmitters(
    dimension: super::DimensionId,
    uuids: impl FnMut() -> super::Uuid + 'static,
    hashes: impl FnMut(BlockPos) -> i32 + 'static,
) -> FluidTransmitters {
    FluidTransmitters::new(
        dimension,
        super::connection::TransmissionType::Fluid,
        FluidFamily::new,
        uuids,
        hashes,
    )
}
