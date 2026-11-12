//! `PressurizedTube` (`content/network/transmitter/PressurizedTube.java`) e `TubeTier` (`tier/TubeTier.java`): a capacidade e a vazão de
//! cada tier do tubo de químico, com os valores de `tiers.toml` do original (os padrões, em mB: `TubeTier.getBaseCapacity` e
//! `getBaseTransferRate`, que `TierConfig.addPressurizedTubesCategory` usa como valor padrão da configuração).
//!
//! O resto do tubo é a base genérica de transmissor com buffer de recurso ([`super::buffered_resource_transmitter`]) e da família
//! ([`super::buffered_resource_network::ResourceFamily`]): o tubo só dá o tier, o tipo de aceitador (a capability de químico) e o validador
//! de órfãos (`CompatibleChemicalTransmitterValidator`, o mesmo [`super::buffered_resource_network::CompatibleResourceValidator`] do
//! fluido). `getRadiationScale` é [`super::buffered_resource_network::ResourceFamily::radiation_scale`].
use super::buffered_resource_transmitter::ResourcePipe;
use crate::chemical::ChemicalResource;

/// `FluidType.BUCKET_VOLUME`, a unidade em que `TubeTier` escreve os valores.
pub const BUCKET_VOLUME: i32 = 1000;

/// `TubeTier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TubeTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

impl TubeTier {
    pub const ALL: [TubeTier; 4] = [
        TubeTier::Basic,
        TubeTier::Advanced,
        TubeTier::Elite,
        TubeTier::Ultimate,
    ];

    /// `ordinal()`.
    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `getSerializedName()`: o nome do tier em minúsculas.
    pub fn serialized_name(self) -> &'static str {
        match self {
            TubeTier::Basic => "basic",
            TubeTier::Advanced => "advanced",
            TubeTier::Elite => "elite",
            TubeTier::Ultimate => "ultimate",
        }
    }

    /// `getCapacity()` (`getBaseCapacity()` sem a configuração): quantos mB cabem no tubo.
    pub fn capacity(self) -> i64 {
        match self {
            TubeTier::Basic => 4 * i64::from(BUCKET_VOLUME),
            TubeTier::Advanced => 16 * i64::from(BUCKET_VOLUME),
            TubeTier::Elite => 256 * i64::from(BUCKET_VOLUME),
            TubeTier::Ultimate => 1_024 * i64::from(BUCKET_VOLUME),
        }
    }

    /// `getTransferRate()` (`getBaseTransferRate()` sem a configuração): quantos mB o tubo puxa por tick.
    pub fn transfer_rate(self) -> i32 {
        match self {
            TubeTier::Basic => 750,
            TubeTier::Advanced => 2 * BUCKET_VOLUME,
            TubeTier::Elite => 64 * BUCKET_VOLUME,
            TubeTier::Ultimate => 256 * BUCKET_VOLUME,
        }
    }

    /// O estado de buffer de um tubo deste tier (`new PressurizedTube(blockProvider, tile)`).
    pub fn new_tube(self) -> ResourcePipe<ChemicalResource> {
        ResourcePipe::new(self.capacity(), self.transfer_rate())
    }
}
