//! `MechanicalPipe` (`content/network/transmitter/MechanicalPipe.java`) e `PipeTier` (`tier/PipeTier.java`): a capacidade e a vazão de cada
//! tier do cano de fluido, com os valores de `tiers.toml` do original (os padrões, em mB: `PipeTier.getBaseCapacity` e
//! `getBaseTransferRate`).
//!
//! O resto do cano é a base genérica de transmissor com buffer de recurso ([`super::buffered_resource_transmitter`]) e da família
//! ([`super::buffered_resource_network::ResourceFamily`]): o cano só dá o tier, o tipo de aceitador (a capability de fluido) e o validador
//! de órfãos (`CompatibleFluidTransmitterValidator`).
use super::buffered_resource_transmitter::ResourcePipe;
use crate::fluid::FluidResource;

/// `FluidType.BUCKET_VOLUME`.
pub const BUCKET_VOLUME: i32 = 1000;

/// `PipeTier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PipeTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

impl PipeTier {
    pub const ALL: [PipeTier; 4] = [
        PipeTier::Basic,
        PipeTier::Advanced,
        PipeTier::Elite,
        PipeTier::Ultimate,
    ];

    /// `ordinal()`.
    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `getSerializedName()`: o nome do tier em minúsculas.
    pub fn serialized_name(self) -> &'static str {
        match self {
            PipeTier::Basic => "basic",
            PipeTier::Advanced => "advanced",
            PipeTier::Elite => "elite",
            PipeTier::Ultimate => "ultimate",
        }
    }

    /// `getCapacity()` (`getBaseCapacity()` sem a configuração): quantos mB cabem no cano.
    pub fn capacity(self) -> i64 {
        match self {
            PipeTier::Basic => 2 * i64::from(BUCKET_VOLUME),
            PipeTier::Advanced => 8 * i64::from(BUCKET_VOLUME),
            PipeTier::Elite => 32 * i64::from(BUCKET_VOLUME),
            PipeTier::Ultimate => 128 * i64::from(BUCKET_VOLUME),
        }
    }

    /// `getTransferRate()` (`getBaseTransferRate()` sem a configuração): quantos mB o cano puxa por tick.
    pub fn transfer_rate(self) -> i32 {
        match self {
            PipeTier::Basic => BUCKET_VOLUME / 4,
            PipeTier::Advanced => BUCKET_VOLUME,
            PipeTier::Elite => 8 * BUCKET_VOLUME,
            PipeTier::Ultimate => 32 * BUCKET_VOLUME,
        }
    }

    /// O estado de buffer de um cano deste tier (`new MechanicalPipe(blockProvider, tile)`).
    pub fn new_pipe(self) -> ResourcePipe<FluidResource> {
        ResourcePipe::new(self.capacity(), self.transfer_rate())
    }
}
