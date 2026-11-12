//! `FluidNetwork` (`content/network/FluidNetwork.java`): a rede de fluido, a [`ResourceFamily`] genérica com o fluido como recurso.
//!
//! A topologia é do [`super::registry::TransmitterNetworkRegistry`] e a base de buffer de recurso é de
//! [`super::buffered_resource_network`] e [`super::buffered_resource_transmitter`]; aqui estão só os nomes do fluido e o cano que entra
//! na família com os valores do tier. O `onUpdate` do Java (a escala, o `FluidTransferEvent` quando o tipo ou a escala mudou, e o
//! `tickEmit`) é o `ResourceFamily::on_update`. O dono dos canos o chama de `TransmitterHost::on_update`, repassa o `network_event`,
//! o `take_share` e o `validate_and_take_share`, e liga `TransmitterWorld::extra_valid_transmitter` a
//! [`ResourceFamily::is_valid_transmitter`] (a regra de `MechanicalPipe.isValidTransmitter`).
//!
//! `fluidScale` não é da rede: é o `currentScale` dela, que o renderizador do cano lê.
use super::buffered_resource_network::{
    CompatibleResourceValidator, ResourceAcceptor, ResourceFamily, ResourceNetwork, TransferEvent,
};
use super::mechanical_pipe::PipeTier;
use super::TransmitterId;
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

impl<H: ResourceAcceptor<FluidResource> + ?Sized> FluidFamily<H> {
    /// Um cano novo: `new MechanicalPipe(blockProvider, tile)`, com a capacidade e a vazão do tier.
    pub fn add_mechanical_pipe(&mut self, transmitter: TransmitterId, pos: BlockPos, tier: PipeTier) {
        self.add_pipe(transmitter, pos, tier.capacity(), tier.transfer_rate());
    }
}
