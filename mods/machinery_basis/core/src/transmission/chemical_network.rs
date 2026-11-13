//! `ChemicalNetwork` (`content/network/ChemicalNetwork.java`): a rede de químico, a [`ResourceFamily`] genérica com o químico como recurso.
//!
//! A topologia é do [`super::registry::TransmitterNetworkRegistry`] e a base de buffer de recurso é de
//! [`super::buffered_resource_network`] e [`super::buffered_resource_transmitter`]; aqui estão só os nomes do químico e o tubo que entra
//! na família com os valores do tier. O `onUpdate` do Java (a escala, o `ChemicalTransferEvent` quando o tipo ou a escala mudou, e o
//! `tickEmit`) é o `ResourceFamily::on_update`. O dono dos tubos o chama de `TransmitterHost::on_update`, repassa o `network_event`,
//! o `take_share` e o `validate_and_take_share`, e liga `TransmitterWorld::extra_valid_transmitter` a
//! [`ResourceFamily::is_valid_transmitter`] (a regra de `PressurizedTube.isValidTransmitter`).
//!
//! # O que o químico tem e o fluido não
//!
//! - `disperse(trigger, resource, amount)`: a `ChemicalNetwork` o sobrescreve para despejar radiação
//!   (`IRadiationManager.INSTANCE.dumpRadiation(level, trigger.getBlockPos(), resource, amount)`), e o fluido o deixa vazio. A família de
//!   químico nasce com [`ChemicalFamily::chemical`], que liga o registro dos despejos; o dono dos tubos lê [`ResourceFamily::take_dispersals`]
//!   e entrega cada um ao gerenciador de radiação (`RadiationManager.dumpRadiation`: só despeja com a radiação ligada, químico não vazio,
//!   quantidade positiva e radioatividade positiva, e então `radiate(level, pos, radioactivity * amount)`). A lib não sabe quais químicos
//!   são radioativos nem a radiação: ela diz onde, o quê e quanto.
//! - `PressurizedTube.getRadiationScale`: [`ResourceFamily::radiation_scale`], com a radioatividade vinda de fora.
//! - `fluidScale` do cano é o `currentScale` da rede, como o do tubo.
//!
//! O que não é portado: o cliente, os textos (`getNeededInfo`, `getTextComponent`, `toString`) e o `containerType` (o tipo de contêiner do
//! original, que aqui é o próprio [`crate::resource_tank::ResourceTank`] genérico).
use super::TransmitterId;
use super::buffered_resource_network::{
    CompatibleResourceValidator, ResourceAcceptor, ResourceFamily, ResourceNetwork, TransferEvent,
};
use super::pressurized_tube::TubeTier;
use crate::chemical::ChemicalResource;
use crate::sim::chunks::BlockPos;

/// `ChemicalNetwork`.
pub type ChemicalNetwork = ResourceNetwork<ChemicalResource>;
/// `ChemicalNetwork.ChemicalTransferEvent`.
pub type ChemicalTransferEvent = TransferEvent<ChemicalResource>;
/// `CompatibleTransmitterValidator.CompatibleChemicalTransmitterValidator`.
pub type CompatibleChemicalTransmitterValidator = CompatibleResourceValidator<ChemicalResource>;
/// As redes de químico e os tubos pressurizados de um dono de transmissores.
pub type ChemicalFamily<H> = ResourceFamily<ChemicalResource, H>;

impl<H: ResourceAcceptor<ChemicalResource> + ?Sized> ChemicalFamily<H> {
    /// A família de químico: como a de qualquer recurso, com o registro dos despejos de `disperse` ligado.
    pub fn chemical() -> Self {
        Self::new().with_dispersal()
    }

    /// Um tubo novo: `new PressurizedTube(blockProvider, tile)`, com a capacidade e a vazão do tier.
    pub fn add_pressurized_tube(
        &mut self,
        transmitter: TransmitterId,
        pos: BlockPos,
        tier: TubeTier,
    ) {
        self.add_pipe(transmitter, pos, tier.capacity(), tier.transfer_rate());
    }
}

/// O dono dos tubos pressurizados de uma dimensão (`ResourceTransmitters` com o químico como recurso).
pub type ChemicalTransmitters = super::resource_owner::ResourceTransmitters<ChemicalResource>;

/// A capability de químico de um vizinho (`ResourceHandler<ChemicalResource>`).
pub type ChemicalHandler = super::resource_owner::Handler<ChemicalResource>;

/// Um dono de tubos novo: a família de químico (que registra os `disperse`, [`ChemicalFamily::chemical`]), os UUID das redes e o `identityHashCode` de cada tubo.
pub fn chemical_transmitters(
    dimension: super::DimensionId,
    uuids: impl FnMut() -> super::Uuid + 'static,
    hashes: impl FnMut(BlockPos) -> i32 + 'static,
) -> ChemicalTransmitters {
    ChemicalTransmitters::new(
        dimension,
        super::connection::TransmissionType::Chemical,
        ChemicalFamily::chemical,
        uuids,
        hashes,
    )
}
